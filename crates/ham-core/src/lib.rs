//! Shared core for the local-first amateur radio operations platform.

pub mod bus;
pub mod event;
pub mod projection;
pub mod proposal;
pub mod runtime_log;
pub mod store;

pub use bus::{
    redact_payload, BusEvent, EventBus, InMemoryEventBus, RuntimeDiagnosticEvent,
    RuntimeEventEnvelope, RuntimeEventFilter, RuntimeEventSeverity,
};
pub use event::{CoreEventEnvelope, NewLogbookEvent};
pub use projection::{Projection, QsoCurrentStateProjection, QsoRecord};
pub use proposal::{
    submit_proposal, OperatorRole, ProposalContext, ProposalOutcome, ProposalValidationError,
};
pub use runtime_log::{
    default_log_directory, RuntimeJsonlLogWriter, RuntimeLogConfig, DEFAULT_RUNTIME_LOG_MAX_BYTES,
    DEFAULT_RUNTIME_LOG_RETAINED_FILES, RUNTIME_LOG_FILE_NAME,
};
pub use store::{ChainVerificationError, InMemoryLogbookEventStore, LogbookEventStore};

#[cfg(test)]
mod tests;
