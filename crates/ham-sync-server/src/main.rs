use std::{
    collections::HashMap,
    env,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::Path,
    process,
    sync::Arc,
    time::Instant,
};

use ham_api_contract::{ApiErrorBody, ApiErrorCode, SELF_HOSTED_ROUTE_STRINGS};
use ham_metrics::{
    MetricsAccess, RouteMatcher, ScrapeAuthorization, ServerMetrics, PROMETHEUS_CONTENT_TYPE,
};
use ham_sync::{
    CloudAuth, CloudHealthResponse, CloudPreviewPullRequest, CloudPullEventsRequest,
    CloudPushEventsRequest, CloudServerConfig, CloudServiceMode, DiagnosticReportUploadRequest,
    DurableCloudSyncPaths, DurableCloudSyncServer, PairDeviceRequest, ReplicationStatus,
    DEFAULT_CLOUD_SYNC_SESSION_TTL_SECONDS,
};
use serde::Serialize;
use serde_json::json;
use uuid::Uuid;

/// `service` label applied to every metric this binary exports.
const METRICS_SERVICE: &str = "ke8ygw-sync-server";
/// Observability routes served in addition to the `/api/v1` sync contract.
const OBSERVABILITY_ROUTES: &[&str] = &["GET /metrics", "GET /ready"];
/// Bounded walk applied when measuring the metadata store directory.
const MAX_STORAGE_WALK_ENTRIES: usize = 20_000;

fn main() {
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
    let context = Arc::new(SyncServerContext::new(
        server,
        paths.clone(),
        MetricsAccess::from_env("HAM_SYNC_METRICS_ENABLED", "HAM_SYNC_METRICS_TOKEN"),
        mode,
    ));

    println!("ham-sync-server listening on http://{addr}");
    println!("mode: {mode:?}");
    println!("metadata store: {}", paths.metadata_store_path.display());
    println!(
        "official event log: {}",
        paths.official_event_log_path.display()
    );
    println!("report directory: {}", paths.report_dir.display());
    match (
        context.metrics_access.enabled(),
        context.metrics_access.requires_token(),
    ) {
        (false, _) => println!("metrics: disabled (HAM_SYNC_METRICS_ENABLED)"),
        (true, true) => println!("metrics: http://{addr}/metrics (bearer token required)"),
        (true, false) => println!(
            "metrics: http://{addr}/metrics (unauthenticated; set HAM_SYNC_METRICS_TOKEN and keep the scrape port private)"
        ),
    }

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_client(&context, &runtime, stream),
            Err(error) => eprintln!("failed to accept sync request: {error}"),
        }
    }
}

/// Everything a request handler needs beyond the sync backend itself.
struct SyncServerContext {
    server: Arc<DurableCloudSyncServer>,
    paths: DurableCloudSyncPaths,
    metrics: ServerMetrics,
    metrics_access: MetricsAccess,
    routes: RouteMatcher,
}

