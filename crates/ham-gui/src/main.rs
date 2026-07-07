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
    build_diagnostic_bundle, confirmations_from_adif, default_official_event_log_path,
    default_service_registry, dx_cluster_spot_to_spot, export_adif, export_adif_with_activations,
    export_diagnostic_zip, export_net_report_markdown, grayline_snapshot, import_adif,
    lookup_callsign_with_service_framework, maidenhead_to_coordinate, missing_credential_status,
    mock_propagation_forecast, mock_weather, online_services_dashboard, parse_dx_cluster_line,
    pota_spot_to_spot, publish_rig_runtime_event, qso_map_objects, station_markers_from_profiles,
    submit_proposal, suggestion_from_rig_state, AdifImportOptions, Coordinate, CoreEventEnvelope,
    CredentialMetadata, CredentialStore, DiagnosticBundleInput, DiagnosticReportType,
    EquipmentItem, EquipmentType, InsecureDevCredentialStore, JsonPermissionGrantStore,
    JsonStationBookStore, JsonSupportStore, JsonlLogbookEventStore, LocalPrefixProvider,
    LogbookEventStore, LookupCache, LookupCacheConfig, LookupProviderStatus, MapLayerStack,
    MockRigProvider, NetControlProjection, NewLogbookEvent, NotificationSeverity,
    OnlineAutomationTask, OnlineNotification, OnlineProviderStatus, OperatorRole,
    PermissionGrantSet, PermissionGrantStatus, PermissionRegistry, PermissionSettings,
    PotaSpotRecord, Projection, ProposalContext, RigConnectionStatus, RigDevice, RigProvider,
    RigProviderStatus, RigState, RuntimeEventFilter, RuntimeEventSeverity, RuntimeLogConfig,
    ServiceCache, ServiceCacheEntry, ServiceRegistry, ServiceRegistrySnapshot, StationBook,
    StationConfiguration, StationProfile, UnsupportedOsCredentialStore, UploadQueue, UploadTarget,
};
use ham_gui::{
    mock::{capability_labels, mock_plugins},
    CommandRegistry, GuiRuntimeBridge, GuiShellState, RuntimeBridgeStatus, RuntimeEventInput,
};
use ham_plugin_sdk::{
    PluginCapability, PluginManifest, ProposalEnvelope, ServiceType, PROPOSAL_ACTIVATION_END,
    PROPOSAL_ACTIVATION_START, PROPOSAL_NET_CHECKIN_CREATE, PROPOSAL_NET_CHECKIN_DELETE,
    PROPOSAL_NET_REPORT_EXPORT, PROPOSAL_NET_SESSION_END, PROPOSAL_NET_SESSION_START,
    PROPOSAL_NET_TRAFFIC_CREATE, PROPOSAL_QSO_ACTIVATION_LINK, PROPOSAL_QSO_CREATE,
    PROPOSAL_QSO_DELETE, PROPOSAL_QSO_NOTE_ADD, PROPOSAL_QSO_RESTORE,
};
use ham_sync::{
    build_handshake_response, metadata_for_event, preview_pull_from_events, pull_missing_events,
    CloudAuth, CloudConnectionState, CloudPreviewPullRequest, CloudPullEventsRequest,
    CloudPullEventsResponse, CloudPushEventsRequest, CloudPushEventsResponse, CloudServerConfig,
    CloudSyncConfig, CloudSyncStatusResponse, DiagnosticReportUploadRequest,
    DiagnosticReportUploadResponse, DiagnosticReportUploadType, DiscoveryPacket,
    GetEventMetadataResponse, GetEventRangeResponse, HandshakeRequest, InMemoryCloudSyncServer,
    ListLogbooksResponse, LocalPeerIdentity, LogbookHeadSummary, PairDeviceRequest,
    PeerObservation, PeerRecord, PeerRegistry, PreviewPullRequest, PreviewPullResponse,
    PullEventsRequest, PullEventsResponse, ReplicationStatus, SyncConfig, PROTOCOL_VERSION,
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
    let support_dir = RuntimeLogConfig::default_for_app()
        .directory
        .join("support");
    let permission_store =
        JsonPermissionGrantStore::new(support_dir.join("plugin-permissions.json"));
    let station_store = JsonStationBookStore::new(support_dir.join("station-book.json"));
    let service_registry_store =
        JsonSupportStore::<ServiceRegistry>::new(support_dir.join("service-registry.json"));
    let service_cache_store =
        JsonSupportStore::<Vec<ServiceCacheEntry>>::new(support_dir.join("service-cache.json"));
    let map_layer_store =
        JsonSupportStore::<MapLayerStack>::new(support_dir.join("map-layers.json"));
    let upload_queue_store =
        JsonSupportStore::<UploadQueue>::new(support_dir.join("upload-queue.json"));
    let lookup_config_store =
        JsonSupportStore::<LookupUiConfig>::new(support_dir.join("lookup-config.json"));
    let rig_config_store =
        JsonSupportStore::<RigUiConfig>::new(support_dir.join("rig-config.json"));
    let online_support_store =
        JsonSupportStore::<OnlineSupportState>::new(support_dir.join("online-support.json"));
    let mut station_book = station_store.load().unwrap_or_default();
    if station_book.profiles.is_empty() {
        seed_default_station_book(&mut station_book);
        let _ = station_store.save(&station_book);
    }
    let credential_store: Box<dyn CredentialStore> =
        if env::var("HAM_PLATFORM_ALLOW_INSECURE_DEV_CREDENTIALS")
            .ok()
            .as_deref()
            == Some("1")
        {
            match InsecureDevCredentialStore::open(support_dir.join("dev-credentials.json"), true) {
                Ok(store) => Box::new(store),
                Err(_) => Box::new(UnsupportedOsCredentialStore),
            }
        } else {
            Box::new(UnsupportedOsCredentialStore)
        };
    let permission_registry = PermissionRegistry::mvp_default();
    let permission_settings = PermissionSettings::default();
    let mut permission_grants = permission_store.load().unwrap_or_default();
    let manifests = plugin_manifests();
    ham_core::grant_builtin_defaults(
        &manifests,
        &permission_registry,
        &permission_settings,
        &mut permission_grants,
    );
    let _ = permission_store.save(&permission_grants);
    for manifest in &manifests {
        let validation = permission_registry.validate_manifest(manifest);
        let _ = bridge.publish(RuntimeEventInput {
            event_type: if validation.is_ok() {
                "plugin.manifest.loaded".to_owned()
            } else {
                "plugin.manifest.invalid".to_owned()
            },
            severity: if validation.is_ok() {
                RuntimeEventSeverity::Info
            } else {
                RuntimeEventSeverity::Warn
            },
            source: "ham-gui".to_owned(),
            source_plugin_id: Some(manifest.plugin_id.clone()),
            workspace_id: Some("dashboard".to_owned()),
            payload_summary: format!("Plugin manifest loaded: {}", manifest.name),
            redacted_payload: Some(json!({
                "plugin_id": manifest.plugin_id,
                "requested_permissions": manifest
                    .requested_or_capabilities()
                    .iter()
                    .map(|permission| permission.as_str())
                    .collect::<Vec<_>>()
            })),
            error: validation.err().map(|error| error.to_string()),
        });
        for permission in manifest.requested_or_capabilities() {
            let _ = bridge.publish(RuntimeEventInput {
                event_type: "plugin.permission.requested".to_owned(),
                severity: RuntimeEventSeverity::Debug,
                source: "ham-gui".to_owned(),
                source_plugin_id: Some(manifest.plugin_id.clone()),
                workspace_id: Some("dashboard".to_owned()),
                payload_summary: format!("{} requested {}", manifest.name, permission.as_str()),
                redacted_payload: Some(json!({
                    "plugin_id": manifest.plugin_id,
                    "permission_id": permission.as_str()
                })),
                error: None,
            });
        }
    }
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

    publish_support_storage_event(
        &bridge,
        "support.storage.opened",
        RuntimeEventSeverity::Info,
        format!("Support storage opened at {}", support_dir.display()),
        None,
    );

    let service_registry = load_support_or(
        &bridge,
        &service_registry_store,
        default_service_registry(),
        "service registry",
    );
    let service_cache = ServiceCache::new();
    let service_cache_entries = load_support_or(
        &bridge,
        &service_cache_store,
        Vec::<ServiceCacheEntry>::new(),
        "service cache",
    );
    proposal_runtime.block_on(service_cache.replace_entries(service_cache_entries));
    let map_layers = load_support_or(
        &bridge,
        &map_layer_store,
        MapLayerStack::default_layers(),
        "map layers",
    );
    let upload_queue = {
        let loaded = load_support_or(
            &bridge,
            &upload_queue_store,
            default_upload_queue(),
            "upload queue",
        );
        if loaded.targets.is_empty() {
            default_upload_queue()
        } else {
            loaded
        }
    };
    let lookup_config = load_support_or(
        &bridge,
        &lookup_config_store,
        LookupUiConfig::default(),
        "lookup config",
    );
    let rig_config = load_support_or(
        &bridge,
        &rig_config_store,
        RigUiConfig::default(),
        "rig config",
    );
    let online_support = load_support_or(
        &bridge,
        &online_support_store,
        OnlineSupportState::default(),
        "online support",
    );

    let state = Arc::new(AppState {
        bridge,
        store,
        logbook_id,
        proposal_runtime,
        sync: Mutex::new(SyncUiState::new(bound_addr.clone())),
        cloud_server: InMemoryCloudSyncServer::new(CloudServerConfig::default()),
        lookup_cache: LookupCache::new(),
        lookup_config: Mutex::new(lookup_config),
        service_registry: Mutex::new(service_registry),
        service_cache,
        map_layers: Mutex::new(map_layers),
        rig_provider: MockRigProvider::default(),
        rig_config: Mutex::new(rig_config),
        station_store,
        station_book: Mutex::new(station_book),
        credential_store: Mutex::new(credential_store),
        upload_queue: Mutex::new(upload_queue),
        online_support: Mutex::new(online_support),
        last_report: Mutex::new(None),
        permission_registry,
        permission_store,
        service_registry_store,
        service_cache_store,
        map_layer_store,
        upload_queue_store,
        permission_grants: Mutex::new(permission_grants),
        permission_settings: Mutex::new(permission_settings),
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
    service_registry: Mutex<ServiceRegistry>,
    service_cache: ServiceCache,
    map_layers: Mutex<MapLayerStack>,
    rig_provider: MockRigProvider,
    rig_config: Mutex<RigUiConfig>,
    station_store: JsonStationBookStore,
    station_book: Mutex<StationBook>,
    credential_store: Mutex<Box<dyn CredentialStore>>,
    upload_queue: Mutex<UploadQueue>,
    online_support: Mutex<OnlineSupportState>,
    last_report: Mutex<Option<DiagnosticReportUploadResponse>>,
    permission_registry: PermissionRegistry,
    permission_store: JsonPermissionGrantStore,
    service_registry_store: JsonSupportStore<ServiceRegistry>,
    service_cache_store: JsonSupportStore<Vec<ServiceCacheEntry>>,
    map_layer_store: JsonSupportStore<MapLayerStack>,
    upload_queue_store: JsonSupportStore<UploadQueue>,
    permission_grants: Mutex<PermissionGrantSet>,
    permission_settings: Mutex<PermissionSettings>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RigUiConfig {
    enable_rig_control: bool,
    default_provider: String,
    default_rig_id: Option<uuid::Uuid>,
    polling_interval_ms: u64,
    auto_fill_from_rig: bool,
    hamlib_endpoint: String,
    serial_settings_placeholder: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OnlineSupportState {
    automation_tasks: Vec<OnlineAutomationTask>,
    notifications: Vec<OnlineNotification>,
}

impl Default for OnlineSupportState {
    fn default() -> Self {
        Self {
            automation_tasks: ham_core::default_online_automation_tasks(),
            notifications: Vec::new(),
        }
    }
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

fn seed_default_station_book(book: &mut StationBook) {
    let mut profile = StationProfile::new("Home Station", "KE8YGW");
    profile.operator_callsign = Some("KE8YGW".to_owned());
    profile.default_grid = Some("EN91".to_owned());
    profile.default_power_watts = Some(100);
    profile.active = true;
    let profile = book.create_profile(profile);
    let mut radio = EquipmentItem::new(EquipmentType::Radio, "Mock HF Rig");
    radio.manufacturer = Some("Demo".to_owned());
    radio.model = Some("MockRig".to_owned());
    let radio = book.create_equipment(radio);
    let mut config = StationConfiguration::new(profile.station_profile_id, "Default HF");
    config.radio_id = Some(radio.equipment_id);
    config.default_power_watts = Some(100);
    if let Ok(config) = book.create_configuration(config) {
        let _ = book.select_configuration(config.configuration_id);
    }
}

fn default_upload_queue() -> UploadQueue {
    let registry = default_service_registry();
    let targets = registry
        .snapshot()
        .providers
        .into_iter()
        .filter(|provider| provider.metadata.service_type == ServiceType::LogUpload)
        .map(|provider| UploadTarget::from_provider(&provider.metadata))
        .collect();
    UploadQueue::new(targets)
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
            service_providers: service_registry_snapshot(&state),
        }),
        ("GET", "/api/runtime-events") => {
            let params = parse_query(query);
            let filter = runtime_filter_from_query(&params);
            json_response(&ApiRuntimeEventsPayload {
                runtime_events: state.bridge.replay(filter, 250),
                runtime_status: state.bridge.status(),
            })
        }
        ("GET", "/api/plugins/permissions") => handle_plugin_permissions(&state),
        ("POST", "/api/plugins/permissions/grant") => {
            handle_plugin_permission_action(&state, &request.body, PermissionGrantStatus::Granted)
        }
        ("POST", "/api/plugins/permissions/deny") => {
            handle_plugin_permission_action(&state, &request.body, PermissionGrantStatus::Denied)
        }
        ("POST", "/api/plugins/permissions/revoke") => {
            handle_plugin_permission_action(&state, &request.body, PermissionGrantStatus::Revoked)
        }
        ("GET", "/api/services/providers") => json_response(&service_registry_snapshot(&state)),
        ("POST", "/api/services/provider/update") => {
            handle_service_provider_update(&state, &request.body)
        }
        ("POST", "/api/services/cache/clear") => handle_service_cache_clear(&state, &request.body),
        ("GET", "/api/credentials") => handle_credentials(&state),
        ("POST", "/api/credentials/create") => handle_credential_create(&state, &request.body),
        ("POST", "/api/credentials/update") => handle_credential_update(&state, &request.body),
        ("POST", "/api/credentials/revoke") => handle_credential_revoke(&state, &request.body),
        ("POST", "/api/credentials/test") => handle_credential_test(&state, &request.body),
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
        ("GET", "/api/station") => handle_station_state(&state),
        ("POST", "/api/station/select-profile") => {
            handle_station_select_profile(&state, &request.body)
        }
        ("GET", "/api/awards") => handle_awards(&state),
        ("GET", "/api/search") => handle_search(&state, query),
        ("GET", "/api/uploads") => handle_uploads(&state),
        ("POST", "/api/uploads/queue") => handle_upload_queue_create(&state, &request.body),
        ("GET", "/api/online-services") => handle_online_services(&state),
        ("GET", "/api/maps/state") => handle_map_state(&state),
        ("POST", "/api/maps/layer/toggle") => handle_map_layer_toggle(&state, &request.body),
        ("GET", "/api/net-control") => handle_net_control_state(&state),
        ("POST", "/api/net/session/start") => handle_net_session_start(&state, &request.body),
        ("POST", "/api/net/session/end") => handle_net_session_end(&state, &request.body),
        ("POST", "/api/net/checkin/create") => handle_net_checkin_create(&state, &request.body),
        ("POST", "/api/net/checkin/delete") => handle_net_checkin_delete(&state, &request.body),
        ("POST", "/api/net/traffic/create") => handle_net_traffic_create(&state, &request.body),
        ("POST", "/api/net/report/export") => handle_net_report_export(&state, &request.body),
        ("GET", "/api/activations") => json_response(&activation_projection_payload(&state)),
        ("GET", "/api/lookup/callsign") => handle_lookup_callsign(&state, query),
        ("POST", "/api/lookup/cache/clear") => handle_lookup_cache_clear(&state),
        ("GET", "/api/lookup/status") => handle_lookup_status(&state),
        ("GET", "/api/rig/status") => handle_rig_status(&state),
        ("POST", "/api/rig/connect") => handle_rig_connect(&state),
        ("POST", "/api/rig/disconnect") => handle_rig_disconnect(&state),
        ("POST", "/api/rig/refresh") => handle_rig_refresh(&state),
        ("POST", "/api/rig/mock/set") => handle_rig_mock_set(&state, &request.body),
        ("GET", "/api/diagnostics/report-preview") => handle_report_preview(&state, query),
        ("POST", "/api/diagnostics/report/export") => handle_report_export(&state, &request.body),
        ("POST", "/api/diagnostics/report/upload") => handle_report_upload(&state, &request.body),
        ("GET", "/api/diagnostics/report/last") => json_response(
            &state
                .last_report
                .lock()
                .expect("last report mutex should not be poisoned")
                .clone(),
        ),
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
    service_providers: ServiceRegistrySnapshot,
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

#[derive(Debug, Serialize)]
struct ApiCredentialPayload {
    backend: ham_core::CredentialBackendStatus,
    credentials: Vec<CredentialMetadata>,
}

#[derive(Debug, Serialize)]
struct ApiNetControlPayload {
    sessions: Vec<ApiNetSessionRecord>,
    active_session: Option<ApiNetSessionRecord>,
    templates: Vec<Value>,
    checkins: Vec<ApiNetCheckInRecord>,
    traffic: Vec<Value>,
    report_preview: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ApiNetSessionRecord {
    net_session_id: uuid::Uuid,
    payload: Value,
    status: String,
    checkin_count: usize,
    late_checkin_count: usize,
    traffic_count: usize,
    emergency_traffic_count: usize,
    duplicate_warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ApiNetCheckInRecord {
    checkin_id: uuid::Uuid,
    payload: Value,
    status: String,
    traffic: String,
    deleted: bool,
}

#[derive(Debug, Deserialize)]
struct CredentialWriteRequest {
    provider_id: String,
    account_id: String,
    service_type: ServiceType,
    label: String,
    secret: String,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct CredentialIdRequest {
    credential_id: uuid::Uuid,
}

#[derive(Debug, Deserialize)]
struct CredentialUpdateRequest {
    credential_id: uuid::Uuid,
    secret: String,
    metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct NetSessionStartRequest {
    net_name: String,
    station_callsign: String,
    net_control_operator_id: String,
    frequency_hz: Option<u64>,
    band: Option<String>,
    mode: Option<String>,
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NetCheckInCreateRequest {
    net_session_id: uuid::Uuid,
    callsign: Option<String>,
    operator_name: Option<String>,
    location: Option<String>,
    grid: Option<String>,
    tactical_callsign: Option<String>,
    status: Option<String>,
    traffic: Option<String>,
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct NetTrafficCreateRequest {
    net_session_id: uuid::Uuid,
    from_callsign: Option<String>,
    to_callsign: Option<String>,
    precedence: String,
    summary: String,
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
struct StationProfileSelectRequest {
    station_profile_id: uuid::Uuid,
}

#[derive(Debug, Deserialize)]
struct UploadQueueCreateRequest {
    target_id: String,
    qso_ids: Vec<uuid::Uuid>,
    all_not_uploaded: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct DiagnosticReportRequest {
    report_type: Option<String>,
    path: Option<String>,
    user_notes: Option<String>,
    short_description: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PermissionActionRequest {
    plugin_id: String,
    permission_id: String,
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct PluginPermissionsPayload {
    plugins: Vec<ham_gui::mock::MockPlugin>,
    manifests: Vec<PluginManifest>,
    registry: Vec<ham_core::PermissionMetadata>,
    grants: PermissionGrantSet,
    settings: PermissionSettings,
}

#[derive(Debug, Deserialize)]
struct ServiceCacheClearRequest {
    service_type: Option<ServiceType>,
}

#[derive(Debug, Deserialize)]
struct ServiceProviderUpdateRequest {
    provider_id: String,
    enabled: Option<bool>,
    priority: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct MapLayerToggleRequest {
    layer_id: String,
    enabled: bool,
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

fn load_support_or<T>(
    bridge: &GuiRuntimeBridge,
    store: &JsonSupportStore<T>,
    fallback: T,
    label: &str,
) -> T
where
    T: Serialize + for<'de> Deserialize<'de> + Default,
{
    match store.load() {
        Ok(value) => {
            publish_support_storage_event(
                bridge,
                "support.storage.loaded",
                RuntimeEventSeverity::Info,
                format!("Loaded {label} support state"),
                Some(json!({"label": label, "path": store.path().display().to_string()})),
            );
            value
        }
        Err(error) => {
            publish_support_storage_event(
                bridge,
                "support.storage.error",
                RuntimeEventSeverity::Warn,
                format!("Using default {label} support state"),
                Some(json!({"label": label, "path": store.path().display().to_string()})),
            );
            eprintln!("failed to load {label} support state: {error}");
            fallback
        }
    }
}

fn save_support_state<T>(
    state: &AppState,
    store: &JsonSupportStore<T>,
    data: &T,
    label: &str,
) -> Result<(), String>
where
    T: Serialize + for<'de> Deserialize<'de> + Default,
{
    match store.save(data) {
        Ok(()) => {
            let summary = format!("Saved {label} support state");
            let _ = publish_gui_runtime(
                state,
                "support.storage.saved",
                RuntimeEventSeverity::Debug,
                &summary,
                Some(json!({"label": label, "path": store.path().display().to_string()})),
                None,
            );
            Ok(())
        }
        Err(error) => {
            let message = error.to_string();
            let summary = format!("Failed to save {label} support state");
            let _ = publish_gui_runtime(
                state,
                "support.storage.error",
                RuntimeEventSeverity::Warn,
                &summary,
                Some(json!({"label": label, "path": store.path().display().to_string()})),
                Some(message.clone()),
            );
            Err(message)
        }
    }
}

fn persist_service_cache(state: &AppState) {
    let entries = state
        .proposal_runtime
        .block_on(state.service_cache.entries());
    let _ = save_support_state(state, &state.service_cache_store, &entries, "service cache");
}

fn publish_support_storage_event(
    bridge: &GuiRuntimeBridge,
    event_type: &str,
    severity: RuntimeEventSeverity,
    payload_summary: String,
    redacted_payload: Option<Value>,
) {
    let _ = bridge.publish(RuntimeEventInput {
        event_type: event_type.to_owned(),
        severity,
        source: "ham-gui".to_owned(),
        source_plugin_id: Some("core.support-storage".to_owned()),
        workspace_id: Some("dashboard".to_owned()),
        payload_summary,
        redacted_payload,
        error: None,
    });
}

fn proposal_context(state: &AppState) -> ProposalContext {
    context_for_manifest(state, core_gui_manifest(), OperatorRole::Admin)
}

fn pota_sota_context(state: &AppState) -> ProposalContext {
    context_for_manifest(state, pota_sota_manifest(), OperatorRole::Admin)
}

fn net_control_context(state: &AppState) -> ProposalContext {
    context_for_manifest(state, net_control_manifest(), OperatorRole::Admin)
}

fn context_for_manifest(
    state: &AppState,
    plugin_manifest: PluginManifest,
    operator_role: OperatorRole,
) -> ProposalContext {
    ProposalContext {
        plugin_manifest,
        operator_role,
        permission_grants: state
            .permission_grants
            .lock()
            .expect("permission grants mutex should not be poisoned")
            .clone(),
    }
}

fn core_gui_manifest() -> PluginManifest {
    let mut manifest = PluginManifest::new(
        "core.gui",
        "Core GUI",
        env!("CARGO_PKG_VERSION"),
        vec![
            PluginCapability::QsoView,
            PluginCapability::QsoCreate,
            PluginCapability::QsoCorrect,
            PluginCapability::QsoDelete,
            PluginCapability::QsoRestore,
            PluginCapability::QsoNoteAdd,
            PluginCapability::QsoViewDeleted,
            PluginCapability::AdifImport,
            PluginCapability::AdifExport,
            PluginCapability::DiagnosticsViewLogs,
            PluginCapability::DiagnosticsExport,
            PluginCapability::DiagnosticsUpload,
            PluginCapability::SyncLanDiscovery,
            PluginCapability::SyncLanPull,
            PluginCapability::SyncLanPush,
            PluginCapability::SyncCloudConnect,
            PluginCapability::SyncCloudPull,
            PluginCapability::SyncCloudPush,
            PluginCapability::ServiceProviderEnable,
            PluginCapability::ServiceProviderDisable,
            PluginCapability::ServiceProviderConfigure,
            PluginCapability::ServiceCacheClear,
            PluginCapability::StationProfileView,
            PluginCapability::StationProfileManage,
            PluginCapability::StationEquipmentView,
            PluginCapability::StationEquipmentManage,
            PluginCapability::StationProfileUse,
            PluginCapability::CredentialViewMetadata,
            PluginCapability::CredentialCreate,
            PluginCapability::CredentialUpdate,
            PluginCapability::CredentialDelete,
            PluginCapability::CredentialUse,
            PluginCapability::CredentialTest,
            PluginCapability::SettingsRead,
            PluginCapability::SettingsWrite,
        ],
    );
    manifest.description = "Built-in GUI surfaces and local operator actions.".to_owned();
    manifest.contributed_panels = vec!["recent-qsos".to_owned(), "event-bus-monitor".to_owned()];
    manifest.contributed_commands = vec!["diagnostics.report.problem".to_owned()];
    manifest
}

fn pota_sota_manifest() -> PluginManifest {
    let mut manifest = PluginManifest::new(
        "plugin.pota-sota",
        "POTA/SOTA Tools",
        env!("CARGO_PKG_VERSION"),
        vec![
            PluginCapability::ActivationView,
            PluginCapability::ActivationCreate,
            PluginCapability::ActivationUpdate,
            PluginCapability::ActivationEnd,
            PluginCapability::QsoCreate,
            PluginCapability::QsoCorrect,
            PluginCapability::QsoNoteAdd,
            PluginCapability::AdifExport,
        ],
    );
    manifest.description = "Portable activation workflow and activation-linked QSOs.".to_owned();
    manifest.contributed_panels = vec![
        "activation-setup".to_owned(),
        "activation-progress".to_owned(),
        "portable-logger-entry".to_owned(),
    ];
    manifest
}

fn lookup_manifest() -> PluginManifest {
    let mut manifest = PluginManifest::new(
        "plugin.callsign-lookup",
        "Callsign Lookup",
        env!("CARGO_PKG_VERSION"),
        vec![
            PluginCapability::LookupCallsign,
            PluginCapability::LookupEntity,
            PluginCapability::LookupGrid,
            PluginCapability::LookupCacheRead,
            PluginCapability::LookupCacheWrite,
            PluginCapability::ServiceCacheRead,
            PluginCapability::ServiceCacheWrite,
            PluginCapability::ServiceCacheClear,
            PluginCapability::QsoSuggestFields,
        ],
    );
    manifest.optional_permissions = vec![PluginCapability::NetworkExternalLookup];
    manifest.contributed_services = vec![
        ServiceType::CallsignLookup,
        ServiceType::EntityLookup,
        ServiceType::GridLookup,
    ];
    manifest.description =
        "Advisory callsign, prefix, grid, and cache-backed enrichment.".to_owned();
    manifest
}

fn rig_manifest() -> PluginManifest {
    let mut manifest = PluginManifest::new(
        "plugin.rig-control",
        "Rig Control",
        env!("CARGO_PKG_VERSION"),
        vec![
            PluginCapability::RigView,
            PluginCapability::RigReadState,
            PluginCapability::RigConfigure,
            PluginCapability::RigControlFrequency,
            PluginCapability::RigControlMode,
            PluginCapability::RigControlPtt,
            PluginCapability::RigControlSplit,
            PluginCapability::QsoSuggestFields,
        ],
    );
    manifest.description = "Mock rig state and future CAT/Hamlib control.".to_owned();
    manifest.contributed_panels = vec!["rig-control".to_owned()];
    manifest
}

fn plugin_manifests() -> Vec<PluginManifest> {
    vec![
        core_gui_manifest(),
        pota_sota_manifest(),
        net_control_manifest(),
        lookup_manifest(),
        rig_manifest(),
        log_upload_manifest(),
        online_services_manifest(),
        spotting_manifest(),
        maps_manifest(),
        weather_manifest(),
        propagation_manifest(),
    ]
}

fn net_control_manifest() -> PluginManifest {
    let mut manifest = PluginManifest::new(
        "plugin.net-control",
        "Net Control",
        env!("CARGO_PKG_VERSION"),
        vec![
            PluginCapability::NetView,
            PluginCapability::NetTemplateCreate,
            PluginCapability::NetTemplateUpdate,
            PluginCapability::NetSessionStart,
            PluginCapability::NetSessionEnd,
            PluginCapability::NetCheckinCreate,
            PluginCapability::NetCheckinUpdate,
            PluginCapability::NetCheckinDelete,
            PluginCapability::NetTrafficManage,
            PluginCapability::NetReportExport,
        ],
    );
    manifest.description =
        "Directed net sessions, check-ins, traffic queue, and net reports.".to_owned();
    manifest.contributed_panels = vec![
        "net-session-control".to_owned(),
        "net-checkin-entry".to_owned(),
        "net-checkin-roster".to_owned(),
        "net-traffic-queue".to_owned(),
        "net-report".to_owned(),
    ];
    manifest.contributed_commands = vec![
        "net.open".to_owned(),
        "net.session.start".to_owned(),
        "net.session.end".to_owned(),
        "net.checkin.focus".to_owned(),
        "net.report.export".to_owned(),
    ];
    manifest
}

fn log_upload_manifest() -> PluginManifest {
    let mut manifest = PluginManifest::new(
        "plugin.log-upload",
        "Log Upload Providers",
        env!("CARGO_PKG_VERSION"),
        vec![
            PluginCapability::AdifExport,
            PluginCapability::UploadLog,
            PluginCapability::UploadConfirmationPull,
            PluginCapability::UploadQueueManage,
            PluginCapability::UploadStatusView,
            PluginCapability::NetworkExternalUpload,
        ],
    );
    manifest.description = "LoTW, eQSL, Club Log, and QRZ Logbook provider stubs.".to_owned();
    manifest.contributed_services = vec![ServiceType::LogUpload];
    manifest
}

fn online_services_manifest() -> PluginManifest {
    let mut manifest = PluginManifest::new(
        "plugin.online-services",
        "Online Services",
        env!("CARGO_PKG_VERSION"),
        vec![
            PluginCapability::ServiceCacheRead,
            PluginCapability::ServiceCacheWrite,
            PluginCapability::ServiceCacheClear,
            PluginCapability::UploadLog,
            PluginCapability::UploadConfirmationPull,
            PluginCapability::UploadQueueManage,
            PluginCapability::UploadStatusView,
            PluginCapability::NetworkExternalUpload,
            PluginCapability::LookupCallsign,
            PluginCapability::LookupEntity,
            PluginCapability::LookupGrid,
            PluginCapability::NetworkExternalLookup,
            PluginCapability::SpottingView,
            PluginCapability::SpottingConfigure,
            PluginCapability::NetworkExternalSpotting,
            PluginCapability::MapView,
            PluginCapability::NetworkExternalMap,
            PluginCapability::WeatherView,
            PluginCapability::NetworkExternalWeather,
            PluginCapability::PropagationView,
            PluginCapability::NetworkExternalPropagation,
            PluginCapability::CredentialViewMetadata,
            PluginCapability::CredentialUse,
            PluginCapability::CredentialTest,
            PluginCapability::AutomationManage,
            PluginCapability::NotificationView,
        ],
    );
    manifest.description =
        "Connected logbooks, lookups, spots, weather, propagation, maps, automation, and notifications.".to_owned();
    manifest.contributed_panels = vec![
        "online-accounts".to_owned(),
        "online-providers".to_owned(),
        "online-upload-queue".to_owned(),
        "online-downloads".to_owned(),
        "confirmation-status".to_owned(),
        "provider-health".to_owned(),
        "service-cache".to_owned(),
        "online-automation".to_owned(),
        "online-notifications".to_owned(),
    ];
    manifest.contributed_commands = vec![
        "online.open".to_owned(),
        "online.upload.queue".to_owned(),
        "online.download.confirmations".to_owned(),
        "online.health.refresh".to_owned(),
        "online.dxcluster.open".to_owned(),
        "online.pota-spots.open".to_owned(),
        "online.sota-spots.open".to_owned(),
    ];
    manifest.contributed_services = vec![
        ServiceType::LogUpload,
        ServiceType::CallsignLookup,
        ServiceType::Spotting,
        ServiceType::Weather,
        ServiceType::Propagation,
        ServiceType::MapTiles,
        ServiceType::Geocoding,
        ServiceType::Notification,
    ];
    manifest
}

fn spotting_manifest() -> PluginManifest {
    let mut manifest = PluginManifest::new(
        "plugin.spotting",
        "Spotting Providers",
        env!("CARGO_PKG_VERSION"),
        vec![
            PluginCapability::SpottingView,
            PluginCapability::SpottingConfigure,
            PluginCapability::NetworkExternalSpotting,
        ],
    );
    manifest.description =
        "DX Cluster, POTA, SOTAWatch, and RBN spotting provider stubs.".to_owned();
    manifest.contributed_panels = vec!["spots-alerts".to_owned(), "dx-cluster".to_owned()];
    manifest.contributed_services = vec![ServiceType::Spotting];
    manifest
}

fn maps_manifest() -> PluginManifest {
    let mut manifest = PluginManifest::new(
        "plugin.maps",
        "Maps",
        env!("CARGO_PKG_VERSION"),
        vec![PluginCapability::MapView, PluginCapability::MapConfigure],
    );
    manifest.description =
        "GIS map workspace, layers, markers, overlays, tiles, and geocoding providers.".to_owned();
    manifest.contributed_panels = vec![
        "interactive-map".to_owned(),
        "map-layers".to_owned(),
        "map-selected-object".to_owned(),
        "map-search".to_owned(),
        "map-filters".to_owned(),
    ];
    manifest.contributed_services = vec![ServiceType::MapTiles, ServiceType::Geocoding];
    manifest
}

fn weather_manifest() -> PluginManifest {
    let mut manifest = PluginManifest::new(
        "plugin.weather",
        "Weather",
        env!("CARGO_PKG_VERSION"),
        vec![PluginCapability::WeatherView],
    );
    manifest.description = "NOAA/Open-Meteo/manual weather provider placeholders.".to_owned();
    manifest.contributed_panels = vec!["weather".to_owned()];
    manifest.contributed_services = vec![ServiceType::Weather];
    manifest
}

fn propagation_manifest() -> PluginManifest {
    let mut manifest = PluginManifest::new(
        "plugin.propagation",
        "Propagation",
        env!("CARGO_PKG_VERSION"),
        vec![PluginCapability::PropagationView],
    );
    manifest.description = "Solar, MUF, grayline, and VOACAP provider placeholders.".to_owned();
    manifest.contributed_panels = vec!["propagation".to_owned()];
    manifest.contributed_services = vec![ServiceType::Propagation];
    manifest
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
        "providers": [lookup_provider_status(state)],
        "service_registry": service_registry_snapshot(state)
    }))
}

fn service_registry_snapshot(state: &AppState) -> ServiceRegistrySnapshot {
    state
        .service_registry
        .lock()
        .expect("service registry mutex should not be poisoned")
        .snapshot()
}

fn handle_service_provider_update(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::ServiceProviderConfigure,
        "Service provider configure permission check",
    ) {
        return response;
    }
    let Ok(request) = serde_json::from_slice::<ServiceProviderUpdateRequest>(body) else {
        return json_error(400, "invalid service provider update JSON");
    };
    let mut registry = state
        .service_registry
        .lock()
        .expect("service registry mutex should not be poisoned");
    if let Some(enabled) = request.enabled {
        if let Err(error) = registry.set_enabled(&request.provider_id, enabled) {
            return json_error(400, error.to_string());
        }
    }
    if let Some(priority) = request.priority {
        if let Err(error) = registry.set_priority(&request.provider_id, priority) {
            return json_error(400, error.to_string());
        }
    }
    let registry_snapshot = registry.clone();
    drop(registry);
    let _ = save_support_state(
        state,
        &state.service_registry_store,
        &registry_snapshot,
        "service registry",
    );
    let _ = publish_gui_runtime(
        state,
        "service.provider.health_changed",
        RuntimeEventSeverity::Info,
        "Service provider settings updated",
        Some(json!({
            "provider_id": request.provider_id,
            "enabled": request.enabled,
            "priority": request.priority
        })),
        None,
    );
    json_response(&service_registry_snapshot(state))
}

fn handle_plugin_permissions(state: &AppState) -> Vec<u8> {
    json_response(&PluginPermissionsPayload {
        plugins: mock_plugins(),
        manifests: plugin_manifests(),
        registry: state.permission_registry.all(),
        grants: state
            .permission_grants
            .lock()
            .expect("permission grants mutex should not be poisoned")
            .clone(),
        settings: state
            .permission_settings
            .lock()
            .expect("permission settings mutex should not be poisoned")
            .clone(),
    })
}

fn handle_plugin_permission_action(
    state: &AppState,
    body: &[u8],
    status: PermissionGrantStatus,
) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<PermissionActionRequest>(body) else {
        return json_error(400, "invalid permission action JSON");
    };
    let permission = parse_permission_id(&request.permission_id);
    if state.permission_registry.get(&permission).is_none() {
        let _ = publish_gui_runtime(
            state,
            "plugin.manifest.invalid",
            RuntimeEventSeverity::Warn,
            "Unknown plugin permission requested",
            Some(json!({"permission_id": request.permission_id, "plugin_id": request.plugin_id})),
            None,
        );
        return json_error(400, "unknown permission");
    }
    let grant = {
        let mut grants = state
            .permission_grants
            .lock()
            .expect("permission grants mutex should not be poisoned");
        let grant = grants.set_status(
            &request.plugin_id,
            permission.clone(),
            status,
            request.reason.clone(),
        );
        if let Err(error) = state.permission_store.save(&grants) {
            return json_error(500, format!("failed to save permission grants: {error}"));
        }
        grant
    };
    let event_type = match status {
        PermissionGrantStatus::Granted => "plugin.permission.granted",
        PermissionGrantStatus::Denied => "plugin.permission.denied",
        PermissionGrantStatus::Pending => "plugin.permission.requested",
        PermissionGrantStatus::Revoked => "plugin.permission.revoked",
    };
    let _ = publish_gui_runtime(
        state,
        event_type,
        RuntimeEventSeverity::Info,
        "Plugin permission state changed",
        Some(json!(&grant)),
        None,
    );
    json_response(
        &json!({"ok": true, "grant": grant, "permissions": handle_plugin_permissions_payload(state)}),
    )
}

fn handle_plugin_permissions_payload(state: &AppState) -> PluginPermissionsPayload {
    PluginPermissionsPayload {
        plugins: mock_plugins(),
        manifests: plugin_manifests(),
        registry: state.permission_registry.all(),
        grants: state
            .permission_grants
            .lock()
            .expect("permission grants mutex should not be poisoned")
            .clone(),
        settings: state
            .permission_settings
            .lock()
            .expect("permission settings mutex should not be poisoned")
            .clone(),
    }
}

fn parse_permission_id(permission_id: &str) -> PluginCapability {
    serde_json::from_value(json!(permission_id))
        .unwrap_or_else(|_| PluginCapability::Other(permission_id.to_owned()))
}

fn ensure_gui_permission(
    state: &AppState,
    manifest: &PluginManifest,
    permission: PluginCapability,
    action_summary: &str,
) -> Result<(), Vec<u8>> {
    let grants = state
        .permission_grants
        .lock()
        .expect("permission grants mutex should not be poisoned")
        .clone();
    match ham_core::check_plugin_permission(manifest, &grants, &permission) {
        Ok(()) => {
            let _ = publish_gui_runtime(
                state,
                "plugin.permission.check.allowed",
                RuntimeEventSeverity::Debug,
                action_summary,
                Some(
                    json!({"plugin_id": manifest.plugin_id, "permission_id": permission.as_str()}),
                ),
                None,
            );
            Ok(())
        }
        Err(error) => {
            let _ = publish_gui_runtime(
                state,
                "plugin.permission.check.denied",
                RuntimeEventSeverity::Warn,
                action_summary,
                Some(
                    json!({"plugin_id": manifest.plugin_id, "permission_id": permission.as_str()}),
                ),
                Some(error.to_string()),
            );
            Err(json_response_with_status(
                403,
                &json!({"ok": false, "error": error.to_string()}),
            ))
        }
    }
}

fn handle_lookup_callsign(state: &AppState, query: &str) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &lookup_manifest(),
        PluginCapability::LookupCallsign,
        "Callsign lookup permission check",
    ) {
        return response;
    }
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
    let registry = state
        .service_registry
        .lock()
        .expect("service registry mutex should not be poisoned")
        .clone();
    let result = state
        .proposal_runtime
        .block_on(lookup_callsign_with_service_framework(
            &provider,
            &registry,
            &state.service_cache,
            &LookupCacheConfig {
                ttl_days: config.cache_ttl_days,
            },
            &state.bridge,
            callsign,
            state.bridge.status().device_id,
        ));
    match result {
        Ok(suggestion) => {
            persist_service_cache(state);
            json_response(&json!({
                "ok": true,
                "suggestion": suggestion,
                "provider_status": lookup_provider_status(state)
            }))
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_lookup_cache_clear(state: &AppState) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &lookup_manifest(),
        PluginCapability::ServiceCacheClear,
        "Service cache clear permission check",
    ) {
        return response;
    }
    let service_cleared = state.proposal_runtime.block_on(
        state
            .service_cache
            .clear_service(ServiceType::CallsignLookup),
    );
    match state
        .proposal_runtime
        .block_on(ham_core::clear_lookup_cache(
            &state.lookup_cache,
            &state.bridge,
            state.bridge.status().device_id,
        )) {
        Ok(()) => {
            persist_service_cache(state);
            json_response(&json!({"ok": true, "service_cache_cleared": service_cleared}))
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_service_cache_clear(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::ServiceCacheClear,
        "Service cache clear permission check",
    ) {
        return response;
    }
    let request = if body.is_empty() {
        ServiceCacheClearRequest { service_type: None }
    } else {
        match serde_json::from_slice::<ServiceCacheClearRequest>(body) {
            Ok(request) => request,
            Err(_) => return json_error(400, "invalid service cache clear JSON"),
        }
    };
    let cleared = match request.service_type {
        Some(service_type) => state
            .proposal_runtime
            .block_on(state.service_cache.clear_service(service_type)),
        None => state
            .proposal_runtime
            .block_on(state.service_cache.clear_all()),
    };
    let _ = publish_gui_runtime(
        state,
        "service.request.cache_miss",
        RuntimeEventSeverity::Info,
        "Service cache cleared",
        Some(json!({"cleared": cleared, "service_type": request.service_type})),
        None,
    );
    persist_service_cache(state);
    json_response(&json!({"ok": true, "cleared": cleared}))
}

fn handle_credentials(state: &AppState) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::CredentialViewMetadata,
        "Credential metadata permission check",
    ) {
        return response;
    }
    let store = state
        .credential_store
        .lock()
        .expect("credential store mutex should not be poisoned");
    json_response(&ApiCredentialPayload {
        backend: store.backend_status(),
        credentials: store.list_metadata(),
    })
}

fn handle_credential_create(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::CredentialCreate,
        "Credential create permission check",
    ) {
        return response;
    }
    let request = match serde_json::from_slice::<CredentialWriteRequest>(body) {
        Ok(request) => request,
        Err(_) => return json_error(400, "invalid credential create JSON"),
    };
    let mut metadata = CredentialMetadata::new(
        request.provider_id,
        request.account_id,
        request.service_type,
        request.label,
    );
    metadata.metadata = request.metadata.unwrap_or_else(|| json!({}));
    let result = state
        .credential_store
        .lock()
        .expect("credential store mutex should not be poisoned")
        .store_credential(metadata, &request.secret);
    match result {
        Ok(metadata) => {
            let _ = publish_gui_runtime(
                state,
                "credential.created",
                RuntimeEventSeverity::Info,
                "Credential metadata created",
                Some(ham_core::credential_runtime_payload(&metadata)),
                None,
            );
            json_response(&json!({"ok": true, "credential": metadata}))
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_credential_update(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::CredentialUpdate,
        "Credential update permission check",
    ) {
        return response;
    }
    let request = match serde_json::from_slice::<CredentialUpdateRequest>(body) {
        Ok(request) => request,
        Err(_) => return json_error(400, "invalid credential update JSON"),
    };
    let result = state
        .credential_store
        .lock()
        .expect("credential store mutex should not be poisoned")
        .update_credential(request.credential_id, &request.secret, request.metadata);
    match result {
        Ok(metadata) => {
            let _ = publish_gui_runtime(
                state,
                "credential.updated",
                RuntimeEventSeverity::Info,
                "Credential metadata updated",
                Some(ham_core::credential_runtime_payload(&metadata)),
                None,
            );
            json_response(&json!({"ok": true, "credential": metadata}))
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_credential_revoke(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::CredentialDelete,
        "Credential revoke permission check",
    ) {
        return response;
    }
    let request = match serde_json::from_slice::<CredentialIdRequest>(body) {
        Ok(request) => request,
        Err(_) => return json_error(400, "invalid credential revoke JSON"),
    };
    let result = state
        .credential_store
        .lock()
        .expect("credential store mutex should not be poisoned")
        .revoke_credential(request.credential_id);
    match result {
        Ok(metadata) => {
            let _ = publish_gui_runtime(
                state,
                "credential.deleted",
                RuntimeEventSeverity::Info,
                "Credential revoked",
                Some(ham_core::credential_runtime_payload(&metadata)),
                None,
            );
            json_response(&json!({"ok": true, "credential": metadata}))
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_credential_test(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::CredentialTest,
        "Credential test permission check",
    ) {
        return response;
    }
    let request = match serde_json::from_slice::<CredentialIdRequest>(body) {
        Ok(request) => request,
        Err(_) => return json_error(400, "invalid credential test JSON"),
    };
    let _ = publish_gui_runtime(
        state,
        "credential.test.started",
        RuntimeEventSeverity::Info,
        "Testing credential availability",
        Some(json!({"credential_id": request.credential_id})),
        None,
    );
    let available = state
        .credential_store
        .lock()
        .expect("credential store mutex should not be poisoned")
        .test_credential(request.credential_id)
        .unwrap_or(false);
    let _ = publish_gui_runtime(
        state,
        if available {
            "credential.test.completed"
        } else {
            "credential.test.failed"
        },
        if available {
            RuntimeEventSeverity::Info
        } else {
            RuntimeEventSeverity::Warn
        },
        "Credential availability test completed",
        Some(json!({"credential_id": request.credential_id, "available": available})),
        None,
    );
    json_response(&json!({"ok": true, "available": available}))
}

fn handle_rig_status(state: &AppState) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &rig_manifest(),
        PluginCapability::RigView,
        "Rig view permission check",
    ) {
        return response;
    }
    json_response(&rig_status_payload(state))
}

fn handle_report_preview(state: &AppState, query: &str) -> Vec<u8> {
    let params = parse_query(query);
    let report_type = parse_report_type(params.get("type").map(String::as_str));
    let input = diagnostic_bundle_input(state, report_type, "");
    json_response(&input.preview())
}

fn handle_report_export(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::DiagnosticsExport,
        "Diagnostics export permission check",
    ) {
        return response;
    }
    let Ok(request) = serde_json::from_slice::<DiagnosticReportRequest>(body) else {
        return json_error(400, "invalid diagnostic export JSON");
    };
    let Some(path) = request
        .path
        .as_deref()
        .filter(|path| !path.trim().is_empty())
    else {
        return json_error(400, "missing output path");
    };
    let report_type = parse_report_type(request.report_type.as_deref());
    let _ = publish_gui_runtime(
        state,
        "diagnostics.report.started",
        RuntimeEventSeverity::Info,
        "Diagnostic report generation started",
        Some(json!({"report_type": report_type})),
        None,
    );
    let _ = publish_gui_runtime(
        state,
        "diagnostics.export.started",
        RuntimeEventSeverity::Info,
        "Diagnostic ZIP export started",
        Some(json!({"report_type": report_type, "path": path})),
        None,
    );
    match build_diagnostic_bundle(diagnostic_bundle_input(
        state,
        report_type,
        request.user_notes.as_deref().unwrap_or_default(),
    )) {
        Ok(bundle) => {
            let _ = publish_gui_runtime(
                state,
                "diagnostics.bundle.created",
                RuntimeEventSeverity::Info,
                "Diagnostic bundle created",
                Some(
                    json!({"bundle_hash": bundle.manifest.bundle_hash, "files": bundle.manifest.included_files}),
                ),
                None,
            );
            let _ = publish_gui_runtime(
                state,
                "diagnostics.redaction.completed",
                RuntimeEventSeverity::Info,
                "Diagnostic redaction completed",
                Some(json!(&bundle.manifest.redaction_summary)),
                None,
            );
            if let Err(error) = export_diagnostic_zip(&bundle, std::path::Path::new(path)) {
                return json_error(400, format!("failed to export diagnostic ZIP: {error}"));
            }
            let _ = publish_gui_runtime(
                state,
                "diagnostics.export.completed",
                RuntimeEventSeverity::Info,
                "Diagnostic ZIP export completed",
                Some(json!({"path": path, "bundle_hash": bundle.manifest.bundle_hash})),
                None,
            );
            json_response(&json!({
                "ok": true,
                "path": path,
                "file_name": bundle.file_name,
                "manifest": bundle.manifest
            }))
        }
        Err(error) => json_error(400, format!("failed to build diagnostic bundle: {error}")),
    }
}

fn handle_report_upload(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::DiagnosticsUpload,
        "Diagnostics upload permission check",
    ) {
        return response;
    }
    let Ok(request) = serde_json::from_slice::<DiagnosticReportRequest>(body) else {
        return json_error(400, "invalid diagnostic upload JSON");
    };
    let Some(auth) = cloud_auth(state) else {
        return json_response_with_status(
            401,
            &json!({"ok": false, "error": "cloud sync authentication is required before upload"}),
        );
    };
    let report_type = parse_report_type(request.report_type.as_deref());
    let _ = publish_gui_runtime(
        state,
        "diagnostics.report.started",
        RuntimeEventSeverity::Info,
        "Diagnostic report generation started",
        Some(json!({"report_type": report_type})),
        None,
    );
    let _ = publish_gui_runtime(
        state,
        "diagnostics.upload.started",
        RuntimeEventSeverity::Info,
        "Diagnostic report upload started",
        Some(json!({"report_type": report_type})),
        None,
    );
    let bundle = match build_diagnostic_bundle(diagnostic_bundle_input(
        state,
        report_type,
        request.user_notes.as_deref().unwrap_or_default(),
    )) {
        Ok(bundle) => bundle,
        Err(error) => {
            return json_error(400, format!("failed to build diagnostic bundle: {error}"))
        }
    };
    let _ = publish_gui_runtime(
        state,
        "diagnostics.bundle.created",
        RuntimeEventSeverity::Info,
        "Diagnostic bundle created",
        Some(
            json!({"bundle_hash": bundle.manifest.bundle_hash, "files": bundle.manifest.included_files}),
        ),
        None,
    );
    let _ = publish_gui_runtime(
        state,
        "diagnostics.redaction.completed",
        RuntimeEventSeverity::Info,
        "Diagnostic redaction completed",
        Some(json!(&bundle.manifest.redaction_summary)),
        None,
    );
    let upload = DiagnosticReportUploadRequest {
        auth,
        report_type: match report_type {
            DiagnosticReportType::Basic => DiagnosticReportUploadType::Basic,
            DiagnosticReportType::Sync => DiagnosticReportUploadType::Sync,
        },
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        core_version: env!("CARGO_PKG_VERSION").to_owned(),
        platform: bundle.manifest.platform.clone(),
        plugin_list: mock_plugins()
            .into_iter()
            .filter(|plugin| plugin.enabled)
            .map(|plugin| plugin.plugin_id)
            .collect(),
        sync_state_summary: Some(sync_state_summary(state)),
        short_description: request
            .short_description
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "No description provided".to_owned()),
        bundle_hash: bundle.manifest.bundle_hash.clone(),
        bundle_bytes: bundle.zip_bytes.clone(),
    };
    match state
        .proposal_runtime
        .block_on(state.cloud_server.upload_report(upload))
    {
        Ok(response) => {
            *state
                .last_report
                .lock()
                .expect("last report mutex should not be poisoned") = Some(response.clone());
            let summary = format!("Diagnostic report uploaded as {}", response.report_id);
            let _ = publish_gui_runtime(
                state,
                "diagnostics.upload.completed",
                RuntimeEventSeverity::Info,
                &summary,
                Some(json!(&response)),
                None,
            );
            json_response(&json!({"ok": true, "upload": response, "manifest": bundle.manifest}))
        }
        Err(error) => {
            let _ = publish_gui_runtime(
                state,
                "diagnostics.upload.failed",
                RuntimeEventSeverity::Error,
                "Diagnostic report upload failed",
                None,
                Some(error.to_string()),
            );
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn diagnostic_bundle_input(
    state: &AppState,
    report_type: DiagnosticReportType,
    user_notes: &str,
) -> DiagnosticBundleInput {
    let status = state.bridge.status();
    let sync_status = (report_type == DiagnosticReportType::Sync)
        .then(|| serde_json::to_value(sync_state_payload(state)).unwrap_or_else(|_| json!({})));
    DiagnosticBundleInput {
        report_type,
        runtime_log_dir: status.log_directory,
        runtime_events: state.bridge.replay(RuntimeEventFilter::default(), 500),
        app_version: env!("CARGO_PKG_VERSION").to_owned(),
        core_version: env!("CARGO_PKG_VERSION").to_owned(),
        device_id: status.device_id,
        session_id: status.session_id,
        account_id: state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned")
            .cloud_account_id
            .clone(),
        plugins: json!(mock_plugins()),
        sync_status,
        user_notes: user_notes.to_owned(),
    }
}

fn parse_report_type(value: Option<&str>) -> DiagnosticReportType {
    match value.unwrap_or("basic").to_ascii_lowercase().as_str() {
        "sync" => DiagnosticReportType::Sync,
        _ => DiagnosticReportType::Basic,
    }
}

fn sync_state_summary(state: &AppState) -> String {
    let sync = state
        .sync
        .lock()
        .expect("sync state mutex should not be poisoned");
    format!(
        "lan_discovery={} cloud_enabled={} warnings={} divergence={}",
        sync.discovery_running,
        sync.cloud_config.enable_cloud_sync,
        sync.warning_count,
        sync.divergence
            .clone()
            .or_else(|| sync.cloud_divergence.clone())
            .unwrap_or_else(|| "none".to_owned())
    )
}

fn handle_rig_connect(state: &AppState) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &rig_manifest(),
        PluginCapability::RigConfigure,
        "Rig configure permission check",
    ) {
        return response;
    }
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
    if let Err(response) = ensure_gui_permission(
        state,
        &rig_manifest(),
        PluginCapability::RigConfigure,
        "Rig configure permission check",
    ) {
        return response;
    }
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
    if let Err(response) = ensure_gui_permission(
        state,
        &rig_manifest(),
        PluginCapability::RigReadState,
        "Rig state read permission check",
    ) {
        return response;
    }
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
    if request.frequency_hz.is_some() {
        if let Err(response) = ensure_gui_permission(
            state,
            &rig_manifest(),
            PluginCapability::RigControlFrequency,
            "Rig frequency control permission check",
        ) {
            return response;
        }
    }
    if request
        .mode
        .as_ref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        if let Err(response) = ensure_gui_permission(
            state,
            &rig_manifest(),
            PluginCapability::RigControlMode,
            "Rig mode control permission check",
        ) {
            return response;
        }
    }
    if request.ptt.is_some() {
        if let Err(response) = ensure_gui_permission(
            state,
            &rig_manifest(),
            PluginCapability::RigControlPtt,
            "Rig PTT control permission check",
        ) {
            return response;
        }
    }
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

fn current_qso_projection(state: &AppState) -> ham_core::QsoCurrentStateProjection {
    state
        .proposal_runtime
        .block_on(state.store.rebuild_projections(state.logbook_id))
        .unwrap_or_else(|_| ham_core::QsoCurrentStateProjection::new())
}

fn handle_station_state(state: &AppState) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::StationProfileView,
        "Station profile view permission check",
    ) {
        return response;
    }
    let book = state
        .station_book
        .lock()
        .expect("station book mutex should not be poisoned")
        .clone();
    json_response(&json!({
        "profiles": book.profiles,
        "equipment": book.equipment,
        "configurations": book.configurations,
        "active_profile_id": book.active_profile_id,
        "active_configuration_id": book.active_configuration_id
    }))
}

fn handle_station_select_profile(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::StationProfileUse,
        "Station profile use permission check",
    ) {
        return response;
    }
    let Ok(request) = serde_json::from_slice::<StationProfileSelectRequest>(body) else {
        return json_error(400, "invalid station profile selection JSON");
    };
    let mut book = state
        .station_book
        .lock()
        .expect("station book mutex should not be poisoned");
    if let Err(error) = book.select_profile(request.station_profile_id) {
        return json_error(400, error.to_string());
    }
    let _ = state.station_store.save(&book);
    let _ = publish_gui_runtime(
        state,
        "station.profile.selected",
        RuntimeEventSeverity::Info,
        "Station profile selected",
        Some(json!({"station_profile_id": request.station_profile_id})),
        None,
    );
    json_response(&json!({"ok": true, "station": &*book}))
}

fn handle_awards(state: &AppState) -> Vec<u8> {
    let projection = current_qso_projection(state);
    let records = projection
        .list(true)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let _ = publish_gui_runtime(
        state,
        "awards.rebuild.started",
        RuntimeEventSeverity::Info,
        "Award progress rebuild started",
        None,
        None,
    );
    let engine = ham_core::AwardEngine::default_mvp();
    let progress = engine.rebuild_from_qsos(&records);
    let _ = publish_gui_runtime(
        state,
        "awards.rebuild.completed",
        RuntimeEventSeverity::Info,
        "Award progress rebuild completed",
        Some(json!({"award_count": progress.len()})),
        None,
    );
    json_response(&json!({
        "definitions": engine.definitions(),
        "progress": progress
    }))
}

fn handle_search(state: &AppState, query: &str) -> Vec<u8> {
    let params = parse_query(query);
    let raw = params.get("q").cloned().unwrap_or_default();
    let parsed = match ham_core::parse_search_query(&raw) {
        Ok(query) => query,
        Err(error) => return json_error(400, error.to_string()),
    };
    let projection = current_qso_projection(state);
    let records = projection
        .list(true)
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let _ = publish_gui_runtime(
        state,
        "search.query.started",
        RuntimeEventSeverity::Debug,
        "QSO search started",
        Some(json!({"query": raw})),
        None,
    );
    let results = ham_core::search_qsos(&records, &parsed);
    let _ = publish_gui_runtime(
        state,
        "search.query.completed",
        RuntimeEventSeverity::Info,
        "QSO search completed",
        Some(json!({"query": raw, "result_count": results.len()})),
        None,
    );
    json_response(&json!({"query": parsed, "results": results}))
}

fn handle_uploads(state: &AppState) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &log_upload_manifest(),
        PluginCapability::UploadStatusView,
        "Upload status view permission check",
    ) {
        return response;
    }
    let queue = state
        .upload_queue
        .lock()
        .expect("upload queue mutex should not be poisoned")
        .clone();
    json_response(&queue)
}

fn handle_upload_queue_create(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &log_upload_manifest(),
        PluginCapability::UploadQueueManage,
        "Upload queue manage permission check",
    ) {
        return response;
    }
    let Ok(request) = serde_json::from_slice::<UploadQueueCreateRequest>(body) else {
        return json_error(400, "invalid upload queue JSON");
    };
    let projection = current_qso_projection(state);
    let qso_ids = if request.all_not_uploaded.unwrap_or(false) || request.qso_ids.is_empty() {
        projection
            .list(false)
            .into_iter()
            .map(|qso| qso.qso_id)
            .collect::<Vec<_>>()
    } else {
        request.qso_ids
    };
    let mut queue = state
        .upload_queue
        .lock()
        .expect("upload queue mutex should not be poisoned");
    let job = match queue.create_job(request.target_id, state.logbook_id, qso_ids.clone()) {
        Ok(job) => job,
        Err(error) => return json_error(400, error.to_string()),
    };
    let queue_snapshot = queue.clone();
    drop(queue);
    let _ = save_support_state(
        state,
        &state.upload_queue_store,
        &queue_snapshot,
        "upload queue",
    );
    let adif = ham_core::adif_for_upload_job(&projection, &qso_ids);
    let _ = publish_gui_runtime(
        state,
        "upload.queue.created",
        RuntimeEventSeverity::Info,
        "Upload job queued",
        Some(json!({"upload_job_id": job.upload_job_id, "qso_count": qso_ids.len()})),
        None,
    );
    let _ = publish_gui_runtime(
        state,
        "upload.adif.generated",
        RuntimeEventSeverity::Info,
        "Upload ADIF generated",
        Some(json!({"upload_job_id": job.upload_job_id, "bytes": adif.len()})),
        None,
    );
    json_response(&json!({"ok": true, "job": job, "adif_preview": adif}))
}

fn handle_online_services(state: &AppState) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &online_services_manifest(),
        PluginCapability::ServiceCacheRead,
        "Online services dashboard permission check",
    ) {
        return response;
    }

    let registry_snapshot = service_registry_snapshot(state);
    let credentials = state
        .credential_store
        .lock()
        .expect("credential store mutex should not be poisoned")
        .list_metadata();
    let providers = registry_snapshot
        .providers
        .iter()
        .filter(|provider| provider.metadata.source_plugin_id == "plugin.online-services")
        .map(|provider| {
            let health = missing_credential_status(&provider.metadata, &credentials)
                .map(|status| ham_core::ProviderHealth {
                    provider_id: status.provider_id,
                    state: match status.status {
                        OnlineProviderStatus::MissingCredentials => {
                            ham_core::ProviderHealthState::MissingConfig
                        }
                        OnlineProviderStatus::Healthy => ham_core::ProviderHealthState::Healthy,
                        OnlineProviderStatus::RateLimited => {
                            ham_core::ProviderHealthState::Degraded
                        }
                        _ => ham_core::ProviderHealthState::Unavailable,
                    },
                    message: status.message,
                    checked_at: status.checked_at,
                    rate_limited: status.status == OnlineProviderStatus::RateLimited,
                })
                .unwrap_or_else(|| provider.health.clone());
            (provider.metadata.clone(), health)
        })
        .collect::<Vec<_>>();
    let queue = state
        .upload_queue
        .lock()
        .expect("upload queue mutex should not be poisoned")
        .clone();
    let mut notifications = providers
        .iter()
        .filter(|(_, health)| health.state == ham_core::ProviderHealthState::MissingConfig)
        .map(|(metadata, health)| {
            let mut notification = OnlineNotification::new(
                "notification.provider.missing_credentials",
                NotificationSeverity::Warning,
                format!("{} needs credentials", metadata.display_name),
                health.message.clone(),
            );
            notification.related_provider_id = Some(metadata.provider_id.clone());
            notification
        })
        .collect::<Vec<_>>();
    if queue
        .jobs
        .iter()
        .any(|job| job.status == ham_core::UploadStatus::Failed)
    {
        notifications.push(OnlineNotification::new(
            "notification.upload.failed",
            NotificationSeverity::Warning,
            "Upload job failed",
            "One or more upload jobs are waiting for retry or operator review.",
        ));
    }
    let online_support = state
        .online_support
        .lock()
        .expect("online support mutex should not be poisoned")
        .clone();
    notifications.extend(online_support.notifications.clone());
    let mut dashboard = state.proposal_runtime.block_on(online_services_dashboard(
        providers,
        credentials,
        &queue,
        &state.service_cache,
        notifications,
    ));
    dashboard.automation_tasks = online_support.automation_tasks;
    let dx_spot = parse_dx_cluster_line("DX de K1ABC: 14074.0 JA1XYZ FT8 loud 1234Z")
        .map(|spot| dx_cluster_spot_to_spot(spot, "dx-cluster"));
    let pota_spot = pota_spot_to_spot(PotaSpotRecord {
        activator: "K8POTA".to_owned(),
        reference: "US-0001".to_owned(),
        frequency_hz: 14_244_000,
        mode: Some("SSB".to_owned()),
        spotted_at: chrono::Utc::now(),
        comments: Some("Mock live POTA spot".to_owned()),
    });
    let mut sota_spot = pota_spot.clone();
    sota_spot.spotted_callsign = "G4SOTA".to_owned();
    sota_spot.reference = Some("W6/CT-001".to_owned());
    sota_spot.source.provider_id = "sotawatch".to_owned();
    sota_spot.source.label = "SOTAWatch".to_owned();
    let confirmations = confirmations_from_adif(
        "lotw",
        "<CALL:5>K1ABC<BAND:3>20M<MODE:3>FT8<QSO_DATE:8>20260706<EOR>",
        chrono::Utc::now(),
    );
    let _ = publish_gui_runtime(
        state,
        "service.request.completed",
        RuntimeEventSeverity::Info,
        "Online services dashboard refreshed",
        Some(json!({
            "provider_count": dashboard.providers.len(),
            "cache_entries": dashboard.cache_entries,
            "credential_values_redacted": true
        })),
        None,
    );

    json_response(&json!({
        "dashboard": dashboard,
        "spots": {
            "dx_cluster": dx_spot.into_iter().collect::<Vec<_>>(),
            "pota": [pota_spot],
            "sota": [sota_spot]
        },
        "confirmation_status": confirmations,
        "weather": mock_weather(Coordinate { latitude: 41.0, longitude: -81.0 }),
        "propagation": mock_propagation_forecast(),
        "api_status": {
            "lotw": "credential_required",
            "eqsl": "credential_required",
            "club_log": "credential_required",
            "qrz": "credential_required",
            "hrdlog": "credential_required",
            "dx_cluster": "offline_parser_ready",
            "pota": "provider_ready_for_network_adapter",
            "sota": "provider_ready_for_network_adapter",
            "weather": "mock_provider_active",
            "propagation": "mock_provider_active"
        }
    }))
}

fn handle_map_state(state: &AppState) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &maps_manifest(),
        PluginCapability::MapView,
        "Map view permission check",
    ) {
        return response;
    }

    let projection = current_qso_projection(state);
    let station_book = state
        .station_book
        .lock()
        .expect("station book mutex should not be poisoned")
        .clone();
    let active_station_coordinate = station_book
        .active_profile()
        .and_then(|profile| profile.default_grid.as_deref())
        .and_then(|grid| maidenhead_to_coordinate(grid).ok());
    let qso_objects = qso_map_objects(&projection, active_station_coordinate, None);
    let station_profiles = station_book
        .profiles
        .iter()
        .filter_map(|profile| serde_json::to_value(profile).ok())
        .collect::<Vec<_>>();
    let station_markers = station_markers_from_profiles(&station_profiles);
    let layers = state
        .map_layers
        .lock()
        .expect("map layers mutex should not be poisoned")
        .clone();
    let enabled_layer = layers
        .layers
        .iter()
        .filter(|layer| layer.enabled)
        .min_by_key(|layer| layer.order)
        .map(|layer| layer.title.clone())
        .unwrap_or_else(|| "none".to_owned());
    let status_coordinate = active_station_coordinate
        .or_else(|| {
            qso_objects
                .iter()
                .map(|object| object.marker.coordinate)
                .next()
        })
        .unwrap_or(Coordinate {
            latitude: 0.0,
            longitude: 0.0,
        });
    let status_grid = ham_core::encode_maidenhead(status_coordinate, 6).unwrap_or_else(|_| {
        active_station_coordinate
            .and_then(|coordinate| ham_core::encode_maidenhead(coordinate, 4).ok())
            .unwrap_or_else(|| "unknown".to_owned())
    });
    let selected_distance = qso_objects
        .iter()
        .find_map(|object| object.distance.as_ref())
        .map(|distance| format!("{:.0} km", distance.kilometers))
        .unwrap_or_else(|| "n/a".to_owned());
    let selected_bearing = qso_objects
        .iter()
        .find_map(|object| object.bearing.as_ref())
        .map(|bearing| format!("{:.0} deg", bearing.initial_degrees))
        .unwrap_or_else(|| "n/a".to_owned());
    let providers = service_registry_snapshot(state)
        .providers
        .into_iter()
        .filter(|provider| {
            matches!(
                provider.metadata.service_type,
                ServiceType::MapTiles
                    | ServiceType::Geocoding
                    | ServiceType::Weather
                    | ServiceType::Propagation
            )
        })
        .collect::<Vec<_>>();
    let _ = publish_gui_runtime(
        state,
        "map.loaded",
        RuntimeEventSeverity::Info,
        "Map state loaded",
        Some(json!({
            "qso_markers": qso_objects.len(),
            "station_markers": station_markers.len(),
            "enabled_layers": layers.layers.iter().filter(|layer| layer.enabled).count()
        })),
        None,
    );

    json_response(&json!({
        "providers": providers,
        "layers": layers,
        "qso_objects": qso_objects,
        "station_markers": station_markers,
        "grayline": grayline_snapshot(chrono::Utc::now()).ok(),
        "propagation": mock_propagation_forecast(),
        "weather": mock_weather(status_coordinate),
        "status": {
            "grid": status_grid,
            "coordinates": {
                "latitude": status_coordinate.latitude,
                "longitude": status_coordinate.longitude
            },
            "distance": selected_distance,
            "bearing": selected_bearing,
            "zoom": "4",
            "selected_layer": enabled_layer
        }
    }))
}

fn handle_map_layer_toggle(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &maps_manifest(),
        PluginCapability::MapConfigure,
        "Map layer configure permission check",
    ) {
        return response;
    }
    let Ok(request) = serde_json::from_slice::<MapLayerToggleRequest>(body) else {
        return json_error(400, "invalid map layer toggle JSON");
    };
    {
        let mut layers = state
            .map_layers
            .lock()
            .expect("map layers mutex should not be poisoned");
        if let Err(error) = layers.set_enabled(&request.layer_id, request.enabled) {
            return json_error(400, error.to_string());
        }
        let layers_snapshot = layers.clone();
        drop(layers);
        let _ = save_support_state(
            state,
            &state.map_layer_store,
            &layers_snapshot,
            "map layers",
        );
    }
    let _ = publish_gui_runtime(
        state,
        if request.enabled {
            "map.layer.enabled"
        } else {
            "map.layer.disabled"
        },
        RuntimeEventSeverity::Info,
        "Map layer visibility changed",
        Some(json!({"layer_id": request.layer_id, "enabled": request.enabled})),
        None,
    );
    handle_map_state(state)
}

