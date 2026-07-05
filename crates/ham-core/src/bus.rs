use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::event::CoreEventEnvelope;

/// Typed runtime event published inside the core.
#[derive(Debug, Clone, PartialEq)]
pub enum BusEvent {
    OfficialLogbookEvent(CoreEventEnvelope),
    Diagnostic(RuntimeDiagnosticEvent),
}

/// Diagnostic-only runtime event.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeDiagnosticEvent {
    pub event_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub message: String,
    pub payload: Value,
}

#[derive(Debug, Error)]
pub enum EventBusError {
    #[error("no active subscribers accepted the event")]
    NoSubscribers,
}

/// Async publish/subscribe bus used by core services and future plugins.
#[async_trait]
pub trait EventBus: Send + Sync {
    async fn publish(&self, event: BusEvent) -> Result<usize, EventBusError>;
    fn subscribe(&self) -> broadcast::Receiver<BusEvent>;
}

#[derive(Debug, Clone)]
pub struct InMemoryEventBus {
    sender: broadcast::Sender<BusEvent>,
}

impl InMemoryEventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }
}

impl Default for InMemoryEventBus {
    fn default() -> Self {
        Self::new(128)
    }
}

#[async_trait]
impl EventBus for InMemoryEventBus {
    async fn publish(&self, event: BusEvent) -> Result<usize, EventBusError> {
        match self.sender.send(event) {
            Ok(receiver_count) => Ok(receiver_count),
            Err(_) => Ok(0),
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<BusEvent> {
        self.sender.subscribe()
    }
}
