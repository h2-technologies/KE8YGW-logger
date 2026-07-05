use std::{
    collections::HashMap,
    env,
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    process,
    sync::Arc,
    thread,
    time::Duration,
};

use ham_core::{RuntimeEventFilter, RuntimeEventSeverity, RuntimeLogConfig};
use ham_gui::{
    mock::{capability_labels, mock_plugins},
    CommandRegistry, GuiRuntimeBridge, GuiShellState, RuntimeBridgeStatus, RuntimeEventInput,
};
use serde::Serialize;
use serde_json::json;

const INDEX_HTML: &str = include_str!("../web/index.html");
const APP_CSS: &str = include_str!("../web/styles.css");
const APP_JS: &str = include_str!("../web/app.js");

fn main() {
    let addr = env::args()
        .nth(1)
        .unwrap_or_else(|| "127.0.0.1:9467".to_owned());

    let listener = match TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("failed to bind ham-gui to {addr}: {error}");
            process::exit(1);
        }
    };

    let bound_addr = listener
        .local_addr()
        .map(|addr| addr.to_string())
        .unwrap_or(addr);
    let bridge = match GuiRuntimeBridge::new(RuntimeLogConfig::default_for_app()) {
        Ok(bridge) => bridge,
        Err(error) => {
            eprintln!("failed to initialize runtime bridge: {error}");
            process::exit(1);
        }
    };
    if let Err(error) = bridge.seed_startup_events() {
        eprintln!("failed to seed startup runtime events: {error}");
    }

    start_demo_runtime_publisher(bridge.clone());

    let state = Arc::new(AppState { bridge });

    println!("ham-gui listening on http://{bound_addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_client(state.clone(), stream),
            Err(error) => eprintln!("failed to accept request: {error}"),
        }
    }
}

#[derive(Debug)]
struct AppState {
    bridge: GuiRuntimeBridge,
}

fn handle_client(state: Arc<AppState>, mut stream: TcpStream) {
    let request_line = {
        let mut reader = BufReader::new(&mut stream);
        let mut line = String::new();
        if reader.read_line(&mut line).is_err() {
            return;
        }
        line
    };

    let target = request_line.split_whitespace().nth(1).unwrap_or("/");
    let (path, query) = split_target(target);

    let response = match path {
        "/" | "/index.html" => response(200, "text/html; charset=utf-8", INDEX_HTML.as_bytes()),
        "/styles.css" => response(200, "text/css; charset=utf-8", APP_CSS.as_bytes()),
        "/app.js" => response(200, "text/javascript; charset=utf-8", APP_JS.as_bytes()),
        "/api/shell" => json_response(&ApiShellPayload {
            shell: GuiShellState::default_shell(),
            commands: CommandRegistry::default_registry(),
            plugins: mock_plugins(),
            runtime_events: state.bridge.replay(RuntimeEventFilter::default(), 100),
            runtime_status: state.bridge.status(),
            known_core_capabilities: capability_labels(),
        }),
        "/api/runtime-events" => {
            let params = parse_query(query);
            let filter = runtime_filter_from_query(&params);
            json_response(&ApiRuntimeEventsPayload {
                runtime_events: state.bridge.replay(filter, 250),
                runtime_status: state.bridge.status(),
            })
        }
        "/api/runtime-events/export" => {
            let params = parse_query(query);
            let filter = runtime_filter_from_query(&params);
            match state.bridge.export_jsonl(filter, 1_000) {
                Ok(bytes) => response_with_headers(
                    200,
                    "application/x-ndjson; charset=utf-8",
                    &bytes,
                    &[(
                        "Content-Disposition",
                        "attachment; filename=\"runtime-events.jsonl\"",
                    )],
                ),
                Err(error) => response(
                    500,
                    "text/plain; charset=utf-8",
                    format!("failed to export runtime events: {error}").as_bytes(),
                ),
            }
        }
        _ => response(404, "text/plain; charset=utf-8", b"not found"),
    };

    if let Err(error) = stream.write_all(&response) {
        eprintln!("failed to write response: {error}");
    }
}

