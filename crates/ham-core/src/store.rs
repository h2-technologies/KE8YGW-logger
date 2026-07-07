use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
};

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::event::{CoreEventEnvelope, NewLogbookEvent};
use crate::projection::{ActivationProjection, Projection, QsoCurrentStateProjection};
use ham_plugin_sdk::{
    OFFICIAL_LOG_ACTIVATION_CANCELLED, OFFICIAL_LOG_ACTIVATION_CREATED,
    OFFICIAL_LOG_ACTIVATION_ENDED, OFFICIAL_LOG_ACTIVATION_NOTE_ADDED,
    OFFICIAL_LOG_ACTIVATION_STARTED, OFFICIAL_LOG_ACTIVATION_UPDATED,
    OFFICIAL_LOG_QSO_ACTIVATION_LINKED, OFFICIAL_LOG_QSO_ACTIVATION_UNLINKED,
    OFFICIAL_LOG_QSO_CORRECTED, OFFICIAL_LOG_QSO_CREATED, OFFICIAL_LOG_QSO_DELETED,
    OFFICIAL_LOG_QSO_NOTE_ADDED, OFFICIAL_LOG_QSO_RESTORED, OFFICIAL_LOG_UPLOAD_COMPLETED,
    OFFICIAL_LOG_UPLOAD_FAILED, OFFICIAL_LOG_UPLOAD_QUEUED,
};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("event not found: {0}")]
    EventNotFound(Uuid),
    #[error("chain verification failed: {0}")]
    ChainVerification(#[from] ChainVerificationError),
    #[error("projection rebuild failed: {0}")]
    Projection(String),
    #[error("event store I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("event store serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("remote event {event_id} has an invalid hash")]
    InvalidRemoteHash { event_id: Uuid },
    #[error("remote event {event_id} uses unsupported schema version {schema_version}")]
    UnsupportedSchemaVersion { event_id: Uuid, schema_version: u32 },
    #[error("remote event {event_id} has unsupported event type {event_type}")]
    UnsupportedEventType { event_id: Uuid, event_type: String },
    #[error("remote event {event_id} previous hash {actual:?} does not connect to local head {expected:?}")]
    RemotePreviousHashMismatch {
        event_id: Uuid,
        expected: Option<String>,
        actual: Option<String>,
    },
    #[error("remote event id {event_id} already exists with different content")]
    DuplicateEventConflict { event_id: Uuid },
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ChainVerificationError {
    #[error("event {event_id} has an invalid hash")]
    InvalidHash { event_id: Uuid },
    #[error("event {event_id} expected previous hash {expected:?}, found {actual:?}")]
    PreviousHashMismatch {
        event_id: Uuid,
        expected: Option<String>,
        actual: Option<String>,
    },
    #[error("stored head {head_hash:?} does not match final event hash {actual_hash:?}")]
    HeadMismatch {
        head_hash: Option<String>,
        actual_hash: Option<String>,
    },
}

#[async_trait]
pub trait LogbookEventStore: Send + Sync {
    async fn append_event(&self, event: NewLogbookEvent) -> Result<CoreEventEnvelope, StoreError>;
    async fn append_verified_remote_event(
        &self,
        event: CoreEventEnvelope,
    ) -> Result<CoreEventEnvelope, StoreError>;
    async fn get_event(&self, event_id: Uuid) -> Result<Option<CoreEventEnvelope>, StoreError>;
    async fn get_head(&self, logbook_id: Uuid) -> Result<Option<String>, StoreError>;
    async fn list_events(&self, logbook_id: Uuid) -> Result<Vec<CoreEventEnvelope>, StoreError>;
    async fn list_events_after(
        &self,
        logbook_id: Uuid,
        after_hash: Option<String>,
    ) -> Result<Vec<CoreEventEnvelope>, StoreError>;
    async fn load_since(
        &self,
        logbook_id: Uuid,
        after_hash: Option<String>,
    ) -> Result<Vec<CoreEventEnvelope>, StoreError> {
        self.list_events_after(logbook_id, after_hash).await
    }
    async fn verify_chain(&self, logbook_id: Uuid) -> Result<(), StoreError>;
    async fn rebuild_projections(
        &self,
        logbook_id: Uuid,
    ) -> Result<QsoCurrentStateProjection, StoreError>;
    async fn rebuild_activation_projections(
        &self,
        logbook_id: Uuid,
    ) -> Result<ActivationProjection, StoreError>;
}

#[derive(Debug)]
pub struct JsonlLogbookEventStore {
    path: PathBuf,
    memory: InMemoryLogbookEventStore,
}

impl JsonlLogbookEventStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, StoreError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let inner = load_inner_from_disk(&path)?;
        Ok(Self {
            path,
            memory: InMemoryLogbookEventStore {
                inner: RwLock::new(inner),
            },
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn persist_event(&self, event: &CoreEventEnvelope) -> Result<(), StoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        serde_json::to_writer(&mut file, event)?;
        file.write_all(b"\n")?;
        Ok(())
    }
}

fn load_inner_from_disk(path: &Path) -> Result<InMemoryStoreInner, StoreError> {
    let mut inner = InMemoryStoreInner::default();
    if !path.exists() {
        return Ok(inner);
    }

    let file = fs::File::open(path)?;
    let reader = io::BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let event: CoreEventEnvelope = serde_json::from_str(&line)?;
        inner
            .events_by_logbook
            .entry(event.logbook_id)
            .or_default()
            .push(event.event_id);
        inner
            .heads
            .insert(event.logbook_id, event.event_hash.clone());
        inner.events_by_id.insert(event.event_id, event);
    }
    Ok(inner)
}

