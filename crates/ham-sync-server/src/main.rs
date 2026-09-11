use std::{
    collections::HashMap,
    env,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use ham_api_contract::{ApiErrorBody, ApiErrorCode};
use ham_sync::{
    projector::{
        ProjectorConfig, DEFAULT_PROJECTION_BATCH_SIZE, DEFAULT_PROJECTION_POLL_INTERVAL_SECONDS,
    },
    CloudAuth, CloudHealthResponse, CloudPreviewPullRequest, CloudPullEventsRequest,
    CloudPushEventsRequest, CloudServerConfig, CloudServiceMode, DiagnosticReportUploadRequest,
    DurableCloudSyncPaths, DurableCloudSyncServer, PairDeviceRequest,
    DEFAULT_CLOUD_SYNC_SESSION_TTL_SECONDS,
};
use serde::Serialize;
use uuid::Uuid;

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        print_usage();
        return;
    }
    // A full projection rebuild is destructive to the projection (never to the
    // official log) and is therefore only ever an explicit operator action.
    let rebuild_projection = args.iter().any(|arg| arg == "--rebuild-projection");
    if let Some(unknown) = args
        .iter()
        .find(|arg| !matches!(arg.as_str(), "--rebuild-projection"))
    {
        eprintln!("unknown argument: {unknown}");
        print_usage();
        process::exit(2);
    }

    let addr = env::var("HAM_SYNC_SERVER_BIND").unwrap_or_else(|_| "127.0.0.1:9740".to_owned());
    let public_url = env::var("HAM_SYNC_PUBLIC_URL").unwrap_or_else(|_| format!("http://{addr}"));
    let pairing_code =
        env::var("HAM_SYNC_PAIRING_CODE").unwrap_or_else(|_| "local-dev-pairing-code".to_owned());
    let sync_session_ttl_seconds = match env::var("HAM_SYNC_SESSION_TTL_SECONDS") {
        Ok(value) => match value.parse::<i64>() {
            Ok(seconds) if seconds > 0 => Some(seconds),
            _ => {
                eprintln!("HAM_SYNC_SESSION_TTL_SECONDS must be a positive integer");
                process::exit(1);
            }
        },
        Err(_) => Some(DEFAULT_CLOUD_SYNC_SESSION_TTL_SECONDS),
    };
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
            sync_session_ttl_seconds,
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

    let projection_shutdown = start_projector(&server, rebuild_projection);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_client(server.clone(), &runtime, stream),
            Err(error) => eprintln!("failed to accept sync request: {error}"),
        }
    }

    projection_shutdown.store(true, Ordering::Relaxed);
}

fn print_usage() {
    eprintln!(
        "usage:
  ham-sync-server [--rebuild-projection]

Options:
  --rebuild-projection  Wipe the SurrealDB projection and replay the whole
                        official event log before serving. This rebuilds the
                        projection only; the append-only JSONL official log is
                        never modified. Without this flag the projector resumes
                        from its stored checkpoint.
  -h, --help            Show this help.

The server reads its configuration from the environment; see .env.example."
    );
}