#[derive(Debug, Serialize)]
struct ApiShellPayload {
    shell: GuiShellState,
    commands: CommandRegistry,
    plugins: Vec<ham_gui::mock::MockPlugin>,
    runtime_events: Vec<ham_core::RuntimeDiagnosticEvent>,
    runtime_status: RuntimeBridgeStatus,
    known_core_capabilities: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ApiRuntimeEventsPayload {
    runtime_events: Vec<ham_core::RuntimeDiagnosticEvent>,
    runtime_status: RuntimeBridgeStatus,
}

fn json_response<T: Serialize>(payload: &T) -> Vec<u8> {
    let body = serde_json::to_vec(payload).expect("serializing GUI shell payload should not fail");
    response(200, "application/json; charset=utf-8", &body)
}

fn response(status: u16, content_type: &str, body: &[u8]) -> Vec<u8> {
    response_with_headers(status, content_type, body, &[])
}

fn response_with_headers(
    status: u16,
    content_type: &str,
    body: &[u8],
    extra_headers: &[(&str, &str)],
) -> Vec<u8> {
    let status_text = match status {
        200 => "OK",
        404 => "Not Found",
        500 => "Internal Server Error",
        _ => "OK",
    };
    let mut header = format!(
        "HTTP/1.1 {status} {status_text}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len(),
    );
    for (name, value) in extra_headers {
        header.push_str(name);
        header.push_str(": ");
        header.push_str(value);
        header.push_str("\r\n");
    }
    header.push_str("\r\n");

    let mut response = header.into_bytes();
    response.extend_from_slice(body);
    response
}

fn split_target(target: &str) -> (&str, &str) {
    target
        .split_once('?')
        .map_or((target, ""), |(path, query)| (path, query))
}

fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (decode_query_value(key), decode_query_value(value))
        })
        .collect()
}

fn runtime_filter_from_query(params: &HashMap<String, String>) -> RuntimeEventFilter {
    RuntimeEventFilter {
        severity: params
            .get("severity")
            .and_then(|severity| parse_severity(severity)),
        category: params
            .get("category")
            .filter(|value| !value.is_empty())
            .cloned(),
        source: params
            .get("source")
            .filter(|value| !value.is_empty())
            .cloned(),
        text: params
            .get("text")
            .filter(|value| !value.is_empty())
            .cloned(),
    }
}

fn parse_severity(value: &str) -> Option<RuntimeEventSeverity> {
    match value {
        "trace" => Some(RuntimeEventSeverity::Trace),
        "debug" => Some(RuntimeEventSeverity::Debug),
        "info" => Some(RuntimeEventSeverity::Info),
        "warn" => Some(RuntimeEventSeverity::Warn),
        "error" => Some(RuntimeEventSeverity::Error),
        _ => None,
    }
}

fn decode_query_value(value: &str) -> String {
    value.replace('+', " ")
}

fn start_demo_runtime_publisher(bridge: GuiRuntimeBridge) {
    thread::spawn(move || {
        let events = [
            (
                "ui.workspace.rendered",
                RuntimeEventSeverity::Debug,
                "Workspace render completed",
            ),
            (
                "plugin.registry.heartbeat",
                RuntimeEventSeverity::Info,
                "Plugin registry heartbeat",
            ),
            (
                "diagnostics.monitor.refresh",
                RuntimeEventSeverity::Trace,
                "Event Bus Monitor replay refreshed",
            ),
            (
                "network.offline",
                RuntimeEventSeverity::Warn,
                "Network integrations are offline in local demo mode",
            ),
        ];
        let mut index = 0usize;
        loop {
            thread::sleep(Duration::from_secs(5));
            let (event_type, severity, summary) = events[index % events.len()];
            if let Err(error) = bridge.publish(RuntimeEventInput {
                event_type: event_type.to_owned(),
                severity,
                source: "ham-gui".to_owned(),
                source_plugin_id: None,
                workspace_id: Some("dashboard".to_owned()),
                payload_summary: summary.to_owned(),
                redacted_payload: Some(json!({"demo": true, "api_token": "redacted-by-core"})),
                error: None,
            }) {
                eprintln!("failed to publish demo runtime event: {error}");
            }
            index += 1;
        }
    });
}
