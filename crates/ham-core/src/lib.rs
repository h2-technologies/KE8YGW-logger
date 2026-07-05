//! Shared core for the local-first amateur radio operations platform.

pub mod bus;
pub mod event;
pub mod projection;
pub mod proposal;
pub mod store;

pub use bus::{BusEvent, EventBus, InMemoryEventBus, RuntimeDiagnosticEvent};
pub use event::{CoreEventEnvelope, NewLogbookEvent};
pub use projection::{Projection, QsoCurrentStateProjection, QsoRecord};
pub use proposal::{
    submit_proposal, OperatorRole, ProposalContext, ProposalOutcome, ProposalValidationError,
};
pub use store::{ChainVerificationError, InMemoryLogbookEventStore, LogbookEventStore};

#[cfg(test)]
mod tests;