/// Starts the JSONL -> SurrealDB projector and returns its shutdown flag.
///
/// The projector is a one-way reader of the official log: it never writes to the
/// log, and the SurrealDB projection it maintains is never authoritative.
fn start_projector(server: &Arc<DurableCloudSyncServer>, rebuild: bool) -> Arc<AtomicBool> {
    let shutdown = Arc::new(AtomicBool::new(false));
    if env::var("HAM_SYNC_PROJECTION_ENABLED").as_deref() == Ok("0") {
        println!("projection: disabled by HAM_SYNC_PROJECTION_ENABLED=0");
        return shutdown;
    }

    let mut config = ProjectorConfig::new(server.official_event_log_path());
    config.batch_size = parse_env_usize(
        "HAM_SYNC_PROJECTION_BATCH_SIZE",
        DEFAULT_PROJECTION_BATCH_SIZE,
    );
    config.poll_interval = Duration::from_secs(parse_env_u64(
        "HAM_SYNC_PROJECTION_POLL_SECONDS",
        DEFAULT_PROJECTION_POLL_INTERVAL_SECONDS,
    ));
    if let Ok(writer_id) = env::var("HAM_SYNC_PROJECTION_WRITER_ID") {
        config.writer_id = writer_id;
    }

    let projector = match server.projector(config) {
        Ok(projector) => Arc::new(projector),
        Err(error) => {
            report_projection_failure(&format!("failed to open the projector: {error}"));
            return shutdown;
        }
    };

    if rebuild {
        println!("projection: full rebuild requested; replaying the official event log");
        match projector.rebuild() {
            Ok(report) => println!(
                "projection: rebuilt {} entries into SurrealDB in {} ms (generation {})",
                report.events_projected, report.elapsed_ms, report.checkpoint.rebuild_generation
            ),
            Err(error) => {
                report_projection_failure(&error.to_string());
                return shutdown;
            }
        }
    }

    match projector.checkpoint() {
        Ok(checkpoint) if checkpoint.is_halted() => {
            report_projection_failure(&format!(
                "the projection is halted at official event log entry {}: {}",
                checkpoint.halt_sequence.unwrap_or(checkpoint.sequence),
                checkpoint
                    .halt_reason
                    .as_deref()
                    .unwrap_or("unknown reason")
            ));
            return shutdown;
        }
        Ok(checkpoint) => println!(
            "projection: resuming from entry {} (byte offset {})",
            checkpoint.sequence, checkpoint.byte_offset
        ),
        Err(error) => {
            report_projection_failure(&format!(
                "failed to read the projection checkpoint: {error}"
            ));
            return shutdown;
        }
    }

    thread::spawn({
        let projector = Arc::clone(&projector);
        let shutdown = Arc::clone(&shutdown);
        move || {
            if let Err(error) = projector.tail(&shutdown) {
                report_projection_failure(&error.to_string());
            }
        }
    });
    shutdown
}

/// Prints a projection failure so it cannot be mistaken for routine log noise.
///
/// The sync server keeps serving: the append-only official log and the sync
/// protocol are unaffected by a frozen read model, and taking the server down
/// would remove the very access an operator needs to investigate. The halt is
/// also recorded durably in the projection checkpoint.
fn report_projection_failure(detail: &str) {
    eprintln!(
        "\n\
         ======================================================================\n\
         ham-sync-server: SURREALDB PROJECTION STOPPED\n\
         {detail}\n\
         The SurrealDB projection is frozen and is now STALE. Anything reading \n\
         it (GUI, plugins, queries) is seeing out-of-date data. Sync and the \n\
         append-only official event log are unaffected.\n\
         Investigate the official event log, then restart with \n\
         `ham-sync-server --rebuild-projection` to replay it from the start.\n\
         ======================================================================\n"
    );
}

fn parse_env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn parse_env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
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
    let response = route_request(server, runtime, request);

    let _ = stream.write_all(&response);
}