impl SyncServerContext {
    fn new(
        server: Arc<DurableCloudSyncServer>,
        paths: DurableCloudSyncPaths,
        metrics_access: MetricsAccess,
        mode: CloudServiceMode,
    ) -> Self {
        let mode = match mode {
            CloudServiceMode::Hosted => "hosted",
            CloudServiceMode::SelfHosted => "self_hosted",
        };
        Self {
            server,
            paths,
            metrics: ServerMetrics::new(METRICS_SERVICE, env!("CARGO_PKG_VERSION"), mode),
            metrics_access,
            routes: RouteMatcher::new(SELF_HOSTED_ROUTE_STRINGS).with_routes(OBSERVABILITY_ROUTES),
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

/// Status, content type, and body of a response before it is written to the wire.
#[derive(Debug)]
struct HttpResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

fn handle_client(
    context: &SyncServerContext,
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
    let response = route_request(context, runtime, request);

    let _ = stream.write_all(&response);
}

/// Serve one request and record its HTTP metrics.
fn route_request(
    context: &SyncServerContext,
    runtime: &tokio::runtime::Runtime,
    request: HttpRequest,
) -> Vec<u8> {
    let started = Instant::now();
    let in_flight = context.metrics.request_started();
    let method = request.method.clone();
    let request_bytes = request.body.len();
    let (path, _) = split_target(&request.target);
    let route = context.routes.label(&method, path).to_owned();

    let response = dispatch(context, runtime, request);

    context.metrics.record_http_request(
        &method,
        &route,
        response.status,
        started.elapsed(),
        request_bytes,
        response.body.len(),
    );
    drop(in_flight);
    encode_response(&response)
}

fn dispatch(
    context: &SyncServerContext,
    runtime: &tokio::runtime::Runtime,
    request: HttpRequest,
) -> HttpResponse {
    let server = context.server.clone();
    let (path, query) = split_target(&request.target);
    let request_id = request_id(&request);

    match (request.method.as_str(), path) {
        ("GET", "/health") => json_response(&server.health()),
        ("GET", "/ready") => readiness_response(context),
        ("GET", "/metrics") => metrics_response(context, &request, request_id.clone()),
        ("POST", "/api/v1/auth/pair") => {
            match serde_json::from_slice::<PairDeviceRequest>(&request.body) {
                Ok(pair) => {
                    let response = runtime.block_on(server.pair_device(pair));
                    context.metrics.increment(
                        "ham_sync_pair_requests_total",
                        "Device pairing attempts handled by the sync server.",
                        &[(
                            "result",
                            if response.accepted {
                                "accepted"
                            } else {
                                "rejected"
                            },
                        )],
                    );
                    json_response(&response)
                }
                Err(_) => {
                    record_error(context, "invalid_request");
                    json_error(
                        400,
                        "invalid pair request",
                        ApiErrorCode::InvalidJson,
                        request_id.clone(),
                    )
                }
            }
        }
        ("GET", "/api/v1/logbooks") => match auth_from_query(query) {
            Some(auth) => match runtime.block_on(server.list_logbooks(&auth)) {
                Ok(payload) => json_response(&payload),
                Err(error) => cloud_error(context, error, request_id.clone()),
            },
            None => {
                record_error(context, "missing_token");
                json_error(
                    401,
                    "missing token",
                    ApiErrorCode::MissingToken,
                    request_id.clone(),
                )
            }
        },
        ("GET", path) if path.starts_with("/api/v1/logbooks/") && path.ends_with("/head") => {
            with_logbook_auth(
                context,
                path,
                query,
                "/head",
                request_id.clone(),
                |auth, logbook_id| match runtime.block_on(server.get_head(&auth, logbook_id)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => cloud_error(context, error, request_id.clone()),
                },
            )
        }
        ("GET", path) if path.starts_with("/api/v1/logbooks/") && path.ends_with("/events") => {
            with_logbook_auth(
                context,
                path,
                query,
                "/events",
                request_id.clone(),
                |auth, logbook_id| {
                    let after_hash = parse_query(query).get("after_hash").cloned();
                    match runtime.block_on(server.event_metadata(&auth, logbook_id, after_hash)) {
                        Ok(payload) => json_response(&payload),
                        Err(error) => cloud_error(context, error, request_id.clone()),
                    }
                },
            )
        }
        ("POST", path)
            if path.starts_with("/api/v1/logbooks/") && path.ends_with("/preview-pull") =>
        {
            match serde_json::from_slice::<CloudPreviewPullRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.preview_pull(payload)) {
                    Ok(payload) => {
                        record_replication_status(context, "preview", payload.status);
                        json_response(&payload)
                    }
                    Err(error) => cloud_error(context, error, request_id.clone()),
                },
                Err(_) => {
                    record_error(context, "invalid_request");
                    json_error(
                        400,
                        "invalid preview request",
                        ApiErrorCode::InvalidJson,
                        request_id.clone(),
                    )
                }
            }
        }
        ("POST", path) if path.starts_with("/api/v1/logbooks/") && path.ends_with("/pull") => {
            match serde_json::from_slice::<CloudPullEventsRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.pull_events(payload)) {
                    Ok(payload) => {
                        record_replication_status(context, "pull", payload.preview.status);
                        context.metrics.add(
                            "ham_sync_events_pulled_total",
                            "Official events returned to clients by pull requests.",
                            &[],
                            payload.events.len() as f64,
                        );
                        json_response(&payload)
                    }
                    Err(error) => cloud_error(context, error, request_id.clone()),
                },
                Err(_) => {
                    record_error(context, "invalid_request");
                    json_error(
                        400,
                        "invalid pull request",
                        ApiErrorCode::InvalidJson,
                        request_id.clone(),
                    )
                }
            }
        }
        ("POST", path) if path.starts_with("/api/v1/logbooks/") && path.ends_with("/push") => {
            match serde_json::from_slice::<CloudPushEventsRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.push_events(payload)) {
                    Ok(payload) => {
                        record_replication_status(context, "push", payload.status);
                        record_push_counts(
                            context,
                            payload.accepted_count,
                            payload.ignored_duplicate_count,
                            payload.rejected_count,
                        );
                        json_response(&payload)
                    }
                    Err(error) => cloud_error(context, error, request_id.clone()),
                },
                Err(_) => {
                    record_error(context, "invalid_request");
                    json_error(
                        400,
                        "invalid push request",
                        ApiErrorCode::InvalidJson,
                        request_id.clone(),
                    )
                }
            }
        }
        ("GET", "/api/v1/sync/status") => match auth_from_query(query) {
            Some(auth) => match runtime.block_on(server.status(Some(&auth))) {
                Ok(payload) => json_response(&payload),
                Err(error) => cloud_error(context, error, request_id.clone()),
            },
            None => match runtime.block_on(server.status(None)) {
                Ok(payload) => json_response(&payload),
                Err(error) => cloud_error(context, error, request_id.clone()),
            },
        },
        ("POST", "/api/v1/reports") => {
            match serde_json::from_slice::<DiagnosticReportUploadRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.upload_report(payload)) {
                    Ok(payload) => {
                        context.metrics.increment(
                            "ham_sync_report_uploads_total",
                            "Diagnostic report bundles accepted by the sync server.",
                            &[("status", report_status_label(payload.status))],
                        );
                        json_response(&payload)
                    }
                    Err(error) => cloud_error(context, error, request_id.clone()),
                },
                Err(_) => {
                    record_error(context, "invalid_request");
                    json_error(
                        400,
                        "invalid report upload request",
                        ApiErrorCode::InvalidJson,
                        request_id.clone(),
                    )
                }
            }
        }
        ("GET", path) if path.starts_with("/api/v1/reports/") => match auth_from_query(query) {
            Some(auth) => {
                let report_id = path.trim_start_matches("/api/v1/reports/");
                match runtime.block_on(server.report_metadata(&auth, report_id)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => cloud_error(context, error, request_id.clone()),
                }
            }
            None => {
                record_error(context, "missing_token");
                json_error(
                    401,
                    "missing token",
                    ApiErrorCode::MissingToken,
                    request_id.clone(),
                )
            }
        },
        _ => json_error(404, "not found", ApiErrorCode::NotFound, request_id),
    }
}

