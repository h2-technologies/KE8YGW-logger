use std::{
    collections::HashMap,
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::{IpAddr, SocketAddr, TcpListener, TcpStream},
    process,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use ham_core::{
    build_diagnostic_bundle, confirmations_from_adif, default_credential_store,
    default_official_event_log_path, default_service_registry, dx_cluster_spot_to_spot,
    export_adif, export_adif_with_activations, export_diagnostic_zip, export_net_report_markdown,
    grayline_snapshot, import_adif, lookup_callsign_with_service_framework,
    maidenhead_to_coordinate, missing_credential_status, mock_propagation_forecast, mock_weather,
    online_services_dashboard, parse_dx_cluster_line, pota_spot_to_spot, publish_rig_runtime_event,
    qso_map_objects, station_markers_from_profiles, submit_proposal, suggestion_from_rig_state,
    AdifImportOptions, Coordinate, CoreEventEnvelope, CredentialMetadata, CredentialStore,
    DiagnosticBundleInput, DiagnosticReportType, EquipmentItem, EquipmentType,
    JsonPermissionGrantStore, JsonStationBookStore, JsonSupportStore, JsonlLogbookEventStore,
    LocalPrefixProvider, LogbookEventStore, LookupCache, LookupCacheConfig, LookupProviderStatus,
    MapLayerStack, MockRigProvider, NetControlProjection, NewLogbookEvent, NotificationSeverity,
    OnlineAutomationTask, OnlineNotification, OnlineProviderStatus, OperatorRole,
    PermissionGrantSet, PermissionGrantStatus, PermissionRegistry, PermissionSettings,
    PotaSpotRecord, Projection, ProposalContext, ProposalOutcome, RigConnectionStatus, RigDevice,
    RigProvider, RigProviderStatus, RigState, RuntimeEventFilter, RuntimeEventSeverity,
    RuntimeLogConfig, ServiceCache, ServiceCacheEntry, ServiceRegistry, ServiceRegistrySnapshot,
    StationBook, StationConfiguration, StationProfile, UploadQueue, UploadTarget,
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
    build_handshake_response, conflict_report_from_preview, lan_auth_signature, metadata_for_event,
    preview_pull_from_events, pull_missing_events, verify_lan_auth_signature, CloudAuth,
    CloudConnectionState, CloudPreviewPullRequest, CloudPullEventsRequest, CloudPullEventsResponse,
    CloudPushEventsRequest, CloudPushEventsResponse, CloudServerConfig, CloudSyncConfig,
    CloudSyncStatusResponse, ConflictReviewSnapshot, DiagnosticReportUploadRequest,
    DiagnosticReportUploadResponse, DiagnosticReportUploadType, DiscoveryPacket,
    GetEventMetadataResponse, GetEventRangeResponse, HandshakeRequest, InMemoryCloudSyncServer,
    JsonConflictReviewStore, JsonLanTrustStore, JsonOfflineMutationQueue, LanDiscoveryService,
    LanPairingAcceptance, LanPeerTrustUpdate, LanTrustSnapshot, ListLogbooksResponse,
    LocalPeerIdentity, LogbookHeadSummary, ManualConflictResolution,
    ManualConflictResolutionChoice, OfflineMutationEnvelope, OfflineMutationInput,
    OfflineQueueSnapshot, PairDeviceRequest, PeerObservation, PeerRecord, PeerRegistry,
    PreviewPullRequest, PreviewPullResponse, PullEventsRequest, PullEventsResponse,
    ReplicationStatus, SyncConfig, SyncConflictReport, LAN_AUTH_SIGNATURE_VERSION,
    OFFLINE_OP_ACTIVATION_END, OFFLINE_OP_ACTIVATION_START, OFFLINE_OP_NET_CHECKIN_CREATE,
    OFFLINE_OP_NET_CHECKIN_DELETE, OFFLINE_OP_NET_SESSION_END, OFFLINE_OP_NET_SESSION_START,
    OFFLINE_OP_NET_TRAFFIC_CREATE, OFFLINE_OP_QSO_CREATE, OFFLINE_OP_QSO_DELETE,
    OFFLINE_OP_QSO_NOTE_ADD, OFFLINE_OP_QSO_RESTORE, OFFLINE_OP_STATION_PROFILE_SELECT,
    PROTOCOL_NAME, PROTOCOL_VERSION,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

const INDEX_HTML: &str = include_str!("../web/index.html");
const APP_CSS: &str = include_str!("../web/styles.css");
const APP_JS: &str = include_str!("../web/app.js");
const LAN_DISCOVERY_LISTEN_WINDOW: Duration = Duration::from_millis(750);
const LAN_DISCOVERY_SLEEP_SLICE: Duration = Duration::from_millis(250);
const LAN_AUTH_DEVICE_ID_HEADER: &str = "x-ke8ygw-lan-device-id";
const LAN_AUTH_REPLAY_NONCE_HEADER: &str = "x-ke8ygw-lan-replay-nonce";
const LAN_AUTH_SIGNATURE_VERSION_HEADER: &str = "x-ke8ygw-lan-signature-version";
const LAN_AUTH_SIGNATURE_HEADER: &str = "x-ke8ygw-lan-signature";

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
    let offline_queue = JsonOfflineMutationQueue::new(support_dir.join("offline-mutations.json"));
    let conflict_review_store =
        JsonConflictReviewStore::new(support_dir.join("conflict-reviews.json"));
    let lan_trust_store = JsonLanTrustStore::new(support_dir.join("lan-trust.json"));
    let mut station_book = station_store.load().unwrap_or_default();
    if station_book.profiles.is_empty() {
        seed_default_station_book(&mut station_book);
        let _ = station_store.save(&station_book);
    }
    let allow_insecure_dev_credentials = env::var("HAM_PLATFORM_ALLOW_INSECURE_DEV_CREDENTIALS")
        .ok()
        .as_deref()
        == Some("1");
    let credential_store: Box<dyn CredentialStore> =
        default_credential_store(&support_dir, allow_insecure_dev_credentials);
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
    match offline_queue.recover_interrupted_writes(chrono::Utc::now()) {
        Ok(recovered) if recovered > 0 => {
            let _ = bridge.publish(RuntimeEventInput {
                event_type: "sync.offline_queue.recovered".to_owned(),
                severity: RuntimeEventSeverity::Info,
                source: "ham-sync".to_owned(),
                source_plugin_id: None,
                workspace_id: Some("dashboard".to_owned()),
                payload_summary: format!("Recovered {recovered} interrupted offline sync attempts"),
                redacted_payload: Some(json!({"recovered_count": recovered})),
                error: None,
            });
        }
        Ok(_) => {}
        Err(error) => {
            let _ = bridge.publish(RuntimeEventInput {
                event_type: "sync.offline_queue.recovery_failed".to_owned(),
                severity: RuntimeEventSeverity::Warn,
                source: "ham-sync".to_owned(),
                source_plugin_id: None,
                workspace_id: Some("dashboard".to_owned()),
                payload_summary: "Offline queue recovery failed".to_owned(),
                redacted_payload: None,
                error: Some(error.to_string()),
            });
        }
    }

    let state = Arc::new(AppState {
        bridge,
        store,
        logbook_id,
        proposal_runtime,
        sync: Mutex::new(SyncUiState::new(bound_addr.clone())),
        offline_queue,
        conflict_review_store,
        lan_trust_store,
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
    offline_queue: JsonOfflineMutationQueue,
    conflict_review_store: JsonConflictReviewStore,
    lan_trust_store: JsonLanTrustStore,
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
    discovery_generation: u64,
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
            discovery_generation: 0,
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
        ("POST", "/api/backup/export") => handle_backup_export(&state, &request.body),
        ("POST", "/api/backup/import/dry-run") => {
            handle_backup_import_dry_run(&state, &request.body)
        }
        ("POST", "/api/backup/import") => handle_backup_import(&state, &request.body),
        ("GET", "/api/sync/divergence/review") => handle_local_divergence_review(&state),
        ("POST", "/api/sync/divergence/export") => {
            handle_local_divergence_export(&state, &request.body)
        }
        ("GET", "/api/diagnostics/report/last") => json_response(
            &state
                .last_report
                .lock()
                .expect("last report mutex should not be poisoned")
                .clone(),
        ),
        ("GET", "/api/sync/state") => json_response(&sync_state_payload(&state)),
        ("GET", "/api/sync/list-logbooks") => handle_sync_list_logbooks(&state, &request),
        ("GET", "/api/sync/get-head") => handle_sync_get_head(&state, query, &request),
        ("GET", "/api/sync/events-since") => handle_sync_events_since(&state, query, &request),
        ("GET", "/api/sync/event-metadata") => handle_sync_event_metadata(&state, query, &request),
        ("POST", "/api/sync/discovery/start") => handle_sync_discovery(&state, true),
        ("POST", "/api/sync/discovery/stop") => handle_sync_discovery(&state, false),
        ("POST", "/api/sync/peers/refresh") => handle_sync_refresh(&state),
        ("POST", "/api/sync/peers/add") => handle_sync_add_peer(&state, &request.body),
        ("POST", "/api/sync/handshake") => handle_sync_handshake(&state, &request.body),
        ("POST", "/api/sync/preview-pull") => handle_sync_preview_pull(&state, &request.body),
        ("POST", "/api/sync/pull-events") => handle_sync_pull_events(&state, &request.body),
        ("GET", "/api/sync/offline-queue") => handle_offline_queue_state(&state),
        ("POST", "/api/sync/offline-queue/recover") => handle_offline_queue_recover(&state),
        ("GET", "/api/sync/conflict-reviews") => handle_conflict_reviews_state(&state),
        ("POST", "/api/sync/conflict-reviews/create") => handle_conflict_review_create(&state),
        ("POST", "/api/sync/conflict-reviews/resolve") => {
            handle_conflict_review_resolve(&state, &request.body)
        }
        ("GET", "/api/sync/lan/trust") => handle_lan_trust_state(&state),
        ("POST", "/api/sync/lan/pairing-token") => handle_lan_pairing_token(&state, &request.body),
        ("POST", "/api/sync/lan/pairing-accept") => {
            handle_lan_pairing_accept(&state, &request.body)
        }
        ("POST", "/api/sync/lan/pairing-complete") => {
            handle_lan_pairing_complete(&state, &request.body)
        }
        ("POST", "/api/sync/lan/revoke") => handle_lan_trust_revoke(&state, &request.body),
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
    headers: HashMap<String, String>,
    body: Vec<u8>,
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
struct BackupImportRequest {
    path: String,
    confirm_dry_run: Option<bool>,
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
    replay_nonce: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ManualLanPeerRequest {
    address: String,
}

#[derive(Debug, Deserialize)]
struct LanPairingTokenRequest {
    approved_by_operator: bool,
    display_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LanPairingAcceptRequest {
    token_id: uuid::Uuid,
    pairing_code: String,
    peer_device_id: uuid::Uuid,
    peer_display_name: String,
    logbook_id: Option<uuid::Uuid>,
    public_key_fingerprint: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LanPairingCompleteRequest {
    peer_id: Option<String>,
    token_id: uuid::Uuid,
    pairing_code: String,
    public_key_fingerprint: Option<String>,
}

#[derive(Debug, Deserialize)]
struct LanTrustRevokeRequest {
    device_id: uuid::Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LanEndpointAuth {
    device_id: uuid::Uuid,
    replay_nonce: String,
    signature: String,
}

#[derive(Debug, Deserialize)]
struct ConflictReviewResolveRequest {
    review_id: uuid::Uuid,
    resolution: ManualConflictResolution,
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
    offline_queue: Option<OfflineQueueSnapshot>,
    offline_queue_error: Option<String>,
    conflict_reviews: Option<ConflictReviewSnapshot>,
    conflict_reviews_error: Option<String>,
    lan_trust: Option<LanTrustSnapshot>,
    lan_trust_error: Option<String>,
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
        403 => "Forbidden",
        409 => "Conflict",
        502 => "Bad Gateway",
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
    let now = chrono::Utc::now();
    let device_id = state.bridge.status().device_id;
    let offline_mutation = match state.offline_queue.enqueue_input(
        OfflineMutationInput::new(
            state.logbook_id,
            device_id,
            device_id,
            OFFLINE_OP_STATION_PROFILE_SELECT,
            json!({"station_profile_id": request.station_profile_id}),
        )
        .with_idempotency_key(format!(
            "station.profile.select:{}:{}",
            request.station_profile_id,
            uuid::Uuid::new_v4()
        )),
        now,
    ) {
        Ok(mutation) => mutation,
        Err(error) => {
            return json_response_with_status(
                500,
                &json!({"ok": false, "error": format!("failed to persist offline mutation before station update: {error}")}),
            )
        }
    };
    let mut book = state
        .station_book
        .lock()
        .expect("station book mutex should not be poisoned");
    if let Err(error) = book.select_profile(request.station_profile_id) {
        let _ = state.offline_queue.mark_user_action_required(
            offline_mutation.operation_id,
            error.to_string(),
            Some("station_profile_invalid".to_owned()),
            chrono::Utc::now(),
        );
        return json_error(400, error.to_string());
    }
    if let Err(error) = state.station_store.save(&book) {
        let _ = state.offline_queue.mark_user_action_required(
            offline_mutation.operation_id,
            error.to_string(),
            Some("station_support_save_failed".to_owned()),
            chrono::Utc::now(),
        );
        return json_response_with_status(
            500,
            &json!({"ok": false, "error": format!("failed to save station support state: {error}")}),
        );
    }
    let accepted_mutation = state
        .offline_queue
        .mark_accepted(offline_mutation.operation_id, chrono::Utc::now())
        .unwrap_or(offline_mutation);
    let _ = publish_gui_runtime(
        state,
        "station.profile.selected",
        RuntimeEventSeverity::Info,
        "Station profile selected",
        Some(json!({"station_profile_id": request.station_profile_id})),
        None,
    );
    json_response(&json!({"ok": true, "station": &*book, "offline_mutation": accepted_mutation}))
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
            "lotw": "deferred_tqsl_certificate_flow",
            "eqsl": "fake_default_live_upload_gated",
            "clublog": "fake_default_live_upload_gated",
            "qrz-logbook": "fake_default_live_upload_gated",
            "qrz-xml": "hosted_lookup_fake_default_live_gated",
            "hamqth": "hosted_lookup_fake_default_live_gated",
            "hrdlog": "fake_only",
            "dx-cluster": "read_once_lifecycle_fake_default_live_gated",
            "pota-spots": "hosted_spot_fetch_fake_default_live_gated",
            "sotawatch": "deferred_api_approval_terms",
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
    let result = submit_offline_tracked_proposal(
        state,
        proposal_type,
        entity_id,
        Some(uuid::Uuid::new_v4()),
        "plugin.net-control",
        net_control_context(state),
        payload,
    );
    match result {
        Ok((outcome, offline_mutation, offline_queue_warning)) => {
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
            json_response(&json!({
                "ok": true,
                "event": outcome.official_event,
                "offline_mutation": offline_mutation,
                "offline_queue_warning": offline_queue_warning,
                "net": net_control_payload(state)
            }))
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
            json_response_with_status(400, &json!({"ok": false, "error": error}))
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
    let (qso, offline_mutation, offline_queue_warning) = match submit_offline_tracked_proposal(
        state,
        PROPOSAL_QSO_CREATE,
        None,
        None,
        "plugin.pota-sota",
        pota_sota_context(state),
        payload,
    ) {
        Ok((outcome, offline_mutation, offline_queue_warning)) => {
            (outcome, offline_mutation, offline_queue_warning)
        }
        Err(error) => return json_response_with_status(400, &json!({"ok": false, "error": error})),
    };
    if let (Some(active), Some(qso_id)) = (
        activation_projection_payload(state).active_activation,
        qso.official_event.entity_id,
    ) {
        let link_payload = json!({"activation_id": active.activation_id});
        let _ = submit_offline_tracked_proposal(
            state,
            PROPOSAL_QSO_ACTIVATION_LINK,
            Some(qso_id),
            None,
            "plugin.pota-sota",
            pota_sota_context(state),
            link_payload,
        );
    }
    json_response(&json!({
        "ok": true,
        "event": qso.official_event,
        "offline_mutation": offline_mutation,
        "offline_queue_warning": offline_queue_warning,
        "projection": qso_projection_payload(state, "include_deleted=true"),
        "activations": activation_projection_payload(state)
    }))
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

fn offline_operation_type_for_proposal(proposal_type: &str) -> String {
    match proposal_type {
        PROPOSAL_QSO_CREATE => OFFLINE_OP_QSO_CREATE,
        PROPOSAL_QSO_DELETE => OFFLINE_OP_QSO_DELETE,
        PROPOSAL_QSO_RESTORE => OFFLINE_OP_QSO_RESTORE,
        PROPOSAL_QSO_NOTE_ADD => OFFLINE_OP_QSO_NOTE_ADD,
        PROPOSAL_ACTIVATION_START => OFFLINE_OP_ACTIVATION_START,
        PROPOSAL_ACTIVATION_END => OFFLINE_OP_ACTIVATION_END,
        PROPOSAL_NET_SESSION_START => OFFLINE_OP_NET_SESSION_START,
        PROPOSAL_NET_SESSION_END => OFFLINE_OP_NET_SESSION_END,
        PROPOSAL_NET_CHECKIN_CREATE => OFFLINE_OP_NET_CHECKIN_CREATE,
        PROPOSAL_NET_CHECKIN_DELETE => OFFLINE_OP_NET_CHECKIN_DELETE,
        PROPOSAL_NET_TRAFFIC_CREATE => OFFLINE_OP_NET_TRAFFIC_CREATE,
        _ => proposal_type,
    }
    .to_owned()
}

fn submit_offline_tracked_proposal(
    state: &AppState,
    proposal_type: &str,
    entity_id: Option<uuid::Uuid>,
    author_operator_id: Option<uuid::Uuid>,
    source_plugin_id: &str,
    context: ProposalContext,
    payload: Value,
) -> Result<(ProposalOutcome, OfflineMutationEnvelope, Option<String>), String> {
    let now = chrono::Utc::now();
    let device_id = state.bridge.status().device_id;
    let operation_id = uuid::Uuid::new_v4();
    let queued = state
        .offline_queue
        .enqueue_input(
            OfflineMutationInput::new(
                state.logbook_id,
                device_id,
                device_id,
                offline_operation_type_for_proposal(proposal_type),
                payload.clone(),
            )
            .with_operation_id(operation_id)
            .with_correlation_id(operation_id)
            .with_idempotency_key(format!("{proposal_type}:{operation_id}")),
            now,
        )
        .map_err(|error| format!("failed to persist offline mutation before submit: {error}"))?;

    let proposal = ProposalEnvelope::new(
        proposal_type,
        state.logbook_id,
        entity_id,
        author_operator_id,
        device_id,
        source_plugin_id,
        1,
        payload,
    );
    let result = state.proposal_runtime.block_on(submit_proposal(
        state.store.as_ref(),
        &state.bridge,
        &context,
        proposal,
    ));
    match result {
        Ok(outcome) => {
            let queue_warning = state
                .offline_queue
                .record_local_event(
                    queued.operation_id,
                    &outcome.official_event,
                    chrono::Utc::now(),
                )
                .err()
                .map(|error| error.to_string());
            Ok((outcome, queued, queue_warning))
        }
        Err(error) => {
            let _ = state.offline_queue.mark_user_action_required(
                queued.operation_id,
                error.to_string(),
                Some("domain_validation_failed".to_owned()),
                chrono::Utc::now(),
            );
            Err(error.to_string())
        }
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
    match submit_offline_tracked_proposal(
        state,
        proposal_type,
        qso_id,
        None,
        "core.gui",
        proposal_context(state),
        payload,
    ) {
        Ok((outcome, offline_mutation, offline_queue_warning)) => json_response(&json!({
            "ok": true,
            "event": outcome.official_event,
            "offline_mutation": offline_mutation,
            "offline_queue_warning": offline_queue_warning,
            "projection": qso_projection_payload(state, "include_deleted=true")
        })),
        Err(error) => json_response_with_status(
            400,
            &json!({
                "ok": false,
                "error": error
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

fn handle_backup_export(state: &AppState, body: &[u8]) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<PathRequest>(body) else {
        return json_error(400, "invalid backup export JSON");
    };
    let events = match state
        .proposal_runtime
        .block_on(state.store.list_events(state.logbook_id))
    {
        Ok(events) => events,
        Err(error) => return json_error(400, error.to_string()),
    };
    let head_hash = events.last().map(|event| event.event_hash.clone());
    let station_book = state
        .station_book
        .lock()
        .expect("station book mutex should not be poisoned")
        .clone();
    let upload_queue = state
        .upload_queue
        .lock()
        .expect("upload queue mutex should not be poisoned")
        .clone();
    let map_layers = state
        .map_layers
        .lock()
        .expect("map layers mutex should not be poisoned")
        .clone();
    let service_registry = state
        .service_registry
        .lock()
        .expect("service registry mutex should not be poisoned")
        .clone();
    let payload = json!({
        "manifest": {
            "format_version": 1,
            "created_at": chrono::Utc::now(),
            "app_version": env!("CARGO_PKG_VERSION"),
            "logbook_id": state.logbook_id,
            "head_hash": head_hash,
            "event_count": events.len(),
            "included_sections": [
                "official_events",
                "station_book",
                "upload_queue",
                "map_layers",
                "service_registry_without_secrets"
            ],
            "excluded_sections": [
                "credential_secret_values",
                "raw_session_tokens",
                "device_tokens",
                "runtime_logs"
            ]
        },
        "official_events": events,
        "station_book": station_book,
        "upload_queue": upload_queue,
        "map_layers": map_layers,
        "service_registry": service_registry
    });
    if payload
        .to_string()
        .to_ascii_lowercase()
        .contains("test-secret")
    {
        return json_error(400, "backup payload contains a secret-like test value");
    }
    let encoded = match serde_json::to_string_pretty(&payload) {
        Ok(encoded) => encoded,
        Err(error) => return json_error(400, error.to_string()),
    };
    if let Err(error) = fs::write(&request.path, encoded) {
        return json_error(400, format!("failed to write backup: {error}"));
    }
    json_response(&json!({
        "ok": true,
        "path": request.path,
        "manifest": payload["manifest"],
        "excluded_sensitive_sections": payload["manifest"]["excluded_sections"]
    }))
}

fn handle_backup_import_dry_run(state: &AppState, body: &[u8]) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<BackupImportRequest>(body) else {
        return json_error(400, "invalid backup import JSON");
    };
    let backup = match read_backup_payload(&request.path) {
        Ok(backup) => backup,
        Err(response) => return response,
    };
    json_response(&local_backup_dry_run_payload(state, &backup))
}

fn handle_backup_import(state: &AppState, body: &[u8]) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<BackupImportRequest>(body) else {
        return json_error(400, "invalid backup import JSON");
    };
    if request.confirm_dry_run != Some(true) {
        return json_error(
            400,
            "confirm_dry_run must be true after reviewing the dry-run result",
        );
    }
    let backup = match read_backup_payload(&request.path) {
        Ok(backup) => backup,
        Err(response) => return response,
    };
    let dry_run = local_backup_dry_run_payload(state, &backup);
    if dry_run.get("ok").and_then(Value::as_bool) != Some(true) {
        return json_response_with_status(400, &dry_run);
    }
    let events = match backup_events(&backup) {
        Ok(events) => events,
        Err(error) => return json_error(400, error),
    };
    let existing = match state
        .proposal_runtime
        .block_on(state.store.list_events(state.logbook_id))
    {
        Ok(events) => events,
        Err(error) => return json_error(400, error.to_string()),
    };
    if existing.len() > events.len() {
        return json_error(
            400,
            "target logbook is ahead of the backup; use divergence review",
        );
    }
    for (index, event) in existing.iter().enumerate() {
        if events.get(index) != Some(event) {
            return json_error(400, "target logbook diverges from backup");
        }
    }
    let skipped_duplicate_count = existing.len();
    let mut imported_count = 0usize;
    for event in events.into_iter().skip(skipped_duplicate_count) {
        if let Err(error) = state
            .proposal_runtime
            .block_on(state.store.append_verified_remote_event(event))
        {
            return json_error(400, error.to_string());
        }
        imported_count += 1;
    }
    if let Some(station_book) = backup.get("station_book") {
        if let Ok(book) = serde_json::from_value::<StationBook>(station_book.clone()) {
            *state
                .station_book
                .lock()
                .expect("station book mutex should not be poisoned") = book.clone();
            let _ = state.station_store.save(&book);
        }
    }
    if let Some(upload_queue) = backup.get("upload_queue") {
        if let Ok(queue) = serde_json::from_value::<UploadQueue>(upload_queue.clone()) {
            *state
                .upload_queue
                .lock()
                .expect("upload queue mutex should not be poisoned") = queue.clone();
            let _ = state.upload_queue_store.save(&queue);
        }
    }
    if let Some(map_layers) = backup.get("map_layers") {
        if let Ok(layers) = serde_json::from_value::<MapLayerStack>(map_layers.clone()) {
            *state
                .map_layers
                .lock()
                .expect("map layers mutex should not be poisoned") = layers.clone();
            let _ = state.map_layer_store.save(&layers);
        }
    }
    let projection = match state
        .proposal_runtime
        .block_on(state.store.rebuild_projections(state.logbook_id))
    {
        Ok(projection) => projection,
        Err(error) => return json_error(400, error.to_string()),
    };
    json_response(&json!({
        "ok": true,
        "imported_official_events_count": imported_count,
        "skipped_duplicate_count": skipped_duplicate_count,
        "restored_support_sections": backup_support_sections(&backup),
        "final_chain_head": state.proposal_runtime.block_on(state.store.get_head(state.logbook_id)).ok().flatten(),
        "projection_rebuild": {
            "ok": true,
            "qso_count": projection.list(false).len()
        },
        "manual_review_needed": false
    }))
}

fn read_backup_payload(path: &str) -> Result<Value, Vec<u8>> {
    let text = fs::read_to_string(path)
        .map_err(|error| json_error(400, format!("failed to read backup: {error}")))?;
    serde_json::from_str(&text)
        .map_err(|error| json_error(400, format!("failed to parse backup JSON: {error}")))
}

fn local_backup_dry_run_payload(state: &AppState, backup: &Value) -> Value {
    let mut errors = Vec::new();
    let warnings = Vec::<String>::new();
    let Some(manifest) = backup.get("manifest") else {
        return json!({"ok": false, "errors": ["backup manifest is required"], "warnings": warnings});
    };
    if manifest.get("format_version").and_then(Value::as_u64) != Some(1) {
        errors.push("unsupported backup format_version".to_owned());
    }
    let manifest_logbook = manifest
        .get("logbook_id")
        .and_then(Value::as_str)
        .and_then(|value| uuid::Uuid::parse_str(value).ok());
    if manifest_logbook != Some(state.logbook_id) {
        errors.push("backup logbook_id does not match this local profile".to_owned());
    }
    let events = match backup_events(backup) {
        Ok(events) => events,
        Err(error) => {
            errors.push(error);
            Vec::new()
        }
    };
    let existing = state
        .proposal_runtime
        .block_on(state.store.list_events(state.logbook_id))
        .unwrap_or_default();
    let mut duplicate_count = 0usize;
    if existing.len() > events.len() {
        errors.push("target logbook is ahead of the backup".to_owned());
    }
    for (index, event) in existing.iter().enumerate() {
        if events.get(index) == Some(event) {
            duplicate_count += 1;
        } else {
            errors.push("target logbook diverges from backup".to_owned());
            break;
        }
    }
    json!({
        "ok": errors.is_empty(),
        "errors": errors,
        "warnings": warnings,
        "backup_version": manifest.get("format_version").cloned().unwrap_or(Value::Null),
        "source_logbook_id": manifest.get("logbook_id").cloned().unwrap_or(Value::Null),
        "target_logbook_id": state.logbook_id,
        "official_event_count": events.len(),
        "skipped_duplicate_count": duplicate_count,
        "support_sections": backup_support_sections(backup),
        "missing_credentials": ["provider credentials are not restored from backups"],
        "would_import": errors.is_empty(),
        "requires_manual_review": !errors.is_empty()
    })
}

fn backup_events(backup: &Value) -> Result<Vec<CoreEventEnvelope>, String> {
    let events = backup
        .get("official_events")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let events: Vec<CoreEventEnvelope> = serde_json::from_value(events)
        .map_err(|error| format!("official_events could not deserialize: {error}"))?;
    let mut previous_hash = None;
    let mut seen = std::collections::HashSet::new();
    for event in &events {
        if !seen.insert(event.event_id) {
            return Err(format!("event {} appears more than once", event.event_id));
        }
        if !event.hash_is_valid() {
            return Err(format!("event {} has an invalid hash", event.event_id));
        }
        if event.previous_hash != previous_hash {
            return Err(format!(
                "event {} breaks previous_hash continuity",
                event.event_id
            ));
        }
        previous_hash = Some(event.event_hash.clone());
    }
    Ok(events)
}

fn backup_support_sections(backup: &Value) -> Vec<String> {
    [
        "station_book",
        "upload_queue",
        "map_layers",
        "service_registry",
    ]
    .into_iter()
    .filter(|section| !backup.get(section).unwrap_or(&Value::Null).is_null())
    .map(str::to_owned)
    .collect()
}

fn handle_local_divergence_review(state: &AppState) -> Vec<u8> {
    json_response(&local_divergence_review_payload(state))
}

fn handle_local_divergence_export(state: &AppState, body: &[u8]) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<PathRequest>(body) else {
        return json_error(400, "invalid divergence export JSON");
    };
    let report = local_divergence_review_payload(state);
    let encoded = match serde_json::to_string_pretty(&report) {
        Ok(encoded) => encoded,
        Err(error) => return json_error(400, error.to_string()),
    };
    if let Err(error) = fs::write(&request.path, encoded) {
        return json_error(400, format!("failed to write divergence report: {error}"));
    }
    json_response(&json!({"ok": true, "path": request.path, "report": report}))
}

fn current_conflict_report(state: &AppState) -> Option<SyncConflictReport> {
    let offline_mutations = state
        .offline_queue
        .load_snapshot(chrono::Utc::now())
        .map(|snapshot| snapshot.mutations)
        .unwrap_or_default();
    let sync = state
        .sync
        .lock()
        .expect("sync mutex should not be poisoned");
    sync.latest_preview
        .as_ref()
        .or(sync.latest_cloud_preview.as_ref())
        .map(|preview| {
            conflict_report_from_preview(preview, &offline_mutations, chrono::Utc::now())
        })
}

fn local_divergence_review_payload(state: &AppState) -> Value {
    let conflict_report = current_conflict_report(state);
    let local_head = state
        .proposal_runtime
        .block_on(state.store.get_head(state.logbook_id))
        .ok()
        .flatten();
    let sync = state
        .sync
        .lock()
        .expect("sync mutex should not be poisoned");
    let remote_head = sync
        .latest_cloud_status
        .as_ref()
        .and_then(|status| status.accessible_logbooks.first())
        .and_then(|head| head.head_hash.clone());
    let divergence = sync
        .divergence
        .clone()
        .or_else(|| sync.cloud_divergence.clone());
    json!({
        "created_at": chrono::Utc::now(),
        "logbook_id": state.logbook_id,
        "local_head_hash": local_head,
        "remote_head_hash": remote_head,
        "common_ancestor": if divergence.is_some() { Value::Null } else { json!(local_head) },
        "missing_local_event_count": sync.latest_preview.as_ref().map(|preview| preview.missing_event_count).unwrap_or(0),
        "missing_remote_event_count": sync.latest_cloud_preview.as_ref().map(|preview| preview.missing_event_count).unwrap_or(0),
        "can_safely_pull": sync.latest_preview.as_ref().is_some_and(|preview| matches!(preview.status, ReplicationStatus::RemoteAhead | ReplicationStatus::InSync)),
        "can_safely_push": sync.cloud_divergence.is_none(),
        "divergence_detected": divergence.is_some(),
        "revoked_device_state": "unknown in local GUI mode",
        "conflict_report": conflict_report,
        "recommended_action": divergence.unwrap_or_else(|| "No divergence detected; use normal preview/push/pull controls.".to_owned())
    })
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
    match submit_offline_tracked_proposal(
        state,
        PROPOSAL_ACTIVATION_START,
        None,
        None,
        "plugin.pota-sota",
        pota_sota_context(state),
        payload,
    ) {
        Ok((outcome, offline_mutation, offline_queue_warning)) => {
            let _ = publish_gui_runtime(
                state,
                "activation.started",
                RuntimeEventSeverity::Info,
                "Portable activation started",
                Some(json!(&outcome.official_event)),
                None,
            );
            json_response(&json!({
                "ok": true,
                "event": outcome.official_event,
                "offline_mutation": offline_mutation,
                "offline_queue_warning": offline_queue_warning,
                "activations": activation_projection_payload(state)
            }))
        }
        Err(error) => json_response_with_status(400, &json!({"ok": false, "error": error})),
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
    match submit_offline_tracked_proposal(
        state,
        PROPOSAL_ACTIVATION_END,
        Some(request.activation_id),
        None,
        "plugin.pota-sota",
        pota_sota_context(state),
        json!({"started_at": started_at, "ended_at": chrono::Utc::now().to_rfc3339()}),
    ) {
        Ok((outcome, offline_mutation, offline_queue_warning)) => {
            let _ = publish_gui_runtime(
                state,
                "activation.ended",
                RuntimeEventSeverity::Info,
                "Portable activation ended",
                Some(json!(&outcome.official_event)),
                None,
            );
            json_response(&json!({
                "ok": true,
                "event": outcome.official_event,
                "offline_mutation": offline_mutation,
                "offline_queue_warning": offline_queue_warning,
                "activations": activation_projection_payload(state)
            }))
        }
        Err(error) => json_response_with_status(400, &json!({"ok": false, "error": error})),
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
    let now = chrono::Utc::now();
    let (offline_queue, offline_queue_error) = match state.offline_queue.load_snapshot(now) {
        Ok(snapshot) => (Some(snapshot), None),
        Err(error) => (None, Some(error.to_string())),
    };
    let (conflict_reviews, conflict_reviews_error) =
        match state.conflict_review_store.load_snapshot() {
            Ok(snapshot) => (Some(snapshot), None),
            Err(error) => (None, Some(error.to_string())),
        };
    let (lan_trust, lan_trust_error) = match state.lan_trust_store.snapshot() {
        Ok(snapshot) => (Some(snapshot), None),
        Err(error) => (None, Some(error.to_string())),
    };
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
        offline_queue,
        offline_queue_error,
        conflict_reviews,
        conflict_reviews_error,
        lan_trust,
        lan_trust_error,
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

fn handle_offline_queue_state(state: &AppState) -> Vec<u8> {
    match state.offline_queue.load_snapshot(chrono::Utc::now()) {
        Ok(snapshot) => json_response(&json!({"ok": true, "offline_queue": snapshot})),
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_offline_queue_recover(state: &AppState) -> Vec<u8> {
    let now = chrono::Utc::now();
    match state.offline_queue.recover_interrupted_writes(now) {
        Ok(recovered_count) => {
            let snapshot = state.offline_queue.load_snapshot(now).ok();
            let _ = publish_gui_runtime(
                state,
                "sync.offline_queue.recovered",
                RuntimeEventSeverity::Info,
                &format!("Recovered {recovered_count} interrupted offline sync attempts"),
                Some(json!({"recovered_count": recovered_count})),
                None,
            );
            json_response(
                &json!({"ok": true, "recovered_count": recovered_count, "offline_queue": snapshot}),
            )
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_conflict_reviews_state(state: &AppState) -> Vec<u8> {
    match state.conflict_review_store.load_snapshot() {
        Ok(snapshot) => json_response(&json!({"ok": true, "conflict_reviews": snapshot})),
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_conflict_review_create(state: &AppState) -> Vec<u8> {
    let Some(report) = current_conflict_report(state) else {
        return json_response_with_status(
            409,
            &json!({"ok": false, "error": "no sync conflict report is available; run preview first"}),
        );
    };
    let now = chrono::Utc::now();
    match state.conflict_review_store.create_review(report, now) {
        Ok(review) => {
            let snapshot = state.conflict_review_store.load_snapshot().ok();
            let _ = publish_gui_runtime(
                state,
                "sync.conflict_review.created",
                RuntimeEventSeverity::Warn,
                "Manual sync conflict review recorded",
                Some(json!({
                    "review_id": review.review_id,
                    "logbook_id": review.report.logbook_id,
                    "status": review.report.status,
                    "conflict_count": review.report.conflicts.len()
                })),
                None,
            );
            json_response(&json!({
                "ok": true,
                "conflict_review": review,
                "conflict_reviews": snapshot
            }))
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_conflict_review_resolve(state: &AppState, body: &[u8]) -> Vec<u8> {
    let Ok(request) = serde_json::from_slice::<ConflictReviewResolveRequest>(body) else {
        return json_error(400, "invalid conflict review resolution JSON");
    };
    let now = chrono::Utc::now();
    match state
        .conflict_review_store
        .resolve_review(request.review_id, request.resolution, now)
    {
        Ok(review) => {
            let marked_user_action = mark_conflict_review_user_action(state, &review, now);
            let snapshot = state.conflict_review_store.load_snapshot().ok();
            let choice = review
                .selected_resolution
                .as_ref()
                .map(|resolution| resolution.choice);
            let _ = publish_gui_runtime(
                state,
                "sync.conflict_review.resolved",
                RuntimeEventSeverity::Info,
                "Manual sync conflict review resolved",
                Some(json!({
                    "review_id": review.review_id,
                    "choice": choice,
                    "marked_user_action_count": marked_user_action
                })),
                None,
            );
            json_response(&json!({
                "ok": true,
                "conflict_review": review,
                "marked_user_action_count": marked_user_action,
                "conflict_reviews": snapshot
            }))
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn mark_conflict_review_user_action(
    state: &AppState,
    review: &ham_sync::ManualConflictReview,
    now: chrono::DateTime<chrono::Utc>,
) -> usize {
    if !review
        .selected_resolution
        .as_ref()
        .is_some_and(|resolution| {
            resolution.choice == ManualConflictResolutionChoice::MarkUserActionRequired
        })
    {
        return 0;
    }
    let mut marked = 0;
    for operation_id in review
        .report
        .conflicts
        .iter()
        .flat_map(|conflict| conflict.related_operation_ids.iter().copied())
    {
        if state
            .offline_queue
            .mark_user_action_required(
                operation_id,
                format!("manual conflict review {}", review.review_id),
                Some("manual_conflict_review".to_owned()),
                now,
            )
            .is_ok()
        {
            marked += 1;
        }
    }
    marked
}

fn handle_lan_trust_state(state: &AppState) -> Vec<u8> {
    match state.lan_trust_store.snapshot() {
        Ok(snapshot) => json_response(&json!({"ok": true, "lan_trust": snapshot})),
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_lan_pairing_token(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::SyncLanDiscovery,
        "LAN pairing token permission check",
    ) {
        return response;
    }
    let Ok(request) = serde_json::from_slice::<LanPairingTokenRequest>(body) else {
        return json_error(400, "invalid LAN pairing token JSON");
    };
    let display_name = request
        .display_name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "KE8YGW Logger Local".to_owned());
    let device_id = local_sync_identity(state).device_id;
    match state.lan_trust_store.issue_pairing_token(
        device_id,
        state.logbook_id,
        display_name,
        request.approved_by_operator,
        chrono::Utc::now(),
    ) {
        Ok(token) => {
            let _ = publish_gui_runtime(
                state,
                "sync.lan.pairing_token.issued",
                RuntimeEventSeverity::Info,
                "LAN pairing token issued after operator approval",
                Some(json!({"token_id": token.token_id, "expires_at": token.expires_at})),
                None,
            );
            json_response(&json!({"ok": true, "pairing": token}))
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_lan_pairing_accept(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::SyncLanDiscovery,
        "LAN pairing accept permission check",
    ) {
        return response;
    }
    let Ok(request) = serde_json::from_slice::<LanPairingAcceptRequest>(body) else {
        return json_error(400, "invalid LAN pairing accept JSON");
    };
    let logbook_id = request.logbook_id.unwrap_or(state.logbook_id);
    let pairing_code = request.pairing_code.trim().to_owned();
    let auth_credential_id = match store_lan_auth_credential(
        state,
        request.peer_device_id,
        &request.peer_display_name,
        &pairing_code,
    ) {
        Ok(credential_id) => credential_id,
        Err(error) => return json_response_with_status(500, &json!({"ok": false, "error": error})),
    };
    match state.lan_trust_store.accept_pairing_token(
        LanPairingAcceptance {
            token_id: request.token_id,
            pairing_code,
            peer_device_id: request.peer_device_id,
            peer_display_name: request.peer_display_name,
            requested_logbooks: vec![logbook_id],
            public_key_fingerprint: request.public_key_fingerprint,
            auth_credential_id: Some(auth_credential_id),
        },
        chrono::Utc::now(),
    ) {
        Ok(device) => {
            let _ = publish_gui_runtime(
                state,
                "sync.lan.device.trusted",
                RuntimeEventSeverity::Info,
                "LAN peer device trusted",
                Some(json!({"device_id": device.device_id, "logbook_ids": device.logbook_ids})),
                None,
            );
            json_response(&json!({"ok": true, "trusted_device": device}))
        }
        Err(error) => {
            delete_lan_auth_credential(state, auth_credential_id);
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_lan_pairing_complete(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::SyncLanDiscovery,
        "LAN reciprocal pairing permission check",
    ) {
        return response;
    }
    let Ok(request) = serde_json::from_slice::<LanPairingCompleteRequest>(body) else {
        return json_error(400, "invalid LAN pairing complete JSON");
    };
    let Some(peer) = selected_peer_record(state, request.peer_id) else {
        return sync_no_peer_error(state, "sync.lan.pairing_complete.failed");
    };
    let pairing_code = request.pairing_code.trim().to_owned();
    if pairing_code.is_empty() {
        return json_error(400, "pairing code is required");
    }
    let local_identity = local_sync_identity(state);
    let remote_accept = LanPairingAcceptRequest {
        token_id: request.token_id,
        pairing_code: pairing_code.clone(),
        peer_device_id: local_identity.device_id,
        peer_display_name: local_identity.display_name,
        logbook_id: Some(state.logbook_id),
        public_key_fingerprint: request.public_key_fingerprint.clone(),
    };

    let mut last_error = None;
    let mut remote_response = None;
    for address in sorted_peer_api_addresses(&peer) {
        match post_lan_peer_pairing_accept(address, &remote_accept) {
            Ok(response) => {
                remote_response = Some(response);
                break;
            }
            Err(error) => last_error = Some(error),
        }
    }
    let Some(remote_response) = remote_response else {
        return sync_lan_transport_error(
            state,
            "sync.lan.pairing_complete.failed",
            last_error.unwrap_or_else(|| "LAN peer has no usable API addresses".to_owned()),
        );
    };

    let auth_credential_id =
        match store_lan_auth_credential(state, peer.device_id, &peer.display_name, &pairing_code) {
            Ok(credential_id) => credential_id,
            Err(error) => {
                return json_response_with_status(500, &json!({"ok": false, "error": error}));
            }
        };
    match state.lan_trust_store.trust_peer_with_auth_credential(
        LanPeerTrustUpdate {
            peer_device_id: peer.device_id,
            peer_display_name: peer.display_name.clone(),
            logbook_id: state.logbook_id,
            pairing_token_id: Some(request.token_id),
            public_key_fingerprint: request.public_key_fingerprint,
            auth_credential_id,
        },
        chrono::Utc::now(),
    ) {
        Ok(device) => {
            let _ = publish_gui_runtime(
                state,
                "sync.lan.device.trusted",
                RuntimeEventSeverity::Info,
                "LAN peer device paired with endpoint authentication",
                Some(json!({
                    "device_id": device.device_id,
                    "logbook_ids": device.logbook_ids,
                    "auth_credential_id": device.auth_credential_id
                })),
                None,
            );
            json_response(&json!({
                "ok": true,
                "trusted_device": device,
                "remote": remote_response
            }))
        }
        Err(error) => {
            delete_lan_auth_credential(state, auth_credential_id);
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn handle_lan_trust_revoke(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::SyncLanDiscovery,
        "LAN trust revoke permission check",
    ) {
        return response;
    }
    let Ok(request) = serde_json::from_slice::<LanTrustRevokeRequest>(body) else {
        return json_error(400, "invalid LAN trust revoke JSON");
    };
    match state
        .lan_trust_store
        .revoke_device(request.device_id, chrono::Utc::now())
    {
        Ok(device) => {
            if let Some(credential_id) = device.auth_credential_id {
                delete_lan_auth_credential(state, credential_id);
            }
            let _ = publish_gui_runtime(
                state,
                "sync.lan.device.revoked",
                RuntimeEventSeverity::Warn,
                "LAN peer device revoked",
                Some(json!({"device_id": device.device_id})),
                None,
            );
            json_response(&json!({"ok": true, "trusted_device": device}))
        }
        Err(error) => {
            json_response_with_status(400, &json!({"ok": false, "error": error.to_string()}))
        }
    }
}

fn store_lan_auth_credential(
    state: &AppState,
    peer_device_id: uuid::Uuid,
    peer_display_name: &str,
    secret: &str,
) -> Result<uuid::Uuid, String> {
    let secret = secret.trim();
    if secret.len() < 32 {
        return Err("LAN pairing code is too short for endpoint authentication".to_owned());
    }
    let mut metadata = CredentialMetadata::new(
        "lan-sync-peer",
        peer_device_id.to_string(),
        ServiceType::Authentication,
        format!("LAN sync auth for {peer_display_name}"),
    );
    metadata.metadata = json!({
        "purpose": "lan_sync_endpoint_auth",
        "peer_device_id": peer_device_id,
    });
    let stored = state
        .credential_store
        .lock()
        .expect("credential store mutex should not be poisoned")
        .store_credential(metadata, secret)
        .map_err(|error| format!("failed to store LAN endpoint auth credential: {error}"))?;
    Ok(stored.credential_id)
}

fn retrieve_lan_auth_secret(state: &AppState, credential_id: uuid::Uuid) -> Result<String, String> {
    state
        .credential_store
        .lock()
        .expect("credential store mutex should not be poisoned")
        .retrieve_secret(credential_id)
        .map_err(|error| format!("failed to retrieve LAN endpoint auth credential: {error}"))
}

fn delete_lan_auth_credential(state: &AppState, credential_id: uuid::Uuid) {
    let _ = state
        .credential_store
        .lock()
        .expect("credential store mutex should not be poisoned")
        .delete_credential(credential_id);
}

fn handle_sync_discovery(state: &Arc<AppState>, running: bool) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::SyncLanDiscovery,
        "LAN discovery permission check",
    ) {
        return response;
    }
    let worker_generation = {
        let mut sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        if running {
            if sync.discovery_running {
                None
            } else {
                sync.discovery_generation = sync.discovery_generation.wrapping_add(1);
                sync.discovery_running = true;
                Some(sync.discovery_generation)
            }
        } else {
            if sync.discovery_running {
                sync.discovery_generation = sync.discovery_generation.wrapping_add(1);
            }
            sync.discovery_running = false;
            None
        }
    };
    let event_type = if running {
        "network.discovery.started"
    } else {
        "network.discovery.stopped"
    };
    let _ = publish_gui_runtime(
        state,
        event_type,
        RuntimeEventSeverity::Info,
        event_type,
        Some(json!({"transport": "ipv4/ipv6 multicast", "mvp": false})),
        None,
    );
    if let Some(generation) = worker_generation {
        start_lan_discovery_worker(state.clone(), generation);
    }
    json_response(&sync_state_payload(state))
}

fn handle_sync_refresh(state: &AppState) -> Vec<u8> {
    let discovery_snapshot = {
        let sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        if sync.discovery_running {
            Some((sync.config.clone(), sync.identity.clone()))
        } else {
            None
        }
    };
    if let Some((config, identity)) = discovery_snapshot {
        match run_lan_discovery_cycle(state, config, identity) {
            Ok(observed_count) => {
                let _ = publish_gui_runtime(
                    state,
                    "network.discovery.refreshed",
                    RuntimeEventSeverity::Info,
                    "LAN discovery refreshed",
                    Some(json!({"observed_count": observed_count})),
                    None,
                );
            }
            Err(error) => {
                record_lan_discovery_error(state, "network.discovery.refresh_failed", error);
            }
        }
        return json_response(&sync_state_payload(state));
    }

    let demo_remote_events = build_demo_remote_events(state);
    let mut sync = state
        .sync
        .lock()
        .expect("sync state mutex should not be poisoned");
    let mut peer = LocalPeerIdentity::new("Demo LAN Peer", Some(sync.config.local_sync_port));
    peer.device_id = demo_peer_device_id();
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

fn start_lan_discovery_worker(state: Arc<AppState>, generation: u64) {
    thread::spawn(move || {
        let _ = publish_gui_runtime(
            &state,
            "network.discovery.worker_started",
            RuntimeEventSeverity::Info,
            "LAN discovery worker started",
            None,
            None,
        );
        loop {
            let snapshot = {
                let sync = state
                    .sync
                    .lock()
                    .expect("sync state mutex should not be poisoned");
                if !sync.discovery_running || sync.discovery_generation != generation {
                    None
                } else {
                    Some((sync.config.clone(), sync.identity.clone()))
                }
            };
            let Some((config, identity)) = snapshot else {
                break;
            };
            if let Err(error) = run_lan_discovery_cycle(&state, config.clone(), identity) {
                record_lan_discovery_error(&state, "network.discovery.failed", error);
            }
            if !wait_for_next_discovery_cycle(
                &state,
                generation,
                Duration::from_secs(config.discovery_interval_seconds.max(1)),
            ) {
                break;
            }
        }
        let _ = publish_gui_runtime(
            &state,
            "network.discovery.worker_stopped",
            RuntimeEventSeverity::Info,
            "LAN discovery worker stopped",
            None,
            None,
        );
    });
}

fn wait_for_next_discovery_cycle(state: &AppState, generation: u64, interval: Duration) -> bool {
    let deadline = Instant::now() + interval;
    loop {
        if !lan_discovery_is_running(state, generation) {
            return false;
        }
        let now = Instant::now();
        if now >= deadline {
            return true;
        }
        thread::sleep((deadline - now).min(LAN_DISCOVERY_SLEEP_SLICE));
    }
}

fn lan_discovery_is_running(state: &AppState, generation: u64) -> bool {
    let sync = state
        .sync
        .lock()
        .expect("sync state mutex should not be poisoned");
    sync.discovery_running && sync.discovery_generation == generation
}

fn run_lan_discovery_cycle(
    state: &AppState,
    config: SyncConfig,
    identity: LocalPeerIdentity,
) -> Result<usize, String> {
    let service = LanDiscoveryService { config, identity };
    let observations = service
        .discover_once(LAN_DISCOVERY_LISTEN_WINDOW)
        .map_err(|error| error.to_string())?;
    let mut observed_count = 0usize;
    for observation in observations {
        if observe_discovery_packet(state, observation.packet, observation.source) {
            observed_count += 1;
        }
    }
    expire_stale_discovery_peers(state);
    Ok(observed_count)
}

fn observe_discovery_packet(state: &AppState, packet: DiscoveryPacket, source: SocketAddr) -> bool {
    if !is_supported_discovery_packet(&packet) {
        publish_discovery_observation(state, &PeerObservation::IgnoredIncompatible, source);
        return false;
    }
    if is_local_discovery_packet(state, &packet) {
        return false;
    }
    let api_address = discovery_api_address(&packet, source);
    if !is_usable_discovery_source(api_address) {
        let _ = publish_gui_runtime(
            state,
            "network.peer.ignored_unroutable",
            RuntimeEventSeverity::Debug,
            "LAN discovery source is not directly routable",
            Some(json!({"source": source.to_string(), "api_address": api_address.to_string()})),
            None,
        );
        return false;
    }
    let identity = match fetch_lan_peer_identity(api_address) {
        Ok(identity) => identity,
        Err(error) => {
            let _ = publish_gui_runtime(
                state,
                "network.peer.unreachable",
                RuntimeEventSeverity::Debug,
                "LAN discovery peer API was unreachable",
                Some(json!({"api_address": api_address.to_string()})),
                Some(error),
            );
            return false;
        }
    };
    if identity.device_id != packet.device_id || identity.session_id != packet.session_id {
        let _ = publish_gui_runtime(
            state,
            "network.peer.ignored_spoofed",
            RuntimeEventSeverity::Warn,
            "LAN discovery identity probe did not match packet",
            Some(json!({
                "api_address": api_address.to_string(),
                "packet_device_id": packet.device_id,
                "probed_device_id": identity.device_id
            })),
            None,
        );
        return false;
    }
    let packet = DiscoveryPacket::from_identity(&identity);
    let observation = {
        let mut sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        let local = sync.identity.clone();
        sync.registry.observe(&local, packet, api_address)
    };
    publish_discovery_observation(state, &observation, api_address);
    matches!(
        observation,
        PeerObservation::Discovered(_) | PeerObservation::Updated(_)
    )
}

fn is_local_discovery_packet(state: &AppState, packet: &DiscoveryPacket) -> bool {
    let sync = state
        .sync
        .lock()
        .expect("sync state mutex should not be poisoned");
    packet.device_id == sync.identity.device_id && packet.session_id == sync.identity.session_id
}

fn is_supported_discovery_packet(packet: &DiscoveryPacket) -> bool {
    packet.protocol_name == PROTOCOL_NAME && packet.protocol_version == PROTOCOL_VERSION
}

fn discovery_api_address(packet: &DiscoveryPacket, source: SocketAddr) -> SocketAddr {
    packet
        .local_api_port
        .map(|port| SocketAddr::new(source.ip(), port))
        .unwrap_or(source)
}

fn is_usable_discovery_source(source: SocketAddr) -> bool {
    match source {
        SocketAddr::V4(_) => true,
        SocketAddr::V6(address) => !address.ip().is_unicast_link_local() || address.scope_id() != 0,
    }
}

fn publish_discovery_observation(
    state: &AppState,
    observation: &PeerObservation,
    source: SocketAddr,
) {
    let (event_type, severity, summary) = match observation {
        PeerObservation::Discovered(_) => (
            "network.peer.discovered",
            RuntimeEventSeverity::Info,
            "LAN peer discovered",
        ),
        PeerObservation::Updated(_) => (
            "network.peer.updated",
            RuntimeEventSeverity::Debug,
            "LAN peer refreshed",
        ),
        PeerObservation::IgnoredIncompatible => (
            "network.peer.ignored_incompatible",
            RuntimeEventSeverity::Warn,
            "Incompatible LAN discovery packet ignored",
        ),
        PeerObservation::IgnoredSelf => return,
    };
    let _ = publish_gui_runtime(
        state,
        event_type,
        severity,
        summary,
        Some(json!({
            "source": source.to_string(),
            "observation": format!("{observation:?}")
        })),
        None,
    );
}

fn expire_stale_discovery_peers(state: &AppState) {
    let expired = {
        let mut sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        let timeout = Duration::from_secs(sync.config.peer_timeout_seconds);
        sync.registry.expire_stale(chrono::Utc::now(), timeout)
    };
    for peer_id in expired {
        let _ = publish_gui_runtime(
            state,
            "network.peer.expired",
            RuntimeEventSeverity::Warn,
            "LAN peer expired",
            Some(json!({"peer_id": peer_id})),
            None,
        );
    }
}

fn record_lan_discovery_error(state: &AppState, event_type: &str, error: String) {
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
        "LAN discovery transport failed",
        None,
        Some(error),
    );
}

fn handle_sync_add_peer(state: &AppState, body: &[u8]) -> Vec<u8> {
    if let Err(response) = ensure_gui_permission(
        state,
        &core_gui_manifest(),
        PluginCapability::SyncLanDiscovery,
        "Manual LAN peer add permission check",
    ) {
        return response;
    }
    let Ok(request) = serde_json::from_slice::<ManualLanPeerRequest>(body) else {
        return json_error(400, "invalid manual LAN peer JSON");
    };
    let address = match parse_lan_peer_address(&request.address) {
        Ok(address) => address,
        Err(error) => return json_response_with_status(400, &json!({"ok": false, "error": error})),
    };
    let identity = match fetch_lan_peer_identity(address) {
        Ok(identity) => identity,
        Err(error) => {
            {
                let mut sync = state
                    .sync
                    .lock()
                    .expect("sync state mutex should not be poisoned");
                sync.warning_count += 1;
            }
            let _ = publish_gui_runtime(
                state,
                "network.peer.unreachable",
                RuntimeEventSeverity::Warn,
                "Manual LAN peer probe failed",
                Some(json!({"address": address.to_string()})),
                Some(error.clone()),
            );
            return json_response_with_status(400, &json!({"ok": false, "error": error}));
        }
    };
    let packet = DiscoveryPacket::from_identity(&identity);
    let observation = {
        let mut sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        let local = sync.identity.clone();
        sync.registry.observe(&local, packet, address)
    };
    let event_type = match &observation {
        PeerObservation::Discovered(_) => "network.peer.discovered",
        PeerObservation::Updated(_) => "network.peer.updated",
        PeerObservation::IgnoredSelf => "network.peer.ignored_self",
        PeerObservation::IgnoredIncompatible => "network.peer.ignored_incompatible",
    };
    let ok = matches!(
        observation,
        PeerObservation::Discovered(_) | PeerObservation::Updated(_)
    );
    let _ = publish_gui_runtime(
        state,
        event_type,
        if ok {
            RuntimeEventSeverity::Info
        } else {
            RuntimeEventSeverity::Warn
        },
        "Manual LAN peer probe completed",
        Some(json!({
            "address": address.to_string(),
            "peer_device_id": identity.device_id,
            "peer_display_name": identity.display_name,
            "observation": format!("{observation:?}")
        })),
        None,
    );
    if ok {
        json_response(
            &json!({"ok": true, "peer_identity": identity, "sync": sync_state_payload(state)}),
        )
    } else {
        json_response_with_status(
            400,
            &json!({"ok": false, "error": format!("{observation:?}")}),
        )
    }
}

fn handle_sync_list_logbooks(state: &AppState, request: &HttpRequest) -> Vec<u8> {
    if let Err(response) = require_lan_endpoint_auth(
        state,
        request,
        state.logbook_id,
        "sync.lan.list_logbooks.rejected",
    ) {
        return response;
    }
    json_response(&ListLogbooksResponse {
        logbooks: vec![logbook_head_summary(state)],
    })
}

fn handle_sync_get_head(state: &AppState, query: &str, request: &HttpRequest) -> Vec<u8> {
    let logbook_id = match requested_sync_logbook_id(state, query) {
        Ok(logbook_id) => logbook_id,
        Err(response) => return response,
    };
    if let Err(response) =
        require_lan_endpoint_auth(state, request, logbook_id, "sync.lan.get_head.rejected")
    {
        return response;
    }
    json_response(&logbook_head_summary(state))
}

fn handle_sync_events_since(state: &AppState, query: &str, request: &HttpRequest) -> Vec<u8> {
    let logbook_id = match requested_sync_logbook_id(state, query) {
        Ok(logbook_id) => logbook_id,
        Err(response) => return response,
    };
    if let Err(response) =
        require_lan_endpoint_auth(state, request, logbook_id, "sync.lan.events_since.rejected")
    {
        return response;
    }
    let params = parse_query(query);
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

fn handle_sync_event_metadata(state: &AppState, query: &str, request: &HttpRequest) -> Vec<u8> {
    let logbook_id = match requested_sync_logbook_id(state, query) {
        Ok(logbook_id) => logbook_id,
        Err(response) => return response,
    };
    if let Err(response) = require_lan_endpoint_auth(
        state,
        request,
        logbook_id,
        "sync.lan.event_metadata.rejected",
    ) {
        return response;
    }
    let params = parse_query(query);
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

fn requested_sync_logbook_id(state: &AppState, query: &str) -> Result<uuid::Uuid, Vec<u8>> {
    let params = parse_query(query);
    let Some(value) = params.get("logbook_id").filter(|value| !value.is_empty()) else {
        return Ok(state.logbook_id);
    };
    let Ok(logbook_id) = uuid::Uuid::parse_str(value) else {
        return Err(json_response_with_status(
            400,
            &json!({"ok": false, "error": "invalid logbook_id"}),
        ));
    };
    if logbook_id != state.logbook_id {
        return Err(json_response_with_status(
            403,
            &json!({"ok": false, "error": "requested logbook is not served by this peer"}),
        ));
    }
    Ok(logbook_id)
}

fn require_lan_endpoint_auth(
    state: &AppState,
    request: &HttpRequest,
    logbook_id: uuid::Uuid,
    event_type: &str,
) -> Result<(), Vec<u8>> {
    let auth = match lan_endpoint_auth_from_headers(&request.headers) {
        Ok(auth) => auth,
        Err(error) => {
            record_lan_endpoint_auth_failure(state, event_type, None, error.clone());
            return Err(json_response_with_status(
                403,
                &json!({"ok": false, "error": error}),
            ));
        }
    };
    let trusted = match state
        .lan_trust_store
        .trusted_peer(auth.device_id, logbook_id)
    {
        Ok(trusted) => trusted,
        Err(error) => {
            let message = error.to_string();
            record_lan_endpoint_auth_failure(
                state,
                event_type,
                Some(auth.device_id),
                message.clone(),
            );
            return Err(json_response_with_status(
                403,
                &json!({"ok": false, "error": message}),
            ));
        }
    };
    let Some(credential_id) = trusted.auth_credential_id else {
        let message = "trusted LAN device is missing endpoint auth credential".to_owned();
        record_lan_endpoint_auth_failure(state, event_type, Some(auth.device_id), message.clone());
        return Err(json_response_with_status(
            403,
            &json!({"ok": false, "error": message}),
        ));
    };
    let secret = match retrieve_lan_auth_secret(state, credential_id) {
        Ok(secret) => secret,
        Err(error) => {
            record_lan_endpoint_auth_failure(
                state,
                event_type,
                Some(auth.device_id),
                error.clone(),
            );
            return Err(json_response_with_status(
                403,
                &json!({"ok": false, "error": error}),
            ));
        }
    };
    if !verify_lan_auth_signature(
        &secret,
        auth.device_id,
        logbook_id,
        &request.method,
        &request.target,
        &auth.replay_nonce,
        &auth.signature,
    ) {
        let message = "trusted LAN request signature is invalid".to_owned();
        record_lan_endpoint_auth_failure(state, event_type, Some(auth.device_id), message.clone());
        return Err(json_response_with_status(
            403,
            &json!({"ok": false, "error": message}),
        ));
    }
    match state.lan_trust_store.authorize_peer(
        auth.device_id,
        logbook_id,
        &auth.replay_nonce,
        chrono::Utc::now(),
    ) {
        Ok(_) => Ok(()),
        Err(error) => {
            let message = error.to_string();
            record_lan_endpoint_auth_failure(
                state,
                event_type,
                Some(auth.device_id),
                message.clone(),
            );
            Err(json_response_with_status(
                403,
                &json!({"ok": false, "error": message}),
            ))
        }
    }
}

fn lan_endpoint_auth_from_headers(
    headers: &HashMap<String, String>,
) -> Result<LanEndpointAuth, String> {
    let device_id = headers
        .get(LAN_AUTH_DEVICE_ID_HEADER)
        .ok_or_else(|| "trusted LAN request is missing device id".to_owned())
        .and_then(|value| {
            uuid::Uuid::parse_str(value)
                .map_err(|_| "trusted LAN request device id is invalid".to_owned())
        })?;
    let replay_nonce = headers
        .get(LAN_AUTH_REPLAY_NONCE_HEADER)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "trusted LAN request is missing replay nonce".to_owned())?;
    if replay_nonce.len() > 128 {
        return Err("trusted LAN request replay nonce is too long".to_owned());
    }
    let signature_version = headers
        .get(LAN_AUTH_SIGNATURE_VERSION_HEADER)
        .map(|value| value.trim())
        .ok_or_else(|| "trusted LAN request is missing signature version".to_owned())?;
    if signature_version != LAN_AUTH_SIGNATURE_VERSION {
        return Err("trusted LAN request signature version is unsupported".to_owned());
    }
    let signature = headers
        .get(LAN_AUTH_SIGNATURE_HEADER)
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "trusted LAN request is missing signature".to_owned())?;
    if signature.len() != 64 || !signature.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("trusted LAN request signature is malformed".to_owned());
    }
    Ok(LanEndpointAuth {
        device_id,
        replay_nonce: replay_nonce.to_owned(),
        signature: signature.to_ascii_lowercase(),
    })
}

fn record_lan_endpoint_auth_failure(
    state: &AppState,
    event_type: &str,
    device_id: Option<uuid::Uuid>,
    error: String,
) {
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
        "LAN sync endpoint rejected an unauthorized peer",
        Some(json!({"device_id": device_id})),
        Some(error),
    );
}

fn handle_sync_handshake(state: &AppState, body: &[u8]) -> Vec<u8> {
    let request =
        serde_json::from_slice::<HandshakePeerRequest>(body).unwrap_or(HandshakePeerRequest {
            peer_id: None,
            replay_nonce: None,
        });
    let local_head = logbook_head_summary(state);
    let Some(peer) = selected_peer_record(state, request.peer_id) else {
        return sync_no_peer_error(state, "sync.handshake.error");
    };
    let remote_head = match remote_head_for_peer(state, &peer) {
        Ok(remote_head) => remote_head,
        Err(error) => return sync_lan_transport_error(state, "sync.handshake.error", error),
    };

    let remote_request = HandshakeRequest {
        protocol_version: PROTOCOL_VERSION,
        device_id: peer.device_id,
        session_id: peer.session_id,
        supported_capabilities: peer.capabilities.clone(),
        logbooks: vec![remote_head],
    };
    let mut sync = state
        .sync
        .lock()
        .expect("sync state mutex should not be poisoned");
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
    let request =
        serde_json::from_slice::<HandshakePeerRequest>(body).unwrap_or(HandshakePeerRequest {
            peer_id: None,
            replay_nonce: None,
        });
    let Some(peer) = selected_peer_record(state, request.peer_id) else {
        return sync_no_peer_error(state, "sync.preview_pull.failed");
    };
    let peer_id = peer.peer_id.clone();

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
    let remote_events = match remote_events_for_peer(state, &peer) {
        Ok(events) => events,
        Err(error) => {
            return sync_lan_transport_error(state, "sync.preview_pull.failed", error);
        }
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
    let request =
        serde_json::from_slice::<HandshakePeerRequest>(body).unwrap_or(HandshakePeerRequest {
            peer_id: None,
            replay_nonce: None,
        });
    let Some(peer) = selected_peer_record(state, request.peer_id) else {
        return sync_no_peer_error(state, "sync.pull.failed");
    };
    let peer_id = peer.peer_id.clone();
    let Some(replay_nonce) = request
        .replay_nonce
        .as_deref()
        .filter(|value| !value.is_empty())
    else {
        return sync_lan_trust_error(
            state,
            "sync.pull.failed",
            "trusted LAN pull requires a replay nonce",
        );
    };
    if let Err(error) = state.lan_trust_store.authorize_peer(
        peer.device_id,
        state.logbook_id,
        replay_nonce,
        chrono::Utc::now(),
    ) {
        return sync_lan_trust_error(state, "sync.pull.failed", error.to_string());
    }

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
    let remote_events = match remote_events_for_peer(state, &peer) {
        Ok(events) => events,
        Err(error) => {
            return sync_lan_transport_error(state, "sync.pull.failed", error);
        }
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
    let all_events = state
        .proposal_runtime
        .block_on(state.store.list_events(state.logbook_id))
        .unwrap_or_default();
    let offline_batch = match state.offline_queue.ready_event_batch(
        state.logbook_id,
        &all_events,
        chrono::Utc::now(),
    ) {
        Ok(batch) => batch,
        Err(error) => {
            return json_response_with_status(
                400,
                &json!({"ok": false, "error": format!("offline queue is not readable: {error}")}),
            )
        }
    };
    for operation_id in &offline_batch.missing_local_event_operation_ids {
        let _ = state.offline_queue.mark_user_action_required(
            *operation_id,
            "offline mutation has no matching local official event",
            Some("missing_local_official_event".to_owned()),
            chrono::Utc::now(),
        );
    }
    for operation_id in &offline_batch.operation_ids {
        let _ = state
            .offline_queue
            .mark_sending(*operation_id, chrono::Utc::now());
    }
    let queued_hashes = offline_batch
        .events
        .iter()
        .map(|event| event.event_hash.clone())
        .collect::<std::collections::HashSet<_>>();
    let events = if offline_batch.events.is_empty() {
        all_events
    } else {
        offline_batch.events.clone()
    };
    let pushed_event_count = events.len();
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
            if push.errors.is_empty() {
                let _ = state
                    .offline_queue
                    .mark_accepted_by_event_hashes(&queued_hashes, chrono::Utc::now());
            } else if push.status == ReplicationStatus::Diverged {
                for operation_id in &offline_batch.operation_ids {
                    let _ = state.offline_queue.mark_blocked(
                        *operation_id,
                        push.errors
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "cloud divergence detected".to_owned()),
                        chrono::Utc::now(),
                    );
                }
            } else {
                for operation_id in &offline_batch.operation_ids {
                    let _ = state.offline_queue.mark_user_action_required(
                        *operation_id,
                        push.errors
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "cloud push rejected".to_owned()),
                        Some("cloud_push_rejected".to_owned()),
                        chrono::Utc::now(),
                    );
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
            json_response(&json!({
                "ok": push.errors.is_empty(),
                "push": push,
                "offline_push_batch": {
                    "operation_ids": offline_batch.operation_ids,
                    "event_count": pushed_event_count,
                    "missing_local_event_operation_ids": offline_batch.missing_local_event_operation_ids
                }
            }))
        }
        Err(error) => {
            for operation_id in &offline_batch.operation_ids {
                let _ = state.offline_queue.record_transient_failure(
                    *operation_id,
                    error.to_string(),
                    Some("cloud_push_failed".to_owned()),
                    chrono::Utc::now(),
                );
            }
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

fn selected_peer_record(state: &AppState, requested: Option<String>) -> Option<PeerRecord> {
    let sync = state
        .sync
        .lock()
        .expect("sync state mutex should not be poisoned");
    let peers = sync.registry.list();
    requested
        .and_then(|requested| peers.iter().find(|peer| peer.peer_id == requested).cloned())
        .or_else(|| peers.into_iter().next())
}

fn remote_events_for_peer(
    state: &AppState,
    peer: &PeerRecord,
) -> Result<Vec<CoreEventEnvelope>, String> {
    let mut last_error = None;
    for address in sorted_peer_api_addresses(peer) {
        match fetch_lan_peer_events(state, address, peer.device_id, state.logbook_id) {
            Ok(events) => {
                let _ = publish_gui_runtime(
                    state,
                    "sync.lan.transport.succeeded",
                    RuntimeEventSeverity::Info,
                    "Fetched remote official events over LAN HTTP",
                    Some(json!({
                        "peer_id": peer.peer_id,
                        "address": address.to_string(),
                        "event_count": events.len()
                    })),
                    None,
                );
                return Ok(events);
            }
            Err(error) => {
                last_error = Some(error);
            }
        }
    }

    if is_demo_peer(peer) {
        let sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        return Ok(sync.demo_remote_events.clone());
    }

    Err(last_error.unwrap_or_else(|| "LAN peer has no usable API addresses".to_owned()))
}

fn remote_head_for_peer(state: &AppState, peer: &PeerRecord) -> Result<LogbookHeadSummary, String> {
    let mut last_error = None;
    for address in sorted_peer_api_addresses(peer) {
        match fetch_lan_peer_head(state, address, peer.device_id, state.logbook_id) {
            Ok(head) => return Ok(head),
            Err(error) => last_error = Some(error),
        }
    }

    if is_demo_peer(peer) {
        let sync = state
            .sync
            .lock()
            .expect("sync state mutex should not be poisoned");
        return Ok(LogbookHeadSummary {
            logbook_id: state.logbook_id,
            head_hash: sync
                .demo_remote_events
                .last()
                .map(|event| event.event_hash.clone()),
            event_count: Some(sync.demo_remote_events.len() as u64),
        });
    }

    Err(last_error.unwrap_or_else(|| "LAN peer has no usable API addresses".to_owned()))
}

fn sorted_peer_api_addresses(peer: &PeerRecord) -> Vec<SocketAddr> {
    let mut addresses = peer.addresses.clone();
    addresses.sort_by_key(|address| lan_api_address_rank(*address));
    addresses
}

fn lan_api_address_rank(address: SocketAddr) -> u8 {
    match address {
        SocketAddr::V4(address) if address.ip().is_loopback() => 0,
        SocketAddr::V4(address) if address.ip().is_private() => 1,
        SocketAddr::V4(address) if address.ip().is_link_local() => 2,
        SocketAddr::V6(address) if is_ipv6_unique_local(address.ip()) => 3,
        SocketAddr::V6(address)
            if address.ip().is_unicast_link_local() && address.scope_id() != 0 =>
        {
            4
        }
        SocketAddr::V6(address) if address.ip().is_loopback() => 5,
        SocketAddr::V6(_) => 8,
        SocketAddr::V4(_) => 9,
    }
}

fn fetch_lan_peer_identity(address: SocketAddr) -> Result<LocalPeerIdentity, String> {
    let state: Value = lan_http_get_json(address, "/api/sync/state", &[])?;
    serde_json::from_value(
        state
            .get("identity")
            .cloned()
            .ok_or_else(|| "LAN peer state did not include identity".to_owned())?,
    )
    .map_err(|error| format!("LAN peer identity JSON was invalid: {error}"))
}

fn fetch_lan_peer_head(
    state: &AppState,
    address: SocketAddr,
    peer_device_id: uuid::Uuid,
    logbook_id: uuid::Uuid,
) -> Result<LogbookHeadSummary, String> {
    let path = format!("/api/sync/get-head?logbook_id={logbook_id}");
    let headers = trusted_lan_request_headers(state, peer_device_id, logbook_id, "GET", &path)?;
    let response: LogbookHeadSummary = lan_http_get_json(address, &path, &headers)?;
    if response.logbook_id != logbook_id {
        return Err(format!(
            "LAN peer returned head for logbook {}, expected {logbook_id}",
            response.logbook_id
        ));
    }
    Ok(response)
}

fn fetch_lan_peer_events(
    state: &AppState,
    address: SocketAddr,
    peer_device_id: uuid::Uuid,
    logbook_id: uuid::Uuid,
) -> Result<Vec<CoreEventEnvelope>, String> {
    let path = format!("/api/sync/events-since?logbook_id={logbook_id}");
    let headers = trusted_lan_request_headers(state, peer_device_id, logbook_id, "GET", &path)?;
    let response: GetEventRangeResponse = lan_http_get_json(address, &path, &headers)?;
    if response.logbook_id != logbook_id {
        return Err(format!(
            "LAN peer returned events for logbook {}, expected {logbook_id}",
            response.logbook_id
        ));
    }
    Ok(response.events)
}

fn post_lan_peer_pairing_accept(
    address: SocketAddr,
    request: &LanPairingAcceptRequest,
) -> Result<Value, String> {
    lan_http_post_json(address, "/api/sync/lan/pairing-accept", request, &[])
}

fn local_sync_device_id(state: &AppState) -> uuid::Uuid {
    local_sync_identity(state).device_id
}

fn local_sync_identity(state: &AppState) -> LocalPeerIdentity {
    state
        .sync
        .lock()
        .expect("sync state mutex should not be poisoned")
        .identity
        .clone()
}

fn trusted_lan_request_headers(
    state: &AppState,
    peer_device_id: uuid::Uuid,
    logbook_id: uuid::Uuid,
    method: &str,
    target: &str,
) -> Result<Vec<(String, String)>, String> {
    let trusted = state
        .lan_trust_store
        .trusted_peer(peer_device_id, logbook_id)
        .map_err(|error| error.to_string())?;
    let credential_id = trusted
        .auth_credential_id
        .ok_or_else(|| "trusted LAN peer is missing endpoint auth credential".to_owned())?;
    let secret = retrieve_lan_auth_secret(state, credential_id)?;
    let requester_device_id = local_sync_device_id(state);
    let replay_nonce = uuid::Uuid::new_v4().to_string();
    let signature = lan_auth_signature(
        &secret,
        requester_device_id,
        logbook_id,
        method,
        target,
        &replay_nonce,
    );
    Ok(vec![
        (
            LAN_AUTH_DEVICE_ID_HEADER.to_owned(),
            requester_device_id.to_string(),
        ),
        (LAN_AUTH_REPLAY_NONCE_HEADER.to_owned(), replay_nonce),
        (
            LAN_AUTH_SIGNATURE_VERSION_HEADER.to_owned(),
            LAN_AUTH_SIGNATURE_VERSION.to_owned(),
        ),
        (LAN_AUTH_SIGNATURE_HEADER.to_owned(), signature),
    ])
}

fn lan_http_get_json<T>(
    address: SocketAddr,
    path: &str,
    extra_headers: &[(String, String)],
) -> Result<T, String>
where
    T: DeserializeOwned,
{
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
        .map_err(|error| format!("failed to connect to LAN peer {address}: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("failed to set LAN peer read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("failed to set LAN peer write timeout: {error}"))?;
    let request = format!(
        "GET {path} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nConnection: close\r\n{}\r\n",
        lan_host_header(address),
        lan_http_extra_headers(extra_headers)
    );
    stream
        .write_all(request.as_bytes())
        .map_err(|error| format!("failed to write LAN peer request: {error}"))?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| format!("failed to read LAN peer response: {error}"))?;
    let body = http_response_body(&response)?;
    serde_json::from_slice(body).map_err(|error| format!("LAN peer JSON was invalid: {error}"))
}

fn lan_http_post_json<T, B>(
    address: SocketAddr,
    path: &str,
    body: &B,
    extra_headers: &[(String, String)],
) -> Result<T, String>
where
    T: DeserializeOwned,
    B: Serialize,
{
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(2))
        .map_err(|error| format!("failed to connect to LAN peer {address}: {error}"))?;
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("failed to set LAN peer read timeout: {error}"))?;
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .map_err(|error| format!("failed to set LAN peer write timeout: {error}"))?;
    let body = serde_json::to_vec(body)
        .map_err(|error| format!("LAN peer request JSON failed: {error}"))?;
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {}\r\nAccept: application/json\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n{}\r\n",
        lan_host_header(address),
        body.len(),
        lan_http_extra_headers(extra_headers)
    );
    stream
        .write_all(request.as_bytes())
        .and_then(|_| stream.write_all(&body))
        .map_err(|error| format!("failed to write LAN peer request: {error}"))?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .map_err(|error| format!("failed to read LAN peer response: {error}"))?;
    let body = http_response_body(&response)?;
    serde_json::from_slice(body).map_err(|error| format!("LAN peer JSON was invalid: {error}"))
}

fn lan_http_extra_headers(extra_headers: &[(String, String)]) -> String {
    let mut headers = String::new();
    for (name, value) in extra_headers {
        headers.push_str(name);
        headers.push_str(": ");
        headers.push_str(value);
        headers.push_str("\r\n");
    }
    headers
}

fn http_response_body(response: &[u8]) -> Result<&[u8], String> {
    let Some(header_end) = response.windows(4).position(|window| window == b"\r\n\r\n") else {
        return Err("LAN peer response did not include HTTP headers".to_owned());
    };
    let headers = String::from_utf8_lossy(&response[..header_end]);
    let status_line = headers.lines().next().unwrap_or_default();
    if !status_line.contains(" 200 ") {
        return Err(format!("LAN peer returned {status_line}"));
    }
    Ok(&response[header_end + 4..])
}

fn parse_lan_peer_address(input: &str) -> Result<SocketAddr, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("LAN peer address is required".to_owned());
    }
    if trimmed.starts_with("https://") {
        return Err("LAN peer transport currently supports http:// only".to_owned());
    }
    let without_scheme = trimmed.strip_prefix("http://").unwrap_or(trimmed);
    let host_port = without_scheme
        .split_once('/')
        .map_or(without_scheme, |(host_port, _)| host_port);
    let address = host_port
        .parse::<SocketAddr>()
        .map_err(|error| format!("invalid LAN peer address {host_port:?}: {error}"))?;
    if address.port() == 0 {
        return Err("LAN peer address must include a nonzero port".to_owned());
    }
    if !is_allowed_lan_peer_ip(address.ip()) {
        return Err("LAN peer address must use a loopback, private, or link-local IP".to_owned());
    }
    Ok(address)
}

fn is_allowed_lan_peer_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        IpAddr::V6(ip) => {
            ip.is_loopback() || ip.is_unicast_link_local() || is_ipv6_unique_local(&ip)
        }
    }
}

fn is_ipv6_unique_local(ip: &std::net::Ipv6Addr) -> bool {
    (ip.segments()[0] & 0xfe00) == 0xfc00
}

fn lan_host_header(address: SocketAddr) -> String {
    if address.is_ipv6() {
        format!("[{}]:{}", address.ip(), address.port())
    } else {
        address.to_string()
    }
}

fn is_demo_peer(peer: &PeerRecord) -> bool {
    peer.device_id == demo_peer_device_id()
}

fn demo_peer_device_id() -> uuid::Uuid {
    uuid::Uuid::parse_str("00000000-0000-4000-8000-0000000000aa")
        .expect("demo peer device id is valid")
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

fn sync_lan_trust_error(state: &AppState, event_type: &str, error: impl Into<String>) -> Vec<u8> {
    let error = error.into();
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
        "LAN peer trust check failed",
        None,
        Some(error.clone()),
    );
    json_response_with_status(403, &json!({"ok": false, "error": error}))
}

fn sync_lan_transport_error(
    state: &AppState,
    event_type: &str,
    error: impl Into<String>,
) -> Vec<u8> {
    let error = error.into();
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
        "LAN peer transport failed",
        None,
        Some(error.clone()),
    );
    json_response_with_status(502, &json!({"ok": false, "error": error}))
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
            author_device_id: demo_peer_device_id(),
            source_device_id: demo_peer_device_id(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lan_peer_address_accepts_http_ipv4_and_ipv6_socket_addresses() {
        assert_eq!(
            parse_lan_peer_address("http://127.0.0.1:9468/api/sync/state").unwrap(),
            "127.0.0.1:9468".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(
            parse_lan_peer_address("192.168.1.25:9468").unwrap(),
            "192.168.1.25:9468".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(
            parse_lan_peer_address("[::1]:9469").unwrap(),
            "[::1]:9469".parse::<SocketAddr>().unwrap()
        );
        assert_eq!(
            parse_lan_peer_address("[fd00::1]:9469").unwrap(),
            "[fd00::1]:9469".parse::<SocketAddr>().unwrap()
        );
        assert!(parse_lan_peer_address("https://127.0.0.1:9468").is_err());
        assert!(parse_lan_peer_address("localhost:9468").is_err());
        assert!(parse_lan_peer_address("8.8.8.8:9468").is_err());
        assert!(parse_lan_peer_address("[2001:4860:4860::8888]:9468").is_err());
        assert!(parse_lan_peer_address("127.0.0.1:0").is_err());
    }

    #[test]
    fn http_response_body_requires_successful_status_and_headers() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n{}";
        assert_eq!(http_response_body(response).unwrap(), b"{}");
        let rejected = b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n";
        assert!(http_response_body(rejected).is_err());
        assert!(http_response_body(b"not http").is_err());
    }

    #[test]
    fn lan_endpoint_auth_from_headers_requires_device_and_nonce() {
        let device_id = uuid::Uuid::new_v4();
        let mut headers = HashMap::new();
        headers.insert(LAN_AUTH_DEVICE_ID_HEADER.to_owned(), device_id.to_string());
        headers.insert(
            LAN_AUTH_REPLAY_NONCE_HEADER.to_owned(),
            "request-nonce".to_owned(),
        );
        headers.insert(
            LAN_AUTH_SIGNATURE_VERSION_HEADER.to_owned(),
            LAN_AUTH_SIGNATURE_VERSION.to_owned(),
        );
        headers.insert(LAN_AUTH_SIGNATURE_HEADER.to_owned(), "a".repeat(64));

        let auth = lan_endpoint_auth_from_headers(&headers).unwrap();
        assert_eq!(auth.device_id, device_id);
        assert_eq!(auth.replay_nonce, "request-nonce");
        assert_eq!(auth.signature, "a".repeat(64));

        headers.remove(LAN_AUTH_REPLAY_NONCE_HEADER);
        assert!(lan_endpoint_auth_from_headers(&headers)
            .unwrap_err()
            .contains("replay nonce"));

        headers.insert(LAN_AUTH_REPLAY_NONCE_HEADER.to_owned(), "x".repeat(129));
        assert!(lan_endpoint_auth_from_headers(&headers)
            .unwrap_err()
            .contains("too long"));

        headers.insert(
            LAN_AUTH_DEVICE_ID_HEADER.to_owned(),
            "not-a-uuid".to_owned(),
        );
        assert!(lan_endpoint_auth_from_headers(&headers)
            .unwrap_err()
            .contains("device id"));

        headers.insert(LAN_AUTH_DEVICE_ID_HEADER.to_owned(), device_id.to_string());
        headers.insert(
            LAN_AUTH_REPLAY_NONCE_HEADER.to_owned(),
            "request-nonce".to_owned(),
        );
        headers.insert(LAN_AUTH_SIGNATURE_HEADER.to_owned(), "not-hex".to_owned());
        assert!(lan_endpoint_auth_from_headers(&headers)
            .unwrap_err()
            .contains("signature"));
    }

    #[test]
    fn lan_http_extra_headers_renders_http_header_lines() {
        let headers = vec![
            (LAN_AUTH_DEVICE_ID_HEADER.to_owned(), "device".to_owned()),
            (LAN_AUTH_REPLAY_NONCE_HEADER.to_owned(), "nonce".to_owned()),
            (
                LAN_AUTH_SIGNATURE_VERSION_HEADER.to_owned(),
                LAN_AUTH_SIGNATURE_VERSION.to_owned(),
            ),
            (LAN_AUTH_SIGNATURE_HEADER.to_owned(), "signature".to_owned()),
        ];
        let rendered = lan_http_extra_headers(&headers);
        assert!(rendered.contains("x-ke8ygw-lan-device-id: device\r\n"));
        assert!(rendered.contains("x-ke8ygw-lan-replay-nonce: nonce\r\n"));
        assert!(rendered.contains("x-ke8ygw-lan-signature-version: hmac-sha256-v1\r\n"));
        assert!(rendered.contains("x-ke8ygw-lan-signature: signature\r\n"));
    }

    #[test]
    fn discovery_source_requires_scoped_ipv6_link_local_addresses() {
        assert!(is_usable_discovery_source(
            "192.168.1.10:9737".parse().unwrap()
        ));
        assert!(is_usable_discovery_source(
            "[fd00::1]:9737".parse().unwrap()
        ));
        assert!(!is_usable_discovery_source(
            "[fe80::272f:463d:a6b2:5af7]:9737".parse().unwrap()
        ));
        assert!(is_usable_discovery_source(
            std::net::SocketAddrV6::new("fe80::272f:463d:a6b2:5af7".parse().unwrap(), 9737, 0, 12)
                .into()
        ));
    }

    #[test]
    fn discovery_api_address_uses_advertised_port() {
        let identity = LocalPeerIdentity::new("Peer", Some(9468));
        let packet = DiscoveryPacket::from_identity(&identity);
        assert_eq!(
            discovery_api_address(&packet, "192.168.1.10:50300".parse().unwrap()),
            "192.168.1.10:9468".parse::<SocketAddr>().unwrap()
        );
        let mut packet_without_port = packet;
        packet_without_port.local_api_port = None;
        assert_eq!(
            discovery_api_address(&packet_without_port, "192.168.1.10:50300".parse().unwrap()),
            "192.168.1.10:50300".parse::<SocketAddr>().unwrap()
        );
    }

    #[test]
    fn unsupported_discovery_packets_are_detected_before_recording() {
        let mut packet =
            DiscoveryPacket::from_identity(&LocalPeerIdentity::new("Peer", Some(9468)));
        assert!(is_supported_discovery_packet(&packet));
        packet.protocol_version = PROTOCOL_VERSION + 1;
        assert!(!is_supported_discovery_packet(&packet));
        packet.protocol_version = PROTOCOL_VERSION;
        packet.protocol_name = "other-sync".to_owned();
        assert!(!is_supported_discovery_packet(&packet));
    }

    #[test]
    fn lan_api_address_rank_prefers_ipv4_private_before_ipv6_link_local() {
        let mut peer = PeerRecord {
            peer_id: "peer".to_owned(),
            device_id: uuid::Uuid::new_v4(),
            session_id: uuid::Uuid::new_v4(),
            display_name: "Peer".to_owned(),
            addresses: vec![
                std::net::SocketAddrV6::new(
                    "fe80::272f:463d:a6b2:5af7".parse().unwrap(),
                    9738,
                    0,
                    12,
                )
                .into(),
                "169.254.38.39:9738".parse().unwrap(),
                "192.168.1.25:9738".parse().unwrap(),
            ],
            protocol_version: PROTOCOL_VERSION,
            capabilities: Vec::new(),
            first_seen: chrono::Utc::now(),
            last_seen: chrono::Utc::now(),
            connection_state: ham_sync::PeerConnectionState::Discovered,
            sync_state: ham_sync::PeerSyncState::Unknown,
        };
        let sorted = sorted_peer_api_addresses(&peer);
        assert_eq!(sorted[0], "192.168.1.25:9738".parse().unwrap());
        assert_eq!(sorted[1], "169.254.38.39:9738".parse().unwrap());
        peer.addresses.push("127.0.0.1:9738".parse().unwrap());
        let sorted = sorted_peer_api_addresses(&peer);
        assert_eq!(sorted[0], "127.0.0.1:9738".parse().unwrap());
    }
}