#[async_trait]
impl LogbookEventStore for JsonlLogbookEventStore {
    async fn append_event(&self, event: NewLogbookEvent) -> Result<CoreEventEnvelope, StoreError> {
        let official_event = self.memory.append_event(event).await?;
        self.persist_event(&official_event)?;
        Ok(official_event)
    }

    async fn append_verified_remote_event(
        &self,
        event: CoreEventEnvelope,
    ) -> Result<CoreEventEnvelope, StoreError> {
        if let Some(existing) = self.memory.get_event(event.event_id).await? {
            if existing == event {
                return Ok(existing);
            }
        }
        let official_event = self.memory.append_verified_remote_event(event).await?;
        self.persist_event(&official_event)?;
        Ok(official_event)
    }

    async fn get_event(&self, event_id: Uuid) -> Result<Option<CoreEventEnvelope>, StoreError> {
        self.memory.get_event(event_id).await
    }

    async fn get_head(&self, logbook_id: Uuid) -> Result<Option<String>, StoreError> {
        self.memory.get_head(logbook_id).await
    }

    async fn list_events(&self, logbook_id: Uuid) -> Result<Vec<CoreEventEnvelope>, StoreError> {
        self.memory.list_events(logbook_id).await
    }

    async fn list_events_after(
        &self,
        logbook_id: Uuid,
        after_hash: Option<String>,
    ) -> Result<Vec<CoreEventEnvelope>, StoreError> {
        self.memory.list_events_after(logbook_id, after_hash).await
    }

    async fn verify_chain(&self, logbook_id: Uuid) -> Result<(), StoreError> {
        self.memory.verify_chain(logbook_id).await
    }

    async fn rebuild_projections(
        &self,
        logbook_id: Uuid,
    ) -> Result<QsoCurrentStateProjection, StoreError> {
        self.memory.rebuild_projections(logbook_id).await
    }

    async fn rebuild_activation_projections(
        &self,
        logbook_id: Uuid,
    ) -> Result<ActivationProjection, StoreError> {
        self.memory.rebuild_activation_projections(logbook_id).await
    }
}

