//! One HTTP listener serving both route trees.
//!
//! The hosted API and the self-hosted sync contract used to be separate
//! binaries on separate ports. They now share this layer: requests under
//! [`SELF_HOSTED_ROUTE_PREFIX`] go to [`crate::sync_router`], everything else
//! goes to the hosted [`HostedServer`].

use std::{
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    sync::Arc,
};

use ham_core::api_contract::SELF_HOSTED_ROUTE_PREFIX;
use serde_json::json;

use crate::sync_storage::DurableCloudSyncServer;
use crate::{parse_query, split_target, ApiRequest, ApiResponse, HostedServer};

/// The hosted API and the self-hosted sync service behind one listener.
#[derive(Clone)]
pub struct MergedServer {
    hosted: HostedServer,
    sync: Arc<DurableCloudSyncServer>,
}

impl MergedServer {
    pub fn new(hosted: HostedServer, sync: Arc<DurableCloudSyncServer>) -> Self {
        Self { hosted, sync }
    }

    pub fn hosted(&self) -> &HostedServer {
        &self.hosted
    }

    pub fn sync(&self) -> &Arc<DurableCloudSyncServer> {
        &self.sync
    }

    /// Health covers both services, so the payload satisfies hosted clients
    /// (`ok`/`service`/`version`) and sync clients (which additionally read
    /// `mode`).
    fn health(&self) -> ApiResponse {
        let body = serde_json::to_vec(&json!({
            "ok": true,
            "service": "ke8ygw-ham-server",
            "version": env!("CARGO_PKG_VERSION"),
            "mode": self.sync.health().mode,
        }))
        .expect("health payload should serialize");
        ApiResponse {
            status: 200,
            headers: std::collections::HashMap::new(),
            body,
        }
    }

    pub fn handle(&self, runtime: &tokio::runtime::Runtime, request: ApiRequest) -> ApiResponse {
        if request.path == "/health" {
            return self.health();
        }
        if request.path.starts_with(SELF_HOSTED_ROUTE_PREFIX) {
            return crate::sync_router::route(&self.sync, runtime, &request);
        }
        runtime.block_on(self.hosted.handle(request))
    }
}

pub fn handle_stream(
    server: &MergedServer,
    runtime: &tokio::runtime::Runtime,
    stream: &mut TcpStream,
) -> std::io::Result<()> {
    let request = {
        let mut reader = BufReader::new(&mut *stream);
        read_http_request(&mut reader)?
    };
    write_response(stream, &server.handle(runtime, request))
}

pub fn write_response(stream: &mut TcpStream, response: &ApiResponse) -> std::io::Result<()> {
    let status_text = match response.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let extra_headers = response
        .headers
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect::<String>();
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: application/json; charset=utf-8\r\n{}Content-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        status_text,
        extra_headers,
        response.body.len()
    );
    stream.write_all(header.as_bytes())?;
    stream.write_all(&response.body)?;
    Ok(())
}