fn handle_net_control_state(state: &AppState) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &net_control_manifest(),
        PluginCapability::NetView,
        "Net Control view permission check",
    ) {
        return response;
    }
    json_response(&net_control_payload(state))
}

fn net_control_payload(state: &AppState) -> ApiNetControlPayload {
    let projection = rebuild_net_projection_for_gui(state);
    let sessions = projection
        .sessions(true)
        .into_iter()
        .map(api_net_session)
        .collect::<Vec<_>>();
    let active_session = projection.active_session().map(api_net_session);
    let active_id = active_session
        .as_ref()
        .map(|session| session.net_session_id);
    let checkins = active_id
        .map(|session_id| {
            projection
                .checkins_for_session(session_id, false)
                .into_iter()
                .map(api_net_checkin)
                .collect()
        })
        .unwrap_or_default();
    let traffic = active_id
        .map(|session_id| {
            projection
                .traffic_for_session(session_id)
                .into_iter()
                .map(|record| record.payload.clone())
                .collect()
        })
        .unwrap_or_default();
    let report_preview =
        active_id.and_then(|session_id| export_net_report_markdown(&projection, session_id).ok());
    ApiNetControlPayload {
        sessions,
        active_session,
        templates: projection.templates().into_iter().cloned().collect(),
        checkins,
        traffic,
        report_preview,
    }
}

