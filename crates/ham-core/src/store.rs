use std::collections::HashMap;

use async_trait::async_trait;
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::event::{CoreEventEnvelope, NewLogbookEvent};

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("event not found: {0}")]
    EventNotFound(Uuid),
    #[error("chain verification failed: {0}")]
    ChainVerification(#[from] ChainVerificationError),
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
    async fn get_event(&self, event_id: Uuid) -> Result<Option<CoreEventEnvelope>, StoreError>;
    async fn get_head(&self, logbook_id: Uuid) -> Result<Option<String>, StoreError>;
    async fn list_events_after(
        &self,
        logbook_id: Uuid,
        after_hash: Option<String>,
    ) -> Result<Vec<CoreEventEnvelope>, StoreError>;
    async fn verify_chain(&self, logbook_id: Uuid) -> Result<(), StoreError>;
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

    async fn get_event(&self, event_id: Uuid) -> Result<Option<CoreEventEnvelope>, StoreError> {
        Ok(self.inner.read().await.events_by_id.get(&event_id).cloned())
    }

    async fn get_head(&self, logbook_id: Uuid) -> Result<Option<String>, StoreError> {
        Ok(self.inner.read().await.heads.get(&logbook_id).cloned())
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
}