/// Serve the Prometheus exposition body for an authorized scrape.
fn metrics_response(
    context: &SyncServerContext,
    request: &HttpRequest,
    request_id: String,
) -> HttpResponse {
    match context
        .metrics_access
        .authorize(request.headers.get("authorization").map(String::as_str))
    {
        ScrapeAuthorization::Allowed => {
            refresh_storage_gauges(context);
            HttpResponse {
                status: 200,
                content_type: PROMETHEUS_CONTENT_TYPE,
                body: context.metrics.render().into_bytes(),
            }
        }
        ScrapeAuthorization::Disabled => {
            json_error(404, "not found", ApiErrorCode::NotFound, request_id)
        }
        ScrapeAuthorization::Unauthorized => json_error(
            401,
            "metrics scrape token required",
            ApiErrorCode::MissingToken,
            request_id,
        ),
    }
}

/// Storage checks behind `/ready` and the `ham_sync_ready` gauge.
struct ReadinessChecks {
    metadata_store: bool,
    official_event_log_directory: bool,
    report_directory: bool,
}

impl ReadinessChecks {
    fn inspect(paths: &DurableCloudSyncPaths) -> Self {
        Self {
            metadata_store: paths.metadata_store_path.exists(),
            // The JSONL official event log file is created on first append, so
            // this checks the directory that will hold it, not the file.
            official_event_log_directory: paths
                .official_event_log_path
                .parent()
                .is_none_or(Path::is_dir),
            report_directory: paths.report_dir.is_dir(),
        }
    }