pub fn read_http_request(reader: &mut BufReader<&mut TcpStream>) -> std::io::Result<ApiRequest> {
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_owned();
    let target = parts.next().unwrap_or("/").to_owned();
    let (path, query) = split_target(&target);

    let mut content_length = 0usize;
    let mut headers = std::collections::HashMap::new();
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            let name = name.trim().to_ascii_lowercase();
            let value = value.trim().to_owned();
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.parse().unwrap_or(0);
            }
            headers.insert(name, value);
        }
    }

    let mut body = vec![0; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }

    Ok(ApiRequest {
        method,
        path: path.to_owned(),
        query: parse_query(query),
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::sync_storage::DurableCloudSyncPaths;
    use crate::{BootstrapAdminRequest, CloudServerConfig, LoginResponse, QsoWriteRequest};
    use ham_core::sync::{
        CloudAuth, CloudPullEventsResponse, CloudPushEventsRequest, CloudPushEventsResponse,
        CloudServiceMode, ReplicationStatus,
    };
    use serde::{de::DeserializeOwned, Serialize};
    use serde_json::{json, Map, Value};
    use std::net::TcpListener;
    use uuid::Uuid;

    fn durable_server(root_label: &str) -> (MergedServer, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "ke8ygw-ham-server-wire-{root_label}-{}",
            Uuid::new_v4()
        ));
        let hosted = HostedServer::with_surreal_paths(
            root.join("surrealdb"),
            root.join("official-events.jsonl"),
        )
        .expect("durable hosted server should open");
        let sync = DurableCloudSyncServer::open(
            CloudServerConfig {
                mode: CloudServiceMode::SelfHosted,
                public_url: "http://127.0.0.1:9750".to_owned(),
                pairing_code: "local-dev-pairing-code".to_owned(),
                sync_session_ttl_seconds: None,
            },
            DurableCloudSyncPaths {
                metadata_store_path: root.join("sync-surrealdb"),
                official_event_log_path: root.join("sync-official-events.jsonl"),
                report_dir: root.join("reports"),
            },
        )
        .expect("durable sync server should open");
        (MergedServer::new(hosted, Arc::new(sync)), root)
    }

    fn raw_json_http_request(
        method: &str,
        target: impl AsRef<str>,
        bearer_token: Option<&str>,
        body: impl Serialize,
    ) -> Vec<u8> {
        let body = serde_json::to_vec(&body).expect("test request should serialize");
        let auth = bearer_token
            .map(|token| format!("Authorization: Bearer {token}\r\n"))
            .unwrap_or_default();
        let mut request = format!(
            "{method} {} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nAccept: application/json\r\nX-Request-Id: hosted-wire-test\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n",
            target.as_ref(),
            body.len()
        )
        .into_bytes();
        request.extend_from_slice(&body);
        request
    }

    fn raw_http_request(
        method: &str,
        target: impl AsRef<str>,
        bearer_token: Option<&str>,
    ) -> Vec<u8> {
        let auth = bearer_token
            .map(|token| format!("Authorization: Bearer {token}\r\n"))
            .unwrap_or_default();
        format!(
            "{method} {} HTTP/1.1\r\nHost: 127.0.0.1\r\nAccept: application/json\r\nX-Request-Id: hosted-wire-test\r\n{auth}Content-Length: 0\r\nConnection: close\r\n\r\n",
            target.as_ref()
        )
        .into_bytes()
    }

    fn wire_round_trip(server: MergedServer, request: Vec<u8>) -> Vec<u8> {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
        let addr = listener
            .local_addr()
            .expect("test listener should have a local address");
        let handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("test runtime should start");
            let (mut stream, _) = listener.accept().expect("test request should connect");
            handle_stream(&server, &runtime, &mut stream).expect("test request should be handled");
        });

        let mut client = TcpStream::connect(addr).expect("test client should connect");
        client
            .write_all(&request)
            .expect("test client should write request");
        client
            .shutdown(std::net::Shutdown::Write)
            .expect("test client should close request body");

        let mut response = Vec::new();
        client
            .read_to_end(&mut response)
            .expect("test client should read response");
        handle.join().expect("test server thread should finish");
        response
    }

    fn response_status(response: &[u8]) -> u16 {
        let text = String::from_utf8(response.to_vec()).expect("HTTP response should be UTF-8");
        text.lines()
            .next()
            .expect("HTTP response should have a status line")
            .split_whitespace()
            .nth(1)
            .expect("HTTP status line should contain a status code")
            .parse()
            .expect("HTTP status should be numeric")
    }

    fn response_body(response: &[u8]) -> &[u8] {
        let marker = b"\r\n\r\n";
        let start = response
            .windows(marker.len())
            .position(|window| window == marker)
            .expect("HTTP response should contain a body separator")
            + marker.len();
        &response[start..]
    }

    fn wire_json<T: DeserializeOwned>(server: MergedServer, request: Vec<u8>) -> T {
        let response = wire_round_trip(server, request);
        assert_eq!(response_status(&response), 200);
        serde_json::from_slice(response_body(&response))
            .expect("HTTP response body should deserialize as JSON")
    }

    #[test]
    fn hosted_wire_sync_push_pull_uses_durable_storage_without_duplicates() {
        let (server, root) = durable_server("sync-round-trip");

        let status: Value = wire_json(
            server.clone(),
            raw_http_request("GET", "/api/v1/status", None),
        );
        assert_eq!(status["durable_server_storage"], true);

        let bootstrap: LoginResponse = wire_json(
            server.clone(),
            raw_json_http_request(
                "POST",
                "/api/v1/admin/bootstrap",
                None,
                BootstrapAdminRequest {
                    email: "wire-admin@example.test".to_owned(),
                    display_name: Some("Wire Admin".to_owned()),
                    device_name: Some("Wire hosted client".to_owned()),
                },
            ),
        );
        let token = bootstrap.session.token.clone();
        let logbook_id = bootstrap
            .logbooks
            .first()
            .expect("bootstrap should create a logbook")
            .logbook_id;

        let created: Value = wire_json(
            server.clone(),
            raw_json_http_request(
                "POST",
                "/api/v1/qsos",
                Some(&token),
                QsoWriteRequest {
                    logbook_id,
                    contacted_callsign: Some("K1WIRE".to_owned()),
                    station_callsign: Some("KE8YGW".to_owned()),
                    operator_callsign: Some("KE8YGW".to_owned()),
                    started_at: Some("2026-07-22T12:00:00Z".to_owned()),
                    mode: Some("SSB".to_owned()),
                    band: Some("20m".to_owned()),
                    frequency_hz: None,
                    notes: None,
                    fields: Map::new(),
                },
            ),
        );
        let created_hash = created["event"]["event_hash"]
            .as_str()
            .expect("QSO create should return an event hash")
            .to_owned();

        let pull: CloudPullEventsResponse = wire_json(
            server.clone(),
            raw_json_http_request(
                "POST",
                "/api/v1/sync/pull",
                Some(&token),
                json!({
                    "logbook_id": logbook_id,
                    "local_head_hash": null,
                }),
            ),
        );
        assert_eq!(pull.preview.status, ReplicationStatus::RemoteAhead);
        assert_eq!(pull.events.len(), 1);
        assert_eq!(pull.events[0].event_hash, created_hash);

        let duplicate_push: CloudPushEventsResponse = wire_json(
            server.clone(),
            raw_json_http_request(
                "POST",
                "/api/v1/sync/push",
                Some(&token),
                CloudPushEventsRequest {
                    auth: CloudAuth {
                        sync_token: "unused-by-hosted-bearer".to_owned(),
                    },
                    logbook_id,
                    events: pull.events.clone(),
                },
            ),
        );
        assert_eq!(duplicate_push.status, ReplicationStatus::Pulled);
        assert_eq!(duplicate_push.accepted_count, 0);
        assert_eq!(duplicate_push.ignored_duplicate_count, 1);
        assert_eq!(duplicate_push.rejected_count, 0);
        assert_eq!(duplicate_push.server_head_hash, Some(created_hash.clone()));

        let official_log = std::fs::read_to_string(root.join("official-events.jsonl"))
            .expect("durable official event log should be readable");
        assert_eq!(official_log.matches(&created_hash).count(), 1);

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn merged_listener_serves_both_route_trees() {
        let (server, root) = durable_server("merged-dispatch");

        // Health answers for hosted clients and carries the sync `mode` field.
        let health: Value = wire_json(server.clone(), raw_http_request("GET", "/health", None));
        assert_eq!(health["ok"], true);
        assert!(health.get("mode").is_some());

        // A hosted route still resolves on the shared listener.
        let status: Value = wire_json(
            server.clone(),
            raw_http_request("GET", "/api/v1/status", None),
        );
        assert_eq!(status["durable_server_storage"], true);

        // A self-hosted sync route resolves on the same listener, under its prefix.
        let response = wire_round_trip(
            server.clone(),
            raw_http_request("GET", "/api/v1/self-hosted/logbooks", None),
        );
        assert_eq!(response_status(&response), 401, "missing sync token");

        // The un-prefixed legacy sync paths are gone: `auth/pair` exists only
        // under the self-hosted prefix, and the hosted tree has no such route.
        let response = wire_round_trip(
            server.clone(),
            raw_json_http_request("POST", "/api/v1/auth/pair", None, json!({})),
        );
        assert_eq!(
            response_status(&response),
            404,
            "the legacy un-prefixed pairing route should no longer resolve"
        );

        let response = wire_round_trip(
            server,
            raw_json_http_request(
                "POST",
                "/api/v1/self-hosted/auth/pair",
                None,
                json!({
                    "pairing_code": "wrong-code",
                    "account_id": "acct",
                    "user_id": "user",
                    "device_id": Uuid::new_v4(),
                    "device_name": "Merged dispatch test",
                    "requested_logbooks": [],
                    "role_hints": [],
                }),
            ),
        );
        assert_eq!(
            response_status(&response),
            200,
            "the prefixed pairing route should resolve on the shared listener"
        );
        let pairing: Value = serde_json::from_slice(response_body(&response))
            .expect("pairing response should be JSON");
        assert_eq!(pairing["accepted"], false, "wrong pairing code is rejected");

        let _ = std::fs::remove_dir_all(root);
    }
}
