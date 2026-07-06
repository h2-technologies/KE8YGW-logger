use std::{
    env,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process,
    sync::Arc,
};

use ham_sync::{
    CloudAuth, CloudHealthResponse, CloudPreviewPullRequest, CloudPullEventsRequest,
    CloudPushEventsRequest, CloudServerConfig, CloudServiceMode, InMemoryCloudSyncServer,
    PairDeviceRequest,
};
use serde::Serialize;
use serde_json::json;
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

    let listener = match TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("failed to bind sync server to {addr}: {error}");
            process::exit(1);
        }
    };
    let runtime = tokio::runtime::Runtime::new().expect("sync server runtime should start");
    let server = Arc::new(InMemoryCloudSyncServer::new(CloudServerConfig {
        mode,
        public_url,
        pairing_code,
    }));

    println!("ham-sync-server listening on http://{addr}");
    println!("mode: {mode:?}");
    println!("storage: in-memory MVP backend");

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
    body: Vec<u8>,
}

fn handle_client(
    server: Arc<InMemoryCloudSyncServer>,
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

    let response = match (request.method.as_str(), path) {
        ("GET", "/health") => json_response(&server.health()),
        ("POST", "/api/v1/auth/pair") => {
            match serde_json::from_slice::<PairDeviceRequest>(&request.body) {
                Ok(pair) => json_response(&runtime.block_on(server.pair_device(pair))),
                Err(error) => json_error(400, format!("invalid pair request: {error}")),
            }
        }
        ("GET", "/api/v1/logbooks") => match auth_from_query(query) {
            Some(auth) => match runtime.block_on(server.list_logbooks(&auth)) {
                Ok(payload) => json_response(&payload),
                Err(error) => json_error(403, error.to_string()),
            },
            None => json_error(401, "missing token"),
        },
        ("GET", path) if path.starts_with("/api/v1/logbooks/") && path.ends_with("/head") => {
            with_logbook_auth(path, query, "/head", |auth, logbook_id| {
                match runtime.block_on(server.get_head(&auth, logbook_id)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => json_error(403, error.to_string()),
                }
            })
        }
        ("GET", path) if path.starts_with("/api/v1/logbooks/") && path.ends_with("/events") => {
            with_logbook_auth(path, query, "/events", |auth, logbook_id| {
                let after_hash = parse_query(query).get("after_hash").cloned();
                match runtime.block_on(server.event_metadata(&auth, logbook_id, after_hash)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => json_error(403, error.to_string()),
                }
            })
        }
        ("POST", path)
            if path.starts_with("/api/v1/logbooks/") && path.ends_with("/preview-pull") =>
        {
            match serde_json::from_slice::<CloudPreviewPullRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.preview_pull(payload)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => json_error(403, error.to_string()),
                },
                Err(error) => json_error(400, format!("invalid preview request: {error}")),
            }
        }
        ("POST", path) if path.starts_with("/api/v1/logbooks/") && path.ends_with("/pull") => {
            match serde_json::from_slice::<CloudPullEventsRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.pull_events(payload)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => json_error(403, error.to_string()),
                },
                Err(error) => json_error(400, format!("invalid pull request: {error}")),
            }
        }
        ("POST", path) if path.starts_with("/api/v1/logbooks/") && path.ends_with("/push") => {
            match serde_json::from_slice::<CloudPushEventsRequest>(&request.body) {
                Ok(payload) => match runtime.block_on(server.push_events(payload)) {
                    Ok(payload) => json_response(&payload),
                    Err(error) => json_error(403, error.to_string()),
                },
                Err(error) => json_error(400, format!("invalid push request: {error}")),
            }
        }
        ("GET", "/api/v1/sync/status") => match auth_from_query(query) {
            Some(auth) => match runtime.block_on(server.status(Some(&auth))) {
                Ok(payload) => json_response(&payload),
                Err(error) => json_error(403, error.to_string()),
            },
            None => match runtime.block_on(server.status(None)) {
                Ok(payload) => json_response(&payload),
                Err(error) => json_error(403, error.to_string()),
            },
        },
        _ => json_error(404, "not found"),
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
    loop {
        let mut header = String::new();
        reader.read_line(&mut header)?;
        let header = header.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().unwrap_or(0);
            }
        }
    }

    let mut body = vec![0; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body)?;
    }
    Ok(HttpRequest {
        method,
        target,
        body,
    })
}

fn with_logbook_auth(
    path: &str,
    query: &str,
    suffix: &str,
    handler: impl FnOnce(CloudAuth, Uuid) -> Vec<u8>,
) -> Vec<u8> {
    let Some(auth) = auth_from_query(query) else {
        return json_error(401, "missing token");
    };
    let Some(logbook_id) = path
        .trim_start_matches("/api/v1/logbooks/")
        .trim_end_matches(suffix)
        .trim_end_matches('/')
        .parse::<Uuid>()
        .ok()
    else {
        return json_error(400, "invalid logbook id");
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

fn json_error(status: u16, message: impl Into<String>) -> Vec<u8> {
    let body = serde_json::to_vec(&json!({ "error": message.into() }))
        .expect("error payload should serialize");
    response(status, "application/json; charset=utf-8", &body)
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
