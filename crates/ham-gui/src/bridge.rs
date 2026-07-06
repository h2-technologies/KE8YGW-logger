use std::{
    io,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use async_trait::async_trait;
use ham_core::{
    BusEvent, EventBus, EventBusError, InMemoryEventBus, RuntimeEventEnvelope, RuntimeEventFilter,
    RuntimeEventSeverity, RuntimeJsonlLogWriter, RuntimeLogConfig,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::runtime::Runtime;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeBridgeStatus {
    pub connected: bool,
    pub session_id: Uuid,
    pub device_id: Uuid,
    pub runtime_event_count: u64,
    pub latest_error_count: u64,
    pub sync_state: String,
    pub log_directory: PathBuf,
}

#[derive(Debug, Clone)]
pub struct RuntimeEventInput {
    pub event_type: String,
    pub severity: RuntimeEventSeverity,
    pub source: String,
    pub source_plugin_id: Option<String>,
    pub workspace_id: Option<String>,
    pub payload_summary: String,
    pub redacted_payload: Option<serde_json::Value>,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GuiRuntimeBridge {
    inner: Arc<GuiRuntimeBridgeInner>,
}

#[derive(Debug)]
struct GuiRuntimeBridgeInner {
    bus: InMemoryEventBus,
    runtime: Runtime,
    log_writer: Mutex<RuntimeJsonlLogWriter>,
    status: Mutex<RuntimeBridgeStatus>,
}

impl GuiRuntimeBridge {
    pub fn new(log_config: RuntimeLogConfig) -> io::Result<Self> {
        let log_writer = RuntimeJsonlLogWriter::new(log_config.clone())?;
        let status = RuntimeBridgeStatus {
            connected: true,
            session_id: Uuid::new_v4(),
            device_id: Uuid::new_v4(),
            runtime_event_count: 0,
            latest_error_count: 0,
            sync_state: "Local only".to_owned(),
            log_directory: log_config.directory,
        };

        Ok(Self {
            inner: Arc::new(GuiRuntimeBridgeInner {
                bus: InMemoryEventBus::new(512),
                runtime: Runtime::new()?,
                log_writer: Mutex::new(log_writer),
                status: Mutex::new(status),
            }),
        })
    }

    pub fn status(&self) -> RuntimeBridgeStatus {
        self.inner
            .status
            .lock()
            .expect("runtime bridge status mutex should not be poisoned")
            .clone()
    }

    pub fn publish(&self, input: RuntimeEventInput) -> io::Result<RuntimeEventEnvelope> {
        let status = self.status();
        let event = RuntimeEventEnvelope::new(
            input.event_type,
            input.severity,
            input.source,
            input.source_plugin_id,
            Uuid::new_v4(),
            status.session_id,
            status.device_id,
            input.workspace_id,
            input.payload_summary,
            input.redacted_payload,
            input.error,
        );

        self.inner
            .log_writer
            .lock()
            .expect("runtime log writer mutex should not be poisoned")
            .append(&event)?;
        self.inner
            .runtime
            .block_on(self.inner.bus.publish_runtime(event.clone()))
            .map_err(io::Error::other)?;

        let mut status = self
            .inner
            .status
            .lock()
            .expect("runtime bridge status mutex should not be poisoned");
        status.runtime_event_count += 1;
        if event.severity == RuntimeEventSeverity::Error {
            status.latest_error_count += 1;
        }

        Ok(event)
    }

    pub fn replay(&self, filter: RuntimeEventFilter, limit: usize) -> Vec<RuntimeEventEnvelope> {
        self.inner
            .runtime
            .block_on(self.inner.bus.replay_runtime_events(filter, limit))
    }

    pub fn export_jsonl(&self, filter: RuntimeEventFilter, limit: usize) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        for event in self.replay(filter, limit).into_iter().rev() {
            serde_json::to_writer(&mut bytes, &event).map_err(io::Error::other)?;
            bytes.push(b'\n');
        }
        Ok(bytes)
    }

    pub fn seed_startup_events(&self) -> io::Result<()> {
        self.publish(RuntimeEventInput {
            event_type: "app.started".to_owned(),
            severity: RuntimeEventSeverity::Info,
            source: "ham-gui".to_owned(),
            source_plugin_id: None,
            workspace_id: Some("dashboard".to_owned()),
            payload_summary: "GUI runtime bridge started".to_owned(),
            redacted_payload: Some(json!({"bridge": "core-event-bus"})),
            error: None,
        })?;
        self.publish(RuntimeEventInput {
            event_type: "diagnostics.logs.ready".to_owned(),
            severity: RuntimeEventSeverity::Info,
            source: "ham-gui".to_owned(),
            source_plugin_id: None,
            workspace_id: None,
            payload_summary: "Runtime JSONL log writer initialized".to_owned(),
            redacted_payload: Some(json!({"rotation": "10MB", "retained_files": 5})),
            error: None,
        })?;
        self.publish(RuntimeEventInput {
            event_type: "sync.state".to_owned(),
            severity: RuntimeEventSeverity::Debug,
            source: "ham-sync".to_owned(),
            source_plugin_id: None,
            workspace_id: Some("dashboard".to_owned()),
            payload_summary: "Sync state is local-only".to_owned(),
            redacted_payload: Some(json!({"state": "local-only"})),
            error: None,
        })?;
        Ok(())
    }
}

#[async_trait]
impl EventBus for GuiRuntimeBridge {
    async fn publish(&self, event: BusEvent) -> Result<usize, EventBusError> {
        if let BusEvent::Runtime(runtime_event) = &event {
            self.inner
                .log_writer
                .lock()
                .expect("runtime log writer mutex should not be poisoned")
                .append(runtime_event)?;

            let mut status = self
                .inner
                .status
                .lock()
                .expect("runtime bridge status mutex should not be poisoned");
            status.runtime_event_count += 1;
            if runtime_event.severity == RuntimeEventSeverity::Error {
                status.latest_error_count += 1;
            }
        }

        self.inner.bus.publish(event).await
    }

    fn subscribe(&self) -> tokio::sync::broadcast::Receiver<BusEvent> {
        self.inner.bus.subscribe()
    }

    async fn replay_runtime_events(
        &self,
        filter: RuntimeEventFilter,
        limit: usize,
    ) -> Vec<RuntimeEventEnvelope> {
        self.inner.bus.replay_runtime_events(filter, limit).await
    }
}
