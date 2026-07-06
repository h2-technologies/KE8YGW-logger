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
    default_official_event_log_path, export_adif, export_adif_with_activations, import_adif,
    lookup_callsign_with_cache, publish_rig_runtime_event, submit_proposal,
    suggestion_from_rig_state, AdifImportOptions, CoreEventEnvelope, JsonlLogbookEventStore,
    LocalPrefixProvider, LogbookEventStore, LookupCache, LookupCacheConfig, LookupProviderStatus,
    MockRigProvider, NewLogbookEvent, OperatorRole, Projection, ProposalContext,
    RigConnectionStatus, RigDevice, RigProvider, RigProviderStatus, RigState, RuntimeEventFilter,
    RuntimeEventSeverity, RuntimeLogConfig,
};
use ham_gui::{
    mock::{capability_labels, mock_plugins},
    CommandRegistry, GuiRuntimeBridge, GuiShellState, RuntimeBridgeStatus, RuntimeEventInput,
};
use ham_plugin_sdk::{
    PluginCapability, PluginManifest, ProposalEnvelope, PROPOSAL_ACTIVATION_END,
    PROPOSAL_ACTIVATION_START, PROPOSAL_QSO_ACTIVATION_LINK, PROPOSAL_QSO_CREATE,
    PROPOSAL_QSO_DELETE, PROPOSAL_QSO_NOTE_ADD, PROPOSAL_QSO_RESTORE,
};
use ham_sync::{
    build_handshake_response, metadata_for_event, preview_pull_from_events, pull_missing_events,
    CloudAuth, CloudConnectionState, CloudPreviewPullRequest, CloudPullEventsRequest,
    CloudPullEventsResponse, CloudPushEventsRequest, CloudPushEventsResponse, CloudServerConfig,
    CloudSyncConfig, CloudSyncStatusResponse, DiscoveryPacket, GetEventMetadataResponse,
    GetEventRangeResponse, HandshakeRequest, InMemoryCloudSyncServer, ListLogbooksResponse,
    LocalPeerIdentity, LogbookHeadSummary, PairDeviceRequest, PeerObservation, PeerRecord,
    PeerRegistry, PreviewPullRequest, PreviewPullResponse, PullEventsRequest, PullEventsResponse,
    ReplicationStatus, SyncConfig, PROTOCOL_VERSION,
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
        cloud_server: InMemoryCloudSyncServer::new(CloudServerConfig::default()),
        lookup_cache: LookupCache::new(),
        lookup_config: Mutex::new(LookupUiConfig::default()),
        rig_provider: MockRigProvider::default(),
        rig_config: Mutex::new(RigUiConfig::default()),
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
    cloud_server: InMemoryCloudSyncServer,
    lookup_cache: LookupCache,
    lookup_config: Mutex<LookupUiConfig>,
    rig_provider: MockRigProvider,
    rig_config: Mutex<RigUiConfig>,
}

#[derive(Debug, Clone, Serialize)]
struct LookupUiConfig {
    enable_lookup: bool,
    enable_online_lookup: bool,
    preferred_provider: String,
    cache_ttl_days: i64,
    offline_prefix_fallback_enabled: bool,
}

