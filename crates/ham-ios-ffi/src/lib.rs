//! C ABI bridge for the native iOS client.
//!
//! Swift owns UI, navigation, Apple APIs, Keychain, notifications, and Files
//! integration. The bridge keeps business operations in Rust by exposing a
//! bounded JSON command ABI over stable C symbols.

use std::{
    ffi::{CStr, CString},
    os::raw::{c_char, c_uchar},
    panic::{catch_unwind, AssertUnwindSafe},
    path::{Path, PathBuf},
    str,
};

use chrono::Utc;
use ham_core::CallsignLookupProvider;
use ham_core::{
    default_service_registry, encode_maidenhead, export_adif, grid_to_lat_lon, infer_band,
    maidenhead_to_coordinate, map_provider_metadata, mock_propagation_forecast, mock_weather,
    online_provider_metadata, parse_adif, qso_map_objects, station_markers_from_profiles,
    submit_proposal, validate_grid, ApplicationSettings, Coordinate, EquipmentItem, EquipmentType,
    InMemoryEventBus, JsonStationBookStore, JsonSupportStore, JsonlLogbookEventStore,
    LocalPrefixProvider, LogbookEventStore, MapLayerStack, OperatorRole, ProposalContext,
    QsoCurrentStateProjection, QsoRecord, StationBook, StationConfiguration, StationProfile,
    UploadQueue, UploadTarget,
};
use ham_plugin_sdk::{
    PluginCapability, PluginManifest, ProposalEnvelope, OFFICIAL_LOG_QSO_CREATED,
    PROPOSAL_ACTIVATION_CANCEL, PROPOSAL_ACTIVATION_CREATE, PROPOSAL_ACTIVATION_END,
    PROPOSAL_ACTIVATION_NOTE_ADD, PROPOSAL_ACTIVATION_START, PROPOSAL_ACTIVATION_UPDATE,
    PROPOSAL_NET_CHECKIN_CREATE, PROPOSAL_NET_CHECKIN_DELETE, PROPOSAL_NET_CHECKIN_UPDATE,
    PROPOSAL_NET_REPORT_EXPORT, PROPOSAL_NET_SESSION_CANCEL, PROPOSAL_NET_SESSION_END,
    PROPOSAL_NET_SESSION_START, PROPOSAL_NET_TEMPLATE_CREATE, PROPOSAL_NET_TEMPLATE_UPDATE,
    PROPOSAL_NET_TRAFFIC_CREATE, PROPOSAL_NET_TRAFFIC_UPDATE, PROPOSAL_QSO_ACTIVATION_LINK,
    PROPOSAL_QSO_ACTIVATION_UNLINK, PROPOSAL_QSO_CORRECT, PROPOSAL_QSO_CREATE, PROPOSAL_QSO_DELETE,
    PROPOSAL_QSO_NOTE_ADD, PROPOSAL_QSO_RESTORE,
};
use ham_sync::{
    CloudConnectionState, CloudSyncConfig, ConflictReviewStatus, JsonConflictReviewStore,
    JsonOfflineMutationQueue, LocalPeerIdentity, ManualConflictResolution,
    ManualConflictResolutionChoice, OfflineMutationEnvelope, OfflineMutationInput, SyncConfig,
    SyncConflictReport, MAX_CONFLICT_REVIEW_NOTE_BYTES, OFFLINE_OP_ACTIVATION_END,
    OFFLINE_OP_ACTIVATION_START, OFFLINE_OP_NET_CHECKIN_CREATE, OFFLINE_OP_NET_CHECKIN_DELETE,
    OFFLINE_OP_NET_SESSION_END, OFFLINE_OP_NET_SESSION_START, OFFLINE_OP_NET_TRAFFIC_CREATE,
    OFFLINE_OP_QSO_CORRECT, OFFLINE_OP_QSO_CREATE, OFFLINE_OP_QSO_DELETE, OFFLINE_OP_QSO_NOTE_ADD,
    OFFLINE_OP_QSO_RESTORE, OFFLINE_OP_STATION_EQUIPMENT_CREATE, OFFLINE_OP_STATION_PROFILE_CREATE,
    OFFLINE_OP_STATION_PROFILE_SELECT,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::runtime::Runtime;
use uuid::Uuid;

const CORE_VERSION: &str = env!("CARGO_PKG_VERSION");
const ABI_VERSION: u32 = 1;
const BRIDGE_SCHEMA_VERSION: u32 = 1;
const IOS_BRIDGE_VERSION: u32 = 1;
const BACKUP_SCHEMA_VERSION: u32 = 1;
const MAX_INPUT_BYTES: usize = 1024 * 1024;
const IOS_PLUGIN_ID: &str = "plugin.ios.native";
const DEFAULT_LOGBOOK_ID: &str = "00000000-0000-4000-8000-000000000001";

#[derive(Debug, Serialize)]
struct BridgeEnvelope {
    ok: bool,
    bridge_version: u32,
    abi_version: u32,
    schema_version: u32,
    generated_at: String,
    data: Option<Value>,
    error: Option<BridgeErrorPayload>,
    correlation_id: String,
}

#[derive(Debug, Serialize)]
struct BridgeErrorPayload {
    code: String,
    message: String,
    details: Value,
}

#[derive(Debug)]
struct BridgeFault {
    code: &'static str,
    message: String,
    details: Value,
}

impl BridgeFault {
    fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_input",
            message: message.into(),
            details: json!({}),
        }
    }

    fn invalid_json(message: impl Into<String>) -> Self {
        Self {
            code: "invalid_json",
            message: message.into(),
            details: json!({}),
        }
    }

    fn unsupported_command(command: impl Into<String>) -> Self {
        Self {
            code: "unsupported_command",
            message: "unsupported Rust bridge command".to_owned(),
            details: json!({"command": command.into()}),
        }
    }

    fn storage(message: impl Into<String>) -> Self {
        Self {
            code: "storage_error",
            message: message.into(),
            details: json!({}),
        }
    }

    fn domain(message: impl Into<String>) -> Self {
        Self {
            code: "domain_rejected",
            message: message.into(),
            details: json!({}),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            code: "internal_error",
            message: message.into(),
            details: json!({}),
        }
    }
}

#[derive(Debug, Deserialize)]
struct BridgeCall {
    command: String,
    correlation_id: Option<Uuid>,
    payload: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct QsoMutationRequest {
    app_support_dir: String,
    #[serde(default)]
    logbook_id: Option<Uuid>,
    #[serde(default)]
    device_id: Option<Uuid>,
    #[serde(default)]
    operation_id: Option<String>,
    #[serde(default)]
    qso: Value,
}

#[derive(Debug, Deserialize)]
struct QsoDeleteRequest {
    app_support_dir: String,
    qso_id: Uuid,
    #[serde(default)]
    logbook_id: Option<Uuid>,
    #[serde(default)]
    device_id: Option<Uuid>,
    #[serde(default)]
    operation_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct QsoListRequest {
    app_support_dir: String,
    #[serde(default)]
    logbook_id: Option<Uuid>,
    #[serde(default)]
    include_deleted: bool,
}

#[derive(Debug, Deserialize)]
struct StationProfileCreateRequest {
    app_support_dir: String,
    #[serde(default)]
    logbook_id: Option<Uuid>,
    #[serde(default)]
    device_id: Option<Uuid>,
    #[serde(default)]
    operation_id: Option<String>,
    #[serde(default)]
    station_profile_id: Option<Uuid>,
    display_name: String,
    station_callsign: String,
    #[serde(default)]
    operator_callsign: Option<String>,
    #[serde(default)]
    profile_type: Option<String>,
    #[serde(default)]
    default_grid: Option<String>,
    #[serde(default)]
    default_qth: Option<String>,
    #[serde(default)]
    default_power_watts: Option<u32>,
    #[serde(default)]
    notes: Option<String>,
    #[serde(default)]
    active: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct StationEquipmentCreateRequest {
    app_support_dir: String,
    #[serde(default)]
    logbook_id: Option<Uuid>,
    #[serde(default)]
    device_id: Option<Uuid>,
    #[serde(default)]
    operation_id: Option<String>,
    #[serde(default)]
    equipment_id: Option<Uuid>,
    equipment_type: String,
    display_name: String,
    #[serde(default)]
    manufacturer: Option<String>,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    serial_number: Option<String>,
    #[serde(default)]
    capabilities: Vec<String>,
    #[serde(default)]
    notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StationSelectProfileRequest {
    app_support_dir: String,
    #[serde(default)]
    logbook_id: Option<Uuid>,
    #[serde(default)]
    device_id: Option<Uuid>,
    #[serde(default)]
    operation_id: Option<String>,
    station_profile_id: Uuid,
}

#[derive(Debug, Deserialize)]
struct SyncSnapshotRequest {
    #[serde(default)]
    app_support_dir: Option<String>,
    #[serde(default)]
    logbook_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct ConflictReviewCreateRequest {
    app_support_dir: String,
    report: SyncConflictReport,
}

#[derive(Debug, Deserialize)]
struct ConflictReviewResolveRequest {
    app_support_dir: String,
    review_id: Uuid,
    resolution: ManualConflictResolution,
}

#[derive(Debug, Deserialize)]
struct ConflictReviewCorrectiveEventsRequest {
    app_support_dir: String,
    #[serde(default)]
    logbook_id: Option<Uuid>,
    #[serde(default)]
    device_id: Option<Uuid>,
    review_id: Uuid,
    #[serde(default)]
    operator_note: Option<String>,
    #[serde(default)]
    proposals: Vec<IosCorrectiveProposalRequest>,
}

#[derive(Debug, Deserialize)]
struct IosCorrectiveProposalRequest {
    proposal_type: String,
    entity_id: Option<Uuid>,
    #[serde(default)]
    operation_id: Option<String>,
    #[serde(default)]
    payload: Value,
}

#[derive(Debug, Deserialize)]
struct ApplicationSettingsUpdateRequest {
    app_support_dir: String,
    settings: ApplicationSettings,
}

#[no_mangle]
pub extern "C" fn ham_ios_abi_version() -> u32 {
    ABI_VERSION
}

#[no_mangle]
/// # Safety
///
/// `ptr` must be a pointer returned by this crate's string-returning FFI
/// functions and must not have already been freed. Passing any other pointer
/// is undefined behavior.
pub unsafe extern "C" fn ham_ios_free_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(ptr));
    }
}

/// Preferred command ABI for Swift.
///
/// The input buffer is copied synchronously by Rust, must be UTF-8 JSON, and
/// must not exceed `MAX_INPUT_BYTES`. Rust owns the returned C string; Swift
/// must release it with `ham_ios_free_string`.
#[no_mangle]
pub extern "C" fn ham_ios_call_json_bytes(ptr: *const c_uchar, len: usize) -> *mut c_char {
    let correlation_id = Uuid::new_v4();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let input = input_bytes(ptr, len)?;
        let call = parse_call(input)?;
        let correlation_id = call.correlation_id.unwrap_or(correlation_id);
        dispatch_call(call, correlation_id).map(|data| (correlation_id, data))
    }));
    match result {
        Ok(Ok((correlation_id, data))) => response_ok(correlation_id, data),
        Ok(Err(error)) => response_err(correlation_id, error),
        Err(_) => response_err(
            correlation_id,
            BridgeFault::internal("panic contained at Rust FFI boundary"),
        ),
    }
}

/// Compatibility C-string command ABI. New Swift code should use
/// `ham_ios_call_json_bytes`.
#[no_mangle]
pub extern "C" fn ham_ios_call_json(input: *const c_char) -> *mut c_char {
    with_string_input(input, |input| {
        let call = parse_call(&input)?;
        let correlation_id = call.correlation_id.unwrap_or_else(Uuid::new_v4);
        dispatch_call(call, correlation_id).map(|data| (correlation_id, data))
    })
}

#[no_mangle]
pub extern "C" fn ham_ios_version_json() -> *mut c_char {
    with_no_input(version_payload)
}

#[no_mangle]
pub extern "C" fn ham_ios_dashboard_snapshot_json() -> *mut c_char {
    with_no_input(dashboard_snapshot_payload)
}

#[no_mangle]
pub extern "C" fn ham_ios_station_book_json() -> *mut c_char {
    with_no_input(|| Ok(json!(default_station_book())))
}

#[no_mangle]
pub extern "C" fn ham_ios_provider_status_json() -> *mut c_char {
    with_no_input(provider_status_payload)
}

#[no_mangle]
pub extern "C" fn ham_ios_map_snapshot_json() -> *mut c_char {
    with_no_input(map_snapshot_payload)
}

#[no_mangle]
pub extern "C" fn ham_ios_sync_snapshot_json() -> *mut c_char {
    with_no_input(sync_snapshot_payload)
}

#[no_mangle]
pub extern "C" fn ham_ios_diagnostics_json() -> *mut c_char {
    with_no_input(|| diagnostics_payload(None))
}

