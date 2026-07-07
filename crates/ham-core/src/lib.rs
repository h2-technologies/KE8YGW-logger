//! Shared core for the local-first amateur radio operations platform.

pub mod adif;
pub mod awards;
pub mod bus;
pub mod diagnostics;
pub mod event;
pub mod lookup;
pub mod permissions;
pub mod projection;
pub mod proposal;
pub mod rig;
pub mod runtime_log;
pub mod search;
pub mod service;
pub mod station;
pub mod store;
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
pub use upload::{
    adif_for_upload_job, append_upload_status_event, build_log_upload_request,
    select_qsos_for_upload, UploadJob as UploadQueueJob, UploadJobItem, UploadQueue,
    UploadQueueError, UploadResult, UploadStatus, UploadTarget,
};

#[cfg(test)]
mod tests;