pub fn default_official_event_log_path() -> PathBuf {
    if let Ok(path) = std::env::var("HAM_PLATFORM_EVENT_LOG") {
        return PathBuf::from(path);
    }

    crate::default_log_directory()
        .join("official")
        .join("official-events.jsonl")
}

#[derive(Debug, Default)]
pub struct InMemoryLogbookEventStore {
    inner: RwLock<InMemoryStoreInner>,
}

#[derive(Debug, Default)]
struct InMemoryStoreInner {
    events_by_id: HashMap<Uuid, CoreEventEnvelope>,
    events_by_logbook: HashMap<Uuid, Vec<Uuid>>,
    heads: HashMap<Uuid, String>,
}

impl InMemoryLogbookEventStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Test helper used to prove chain verification detects changed history.
    pub async fn replace_event_for_testing(&self, event: CoreEventEnvelope) {
        let mut inner = self.inner.write().await;
        inner.events_by_id.insert(event.event_id, event);
    }
}

#[async_trait]
impl LogbookEventStore for InMemoryLogbookEventStore {
    async fn append_event(&self, event: NewLogbookEvent) -> Result<CoreEventEnvelope, StoreError> {
        let mut inner = self.inner.write().await;
        let previous_hash = inner.heads.get(&event.logbook_id).cloned();
        let official_event = CoreEventEnvelope::from_new(event, previous_hash);

        inner
            .events_by_logbook
            .entry(official_event.logbook_id)
            .or_default()
            .push(official_event.event_id);
        inner
            .heads
            .insert(official_event.logbook_id, official_event.event_hash.clone());
        inner
            .events_by_id
            .insert(official_event.event_id, official_event.clone());

        Ok(official_event)
    }

    async fn append_verified_remote_event(
        &self,
        event: CoreEventEnvelope,
    ) -> Result<CoreEventEnvelope, StoreError> {
        validate_supported_remote_event(&event)?;

        let mut inner = self.inner.write().await;
        if let Some(existing) = inner.events_by_id.get(&event.event_id) {
            if existing == &event {
                return Ok(existing.clone());
            }
            return Err(StoreError::DuplicateEventConflict {
                event_id: event.event_id,
            });
        }

        let expected_previous_hash = inner.heads.get(&event.logbook_id).cloned();
        if event.previous_hash != expected_previous_hash {
            return Err(StoreError::RemotePreviousHashMismatch {
                event_id: event.event_id,
                expected: expected_previous_hash,
                actual: event.previous_hash.clone(),
            });
        }

        inner
            .events_by_logbook
            .entry(event.logbook_id)
            .or_default()
            .push(event.event_id);
        inner
            .heads
            .insert(event.logbook_id, event.event_hash.clone());
        inner.events_by_id.insert(event.event_id, event.clone());

        Ok(event)
    }

    async fn get_event(&self, event_id: Uuid) -> Result<Option<CoreEventEnvelope>, StoreError> {
        Ok(self.inner.read().await.events_by_id.get(&event_id).cloned())
    }

    async fn get_head(&self, logbook_id: Uuid) -> Result<Option<String>, StoreError> {
        Ok(self.inner.read().await.heads.get(&logbook_id).cloned())
    }

    async fn list_events(&self, logbook_id: Uuid) -> Result<Vec<CoreEventEnvelope>, StoreError> {
        self.list_events_after(logbook_id, None).await
    }

    async fn list_events_after(
        &self,
        logbook_id: Uuid,
        after_hash: Option<String>,
    ) -> Result<Vec<CoreEventEnvelope>, StoreError> {
        let inner = self.inner.read().await;
        let Some(event_ids) = inner.events_by_logbook.get(&logbook_id) else {
            return Ok(Vec::new());
        };

        let start_index = match after_hash {
            Some(hash) => event_ids
                .iter()
                .position(|event_id| {
                    inner
                        .events_by_id
                        .get(event_id)
                        .is_some_and(|event| event.event_hash == hash)
                })
                .map_or(0, |index| index + 1),
            None => 0,
        };

        Ok(event_ids[start_index..]
            .iter()
            .filter_map(|event_id| inner.events_by_id.get(event_id).cloned())
            .collect())
    }