#[no_mangle]
pub extern "C" fn ham_ios_lookup_callsign_json(callsign: *const c_char) -> *mut c_char {
    with_string_input(callsign, |callsign| {
        lookup_callsign_payload(&callsign).map(|data| (Uuid::new_v4(), data))
    })
}

#[no_mangle]
pub extern "C" fn ham_ios_grid_info_json(grid: *const c_char) -> *mut c_char {
    with_string_input(grid, |grid| {
        Ok((
            Uuid::new_v4(),
            json!({
                "grid": grid,
                "valid": validate_grid(&grid),
                "coordinate": grid_to_lat_lon(&grid).ok(),
                "map_coordinate": maidenhead_to_coordinate(&grid).ok()
            }),
        ))
    })
}

#[no_mangle]
pub extern "C" fn ham_ios_infer_band_json(frequency_hz: u64) -> *mut c_char {
    with_no_input(|| {
        Ok(json!({
            "frequency_hz": frequency_hz,
            "band": infer_band(frequency_hz)
        }))
    })
}

#[no_mangle]
pub extern "C" fn ham_ios_parse_adif_json(input: *const c_char) -> *mut c_char {
    with_string_input(input, |input| {
        Ok((
            Uuid::new_v4(),
            json!({
                "records": parse_adif(&input)
            }),
        ))
    })
}

#[no_mangle]
pub extern "C" fn ham_ios_export_adif_json(qsos_json: *const c_char) -> *mut c_char {
    with_string_input(qsos_json, |input| {
        export_adif_payload(&input).map(|data| (Uuid::new_v4(), data))
    })
}

fn with_no_input<F>(operation: F) -> *mut c_char
where
    F: FnOnce() -> Result<Value, BridgeFault>,
{
    let correlation_id = Uuid::new_v4();
    match catch_unwind(AssertUnwindSafe(operation)) {
        Ok(Ok(data)) => response_ok(correlation_id, data),
        Ok(Err(error)) => response_err(correlation_id, error),
        Err(_) => response_err(
            correlation_id,
            BridgeFault::internal("panic contained at Rust FFI boundary"),
        ),
    }
}

fn with_string_input<F>(input: *const c_char, operation: F) -> *mut c_char
where
    F: FnOnce(String) -> Result<(Uuid, Value), BridgeFault>,
{
    let correlation_id = Uuid::new_v4();
    let result = catch_unwind(AssertUnwindSafe(|| {
        let input = input_c_string(input)?;
        operation(input)
    }));
    match result {
        Ok(Ok((correlation_id, data))) => response_ok(correlation_id, data),
        Ok(Err(error)) => response_err(correlation_id, error),
        Err(_) => response_err(
            correlation_id,
            BridgeFault::internal("panic contained at Rust FFI boundary"),
        ),
    }
}

fn dispatch_call(call: BridgeCall, correlation_id: Uuid) -> Result<Value, BridgeFault> {
    #[cfg(test)]
    if call.command == "__test_panic" {
        panic!("intentional FFI boundary panic test");
    }

    let payload = call.payload.unwrap_or_else(|| json!({}));
    match call.command.as_str() {
        "version" => version_payload(),
        "bridge.self_test" => bridge_self_test_payload(),
        "dashboard.snapshot" => dashboard_snapshot_payload(),
        "station.book" => station_book_command_payload(payload),
        "provider.status" => provider_status_payload(),
        "map.snapshot" => map_snapshot_payload(),
        "sync.snapshot" => sync_snapshot_command(payload),
        "sync.offline_queue.snapshot" => sync_snapshot_command(payload),
        "sync.offline_queue.recover" => sync_offline_queue_recover_command(payload),
        "sync.conflict_reviews.snapshot" => sync_snapshot_command(payload),
        "sync.conflict_reviews.create" => sync_conflict_review_create_command(payload),
        "sync.conflict_reviews.resolve" => sync_conflict_review_resolve_command(payload),
        "sync.conflict_reviews.corrective_events" => {
            sync_conflict_review_corrective_events_command(payload, correlation_id)
        }
        "diagnostics.snapshot" => diagnostics_command_payload(payload),
        "settings.get" => settings_get_command(payload),
        "settings.create_default" => settings_create_default_command(payload),
        "settings.update" => settings_update_command(payload),
        "lookup.callsign" => {
            let callsign = string_field(&payload, "callsign")?;
            lookup_callsign_payload(&callsign)
        }
        "grid.info" => {
            let grid = string_field(&payload, "grid")?;
            Ok(json!({
                "grid": grid,
                "valid": validate_grid(&grid),
                "coordinate": grid_to_lat_lon(&grid).ok(),
                "map_coordinate": maidenhead_to_coordinate(&grid).ok()
            }))
        }
        "adif.export" => {
            let records = payload.get("records").cloned().unwrap_or_else(|| json!([]));
            export_adif_payload(&records.to_string())
        }
        "adif.parse" => {
            let adif = payload
                .get("adif")
                .and_then(Value::as_str)
                .ok_or_else(|| BridgeFault::invalid_input("field `adif` is required"))?;
            Ok(json!({
                "records": parse_adif(adif)
            }))
        }
        "qso.create" => create_qso_command(payload, correlation_id),
        "qso.delete" => delete_qso_command(payload, correlation_id),
        "qso.list" => list_qsos_command(payload),
        "station.profile.create" => station_profile_create_command(payload, correlation_id),
        "station.equipment.create" => station_equipment_create_command(payload, correlation_id),
        "station.profile.select" => station_profile_select_command(payload, correlation_id),
        "activation.start" => {
            domain_proposal_command(payload, PROPOSAL_ACTIVATION_START, None, correlation_id)
        }
        "activation.end" => domain_proposal_command(
            payload,
            PROPOSAL_ACTIVATION_END,
            Some("activation_id"),
            correlation_id,
        ),
        "net.session.start" => {
            domain_proposal_command(payload, PROPOSAL_NET_SESSION_START, None, correlation_id)
        }
        "net.session.end" => domain_proposal_command(
            payload,
            PROPOSAL_NET_SESSION_END,
            Some("net_session_id"),
            correlation_id,
        ),
        "net.checkin.create" => {
            domain_proposal_command(payload, PROPOSAL_NET_CHECKIN_CREATE, None, correlation_id)
        }
        "net.traffic.create" => {
            domain_proposal_command(payload, PROPOSAL_NET_TRAFFIC_CREATE, None, correlation_id)
        }
        command => Err(BridgeFault::unsupported_command(command)),
    }
}

fn version_payload() -> Result<Value, BridgeFault> {
    Ok(json!({
        "app": "KE8YGW Logger",
        "core_version": CORE_VERSION,
        "bridge_version": IOS_BRIDGE_VERSION,
        "abi_version": ABI_VERSION,
        "bridge_schema_version": BRIDGE_SCHEMA_VERSION,
        "sync_protocol_version": ham_sync::PROTOCOL_VERSION,
        "backup_schema_version": BACKUP_SCHEMA_VERSION,
        "rust_modules": [
            "ham-core",
            "ham-sync",
            "ham-plugin-sdk"
        ],
        "contract": "swiftui -> observable view models -> rust ffi -> ham-core",
        "build_target": build_target()
    }))
}

fn bridge_self_test_payload() -> Result<Value, BridgeFault> {
    let runtime = Runtime::new().map_err(|error| BridgeFault::internal(error.to_string()))?;
    let store = ham_core::InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let logbook_id = default_logbook_id();
    let device_id = Uuid::new_v4();
    let proposal = ProposalEnvelope::new(
        PROPOSAL_QSO_CREATE,
        logbook_id,
        None,
        None,
        device_id,
        IOS_PLUGIN_ID,
        1,
        json!({
            "station_callsign": "KE8YGW",
            "operator_callsign": "KE8YGW",
            "contacted_callsign": "W1AW",
            "started_at": Utc::now().to_rfc3339(),
            "mode": "SSB",
            "source": "ios.bridge.self_test"
        }),
    );
    let outcome = runtime
        .block_on(submit_proposal(
            &store,
            &bus,
            &ios_proposal_context(),
            proposal,
        ))
        .map_err(|error| BridgeFault::domain(error.to_string()))?;
    let projection = runtime
        .block_on(store.rebuild_projections(logbook_id))
        .map_err(|error| BridgeFault::storage(error.to_string()))?;
    Ok(json!({
        "success": true,
        "library_linked": true,
        "abi_version": ABI_VERSION,
        "bridge_schema_version": BRIDGE_SCHEMA_VERSION,
        "core_version": CORE_VERSION,
        "sync_protocol_version": ham_sync::PROTOCOL_VERSION,
        "backup_schema_version": BACKUP_SCHEMA_VERSION,
        "build_target": build_target(),
        "json_round_trip": true,
        "error_round_trip": true,
        "allocation_model": "rust_allocated_c_string_swift_frees_with_ham_ios_free_string",
        "harmless_domain_operation": {
            "event_type": outcome.official_event.event_type,
            "event_id": outcome.official_event.event_id,
            "projection_count": projection.list(false).len()
        }
    }))
}

fn dashboard_snapshot_payload() -> Result<Value, BridgeFault> {
    let station = default_station_book();
    let active_profile = station.active_profile().cloned();
    let active_configuration = station.active_configuration().cloned();
    let service_registry = default_service_registry().snapshot();
    let providers = online_provider_metadata();
    let upload_queue = default_upload_queue(&providers);
    let sync_config = SyncConfig::default();
    let cloud_config = CloudSyncConfig::default();
    let map_coordinate = active_profile
        .as_ref()
        .and_then(|profile| profile.default_grid.as_deref())
        .and_then(|grid| maidenhead_to_coordinate(grid).ok())
        .unwrap_or(Coordinate {
            latitude: 41.0,
            longitude: -81.0,
        });

    Ok(json!({
        "operator": active_profile.as_ref().and_then(|profile| profile.operator_callsign.clone()).unwrap_or_else(|| "KE8YGW".to_owned()),
        "active_station": active_profile,
        "active_configuration": active_configuration,
        "current_profile": "Home Station",
        "gps": {
            "available": true,
            "source": "ios-core-location",
            "coordinate": {
                "latitude": map_coordinate.latitude,
                "longitude": map_coordinate.longitude
            },
            "grid": encode_maidenhead(map_coordinate, 6).unwrap_or_else(|_| "unknown".to_owned())
        },
        "recent_qsos": Vec::<Value>::new(),
        "pending_uploads": upload_queue.jobs.len(),
        "provider_status": provider_status_summary(&service_registry),
        "sync_status": {
            "mode": "offline_first",
            "lan_discovery": sync_config.enable_lan_discovery,
            "cloud_connection_state": CloudConnectionState::Disconnected,
            "server_url": cloud_config.sync_server_url,
            "pending_changes": 0,
            "conflicts": 0
        },
        "offline": true,
        "battery": {
            "source": "ios-uidevice",
            "status": "provided_by_swift"
        },
        "network": {
            "source": "ios-network-framework",
            "status": "provided_by_swift"
        },
        "capabilities": [
            "casual_logging",
            "portable_logging",
            "pota",
            "sota",
            "net_control",
            "provider_status",
            "maps",
            "diagnostics",
            "hosted_sync_model"
        ]
    }))
}

fn station_book_command_payload(payload: Value) -> Result<Value, BridgeFault> {
    if let Some(app_support_dir) = payload
        .get("app_support_dir")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    {
        let store = station_store(app_support_dir)?;
        let mut book = store
            .load()
            .map_err(|error| BridgeFault::storage(error.to_string()))?;
        ensure_station_book_seeded(&store, &mut book)?;
        Ok(json!(book))
    } else {
        Ok(json!(default_station_book()))
    }
}

fn settings_get_command(payload: Value) -> Result<Value, BridgeFault> {
    let app_support_dir = string_field(&payload, "app_support_dir")?;
    let store = settings_store(&app_support_dir)?;
    if !store.path().exists() {
        return Ok(json!({
            "exists": false,
            "created": false,
            "settings": null,
            "record_count": 0
        }));
    }
    let settings = store
        .load()
        .map_err(|error| BridgeFault::storage(error.to_string()))?
        .normalized()
        .map_err(|error| BridgeFault::invalid_input(error.to_string()))?;
    Ok(json!({
        "exists": true,
        "created": false,
        "settings": settings,
        "record_count": 1
    }))
}

fn settings_create_default_command(payload: Value) -> Result<Value, BridgeFault> {
    let app_support_dir = string_field(&payload, "app_support_dir")?;
    let store = settings_store(&app_support_dir)?;
    if store.path().exists() {
        let settings = store
            .load()
            .map_err(|error| BridgeFault::storage(error.to_string()))?
            .normalized()
            .map_err(|error| BridgeFault::invalid_input(error.to_string()))?;
        return Ok(json!({
            "exists": true,
            "created": false,
            "settings": settings,
            "record_count": 1
        }));
    }
    let settings = ApplicationSettings::default()
        .normalized()
        .map_err(|error| BridgeFault::invalid_input(error.to_string()))?;
    store
        .save(&settings)
        .map_err(|error| BridgeFault::storage(error.to_string()))?;
    Ok(json!({
        "exists": true,
        "created": true,
        "settings": settings,
        "record_count": 1
    }))
}