fn route_request(
    server: Arc<DurableCloudSyncServer>,
    runtime: &tokio::runtime::Runtime,
    request: HttpRequest,
) -> Vec<u8> {
    let (path, query) = split_target(&request.target);
    let request_id = request_id(&request);

    match (request.method.as_str(), path) {
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
    }
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
    use ham_core::{CoreEventEnvelope, NewLogbookEvent};
    use ham_sync::{
        CloudPullEventsResponse, CloudPushEventsResponse, ListLogbooksResponse, PairDeviceResponse,
        PreviewPullResponse, ReplicationStatus,
    };
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};

    const EVENT_QSO_CREATED: &str = "official.log.qso.created";

    fn durable_paths(label: &str) -> DurableCloudSyncPaths {
        let root =
            std::env::temp_dir().join(format!("ke8ygw-ham-sync-server-{label}-{}", Uuid::new_v4()));
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

    fn http_request(method: &str, target: impl Into<String>, body: impl Serialize) -> HttpRequest {
        let body = serde_json::to_vec(&body).expect("test request should serialize");
        let mut headers = HashMap::new();
        headers.insert("x-request-id".to_owned(), "sync-route-test".to_owned());
        headers.insert("content-length".to_owned(), body.len().to_string());
        HttpRequest {
            method: method.to_owned(),
            target: target.into(),
            headers,
            body,
        }
    }

    fn empty_http_request(method: &str, target: impl Into<String>) -> HttpRequest {
        HttpRequest {
            method: method.to_owned(),
            target: target.into(),
            headers: HashMap::from([("x-request-id".to_owned(), "sync-route-test".to_owned())]),
            body: Vec::new(),
        }
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
        request: HttpRequest,
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

    fn wire_round_trip(server: Arc<DurableCloudSyncServer>, request: Vec<u8>) -> Vec<u8> {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
        let addr = listener
            .local_addr()
            .expect("test listener should have a local address");
        let server_thread = server.clone();
        let handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("test runtime should start");
            let (stream, _) = listener.accept().expect("test request should connect");
            handle_client(server_thread, &runtime, stream);
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
                "/api/v1/auth/pair",
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
            empty_http_request("GET", format!("/api/v1/logbooks?token={}", auth.sync_token)),
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
                format!("/api/v1/logbooks/{logbook_id}/push"),
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
                format!("/api/v1/logbooks/{logbook_id}/push"),
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
                format!("/api/v1/logbooks/{logbook_id}/pull"),
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
            empty_http_request("GET", "/api/v1/logbooks?token=missing-token"),
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
                "/api/v1/auth/pair",
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
                format!("/api/v1/logbooks/{logbook_id}/push"),
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
                format!("/api/v1/logbooks/{logbook_id}/pull"),
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
                "/api/v1/auth/pair",
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
                format!("/api/v1/logbooks?token={}", session.sync_token),
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

    #[test]
    fn self_hosted_wire_endpoint_rejects_divergent_branch_and_reconciles_after_pull() {
        let paths = durable_paths("wire-divergence-reconcile");
        let server = durable_server(CloudServerConfig::default(), &paths);
        let logbook_id = Uuid::new_v4();
        let desktop_device = Uuid::new_v4();
        let ios_device = Uuid::new_v4();

        let desktop_pair: PairDeviceResponse = wire_json(
            server.clone(),
            raw_json_http_request(
                "POST",
                "/api/v1/auth/pair",
                pair_request(logbook_id, desktop_device),
            ),
        );
        assert!(desktop_pair.accepted);
        let desktop_auth = CloudAuth {
            sync_token: desktop_pair
                .session
                .expect("desktop pairing should return a session")
                .sync_token,
        };
        let ios_pair: PairDeviceResponse = wire_json(
            server.clone(),
            raw_json_http_request(
                "POST",
                "/api/v1/auth/pair",
                pair_request(logbook_id, ios_device),
            ),
        );
        let ios_auth = CloudAuth {
            sync_token: ios_pair
                .session
                .expect("iOS pairing should return a session")
                .sync_token,
        };

        let base = sample_qso_event(logbook_id, None, desktop_device);
        let accepted: CloudPushEventsResponse = wire_json(
            server.clone(),
            raw_json_http_request(
                "POST",
                format!("/api/v1/logbooks/{logbook_id}/push"),
                CloudPushEventsRequest {
                    auth: desktop_auth.clone(),
                    logbook_id,
                    events: vec![base.clone()],
                },
            ),
        );
        assert_eq!(accepted.status, ReplicationStatus::Pulled);
        assert_eq!(accepted.accepted_count, 1);
        assert_eq!(accepted.server_head_hash, Some(base.event_hash.clone()));

        // The second device logged offline from an empty local chain, so its
        // branch does not continue the server head.
        let divergent = sample_qso_event(logbook_id, None, ios_device);
        let rejected: CloudPushEventsResponse = wire_json(
            server.clone(),
            raw_json_http_request(
                "POST",
                format!("/api/v1/logbooks/{logbook_id}/push"),
                CloudPushEventsRequest {
                    auth: ios_auth.clone(),
                    logbook_id,
                    events: vec![divergent.clone()],
                },
            ),
        );
        assert_eq!(rejected.status, ReplicationStatus::Diverged);
        assert_eq!(rejected.accepted_count, 0);
        assert_eq!(rejected.rejected_count, 1);
        assert_eq!(
            rejected.server_head_hash,
            Some(base.event_hash.clone()),
            "a divergent branch must never advance the self-hosted head"
        );

        let divergent_preview: PreviewPullResponse = wire_json(
            server.clone(),
            raw_json_http_request(
                "POST",
                format!("/api/v1/logbooks/{logbook_id}/preview-pull"),
                CloudPreviewPullRequest {
                    auth: ios_auth.clone(),
                    logbook_id,
                    local_head_hash: Some(divergent.event_hash.clone()),
                },
            ),
        );
        assert_eq!(divergent_preview.status, ReplicationStatus::Diverged);
        assert_eq!(divergent_preview.missing_event_count, 0);
        assert!(divergent_preview.events.is_empty());

        // The safe recovery path is to pull the real head first and re-apply the
        // offline work on top of it.
        let pull: CloudPullEventsResponse = wire_json(
            server.clone(),
            raw_json_http_request(
                "POST",
                format!("/api/v1/logbooks/{logbook_id}/pull"),
                CloudPullEventsRequest {
                    auth: ios_auth.clone(),
                    logbook_id,
                    local_head_hash: None,
                },
            ),
        );
        assert_eq!(pull.preview.status, ReplicationStatus::RemoteAhead);
        assert_eq!(pull.events, vec![base.clone()]);

        let reapplied = sample_qso_event(logbook_id, Some(base.event_hash.clone()), ios_device);
        let reconciled: CloudPushEventsResponse = wire_json(
            server.clone(),
            raw_json_http_request(
                "POST",
                format!("/api/v1/logbooks/{logbook_id}/push"),
                CloudPushEventsRequest {
                    auth: ios_auth.clone(),
                    logbook_id,
                    events: vec![reapplied.clone()],
                },
            ),
        );
        assert_eq!(reconciled.status, ReplicationStatus::Pulled);
        assert_eq!(reconciled.accepted_count, 1);
        assert_eq!(
            reconciled.server_head_hash,
            Some(reapplied.event_hash.clone())
        );

        let replayed: CloudPushEventsResponse = wire_json(
            server.clone(),
            raw_json_http_request(
                "POST",
                format!("/api/v1/logbooks/{logbook_id}/push"),
                CloudPushEventsRequest {
                    auth: ios_auth,
                    logbook_id,
                    events: vec![base.clone(), reapplied.clone()],
                },
            ),
        );
        assert_eq!(replayed.status, ReplicationStatus::Pulled);
        assert_eq!(replayed.accepted_count, 0);
        assert_eq!(replayed.ignored_duplicate_count, 2);
        assert_eq!(
            replayed.server_head_hash,
            Some(reapplied.event_hash.clone())
        );

        let official_log = std::fs::read_to_string(&paths.official_event_log_path)
            .expect("durable official event log should be readable");
        assert_eq!(official_log.matches(&base.event_hash).count(), 2);
        assert_eq!(official_log.matches(&reapplied.event_hash).count(), 1);
        assert_eq!(
            official_log.matches(&divergent.event_hash).count(),
            0,
            "rejected divergent events must never reach durable storage"
        );

        let _ = std::fs::remove_dir_all(
            paths
                .metadata_store_path
                .parent()
                .expect("test paths should have a root"),
        );
    }
}