    async fn verify_chain(&self, logbook_id: Uuid) -> Result<(), StoreError> {
        let inner = self.inner.read().await;
        let Some(event_ids) = inner.events_by_logbook.get(&logbook_id) else {
            return Ok(());
        };

        let mut expected_previous_hash: Option<String> = None;
        let mut actual_head: Option<String> = None;

        for event_id in event_ids {
            let event = inner
                .events_by_id
                .get(event_id)
                .ok_or(StoreError::EventNotFound(*event_id))?;

            if !event.hash_is_valid() {
                return Err(ChainVerificationError::InvalidHash {
                    event_id: event.event_id,
                }
                .into());
            }

            if event.previous_hash != expected_previous_hash {
                return Err(ChainVerificationError::PreviousHashMismatch {
                    event_id: event.event_id,
                    expected: expected_previous_hash,
                    actual: event.previous_hash.clone(),
                }
                .into());
            }

            actual_head = Some(event.event_hash.clone());
            expected_previous_hash = actual_head.clone();
        }

        let stored_head = inner.heads.get(&logbook_id).cloned();
        if stored_head != actual_head {
            return Err(ChainVerificationError::HeadMismatch {
                head_hash: stored_head,
                actual_hash: actual_head,
            }
            .into());
        }

        Ok(())
    }

    async fn rebuild_projections(
        &self,
        logbook_id: Uuid,
    ) -> Result<QsoCurrentStateProjection, StoreError> {
        let events = self.list_events(logbook_id).await?;
        let mut projection = QsoCurrentStateProjection::new();
        projection
            .rebuild(&events)
            .map_err(|error| StoreError::Projection(error.to_string()))?;
        Ok(projection)
    }

    async fn rebuild_activation_projections(
        &self,
        logbook_id: Uuid,
    ) -> Result<ActivationProjection, StoreError> {
        let events = self.list_events(logbook_id).await?;
        let mut projection = ActivationProjection::new();
        projection
            .rebuild(&events)
            .map_err(|error| StoreError::Projection(error.to_string()))?;
        Ok(projection)
    }
}

pub fn validate_supported_remote_event(event: &CoreEventEnvelope) -> Result<(), StoreError> {
    if !event.hash_is_valid() {
        return Err(StoreError::InvalidRemoteHash {
            event_id: event.event_id,
        });
    }

    if event.schema_version != 1 {
        return Err(StoreError::UnsupportedSchemaVersion {
            event_id: event.event_id,
            schema_version: event.schema_version,
        });
    }

    if !matches!(
        event.event_type.as_str(),
        OFFICIAL_LOG_QSO_CREATED
            | OFFICIAL_LOG_QSO_CORRECTED
            | OFFICIAL_LOG_QSO_DELETED
            | OFFICIAL_LOG_QSO_RESTORED
            | OFFICIAL_LOG_QSO_NOTE_ADDED
            | OFFICIAL_LOG_ACTIVATION_CREATED
            | OFFICIAL_LOG_ACTIVATION_UPDATED
            | OFFICIAL_LOG_ACTIVATION_STARTED
            | OFFICIAL_LOG_ACTIVATION_ENDED
            | OFFICIAL_LOG_ACTIVATION_CANCELLED
            | OFFICIAL_LOG_ACTIVATION_NOTE_ADDED
            | OFFICIAL_LOG_QSO_ACTIVATION_LINKED
            | OFFICIAL_LOG_QSO_ACTIVATION_UNLINKED
            | OFFICIAL_LOG_UPLOAD_QUEUED
            | OFFICIAL_LOG_UPLOAD_COMPLETED
            | OFFICIAL_LOG_UPLOAD_FAILED
    ) {
        return Err(StoreError::UnsupportedEventType {
            event_id: event.event_id,
            event_type: event.event_type.clone(),
        });
    }

    Ok(())
}