fn settings_update_command(payload: Value) -> Result<Value, BridgeFault> {
    let request: ApplicationSettingsUpdateRequest = serde_json::from_value(payload)
        .map_err(|error| BridgeFault::invalid_input(error.to_string()))?;
    let store = settings_store(&request.app_support_dir)?;
    let existing = if store.path().exists() {
        Some(
            store
                .load()
                .map_err(|error| BridgeFault::storage(error.to_string()))?,
        )
    } else {
        None
    };
    let mut settings = request
        .settings
        .normalized()
        .map_err(|error| BridgeFault::invalid_input(error.to_string()))?;
    if let Some(existing) = existing {
        settings.created_at = existing.created_at;
    }
    store
        .save(&settings)
        .map_err(|error| BridgeFault::storage(error.to_string()))?;
    Ok(json!({
        "exists": true,
        "created": false,
        "settings": settings,
        "record_count": 1
    }))
}

fn provider_status_payload() -> Result<Value, BridgeFault> {
    let registry = default_service_registry().snapshot();
    let online = online_provider_metadata();
    let queue = default_upload_queue(&online);

    Ok(json!({
        "service_registry": registry,
        "online_providers": online,
        "upload_queue": queue,
        "api_status": {
            "qrz": "stub_requires_credentials",
            "hamqth": "stub_requires_credentials",
            "pota": "provider_ready_for_network_adapter",
            "sotawatch": "provider_ready_for_network_adapter",
            "dx_cluster": "offline_parser_ready",
            "club_log": "stub_requires_credentials",
            "qrz_logbook": "stub_requires_credentials",
            "eqsl": "stub_requires_credentials",
            "lotw": "stub_requires_credentials"
        },
        "credential_states": {
            "qrz": "missing",
            "hamqth": "missing",
            "club_log": "missing",
            "qrz_logbook": "missing",
            "eqsl": "missing",
            "lotw": "missing"
        }
    }))
}

fn map_snapshot_payload() -> Result<Value, BridgeFault> {
    let station = default_station_book();
    let coordinate = station
        .active_profile()
        .and_then(|profile| profile.default_grid.as_deref())
        .and_then(|grid| maidenhead_to_coordinate(grid).ok())
        .unwrap_or(Coordinate {
            latitude: 41.0,
            longitude: -81.0,
        });
    let qso_projection = QsoCurrentStateProjection::new();
    let qso_objects = qso_map_objects(&qso_projection, Some(coordinate), None);
    let station_profiles = station
        .profiles
        .iter()
        .filter_map(|profile| serde_json::to_value(profile).ok())
        .collect::<Vec<_>>();
    let providers = [
        map_provider_metadata(
            "offline-map",
            "Offline Placeholder Map",
            vec!["map.tiles.offline".to_owned(), "map.raster".to_owned()],
            90,
            true,
            false,
        ),
        map_provider_metadata(
            "open-street-map",
            "OpenStreetMap Placeholder",
            vec!["map.tiles.online".to_owned(), "map.raster".to_owned()],
            50,
            false,
            true,
        ),
        map_provider_metadata(
            "mock-map",
            "Mock Map Provider",
            vec!["map.tiles.offline".to_owned(), "map.vector".to_owned()],
            10,
            true,
            false,
        ),
    ];

    Ok(json!({
        "providers": providers,
        "layers": MapLayerStack::default_layers(),
        "qso_objects": qso_objects,
        "station_markers": station_markers_from_profiles(&station_profiles),
        "weather": mock_weather(coordinate),
        "propagation": mock_propagation_forecast(),
        "status": {
            "grid": encode_maidenhead(coordinate, 6).unwrap_or_else(|_| "unknown".to_owned()),
            "coordinates": {
                "latitude": coordinate.latitude,
                "longitude": coordinate.longitude
            },
            "distance": "n/a",
            "bearing": "n/a",
            "zoom": "8",
            "selected_layer": "Stations"
        }
    }))
}

fn sync_snapshot_payload() -> Result<Value, BridgeFault> {
    sync_snapshot_for_support(None, default_logbook_id())
}

fn sync_snapshot_command(payload: Value) -> Result<Value, BridgeFault> {
    let request: SyncSnapshotRequest = serde_json::from_value(payload).map_err(|error| {
        BridgeFault::invalid_json(format!("invalid sync snapshot payload: {error}"))
    })?;
    sync_snapshot_for_support(
        request.app_support_dir.as_deref(),
        request.logbook_id.unwrap_or_else(default_logbook_id),
    )
}

fn sync_offline_queue_recover_command(payload: Value) -> Result<Value, BridgeFault> {
    let request: SyncSnapshotRequest = serde_json::from_value(payload).map_err(|error| {
        BridgeFault::invalid_json(format!("invalid sync recovery payload: {error}"))
    })?;
    let app_support_dir = request
        .app_support_dir
        .as_deref()
        .ok_or_else(|| BridgeFault::invalid_input("app_support_dir is required"))?;
    let queue = offline_queue(app_support_dir)?;
    let now = Utc::now();
    let recovery = queue
        .recover_or_initialize(now)
        .map_err(|error| BridgeFault::storage(error.to_string()))?;
    let mut snapshot = sync_snapshot_for_support(
        Some(app_support_dir),
        request.logbook_id.unwrap_or_else(default_logbook_id),
    )?;
    snapshot["recovered_count"] = json!(recovery.recovered_interrupted_writes);
    snapshot["recovery"] = json!(recovery);
    Ok(snapshot)
}

fn sync_conflict_review_create_command(payload: Value) -> Result<Value, BridgeFault> {
    let request: ConflictReviewCreateRequest =
        serde_json::from_value(payload).map_err(|error| {
            BridgeFault::invalid_json(format!("invalid conflict review create payload: {error}"))
        })?;
    let store = conflict_review_store(&request.app_support_dir)?;
    let review = store
        .create_review(request.report, Utc::now())
        .map_err(|error| BridgeFault::storage(error.to_string()))?;
    let snapshot = store
        .load_snapshot()
        .map_err(|error| BridgeFault::storage(error.to_string()))?;
    Ok(json!({
        "conflict_review": review,
        "conflict_reviews": snapshot
    }))
}

fn sync_conflict_review_resolve_command(payload: Value) -> Result<Value, BridgeFault> {
    let request: ConflictReviewResolveRequest =
        serde_json::from_value(payload).map_err(|error| {
            BridgeFault::invalid_json(format!(
                "invalid conflict review resolution payload: {error}"
            ))
        })?;
    let store = conflict_review_store(&request.app_support_dir)?;
    let review = store
        .resolve_review(request.review_id, request.resolution, Utc::now())
        .map_err(|error| BridgeFault::storage(error.to_string()))?;
    let snapshot = store
        .load_snapshot()
        .map_err(|error| BridgeFault::storage(error.to_string()))?;
    Ok(json!({
        "conflict_review": review,
        "conflict_reviews": snapshot
    }))
}

fn sync_conflict_review_corrective_events_command(
    payload: Value,
    correlation_id: Uuid,
) -> Result<Value, BridgeFault> {
    let request: ConflictReviewCorrectiveEventsRequest =
        serde_json::from_value(payload).map_err(|error| {
            BridgeFault::invalid_json(format!(
                "invalid conflict review corrective-events payload: {error}"
            ))
        })?;
    if request.proposals.is_empty() {
        return Err(BridgeFault::invalid_input(
            "at least one corrective proposal is required",
        ));
    }
    if request
        .operator_note
        .as_ref()
        .is_some_and(|note| note.len() > MAX_CONFLICT_REVIEW_NOTE_BYTES)
    {
        return Err(BridgeFault::invalid_input(
            "conflict review note is too large",
        ));
    }
    for proposal in &request.proposals {
        let proposal_type = proposal.proposal_type.trim();
        if proposal_type.is_empty() {
            return Err(BridgeFault::invalid_input(
                "corrective proposal_type is required",
            ));
        }
        if !is_supported_ios_corrective_proposal(proposal_type) {
            return Err(BridgeFault::invalid_input(format!(
                "unsupported corrective proposal type `{proposal_type}`"
            )));
        }
        if !proposal.payload.is_object() {
            return Err(BridgeFault::invalid_input(
                "corrective proposal payload must be a JSON object",
            ));
        }
        if corrective_proposal_requires_entity_id(proposal_type) && proposal.entity_id.is_none() {
            return Err(BridgeFault::invalid_input(
                "corrective proposal entity_id is required for this proposal type",
            ));
        }
        if let Some(operation_id) = proposal
            .operation_id
            .as_ref()
            .filter(|value| !value.trim().is_empty())
        {
            Uuid::parse_str(operation_id).map_err(|_| {
                BridgeFault::invalid_input("corrective proposal operation_id must be a UUID")
            })?;
        }
    }

    let review_store = conflict_review_store(&request.app_support_dir)?;
    let review_snapshot = review_store
        .load_snapshot()
        .map_err(|error| BridgeFault::storage(error.to_string()))?;
    if !review_snapshot.reviews.iter().any(|review| {
        review.review_id == request.review_id && review.status == ConflictReviewStatus::Open
    }) {
        return Err(BridgeFault::invalid_input(
            "open conflict review was not found",
        ));
    }

    let runtime = Runtime::new().map_err(|error| BridgeFault::internal(error.to_string()))?;
    runtime.block_on(async move {
        let logbook_id = request.logbook_id.unwrap_or_else(default_logbook_id);
        let device_id = request.device_id.unwrap_or_else(Uuid::new_v4);
        let event_store = event_store(&request.app_support_dir)?;
        let mut corrective_events = Vec::new();
        let mut corrective_event_hashes = Vec::new();
        let mut offline_mutations = Vec::new();

        for proposal in request.proposals {
            let proposal_type = proposal.proposal_type.trim().to_owned();
            let operation_id = proposal
                .operation_id
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let proposal_payload = corrective_payload_with_entity_id(
                &proposal_type,
                proposal.payload,
                proposal.entity_id,
            )?;
            let operation_type = ios_corrective_offline_operation_type(&proposal_type);
            let queued = enqueue_ios_mutation_with_input(
                &request.app_support_dir,
                IosMutationQueueInput {
                    logbook_id,
                    device_id,
                    operation_type: &operation_type,
                    payload: proposal_payload.clone(),
                    external_operation_id: &operation_id,
                    correlation_id,
                    entity_id: proposal.entity_id,
                },
            )?;
            let proposal = ProposalEnvelope::new(
                &proposal_type,
                logbook_id,
                proposal.entity_id,
                None,
                device_id,
                IOS_PLUGIN_ID,
                1,
                proposal_payload,
            );
            let outcome = submit_proposal(
                &event_store,
                &InMemoryEventBus::default(),
                &ios_proposal_context(),
                proposal,
            )
            .await
            .map_err(|error| {
                mark_ios_user_action_required(
                    &request.app_support_dir,
                    &queued,
                    error.to_string(),
                    "domain_validation_failed",
                );
                BridgeFault::domain(error.to_string())
            })?;
            let offline_mutation = record_ios_official_event(
                &request.app_support_dir,
                &queued,
                &outcome.official_event,
            )?;
            corrective_event_hashes.push(outcome.official_event.event_hash.clone());
            corrective_events.push(outcome.official_event);
            offline_mutations.push(offline_mutation);
        }

        let mut resolution =
            ManualConflictResolution::new(ManualConflictResolutionChoice::CreateCorrectiveEvents)
                .with_corrective_event_hashes(corrective_event_hashes.clone())
                .with_resolved_by_device_id(device_id);
        if let Some(note) = request.operator_note.filter(|note| !note.trim().is_empty()) {
            resolution = resolution.with_note(note);
        }
        let review = review_store
            .resolve_review(request.review_id, resolution, Utc::now())
            .map_err(|error| BridgeFault::storage(error.to_string()))?;
        let snapshot = review_store
            .load_snapshot()
            .map_err(|error| BridgeFault::storage(error.to_string()))?;
        let pending_events = event_store
            .list_events(logbook_id)
            .await
            .map_err(|error| BridgeFault::storage(error.to_string()))?
            .len();
        Ok(json!({
            "conflict_review": review,
            "conflict_reviews": snapshot,
            "corrective_events": corrective_events,
            "corrective_event_hashes": corrective_event_hashes,
            "offline_mutations": offline_mutations,
            "projection": {
                "source": "rust",
                "schema_version": BRIDGE_SCHEMA_VERSION,
                "pending_event_count": pending_events
            }
        }))
    })
}

