//! Shared core for the local-first amateur radio operations platform.

pub mod adif;
pub mod bus;
pub mod event;
pub mod projection;
pub mod proposal;
pub mod runtime_log;
pub mod store;

pub use adif::{
    export_adif, export_adif_with_activations, import_adif, parse_adif, AdifImportOptions,
    AdifImportSummary, DuplicatePolicy,
};
pub use bus::{
    redact_payload, BusEvent, EventBus, EventBusError, InMemoryEventBus, RuntimeDiagnosticEvent,
    RuntimeEventEnvelope, RuntimeEventFilter, RuntimeEventSeverity,
};
pub use event::{CoreEventEnvelope, NewLogbookEvent};
pub use projection::{
    ActivationProjection, ActivationRecord, Projection, QsoCurrentStateProjection, QsoRecord,
};
pub use proposal::{
    submit_proposal, OperatorRole, ProposalContext, ProposalOutcome, ProposalValidationError,
};
pub use runtime_log::{
    default_log_directory, RuntimeJsonlLogWriter, RuntimeLogConfig, DEFAULT_RUNTIME_LOG_MAX_BYTES,
    DEFAULT_RUNTIME_LOG_RETAINED_FILES, RUNTIME_LOG_FILE_NAME,
};
pub use store::{
    default_official_event_log_path, validate_supported_remote_event, ChainVerificationError,
    InMemoryLogbookEventStore, JsonlLogbookEventStore, LogbookEventStore, StoreError,
};

#[cfg(test)]
mod tests;
