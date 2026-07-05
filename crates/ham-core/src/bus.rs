use std::{collections::VecDeque, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::event::CoreEventEnvelope;

/// Typed runtime event published inside the core.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BusEvent {
    OfficialLogbookEvent(CoreEventEnvelope),
    Runtime(RuntimeEventEnvelope),
}

/// Backward-compatible name for diagnostic runtime events.
pub type RuntimeDiagnosticEvent = RuntimeEventEnvelope;

/// Diagnostic-only runtime event. This is separate from official logbook
/// events and is safe to persist in rotating JSONL logs after redaction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeEventEnvelope {
    pub event_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub severity: RuntimeEventSeverity,
    pub source: String,
    pub source_plugin_id: Option<String>,
    pub correlation_id: Uuid,
    pub session_id: Uuid,
    pub device_id: Uuid,
    pub workspace_id: Option<String>,
    pub payload_summary: String,
    pub redacted_payload: Option<Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuntimeEventSeverity {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl RuntimeEventEnvelope {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        event_type: impl Into<String>,
        severity: RuntimeEventSeverity,
        source: impl Into<String>,
        source_plugin_id: Option<String>,
        correlation_id: Uuid,
        session_id: Uuid,
        device_id: Uuid,
        workspace_id: Option<String>,
        payload_summary: impl Into<String>,
        redacted_payload: Option<Value>,
        error: Option<String>,
    ) -> Self {
        Self {
            event_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: event_type.into(),
            severity,
            source: source.into(),
            source_plugin_id,
            correlation_id,
            session_id,
            device_id,
            workspace_id,
            payload_summary: payload_summary.into(),
            redacted_payload: redacted_payload.map(redact_payload),
            error,
        }
    }

    pub fn category(&self) -> &str {
        self.event_type
            .split('.')
            .next()
            .unwrap_or(&self.event_type)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEventFilter {
    pub severity: Option<RuntimeEventSeverity>,
    pub category: Option<String>,
    pub source: Option<String>,
    pub text: Option<String>,
}

impl RuntimeEventFilter {
    pub fn matches(&self, event: &RuntimeEventEnvelope) -> bool {
        if self
            .severity
            .is_some_and(|severity| event.severity != severity)
        {
            return false;
        }

        if let Some(category) = &self.category {
            let prefix = format!("{category}.");
            if event.event_type != *category && !event.event_type.starts_with(&prefix) {
                return false;
            }
        }

        if let Some(source) = &self.source {
            let source = source.to_ascii_lowercase();
            let source_matches = event.source.to_ascii_lowercase().contains(&source);
            let plugin_matches = event
                .source_plugin_id
                .as_deref()
                .is_some_and(|plugin| plugin.to_ascii_lowercase().contains(&source));
            if !source_matches && !plugin_matches {
                return false;
            }
        }

        if let Some(text) = &self.text {
            let haystack = format!(
                "{} {} {} {}",
                event.event_type,
                event.source,
                event.payload_summary,
                event.error.as_deref().unwrap_or_default()
            )
            .to_ascii_lowercase();
            if !haystack.contains(&text.to_ascii_lowercase()) {
                return false;
            }
        }

        true
    }
}

pub fn redact_payload(payload: Value) -> Value {
    match payload {
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| {
                    if is_secret_like_key(&key) {
                        (key, Value::String("[REDACTED]".to_owned()))
                    } else {
                        (key, redact_payload(value))
                    }
                })
                .collect(),
        ),
        Value::Array(values) => Value::Array(values.into_iter().map(redact_payload).collect()),
        other => other,
    }
}

fn is_secret_like_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "password",
        "passwd",
        "token",
        "secret",
        "api_key",
        "apikey",
        "authorization",
        "auth",
        "private_key",
    ]
    .iter()
    .any(|needle| key.contains(needle))
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
    async fn replay_runtime_events(
        &self,
        filter: RuntimeEventFilter,
        limit: usize,
    ) -> Vec<RuntimeEventEnvelope>;
}

#[derive(Debug, Clone)]
pub struct InMemoryEventBus {
    sender: broadcast::Sender<BusEvent>,
    recent_runtime_events: Arc<RwLock<VecDeque<RuntimeEventEnvelope>>>,
    replay_capacity: usize,
}