fn sync_snapshot_for_support(
    app_support_dir: Option<&str>,
    logbook_id: Uuid,
) -> Result<Value, BridgeFault> {
    let queue_snapshot = match app_support_dir {
        Some(dir) => {
            let queue = offline_queue(dir)?;
            Some(
                queue
                    .load_snapshot(Utc::now())
                    .map_err(|error| BridgeFault::storage(error.to_string()))?,
            )
        }
        None => None,
    };
    let conflict_reviews = match app_support_dir {
        Some(dir) => {
            let store = conflict_review_store(dir)?;
            Some(
                store
                    .load_snapshot()
                    .map_err(|error| BridgeFault::storage(error.to_string()))?,
            )
        }
        None => None,
    };
    let pending_events = match app_support_dir {
        Some(dir) => {
            let runtime =
                Runtime::new().map_err(|error| BridgeFault::internal(error.to_string()))?;
            let store = event_store(dir)?;
            runtime
                .block_on(store.list_events(logbook_id))
                .map_err(|error| BridgeFault::storage(error.to_string()))?
                .len()
        }
        None => 0,
    };
    let pending_changes = queue_snapshot
        .as_ref()
        .map(|snapshot| {
            snapshot.health.pending
                + snapshot.health.retrying
                + snapshot.health.sending
                + snapshot.health.blocked
                + snapshot.health.failed
                + snapshot.health.user_action_required
        })
        .unwrap_or(0);
    Ok(json!({
        "config": SyncConfig::default(),
        "identity": LocalPeerIdentity::new("KE8YGW Logger iOS", None),
        "cloud_config": CloudSyncConfig::default(),
        "cloud_connection_state": CloudConnectionState::Disconnected,
        "sync_protocol_version": ham_sync::PROTOCOL_VERSION,
        "pending_changes": pending_changes,
        "pending_events": pending_events,
        "offline_queue": queue_snapshot,
        "conflict_reviews": conflict_reviews,
        "conflicts": Vec::<Value>::new(),
        "history": Vec::<Value>::new(),
        "retry_policy": {
            "network_required": true,
            "background_retry_supported": true,
            "permanent_user_action_states": ["blocked", "failed", "user_action_required"]
        }
    }))
}

fn diagnostics_command_payload(payload: Value) -> Result<Value, BridgeFault> {
    let app_support_dir = payload
        .get("app_support_dir")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty());
    diagnostics_payload(app_support_dir)
}

fn diagnostics_payload(app_support_dir: Option<&str>) -> Result<Value, BridgeFault> {
    let registry = default_service_registry().snapshot();
    let station = match app_support_dir {
        Some(dir) => {
            let store = station_store(dir)?;
            let mut book = store
                .load()
                .map_err(|error| BridgeFault::storage(error.to_string()))?;
            ensure_station_book_seeded(&store, &mut book)?;
            book
        }
        None => default_station_book(),
    };
    let pending_events = match app_support_dir {
        Some(dir) => {
            let runtime =
                Runtime::new().map_err(|error| BridgeFault::internal(error.to_string()))?;
            let store = event_store(dir)?;
            runtime
                .block_on(store.list_events(default_logbook_id()))
                .map(|events| events.len())
                .unwrap_or(0)
        }
        None => 0,
    };

    Ok(json!({
        "rust_version": CORE_VERSION,
        "core_version": CORE_VERSION,
        "bridge_loaded": true,
        "abi_version": ABI_VERSION,
        "bridge_schema_version": BRIDGE_SCHEMA_VERSION,
        "sync_protocol_version": ham_sync::PROTOCOL_VERSION,
        "backup_schema_version": BACKUP_SCHEMA_VERSION,
        "build_target": build_target(),
        "database_status": {
            "official_event_store": app_support_dir.map(|_| "opened").unwrap_or("not_opened_by_snapshot"),
            "projection_cache": "swiftdata_projection_cache"
        },
        "provider_health": provider_status_summary(&registry),
        "sync_queue": {
            "pending_uploads": 0,
            "pending_sync_events": pending_events,
            "conflicts": 0
        },
        "station": {
            "profiles": station.profiles.len(),
            "equipment": station.equipment.len(),
            "configurations": station.configurations.len()
        },
        "swiftdata_projection": {
            "authority": "cache_only",
            "schema_version": BRIDGE_SCHEMA_VERSION
        },
        "memory_usage": {
            "source": "ios-process-info",
            "value": "provided_by_swift"
        },
        "storage": {
            "source": "ios-filemanager",
            "value": "provided_by_swift"
        },
        "crash_information": {
            "source": "ios-diagnostics",
            "value": "provided_by_swift"
        },
        "logs": {
            "runtime_jsonl": "ham-core runtime log format supported"
        },
        "report_id": Uuid::new_v4()
    }))
}

fn lookup_callsign_payload(callsign: &str) -> Result<Value, BridgeFault> {
    let runtime = Runtime::new().map_err(|error| BridgeFault::internal(error.to_string()))?;
    let provider = LocalPrefixProvider;

    match runtime.block_on(provider.lookup_callsign(callsign)) {
        Ok(result) => Ok(json!({
            "callsign": callsign.trim().to_ascii_uppercase(),
            "provider_id": provider.provider_id(),
            "result": result,
            "source": "ham-core-local-prefix"
        })),
        Err(error) => Err(BridgeFault::domain(error.to_string())),
    }
}

fn export_adif_payload(input: &str) -> Result<Value, BridgeFault> {
    let records = serde_json::from_str::<Vec<Value>>(input)
        .map_err(|error| BridgeFault::invalid_json(format!("invalid QSO JSON: {error}")))?;
    let mut projection = QsoCurrentStateProjection::new();
    for payload in records {
        let qso_id = payload
            .get("qso_id")
            .or_else(|| payload.get("id"))
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap_or_else(Uuid::new_v4);
        projection.upsert_record(QsoRecord {
            qso_id,
            payload,
            note_history: Vec::new(),
            deleted: false,
            last_event_hash: "ios-export-bridge".to_owned(),
        });
    }
    Ok(json!({
        "adif": export_adif(&projection, false),
        "backup_schema_version": BACKUP_SCHEMA_VERSION
    }))
}

fn create_qso_command(payload: Value, correlation_id: Uuid) -> Result<Value, BridgeFault> {
    let request: QsoMutationRequest = serde_json::from_value(payload).map_err(|error| {
        BridgeFault::invalid_json(format!("invalid qso.create payload: {error}"))
    })?;
    let runtime = Runtime::new().map_err(|error| BridgeFault::internal(error.to_string()))?;
    runtime.block_on(async move {
        let logbook_id = request.logbook_id.unwrap_or_else(default_logbook_id);
        let device_id = request.device_id.unwrap_or_else(Uuid::new_v4);
        let operation_id = request
            .operation_id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| correlation_id.to_string());
        let store = event_store(&request.app_support_dir)?;
        let station_store = station_store(&request.app_support_dir)?;
        let mut station_book = station_store
            .load()
            .map_err(|error| BridgeFault::storage(error.to_string()))?;
        ensure_station_book_seeded(&station_store, &mut station_book)?;

        if let Some(existing) =
            find_qso_event_by_operation(&store, logbook_id, &operation_id).await?
        {
            let projection = store
                .rebuild_projections(logbook_id)
                .await
                .map_err(|error| BridgeFault::storage(error.to_string()))?;
            return qso_mutation_result(&store, logbook_id, &projection, &existing, true).await;
        }

        let mut qso_payload = normalize_qso_payload(request.qso, &operation_id)?;
        station_book.apply_defaults_to_qso_payload(&mut qso_payload);
        let queued = enqueue_ios_mutation(
            &request.app_support_dir,
            logbook_id,
            device_id,
            OFFLINE_OP_QSO_CREATE,
            qso_payload.clone(),
            &operation_id,
            correlation_id,
        )?;

        let proposal = ProposalEnvelope::new(
            PROPOSAL_QSO_CREATE,
            logbook_id,
            None,
            None,
            device_id,
            IOS_PLUGIN_ID,
            1,
            qso_payload,
        );
        let outcome = submit_proposal(
            &store,
            &InMemoryEventBus::default(),
            &ios_proposal_context(),
            proposal,
        )
        .await
        .map_err(|error| {
            mark_ios_user_action_required(
                &request.app_support_dir,
                &queued,
                error.to_string(),
                "domain_validation_failed",
            );
            BridgeFault::domain(error.to_string())
        })?;
        let offline_mutation =
            record_ios_official_event(&request.app_support_dir, &queued, &outcome.official_event)?;
        let projection = store
            .rebuild_projections(logbook_id)
            .await
            .map_err(|error| BridgeFault::storage(error.to_string()))?;
        let mut result = qso_mutation_result(
            &store,
            logbook_id,
            &projection,
            &outcome.official_event,
            false,
        )
        .await?;
        result["offline_mutation"] = json!(offline_mutation);
        Ok(result)
    })
}

fn delete_qso_command(payload: Value, correlation_id: Uuid) -> Result<Value, BridgeFault> {
    let request: QsoDeleteRequest = serde_json::from_value(payload).map_err(|error| {
        BridgeFault::invalid_json(format!("invalid qso.delete payload: {error}"))
    })?;
    let runtime = Runtime::new().map_err(|error| BridgeFault::internal(error.to_string()))?;
    runtime.block_on(async move {
        let logbook_id = request.logbook_id.unwrap_or_else(default_logbook_id);
        let device_id = request.device_id.unwrap_or_else(Uuid::new_v4);
        let operation_id = request
            .operation_id
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| correlation_id.to_string());
        let store = event_store(&request.app_support_dir)?;
        let payload = json!({
            "qso_id": request.qso_id,
            "reason": "ios_delete",
            "client_operation_id": operation_id
        });
        let queued = enqueue_ios_mutation(
            &request.app_support_dir,
            logbook_id,
            device_id,
            OFFLINE_OP_QSO_DELETE,
            payload.clone(),
            &operation_id,
            correlation_id,
        )?;
        let proposal = ProposalEnvelope::new(
            PROPOSAL_QSO_DELETE,
            logbook_id,
            Some(request.qso_id),
            None,
            device_id,
            IOS_PLUGIN_ID,
            1,
            payload,
        );
        let outcome = submit_proposal(
            &store,
            &InMemoryEventBus::default(),
            &ios_proposal_context(),
            proposal,
        )
        .await
        .map_err(|error| {
            mark_ios_user_action_required(
                &request.app_support_dir,
                &queued,
                error.to_string(),
                "domain_validation_failed",
            );
            BridgeFault::domain(error.to_string())
        })?;
        let offline_mutation =
            record_ios_official_event(&request.app_support_dir, &queued, &outcome.official_event)?;
        let projection = store
            .rebuild_projections(logbook_id)
            .await
            .map_err(|error| BridgeFault::storage(error.to_string()))?;
        let mut result = qso_mutation_result(
            &store,
            logbook_id,
            &projection,
            &outcome.official_event,
            false,
        )
        .await?;
        result["offline_mutation"] = json!(offline_mutation);
        Ok(result)
    })
}

fn list_qsos_command(payload: Value) -> Result<Value, BridgeFault> {
    let request: QsoListRequest = serde_json::from_value(payload)
        .map_err(|error| BridgeFault::invalid_json(format!("invalid qso.list payload: {error}")))?;
    let runtime = Runtime::new().map_err(|error| BridgeFault::internal(error.to_string()))?;
    runtime.block_on(async move {
        let logbook_id = request.logbook_id.unwrap_or_else(default_logbook_id);
        let store = event_store(&request.app_support_dir)?;
        let projection = store
            .rebuild_projections(logbook_id)
            .await
            .map_err(|error| BridgeFault::storage(error.to_string()))?;
        let records = projection
            .list(request.include_deleted)
            .into_iter()
            .map(qso_record_json)
            .collect::<Vec<_>>();
        let pending_events = store
            .list_events(logbook_id)
            .await
            .map_err(|error| BridgeFault::storage(error.to_string()))?
            .len();
        Ok(json!({
            "records": records,
            "projection": {
                "source": "rust",
                "schema_version": BRIDGE_SCHEMA_VERSION,
                "pending_event_count": pending_events
            }
        }))
    })
}