fn rebuild_net_projection_for_gui(state: &AppState) -> NetControlProjection {
    let events = state
        .proposal_runtime
        .block_on(state.store.list_events(state.logbook_id))
        .unwrap_or_default();
    let mut projection = NetControlProjection::new();
    let _ = projection.rebuild(&events);
    projection
}

fn api_net_session(record: &ham_core::NetSessionRecord) -> ApiNetSessionRecord {
    ApiNetSessionRecord {
        net_session_id: record.net_session_id,
        payload: record.payload.clone(),
        status: format!("{:?}", record.status).to_ascii_lowercase(),
        checkin_count: record.checkin_count,
        late_checkin_count: record.late_checkin_count,
        traffic_count: record.traffic_count,
        emergency_traffic_count: record.emergency_traffic_count,
        duplicate_warnings: record.duplicate_warnings.clone(),
    }
}

fn api_net_checkin(record: &ham_core::NetCheckInRecord) -> ApiNetCheckInRecord {
    ApiNetCheckInRecord {
        checkin_id: record.checkin_id,
        payload: record.payload.clone(),
        status: format!("{:?}", record.status).to_ascii_lowercase(),
        traffic: format!("{:?}", record.traffic).to_ascii_lowercase(),
        deleted: record.deleted,
    }
}

