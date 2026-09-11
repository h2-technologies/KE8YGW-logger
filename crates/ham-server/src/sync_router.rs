//! Self-hosted sync and report routes.
//!
//! These share a listener and an HTTP layer with the hosted API; the
//! `/api/v1/self-hosted` prefix keeps the two route trees unambiguous.

use std::sync::Arc;

use ham_core::api_contract::{ApiErrorBody, ApiErrorCode, SELF_HOSTED_ROUTE_PREFIX};
use ham_core::sync::{
    CloudAuth, CloudPreviewPullRequest, CloudPullEventsRequest, CloudPushEventsRequest,
    CloudSyncError, DiagnosticReportUploadRequest, PairDeviceRequest,
};
use serde::Serialize;
use uuid::Uuid;

use crate::sync_storage::DurableCloudSyncServer;
use crate::{ApiRequest, ApiResponse};

/// True when `path` belongs to the self-hosted sync contract.
pub fn owns_path(path: &str) -> bool {
    path == "/health" || path.starts_with(SELF_HOSTED_ROUTE_PREFIX)
}

/// Strips the self-hosted prefix, yielding the route-relative path.
fn relative(path: &str) -> &str {
    path.strip_prefix(SELF_HOSTED_ROUTE_PREFIX).unwrap_or(path)
}

pub fn route(
    server: &Arc<DurableCloudSyncServer>,
    runtime: &tokio::runtime::Runtime,
    request: &ApiRequest,
) -> ApiResponse {
    let request_id = request_id(request);
    let path = relative(&request.path);
    let token = request.query.get("token").cloned();

    match (request.method.as_str(), path) {
        ("GET", "/health") => json_response(&server.health()),
        ("POST", "/auth/pair") => {
            match serde_json::from_slice::<PairDeviceRequest>(&request.body) {
                Ok(pair) => json_response(&runtime.block_on(server.pair_device(pair))),
                Err(_) => json_error(
                    400,
                    "invalid pair request",
                    ApiErrorCode::InvalidJson,
                    request_id,
                ),
            }
        }
        ("GET", "/logbooks") => match auth_from_token(token.as_deref()) {
            Some(auth) => match runtime.block_on(server.list_logbooks(&auth)) {
                Ok(payload) => json_response(&payload),
                Err(error) => cloud_error(error, request_id),
            },
            None => json_error(401, "missing token", ApiErrorCode::MissingToken, request_id),
        },
        ("GET", path) if path.starts_with("/logbooks/") && path.ends_with("/head") => {
            with_logbook_auth(
                path,
                token.as_deref(),
                "/head",
                request_id.clone(),
                |auth, logbook_id| match runtime.block_on(server.get_head(&auth, logbook_id)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => cloud_error(error, request_id.clone()),
                },
            )
        }
        ("GET", path) if path.starts_with("/logbooks/") && path.ends_with("/events") => {
            with_logbook_auth(
                path,
                token.as_deref(),
                "/events",
                request_id.clone(),
                |auth, logbook_id| {
                    let after_hash = request.query.get("after_hash").cloned();
                    match runtime.block_on(server.event_metadata(&auth, logbook_id, after_hash)) {
                        Ok(payload) => json_response(&payload),
                        Err(error) => cloud_error(error, request_id.clone()),
                    }
                },
            )
        }
        ("POST", path) if path.starts_with("/logbooks/") && path.ends_with("/preview-pull") => {
            match serde_json::from_slice::<CloudPreviewPullRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.preview_pull(payload)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => cloud_error(error, request_id),
                },
                Err(_) => json_error(
                    400,
                    "invalid preview request",
                    ApiErrorCode::InvalidJson,
                    request_id,
                ),
            }
        }
        ("POST", path) if path.starts_with("/logbooks/") && path.ends_with("/pull") => {
            match serde_json::from_slice::<CloudPullEventsRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.pull_events(payload)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => cloud_error(error, request_id),
                },
                Err(_) => json_error(
                    400,
                    "invalid pull request",
                    ApiErrorCode::InvalidJson,
                    request_id,
                ),
            }
        }
        ("POST", path) if path.starts_with("/logbooks/") && path.ends_with("/push") => {
            match serde_json::from_slice::<CloudPushEventsRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.push_events(payload)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => cloud_error(error, request_id),
                },
                Err(_) => json_error(
                    400,
                    "invalid push request",
                    ApiErrorCode::InvalidJson,
                    request_id,
                ),
            }
        }
        ("GET", "/sync/status") => {
            let auth = auth_from_token(token.as_deref());
            match runtime.block_on(server.status(auth.as_ref())) {
                Ok(payload) => json_response(&payload),
                Err(error) => cloud_error(error, request_id),
            }
        }
        ("POST", "/reports") => {
            match serde_json::from_slice::<DiagnosticReportUploadRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.upload_report(payload)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => cloud_error(error, request_id),
                },
                Err(_) => json_error(
                    400,
                    "invalid report upload request",
                    ApiErrorCode::InvalidJson,
                    request_id,
                ),
            }
        }
        ("GET", path) if path.starts_with("/reports/") => match auth_from_token(token.as_deref()) {
            Some(auth) => {
                let report_id = path.trim_start_matches("/reports/");
                match runtime.block_on(server.report_metadata(&auth, report_id)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => cloud_error(error, request_id),
                }
            }
            None => json_error(401, "missing token", ApiErrorCode::MissingToken, request_id),
        },
        _ => json_error(404, "not found", ApiErrorCode::NotFound, request_id),
    }
}