fn station_profile_create_command(
    payload: Value,
    correlation_id: Uuid,
) -> Result<Value, BridgeFault> {
    let request: StationProfileCreateRequest =
        serde_json::from_value(payload).map_err(|error| {
            BridgeFault::invalid_json(format!("invalid station.profile.create payload: {error}"))
        })?;
    let store = station_store(&request.app_support_dir)?;
    let mut book = store
        .load()
        .map_err(|error| BridgeFault::storage(error.to_string()))?;
    ensure_station_book_seeded(&store, &mut book)?;

    let profile_id = request.station_profile_id.unwrap_or_else(Uuid::new_v4);
    if let Some(existing) = book
        .profiles
        .iter()
        .find(|profile| profile.station_profile_id == profile_id)
        .cloned()
    {
        return Ok(json!({
            "profile": existing,
            "station_book": book,
            "idempotent": true,
            "projection_source": "rust"
        }));
    }

    let logbook_id = request.logbook_id.unwrap_or_else(default_logbook_id);
    let device_id = request.device_id.unwrap_or_else(Uuid::new_v4);
    let operation_id = request
        .operation_id
        .clone()
        .unwrap_or_else(|| profile_id.to_string());
    let queued = enqueue_ios_mutation(
        &request.app_support_dir,
        logbook_id,
        device_id,
        OFFLINE_OP_STATION_PROFILE_CREATE,
        json!({
            "station_profile_id": profile_id,
            "display_name": request.display_name.clone(),
            "station_callsign": request.station_callsign.clone(),
            "operator_callsign": request.operator_callsign.clone(),
            "profile_type": request.profile_type.clone(),
            "default_grid": request.default_grid.clone(),
            "default_qth": request.default_qth.clone(),
            "default_power_watts": request.default_power_watts,
            "notes": request.notes.clone(),
            "active": request.active
        }),
        &operation_id,
        correlation_id,
    )?;
    let mut profile = StationProfile::new(request.display_name, request.station_callsign);
    profile.station_profile_id = profile_id;
    profile.operator_callsign = request.operator_callsign;
    profile.default_grid = request.default_grid;
    profile.default_qth = request.default_qth;
    profile.default_power_watts = request.default_power_watts;
    profile.notes = request.notes;
    profile.tags = request.profile_type.into_iter().collect();
    profile.active = request.active.unwrap_or(book.profiles.is_empty());
    let created = book.create_profile(profile);
    store.save(&book).map_err(|error| {
        mark_ios_user_action_required(
            &request.app_support_dir,
            &queued,
            error.to_string(),
            "station_support_save_failed",
        );
        BridgeFault::storage(error.to_string())
    })?;
    let offline_mutation = mark_ios_mutation_accepted(&request.app_support_dir, &queued)?;

    Ok(json!({
        "profile": created,
        "station_book": book,
        "offline_mutation": offline_mutation,
        "idempotent": false,
        "projection_source": "rust"
    }))
}

fn station_equipment_create_command(
    payload: Value,
    correlation_id: Uuid,
) -> Result<Value, BridgeFault> {
    let request: StationEquipmentCreateRequest =
        serde_json::from_value(payload).map_err(|error| {
            BridgeFault::invalid_json(format!("invalid station.equipment.create payload: {error}"))
        })?;
    let store = station_store(&request.app_support_dir)?;
    let mut book = store
        .load()
        .map_err(|error| BridgeFault::storage(error.to_string()))?;
    ensure_station_book_seeded(&store, &mut book)?;

    let equipment_id = request.equipment_id.unwrap_or_else(Uuid::new_v4);
    if let Some(existing) = book
        .equipment
        .iter()
        .find(|item| item.equipment_id == equipment_id)
        .cloned()
    {
        return Ok(json!({
            "equipment": existing,
            "station_book": book,
            "idempotent": true,
            "projection_source": "rust"
        }));
    }

    let logbook_id = request.logbook_id.unwrap_or_else(default_logbook_id);
    let device_id = request.device_id.unwrap_or_else(Uuid::new_v4);
    let operation_id = request
        .operation_id
        .clone()
        .unwrap_or_else(|| equipment_id.to_string());
    let queued = enqueue_ios_mutation(
        &request.app_support_dir,
        logbook_id,
        device_id,
        OFFLINE_OP_STATION_EQUIPMENT_CREATE,
        json!({
            "equipment_id": equipment_id,
            "equipment_type": request.equipment_type.clone(),
            "display_name": request.display_name.clone(),
            "manufacturer": request.manufacturer.clone(),
            "model": request.model.clone(),
            "serial_number": request.serial_number.clone(),
            "capabilities": request.capabilities.clone(),
            "notes": request.notes.clone()
        }),
        &operation_id,
        correlation_id,
    )?;
    let equipment_type = parse_equipment_type(&request.equipment_type)?;
    let mut item = EquipmentItem::new(equipment_type, request.display_name);
    item.equipment_id = equipment_id;
    item.manufacturer = request.manufacturer;
    item.model = request.model;
    item.serial_number = request.serial_number;
    item.capabilities = request.capabilities;
    item.notes = request.notes;
    let created = book.create_equipment(item);
    store.save(&book).map_err(|error| {
        mark_ios_user_action_required(
            &request.app_support_dir,
            &queued,
            error.to_string(),
            "station_support_save_failed",
        );
        BridgeFault::storage(error.to_string())
    })?;
    let offline_mutation = mark_ios_mutation_accepted(&request.app_support_dir, &queued)?;

    Ok(json!({
        "equipment": created,
        "station_book": book,
        "offline_mutation": offline_mutation,
        "idempotent": false,
        "projection_source": "rust"
    }))
}

fn station_profile_select_command(
    payload: Value,
    correlation_id: Uuid,
) -> Result<Value, BridgeFault> {
    let request: StationSelectProfileRequest =
        serde_json::from_value(payload).map_err(|error| {
            BridgeFault::invalid_json(format!("invalid station.profile.select payload: {error}"))
        })?;
    let store = station_store(&request.app_support_dir)?;
    let mut book = store
        .load()
        .map_err(|error| BridgeFault::storage(error.to_string()))?;
    ensure_station_book_seeded(&store, &mut book)?;
    let logbook_id = request.logbook_id.unwrap_or_else(default_logbook_id);
    let device_id = request.device_id.unwrap_or_else(Uuid::new_v4);
    let operation_id = request
        .operation_id
        .clone()
        .unwrap_or_else(|| request.station_profile_id.to_string());
    let queued = enqueue_ios_mutation(
        &request.app_support_dir,
        logbook_id,
        device_id,
        OFFLINE_OP_STATION_PROFILE_SELECT,
        json!({"station_profile_id": request.station_profile_id}),
        &operation_id,
        correlation_id,
    )?;
    book.select_profile(request.station_profile_id)
        .map_err(|error| {
            mark_ios_user_action_required(
                &request.app_support_dir,
                &queued,
                error.to_string(),
                "station_profile_invalid",
            );
            BridgeFault::domain(error.to_string())
        })?;
    store.save(&book).map_err(|error| {
        mark_ios_user_action_required(
            &request.app_support_dir,
            &queued,
            error.to_string(),
            "station_support_save_failed",
        );
        BridgeFault::storage(error.to_string())
    })?;
    let offline_mutation = mark_ios_mutation_accepted(&request.app_support_dir, &queued)?;

    Ok(json!({
        "station_book": book,
        "offline_mutation": offline_mutation,
        "projection_source": "rust"
    }))
}

fn domain_proposal_command(
    payload: Value,
    proposal_type: &str,
    entity_id_field: Option<&str>,
    correlation_id: Uuid,
) -> Result<Value, BridgeFault> {
    let runtime = Runtime::new().map_err(|error| BridgeFault::internal(error.to_string()))?;
    runtime.block_on(async move {
        let mut object = payload.as_object().cloned().ok_or_else(|| {
            BridgeFault::invalid_input("domain command payload must be an object")
        })?;
        let app_support_dir = object
            .remove("app_support_dir")
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| BridgeFault::invalid_input("app_support_dir is required"))?;
        let logbook_id = object
            .remove("logbook_id")
            .and_then(|value| value.as_str().and_then(|text| Uuid::parse_str(text).ok()))
            .unwrap_or_else(default_logbook_id);
        let device_id = object
            .remove("device_id")
            .and_then(|value| value.as_str().and_then(|text| Uuid::parse_str(text).ok()))
            .unwrap_or_else(Uuid::new_v4);
        let entity_id = entity_id_field
            .and_then(|field| object.get(field))
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok());
        object.insert("source".to_owned(), json!("ios/native"));
        let external_operation_id = object
            .get("operation_id")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| correlation_id.to_string());
        let proposal_payload = Value::Object(object);
        let queued = enqueue_ios_mutation(
            &app_support_dir,
            logbook_id,
            device_id,
            ios_offline_operation_type(proposal_type),
            proposal_payload.clone(),
            &external_operation_id,
            correlation_id,
        )?;

        let store = event_store(&app_support_dir)?;
        let proposal = ProposalEnvelope::new(
            proposal_type,
            logbook_id,
            entity_id,
            None,
            device_id,
            IOS_PLUGIN_ID,
            1,
            proposal_payload,
        );
        let outcome = submit_proposal(
            &store,
            &InMemoryEventBus::default(),
            &ios_proposal_context(),
            proposal,
        )
        .await
        .map_err(|error| {
            mark_ios_user_action_required(
                &app_support_dir,
                &queued,
                error.to_string(),
                "domain_validation_failed",
            );
            BridgeFault::domain(error.to_string())
        })?;
        let offline_mutation =
            record_ios_official_event(&app_support_dir, &queued, &outcome.official_event)?;
        let pending_events = store
            .list_events(logbook_id)
            .await
            .map_err(|error| BridgeFault::storage(error.to_string()))?
            .len();
        Ok(json!({
            "accepted": true,
            "official_event": outcome.official_event,
            "offline_mutation": offline_mutation,
            "projection": {
                "source": "rust",
                "schema_version": BRIDGE_SCHEMA_VERSION,
                "pending_event_count": pending_events
            }
        }))
    })
}

async fn find_qso_event_by_operation(
    store: &JsonlLogbookEventStore,
    logbook_id: Uuid,
    operation_id: &str,
) -> Result<Option<ham_core::CoreEventEnvelope>, BridgeFault> {
    let events = store
        .list_events(logbook_id)
        .await
        .map_err(|error| BridgeFault::storage(error.to_string()))?;
    Ok(events.into_iter().find(|event| {
        event.event_type == OFFICIAL_LOG_QSO_CREATED
            && event
                .payload
                .get("client_operation_id")
                .and_then(Value::as_str)
                == Some(operation_id)
    }))
}

async fn qso_mutation_result(
    store: &JsonlLogbookEventStore,
    logbook_id: Uuid,
    projection: &QsoCurrentStateProjection,
    event: &ham_core::CoreEventEnvelope,
    idempotent: bool,
) -> Result<Value, BridgeFault> {
    let qso = event
        .entity_id
        .and_then(|qso_id| projection.get_including_deleted(qso_id))
        .map(qso_record_json);
    let pending_events = store
        .list_events(logbook_id)
        .await
        .map_err(|error| BridgeFault::storage(error.to_string()))?
        .len();
    Ok(json!({
        "accepted": true,
        "idempotent": idempotent,
        "official_event": event,
        "qso": qso,
        "projection": {
            "source": "rust",
            "schema_version": BRIDGE_SCHEMA_VERSION,
            "last_rust_revision": event.event_hash,
            "pending_event_count": pending_events
        },
        "sync": {
            "pending_event_count": pending_events,
            "authority": "ham-sync"
        }
    }))
}

fn normalize_qso_payload(mut payload: Value, operation_id: &str) -> Result<Value, BridgeFault> {
    if !payload.is_object() {
        return Err(BridgeFault::invalid_input(
            "qso payload must be a JSON object",
        ));
    }
    let contacted_callsign = payload
        .get("contacted_callsign")
        .or_else(|| payload.get("callsign"))
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BridgeFault::invalid_input("qso requires contacted_callsign"))?;
    let mode = payload
        .get("mode")
        .and_then(Value::as_str)
        .map(|value| value.trim().to_ascii_uppercase())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| BridgeFault::invalid_input("qso requires mode"))?;
    let started_at = payload
        .get("started_at")
        .and_then(Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    payload["contacted_callsign"] = json!(contacted_callsign);
    payload["mode"] = json!(mode);
    payload["started_at"] = json!(started_at);
    payload["source"] = payload
        .get("source")
        .cloned()
        .unwrap_or_else(|| json!("ios/native"));
    payload["client_operation_id"] = json!(operation_id);

    if payload
        .get("station_callsign")
        .and_then(Value::as_str)
        .is_none()
    {
        payload["station_callsign"] = json!("KE8YGW");
    }
    if payload
        .get("operator_callsign")
        .and_then(Value::as_str)
        .is_none()
    {
        payload["operator_callsign"] = payload["station_callsign"].clone();
    }
    if let Some(frequency_mhz) = payload.get("frequency_mhz").and_then(Value::as_f64) {
        if payload.get("frequency_hz").is_none() && frequency_mhz > 0.0 {
            payload["frequency_hz"] = json!((frequency_mhz * 1_000_000.0).round() as u64);
        }
    }
    Ok(payload)
}