fn handle_net_session_start(state: &AppState, body: &[u8]) -> Vec<u8> {
    let request = match serde_json::from_slice::<NetSessionStartRequest>(body) {
        Ok(request) => request,
        Err(_) => return json_error(400, "invalid net session start JSON"),
    };
    let mut payload = json!({
        "station_callsign": request.station_callsign,
        "net_control_operator_id": request.net_control_operator_id,
        "net_name": request.net_name,
        "started_at": chrono::Utc::now().to_rfc3339(),
        "status": "active"
    });
    if let Some(value) = request.frequency_hz {
        payload["frequency_hz"] = json!(value);
    }
    if let Some(value) = request.band {
        payload["band"] = json!(value);
    }
    if let Some(value) = request.mode {
        payload["mode"] = json!(value);
    }
    if let Some(value) = request.notes {
        payload["notes"] = json!(value);
    }
    submit_net_proposal(state, PROPOSAL_NET_SESSION_START, None, payload)
}

fn handle_net_session_end(state: &AppState, body: &[u8]) -> Vec<u8> {
    let request = match serde_json::from_slice::<CredentialIdRequest>(body) {
        Ok(request) => request,
        Err(_) => return json_error(400, "invalid net session end JSON"),
    };
    submit_net_proposal(
        state,
        PROPOSAL_NET_SESSION_END,
        Some(request.credential_id),
        json!({"ended_at": chrono::Utc::now().to_rfc3339()}),
    )
}

