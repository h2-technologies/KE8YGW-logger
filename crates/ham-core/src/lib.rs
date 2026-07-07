//! Shared core for the local-first amateur radio operations platform.

pub mod adif;
pub mod awards;
pub mod bus;
pub mod credential;
pub mod diagnostics;
pub mod event;
pub mod lookup;
pub mod map;
pub mod net;
pub mod online;
pub mod permissions;
pub mod projection;
pub mod proposal;
pub mod rig;
pub mod runtime_log;
pub mod search;
pub mod service;
pub mod station;
pub mod store;
pub mod support;
pub mod upload;

pub use adif::{
    export_adif, export_adif_with_activations, import_adif, parse_adif, AdifImportOptions,
    AdifImportSummary, DuplicatePolicy,
};
pub use awards::{
    compute_award_progress, default_award_definitions, AwardCredit, AwardDefinition, AwardEngine,
    AwardPlugin, AwardProgress, AwardRequirement, AwardRule,
};
pub use bus::{
    redact_payload, BusEvent, EventBus, EventBusError, InMemoryEventBus, RuntimeDiagnosticEvent,
    RuntimeEventEnvelope, RuntimeEventFilter, RuntimeEventSeverity,
};
pub use credential::{
    authorize_credential_action, credential_runtime_payload, os_backend_name,
    required_credentials_satisfied, CredentialBackendStatus, CredentialError, CredentialMetadata,
    CredentialStatus, CredentialStore, InsecureDevCredentialStore, UnsupportedOsCredentialStore,
};
pub use diagnostics::{
    action_timeline, build_diagnostic_bundle, bundle_content_hash, export_diagnostic_zip,
    redact_for_report, ActionTimelineEntry, DiagnosticBundle, DiagnosticBundleFile,
    DiagnosticBundleInput, DiagnosticBundleManifest, DiagnosticBundlePreview, DiagnosticReportType,
    RedactionSummary, REPORT_FORMAT_VERSION,
};
pub use event::{CoreEventEnvelope, NewLogbookEvent};
pub use lookup::lookup_callsign_with_service_framework;
pub use lookup::{
    clear_lookup_cache, grid_to_lat_lon, lookup_callsign_with_cache, normalize_callsign,
    validate_grid, CallsignLookupProvider, EntityInfo, GridInfo, LocalPrefixProvider, LookupCache,
    LookupCacheConfig, LookupError, LookupProviderStatus, LookupResult, LookupSuggestion,
    MockLookupProvider, QrzLookupProviderStub,
};
pub use map::{
    bearing_between, encode_maidenhead, final_bearing, grayline_snapshot, great_circle_distance,
    great_circle_midpoint, great_circle_path, grid_precision, maidenhead_bounds,
    maidenhead_to_coordinate, map_provider_metadata, mock_propagation_forecast, mock_weather,
    neighbor_grids, normalize_maidenhead, publish_map_runtime_event, qso_map_objects,
    station_markers_from_profiles, validate_maidenhead, BandConditions, BearingResult, Coordinate,
    CurrentWeather, DistanceResult, DistanceUnit, ElevationResult, Forecast, GeoBounds, GeoPath,
    GeoPoint, GeoPolygon, GraylineSnapshot, GridSquare, MapError, MapLayer, MapLayerKind,
    MapLayerStack, MapMarker, MapMarkerType, MapOverlay, MapProvider, PlaceholderMapProvider,
    PropagationForecast, QsoMapFilter, QsoMapObject, SolarConditions, Wind,
};
pub use net::{
    export_net_report_markdown, NetCheckInRecord, NetCheckInStatus, NetControlProjection,
    NetProjectionError, NetSessionRecord, NetSessionStatus, NetTemplate, NetTrafficLevel,
    NetTrafficPrecedence, NetTrafficRecord, NetTrafficStatus,
};
pub use online::{
    append_confirmation_events, cache_provider_value, confirmations_from_adif,
    default_online_automation_tasks, dx_cluster_spot_to_spot, execute_upload_with_provider,
    missing_credential_status, next_retry_delay, notification_for_upload_result,
    online_provider_metadata, online_runtime_event_payload, online_services_dashboard,
    parse_dx_cluster_line, parse_noaa_solar_summary, pota_spot_to_spot, provider_metadata_for_kind,
    runtime_severity_for_provider_status, upload_engine_stats, upload_execution_from_response,
    ConfirmationDownloadRequest, ConfirmationDownloadResponse, ConfirmationRecord, DxClusterSpot,
    NotificationSeverity, OnlineAccount, OnlineAutomationTask, OnlineNotification,
    OnlineProviderHealth, OnlineProviderStatus, OnlineServiceError, OnlineServiceProviderKind,
    OnlineServicesDashboard, PotaSpotRecord, RetryPolicy, SolarIndexReport, UploadEngineConfig,
    UploadEngineStats, UploadExecutionResult,
};
pub use permissions::{
    check_plugin_permission, grant_builtin_defaults, JsonPermissionGrantStore, PermissionError,
    PermissionGrant, PermissionGrantSet, PermissionGrantStatus, PermissionMetadata,
    PermissionRegistry, PermissionRiskLevel, PermissionSettings,
};
pub use projection::{
    ActivationProjection, ActivationRecord, Projection, QsoCurrentStateProjection, QsoRecord,
};
pub use proposal::{
    submit_proposal, OperatorRole, ProposalContext, ProposalOutcome, ProposalValidationError,
};
pub use rig::{
    apply_rig_suggestion_to_form, infer_band, publish_rig_runtime_event, suggestion_from_rig_state,
    HamlibProviderStub, MockRigProvider, RigAutofillSuggestion, RigConnectionStatus,
    RigConnectionType, RigDevice, RigError, RigProvider, RigProviderStatus, RigState,
};
pub use runtime_log::{
    default_log_directory, RuntimeJsonlLogWriter, RuntimeLogConfig, DEFAULT_RUNTIME_LOG_MAX_BYTES,
    DEFAULT_RUNTIME_LOG_RETAINED_FILES, RUNTIME_LOG_FILE_NAME,
};
pub use search::{
    parse_search_query, search_qsos, DateRange, JsonSavedSearchStore, SavedSearch, SavedSearchBook,
    SavedSearchStoreError, SearchError, SearchFilter, SearchQuery, SearchResult,
};
pub use service::{
    authorize_service_request, cache_entry_for_value, default_service_registry,
    local_prefix_provider_metadata, mock_lookup_provider_metadata, publish_service_runtime_event,
    qrz_lookup_provider_metadata, CallsignLookupRequest, CallsignLookupResponse, LogUploadProvider,
    LogUploadRequest, LogUploadResponse, MapTileRequest, MapTileResponse, MockSpottingProvider,
    PropagationRequest, PropagationResponse, ProviderHealth, ProviderHealthState,
    ProviderSelectionCriteria, RegisteredServiceProvider, ServiceCache, ServiceCacheEntry,
    ServiceError, ServiceProvider, ServiceProviderMetadata, ServiceRegistry,
    ServiceRegistrySnapshot, Spot, SpotAlertRule, SpotFilter, SpotQueryRequest, SpotQueryResponse,
    SpotSource, SpottingProvider, StubLogUploadProvider, UploadJob, UploadJobStatus,
    WeatherRequest, WeatherResponse,
};
pub use station::{
    EquipmentItem, EquipmentStatus, EquipmentType, JsonStationBookStore, StationBook,
    StationConfiguration, StationProfile, StationStoreError,
};
pub use store::{
    default_official_event_log_path, validate_supported_remote_event, ChainVerificationError,
    InMemoryLogbookEventStore, JsonlLogbookEventStore, LogbookEventStore, StoreError,
};
pub use support::{JsonSupportStore, SupportEnvelope, SupportStoreError, SUPPORT_FILE_VERSION};
pub use upload::{
    adif_for_upload_job, append_upload_status_event, build_log_upload_request,
    select_qsos_for_upload, UploadJob as UploadQueueJob, UploadJobItem, UploadQueue,
    UploadQueueError, UploadResult, UploadStatus, UploadTarget,
};

#[cfg(test)]
mod tests;