fn qso_record_json(record: &QsoRecord) -> Value {
    json!({
        "qso_id": record.qso_id,
        "payload": record.payload,
        "note_history": record.note_history,
        "deleted": record.deleted,
        "last_event_hash": record.last_event_hash,
        "projection_source": "rust",
        "schema_version": BRIDGE_SCHEMA_VERSION
    })
}

fn station_store(app_support_dir: &str) -> Result<JsonStationBookStore, BridgeFault> {
    Ok(JsonStationBookStore::new(
        rust_support_dir(app_support_dir)?.join("station-book.json"),
    ))
}

fn settings_store(
    app_support_dir: &str,
) -> Result<JsonSupportStore<ApplicationSettings>, BridgeFault> {
    Ok(JsonSupportStore::new(
        rust_support_dir(app_support_dir)?.join("application-settings.json"),
    ))
}

fn event_store(app_support_dir: &str) -> Result<JsonlLogbookEventStore, BridgeFault> {
    JsonlLogbookEventStore::open(rust_support_dir(app_support_dir)?.join("official-events.jsonl"))
        .map_err(|error| BridgeFault::storage(error.to_string()))
}

fn offline_queue(app_support_dir: &str) -> Result<JsonOfflineMutationQueue, BridgeFault> {
    Ok(JsonOfflineMutationQueue::new(
        rust_support_dir(app_support_dir)?.join("offline-mutations.json"),
    ))
}

fn conflict_review_store(app_support_dir: &str) -> Result<JsonConflictReviewStore, BridgeFault> {
    Ok(JsonConflictReviewStore::new(
        rust_support_dir(app_support_dir)?.join("conflict-reviews.json"),
    ))
}

struct IosMutationQueueInput<'a> {
    logbook_id: Uuid,
    device_id: Uuid,
    operation_type: &'a str,
    payload: Value,
    external_operation_id: &'a str,
    correlation_id: Uuid,
    entity_id: Option<Uuid>,
}

fn enqueue_ios_mutation(
    app_support_dir: &str,
    logbook_id: Uuid,
    device_id: Uuid,
    operation_type: &str,
    payload: Value,
    external_operation_id: &str,
    correlation_id: Uuid,
) -> Result<OfflineMutationEnvelope, BridgeFault> {
    enqueue_ios_mutation_with_input(
        app_support_dir,
        IosMutationQueueInput {
            logbook_id,
            device_id,
            operation_type,
            payload,
            external_operation_id,
            correlation_id,
            entity_id: None,
        },
    )
}

fn enqueue_ios_mutation_with_input(
    app_support_dir: &str,
    input: IosMutationQueueInput<'_>,
) -> Result<OfflineMutationEnvelope, BridgeFault> {
    let queue = offline_queue(app_support_dir)?;
    let operation_id = Uuid::parse_str(input.external_operation_id).unwrap_or(input.correlation_id);
    let entity_id = input
        .entity_id
        .or_else(|| payload_entity_id(input.operation_type, &input.payload));
    queue
        .enqueue_input(
            OfflineMutationInput::new(
                input.logbook_id,
                input.device_id,
                input.device_id,
                input.operation_type,
                input.payload,
            )
            .with_operation_id(operation_id)
            .with_correlation_id(input.correlation_id)
            .with_entity_id(entity_id)
            .with_idempotency_key(format!(
                "{}:{}",
                input.operation_type, input.external_operation_id
            )),
            Utc::now(),
        )
        .map_err(|error| BridgeFault::storage(error.to_string()))
}

fn payload_entity_id(operation_type: &str, payload: &Value) -> Option<Uuid> {
    let fields: &[&str] = match operation_type {
        OFFLINE_OP_QSO_CREATE
        | OFFLINE_OP_QSO_CORRECT
        | OFFLINE_OP_QSO_DELETE
        | OFFLINE_OP_QSO_RESTORE
        | OFFLINE_OP_QSO_NOTE_ADD => &["entity_id", "qso_id"],
        OFFLINE_OP_ACTIVATION_START | OFFLINE_OP_ACTIVATION_END => &["entity_id", "activation_id"],
        OFFLINE_OP_NET_SESSION_START | OFFLINE_OP_NET_SESSION_END => {
            &["entity_id", "net_session_id"]
        }
        OFFLINE_OP_NET_CHECKIN_CREATE | OFFLINE_OP_NET_CHECKIN_DELETE => {
            &["entity_id", "checkin_id"]
        }
        OFFLINE_OP_NET_TRAFFIC_CREATE => &["entity_id", "traffic_id"],
        OFFLINE_OP_STATION_PROFILE_CREATE | OFFLINE_OP_STATION_PROFILE_SELECT => {
            &["entity_id", "station_profile_id"]
        }
        OFFLINE_OP_STATION_EQUIPMENT_CREATE => &["entity_id", "equipment_id"],
        _ => &["entity_id"],
    };
    fields.iter().find_map(|field| {
        payload
            .get(*field)
            .and_then(Value::as_str)
            .and_then(|value| Uuid::parse_str(value).ok())
    })
}

fn record_ios_official_event(
    app_support_dir: &str,
    queued: &OfflineMutationEnvelope,
    event: &ham_core::CoreEventEnvelope,
) -> Result<OfflineMutationEnvelope, BridgeFault> {
    offline_queue(app_support_dir)?
        .record_local_event(queued.operation_id, event, Utc::now())
        .map_err(|error| BridgeFault::storage(error.to_string()))
}

fn mark_ios_user_action_required(
    app_support_dir: &str,
    queued: &OfflineMutationEnvelope,
    reason: impl Into<String>,
    error_code: impl Into<String>,
) {
    if let Ok(queue) = offline_queue(app_support_dir) {
        let _ = queue.mark_user_action_required(
            queued.operation_id,
            reason.into(),
            Some(error_code.into()),
            Utc::now(),
        );
    }
}

fn mark_ios_mutation_accepted(
    app_support_dir: &str,
    queued: &OfflineMutationEnvelope,
) -> Result<OfflineMutationEnvelope, BridgeFault> {
    offline_queue(app_support_dir)?
        .mark_accepted(queued.operation_id, Utc::now())
        .map_err(|error| BridgeFault::storage(error.to_string()))
}

fn ios_offline_operation_type(proposal_type: &str) -> &'static str {
    match proposal_type {
        PROPOSAL_ACTIVATION_START => OFFLINE_OP_ACTIVATION_START,
        PROPOSAL_ACTIVATION_END => OFFLINE_OP_ACTIVATION_END,
        PROPOSAL_NET_SESSION_START => OFFLINE_OP_NET_SESSION_START,
        PROPOSAL_NET_SESSION_END => OFFLINE_OP_NET_SESSION_END,
        PROPOSAL_NET_CHECKIN_CREATE => OFFLINE_OP_NET_CHECKIN_CREATE,
        PROPOSAL_NET_TRAFFIC_CREATE => OFFLINE_OP_NET_TRAFFIC_CREATE,
        _ => "ios.domain.mutation",
    }
}

fn ios_corrective_offline_operation_type(proposal_type: &str) -> String {
    match proposal_type {
        PROPOSAL_QSO_CREATE => OFFLINE_OP_QSO_CREATE,
        PROPOSAL_QSO_CORRECT => OFFLINE_OP_QSO_CORRECT,
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

fn is_supported_ios_corrective_proposal(proposal_type: &str) -> bool {
    matches!(
        proposal_type,
        PROPOSAL_QSO_CREATE
            | PROPOSAL_QSO_CORRECT
            | PROPOSAL_QSO_DELETE
            | PROPOSAL_QSO_RESTORE
            | PROPOSAL_QSO_NOTE_ADD
            | PROPOSAL_ACTIVATION_CREATE
            | PROPOSAL_ACTIVATION_UPDATE
            | PROPOSAL_ACTIVATION_START
            | PROPOSAL_ACTIVATION_END
            | PROPOSAL_ACTIVATION_CANCEL
            | PROPOSAL_ACTIVATION_NOTE_ADD
            | PROPOSAL_QSO_ACTIVATION_LINK
            | PROPOSAL_QSO_ACTIVATION_UNLINK
            | PROPOSAL_NET_TEMPLATE_CREATE
            | PROPOSAL_NET_TEMPLATE_UPDATE
            | PROPOSAL_NET_SESSION_START
            | PROPOSAL_NET_SESSION_END
            | PROPOSAL_NET_SESSION_CANCEL
            | PROPOSAL_NET_CHECKIN_CREATE
            | PROPOSAL_NET_CHECKIN_UPDATE
            | PROPOSAL_NET_CHECKIN_DELETE
            | PROPOSAL_NET_TRAFFIC_CREATE
            | PROPOSAL_NET_TRAFFIC_UPDATE
            | PROPOSAL_NET_REPORT_EXPORT
    )
}

fn corrective_proposal_requires_entity_id(proposal_type: &str) -> bool {
    !matches!(
        proposal_type,
        PROPOSAL_QSO_CREATE
            | PROPOSAL_ACTIVATION_CREATE
            | PROPOSAL_ACTIVATION_START
            | PROPOSAL_NET_TEMPLATE_CREATE
            | PROPOSAL_NET_SESSION_START
            | PROPOSAL_NET_CHECKIN_CREATE
            | PROPOSAL_NET_TRAFFIC_CREATE
    )
}

fn corrective_payload_with_entity_id(
    proposal_type: &str,
    payload: Value,
    entity_id: Option<Uuid>,
) -> Result<Value, BridgeFault> {
    let Some(mut object) = payload.as_object().cloned() else {
        return Err(BridgeFault::invalid_input(
            "corrective proposal payload must be a JSON object",
        ));
    };
    if let Some(entity_id) = entity_id {
        let entity_key = proposal_entity_key(proposal_type);
        object
            .entry(entity_key.to_owned())
            .or_insert_with(|| json!(entity_id));
    }
    Ok(Value::Object(object))
}

fn proposal_entity_key(proposal_type: &str) -> &'static str {
    match proposal_type {
        PROPOSAL_NET_TEMPLATE_CREATE | PROPOSAL_NET_TEMPLATE_UPDATE => "net_template_id",
        PROPOSAL_ACTIVATION_CREATE
        | PROPOSAL_ACTIVATION_UPDATE
        | PROPOSAL_ACTIVATION_START
        | PROPOSAL_ACTIVATION_END
        | PROPOSAL_ACTIVATION_CANCEL
        | PROPOSAL_ACTIVATION_NOTE_ADD => "activation_id",
        PROPOSAL_NET_SESSION_START | PROPOSAL_NET_SESSION_END | PROPOSAL_NET_SESSION_CANCEL => {
            "net_session_id"
        }
        PROPOSAL_NET_CHECKIN_CREATE | PROPOSAL_NET_CHECKIN_UPDATE | PROPOSAL_NET_CHECKIN_DELETE => {
            "checkin_id"
        }
        PROPOSAL_NET_TRAFFIC_CREATE | PROPOSAL_NET_TRAFFIC_UPDATE => "traffic_id",
        _ => "qso_id",
    }
}

fn rust_support_dir(app_support_dir: &str) -> Result<PathBuf, BridgeFault> {
    let trimmed = app_support_dir.trim();
    if trimmed.is_empty() {
        return Err(BridgeFault::invalid_input("app_support_dir is required"));
    }
    Ok(Path::new(trimmed).join("Rust"))
}

fn ensure_station_book_seeded(
    store: &JsonStationBookStore,
    book: &mut StationBook,
) -> Result<(), BridgeFault> {
    if !book.profiles.is_empty() {
        return Ok(());
    }
    *book = default_station_book();
    store
        .save(book)
        .map_err(|error| BridgeFault::storage(error.to_string()))
}

fn parse_equipment_type(value: &str) -> Result<EquipmentType, BridgeFault> {
    match value.trim().to_ascii_lowercase().as_str() {
        "radio" => Ok(EquipmentType::Radio),
        "antenna" => Ok(EquipmentType::Antenna),
        "amplifier" => Ok(EquipmentType::Amplifier),
        "tuner" => Ok(EquipmentType::Tuner),
        "rotor" => Ok(EquipmentType::Rotor),
        "interface" => Ok(EquipmentType::Interface),
        "power_supply" | "power-supply" | "power supply" => Ok(EquipmentType::PowerSupply),
        "accessory" => Ok(EquipmentType::Accessory),
        other => Err(BridgeFault::invalid_input(format!(
            "unsupported equipment_type {other}"
        ))),
    }
}

fn ios_proposal_context() -> ProposalContext {
    ProposalContext::local_admin(
        PluginManifest::new(
            IOS_PLUGIN_ID,
            "KE8YGW Logger iOS",
            CORE_VERSION,
            vec![
                PluginCapability::QsoCreate,
                PluginCapability::QsoCorrect,
                PluginCapability::QsoDelete,
                PluginCapability::QsoRestore,
                PluginCapability::QsoNoteAdd,
                PluginCapability::ActivationCreate,
                PluginCapability::ActivationUpdate,
                PluginCapability::ActivationEnd,
                PluginCapability::ActivationCancel,
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
        ),
        OperatorRole::Admin,
    )
}

fn default_logbook_id() -> Uuid {
    Uuid::parse_str(DEFAULT_LOGBOOK_ID).expect("default logbook id is valid")
}

fn default_station_book() -> StationBook {
    let mut book = StationBook::default();
    let mut home = StationProfile::new("Home Station", "KE8YGW");
    home.operator_callsign = Some("KE8YGW".to_owned());
    home.default_grid = Some("EN91".to_owned());
    home.default_qth = Some("Cleveland, OH".to_owned());
    home.default_power_watts = Some(100);
    home.tags = vec!["home".to_owned()];
    home.active = true;
    let home = book.create_profile(home);

    let mut portable = StationProfile::new("Portable Station", "KE8YGW/P");
    portable.operator_callsign = Some("KE8YGW".to_owned());
    portable.default_power_watts = Some(10);
    portable.tags = vec!["portable".to_owned(), "pota".to_owned(), "sota".to_owned()];
    book.create_profile(portable);

    let mut radio = EquipmentItem::new(EquipmentType::Radio, "Field HF Rig");
    radio.manufacturer = Some("Generic".to_owned());
    radio.model = Some("Portable 100".to_owned());
    radio.capabilities = vec![
        "hf".to_owned(),
        "voice".to_owned(),
        "cw".to_owned(),
        "digital".to_owned(),
    ];
    let radio = book.create_equipment(radio);

    let mut antenna = EquipmentItem::new(EquipmentType::Antenna, "Linked Dipole");
    antenna.capabilities = vec!["40m".to_owned(), "20m".to_owned(), "10m".to_owned()];
    let antenna = book.create_equipment(antenna);

    let mut amplifier = EquipmentItem::new(EquipmentType::Amplifier, "Barefoot");
    amplifier.notes = Some("No amplifier in default iOS profile".to_owned());
    let amplifier = book.create_equipment(amplifier);

    let mut config = StationConfiguration::new(home.station_profile_id, "HF Voice/Digital");
    config.radio_id = Some(radio.equipment_id);
    config.antenna_id = Some(antenna.equipment_id);
    config.amplifier_id = Some(amplifier.equipment_id);
    config.band_hint = Some("20m".to_owned());
    config.mode_hint = Some("SSB".to_owned());
    config.default_power_watts = Some(100);
    let config = book
        .create_configuration(config)
        .expect("default profile exists");
    let _ = book.select_configuration(config.configuration_id);
    book
}

fn default_upload_queue(providers: &[ham_core::ServiceProviderMetadata]) -> UploadQueue {
    let targets = providers
        .iter()
        .filter(|provider| {
            matches!(
                provider.provider_id.as_str(),
                "lotw" | "eqsl" | "club-log" | "qrz-logbook"
            )
        })
        .map(UploadTarget::from_provider)
        .collect::<Vec<_>>();
    UploadQueue::new(targets)
}

fn provider_status_summary(registry: &ham_core::ServiceRegistrySnapshot) -> Value {
    let providers = registry
        .providers
        .iter()
        .map(|provider| {
            json!({
                "provider_id": provider.metadata.provider_id,
                "display_name": provider.metadata.display_name,
                "service_type": provider.metadata.service_type,
                "enabled": provider.enabled,
                "health": provider.health,
                "requires_network_access": provider.metadata.requires_network_access,
                "supports_offline": provider.metadata.supports_offline,
                "required_credentials": provider.metadata.required_credentials
            })
        })
        .collect::<Vec<_>>();

    json!({
        "providers": providers,
        "preferred_providers": registry.preferred_providers
    })
}

fn input_bytes<'a>(ptr: *const c_uchar, len: usize) -> Result<&'a str, BridgeFault> {
    if ptr.is_null() {
        return Err(BridgeFault::invalid_input("null input pointer"));
    }
    if len > MAX_INPUT_BYTES {
        return Err(BridgeFault::invalid_input(format!(
            "input exceeds maximum size of {MAX_INPUT_BYTES} bytes"
        )));
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    str::from_utf8(bytes).map_err(|error| BridgeFault::invalid_input(error.to_string()))
}

fn input_c_string(ptr: *const c_char) -> Result<String, BridgeFault> {
    if ptr.is_null() {
        return Err(BridgeFault::invalid_input("null string pointer"));
    }
    let bytes = unsafe { CStr::from_ptr(ptr).to_bytes() };
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(BridgeFault::invalid_input(format!(
            "input exceeds maximum size of {MAX_INPUT_BYTES} bytes"
        )));
    }
    str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|error| BridgeFault::invalid_input(error.to_string()))
}

fn parse_call(input: &str) -> Result<BridgeCall, BridgeFault> {
    serde_json::from_str(input)
        .map_err(|error| BridgeFault::invalid_json(format!("invalid bridge request JSON: {error}")))
}

fn string_field(payload: &Value, field: &str) -> Result<String, BridgeFault> {
    payload
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_owned)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| BridgeFault::invalid_input(format!("field `{field}` is required")))
}