fn handle_net_checkin_create(state: &AppState, body: &[u8]) -> Vec<u8> {
    let request = match serde_json::from_slice::<NetCheckInCreateRequest>(body) {
        Ok(request) => request,
        Err(_) => return json_error(400, "invalid net check-in JSON"),
    };
    let mut payload = json!({
        "net_session_id": request.net_session_id,
        "checkin_time": chrono::Utc::now().to_rfc3339(),
        "status": request.status.unwrap_or_else(|| "checked_in".to_owned()),
        "traffic": request.traffic.unwrap_or_else(|| "none".to_owned())
    });
    for (key, value) in [
        ("callsign", request.callsign),
        ("operator_name", request.operator_name),
        ("location", request.location),
        ("grid", request.grid),
        ("tactical_callsign", request.tactical_callsign),
        ("notes", request.notes),
    ] {
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            payload[key] = json!(value);
        }
    }
    submit_net_proposal(state, PROPOSAL_NET_CHECKIN_CREATE, None, payload)
}

fn handle_net_checkin_delete(state: &AppState, body: &[u8]) -> Vec<u8> {
    let request = match serde_json::from_slice::<CredentialIdRequest>(body) {
        Ok(request) => request,
        Err(_) => return json_error(400, "invalid net check-in delete JSON"),
    };
    submit_net_proposal(
        state,
        PROPOSAL_NET_CHECKIN_DELETE,
        Some(request.credential_id),
        json!({"reason": "operator tombstone"}),
    )
}