impl InMemoryEventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self {
            sender,
            recent_runtime_events: Arc::new(RwLock::new(VecDeque::with_capacity(capacity))),
            replay_capacity: capacity,
        }
    }

    pub async fn publish_runtime(
        &self,
        event: RuntimeEventEnvelope,
    ) -> Result<usize, EventBusError> {
        self.publish(BusEvent::Runtime(event)).await
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
        if let BusEvent::Runtime(runtime_event) = &event {
            let mut recent = self.recent_runtime_events.write().await;
            if recent.len() >= self.replay_capacity {
                recent.pop_front();
            }
            recent.push_back(runtime_event.clone());
        }

        match self.sender.send(event) {
            Ok(receiver_count) => Ok(receiver_count),
            Err(_) => Ok(0),
        }
    }

    fn subscribe(&self) -> broadcast::Receiver<BusEvent> {
        self.sender.subscribe()
    }

    async fn replay_runtime_events(
        &self,
        filter: RuntimeEventFilter,
        limit: usize,
    ) -> Vec<RuntimeEventEnvelope> {
        self.recent_runtime_events
            .read()
            .await
            .iter()
            .rev()
            .filter(|event| filter.matches(event))
            .take(limit)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        redact_payload, BusEvent, EventBus, InMemoryEventBus, RuntimeEventEnvelope,
        RuntimeEventFilter, RuntimeEventSeverity,
    };
    use uuid::Uuid;

    fn runtime_event(event_type: &str, severity: RuntimeEventSeverity) -> RuntimeEventEnvelope {
        RuntimeEventEnvelope::new(
            event_type,
            severity,
            "test-source",
            Some("plugin.test".to_owned()),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            Some("dashboard".to_owned()),
            "test summary",
            Some(json!({"token": "abc123", "safe": "value"})),
            None,
        )
    }

    #[test]
    fn runtime_event_serializes_and_deserializes() {
        let event = runtime_event("diagnostics.test", RuntimeEventSeverity::Info);
        let encoded = serde_json::to_string(&event).unwrap();
        let decoded: RuntimeEventEnvelope = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.event_id, event.event_id);
        assert_eq!(decoded.event_type, "diagnostics.test");
    }

    #[tokio::test]
    async fn pub_sub_receives_runtime_events() {
        let bus = InMemoryEventBus::default();
        let mut subscriber = bus.subscribe();
        let event = runtime_event("ui.started", RuntimeEventSeverity::Info);

        bus.publish_runtime(event.clone()).await.unwrap();

        let received = subscriber.recv().await.unwrap();
        assert!(
            matches!(received, BusEvent::Runtime(received) if received.event_id == event.event_id)
        );
    }

    #[tokio::test]
    async fn multiple_subscribers_receive_same_event() {
        let bus = InMemoryEventBus::default();
        let mut first = bus.subscribe();
        let mut second = bus.subscribe();
        let event = runtime_event("sync.state", RuntimeEventSeverity::Debug);

        bus.publish_runtime(event.clone()).await.unwrap();

        assert!(
            matches!(first.recv().await.unwrap(), BusEvent::Runtime(received) if received.event_id == event.event_id)
        );
        assert!(
            matches!(second.recv().await.unwrap(), BusEvent::Runtime(received) if received.event_id == event.event_id)
        );
    }

    #[test]
    fn runtime_event_filter_matches_category_severity_source_and_text() {
        let event = runtime_event("plugin.loaded", RuntimeEventSeverity::Warn);
        let filter = RuntimeEventFilter {
            severity: Some(RuntimeEventSeverity::Warn),
            category: Some("plugin".to_owned()),
            source: Some("test".to_owned()),
            text: Some("summary".to_owned()),
        };

        assert!(filter.matches(&event));
    }

    #[test]
    fn redaction_helper_masks_secret_like_fields() {
        let redacted = redact_payload(json!({
            "password": "secret",
            "nested": {
                "api_key": "abc",
                "callsign": "K1ABC"
            }
        }));

        assert_eq!(redacted["password"], "[REDACTED]");
        assert_eq!(redacted["nested"]["api_key"], "[REDACTED]");
        assert_eq!(redacted["nested"]["callsign"], "K1ABC");
    }
}