fn response_ok(correlation_id: Uuid, data: Value) -> *mut c_char {
    let envelope = BridgeEnvelope {
        ok: true,
        bridge_version: IOS_BRIDGE_VERSION,
        abi_version: ABI_VERSION,
        schema_version: BRIDGE_SCHEMA_VERSION,
        generated_at: Utc::now().to_rfc3339(),
        data: Some(data),
        error: None,
        correlation_id: correlation_id.to_string(),
    };
    string_ptr(serde_json::to_string(&envelope).expect("bridge envelope should serialize"))
}

fn response_err(correlation_id: Uuid, error: BridgeFault) -> *mut c_char {
    let envelope = BridgeEnvelope {
        ok: false,
        bridge_version: IOS_BRIDGE_VERSION,
        abi_version: ABI_VERSION,
        schema_version: BRIDGE_SCHEMA_VERSION,
        generated_at: Utc::now().to_rfc3339(),
        data: None,
        error: Some(BridgeErrorPayload {
            code: error.code.to_owned(),
            message: error.message,
            details: error.details,
        }),
        correlation_id: correlation_id.to_string(),
    };
    string_ptr(serde_json::to_string(&envelope).expect("bridge error should serialize"))
}

fn string_ptr(value: String) -> *mut c_char {
    match CString::new(value) {
        Ok(value) => value.into_raw(),
        Err(_) => CString::new(
            "{\"ok\":false,\"error\":{\"code\":\"internal_error\",\"message\":\"interior nul byte\",\"details\":{}},\"data\":null}",
        )
        .expect("static error has no nul")
        .into_raw(),
    }
}