    const fn ready(&self) -> bool {
        self.metadata_store && self.official_event_log_directory && self.report_directory
    }
}

/// Report whether durable sync storage is reachable for writes and reads.
fn readiness_response(context: &SyncServerContext) -> HttpResponse {
    let checks = refresh_readiness_gauge(context);
    let ready = checks.ready();
    let payload = json!({
        "ready": ready,
        "service": METRICS_SERVICE,
        "version": env!("CARGO_PKG_VERSION"),
        "checks": {
            "metadata_store": checks.metadata_store,
            "official_event_log_directory": checks.official_event_log_directory,
            "report_directory": checks.report_directory,
        }
    });
    HttpResponse {
        status: if ready { 200 } else { 503 },
        content_type: "application/json; charset=utf-8",
        body: serde_json::to_vec(&payload).expect("readiness payload should serialize"),
    }
}

/// Re-check storage and publish `ham_sync_ready`, so the gauge is present from
/// the first scrape rather than only after a `/ready` probe.
fn refresh_readiness_gauge(context: &SyncServerContext) -> ReadinessChecks {
    let checks = ReadinessChecks::inspect(&context.paths);
    context.metrics.gauge(
        "ham_sync_ready",
        "1 when durable sync storage is reachable, 0 otherwise.",
        &[],
        f64::from(u8::from(checks.ready())),
    );
    checks
}

/// Refresh storage gauges sampled from the filesystem at scrape time.
fn refresh_storage_gauges(context: &SyncServerContext) {
    refresh_readiness_gauge(context);
    let event_log = file_size_bytes(&context.paths.official_event_log_path);
    context.metrics.gauge(
        "ham_sync_event_log_bytes",
        "Size in bytes of the durable official event log.",
        &[],
        event_log as f64,
    );

    let (report_files, report_bytes) = directory_usage(&context.paths.report_dir);
    context.metrics.gauge(
        "ham_sync_report_files",
        "Diagnostic report files stored on the sync server.",
        &[],
        report_files as f64,
    );
    context.metrics.gauge(
        "ham_sync_report_bytes",
        "Total bytes of diagnostic report payloads stored on the sync server.",
        &[],
        report_bytes as f64,
    );

    let (_, metadata_bytes) = directory_usage(&context.paths.metadata_store_path);
    context.metrics.gauge(
        "ham_sync_metadata_store_bytes",
        "Total bytes used by the durable sync metadata store.",
        &[],
        metadata_bytes as f64,
    );
}

fn file_size_bytes(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

/// Count files and bytes under `root`, stopping after a bounded number of
/// entries so a scrape can never walk an unbounded directory tree.
fn directory_usage(root: &Path) -> (u64, u64) {
    let mut files = 0u64;
    let mut bytes = 0u64;
    let mut visited = 0usize;
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            visited += 1;
            if visited > MAX_STORAGE_WALK_ENTRIES {
                return (files, bytes);
            }
            match entry.metadata() {
                Ok(metadata) if metadata.is_dir() => pending.push(entry.path()),
                Ok(metadata) => {
                    files += 1;
                    bytes += metadata.len();
                }
                Err(_) => continue,
            }
        }
    }
    (files, bytes)
}

