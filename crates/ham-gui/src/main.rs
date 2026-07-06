use std::{
    collections::HashMap,
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    process,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use ham_core::{
    default_official_event_log_path, export_adif, import_adif, submit_proposal, AdifImportOptions,
    JsonlLogbookEventStore, LogbookEventStore, OperatorRole, Projection, ProposalContext,
    RuntimeEventFilter, RuntimeEventSeverity, RuntimeLogConfig,
};
use ham_gui::{
    mock::{capability_labels, mock_plugins},
    CommandRegistry, GuiRuntimeBridge, GuiShellState, RuntimeBridgeStatus, RuntimeEventInput,
};
use ham_plugin_sdk::{
    PluginCapability, PluginManifest, ProposalEnvelope, PROPOSAL_QSO_CREATE, PROPOSAL_QSO_DELETE,
    PROPOSAL_QSO_NOTE_ADD, PROPOSAL_QSO_RESTORE,
};
use ham_sync::{
    build_handshake_response, DiscoveryPacket, HandshakeRequest, LocalPeerIdentity,
    LogbookHeadSummary, PeerObservation, PeerRecord, PeerRegistry, SyncConfig, PROTOCOL_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

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

    let proposal_runtime =
        tokio::runtime::Runtime::new().expect("creating GUI proposal runtime should succeed");
    let store_path = default_official_event_log_path();
    let store = match JsonlLogbookEventStore::open(&store_path) {
        Ok(store) => Arc::new(store),
        Err(error) => {
            eprintln!("failed to open official event store: {error}");
            process::exit(1);
        }
    };
    let logbook_id = default_logbook_id();
    let _ = bridge.publish(RuntimeEventInput {
        event_type: "storage.opened".to_owned(),
        severity: RuntimeEventSeverity::Info,
        source: "ham-gui".to_owned(),
        source_plugin_id: None,
        workspace_id: Some("dashboard".to_owned()),
        payload_summary: format!("Official event store opened at {}", store_path.display()),
        redacted_payload: None,
        error: None,
    });
    match proposal_runtime.block_on(store.verify_chain(logbook_id)) {
        Ok(()) => {
            let _ = bridge.publish(RuntimeEventInput {
                event_type: "official.log.chain.verified".to_owned(),
                severity: RuntimeEventSeverity::Info,
                source: "ham-core".to_owned(),
                source_plugin_id: None,
                workspace_id: Some("dashboard".to_owned()),
                payload_summary: "Official event hash chain verified".to_owned(),
                redacted_payload: None,
                error: None,
            });
        }
        Err(error) => {
            let _ = bridge.publish(RuntimeEventInput {
                event_type: "storage.error".to_owned(),
                severity: RuntimeEventSeverity::Error,
                source: "ham-core".to_owned(),
                source_plugin_id: None,
                workspace_id: Some("dashboard".to_owned()),
                payload_summary: "Official event hash chain verification failed".to_owned(),
                redacted_payload: None,
                error: Some(error.to_string()),
            });
        }
    }
    if proposal_runtime
        .block_on(store.rebuild_projections(logbook_id))
        .is_ok()
    {
        let _ = bridge.publish(RuntimeEventInput {
            event_type: "projection.qso.rebuilt".to_owned(),
            severity: RuntimeEventSeverity::Info,
            source: "ham-core".to_owned(),
            source_plugin_id: None,
            workspace_id: Some("dashboard".to_owned()),
            payload_summary: "QSO projection rebuilt from official events".to_owned(),
            redacted_payload: None,
            error: None,
        });
    }

    start_demo_runtime_publisher(bridge.clone());

    let state = Arc::new(AppState {
        bridge,
        store,
        logbook_id,
        proposal_runtime,
        sync: Mutex::new(SyncUiState::new(bound_addr.clone())),
    });

    println!("ham-gui listening on http://{bound_addr}");

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => handle_client(state.clone(), stream),
            Err(error) => eprintln!("failed to accept request: {error}"),
        }
    }
}