impl Default for LookupUiConfig {
    fn default() -> Self {
        Self {
            enable_lookup: true,
            enable_online_lookup: false,
            preferred_provider: "local-prefix".to_owned(),
            cache_ttl_days: 30,
            offline_prefix_fallback_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
struct RigUiConfig {
    enable_rig_control: bool,
    default_provider: String,
    default_rig_id: Option<uuid::Uuid>,
    polling_interval_ms: u64,
    auto_fill_from_rig: bool,
    hamlib_endpoint: String,
    serial_settings_placeholder: String,
}

impl Default for RigUiConfig {
    fn default() -> Self {
        Self {
            enable_rig_control: true,
            default_provider: "mock".to_owned(),
            default_rig_id: None,
            polling_interval_ms: 1_000,
            auto_fill_from_rig: true,
            hamlib_endpoint: "127.0.0.1:4532".to_owned(),
            serial_settings_placeholder: "Serial CAT settings are planned".to_owned(),
        }
    }
}

#[derive(Debug)]
struct SyncUiState {
    config: SyncConfig,
    identity: LocalPeerIdentity,
    registry: PeerRegistry,
    discovery_running: bool,
    latest_handshake: Option<ham_sync::HandshakeResponse>,
    latest_preview: Option<PreviewPullResponse>,
    latest_pull: Option<PullEventsResponse>,
    last_sync_time: Option<String>,
    demo_remote_events: Vec<CoreEventEnvelope>,
    divergence: Option<String>,
    cloud_config: CloudSyncConfig,
    cloud_auth: Option<CloudAuth>,
    cloud_account_id: Option<String>,
    latest_cloud_status: Option<CloudSyncStatusResponse>,
    latest_cloud_preview: Option<PreviewPullResponse>,
    latest_cloud_pull: Option<CloudPullEventsResponse>,
    latest_cloud_push: Option<CloudPushEventsResponse>,
    last_cloud_push_time: Option<String>,
    last_cloud_pull_time: Option<String>,
    cloud_divergence: Option<String>,
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
            latest_preview: None,
            latest_pull: None,
            last_sync_time: None,
            demo_remote_events: Vec::new(),
            divergence: None,
            cloud_config: CloudSyncConfig {
                sync_server_url: "http://127.0.0.1:9740".to_owned(),
                device_name: "KE8YGW Logger Local".to_owned(),
                ..CloudSyncConfig::default()
            },
            cloud_auth: None,
            cloud_account_id: None,
            latest_cloud_status: None,
            latest_cloud_preview: None,
            latest_cloud_pull: None,
            latest_cloud_push: None,
            last_cloud_push_time: None,
            last_cloud_pull_time: None,
            cloud_divergence: None,
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
        ("GET", "/api/activations") => json_response(&activation_projection_payload(&state)),
        ("GET", "/api/lookup/callsign") => handle_lookup_callsign(&state, query),
        ("POST", "/api/lookup/cache/clear") => handle_lookup_cache_clear(&state),
        ("GET", "/api/lookup/status") => handle_lookup_status(&state),
        ("GET", "/api/rig/status") => handle_rig_status(&state),
        ("POST", "/api/rig/connect") => handle_rig_connect(&state),
        ("POST", "/api/rig/disconnect") => handle_rig_disconnect(&state),
        ("POST", "/api/rig/refresh") => handle_rig_refresh(&state),
        ("POST", "/api/rig/mock/set") => handle_rig_mock_set(&state, &request.body),
        ("GET", "/api/sync/state") => json_response(&sync_state_payload(&state)),
        ("GET", "/api/sync/list-logbooks") => json_response(&ListLogbooksResponse {
            logbooks: vec![logbook_head_summary(&state)],
        }),
        ("GET", "/api/sync/get-head") => json_response(&logbook_head_summary(&state)),
        ("GET", "/api/sync/events-since") => handle_sync_events_since(&state, query),
        ("GET", "/api/sync/event-metadata") => handle_sync_event_metadata(&state, query),
        ("POST", "/api/sync/discovery/start") => handle_sync_discovery(&state, true),
        ("POST", "/api/sync/discovery/stop") => handle_sync_discovery(&state, false),
        ("POST", "/api/sync/peers/refresh") => handle_sync_refresh(&state),
        ("POST", "/api/sync/handshake") => handle_sync_handshake(&state, &request.body),
        ("POST", "/api/sync/preview-pull") => handle_sync_preview_pull(&state, &request.body),
        ("POST", "/api/sync/pull-events") => handle_sync_pull_events(&state, &request.body),
        ("POST", "/api/sync/cloud/connect") => handle_cloud_connect(&state, &request.body),
        ("POST", "/api/sync/cloud/push") => handle_cloud_push(&state),
        ("POST", "/api/sync/cloud/preview-pull") => handle_cloud_preview_pull(&state),
        ("POST", "/api/sync/cloud/pull") => handle_cloud_pull(&state),
        ("GET", "/api/log/verify") => handle_verify_chain(&state),
        ("POST", "/api/projections/rebuild") => handle_rebuild_projections(&state),
        ("POST", "/api/adif/import") => handle_adif_import(&state, &request.body),
        ("POST", "/api/adif/export") => handle_adif_export(&state, &request.body),
        ("POST", "/api/activation/start") => handle_activation_start(&state, &request.body),
        ("POST", "/api/activation/end") => handle_activation_end(&state, &request.body),
        ("POST", "/api/activation/export-adif") => {
            handle_activation_adif_export(&state, &request.body)
        }
        ("POST", "/api/qso/portable-create") => handle_portable_qso_create(&state, &request.body),
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

#[derive(Debug, Serialize)]
struct ApiActivationPayload {
    activations: Vec<ApiActivationRecord>,
    active_activation: Option<ApiActivationRecord>,
}

#[derive(Debug, Clone, Serialize)]
struct ApiActivationRecord {
    activation_id: uuid::Uuid,
    payload: Value,
    status: String,
    qso_count: usize,
    unique_callsign_count: usize,
    band_summary: HashMap<String, usize>,
    mode_summary: HashMap<String, usize>,
    note_history: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct CreateQsoRequest {
    contacted_callsign: String,
    mode: String,
    frequency_hz: Option<u64>,
    band: Option<String>,
    notes: Option<String>,
    name: Option<String>,
    qth: Option<String>,
    grid: Option<String>,
    country: Option<String>,
    dxcc: Option<u16>,
    cq_zone: Option<u8>,
    itu_zone: Option<u8>,
    lookup_source: Option<String>,
    lookup_confidence: Option<f32>,
    enriched_fields: Option<Vec<String>>,
    submode: Option<String>,
    rig_source: Option<String>,
    rig_id: Option<uuid::Uuid>,
    rig_enriched_fields: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct RigSetRequest {
    frequency_hz: Option<u64>,
    mode: Option<String>,
    ptt: Option<bool>,
}

#[derive(Debug, Serialize)]
struct RigStatusPayload {
    config: RigUiConfig,
    devices: Vec<RigDevice>,
    active_state: Option<RigState>,
    provider_status: RigProviderStatus,
    connected_count: usize,
    autofill_suggestion: Option<ham_core::RigAutofillSuggestion>,
}

#[derive(Debug, Deserialize)]
struct StartActivationRequest {
    activation_type: String,
    reference: String,
    station_callsign: String,
    operator_callsign: String,
    grid: Option<String>,
    location_name: Option<String>,
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EndActivationRequest {
    activation_id: uuid::Uuid,
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

#[derive(Debug, Deserialize)]
struct CloudConnectRequest {
    server_url: Option<String>,
    device_name: Option<String>,
    pairing_code: Option<String>,
    account_id: Option<String>,
    user_id: Option<String>,
    enable_cloud_sync: Option<bool>,
    prefer_lan_sync: Option<bool>,
    auto_push_enabled: Option<bool>,
    auto_pull_enabled: Option<bool>,
    sync_interval_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
struct SyncStatePayload {
    config: SyncConfig,
    identity: LocalPeerIdentity,
    discovery_running: bool,
    peers: Vec<PeerRecord>,
    latest_handshake: Option<ham_sync::HandshakeResponse>,
    latest_preview: Option<PreviewPullResponse>,
    latest_pull: Option<PullEventsResponse>,
    last_sync_time: Option<String>,
    local_head: LogbookHeadSummary,
    remote_head: Option<LogbookHeadSummary>,
    divergence: Option<String>,
    cloud_config: CloudSyncConfig,
    cloud_connection_state: CloudConnectionState,
    cloud_account_id: Option<String>,
    cloud_status: Option<CloudSyncStatusResponse>,
    latest_cloud_preview: Option<PreviewPullResponse>,
    latest_cloud_pull: Option<CloudPullEventsResponse>,
    latest_cloud_push: Option<CloudPushEventsResponse>,
    last_cloud_push_time: Option<String>,
    last_cloud_pull_time: Option<String>,
    cloud_divergence: Option<String>,
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

fn pota_sota_context() -> ProposalContext {
    ProposalContext {
        plugin_manifest: PluginManifest {
            plugin_id: "plugin.pota-sota".to_owned(),
            name: "POTA/SOTA Tools".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            capabilities: vec![
                PluginCapability::ActivationCreate,
                PluginCapability::ActivationUpdate,
                PluginCapability::ActivationEnd,
                PluginCapability::ActivationView,
                PluginCapability::QsoCreate,
                PluginCapability::QsoCorrect,
                PluginCapability::QsoNoteAdd,
                PluginCapability::AdifExport,
            ],
        },
        operator_role: OperatorRole::Admin,
    }
}

fn lookup_provider_status(state: &AppState) -> LookupProviderStatus {
    let config = state
        .lookup_config
        .lock()
        .expect("lookup config mutex should not be poisoned")
        .clone();
    LookupProviderStatus {
        provider_id: config.preferred_provider,
        healthy: config.enable_lookup,
        message: if config.enable_online_lookup {
            "Online lookup is configured as a future stub; offline fallback is active".to_owned()
        } else {
            "Offline prefix resolver active".to_owned()
        },
        rate_limited: false,
    }
}

fn handle_lookup_status(state: &AppState) -> Vec<u8> {
    let config = state
        .lookup_config
        .lock()
        .expect("lookup config mutex should not be poisoned")
        .clone();
    json_response(&json!({
        "config": config,
        "providers": [lookup_provider_status(state)]
    }))
}

fn handle_lookup_callsign(state: &AppState, query: &str) -> Vec<u8> {
    let params = parse_query(query);
    let Some(callsign) = params
        .get("callsign")
        .filter(|value| !value.trim().is_empty())
    else {
        return json_error(400, "missing callsign");
    };
    let config = state
        .lookup_config
        .lock()
        .expect("lookup config mutex should not be poisoned")
        .clone();
    if !config.enable_lookup {
        return json_response_with_status(400, &json!({"ok": false, "error": "lookup disabled"}));
    }
    let provider = LocalPrefixProvider;
    let result = state.proposal_runtime.block_on(lookup_callsign_with_cache(
        &provider,
        &state.lookup_cache,
        &LookupCacheConfig {
            ttl_days: config.cache_ttl_days,
        },
        &state.bridge,
        callsign,
        state.bridge.status().device_id,
    ));
    match result {
        Ok(suggestion) => json_response(&json!({
            "ok": true,
            "suggestion": suggestion,
            "provider_status": lookup_provider_status(state)
        })),
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_lookup_cache_clear(state: &AppState) -> Vec<u8> {
    match state
        .proposal_runtime
        .block_on(ham_core::clear_lookup_cache(
            &state.lookup_cache,
            &state.bridge,
            state.bridge.status().device_id,
        )) {
        Ok(()) => json_response(&json!({"ok": true})),
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_rig_status(state: &AppState) -> Vec<u8> {
    json_response(&rig_status_payload(state))
}

fn handle_rig_connect(state: &AppState) -> Vec<u8> {
    let devices = state
        .proposal_runtime
        .block_on(state.rig_provider.list_supported_rigs());
    let Some(rig_id) = devices.first().map(|device| device.rig_id) else {
        return json_error(400, "no rig providers available");
    };
    let _ = state.proposal_runtime.block_on(publish_rig_runtime_event(
        &state.bridge,
        state.bridge.status().device_id,
        "rig.connect.started",
        RuntimeEventSeverity::Info,
        "Connecting mock rig",
        Some(json!({"rig_id": rig_id, "provider": "mock"})),
        None,
    ));
    match state
        .proposal_runtime
        .block_on(state.rig_provider.connect(rig_id))
    {
        Ok(device) => {
            let _ = state.proposal_runtime.block_on(publish_rig_runtime_event(
                &state.bridge,
                state.bridge.status().device_id,
                "rig.provider.loaded",
                RuntimeEventSeverity::Info,
                "Mock rig provider loaded",
                Some(json!({"provider": "mock"})),
                None,
            ));
            let _ = state.proposal_runtime.block_on(publish_rig_runtime_event(
                &state.bridge,
                state.bridge.status().device_id,
                "rig.connect.succeeded",
                RuntimeEventSeverity::Info,
                "Mock rig connected",
                Some(json!(&device)),
                None,
            ));
            json_response(&json!({"ok": true, "rig": device, "status": rig_status_payload(state)}))
        }
        Err(error) => {
            let _ = state.proposal_runtime.block_on(publish_rig_runtime_event(
                &state.bridge,
                state.bridge.status().device_id,
                "rig.connect.failed",
                RuntimeEventSeverity::Error,
                "Mock rig connection failed",
                Some(json!({"rig_id": rig_id})),
                Some(error.to_string()),
            ));
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_rig_disconnect(state: &AppState) -> Vec<u8> {
    let devices = state
        .proposal_runtime
        .block_on(state.rig_provider.list_supported_rigs());
    let Some(rig_id) = devices.first().map(|device| device.rig_id) else {
        return json_error(400, "no rig providers available");
    };
    match state
        .proposal_runtime
        .block_on(state.rig_provider.disconnect(rig_id))
    {
        Ok(device) => {
            let _ = state.proposal_runtime.block_on(publish_rig_runtime_event(
                &state.bridge,
                state.bridge.status().device_id,
                "rig.disconnected",
                RuntimeEventSeverity::Info,
                "Mock rig disconnected",
                Some(json!(&device)),
                None,
            ));
            json_response(&json!({"ok": true, "rig": device, "status": rig_status_payload(state)}))
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_rig_refresh(state: &AppState) -> Vec<u8> {
    let devices = state
        .proposal_runtime
        .block_on(state.rig_provider.list_supported_rigs());
    let Some(rig_id) = devices.first().map(|device| device.rig_id) else {
        return json_error(400, "no rig providers available");
    };
    match state
        .proposal_runtime
        .block_on(state.rig_provider.get_state(rig_id))
    {
        Ok(rig_state) => {
            let _ = state.proposal_runtime.block_on(publish_rig_runtime_event(
                &state.bridge,
                state.bridge.status().device_id,
                "rig.state.changed",
                RuntimeEventSeverity::Info,
                "Rig state refreshed",
                Some(json!(&rig_state)),
                None,
            ));
            let _ = state.proposal_runtime.block_on(publish_rig_runtime_event(
                &state.bridge,
                state.bridge.status().device_id,
                "rig.autofill.suggestion.created",
                RuntimeEventSeverity::Debug,
                "Rig autofill suggestion created",
                Some(json!(suggestion_from_rig_state(&rig_state))),
                None,
            ));
            json_response(
                &json!({"ok": true, "state": rig_state, "status": rig_status_payload(state)}),
            )
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_rig_mock_set(state: &AppState, body: &[u8]) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<RigSetRequest>(body) else {
        return json_error(400, "invalid rig mock JSON");
    };
    let devices = state
        .proposal_runtime
        .block_on(state.rig_provider.list_supported_rigs());
    let Some(rig_id) = devices.first().map(|device| device.rig_id) else {
        return json_error(400, "no rig providers available");
    };
    let mut latest_state = None;
    if let Some(frequency_hz) = request.frequency_hz {
        match state
            .proposal_runtime
            .block_on(state.rig_provider.set_frequency(rig_id, frequency_hz))
        {
            Ok(rig_state) => {
                latest_state = Some(rig_state.clone());
                let _ = state.proposal_runtime.block_on(publish_rig_runtime_event(
                    &state.bridge,
                    state.bridge.status().device_id,
                    "rig.frequency.changed",
                    RuntimeEventSeverity::Info,
                    format!("Mock rig frequency set to {frequency_hz} Hz"),
                    Some(json!(&rig_state)),
                    None,
                ));
            }
            Err(error) => {
                return json_response_with_status(
                    400,
                    &json!({"ok": false, "error": error.to_string()}),
                )
            }
        }
    }
    if let Some(mode) = request
        .mode
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        match state
            .proposal_runtime
            .block_on(state.rig_provider.set_mode(rig_id, mode))
        {
            Ok(rig_state) => {
                latest_state = Some(rig_state.clone());
                let _ = state.proposal_runtime.block_on(publish_rig_runtime_event(
                    &state.bridge,
                    state.bridge.status().device_id,
                    "rig.mode.changed",
                    RuntimeEventSeverity::Info,
                    format!("Mock rig mode set to {}", mode.trim().to_ascii_uppercase()),
                    Some(json!(&rig_state)),
                    None,
                ));
            }
            Err(error) => {
                return json_response_with_status(
                    400,
                    &json!({"ok": false, "error": error.to_string()}),
                )
            }
        }
    }
    if let Some(ptt) = request.ptt {
        match state
            .proposal_runtime
            .block_on(state.rig_provider.set_ptt(rig_id, ptt))
        {
            Ok(rig_state) => {
                latest_state = Some(rig_state.clone());
                let _ = state.proposal_runtime.block_on(publish_rig_runtime_event(
                    &state.bridge,
                    state.bridge.status().device_id,
                    "rig.ptt.changed",
                    RuntimeEventSeverity::Info,
                    if ptt {
                        "Mock rig PTT on"
                    } else {
                        "Mock rig PTT off"
                    },
                    Some(json!(&rig_state)),
                    None,
                ));
            }
            Err(error) => {
                return json_response_with_status(
                    400,
                    &json!({"ok": false, "error": error.to_string()}),
                )
            }
        }
    }
    let _ = state.proposal_runtime.block_on(publish_rig_runtime_event(
        &state.bridge,
        state.bridge.status().device_id,
        "rig.command.sent",
        RuntimeEventSeverity::Debug,
        "Mock rig command applied",
        Some(json!({"rig_id": rig_id})),
        None,
    ));
    json_response(&json!({
        "ok": true,
        "state": latest_state,
        "status": rig_status_payload(state)
    }))
}

fn rig_status_payload(state: &AppState) -> RigStatusPayload {
    let config = state
        .rig_config
        .lock()
        .expect("rig config mutex should not be poisoned")
        .clone();
    let devices = state
        .proposal_runtime
        .block_on(state.rig_provider.list_supported_rigs());
    let active_state = devices
        .iter()
        .find(|device| device.connection_status == RigConnectionStatus::Connected)
        .and_then(|device| {
            state
                .proposal_runtime
                .block_on(state.rig_provider.get_state(device.rig_id))
                .ok()
        });
    let provider_status = state
        .proposal_runtime
        .block_on(state.rig_provider.provider_status());
    let connected_count = devices
        .iter()
        .filter(|device| device.connection_status == RigConnectionStatus::Connected)
        .count();
    let autofill_suggestion = active_state.as_ref().map(suggestion_from_rig_state);
    RigStatusPayload {
        config,
        devices,
        active_state,
        provider_status,
        connected_count,
        autofill_suggestion,
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

fn activation_projection_payload(state: &AppState) -> ApiActivationPayload {
    let projection = state
        .proposal_runtime
        .block_on(state.store.rebuild_activation_projections(state.logbook_id))
        .unwrap_or_else(|_| ham_core::ActivationProjection::new());
    let activations = projection
        .list(true)
        .into_iter()
        .map(api_activation_record)
        .collect::<Vec<_>>();
    let active_activation = projection
        .active_for_station_operator("KE8YGW", "KE8YGW")
        .map(api_activation_record);
    ApiActivationPayload {
        activations,
        active_activation,
    }
}

fn api_activation_record(record: &ham_core::ActivationRecord) -> ApiActivationRecord {
    ApiActivationRecord {
        activation_id: record.activation_id,
        payload: record.payload.clone(),
        status: record.status.clone(),
        qso_count: record.qso_count,
        unique_callsign_count: record.unique_callsign_count,
        band_summary: record.band_summary.clone(),
        mode_summary: record.mode_summary.clone(),
        note_history: record.note_history.clone(),
    }
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
    if let Some(band) = request
        .band
        .as_deref()
        .filter(|band| !band.trim().is_empty())
    {
        payload["band"] = json!(band);
    }
    if let Some(notes) = request
        .notes
        .as_deref()
        .filter(|notes| !notes.trim().is_empty())
    {
        payload["notes"] = json!(notes);
    }
    apply_accepted_lookup_fields(&mut payload, &request);
    apply_accepted_rig_fields(&mut payload, &request);

    submit_gui_proposal(state, PROPOSAL_QSO_CREATE, None, payload)
}

fn handle_portable_qso_create(state: &AppState, body: &[u8]) -> Vec<u8> {
    handle_qso_create_with_activation(state, body)
}

fn handle_qso_create_with_activation(state: &AppState, body: &[u8]) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<CreateQsoRequest>(body) else {
        return json_error(400, "invalid portable QSO create JSON");
    };
    let active = activation_projection_payload(state).active_activation;
    let mut payload = json!({
        "station_callsign": "KE8YGW",
        "operator_callsign": "KE8YGW",
        "contacted_callsign": request.contacted_callsign.trim().to_ascii_uppercase(),
        "started_at": chrono::Utc::now().to_rfc3339(),
        "mode": request.mode.trim().to_ascii_uppercase(),
        "source": "plugin/pota-sota"
    });
    if let Some(frequency_hz) = request.frequency_hz {
        payload["frequency_hz"] = json!(frequency_hz);
    }
    if let Some(band) = request
        .band
        .as_deref()
        .filter(|band| !band.trim().is_empty())
    {
        payload["band"] = json!(band);
    }
    if let Some(notes) = request
        .notes
        .as_deref()
        .filter(|notes| !notes.trim().is_empty())
    {
        payload["notes"] = json!(notes);
    }
    apply_accepted_lookup_fields(&mut payload, &request);
    apply_accepted_rig_fields(&mut payload, &request);
    if let Some(active) = active {
        payload["activation_id"] = json!(active.activation_id);
        if let Some(kind) = active
            .payload
            .get("activation_type")
            .and_then(Value::as_str)
        {
            payload["my_sig"] = json!(kind.to_ascii_uppercase());
        }
        if let Some(reference) = active
            .payload
            .get("park_id")
            .or_else(|| active.payload.get("summit_id"))
            .and_then(Value::as_str)
        {
            payload["my_sig_info"] = json!(reference);
        }
        if let Some(grid) = active.payload.get("grid").and_then(Value::as_str) {
            payload["grid"] = json!(grid);
        }
    }
    let proposal = ProposalEnvelope::new(
        PROPOSAL_QSO_CREATE,
        state.logbook_id,
        None,
        None,
        state.bridge.status().device_id,
        "plugin.pota-sota",
        1,
        payload,
    );
    let qso = match state.proposal_runtime.block_on(submit_proposal(
        state.store.as_ref(),
        &state.bridge,
        &pota_sota_context(),
        proposal,
    )) {
        Ok(outcome) => outcome,
        Err(error) => {
            return json_response_with_status(
                400,
                &json!({"ok": false, "error": error.to_string()}),
            )
        }
    };
    if let (Some(active), Some(qso_id)) = (
        activation_projection_payload(state).active_activation,
        qso.official_event.entity_id,
    ) {
        let link = ProposalEnvelope::new(
            PROPOSAL_QSO_ACTIVATION_LINK,
            state.logbook_id,
            Some(qso_id),
            None,
            state.bridge.status().device_id,
            "plugin.pota-sota",
            1,
            json!({"activation_id": active.activation_id}),
        );
        let _ = state.proposal_runtime.block_on(submit_proposal(
            state.store.as_ref(),
            &state.bridge,
            &pota_sota_context(),
            link,
        ));
    }
    json_response(
        &json!({"ok": true, "event": qso.official_event, "projection": qso_projection_payload(state, "include_deleted=true"), "activations": activation_projection_payload(state)}),
    )
}

fn apply_accepted_lookup_fields(payload: &mut Value, request: &CreateQsoRequest) {
    if let Some(name) = request
        .name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload["name"] = json!(name);
    }
    if let Some(qth) = request
        .qth
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload["qth"] = json!(qth);
    }
    if let Some(grid) = request
        .grid
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload["grid"] = json!(grid.to_ascii_uppercase());
    }
    if let Some(country) = request
        .country
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload["country"] = json!(country);
    }
    if let Some(dxcc) = request.dxcc {
        payload["dxcc"] = json!(dxcc);
    }
    if let Some(cq_zone) = request.cq_zone {
        payload["cq_zone"] = json!(cq_zone);
    }
    if let Some(itu_zone) = request.itu_zone {
        payload["itu_zone"] = json!(itu_zone);
    }
    if let Some(source) = request
        .lookup_source
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload["lookup_source"] = json!(source);
    }
    if let Some(confidence) = request.lookup_confidence {
        payload["lookup_confidence"] = json!(confidence);
    }
    if let Some(fields) = &request.enriched_fields {
        payload["enriched_fields"] = json!(fields);
    }
}

fn apply_accepted_rig_fields(payload: &mut Value, request: &CreateQsoRequest) {
    if let Some(submode) = request
        .submode
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload["submode"] = json!(submode.trim().to_ascii_uppercase());
    }
    if let Some(source) = request
        .rig_source
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload["rig_source"] = json!(source);
    }
    if let Some(rig_id) = request.rig_id {
        payload["rig_id"] = json!(rig_id);
    }
    if let Some(fields) = &request.rig_enriched_fields {
        payload["rig_enriched_fields"] = json!(fields);
    }
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

fn handle_activation_start(state: &AppState, body: &[u8]) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<StartActivationRequest>(body) else {
        return json_error(400, "invalid activation start JSON");
    };
    let kind = request.activation_type.trim().to_ascii_lowercase();
    let mut payload = json!({
        "activation_type": kind,
        "station_callsign": request.station_callsign.trim().to_ascii_uppercase(),
        "operator_callsign": request.operator_callsign.trim().to_ascii_uppercase(),
        "started_at": chrono::Utc::now().to_rfc3339(),
        "status": "active"
    });
    if kind == "pota" {
        payload["park_id"] = json!(request.reference.trim().to_ascii_uppercase());
    } else if kind == "sota" {
        payload["summit_id"] = json!(request.reference.trim().to_ascii_uppercase());
    } else {
        payload["reference"] = json!(request.reference.trim());
    }
    if let Some(grid) = request.grid.filter(|value| !value.trim().is_empty()) {
        payload["grid"] = json!(grid.trim().to_ascii_uppercase());
    }
    if let Some(location) = request
        .location_name
        .filter(|value| !value.trim().is_empty())
    {
        payload["location_name"] = json!(location);
    }
    if let Some(notes) = request.notes.filter(|value| !value.trim().is_empty()) {
        payload["notes"] = json!(notes);
    }
    let proposal = ProposalEnvelope::new(
        PROPOSAL_ACTIVATION_START,
        state.logbook_id,
        None,
        None,
        state.bridge.status().device_id,
        "plugin.pota-sota",
        1,
        payload,
    );
    match state.proposal_runtime.block_on(submit_proposal(
        state.store.as_ref(),
        &state.bridge,
        &pota_sota_context(),
        proposal,
    )) {
        Ok(outcome) => {
            let _ = publish_gui_runtime(
                state,
                "activation.started",
                RuntimeEventSeverity::Info,
                "Portable activation started",
                Some(json!(&outcome.official_event)),
                None,
            );
            json_response(
                &json!({"ok": true, "event": outcome.official_event, "activations": activation_projection_payload(state)}),
            )
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_activation_end(state: &AppState, body: &[u8]) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<EndActivationRequest>(body) else {
        return json_error(400, "invalid activation end JSON");
    };
    let started_at = state
        .proposal_runtime
        .block_on(state.store.rebuild_activation_projections(state.logbook_id))
        .ok()
        .and_then(|projection| projection.get(request.activation_id).cloned())
        .and_then(|record| {
            record
                .payload
                .get("started_at")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let proposal = ProposalEnvelope::new(
        PROPOSAL_ACTIVATION_END,
        state.logbook_id,
        Some(request.activation_id),
        None,
        state.bridge.status().device_id,
        "plugin.pota-sota",
        1,
        json!({"started_at": started_at, "ended_at": chrono::Utc::now().to_rfc3339()}),
    );
    match state.proposal_runtime.block_on(submit_proposal(
        state.store.as_ref(),
        &state.bridge,
        &pota_sota_context(),
        proposal,
    )) {
        Ok(outcome) => {
            let _ = publish_gui_runtime(
                state,
                "activation.ended",
                RuntimeEventSeverity::Info,
                "Portable activation ended",
                Some(json!(&outcome.official_event)),
                None,
            );
            json_response(
                &json!({"ok": true, "event": outcome.official_event, "activations": activation_projection_payload(state)}),
            )
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_activation_adif_export(state: &AppState, body: &[u8]) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<PathRequest>(body) else {
        return json_error(400, "invalid activation ADIF export JSON");
    };
    let _ = publish_gui_runtime(
        state,
        "export.activation.adif.started",
        RuntimeEventSeverity::Info,
        "Exporting activation ADIF",
        None,
        None,
    );
    let qsos = match state
        .proposal_runtime
        .block_on(state.store.rebuild_projections(state.logbook_id))
    {
        Ok(projection) => projection,
        Err(error) => return json_error(400, error.to_string()),
    };
    let activations = match state
        .proposal_runtime
        .block_on(state.store.rebuild_activation_projections(state.logbook_id))
    {
        Ok(projection) => projection,
        Err(error) => return json_error(400, error.to_string()),
    };
    let adif = export_adif_with_activations(&qsos, Some(&activations), false);
    if let Err(error) = fs::write(&request.path, adif) {
        return json_error(
            400,
            format!("failed to write activation ADIF file: {error}"),
        );
    }
    let _ = publish_gui_runtime(
        state,
        "export.activation.adif.completed",
        RuntimeEventSeverity::Info,
        "Activation ADIF exported",
        Some(json!({"path": request.path})),
        None,
    );
    json_response(&json!({"ok": true, "path": request.path}))
}

fn sync_state_payload(state: &AppState) -> SyncStatePayload {
    let sync = state
        .sync
        .lock()
        .expect("sync state mutex should not be poisoned");
    let local_head = logbook_head_summary(state);
    let remote_head = sync
        .demo_remote_events
        .last()
        .map(|event| LogbookHeadSummary {
            logbook_id: event.logbook_id,
            head_hash: Some(event.event_hash.clone()),
            event_count: Some(sync.demo_remote_events.len() as u64),
        });
    SyncStatePayload {
        config: sync.config.clone(),
        identity: sync.identity.clone(),
        discovery_running: sync.discovery_running,
        peers: sync.registry.list(),
        latest_handshake: sync.latest_handshake.clone(),
        latest_preview: sync.latest_preview.clone(),
        latest_pull: sync.latest_pull.clone(),
        last_sync_time: sync.last_sync_time.clone(),
        local_head,
        remote_head,
        divergence: sync.divergence.clone(),
        cloud_config: sync.cloud_config.clone(),
        cloud_connection_state: if sync.cloud_auth.is_some() {
            CloudConnectionState::Connected
        } else {
            CloudConnectionState::Disconnected
        },
        cloud_account_id: sync.cloud_account_id.clone(),
        cloud_status: sync.latest_cloud_status.clone(),
        latest_cloud_preview: sync.latest_cloud_preview.clone(),
        latest_cloud_pull: sync.latest_cloud_pull.clone(),
        latest_cloud_push: sync.latest_cloud_push.clone(),
        last_cloud_push_time: sync.last_cloud_push_time.clone(),
        last_cloud_pull_time: sync.last_cloud_pull_time.clone(),
        cloud_divergence: sync.cloud_divergence.clone(),
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
    let demo_remote_events = build_demo_remote_events(state);
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
    sync.demo_remote_events = demo_remote_events;
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

fn handle_sync_events_since(state: &AppState, query: &str) -> Vec<u8> {
    let params = parse_query(query);
    let logbook_id = params
        .get("logbook_id")
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .unwrap_or(state.logbook_id);
    let after_hash = params
        .get("after_hash")
        .filter(|value| !value.is_empty())
        .cloned();
    match state
        .proposal_runtime
        .block_on(state.store.list_events_after(logbook_id, after_hash))
    {
        Ok(events) => json_response(&GetEventRangeResponse { logbook_id, events }),
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_sync_event_metadata(state: &AppState, query: &str) -> Vec<u8> {
    let params = parse_query(query);
    let logbook_id = params
        .get("logbook_id")
        .and_then(|value| uuid::Uuid::parse_str(value).ok())
        .unwrap_or(state.logbook_id);
    let after_hash = params
        .get("after_hash")
        .filter(|value| !value.is_empty())
        .cloned();
    match state
        .proposal_runtime
        .block_on(state.store.list_events_after(logbook_id, after_hash))
    {
        Ok(events) => json_response(&GetEventMetadataResponse {
            logbook_id,
            events: events.iter().map(metadata_for_event).collect(),
        }),
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
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

fn handle_sync_preview_pull(state: &AppState, body: &[u8]) -> Vec<u8> {
    let request = serde_json::from_slice::<HandshakePeerRequest>(body)
        .unwrap_or(HandshakePeerRequest { peer_id: None });
    let Some(peer_id) = selected_peer_id(state, request.peer_id) else {
        return sync_no_peer_error(state, "sync.preview_pull.failed");
    };

    let _ = publish_gui_runtime(
        state,
        "sync.preview_pull.started",
        RuntimeEventSeverity::Info,
        "Previewing pull from selected peer",
        None,
        None,
    );

    let local_head = state
        .proposal_runtime
        .block_on(state.store.get_head(state.logbook_id))
        .unwrap_or(None);
    let remote_events = {
        let sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        sync.demo_remote_events.clone()
    };
    let preview = preview_pull_from_events(
        PreviewPullRequest {
            peer_id,
            logbook_id: state.logbook_id,
            local_head_hash: local_head,
        },
        &remote_events,
    );
    {
        let mut sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        if preview.status == ReplicationStatus::Diverged {
            sync.divergence = Some(preview.message.clone());
            sync.warning_count += 1;
        }
        sync.latest_preview = Some(preview.clone());
    }
    let event_type = if preview.status == ReplicationStatus::Diverged {
        "sync.divergence.detected"
    } else {
        "sync.preview_pull.completed"
    };
    let _ = publish_gui_runtime(
        state,
        event_type,
        if preview.status == ReplicationStatus::Diverged {
            RuntimeEventSeverity::Warn
        } else {
            RuntimeEventSeverity::Info
        },
        &preview.message,
        Some(json!(&preview)),
        None,
    );
    json_response(&json!({"ok": true, "preview": preview}))
}

fn handle_sync_pull_events(state: &AppState, body: &[u8]) -> Vec<u8> {
    let request = serde_json::from_slice::<HandshakePeerRequest>(body)
        .unwrap_or(HandshakePeerRequest { peer_id: None });
    let Some(peer_id) = selected_peer_id(state, request.peer_id) else {
        return sync_no_peer_error(state, "sync.pull.failed");
    };

    let _ = publish_gui_runtime(
        state,
        "sync.pull.started",
        RuntimeEventSeverity::Info,
        "Pulling missing official events from selected peer",
        None,
        None,
    );
    let local_head = state
        .proposal_runtime
        .block_on(state.store.get_head(state.logbook_id))
        .unwrap_or(None);
    let remote_events = {
        let sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        sync.demo_remote_events.clone()
    };
    for event in &remote_events {
        let _ = publish_gui_runtime(
            state,
            "sync.remote_event.received",
            RuntimeEventSeverity::Debug,
            &format!("Received remote event metadata {}", event.event_hash),
            Some(json!(metadata_for_event(event))),
            None,
        );
    }
    let pull = state.proposal_runtime.block_on(pull_missing_events(
        state.store.as_ref(),
        PullEventsRequest {
            peer_id,
            logbook_id: state.logbook_id,
            local_head_hash: local_head,
        },
        remote_events,
    ));
    let severity = if matches!(
        pull.status,
        ReplicationStatus::Pulled | ReplicationStatus::InSync
    ) {
        RuntimeEventSeverity::Info
    } else {
        RuntimeEventSeverity::Warn
    };
    let event_type = match pull.status {
        ReplicationStatus::Pulled | ReplicationStatus::InSync => "sync.pull.completed",
        ReplicationStatus::Diverged => "sync.divergence.detected",
        ReplicationStatus::Rejected | ReplicationStatus::RemoteAhead => "sync.pull.failed",
    };
    if pull.accepted_count > 0 {
        let _ = publish_gui_runtime(
            state,
            "sync.remote_event.accepted",
            RuntimeEventSeverity::Info,
            &format!("Accepted {} remote official events", pull.accepted_count),
            Some(json!(&pull)),
            None,
        );
        let _ = publish_gui_runtime(
            state,
            "sync.pull.progress",
            RuntimeEventSeverity::Info,
            &format!("Pulled {} official events", pull.accepted_count),
            Some(json!(&pull)),
            None,
        );
    }
    if pull.rejected_count > 0 {
        let _ = publish_gui_runtime(
            state,
            "sync.remote_event.rejected",
            RuntimeEventSeverity::Warn,
            "One or more remote official events were rejected",
            Some(json!(&pull)),
            pull.errors.first().cloned(),
        );
    }
    if matches!(
        pull.status,
        ReplicationStatus::Pulled | ReplicationStatus::InSync
    ) {
        let _ = state
            .proposal_runtime
            .block_on(state.store.verify_chain(state.logbook_id));
        let projection = state
            .proposal_runtime
            .block_on(state.store.rebuild_projections(state.logbook_id));
        let qso_count = projection
            .as_ref()
            .map(|projection| projection.list(false).len())
            .unwrap_or_default();
        let _ = publish_gui_runtime(
            state,
            "projection.qso.rebuilt",
            RuntimeEventSeverity::Info,
            &format!("QSO projection rebuilt after sync ({qso_count} visible QSOs)"),
            None,
            projection.err().map(|error| error.to_string()),
        );
    }
    {
        let mut sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        sync.latest_pull = Some(pull.clone());
        sync.last_sync_time = Some(chrono::Utc::now().to_rfc3339());
        if pull.status == ReplicationStatus::Diverged {
            sync.divergence = pull.errors.first().cloned();
            sync.warning_count += 1;
        }
    }
    let _ = publish_gui_runtime(
        state,
        event_type,
        severity,
        &format!("Pull finished with status {:?}", pull.status),
        Some(json!(&pull)),
        pull.errors.first().cloned(),
    );
    json_response(&json!({"ok": pull.errors.is_empty(), "pull": pull}))
}

fn handle_cloud_connect(state: &AppState, body: &[u8]) -> Vec<u8> {
    let request =
        serde_json::from_slice::<CloudConnectRequest>(body).unwrap_or(CloudConnectRequest {
            server_url: None,
            device_name: None,
            pairing_code: None,
            account_id: None,
            user_id: None,
            enable_cloud_sync: None,
            prefer_lan_sync: None,
            auto_push_enabled: None,
            auto_pull_enabled: None,
            sync_interval_seconds: None,
        });
    let _ = publish_cloud_runtime(
        state,
        "sync.cloud.connect.started",
        RuntimeEventSeverity::Info,
        "Connecting cloud sync",
        None,
        None,
    );
    let (device_id, device_name, pairing_code, account_id, user_id) = {
        let mut sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        if let Some(server_url) = request.server_url.filter(|value| !value.trim().is_empty()) {
            sync.cloud_config.sync_server_url = server_url;
        }
        if let Some(device_name) = request.device_name.filter(|value| !value.trim().is_empty()) {
            sync.cloud_config.device_name = device_name;
        }
        if let Some(enabled) = request.enable_cloud_sync {
            sync.cloud_config.enable_cloud_sync = enabled;
        }
        if let Some(prefer_lan) = request.prefer_lan_sync {
            sync.cloud_config.prefer_lan_sync = prefer_lan;
        }
        if let Some(auto_push) = request.auto_push_enabled {
            sync.cloud_config.auto_push_enabled = auto_push;
        }
        if let Some(auto_pull) = request.auto_pull_enabled {
            sync.cloud_config.auto_pull_enabled = auto_pull;
        }
        if let Some(interval) = request.sync_interval_seconds {
            sync.cloud_config.sync_interval_seconds = interval.max(30);
        }
        (
            sync.identity.device_id,
            sync.cloud_config.device_name.clone(),
            request
                .pairing_code
                .unwrap_or_else(|| "local-dev-pairing-code".to_owned()),
            request
                .account_id
                .unwrap_or_else(|| "local-account".to_owned()),
            request.user_id.unwrap_or_else(|| "local-user".to_owned()),
        )
    };
    let pair = PairDeviceRequest {
        pairing_code,
        account_id,
        user_id,
        device_id,
        device_name,
        requested_logbooks: vec![state.logbook_id],
        role_hints: vec!["admin".to_owned(), "log.sync".to_owned()],
    };
    let response = state
        .proposal_runtime
        .block_on(state.cloud_server.pair_device(pair));
    if let Some(session) = response.session.clone() {
        let auth = CloudAuth {
            sync_token: session.sync_token.clone(),
        };
        let status = state
            .proposal_runtime
            .block_on(state.cloud_server.status(Some(&auth)))
            .ok();
        {
            let mut sync = state
                .sync
                .lock()
                .expect("sync state mutex should not be poisoned");
            sync.cloud_config.enable_cloud_sync = true;
            sync.cloud_auth = Some(auth);
            sync.cloud_account_id = Some(session.account_id.clone());
            sync.latest_cloud_status = status;
        }
        let _ = publish_cloud_runtime(
            state,
            "sync.cloud.connect.succeeded",
            RuntimeEventSeverity::Info,
            "Cloud sync connected",
            Some(
                json!({"device_id": session.device_id, "authorized_logbooks": session.authorized_logbooks}),
            ),
            None,
        );
        json_response(&json!({"ok": true, "session": session}))
    } else {
        {
            let mut sync = state
                .sync
                .lock()
                .expect("sync state mutex should not be poisoned");
            sync.warning_count += 1;
        }
        let error = response
            .reason
            .unwrap_or_else(|| "pairing rejected".to_owned());
        let _ = publish_cloud_runtime(
            state,
            "sync.cloud.connect.failed",
            RuntimeEventSeverity::Warn,
            "Cloud sync pairing failed",
            None,
            Some(error.clone()),
        );
        json_response_with_status(400, &json!({"ok": false, "error": error}))
    }
}

fn handle_cloud_push(state: &AppState) -> Vec<u8> {
    let Some(auth) = cloud_auth(state) else {
        return cloud_auth_error(state, "sync.cloud.push.failed");
    };
    let _ = publish_cloud_runtime(
        state,
        "sync.cloud.push.started",
        RuntimeEventSeverity::Info,
        "Pushing local official events to cloud",
        None,
        None,
    );
    let events = state
        .proposal_runtime
        .block_on(state.store.list_events(state.logbook_id))
        .unwrap_or_default();
    let response = state
        .proposal_runtime
        .block_on(state.cloud_server.push_events(CloudPushEventsRequest {
            auth,
            logbook_id: state.logbook_id,
            events,
        }));
    match response {
        Ok(push) => {
            let event_type = if matches!(
                push.status,
                ReplicationStatus::Pulled | ReplicationStatus::InSync
            ) {
                "sync.cloud.push.completed"
            } else if push.status == ReplicationStatus::Diverged {
                "sync.cloud.divergence.detected"
            } else {
                "sync.cloud.push.failed"
            };
            {
                let mut sync = state
                    .sync
                    .lock()
                    .expect("sync state mutex should not be poisoned");
                sync.latest_cloud_push = Some(push.clone());
                sync.last_cloud_push_time = Some(chrono::Utc::now().to_rfc3339());
                if push.status == ReplicationStatus::Diverged {
                    sync.cloud_divergence = push.errors.first().cloned();
                    sync.warning_count += 1;
                }
            }
            let _ = publish_cloud_runtime(
                state,
                "sync.cloud.push.progress",
                RuntimeEventSeverity::Info,
                &format!("Cloud accepted {} events", push.accepted_count),
                Some(json!(&push)),
                None,
            );
            let _ = publish_cloud_runtime(
                state,
                event_type,
                if push.errors.is_empty() {
                    RuntimeEventSeverity::Info
                } else {
                    RuntimeEventSeverity::Warn
                },
                &format!("Cloud push finished with {:?}", push.status),
                Some(json!(&push)),
                push.errors.first().cloned(),
            );
            json_response(&json!({"ok": push.errors.is_empty(), "push": push}))
        }
        Err(error) => {
            let _ = publish_cloud_runtime(
                state,
                "sync.cloud.push.failed",
                RuntimeEventSeverity::Warn,
                "Cloud push failed",
                None,
                Some(error.to_string()),
            );
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_cloud_preview_pull(state: &AppState) -> Vec<u8> {
    let Some(auth) = cloud_auth(state) else {
        return cloud_auth_error(state, "sync.cloud.preview_pull.failed");
    };
    let local_head = state
        .proposal_runtime
        .block_on(state.store.get_head(state.logbook_id))
        .unwrap_or(None);
    let _ = publish_cloud_runtime(
        state,
        "sync.cloud.preview_pull.started",
        RuntimeEventSeverity::Info,
        "Previewing pull from cloud",
        None,
        None,
    );
    let response = state
        .proposal_runtime
        .block_on(state.cloud_server.preview_pull(CloudPreviewPullRequest {
            auth,
            logbook_id: state.logbook_id,
            local_head_hash: local_head,
        }));
    match response {
        Ok(preview) => {
            {
                let mut sync = state
                    .sync
                    .lock()
                    .expect("sync state mutex should not be poisoned");
                if preview.status == ReplicationStatus::Diverged {
                    sync.cloud_divergence = Some(preview.message.clone());
                    sync.warning_count += 1;
                }
                sync.latest_cloud_preview = Some(preview.clone());
            }
            let event_type = if preview.status == ReplicationStatus::Diverged {
                "sync.cloud.divergence.detected"
            } else {
                "sync.cloud.preview_pull.completed"
            };
            let _ = publish_cloud_runtime(
                state,
                event_type,
                if preview.status == ReplicationStatus::Diverged {
                    RuntimeEventSeverity::Warn
                } else {
                    RuntimeEventSeverity::Info
                },
                &preview.message,
                Some(json!(&preview)),
                None,
            );
            json_response(&json!({"ok": true, "preview": preview}))
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_cloud_pull(state: &AppState) -> Vec<u8> {
    let Some(auth) = cloud_auth(state) else {
        return cloud_auth_error(state, "sync.cloud.pull.failed");
    };
    let local_head = state
        .proposal_runtime
        .block_on(state.store.get_head(state.logbook_id))
        .unwrap_or(None);
    let _ = publish_cloud_runtime(
        state,
        "sync.cloud.pull.started",
        RuntimeEventSeverity::Info,
        "Pulling missing official events from cloud",
        None,
        None,
    );
    let response = state
        .proposal_runtime
        .block_on(state.cloud_server.pull_events(CloudPullEventsRequest {
            auth,
            logbook_id: state.logbook_id,
            local_head_hash: local_head.clone(),
        }));
    match response {
        Ok(cloud_pull) => {
            let pull = state.proposal_runtime.block_on(pull_missing_events(
                state.store.as_ref(),
                PullEventsRequest {
                    peer_id: "cloud".to_owned(),
                    logbook_id: state.logbook_id,
                    local_head_hash: local_head,
                },
                cloud_pull.events.clone(),
            ));
            if matches!(
                pull.status,
                ReplicationStatus::Pulled | ReplicationStatus::InSync
            ) {
                let _ = state
                    .proposal_runtime
                    .block_on(state.store.verify_chain(state.logbook_id));
                let _ = state
                    .proposal_runtime
                    .block_on(state.store.rebuild_projections(state.logbook_id));
            }
            {
                let mut sync = state
                    .sync
                    .lock()
                    .expect("sync state mutex should not be poisoned");
                sync.latest_cloud_preview = Some(cloud_pull.preview.clone());
                sync.latest_cloud_pull = Some(cloud_pull.clone());
                sync.last_cloud_pull_time = Some(chrono::Utc::now().to_rfc3339());
                if pull.status == ReplicationStatus::Diverged {
                    sync.cloud_divergence = pull.errors.first().cloned();
                    sync.warning_count += 1;
                }
            }
            let event_type = if matches!(
                pull.status,
                ReplicationStatus::Pulled | ReplicationStatus::InSync
            ) {
                "sync.cloud.pull.completed"
            } else if pull.status == ReplicationStatus::Diverged {
                "sync.cloud.divergence.detected"
            } else {
                "sync.cloud.pull.failed"
            };
            let _ = publish_cloud_runtime(
                state,
                event_type,
                if pull.errors.is_empty() {
                    RuntimeEventSeverity::Info
                } else {
                    RuntimeEventSeverity::Warn
                },
                &format!("Cloud pull finished with {:?}", pull.status),
                Some(json!({"server": cloud_pull, "local": pull})),
                pull.errors.first().cloned(),
            );
            json_response(
                &json!({"ok": pull.errors.is_empty(), "server_pull": cloud_pull, "local_pull": pull}),
            )
        }
        Err(error) => {
            let _ = publish_cloud_runtime(
                state,
                "sync.cloud.pull.failed",
                RuntimeEventSeverity::Warn,
                "Cloud pull failed",
                None,
                Some(error.to_string()),
            );
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
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

fn selected_peer_id(state: &AppState, requested: Option<String>) -> Option<String> {
    let sync = state
        .sync
        .lock()
        .expect("sync state mutex should not be poisoned");
    requested.or_else(|| {
        sync.registry
            .list()
            .into_iter()
            .next()
            .map(|peer| peer.peer_id)
    })
}

fn sync_no_peer_error(state: &AppState, event_type: &str) -> Vec<u8> {
    {
        let mut sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        sync.warning_count += 1;
    }
    let _ = publish_gui_runtime(
        state,
        event_type,
        RuntimeEventSeverity::Warn,
        "No peer selected",
        None,
        Some("no discovered peers".to_owned()),
    );
    json_response_with_status(400, &json!({"ok": false, "error": "no discovered peers"}))
}

fn cloud_auth(state: &AppState) -> Option<CloudAuth> {
    state
        .sync
        .lock()
        .expect("sync state mutex should not be poisoned")
        .cloud_auth
        .clone()
}

fn cloud_auth_error(state: &AppState, event_type: &str) -> Vec<u8> {
    {
        let mut sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        sync.warning_count += 1;
    }
    let _ = publish_cloud_runtime(
        state,
        event_type,
        RuntimeEventSeverity::Warn,
        "Cloud sync is not connected",
        None,
        Some("cloud sync is not connected".to_owned()),
    );
    json_response_with_status(
        401,
        &json!({"ok": false, "error": "cloud sync is not connected"}),
    )
}

fn build_demo_remote_events(state: &AppState) -> Vec<CoreEventEnvelope> {
    let mut events = state
        .proposal_runtime
        .block_on(state.store.list_events(state.logbook_id))
        .unwrap_or_default();
    let previous_hash = events.last().map(|event| event.event_hash.clone());
    events.push(CoreEventEnvelope::from_new(
        NewLogbookEvent {
            event_type: ham_plugin_sdk::OFFICIAL_LOG_QSO_CREATED.to_owned(),
            logbook_id: state.logbook_id,
            entity_id: Some(uuid::Uuid::new_v4()),
            author_operator_id: None,
            station_callsign: "KE8YGW".to_owned(),
            operator_callsign: Some("KE8YGW".to_owned()),
            author_device_id: uuid::Uuid::parse_str("00000000-0000-4000-8000-0000000000aa")
                .expect("demo device id is valid"),
            source_device_id: uuid::Uuid::parse_str("00000000-0000-4000-8000-0000000000aa")
                .expect("demo device id is valid"),
            correlation_id: uuid::Uuid::new_v4(),
            source_plugin_id: Some("sync.demo.peer".to_owned()),
            schema_version: 1,
            payload: json!({
                "qso_id": uuid::Uuid::new_v4(),
                "station_callsign": "KE8YGW",
                "operator_callsign": "KE8YGW",
                "contacted_callsign": "N0SYNC",
                "started_at": chrono::Utc::now().to_rfc3339(),
                "mode": "SSB",
                "band": "20m",
                "source": "sync-demo"
            }),
        },
        previous_hash,
    ));
    events
}

fn publish_gui_runtime(
    state: &AppState,
    event_type: &str,
    severity: RuntimeEventSeverity,
    summary: &str,
    redacted_payload: Option<Value>,
    error: Option<String>,
) -> std::io::Result<ham_core::RuntimeEventEnvelope> {
    state.bridge.publish(RuntimeEventInput {
        event_type: event_type.to_owned(),
        severity,
        source: "ham-sync".to_owned(),
        source_plugin_id: None,
        workspace_id: Some("dashboard".to_owned()),
        payload_summary: summary.to_owned(),
        redacted_payload,
        error,
    })
}

fn publish_cloud_runtime(
    state: &AppState,
    event_type: &str,
    severity: RuntimeEventSeverity,
    summary: &str,
    redacted_payload: Option<Value>,
    error: Option<String>,
) -> std::io::Result<ham_core::RuntimeEventEnvelope> {
    state.bridge.publish(RuntimeEventInput {
        event_type: event_type.to_owned(),
        severity,
        source: "ham-sync-cloud".to_owned(),
        source_plugin_id: None,
        workspace_id: Some("dashboard".to_owned()),
        payload_summary: summary.to_owned(),
        redacted_payload,
        error,
    })
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