fn with_logbook_auth(
    path: &str,
    token: Option<&str>,
    suffix: &str,
    request_id: String,
    handler: impl FnOnce(CloudAuth, Uuid) -> ApiResponse,
) -> ApiResponse {
    let Some(auth) = auth_from_token(token) else {
        return json_error(401, "missing token", ApiErrorCode::MissingToken, request_id);
    };
    let Some(logbook_id) = path
        .trim_start_matches("/logbooks/")
        .trim_end_matches(suffix)
        .trim_end_matches('/')
        .parse::<Uuid>()
        .ok()
    else {
        return json_error(
            400,
            "invalid logbook id",
            ApiErrorCode::InvalidUuid,
            request_id,
        );
    };
    handler(auth, logbook_id)
}

fn auth_from_token(token: Option<&str>) -> Option<CloudAuth> {
    token
        .filter(|token| !token.is_empty())
        .map(|sync_token| CloudAuth {
            sync_token: sync_token.to_owned(),
        })
}

fn response(status: u16, body: Vec<u8>) -> ApiResponse {
    ApiResponse {
        status,
        headers: std::collections::HashMap::new(),
        body,
    }
}

fn json_response<T: Serialize>(payload: &T) -> ApiResponse {
    let body = serde_json::to_vec(payload).expect("sync payload should serialize");
    response(200, body)
}

fn cloud_error(error: CloudSyncError, request_id: String) -> ApiResponse {
    match error {
        CloudSyncError::Unauthenticated => json_error(
            401,
            "unauthenticated",
            ApiErrorCode::InvalidToken,
            request_id,
        ),
        CloudSyncError::UnauthorizedLogbook(_) => {
            json_error(403, "forbidden", ApiErrorCode::Forbidden, request_id)
        }
        CloudSyncError::PairingRejected(_) | CloudSyncError::Validation(_) => json_error(
            400,
            "request validation failed",
            ApiErrorCode::ValidationFailed,
            request_id,
        ),
        CloudSyncError::Store(_) => json_error(
            500,
            "request could not be completed",
            ApiErrorCode::StoreUnavailable,
            request_id,
        ),
    }
}

fn json_error(
    status: u16,
    message: impl Into<String>,
    code: ApiErrorCode,
    request_id: String,
) -> ApiResponse {
    let body = serde_json::to_vec(&ApiErrorBody::new(message.into(), code, request_id, false))
        .expect("error payload should serialize");
    response(status, body)
}