fn handle_net_traffic_create(state: &AppState, body: &[u8]) -> Vec<u8> {
    let request = match serde_json::from_slice::<NetTrafficCreateRequest>(body) {
        Ok(request) => request,
        Err(_) => return json_error(400, "invalid net traffic JSON"),
    };
    let now = chrono::Utc::now().to_rfc3339();
    let mut payload = json!({
        "net_session_id": request.net_session_id,
        "precedence": request.precedence,
        "summary": request.summary,
        "status": "listed",
        "created_at": now,
        "updated_at": now
    });
    if let Some(value) = request.from_callsign {
        payload["from_callsign"] = json!(value);
    }
    if let Some(value) = request.to_callsign {
        payload["to_callsign"] = json!(value);
    }
    submit_net_proposal(state, PROPOSAL_NET_TRAFFIC_CREATE, None, payload)
}

fn handle_net_report_export(state: &AppState, body: &[u8]) -> Vec<u8> {
    let request = match serde_json::from_slice::<CredentialIdRequest>(body) {
        Ok(request) => request,
        Err(_) => return json_error(400, "invalid net report export JSON"),
    };
    let projection = rebuild_net_projection_for_gui(state);
    let report = match export_net_report_markdown(&projection, request.credential_id) {
        Ok(report) => report,
        Err(error) => {
            return json_response_with_status(
                400,
                &json!({"ok": false, "error": error.to_string()}),
            )
        }
    };
    submit_net_proposal(
        state,
        PROPOSAL_NET_REPORT_EXPORT,
        Some(request.credential_id),
        json!({"format": "markdown", "summary": report}),
    )
}