struct AppState {
    bridge: GuiRuntimeBridge,
    store: Arc<JsonlLogbookEventStore>,
    logbook_id: uuid::Uuid,
    proposal_runtime: tokio::runtime::Runtime,
    sync: Mutex<SyncUiState>,
}

#[derive(Debug)]
struct SyncUiState {
    config: SyncConfig,
    identity: LocalPeerIdentity,
    registry: PeerRegistry,
    discovery_running: bool,
    latest_handshake: Option<ham_sync::HandshakeResponse>,
    warning_count: u64,
}

impl SyncUiState {
    fn new(bound_addr: String) -> Self {
        let port = bound_addr
            .rsplit_once(':')
            .and_then(|(_, port)| port.parse::<u16>().ok());
        Self {
            config: SyncConfig::default(),
            identity: LocalPeerIdentity::new("KE8YGW Logger Local", port),
            registry: PeerRegistry::default(),
            discovery_running: false,
            latest_handshake: None,
            warning_count: 0,
        }
    }
}

fn handle_client(state: Arc<AppState>, mut stream: TcpStream) {
    let request = {
        let mut reader = BufReader::new(&mut stream);
        if let Ok(request) = read_http_request(&mut reader) {
            request
        } else {
            return;
        }
    };

    let target = request.target.as_str();
    let (path, query) = split_target(target);

    let response = match (request.method.as_str(), path) {
        ("GET", "/") | ("GET", "/index.html") => {
            response(200, "text/html; charset=utf-8", INDEX_HTML.as_bytes())
        }
        ("GET", "/styles.css") => response(200, "text/css; charset=utf-8", APP_CSS.as_bytes()),
        ("GET", "/app.js") => response(200, "text/javascript; charset=utf-8", APP_JS.as_bytes()),
        ("GET", "/api/shell") => json_response(&ApiShellPayload {
            shell: GuiShellState::default_shell(),
            commands: CommandRegistry::default_registry(),
            plugins: mock_plugins(),
            runtime_events: state.bridge.replay(RuntimeEventFilter::default(), 100),
            runtime_status: state.bridge.status(),
            known_core_capabilities: capability_labels(),
        }),
        ("GET", "/api/runtime-events") => {
            let params = parse_query(query);
            let filter = runtime_filter_from_query(&params);
            json_response(&ApiRuntimeEventsPayload {
                runtime_events: state.bridge.replay(filter, 250),
                runtime_status: state.bridge.status(),
            })
        }
        ("GET", "/api/runtime-events/export") => {
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
        ("GET", "/api/qsos") => json_response(&qso_projection_payload(&state, query)),
        ("GET", "/api/sync/state") => json_response(&sync_state_payload(&state)),
        ("POST", "/api/sync/discovery/start") => handle_sync_discovery(&state, true),
        ("POST", "/api/sync/discovery/stop") => handle_sync_discovery(&state, false),
        ("POST", "/api/sync/peers/refresh") => handle_sync_refresh(&state),
        ("POST", "/api/sync/handshake") => handle_sync_handshake(&state, &request.body),
        ("GET", "/api/log/verify") => handle_verify_chain(&state),
        ("POST", "/api/projections/rebuild") => handle_rebuild_projections(&state),
        ("POST", "/api/adif/import") => handle_adif_import(&state, &request.body),
        ("POST", "/api/adif/export") => handle_adif_export(&state, &request.body),
        ("POST", "/api/qso/create") => handle_qso_create(&state, &request.body),
        ("POST", "/api/qso/delete") => {
            handle_qso_simple_action(&state, &request.body, PROPOSAL_QSO_DELETE)
        }
        ("POST", "/api/qso/restore") => {
            handle_qso_simple_action(&state, &request.body, PROPOSAL_QSO_RESTORE)
        }
        ("POST", "/api/qso/note") => handle_qso_note(&state, &request.body),
        _ => response(404, "text/plain; charset=utf-8", b"not found"),
    };

    if let Err(error) = stream.write_all(&response) {
        eprintln!("failed to write response: {error}");
    }
}

#[derive(Debug)]
struct HttpRequest {
    method: String,
    target: String,
    body: Vec<u8>,
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

#[derive(Debug, Serialize)]
struct ApiQsoPayload {
    qsos: Vec<ApiQsoRecord>,
}

#[derive(Debug, Serialize)]
struct ApiQsoRecord {
    qso_id: uuid::Uuid,
    payload: Value,
    note_history: Vec<Value>,
    deleted: bool,
    last_event_hash: String,
}

#[derive(Debug, Deserialize)]
struct CreateQsoRequest {
    contacted_callsign: String,
    mode: String,
    frequency_hz: Option<u64>,
    band: Option<String>,
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QsoIdRequest {
    qso_id: uuid::Uuid,
}

#[derive(Debug, Deserialize)]
struct NoteQsoRequest {
    qso_id: uuid::Uuid,
    note: String,
}

#[derive(Debug, Deserialize)]
struct PathRequest {
    path: String,
    include_deleted: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct HandshakePeerRequest {
    peer_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct SyncStatePayload {
    config: SyncConfig,
    identity: LocalPeerIdentity,
    discovery_running: bool,
    peers: Vec<PeerRecord>,
    latest_handshake: Option<ham_sync::HandshakeResponse>,
    warning_count: u64,
}

fn json_response<T: Serialize>(payload: &T) -> Vec<u8> {
    let body = serde_json::to_vec(payload).expect("serializing GUI shell payload should not fail");
    response(200, "application/json; charset=utf-8", &body)
}

fn json_error(status: u16, message: impl Into<String>) -> Vec<u8> {
    json_response_with_status(status, &json!({ "error": message.into() }))
}

fn json_response_with_status<T: Serialize>(status: u16, payload: &T) -> Vec<u8> {
    let body = serde_json::to_vec(payload).expect("serializing GUI payload should not fail");
    response(status, "application/json; charset=utf-8", &body)
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
        400 => "Bad Request",
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

fn proposal_context() -> ProposalContext {
    ProposalContext {
        plugin_manifest: PluginManifest {
            plugin_id: "core.gui".to_owned(),
            name: "Core GUI".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            capabilities: vec![
                PluginCapability::QsoCreate,
                PluginCapability::QsoCorrect,
                PluginCapability::QsoDelete,
                PluginCapability::QsoRestore,
                PluginCapability::QsoNoteAdd,
                PluginCapability::QsoViewDeleted,
            ],
        },
        operator_role: OperatorRole::Admin,
    }
}

fn qso_projection_payload(state: &AppState, query: &str) -> ApiQsoPayload {
    let include_deleted = parse_query(query)
        .get("include_deleted")
        .is_some_and(|value| value == "true");
    let events = state
        .proposal_runtime
        .block_on(state.store.list_events(state.logbook_id))
        .unwrap_or_default();
    let mut projection = ham_core::QsoCurrentStateProjection::new();
    let _ = projection.rebuild(&events);
    let qsos = projection
        .list(include_deleted)
        .into_iter()
        .map(|record| ApiQsoRecord {
            qso_id: record.qso_id,
            payload: record.payload.clone(),
            note_history: record.note_history.clone(),
            deleted: record.deleted,
            last_event_hash: record.last_event_hash.clone(),
        })
        .collect();

    ApiQsoPayload { qsos }
}

fn handle_qso_create(state: &AppState, body: &[u8]) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<CreateQsoRequest>(body) else {
        return json_error(400, "invalid QSO create JSON");
    };

    let mut payload = json!({
        "station_callsign": "KE8YGW",
        "operator_callsign": "KE8YGW",
        "contacted_callsign": request.contacted_callsign.trim().to_ascii_uppercase(),
        "started_at": chrono::Utc::now().to_rfc3339(),
        "mode": request.mode.trim().to_ascii_uppercase(),
        "source": "manual"
    });
    if let Some(frequency_hz) = request.frequency_hz {
        payload["frequency_hz"] = json!(frequency_hz);
    }
    if let Some(band) = request.band.filter(|band| !band.trim().is_empty()) {
        payload["band"] = json!(band);
    }
    if let Some(notes) = request.notes.filter(|notes| !notes.trim().is_empty()) {
        payload["notes"] = json!(notes);
    }

    submit_gui_proposal(state, PROPOSAL_QSO_CREATE, None, payload)
}

fn handle_qso_simple_action(state: &AppState, body: &[u8], proposal_type: &str) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<QsoIdRequest>(body) else {
        return json_error(400, "invalid QSO action JSON");
    };
    submit_gui_proposal(state, proposal_type, Some(request.qso_id), json!({}))
}

fn handle_qso_note(state: &AppState, body: &[u8]) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<NoteQsoRequest>(body) else {
        return json_error(400, "invalid QSO note JSON");
    };
    submit_gui_proposal(
        state,
        PROPOSAL_QSO_NOTE_ADD,
        Some(request.qso_id),
        json!({"note": request.note}),
    )
}

fn submit_gui_proposal(
    state: &AppState,
    proposal_type: &str,
    qso_id: Option<uuid::Uuid>,
    payload: Value,
) -> Vec<u8> {
    let proposal = ProposalEnvelope::new(
        proposal_type,
        state.logbook_id,
        qso_id,
        None,
        state.bridge.status().device_id,
        "core.gui",
        1,
        payload,
    );
    match state.proposal_runtime.block_on(submit_proposal(
        state.store.as_ref(),
        &state.bridge,
        &proposal_context(),
        proposal,
    )) {
        Ok(outcome) => json_response(&json!({
            "ok": true,
            "event": outcome.official_event,
            "projection": qso_projection_payload(state, "include_deleted=true")
        })),
        Err(error) => json_response_with_status(
            400,
            &json!({
                "ok": false,
                "error": error.to_string()
            }),
        ),
    }
}

fn handle_verify_chain(state: &AppState) -> Vec<u8> {
    match state
        .proposal_runtime
        .block_on(state.store.verify_chain(state.logbook_id))
    {
        Ok(()) => json_response(&json!({"ok": true})),
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_rebuild_projections(state: &AppState) -> Vec<u8> {
    match state
        .proposal_runtime
        .block_on(state.store.rebuild_projections(state.logbook_id))
    {
        Ok(projection) => {
            json_response(&json!({"ok": true, "qso_count": projection.list(false).len()}))
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_adif_import(state: &AppState, body: &[u8]) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<PathRequest>(body) else {
        return json_error(400, "invalid ADIF import JSON");
    };
    let input = match fs::read_to_string(&request.path) {
        Ok(input) => input,
        Err(error) => return json_error(400, format!("failed to read ADIF file: {error}")),
    };
    let _ = state.bridge.publish(RuntimeEventInput {
        event_type: "import.adif.started".to_owned(),
        severity: RuntimeEventSeverity::Info,
        source: "ham-gui".to_owned(),
        source_plugin_id: Some("core.gui".to_owned()),
        workspace_id: Some("casual-logger".to_owned()),
        payload_summary: format!("Importing ADIF from {}", request.path),
        redacted_payload: None,
        error: None,
    });
    let options =
        AdifImportOptions::mvp_default("KE8YGW", "core.gui", state.bridge.status().device_id);
    let summary = state.proposal_runtime.block_on(import_adif(
        state.store.as_ref(),
        &state.bridge,
        &proposal_context(),
        state.logbook_id,
        &input,
        &options,
    ));
    let _ = state.bridge.publish(RuntimeEventInput {
        event_type: "import.adif.completed".to_owned(),
        severity: RuntimeEventSeverity::Info,
        source: "ham-gui".to_owned(),
        source_plugin_id: Some("core.gui".to_owned()),
        workspace_id: Some("casual-logger".to_owned()),
        payload_summary: format!("Imported {} ADIF records", summary.imported_count),
        redacted_payload: Some(json!(&summary)),
        error: None,
    });
    json_response(&summary)
}

fn handle_adif_export(state: &AppState, body: &[u8]) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<PathRequest>(body) else {
        return json_error(400, "invalid ADIF export JSON");
    };
    let include_deleted = request.include_deleted.unwrap_or(false);
    let _ = state.bridge.publish(RuntimeEventInput {
        event_type: "export.adif.started".to_owned(),
        severity: RuntimeEventSeverity::Info,
        source: "ham-gui".to_owned(),
        source_plugin_id: Some("core.gui".to_owned()),
        workspace_id: Some("casual-logger".to_owned()),
        payload_summary: format!("Exporting ADIF to {}", request.path),
        redacted_payload: None,
        error: None,
    });
    let projection = match state
        .proposal_runtime
        .block_on(state.store.rebuild_projections(state.logbook_id))
    {
        Ok(projection) => projection,
        Err(error) => return json_error(400, error.to_string()),
    };
    let adif = export_adif(&projection, include_deleted);
    if let Err(error) = fs::write(&request.path, adif) {
        return json_error(400, format!("failed to write ADIF file: {error}"));
    }
    let _ = state.bridge.publish(RuntimeEventInput {
        event_type: "export.adif.completed".to_owned(),
        severity: RuntimeEventSeverity::Info,
        source: "ham-gui".to_owned(),
        source_plugin_id: Some("core.gui".to_owned()),
        workspace_id: Some("casual-logger".to_owned()),
        payload_summary: format!("ADIF exported to {}", request.path),
        redacted_payload: None,
        error: None,
    });
    json_response(&json!({"ok": true, "path": request.path}))
}

fn sync_state_payload(state: &AppState) -> SyncStatePayload {
    let sync = state
        .sync
        .lock()
        .expect("sync state mutex should not be poisoned");
    SyncStatePayload {
        config: sync.config.clone(),
        identity: sync.identity.clone(),
        discovery_running: sync.discovery_running,
        peers: sync.registry.list(),
        latest_handshake: sync.latest_handshake.clone(),
        warning_count: sync.warning_count,
    }
}

fn handle_sync_discovery(state: &AppState, running: bool) -> Vec<u8> {
    {
        let mut sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        sync.discovery_running = running;
    }
    let event_type = if running {
        "network.discovery.started"
    } else {
        "network.discovery.stopped"
    };
    let _ = state.bridge.publish(RuntimeEventInput {
        event_type: event_type.to_owned(),
        severity: RuntimeEventSeverity::Info,
        source: "ham-sync".to_owned(),
        source_plugin_id: None,
        workspace_id: Some("dashboard".to_owned()),
        payload_summary: event_type.to_owned(),
        redacted_payload: Some(json!({"transport": "ipv4/ipv6 multicast", "mvp": true})),
        error: None,
    });
    json_response(&sync_state_payload(state))
}

fn handle_sync_refresh(state: &AppState) -> Vec<u8> {
    let mut sync = state
        .sync
        .lock()
        .expect("sync state mutex should not be poisoned");
    let mut peer = LocalPeerIdentity::new("Demo LAN Peer", Some(sync.config.local_sync_port));
    peer.device_id = uuid::Uuid::parse_str("00000000-0000-4000-8000-0000000000aa").unwrap();
    let packet = DiscoveryPacket::from_identity(&peer);
    let identity = sync.identity.clone();
    let observation = sync
        .registry
        .observe(&identity, packet, "127.0.0.1:9738".parse().unwrap());
    drop(sync);
    let event_type = match observation {
        PeerObservation::Discovered(_) => "network.peer.discovered",
        PeerObservation::Updated(_) => "network.peer.updated",
        PeerObservation::IgnoredSelf => "network.peer.ignored_self",
        PeerObservation::IgnoredIncompatible => "network.peer.ignored_incompatible",
    };
    let _ = state.bridge.publish(RuntimeEventInput {
        event_type: event_type.to_owned(),
        severity: RuntimeEventSeverity::Info,
        source: "ham-sync".to_owned(),
        source_plugin_id: None,
        workspace_id: Some("dashboard".to_owned()),
        payload_summary: "Peer registry refreshed".to_owned(),
        redacted_payload: None,
        error: None,
    });
    json_response(&sync_state_payload(state))
}

fn handle_sync_handshake(state: &AppState, body: &[u8]) -> Vec<u8> {
    let request = serde_json::from_slice::<HandshakePeerRequest>(body)
        .unwrap_or(HandshakePeerRequest { peer_id: None });
    let local_head = logbook_head_summary(state);
    let mut sync = state
        .sync
        .lock()
        .expect("sync state mutex should not be poisoned");
    let Some(peer) = request
        .peer_id
        .as_ref()
        .and_then(|peer_id| {
            sync.registry
                .list()
                .into_iter()
                .find(|peer| &peer.peer_id == peer_id)
        })
        .or_else(|| sync.registry.list().into_iter().next())
    else {
        sync.warning_count += 1;
        drop(sync);
        let _ = state.bridge.publish(RuntimeEventInput {
            event_type: "sync.handshake.error".to_owned(),
            severity: RuntimeEventSeverity::Warn,
            source: "ham-sync".to_owned(),
            source_plugin_id: None,
            workspace_id: Some("dashboard".to_owned()),
            payload_summary: "No peer selected for handshake".to_owned(),
            redacted_payload: None,
            error: Some("no discovered peers".to_owned()),
        });
        return json_response_with_status(
            400,
            &json!({"ok": false, "error": "no discovered peers"}),
        );
    };

    let remote_request = HandshakeRequest {
        protocol_version: PROTOCOL_VERSION,
        device_id: peer.device_id,
        session_id: peer.session_id,
        supported_capabilities: peer.capabilities.clone(),
        logbooks: vec![LogbookHeadSummary {
            logbook_id: state.logbook_id,
            head_hash: None,
            event_count: Some(0),
        }],
    };
    let response = build_handshake_response(&sync.identity, &[local_head], &remote_request);
    sync.latest_handshake = Some(response.clone());
    drop(sync);
    let status = response
        .matching_logbooks
        .first()
        .map(|comparison| format!("{:?}", comparison.status))
        .unwrap_or_else(|| "Unknown".to_owned());
    let _ = state.bridge.publish(RuntimeEventInput {
        event_type: "sync.handshake.accepted".to_owned(),
        severity: RuntimeEventSeverity::Info,
        source: "ham-sync".to_owned(),
        source_plugin_id: None,
        workspace_id: Some("dashboard".to_owned()),
        payload_summary: format!("Handshake completed; head status {status}"),
        redacted_payload: Some(json!(&response)),
        error: None,
    });
    json_response(&json!({"ok": true, "handshake": response}))
}

fn logbook_head_summary(state: &AppState) -> LogbookHeadSummary {
    let head_hash = state
        .proposal_runtime
        .block_on(state.store.get_head(state.logbook_id))
        .unwrap_or(None);
    let event_count = state
        .proposal_runtime
        .block_on(state.store.list_events(state.logbook_id))
        .map(|events| events.len() as u64)
        .ok();
    LogbookHeadSummary {
        logbook_id: state.logbook_id,
        head_hash,
        event_count,
    }
}

fn default_logbook_id() -> uuid::Uuid {
    uuid::Uuid::parse_str("00000000-0000-4000-8000-000000000001")
        .expect("default logbook id is a valid UUID")
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