fn build_target() -> Value {
    json!({
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "family": std::env::consts::FAMILY,
        "ios": cfg!(target_os = "ios"),
        "simulator": cfg!(target_env = "sim")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take_string(ptr: *mut c_char) -> String {
        let text = unsafe { CStr::from_ptr(ptr).to_string_lossy().into_owned() };
        unsafe {
            ham_ios_free_string(ptr);
        }
        text
    }

    fn call_json(value: Value) -> Value {
        let text = value.to_string();
        let ptr = ham_ios_call_json_bytes(text.as_ptr(), text.len());
        serde_json::from_str(&take_string(ptr)).unwrap()
    }

    #[test]
    fn version_payload_is_json() {
        let ptr = ham_ios_version_json();
        let value: Value = serde_json::from_str(&take_string(ptr)).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["bridge_version"], IOS_BRIDGE_VERSION);
        assert_eq!(value["data"]["bridge_version"], IOS_BRIDGE_VERSION);
        assert_eq!(value["data"]["abi_version"], ABI_VERSION);
    }

    #[test]
    fn null_input_returns_structured_error() {
        let ptr = ham_ios_call_json_bytes(std::ptr::null(), 0);
        let value: Value = serde_json::from_str(&take_string(ptr)).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "invalid_input");
    }

    #[test]
    fn invalid_utf8_returns_structured_error() {
        let bytes = [0xff_u8];
        let ptr = ham_ios_call_json_bytes(bytes.as_ptr(), bytes.len());
        let value: Value = serde_json::from_str(&take_string(ptr)).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "invalid_input");
    }

    #[test]
    fn oversized_input_returns_before_reading_buffer() {
        let bytes = b"{";
        let ptr = ham_ios_call_json_bytes(bytes.as_ptr(), MAX_INPUT_BYTES + 1);
        let value: Value = serde_json::from_str(&take_string(ptr)).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "invalid_input");
    }

    #[test]
    fn invalid_json_returns_structured_error() {
        let input = b"{not-json";
        let ptr = ham_ios_call_json_bytes(input.as_ptr(), input.len());
        let value: Value = serde_json::from_str(&take_string(ptr)).unwrap();
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "invalid_json");
    }

    #[test]
    fn panic_is_contained() {
        let value = call_json(json!({"command": "__test_panic"}));
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], "internal_error");
    }

    #[test]
    fn lookup_uses_core_prefix_provider() {
        let callsign = CString::new("ke8ygw").unwrap();
        let ptr = ham_ios_lookup_callsign_json(callsign.as_ptr());
        let value: Value = serde_json::from_str(&take_string(ptr)).unwrap();
        assert_eq!(value["ok"], true);
        assert_eq!(value["data"]["provider_id"], "local-prefix");
        assert_eq!(value["data"]["result"]["country"], "United States");
    }

    #[test]
    fn station_book_has_equipment_and_profiles() {
        let book = default_station_book();
        assert!(book.profiles.len() >= 2);
        assert!(book.equipment.len() >= 3);
        assert!(book.active_profile().is_some());
        assert!(book.active_configuration().is_some());
    }

    #[test]
    fn adif_export_uses_core_projection() {
        let payload = json!([{
            "qso_id": Uuid::new_v4().to_string(),
            "contacted_callsign": "K1ABC",
            "station_callsign": "KE8YGW",
            "operator_callsign": "KE8YGW",
            "started_at": "2026-07-10T12:00:00Z",
            "band": "20m",
            "mode": "SSB",
            "frequency_hz": 14250000,
            "rst_sent": "59",
            "rst_received": "57"
        }]);
        let c_json = CString::new(payload.to_string()).unwrap();
        let ptr = ham_ios_export_adif_json(c_json.as_ptr());
        let value: Value = serde_json::from_str(&take_string(ptr)).unwrap();
        let adif = value["data"]["adif"].as_str().unwrap();
        assert!(adif.contains("<CALL:5>K1ABC"));
        assert!(adif.contains("<MODE:3>SSB"));
    }

    #[test]
    fn qso_create_uses_proposal_pipeline_and_is_idempotent() {
        let app_support_dir = std::env::temp_dir().join(format!("ham-ios-{}", Uuid::new_v4()));
        let operation_id = Uuid::new_v4().to_string();
        let request = json!({
            "command": "qso.create",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "operation_id": operation_id,
                "qso": {
                    "contacted_callsign": "k1abc",
                    "station_callsign": "ke8ygw",
                    "operator_callsign": "ke8ygw",
                    "started_at": "2026-07-10T12:00:00Z",
                    "mode": "ssb",
                    "band": "20m"
                }
            }
        });
        let first = call_json(request.clone());
        let second = call_json(request);

        assert_eq!(first["ok"], true);
        assert_eq!(first["data"]["accepted"], true);
        assert_eq!(second["ok"], true);
        assert_eq!(second["data"]["idempotent"], true);
        assert_eq!(
            first["data"]["qso"]["qso_id"],
            second["data"]["qso"]["qso_id"]
        );
        assert_eq!(
            first["data"]["offline_mutation"]["entity_id"],
            first["data"]["official_event"]["entity_id"]
        );
        let snapshot = call_json(json!({
            "command": "sync.snapshot",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy()
            }
        }));
        assert_eq!(snapshot["ok"], true);
        assert_eq!(snapshot["data"]["offline_queue"]["health"]["pending"], 1);
        assert_eq!(snapshot["data"]["offline_queue"]["health"]["total"], 1);
        let _ = std::fs::remove_dir_all(app_support_dir);
    }

    #[test]
    fn station_profile_create_uses_rust_station_store() {
        let app_support_dir = std::env::temp_dir().join(format!("ham-ios-{}", Uuid::new_v4()));
        let profile_id = Uuid::new_v4();
        let request = json!({
            "command": "station.profile.create",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "station_profile_id": profile_id,
                "display_name": "Test Portable",
                "station_callsign": "K1ABC/P",
                "operator_callsign": "K1ABC",
                "profile_type": "portable",
                "default_power_watts": 10
            }
        });
        let first = call_json(request.clone());
        let second = call_json(request);

        assert_eq!(first["ok"], true);
        assert_eq!(first["data"]["profile"]["station_callsign"], "K1ABC/P");
        assert_eq!(first["data"]["offline_mutation"]["status"], "accepted");
        assert_eq!(second["data"]["idempotent"], true);
        let snapshot = call_json(json!({
            "command": "sync.snapshot",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy()
            }
        }));
        assert_eq!(snapshot["ok"], true);
        assert_eq!(snapshot["data"]["offline_queue"]["health"]["accepted"], 1);
        let _ = std::fs::remove_dir_all(app_support_dir);
    }

    #[test]
    fn conflict_review_create_and_resolve_use_rust_store() {
        let app_support_dir = std::env::temp_dir().join(format!("ham-ios-{}", Uuid::new_v4()));
        let logbook_id = Uuid::new_v4();
        let preview = ham_sync::PreviewPullResponse {
            peer_id: "ios-peer".to_owned(),
            logbook_id,
            status: ham_sync::ReplicationStatus::Diverged,
            local_head_hash: Some("local".to_owned()),
            remote_head_hash: Some("remote".to_owned()),
            missing_event_count: 0,
            remote_event_count: 2,
            events: Vec::new(),
            message: "Remote chain does not contain the local head".to_owned(),
        };
        let report = ham_sync::conflict_report_from_preview(&preview, &[], Utc::now());
        let created = call_json(json!({
            "command": "sync.conflict_reviews.create",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "report": report
            }
        }));
        assert_eq!(created["ok"], true);
        assert_eq!(created["data"]["conflict_reviews"]["health"]["open"], 1);
        let review_id = created["data"]["conflict_review"]["review_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let unsafe_pull = call_json(json!({
            "command": "sync.conflict_reviews.resolve",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "review_id": review_id,
                "resolution": {
                    "choice": "pull_remote_after_review",
                    "operator_note": "unsafe attempt",
                    "corrective_event_hashes": [],
                    "resolved_by_device_id": Uuid::new_v4()
                }
            }
        }));
        assert_eq!(unsafe_pull["ok"], false);
        assert_eq!(unsafe_pull["error"]["code"], "storage_error");

        let resolved = call_json(json!({
            "command": "sync.conflict_reviews.resolve",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "review_id": review_id,
                "resolution": {
                    "choice": "keep_local_history",
                    "operator_note": "Keep local append-only history",
                    "corrective_event_hashes": [],
                    "resolved_by_device_id": Uuid::new_v4()
                }
            }
        }));
        assert_eq!(resolved["ok"], true);
        assert_eq!(resolved["data"]["conflict_review"]["status"], "resolved");
        let snapshot = call_json(json!({
            "command": "sync.snapshot",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy()
            }
        }));
        assert_eq!(
            snapshot["data"]["conflict_reviews"]["health"]["resolved"],
            1
        );
        let _ = std::fs::remove_dir_all(app_support_dir);
    }

    #[test]
    fn conflict_review_corrective_events_use_proposal_pipeline() {
        let app_support_dir = std::env::temp_dir().join(format!("ham-ios-{}", Uuid::new_v4()));
        let logbook_id = default_logbook_id();
        let device_id = Uuid::new_v4();
        let qso = call_json(json!({
            "command": "qso.create",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "logbook_id": logbook_id,
                "device_id": device_id,
                "operation_id": Uuid::new_v4().to_string(),
                "qso": {
                    "contacted_callsign": "w1aw",
                    "station_callsign": "ke8ygw",
                    "operator_callsign": "ke8ygw",
                    "started_at": "2026-07-10T12:00:00Z",
                    "mode": "ssb",
                    "band": "20m"
                }
            }
        }));
        assert_eq!(qso["ok"], true);
        let qso_id = qso["data"]["qso"]["qso_id"].as_str().unwrap().to_owned();

        let preview = ham_sync::PreviewPullResponse {
            peer_id: "ios-peer".to_owned(),
            logbook_id,
            status: ham_sync::ReplicationStatus::Diverged,
            local_head_hash: Some("local".to_owned()),
            remote_head_hash: Some("remote".to_owned()),
            missing_event_count: 0,
            remote_event_count: 2,
            events: Vec::new(),
            message: "Remote chain does not contain the local head".to_owned(),
        };
        let report = ham_sync::conflict_report_from_preview(&preview, &[], Utc::now());
        let created = call_json(json!({
            "command": "sync.conflict_reviews.create",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "report": report
            }
        }));
        assert_eq!(created["ok"], true);
        let review_id = created["data"]["conflict_review"]["review_id"]
            .as_str()
            .unwrap()
            .to_owned();

        let resolved = call_json(json!({
            "command": "sync.conflict_reviews.corrective_events",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "logbook_id": logbook_id,
                "device_id": device_id,
                "review_id": review_id,
                "operator_note": "Resolved with corrective note.",
                "proposals": [{
                    "proposal_type": PROPOSAL_QSO_NOTE_ADD,
                    "entity_id": qso_id.clone(),
                    "operation_id": Uuid::new_v4().to_string(),
                    "payload": {
                        "note": "Remote branch reviewed; local QSO retained."
                    }
                }]
            }
        }));
        assert_eq!(resolved["ok"], true);
        assert_eq!(
            resolved["data"]["conflict_review"]["selected_resolution"]["choice"],
            "create_corrective_events"
        );
        assert_eq!(
            resolved["data"]["corrective_events"][0]["event_type"],
            "official.log.qso.note_added"
        );
        assert_eq!(
            resolved["data"]["corrective_event_hashes"][0],
            resolved["data"]["corrective_events"][0]["event_hash"]
        );
        assert_eq!(
            resolved["data"]["offline_mutations"][0]["entity_id"]
                .as_str()
                .unwrap(),
            qso_id
        );

        let qsos = call_json(json!({
            "command": "qso.list",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "logbook_id": logbook_id
            }
        }));
        assert_eq!(qsos["ok"], true);
        assert_eq!(
            qsos["data"]["records"][0]["note_history"][0]["note"],
            "Remote branch reviewed; local QSO retained."
        );
        let _ = std::fs::remove_dir_all(app_support_dir);
    }

    #[test]
    fn conflict_review_corrective_events_reject_empty_proposals() {
        let app_support_dir = std::env::temp_dir().join(format!("ham-ios-{}", Uuid::new_v4()));
        let logbook_id = default_logbook_id();
        let preview = ham_sync::PreviewPullResponse {
            peer_id: "ios-peer".to_owned(),
            logbook_id,
            status: ham_sync::ReplicationStatus::Diverged,
            local_head_hash: Some("local".to_owned()),
            remote_head_hash: Some("remote".to_owned()),
            missing_event_count: 0,
            remote_event_count: 2,
            events: Vec::new(),
            message: "Remote chain does not contain the local head".to_owned(),
        };
        let report = ham_sync::conflict_report_from_preview(&preview, &[], Utc::now());
        let created = call_json(json!({
            "command": "sync.conflict_reviews.create",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "report": report
            }
        }));
        let review_id = created["data"]["conflict_review"]["review_id"]
            .as_str()
            .unwrap()
            .to_owned();

        let rejected = call_json(json!({
            "command": "sync.conflict_reviews.corrective_events",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "review_id": review_id,
                "proposals": []
            }
        }));
        assert_eq!(rejected["ok"], false);
        assert_eq!(rejected["error"]["code"], "invalid_input");

        let snapshot = call_json(json!({
            "command": "sync.conflict_reviews.snapshot",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "logbook_id": logbook_id
            }
        }));
        assert_eq!(snapshot["data"]["conflict_reviews"]["health"]["open"], 1);
        let _ = std::fs::remove_dir_all(app_support_dir);
    }

    #[test]
    fn settings_get_reports_absent_without_creating_defaults() {
        let app_support_dir = std::env::temp_dir().join(format!("ham-ios-{}", Uuid::new_v4()));
        let response = call_json(json!({
            "command": "settings.get",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy()
            }
        }));

        assert_eq!(response["ok"], true);
        assert_eq!(response["data"]["exists"], false);
        assert_eq!(response["data"]["record_count"], 0);
        assert!(!app_support_dir
            .join("Rust/application-settings.json")
            .exists());
        let _ = std::fs::remove_dir_all(app_support_dir);
    }

    #[test]
    fn settings_default_creation_is_idempotent_and_reloads() {
        let app_support_dir = std::env::temp_dir().join(format!("ham-ios-{}", Uuid::new_v4()));
        let request = json!({
            "command": "settings.create_default",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy()
            }
        });
        let first = call_json(request.clone());
        let second = call_json(request);
        let loaded = call_json(json!({
            "command": "settings.get",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy()
            }
        }));

        assert_eq!(first["ok"], true);
        assert_eq!(first["data"]["created"], true);
        assert_eq!(second["ok"], true);
        assert_eq!(second["data"]["created"], false);
        assert_eq!(second["data"]["record_count"], 1);
        assert_eq!(loaded["data"]["exists"], true);
        assert_eq!(
            loaded["data"]["settings"]["operator"]["primary_callsign"],
            "KE8YGW"
        );
        let _ = std::fs::remove_dir_all(app_support_dir);
    }

    #[test]
    fn settings_update_persists_valid_changes_and_rejects_invalid_url() {
        let app_support_dir = std::env::temp_dir().join(format!("ham-ios-{}", Uuid::new_v4()));
        let created = call_json(json!({
            "command": "settings.create_default",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy()
            }
        }));
        let mut settings = created["data"]["settings"].clone();
        settings["operator"]["primary_callsign"] = json!("k1abc");
        settings["sync"]["sync_server_url"] = json!("https://sync.example.test");
        let updated = call_json(json!({
            "command": "settings.update",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "settings": settings
            }
        }));
        assert_eq!(updated["ok"], true);
        assert_eq!(
            updated["data"]["settings"]["operator"]["primary_callsign"],
            "K1ABC"
        );
        let mut invalid = updated["data"]["settings"].clone();
        invalid["sync"]["sync_server_url"] = json!("not-a-url");
        let rejected = call_json(json!({
            "command": "settings.update",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "settings": invalid
            }
        }));
        let loaded = call_json(json!({
            "command": "settings.get",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy()
            }
        }));

        assert_eq!(rejected["ok"], false);
        assert_eq!(
            loaded["data"]["settings"]["sync"]["sync_server_url"],
            "https://sync.example.test"
        );
        let _ = std::fs::remove_dir_all(app_support_dir);
    }

    #[test]
    fn settings_storage_does_not_contain_plaintext_secret_values() {
        let app_support_dir = std::env::temp_dir().join(format!("ham-ios-{}", Uuid::new_v4()));
        let created = call_json(json!({
            "command": "settings.create_default",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy()
            }
        }));
        let mut settings = created["data"]["settings"].clone();
        settings["providers"]["credential_metadata"]["qrz-xml"] = json!({
            "username": "KE8YGW",
            "password_configured": "true"
        });
        let updated = call_json(json!({
            "command": "settings.update",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "settings": settings
            }
        }));
        let serialized =
            std::fs::read_to_string(app_support_dir.join("Rust/application-settings.json"))
                .unwrap();

        assert_eq!(updated["ok"], true);
        assert!(!serialized.contains("super-secret"));
        assert!(!serialized.contains("\"password\":\""));
        let _ = std::fs::remove_dir_all(app_support_dir);
    }

    #[test]
    fn activation_start_uses_proposal_pipeline() {
        let app_support_dir = std::env::temp_dir().join(format!("ham-ios-{}", Uuid::new_v4()));
        let response = call_json(json!({
            "command": "activation.start",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "activation_type": "pota",
                "station_callsign": "KE8YGW",
                "operator_callsign": "KE8YGW",
                "started_at": "2026-07-10T12:00:00Z",
                "park_id": "US-1234"
            }
        }));

        assert_eq!(response["ok"], true);
        assert_eq!(
            response["data"]["official_event"]["event_type"],
            "official.log.activation.started"
        );
        assert!(response["data"]["official_event"]["entity_id"].is_string());
        let _ = std::fs::remove_dir_all(app_support_dir);
    }

    #[test]
    fn net_session_and_checkin_use_proposal_pipeline() {
        let app_support_dir = std::env::temp_dir().join(format!("ham-ios-{}", Uuid::new_v4()));
        let started = call_json(json!({
            "command": "net.session.start",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "station_callsign": "KE8YGW",
                "net_control_operator_id": "KE8YGW",
                "net_name": "Test Net",
                "started_at": "2026-07-10T12:00:00Z"
            }
        }));
        let session_id = started["data"]["official_event"]["entity_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let checkin = call_json(json!({
            "command": "net.checkin.create",
            "payload": {
                "app_support_dir": app_support_dir.to_string_lossy(),
                "net_session_id": session_id,
                "callsign": "K1ABC",
                "checkin_time": "2026-07-10T12:05:00Z"
            }
        }));

        assert_eq!(started["ok"], true);
        assert_eq!(checkin["ok"], true);
        assert_eq!(
            checkin["data"]["official_event"]["event_type"],
            "official.log.net.checkin.created"
        );
        let _ = std::fs::remove_dir_all(app_support_dir);
    }
}
