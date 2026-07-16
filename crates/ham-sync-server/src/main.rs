use std::{
    collections::HashMap,
    env,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process,
    sync::Arc,
};

use ham_api_contract::{ApiErrorBody, ApiErrorCode};
use ham_sync::{
    CloudAuth, CloudHealthResponse, CloudPreviewPullRequest, CloudPullEventsRequest,
    CloudPushEventsRequest, CloudServerConfig, CloudServiceMode, DiagnosticReportUploadRequest,
    DurableCloudSyncPaths, DurableCloudSyncServer, PairDeviceRequest,
};
use serde::Serialize;
use uuid::Uuid;

fn main() {
    let addr = env::var("HAM_SYNC_SERVER_BIND").unwrap_or_else(|_| "127.0.0.1:9740".to_owned());
    let public_url = env::var("HAM_SYNC_PUBLIC_URL").unwrap_or_else(|_| format!("http://{addr}"));
    let pairing_code =
        env::var("HAM_SYNC_PAIRING_CODE").unwrap_or_else(|_| "local-dev-pairing-code".to_owned());
    let mode = match env::var("HAM_SYNC_SERVICE_MODE")
        .unwrap_or_else(|_| "self_hosted".to_owned())
        .as_str()
    {
        "hosted" => CloudServiceMode::Hosted,
        _ => CloudServiceMode::SelfHosted,
    };

    let paths = DurableCloudSyncPaths::from_env();
    let server = match DurableCloudSyncServer::open(
        CloudServerConfig {
            mode,
            public_url,
            pairing_code,
        },
        paths.clone(),
    ) {
        Ok(server) => Arc::new(server),
        Err(error) => {
            eprintln!("failed to initialize durable sync storage: {error}");
            process::exit(1);
        }
    };
    let listener = match TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("failed to bind sync server to {addr}: {error}");
            process::exit(1);
        }
    };
    let runtime = tokio::runtime::Runtime::new().expect("sync server runtime should start");

    println!("ham-sync-server listening on http://{addr}");
    println!("mode: {mode:?}");
    println!("metadata store: {}", paths.metadata_store_path.display());
    println!(
        "official event log: {}",
        paths.official_event_log_path.display()
    );
    println!("report directory: {}", paths.report_dir.display());

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_client(server.clone(), &runtime, stream),
            Err(error) => eprintln!("failed to accept sync request: {error}"),
        }
    }
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    target: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

fn handle_client(
    server: Arc<DurableCloudSyncServer>,
    runtime: &tokio::runtime::Runtime,
    mut stream: TcpStream,
) {
    let request = {
        let mut reader = BufReader::new(&mut stream);
        match read_http_request(&mut reader) {
            Ok(request) => request,
            Err(_) => return,
        }
    };
    let (path, query) = split_target(&request.target);
    let request_id = request_id(&request);

    let response = match (request.method.as_str(), path) {
        ("GET", "/health") => json_response(&server.health()),
        ("POST", "/api/v1/auth/pair") => {
            match serde_json::from_slice::<PairDeviceRequest>(&request.body) {
                Ok(pair) => json_response(&runtime.block_on(server.pair_device(pair))),
                Err(_) => json_error(
                    400,
                    "invalid pair request",
                    ApiErrorCode::InvalidJson,
                    request_id.clone(),
                ),
            }
        }
        ("GET", "/api/v1/logbooks") => match auth_from_query(query) {
            Some(auth) => match runtime.block_on(server.list_logbooks(&auth)) {
                Ok(payload) => json_response(&payload),
                Err(error) => cloud_error(error, request_id.clone()),
            },
            None => json_error(
                401,
                "missing token",
                ApiErrorCode::MissingToken,
                request_id.clone(),
            ),
        },
        ("GET", path) if path.starts_with("/api/v1/logbooks/") && path.ends_with("/head") => {
            with_logbook_auth(
                path,
                query,
                "/head",
                request_id.clone(),
                |auth, logbook_id| match runtime.block_on(server.get_head(&auth, logbook_id)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => cloud_error(error, request_id.clone()),
                },
            )
        }
        ("GET", path) if path.starts_with("/api/v1/logbooks/") && path.ends_with("/events") => {
            with_logbook_auth(
                path,
                query,
                "/events",
                request_id.clone(),
                |auth, logbook_id| {
                    let after_hash = parse_query(query).get("after_hash").cloned();
                    match runtime.block_on(server.event_metadata(&auth, logbook_id, after_hash)) {
                        Ok(payload) => json_response(&payload),
                        Err(error) => cloud_error(error, request_id.clone()),
                    }
                },
            )
        }
        ("POST", path)
            if path.starts_with("/api/v1/logbooks/") && path.ends_with("/preview-pull") =>
        {
            match serde_json::from_slice::<CloudPreviewPullRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.preview_pull(payload)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => cloud_error(error, request_id.clone()),
                },
                Err(_) => json_error(
                    400,
                    "invalid preview request",
                    ApiErrorCode::InvalidJson,
                    request_id.clone(),
                ),
            }
        }
        ("POST", path) if path.starts_with("/api/v1/logbooks/") && path.ends_with("/pull") => {
            match serde_json::from_slice::<CloudPullEventsRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.pull_events(payload)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => cloud_error(error, request_id.clone()),
                },
                Err(_) => json_error(
                    400,
                    "invalid pull request",
                    ApiErrorCode::InvalidJson,
                    request_id.clone(),
                ),
            }
        }
        ("POST", path) if path.starts_with("/api/v1/logbooks/") && path.ends_with("/push") => {
            match serde_json::from_slice::<CloudPushEventsRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.push_events(payload)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => cloud_error(error, request_id.clone()),
                },
                Err(_) => json_error(
                    400,
                    "invalid push request",
                    ApiErrorCode::InvalidJson,
                    request_id.clone(),
                ),
            }
        }
        ("GET", "/api/v1/sync/status") => match auth_from_query(query) {
            Some(auth) => match runtime.block_on(server.status(Some(&auth))) {
                Ok(payload) => json_response(&payload),
                Err(error) => cloud_error(error, request_id.clone()),
            },
            None => match runtime.block_on(server.status(None)) {
                Ok(payload) => json_response(&payload),
                Err(error) => cloud_error(error, request_id.clone()),
            },
        },
        ("POST", "/api/v1/reports") => {
            match serde_json::from_slice::<DiagnosticReportUploadRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.upload_report(payload)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => cloud_error(error, request_id.clone()),
                },
                Err(_) => json_error(
                    400,
                    "invalid report upload request",
                    ApiErrorCode::InvalidJson,
                    request_id.clone(),
                ),
            }
        }
        ("GET", path) if path.starts_with("/api/v1/reports/") => match auth_from_query(query) {
            Some(auth) => {
                let report_id = path.trim_start_matches("/api/v1/reports/");
                match runtime.block_on(server.report_metadata(&auth, report_id)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => cloud_error(error, request_id.clone()),
                }
            }
            None => json_error(
                401,
                "missing token",
                ApiErrorCode::MissingToken,
                request_id.clone(),
            ),
        },
        _ => json_error(404, "not found", ApiErrorCode::NotFound, request_id),
    };

    let _ = stream.write_all(&response);
}