fn request_id(request: &ApiRequest) -> String {
    request
        .headers
        .get("x-request-id")
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync_storage::DurableCloudSyncPaths;
    use crate::{http, HostedServer, MergedServer};
    use ham_core::sync::{
        CloudPullEventsResponse, CloudPushEventsResponse, CloudServerConfig, ListLogbooksResponse,
        PairDeviceResponse, ReplicationStatus,
    };
    use ham_core::{CoreEventEnvelope, NewLogbookEvent};
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};

    const EVENT_QSO_CREATED: &str = "official.log.qso.created";

    fn durable_paths(label: &str) -> DurableCloudSyncPaths {
        let root =
            std::env::temp_dir().join(format!("ke8ygw-ham-server-sync-{label}-{}", Uuid::new_v4()));
        DurableCloudSyncPaths {
            metadata_store_path: root.join("surrealdb"),
            official_event_log_path: root.join("official-events.jsonl"),
            report_dir: root.join("reports"),
        }
    }

    fn durable_server(
        config: CloudServerConfig,
        paths: &DurableCloudSyncPaths,
    ) -> Arc<DurableCloudSyncServer> {
        let mut last_error = None;
        for _ in 0..20 {
            match DurableCloudSyncServer::open(config.clone(), paths.clone()) {
                Ok(server) => return Arc::new(server),
                Err(error) => {
                    last_error = Some(error);
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        panic!(
            "failed to open SurrealDB sync route test server: {}",
            last_error.unwrap()
        );
    }

    fn pair_request(logbook_id: Uuid, device_id: Uuid) -> PairDeviceRequest {
        PairDeviceRequest {
            pairing_code: "local-dev-pairing-code".to_owned(),
            account_id: "acct-route".to_owned(),
            user_id: "user-route".to_owned(),
            device_id,
            device_name: "Route Test Device".to_owned(),
            requested_logbooks: vec![logbook_id],
            role_hints: vec!["admin".to_owned()],
        }
    }

    fn api_request(method: &str, target: impl Into<String>, body: Vec<u8>) -> ApiRequest {
        let target = target.into();
        let (path, query) = target
            .split_once('?')
            .map_or((target.as_str(), ""), |(path, query)| (path, query));
        let mut headers = HashMap::new();
        headers.insert("x-request-id".to_owned(), "sync-route-test".to_owned());
        headers.insert("content-length".to_owned(), body.len().to_string());
        ApiRequest {
            method: method.to_owned(),
            path: path.to_owned(),
            query: crate::parse_query(query),
            headers,
            body,
        }
    }

    fn http_request(method: &str, target: impl Into<String>, body: impl Serialize) -> ApiRequest {
        let body = serde_json::to_vec(&body).expect("test request should serialize");
        api_request(method, target, body)
    }

    fn empty_http_request(method: &str, target: impl Into<String>) -> ApiRequest {
        api_request(method, target, Vec::new())
    }

    /// Renders an [`ApiResponse`] as raw HTTP so route tests and wire tests can
    /// share the same assertions.
    fn raw_bytes(response: &ApiResponse) -> Vec<u8> {
        let mut out = format!(
            "HTTP/1.1 {} STATUS\r\nContent-Length: {}\r\n\r\n",
            response.status,
            response.body.len()
        )
        .into_bytes();
        out.extend_from_slice(&response.body);
        out
    }

    fn route_request(
        server: Arc<DurableCloudSyncServer>,
        runtime: &tokio::runtime::Runtime,
        request: ApiRequest,
    ) -> Vec<u8> {
        raw_bytes(&route(&server, runtime, &request))
    }

    fn response_status(response: &[u8]) -> u16 {
        let text = String::from_utf8(response.to_vec()).expect("HTTP response should be UTF-8");
        text.lines()
            .next()
            .expect("HTTP response should have status line")
            .split_whitespace()
            .nth(1)
            .expect("HTTP status line should contain status code")
            .parse()
            .expect("HTTP status should be numeric")
    }

    fn response_body(response: &[u8]) -> &[u8] {
        let marker = b"\r\n\r\n";
        let start = response
            .windows(marker.len())
            .position(|window| window == marker)
            .expect("HTTP response should contain body separator")
            + marker.len();
        &response[start..]
    }

    fn response_json<T: DeserializeOwned>(response: &[u8]) -> T {
        serde_json::from_slice(response_body(response)).expect("HTTP response body should be JSON")
    }

    fn route_json<T: DeserializeOwned>(
        server: Arc<DurableCloudSyncServer>,
        runtime: &tokio::runtime::Runtime,
        request: ApiRequest,
    ) -> T {
        let response = route_request(server, runtime, request);
        assert_eq!(response_status(&response), 200);
        response_json(&response)
    }

    fn raw_json_http_request(
        method: &str,
        target: impl AsRef<str>,
        body: impl Serialize,
    ) -> Vec<u8> {
        let body = serde_json::to_vec(&body).expect("test request should serialize");
        let mut request = format!(
            "{method} {} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\nAccept: application/json\r\nX-Request-Id: sync-wire-test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            target.as_ref(),
            body.len()
        )
        .into_bytes();
        request.extend_from_slice(&body);
        request
    }

    /// Serves the sync routes through the same merged listener the binary uses,
    /// so wire tests exercise the real dispatch path.
    fn wire_round_trip(server: Arc<DurableCloudSyncServer>, request: Vec<u8>) -> Vec<u8> {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
        let addr = listener
            .local_addr()
            .expect("test listener should have a local address");
        let merged = MergedServer::new(HostedServer::new(), server);
        let handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("test runtime should start");
            let (mut stream, _) = listener.accept().expect("test request should connect");
            http::handle_stream(&merged, &runtime, &mut stream)
                .expect("test request should be handled");
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

    fn wire_json<T: DeserializeOwned>(server: Arc<DurableCloudSyncServer>, request: Vec<u8>) -> T {
        let response = wire_round_trip(server, request);
        assert_eq!(response_status(&response), 200);
        response_json(&response)
    }

    fn sample_qso_event(
        logbook_id: Uuid,
        previous_hash: Option<String>,
        device_id: Uuid,
    ) -> CoreEventEnvelope {
        let qso_id = Uuid::new_v4();
        CoreEventEnvelope::from_new(
            NewLogbookEvent {
                event_type: EVENT_QSO_CREATED.to_owned(),
                logbook_id,
                entity_id: Some(qso_id),
                author_operator_id: None,
                station_callsign: "K1HTTP".to_owned(),
                operator_callsign: Some("K1HTTP".to_owned()),
                author_device_id: device_id,
                source_device_id: device_id,
                correlation_id: Uuid::new_v4(),
                source_plugin_id: Some("self-hosted-route-test".to_owned()),
                schema_version: 1,
                payload: json!({
                    "qso_id": qso_id,
                    "station_callsign": "K1HTTP",
                    "operator_callsign": "K1HTTP",
                    "contacted_callsign": "K1REMOTE",
                    "started_at": "2026-07-05T12:00:00Z",
                    "band": "20m",
                    "mode": "SSB"
                }),
            },
            previous_hash,
        )
    }

    #[test]
    fn self_hosted_errors_keep_stable_shape() {
        let response = json_error(
            401,
            "missing token",
            ApiErrorCode::MissingToken,
            "sync-contract-test".to_owned(),
        );
        assert_eq!(response.status, 401);
        let json: Value =
            serde_json::from_slice(&response.body).expect("error body should be JSON");
        assert_eq!(json["error"], "missing token");
        assert_eq!(json["code"], "missing_token");
        assert_eq!(json["request_id"], "sync-contract-test");
        assert_eq!(json["retryable"], false);
    }

    #[test]
    fn self_hosted_routes_pair_push_pull_duplicates_and_auth_errors() {
        let runtime = tokio::runtime::Runtime::new().expect("test runtime should start");
        let paths = durable_paths("route-round-trip");
        let server = durable_server(CloudServerConfig::default(), &paths);
        let logbook_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();

        let pair: PairDeviceResponse = route_json(
            server.clone(),
            &runtime,
            http_request(
                "POST",
                "/api/v1/self-hosted/auth/pair",
                pair_request(logbook_id, device_id),
            ),
        );
        assert!(pair.accepted);
        let session = pair.session.expect("paired route should return a session");
        assert_eq!(session.authorized_logbooks, vec![logbook_id]);
        assert!(session.expires_at.is_some());
        let auth = CloudAuth {
            sync_token: session.sync_token.clone(),
        };

        let logbooks: ListLogbooksResponse = route_json(
            server.clone(),
            &runtime,
            empty_http_request(
                "GET",
                format!("/api/v1/self-hosted/logbooks?token={}", auth.sync_token),
            ),
        );
        assert_eq!(logbooks.logbooks.len(), 1);
        assert_eq!(logbooks.logbooks[0].logbook_id, logbook_id);
        assert_eq!(logbooks.logbooks[0].head_hash, None);

        let event = sample_qso_event(logbook_id, None, device_id);
        let push: CloudPushEventsResponse = route_json(
            server.clone(),
            &runtime,
            http_request(
                "POST",
                format!("/api/v1/self-hosted/logbooks/{logbook_id}/push"),
                CloudPushEventsRequest {
                    auth: auth.clone(),
                    logbook_id,
                    events: vec![event.clone()],
                },
            ),
        );
        assert_eq!(push.status, ReplicationStatus::Pulled);
        assert_eq!(push.accepted_count, 1);
        assert_eq!(push.ignored_duplicate_count, 0);
        assert_eq!(push.rejected_count, 0);
        assert_eq!(push.server_head_hash, Some(event.event_hash.clone()));

        let duplicate: CloudPushEventsResponse = route_json(
            server.clone(),
            &runtime,
            http_request(
                "POST",
                format!("/api/v1/self-hosted/logbooks/{logbook_id}/push"),
                CloudPushEventsRequest {
                    auth: auth.clone(),
                    logbook_id,
                    events: vec![event.clone()],
                },
            ),
        );
        assert_eq!(duplicate.accepted_count, 0);
        assert_eq!(duplicate.ignored_duplicate_count, 1);
        assert_eq!(duplicate.server_head_hash, Some(event.event_hash.clone()));

        let pull: CloudPullEventsResponse = route_json(
            server.clone(),
            &runtime,
            http_request(
                "POST",
                format!("/api/v1/self-hosted/logbooks/{logbook_id}/pull"),
                CloudPullEventsRequest {
                    auth,
                    logbook_id,
                    local_head_hash: None,
                },
            ),
        );
        assert_eq!(pull.preview.status, ReplicationStatus::RemoteAhead);
        assert_eq!(pull.preview.missing_event_count, 1);
        assert_eq!(pull.events, vec![event]);

        let bad_auth_response = route_request(
            server,
            &runtime,
            empty_http_request("GET", "/api/v1/self-hosted/logbooks?token=missing-token"),
        );
        assert_eq!(response_status(&bad_auth_response), 401);
        let error: ApiErrorBody = response_json(&bad_auth_response);
        assert_eq!(error.code, ApiErrorCode::InvalidToken.as_str());

        let _ = std::fs::remove_dir_all(
            paths
                .metadata_store_path
                .parent()
                .expect("test paths should have a root"),
        );
    }

    #[test]
    fn self_hosted_wire_endpoint_pair_push_pull_round_trip() {
        let paths = durable_paths("wire-round-trip");
        let server = durable_server(CloudServerConfig::default(), &paths);
        let logbook_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();

        let pair: PairDeviceResponse = wire_json(
            server.clone(),
            raw_json_http_request(
                "POST",
                "/api/v1/self-hosted/auth/pair",
                pair_request(logbook_id, device_id),
            ),
        );
        assert!(pair.accepted);
        let session = pair.session.expect("wire pairing should return a session");
        let auth = CloudAuth {
            sync_token: session.sync_token,
        };

        let event = sample_qso_event(logbook_id, None, device_id);
        let push: CloudPushEventsResponse = wire_json(
            server.clone(),
            raw_json_http_request(
                "POST",
                format!("/api/v1/self-hosted/logbooks/{logbook_id}/push"),
                CloudPushEventsRequest {
                    auth: auth.clone(),
                    logbook_id,
                    events: vec![event.clone()],
                },
            ),
        );
        assert_eq!(push.status, ReplicationStatus::Pulled);
        assert_eq!(push.accepted_count, 1);
        assert_eq!(push.server_head_hash, Some(event.event_hash.clone()));

        let pull: CloudPullEventsResponse = wire_json(
            server,
            raw_json_http_request(
                "POST",
                format!("/api/v1/self-hosted/logbooks/{logbook_id}/pull"),
                json!({
                    "auth": {
                        "sync_token": auth.sync_token,
                    },
                    "logbook_id": logbook_id,
                    "local_head_hash": null,
                }),
            ),
        );
        assert_eq!(pull.preview.status, ReplicationStatus::RemoteAhead);
        assert_eq!(pull.events, vec![event]);

        let _ = std::fs::remove_dir_all(
            paths
                .metadata_store_path
                .parent()
                .expect("test paths should have a root"),
        );
    }

    #[test]
    fn self_hosted_routes_reject_expired_session_tokens() {
        let runtime = tokio::runtime::Runtime::new().expect("test runtime should start");
        let paths = durable_paths("expired-route-token");
        let server = durable_server(
            CloudServerConfig {
                sync_session_ttl_seconds: Some(0),
                ..CloudServerConfig::default()
            },
            &paths,
        );
        let logbook_id = Uuid::new_v4();

        let pair: PairDeviceResponse = route_json(
            server.clone(),
            &runtime,
            http_request(
                "POST",
                "/api/v1/self-hosted/auth/pair",
                pair_request(logbook_id, Uuid::new_v4()),
            ),
        );
        let session = pair.session.expect("pairing should still issue a session");
        assert!(session.expires_at.is_some());

        let response = route_request(
            server,
            &runtime,
            empty_http_request(
                "GET",
                format!("/api/v1/self-hosted/logbooks?token={}", session.sync_token),
            ),
        );

        assert_eq!(response_status(&response), 401);
        let error: ApiErrorBody = response_json(&response);
        assert_eq!(error.code, ApiErrorCode::InvalidToken.as_str());

        let _ = std::fs::remove_dir_all(
            paths
                .metadata_store_path
                .parent()
                .expect("test paths should have a root"),
        );
    }
}
