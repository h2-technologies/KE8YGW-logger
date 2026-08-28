use std::{
    env,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process,
};

use ham_server::{parse_query, split_target, ApiRequest, HostedServer, SurrealHostedConfig};

fn main() {
    let addr = env::var("HAM_SERVER_BIND").unwrap_or_else(|_| "127.0.0.1:9750".to_owned());
    let listener = match TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("failed to bind ham-server to {addr}: {error}");
            process::exit(1);
        }
    };
    let runtime = tokio::runtime::Runtime::new().expect("ham-server runtime should start");
    let metadata_config = SurrealHostedConfig::from_env();
    let metadata_label = metadata_config.label();
    let server = match HostedServer::with_surreal_config(metadata_config) {
        Ok(server) => server,
        Err(error) => {
            eprintln!(
                "failed to open ham-server SurrealDB metadata store at {metadata_label}: {error}"
            );
            process::exit(1);
        }
    };

    println!("ham-server listening on http://{addr}");
    println!("metadata store: {metadata_label}");

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(error) = handle_stream(&server, &runtime, &mut stream) {
                    eprintln!("failed to handle request: {error}");
                }
            }
            Err(error) => eprintln!("failed to accept request: {error}"),
        }
    }
}

fn handle_stream(
    server: &HostedServer,
    runtime: &tokio::runtime::Runtime,
    stream: &mut TcpStream,
) -> std::io::Result<()> {
    let request = {
        let mut reader = BufReader::new(&mut *stream);
        read_http_request(&mut reader)?
    };
    let response = runtime.block_on(server.handle(request));
    let status_text = match response.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
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

fn read_http_request(reader: &mut BufReader<&mut TcpStream>) -> std::io::Result<ApiRequest> {
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

    use ham_server::{BootstrapAdminRequest, LoginResponse, QsoWriteRequest};
    use ham_sync::{
        CloudAuth, CloudPullEventsResponse, CloudPushEventsRequest, CloudPushEventsResponse,
        ReplicationStatus,
    };
    use serde::{de::DeserializeOwned, Serialize};
    use serde_json::{json, Map, Value};
    use uuid::Uuid;

    fn durable_server(root_label: &str) -> (HostedServer, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "ke8ygw-ham-server-wire-{root_label}-{}",
            Uuid::new_v4()
        ));
        let server = HostedServer::with_surreal_paths(
            root.join("surrealdb"),
            root.join("official-events.jsonl"),
        )
        .expect("durable hosted server should open");
        (server, root)
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

    fn wire_round_trip(server: HostedServer, request: Vec<u8>) -> Vec<u8> {
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

    fn wire_json<T: DeserializeOwned>(server: HostedServer, request: Vec<u8>) -> T {
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
}