fn read_http_request(reader: &mut BufReader<&mut TcpStream>) -> std::io::Result<HttpRequest> {
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_owned();
    let target = parts.next().unwrap_or("/").to_owned();

    let mut content_length = 0usize;
    let mut headers = HashMap::new();
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
    Ok(HttpRequest {
        method,
        target,
        headers,
        body,
    })
}

fn with_logbook_auth(
    path: &str,
    query: &str,
    suffix: &str,
    request_id: String,
    handler: impl FnOnce(CloudAuth, Uuid) -> Vec<u8>,
) -> Vec<u8> {
    let Some(auth) = auth_from_query(query) else {
        return json_error(401, "missing token", ApiErrorCode::MissingToken, request_id);
    };
    let Some(logbook_id) = path
        .trim_start_matches("/api/v1/logbooks/")
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

fn auth_from_query(query: &str) -> Option<CloudAuth> {
    parse_query(query)
        .get("token")
        .filter(|token| !token.is_empty())
        .map(|sync_token| CloudAuth {
            sync_token: sync_token.clone(),
        })
}

fn split_target(target: &str) -> (&str, &str) {
    target
        .split_once('?')
        .map_or((target, ""), |(path, query)| (path, query))
}

fn parse_query(query: &str) -> std::collections::HashMap<String, String> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (key.to_owned(), value.replace('+', " "))
        })
        .collect()
}

fn json_response<T: Serialize>(payload: &T) -> Vec<u8> {
    let body = serde_json::to_vec(payload).expect("sync payload should serialize");
    response(200, "application/json; charset=utf-8", &body)
}

fn cloud_error(error: ham_sync::CloudSyncError, request_id: String) -> Vec<u8> {
    match error {
        ham_sync::CloudSyncError::Unauthenticated => json_error(
            401,
            "unauthenticated",
            ApiErrorCode::InvalidToken,
            request_id,
        ),
        ham_sync::CloudSyncError::UnauthorizedLogbook(_) => {
            json_error(403, "forbidden", ApiErrorCode::Forbidden, request_id)
        }
        ham_sync::CloudSyncError::PairingRejected(_) | ham_sync::CloudSyncError::Validation(_) => {
            json_error(
                400,
                "request validation failed",
                ApiErrorCode::ValidationFailed,
                request_id,
            )
        }
        ham_sync::CloudSyncError::Store(_) => json_error(
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
) -> Vec<u8> {
    let body = serde_json::to_vec(&ApiErrorBody::new(message.into(), code, request_id, false))
        .expect("error payload should serialize");
    response(status, "application/json; charset=utf-8", &body)
}

fn request_id(request: &HttpRequest) -> String {
    request
        .headers
        .get("x-request-id")
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn response(status: u16, content_type: &str, body: &[u8]) -> Vec<u8> {
    let status_text = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        _ => "OK",
    };
    let mut response = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .into_bytes();
    response.extend_from_slice(body);
    response
}

#[allow(dead_code)]
fn _assert_health_is_serializable(_: CloudHealthResponse) {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn self_hosted_errors_keep_stable_shape() {
        let response = json_error(
            401,
            "missing token",
            ApiErrorCode::MissingToken,
            "sync-contract-test".to_owned(),
        );
        let text = String::from_utf8(response).expect("HTTP response should be UTF-8");
        let body = text
            .split("\r\n\r\n")
            .nth(1)
            .expect("HTTP response should contain a body");
        let json: Value = serde_json::from_str(body).expect("error body should be JSON");
        assert_eq!(json["error"], "missing token");
        assert_eq!(json["code"], "missing_token");
        assert_eq!(json["request_id"], "sync-contract-test");
        assert_eq!(json["retryable"], false);
    }
}