fn record_replication_status(
    context: &SyncServerContext,
    operation: &str,
    status: ReplicationStatus,
) {
    context.metrics.increment(
        "ham_sync_replication_results_total",
        "Replication outcomes by sync operation.",
        &[("operation", operation), ("status", status_label(status))],
    );
}

fn record_push_counts(
    context: &SyncServerContext,
    accepted: usize,
    duplicate: usize,
    rejected: usize,
) {
    for (result, count) in [
        ("accepted", accepted),
        ("duplicate", duplicate),
        ("rejected", rejected),
    ] {
        context.metrics.add(
            "ham_sync_events_pushed_total",
            "Official events received by push requests, by server-side outcome.",
            &[("result", result)],
            count as f64,
        );
    }
}

fn record_error(context: &SyncServerContext, kind: &str) {
    context.metrics.increment(
        "ham_sync_errors_total",
        "Sync server request failures by cause.",
        &[("kind", kind)],
    );
}

const fn status_label(status: ReplicationStatus) -> &'static str {
    match status {
        ReplicationStatus::InSync => "in_sync",
        ReplicationStatus::RemoteAhead => "remote_ahead",
        ReplicationStatus::Pulled => "pulled",
        ReplicationStatus::Diverged => "diverged",
        ReplicationStatus::Rejected => "rejected",
    }
}