fn submit_net_proposal(
    state: &AppState,
    proposal_type: &str,
    entity_id: Option<uuid::Uuid>,
    payload: Value,
) -> Vec<u8> {
    let proposal = ProposalEnvelope::new(
        proposal_type,
        state.logbook_id,
        entity_id,
        Some(uuid::Uuid::new_v4()),
        state.bridge.status().device_id,
        "plugin.net-control",
        1,
        payload,
    );
    let result = state.proposal_runtime.block_on(submit_proposal(
        state.store.as_ref(),
        &state.bridge,
        &net_control_context(state),
        proposal,
    ));
    match result {
        Ok(outcome) => {
            let event_name = match proposal_type {
                PROPOSAL_NET_SESSION_START => "net.session.started",
                PROPOSAL_NET_SESSION_END => "net.session.ended",
                PROPOSAL_NET_CHECKIN_CREATE => "net.checkin.accepted",
                PROPOSAL_NET_TRAFFIC_CREATE => "net.traffic.created",
                PROPOSAL_NET_REPORT_EXPORT => "net.report.exported",
                _ => "net.proposal.accepted",
            };
            let _ = publish_gui_runtime(
                state,
                event_name,
                RuntimeEventSeverity::Info,
                "Net Control proposal accepted",
                Some(json!({
                    "official_event_id": outcome.official_event.event_id,
                    "event_type": outcome.official_event.event_type
                })),
                None,
            );
            json_response(
                &json!({"ok": true, "event": outcome.official_event, "net": net_control_payload(state)}),
            )
        }
        Err(error) => {
            let _ = publish_gui_runtime(
                state,
                "net.checkin.rejected",
                RuntimeEventSeverity::Warn,
                "Net Control proposal rejected",
                None,
                Some(error.to_string()),
            );
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
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
    apply_active_station_defaults(state, &mut payload);

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
    apply_active_station_defaults(state, &mut payload);
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
        &pota_sota_context(state),
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
            &pota_sota_context(state),
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

fn apply_active_station_defaults(state: &AppState, payload: &mut Value) {
    let book = state
        .station_book
        .lock()
        .expect("station book mutex should not be poisoned")
        .clone();
    book.apply_defaults_to_qso_payload(payload);
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
        &proposal_context(state),
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
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::AdifImport,
        "ADIF import permission check",
    ) {
        return response;
    }
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
        &proposal_context(state),
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
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::AdifExport,
        "ADIF export permission check",
    ) {
        return response;
    }
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
        &pota_sota_context(state),
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
        &pota_sota_context(state),
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
    if let Err(response) = ensure_gui_permission(
        state,
        &pota_sota_manifest(),
        PluginCapability::AdifExport,
        "Activation ADIF export permission check",
    ) {
        return response;
    }
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
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::SyncLanDiscovery,
        "LAN discovery permission check",
    ) {
        return response;
    }
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
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::SyncLanPull,
        "LAN sync preview permission check",
    ) {
        return response;
    }
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
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::SyncLanPull,
        "LAN sync pull permission check",
    ) {
        return response;
    }
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
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::SyncCloudConnect,
        "Cloud sync connect permission check",
    ) {
        return response;
    }
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
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::SyncCloudPush,
        "Cloud sync push permission check",
    ) {
        return response;
    }
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
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::SyncCloudPull,
        "Cloud sync preview pull permission check",
    ) {
        return response;
    }
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
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::SyncCloudPull,
        "Cloud sync pull permission check",
    ) {
        return response;
    }
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