const fn report_status_label(status: ham_sync::DiagnosticReportStatus) -> &'static str {
    match status {
        ham_sync::DiagnosticReportStatus::Submitted => "submitted",
        ham_sync::DiagnosticReportStatus::Triaged => "triaged",
        ham_sync::DiagnosticReportStatus::Investigating => "investigating",
        ham_sync::DiagnosticReportStatus::WaitingOnUser => "waiting_on_user",
        ham_sync::DiagnosticReportStatus::Fixed => "fixed",
        ham_sync::DiagnosticReportStatus::Closed => "closed",
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
    context: &SyncServerContext,
    path: &str,
    query: &str,
    suffix: &str,
    request_id: String,
    handler: impl FnOnce(CloudAuth, Uuid) -> HttpResponse,
) -> HttpResponse {
    let Some(auth) = auth_from_query(query) else {
        record_error(context, "missing_token");
        return json_error(401, "missing token", ApiErrorCode::MissingToken, request_id);
    };
    let Some(logbook_id) = path
        .trim_start_matches("/api/v1/logbooks/")
        .trim_end_matches(suffix)
        .trim_end_matches('/')
        .parse::<Uuid>()
        .ok()
    else {
        record_error(context, "invalid_request");
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

fn json_response<T: Serialize>(payload: &T) -> HttpResponse {
    HttpResponse {
        status: 200,
        content_type: "application/json; charset=utf-8",
        body: serde_json::to_vec(payload).expect("sync payload should serialize"),
    }
}

fn cloud_error(
    context: &SyncServerContext,
    error: ham_sync::CloudSyncError,
    request_id: String,
) -> HttpResponse {
    match error {
        ham_sync::CloudSyncError::Unauthenticated => {
            record_error(context, "unauthenticated");
            json_error(
                401,
                "unauthenticated",
                ApiErrorCode::InvalidToken,
                request_id,
            )
        }
        ham_sync::CloudSyncError::UnauthorizedLogbook(_) => {
            record_error(context, "unauthorized_logbook");
            json_error(403, "forbidden", ApiErrorCode::Forbidden, request_id)
        }
        ham_sync::CloudSyncError::PairingRejected(_) | ham_sync::CloudSyncError::Validation(_) => {
            record_error(context, "validation");
            json_error(
                400,
                "request validation failed",
                ApiErrorCode::ValidationFailed,
                request_id,
            )
        }
        ham_sync::CloudSyncError::Store(_) => {
            record_error(context, "store");
            json_error(
                500,
                "request could not be completed",
                ApiErrorCode::StoreUnavailable,
                request_id,
            )
        }
    }
}

fn json_error(
    status: u16,
    message: impl Into<String>,
    code: ApiErrorCode,
    request_id: String,
) -> HttpResponse {
    HttpResponse {
        status,
        content_type: "application/json; charset=utf-8",
        body: serde_json::to_vec(&ApiErrorBody::new(message.into(), code, request_id, false))
            .expect("error payload should serialize"),
    }
}

fn request_id(request: &HttpRequest) -> String {
    request
        .headers
        .get("x-request-id")
        .filter(|value| !value.trim().is_empty())
        .cloned()
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

fn encode_response(response: &HttpResponse) -> Vec<u8> {
    let status_text = match response.status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        403 => "Forbidden",
        404 => "Not Found",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "OK",
    };
    let mut encoded = format!(
        "HTTP/1.1 {} {status_text}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.status,
        response.content_type,
        response.body.len()
    )
    .into_bytes();
    encoded.extend_from_slice(&response.body);
    encoded
}

#[allow(dead_code)]
fn _assert_health_is_serializable(_: CloudHealthResponse) {}

#[cfg(test)]
mod tests {
    use super::*;
    use ham_core::{CoreEventEnvelope, NewLogbookEvent};
    use ham_sync::{
        CloudPullEventsResponse, CloudPushEventsResponse, ListLogbooksResponse, PairDeviceResponse,
        ReplicationStatus,
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
    ) -> Arc<SyncServerContext> {
        durable_server_with_access(config, paths, MetricsAccess::new(true, None))
    }

    fn durable_server_with_access(
        config: CloudServerConfig,
        paths: &DurableCloudSyncPaths,
        access: MetricsAccess,
    ) -> Arc<SyncServerContext> {
        let mut last_error = None;
        for _ in 0..20 {
            match DurableCloudSyncServer::open(config.clone(), paths.clone()) {
                Ok(server) => {
                    return Arc::new(SyncServerContext::new(
                        Arc::new(server),
                        paths.clone(),
                        access,
                        CloudServiceMode::SelfHosted,
                    ))
                }
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

    fn metrics_body(context: &SyncServerContext, token: Option<&str>) -> String {
        let mut request = empty_http_request("GET", "/metrics");
        if let Some(token) = token {
            request
                .headers
                .insert("authorization".to_owned(), format!("Bearer {token}"));
        }
        let response = route_request(context, &test_runtime(), request);
        assert_eq!(response_status(&response), 200);
        String::from_utf8(response_body(&response).to_vec())
            .expect("metrics body should be UTF-8 text")
    }

    fn test_runtime() -> tokio::runtime::Runtime {
        tokio::runtime::Runtime::new().expect("test runtime should start")
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
        server: Arc<SyncServerContext>,
        runtime: &tokio::runtime::Runtime,
        request: HttpRequest,
    ) -> T {
        let response = route_request(&server, runtime, request);
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

    fn wire_round_trip(server: Arc<SyncServerContext>, request: Vec<u8>) -> Vec<u8> {
        let listener = TcpListener::bind("127.0.0.1:0").expect("test listener should bind");
        let addr = listener
            .local_addr()
            .expect("test listener should have a local address");
        let server_thread = server.clone();
        let handle = std::thread::spawn(move || {
            let runtime = tokio::runtime::Runtime::new().expect("test runtime should start");
            let (stream, _) = listener.accept().expect("test request should connect");
            handle_client(&server_thread, &runtime, stream);
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

    fn wire_json<T: DeserializeOwned>(server: Arc<SyncServerContext>, request: Vec<u8>) -> T {
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
        let response = encode_response(&json_error(
            401,
            "missing token",
            ApiErrorCode::MissingToken,
            "sync-contract-test".to_owned(),
        ));
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
            &server,
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
            &server,
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
    fn metrics_endpoint_exposes_prometheus_text_for_sync_traffic() {
        let runtime = test_runtime();
        let paths = durable_paths("metrics-endpoint");
        let context = durable_server(CloudServerConfig::default(), &paths);
        let logbook_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();

        let pair: PairDeviceResponse = route_json(
            context.clone(),
            &runtime,
            http_request(
                "POST",
                "/api/v1/auth/pair",
                pair_request(logbook_id, device_id),
            ),
        );
        let auth = CloudAuth {
            sync_token: pair
                .session
                .expect("pairing should return a session")
                .sync_token,
        };
        let event = sample_qso_event(logbook_id, None, device_id);
        let _: CloudPushEventsResponse = route_json(
            context.clone(),
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
        let _: CloudPullEventsResponse = route_json(
            context.clone(),
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

        let body = metrics_body(&context, None);

        assert!(body.contains("# TYPE ham_http_requests_total counter"));
        assert!(body.contains("ham_build_info{mode=\"self_hosted\",service=\"ke8ygw-sync-server\""));
        assert!(body.contains(
            "ham_sync_pair_requests_total{result=\"accepted\",service=\"ke8ygw-sync-server\"} 1"
        ));
        assert!(body.contains(
            "ham_sync_events_pushed_total{result=\"accepted\",service=\"ke8ygw-sync-server\"} 1"
        ));
        assert!(body.contains("ham_sync_events_pulled_total{service=\"ke8ygw-sync-server\"} 1"));
        assert!(body.contains(
            "ham_sync_replication_results_total{operation=\"push\",service=\"ke8ygw-sync-server\",status=\"pulled\"} 1"
        ));
        assert!(body.contains(
            "ham_sync_replication_results_total{operation=\"pull\",service=\"ke8ygw-sync-server\",status=\"remote_ahead\"} 1"
        ));
        assert!(body.contains("ham_sync_event_log_bytes{service=\"ke8ygw-sync-server\"}"));
        assert!(body.contains("ham_sync_metadata_store_bytes{service=\"ke8ygw-sync-server\"}"));
        assert!(body.contains("ham_http_request_duration_seconds_bucket"));
        // The scrape counts itself, so exactly one request is in flight while
        // the exposition body is rendered.
        assert!(body.contains("ham_http_requests_in_flight{service=\"ke8ygw-sync-server\"} 1"));

        let _ = std::fs::remove_dir_all(
            paths
                .metadata_store_path
                .parent()
                .expect("test paths should have a root"),
        );
    }

    #[test]
    fn metrics_route_labels_stay_bounded_and_omit_identifiers() {
        let runtime = test_runtime();
        let paths = durable_paths("metrics-route-labels");
        let context = durable_server(CloudServerConfig::default(), &paths);
        let logbook_id = Uuid::new_v4();

        let unauthorized = route_request(
            &context,
            &runtime,
            empty_http_request(
                "GET",
                format!("/api/v1/logbooks/{logbook_id}/head?token=nope"),
            ),
        );
        assert_eq!(response_status(&unauthorized), 401);
        let missing = route_request(&context, &runtime, empty_http_request("GET", "/nope"));
        assert_eq!(response_status(&missing), 404);

        let body = metrics_body(&context, None);

        assert!(!body.contains(&logbook_id.to_string()), "{body}");
        assert!(body.contains("route=\"GET /api/v1/logbooks/:logbook_id/head\""));
        assert!(body.contains("route=\"unmatched\",service=\"ke8ygw-sync-server\",status=\"404\""));
        assert!(body.contains(
            "ham_sync_errors_total{kind=\"unauthenticated\",service=\"ke8ygw-sync-server\"} 1"
        ));

        let _ = std::fs::remove_dir_all(
            paths
                .metadata_store_path
                .parent()
                .expect("test paths should have a root"),
        );
    }

    #[test]
    fn metrics_endpoint_enforces_the_configured_scrape_token() {
        let runtime = test_runtime();
        let paths = durable_paths("metrics-token");
        let context = durable_server_with_access(
            CloudServerConfig::default(),
            &paths,
            MetricsAccess::new(true, Some("scrape-secret".to_owned())),
        );

        let anonymous = route_request(&context, &runtime, empty_http_request("GET", "/metrics"));
        assert_eq!(response_status(&anonymous), 401);
        let error: ApiErrorBody = response_json(&anonymous);
        assert_eq!(error.code, ApiErrorCode::MissingToken.as_str());

        let wrong_token = {
            let mut request = empty_http_request("GET", "/metrics");
            request
                .headers
                .insert("authorization".to_owned(), "Bearer wrong".to_owned());
            route_request(&context, &runtime, request)
        };
        assert_eq!(response_status(&wrong_token), 401);

        let body = metrics_body(&context, Some("scrape-secret"));
        assert!(body.contains("ham_build_info"));
        assert!(
            !body.contains("scrape-secret"),
            "scrape token must never be echoed"
        );

        let _ = std::fs::remove_dir_all(
            paths
                .metadata_store_path
                .parent()
                .expect("test paths should have a root"),
        );
    }

    #[test]
    fn a_disabled_metrics_endpoint_is_not_found() {
        let runtime = test_runtime();
        let paths = durable_paths("metrics-disabled");
        let context = durable_server_with_access(
            CloudServerConfig::default(),
            &paths,
            MetricsAccess::new(false, None),
        );

        let response = route_request(&context, &runtime, empty_http_request("GET", "/metrics"));

        assert_eq!(response_status(&response), 404);
        let error: ApiErrorBody = response_json(&response);
        assert_eq!(error.code, ApiErrorCode::NotFound.as_str());

        let _ = std::fs::remove_dir_all(
            paths
                .metadata_store_path
                .parent()
                .expect("test paths should have a root"),
        );
    }

    #[test]
    fn readiness_reports_durable_storage_state() {
        let runtime = test_runtime();
        let paths = durable_paths("readiness");
        let context = durable_server(CloudServerConfig::default(), &paths);

        let response = route_request(&context, &runtime, empty_http_request("GET", "/ready"));

        assert_eq!(response_status(&response), 200);
        let payload: Value = response_json(&response);
        assert_eq!(payload["ready"], true);
        assert_eq!(payload["service"], "ke8ygw-sync-server");
        assert_eq!(payload["checks"]["metadata_store"], true);
        assert_eq!(payload["checks"]["report_directory"], true);

        let body = metrics_body(&context, None);
        assert!(body.contains("ham_sync_ready{service=\"ke8ygw-sync-server\"} 1"));

        let _ = std::fs::remove_dir_all(
            paths
                .metadata_store_path
                .parent()
                .expect("test paths should have a root"),
        );
    }

    #[test]
    fn readiness_reports_unavailable_storage_with_503() {
        let runtime = test_runtime();
        let paths = durable_paths("readiness-missing");
        let context = durable_server(CloudServerConfig::default(), &paths);
        let _ = std::fs::remove_dir_all(&paths.report_dir);

        let response = route_request(&context, &runtime, empty_http_request("GET", "/ready"));

        assert_eq!(response_status(&response), 503);
        let payload: Value = response_json(&response);
        assert_eq!(payload["ready"], false);
        assert_eq!(payload["checks"]["report_directory"], false);

        let _ = std::fs::remove_dir_all(
            paths
                .metadata_store_path
                .parent()
                .expect("test paths should have a root"),
        );
    }

    #[test]
    fn the_metrics_endpoint_is_served_over_the_wire_with_the_prometheus_content_type() {
        let paths = durable_paths("metrics-wire");
        let context = durable_server(CloudServerConfig::default(), &paths);

        let response = wire_round_trip(
            context,
            "GET /metrics HTTP/1.1\r\nHost: 127.0.0.1\r\nX-Request-Id: sync-wire-test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                .to_owned()
                .into_bytes(),
        );

        let text = String::from_utf8(response).expect("HTTP response should be UTF-8");
        assert!(text.starts_with("HTTP/1.1 200 OK"));
        assert!(text.contains("Content-Type: text/plain; version=0.0.4; charset=utf-8"));
        assert!(text.contains("# TYPE ham_build_info gauge"));

        let _ = std::fs::remove_dir_all(
            paths
                .metadata_store_path
                .parent()
                .expect("test paths should have a root"),
        );
    }
}
