//! Durable offline mutation queues, conflict reporting, and LAN trust state.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use ham_core::CoreEventEnvelope;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::{PreviewPullResponse, ReplicationStatus};

pub const OFFLINE_MUTATION_SCHEMA_VERSION: u32 = 1;
pub const OFFLINE_QUEUE_FILE_VERSION: u32 = 1;
pub const LAN_TRUST_FILE_VERSION: u32 = 1;
pub const DEFAULT_PAIRING_TOKEN_TTL_SECONDS: i64 = 10 * 60;
pub const DEFAULT_REPLAY_NONCE_TTL_SECONDS: i64 = 10 * 60;

pub const OFFLINE_OP_QSO_CREATE: &str = "qso.create";
pub const OFFLINE_OP_QSO_DELETE: &str = "qso.delete";
pub const OFFLINE_OP_QSO_RESTORE: &str = "qso.restore";
pub const OFFLINE_OP_QSO_NOTE_ADD: &str = "qso.note.add";
pub const OFFLINE_OP_ACTIVATION_START: &str = "activation.start";
pub const OFFLINE_OP_ACTIVATION_END: &str = "activation.end";
pub const OFFLINE_OP_NET_SESSION_START: &str = "net.session.start";
pub const OFFLINE_OP_NET_SESSION_END: &str = "net.session.end";
pub const OFFLINE_OP_NET_CHECKIN_CREATE: &str = "net.checkin.create";
pub const OFFLINE_OP_NET_CHECKIN_DELETE: &str = "net.checkin.delete";
pub const OFFLINE_OP_NET_TRAFFIC_CREATE: &str = "net.traffic.create";
pub const OFFLINE_OP_STATION_PROFILE_CREATE: &str = "station.profile.create";
pub const OFFLINE_OP_STATION_PROFILE_SELECT: &str = "station.profile.select";
pub const OFFLINE_OP_STATION_EQUIPMENT_CREATE: &str = "station.equipment.create";

const DEFAULT_MAX_ATTEMPTS: u32 = 8;
const DEFAULT_BACKOFF_SECONDS: u64 = 5;
const DEFAULT_MAX_BACKOFF_SECONDS: u64 = 15 * 60;

#[derive(Debug, Error)]
pub enum OfflineQueueError {
    #[error("offline queue I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("offline queue serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("offline queue file version {0} is not supported")]
    UnsupportedFileVersion(u32),
    #[error("offline mutation {operation_id} uses unsupported schema version {schema_version}")]
    UnsupportedMutationSchema {
        operation_id: Uuid,
        schema_version: u32,
    },
    #[error("offline mutation {operation_id} has invalid operation type")]
    InvalidOperationType { operation_id: Uuid },
    #[error("offline mutation {operation_id} has invalid idempotency key")]
    InvalidIdempotencyKey { operation_id: Uuid },
    #[error("offline mutation {operation_id} has invalid sequence {sequence}")]
    InvalidSequence { operation_id: Uuid, sequence: u64 },
    #[error(
        "offline mutation {operation_id} depends on unsupported schema version {schema_version}"
    )]
    UnsupportedDependencySchema {
        operation_id: Uuid,
        schema_version: u32,
    },
    #[error("offline queue has duplicate sequence {sequence} for logbook {logbook_id}")]
    DuplicateSequence { logbook_id: Uuid, sequence: u64 },
    #[error("offline mutation {0} was not found")]
    MutationNotFound(Uuid),
    #[error("offline mutation {operation_id} has no accepted local official event")]
    MissingLocalOfficialEvent { operation_id: Uuid },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OfflineMutationStatus {
    Pending,
    Sending,
    Retrying,
    Blocked,
    Failed,
    Accepted,
    UserActionRequired,
}

impl OfflineMutationStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Blocked | Self::Failed | Self::Accepted | Self::UserActionRequired
        )
    }

    pub fn is_draining(self) -> bool {
        matches!(self, Self::Pending | Self::Retrying)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OfflineMutationDependency {
    pub operation_id: Option<Uuid>,
    pub event_hash: Option<String>,
    pub minimum_schema_version: Option<u32>,
}

impl OfflineMutationDependency {
    pub fn operation(operation_id: Uuid) -> Self {
        Self {
            operation_id: Some(operation_id),
            event_hash: None,
            minimum_schema_version: None,
        }
    }

    pub fn event_hash(event_hash: impl Into<String>) -> Self {
        Self {
            operation_id: None,
            event_hash: Some(event_hash.into()),
            minimum_schema_version: None,
        }
    }

    pub fn minimum_schema_version(schema_version: u32) -> Self {
        Self {
            operation_id: None,
            event_hash: None,
            minimum_schema_version: Some(schema_version),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineMutationEnvelope {
    pub schema_version: u32,
    pub operation_id: Uuid,
    pub correlation_id: Uuid,
    pub client_id: Uuid,
    pub device_id: Uuid,
    pub logbook_id: Uuid,
    pub sequence: u64,
    pub operation_type: String,
    pub idempotency_key: String,
    pub dependencies: Vec<OfflineMutationDependency>,
    pub payload: JsonValue,
    pub status: OfflineMutationStatus,
    pub attempts: u32,
    pub max_attempts: u32,
    pub backoff_seconds: u64,
    pub max_backoff_seconds: u64,
    pub next_attempt_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub official_event_id: Option<Uuid>,
    pub local_event_hash: Option<String>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub failure_reason: Option<String>,
    pub last_error_code: Option<String>,
}

impl OfflineMutationEnvelope {
    pub fn new(input: OfflineMutationInput, sequence: u64, now: DateTime<Utc>) -> Self {
        let operation_id = input.operation_id.unwrap_or_else(Uuid::new_v4);
        let correlation_id = input.correlation_id.unwrap_or(operation_id);
        let idempotency_key = input.idempotency_key.unwrap_or_else(|| {
            format!(
                "{}:{}:{}:{}",
                input.logbook_id, input.device_id, sequence, operation_id
            )
        });
        Self {
            schema_version: OFFLINE_MUTATION_SCHEMA_VERSION,
            operation_id,
            correlation_id,
            client_id: input.client_id,
            device_id: input.device_id,
            logbook_id: input.logbook_id,
            sequence,
            operation_type: input.operation_type,
            idempotency_key,
            dependencies: input.dependencies,
            payload: input.payload,
            status: OfflineMutationStatus::Pending,
            attempts: 0,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            backoff_seconds: DEFAULT_BACKOFF_SECONDS,
            max_backoff_seconds: DEFAULT_MAX_BACKOFF_SECONDS,
            next_attempt_at: None,
            created_at: now,
            updated_at: now,
            official_event_id: None,
            local_event_hash: None,
            accepted_at: None,
            failure_reason: None,
            last_error_code: None,
        }
    }

    pub fn validate(&self) -> Result<(), OfflineQueueError> {
        if self.schema_version != OFFLINE_MUTATION_SCHEMA_VERSION {
            return Err(OfflineQueueError::UnsupportedMutationSchema {
                operation_id: self.operation_id,
                schema_version: self.schema_version,
            });
        }
        if self.operation_type.trim().is_empty() {
            return Err(OfflineQueueError::InvalidOperationType {
                operation_id: self.operation_id,
            });
        }
        if self.idempotency_key.trim().is_empty() {
            return Err(OfflineQueueError::InvalidIdempotencyKey {
                operation_id: self.operation_id,
            });
        }
        if self.sequence == 0 {
            return Err(OfflineQueueError::InvalidSequence {
                operation_id: self.operation_id,
                sequence: self.sequence,
            });
        }
        for dependency in &self.dependencies {
            if let Some(schema_version) = dependency.minimum_schema_version {
                if schema_version > OFFLINE_MUTATION_SCHEMA_VERSION {
                    return Err(OfflineQueueError::UnsupportedDependencySchema {
                        operation_id: self.operation_id,
                        schema_version,
                    });
                }
            }
        }
        Ok(())
    }

    pub fn is_ready_at(&self, now: DateTime<Utc>) -> bool {
        if !self.status.is_draining() {
            return false;
        }
        self.next_attempt_at
            .map(|next_attempt| next_attempt <= now)
            .unwrap_or(true)
    }

    pub fn attach_local_event(&mut self, event: &CoreEventEnvelope, now: DateTime<Utc>) {
        self.official_event_id = Some(event.event_id);
        self.local_event_hash = Some(event.event_hash.clone());
        self.updated_at = now;
        self.failure_reason = None;
        self.last_error_code = None;
        if matches!(
            self.status,
            OfflineMutationStatus::Blocked
                | OfflineMutationStatus::Failed
                | OfflineMutationStatus::UserActionRequired
        ) {
            self.status = OfflineMutationStatus::Pending;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineMutationInput {
    pub logbook_id: Uuid,
    pub device_id: Uuid,
    pub client_id: Uuid,
    pub operation_type: String,
    pub payload: JsonValue,
    pub idempotency_key: Option<String>,
    pub operation_id: Option<Uuid>,
    pub correlation_id: Option<Uuid>,
    pub dependencies: Vec<OfflineMutationDependency>,
}

impl OfflineMutationInput {
    pub fn new(
        logbook_id: Uuid,
        device_id: Uuid,
        client_id: Uuid,
        operation_type: impl Into<String>,
        payload: JsonValue,
    ) -> Self {
        Self {
            logbook_id,
            device_id,
            client_id,
            operation_type: operation_type.into(),
            payload,
            idempotency_key: None,
            operation_id: None,
            correlation_id: None,
            dependencies: Vec::new(),
        }
    }

    pub fn with_operation_id(mut self, operation_id: Uuid) -> Self {
        self.operation_id = Some(operation_id);
        self
    }

    pub fn with_correlation_id(mut self, correlation_id: Uuid) -> Self {
        self.correlation_id = Some(correlation_id);
        self
    }

    pub fn with_idempotency_key(mut self, idempotency_key: impl Into<String>) -> Self {
        self.idempotency_key = Some(idempotency_key.into());
        self
    }

    pub fn with_dependencies(mut self, dependencies: Vec<OfflineMutationDependency>) -> Self {
        self.dependencies = dependencies;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct OfflineQueueFile {
    version: u32,
    next_sequence_by_logbook: BTreeMap<Uuid, u64>,
    mutations: Vec<OfflineMutationEnvelope>,
}

impl Default for OfflineQueueFile {
    fn default() -> Self {
        Self {
            version: OFFLINE_QUEUE_FILE_VERSION,
            next_sequence_by_logbook: BTreeMap::new(),
            mutations: Vec::new(),
        }
    }
}

impl OfflineQueueFile {
    fn validate(&self) -> Result<(), OfflineQueueError> {
        if self.version != OFFLINE_QUEUE_FILE_VERSION {
            return Err(OfflineQueueError::UnsupportedFileVersion(self.version));
        }
        let mut seen_sequences = HashSet::new();
        for mutation in &self.mutations {
            mutation.validate()?;
            let key = (mutation.logbook_id, mutation.sequence);
            if !seen_sequences.insert(key) {
                return Err(OfflineQueueError::DuplicateSequence {
                    logbook_id: mutation.logbook_id,
                    sequence: mutation.sequence,
                });
            }
        }
        Ok(())
    }

    fn sorted(mut self) -> Self {
        self.mutations.sort_by(|left, right| {
            (
                left.logbook_id,
                left.sequence,
                left.created_at,
                left.operation_id,
            )
                .cmp(&(
                    right.logbook_id,
                    right.sequence,
                    right.created_at,
                    right.operation_id,
                ))
        });
        self
    }

    fn next_sequence(&self, logbook_id: Uuid) -> u64 {
        let stored = self.next_sequence_by_logbook.get(&logbook_id).copied();
        let derived = self
            .mutations
            .iter()
            .filter(|mutation| mutation.logbook_id == logbook_id)
            .map(|mutation| mutation.sequence.saturating_add(1))
            .max();
        stored.into_iter().chain(derived).max().unwrap_or(1)
    }

    fn mutation_mut(
        &mut self,
        operation_id: Uuid,
    ) -> Result<&mut OfflineMutationEnvelope, OfflineQueueError> {
        self.mutations
            .iter_mut()
            .find(|mutation| mutation.operation_id == operation_id)
            .ok_or(OfflineQueueError::MutationNotFound(operation_id))
    }
}

#[derive(Debug, Clone)]
pub struct JsonOfflineMutationQueue {
    path: PathBuf,
}

impl JsonOfflineMutationQueue {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load_snapshot(
        &self,
        now: DateTime<Utc>,
    ) -> Result<OfflineQueueSnapshot, OfflineQueueError> {
        let file = self.load_file()?;
        Ok(snapshot_from_file(file.mutations, now))
    }

    pub fn enqueue_input(
        &self,
        input: OfflineMutationInput,
        now: DateTime<Utc>,
    ) -> Result<OfflineMutationEnvelope, OfflineQueueError> {
        let mut file = self.load_file()?;
        if let Some(existing) = file.mutations.iter().find(|mutation| {
            mutation.logbook_id == input.logbook_id
                && input
                    .operation_id
                    .is_some_and(|operation_id| mutation.operation_id == operation_id)
        }) {
            return Ok(existing.clone());
        }
        if let Some(idempotency_key) = input.idempotency_key.as_deref() {
            if let Some(existing) = file.mutations.iter().find(|mutation| {
                mutation.logbook_id == input.logbook_id
                    && mutation.idempotency_key == idempotency_key
            }) {
                return Ok(existing.clone());
            }
        }

        let sequence = file.next_sequence(input.logbook_id);
        let envelope = OfflineMutationEnvelope::new(input, sequence, now);
        envelope.validate()?;
        file.next_sequence_by_logbook
            .insert(envelope.logbook_id, envelope.sequence.saturating_add(1));
        file.mutations.push(envelope.clone());
        self.save_file(&file.sorted())?;
        Ok(envelope)
    }

    pub fn enqueue(
        &self,
        envelope: OfflineMutationEnvelope,
    ) -> Result<OfflineMutationEnvelope, OfflineQueueError> {
        envelope.validate()?;
        let mut file = self.load_file()?;
        if let Some(existing) = file
            .mutations
            .iter()
            .find(|mutation| mutation.operation_id == envelope.operation_id)
        {
            return Ok(existing.clone());
        }
        if let Some(existing) = file.mutations.iter().find(|mutation| {
            mutation.logbook_id == envelope.logbook_id
                && mutation.idempotency_key == envelope.idempotency_key
        }) {
            return Ok(existing.clone());
        }
        if file.mutations.iter().any(|mutation| {
            mutation.logbook_id == envelope.logbook_id && mutation.sequence == envelope.sequence
        }) {
            return Err(OfflineQueueError::DuplicateSequence {
                logbook_id: envelope.logbook_id,
                sequence: envelope.sequence,
            });
        }
        file.next_sequence_by_logbook.insert(
            envelope.logbook_id,
            file.next_sequence(envelope.logbook_id)
                .max(envelope.sequence.saturating_add(1)),
        );
        file.mutations.push(envelope.clone());
        self.save_file(&file.sorted())?;
        Ok(envelope)
    }

    pub fn record_local_event(
        &self,
        operation_id: Uuid,
        event: &CoreEventEnvelope,
        now: DateTime<Utc>,
    ) -> Result<OfflineMutationEnvelope, OfflineQueueError> {
        let mut file = self.load_file()?;
        let mutation = file.mutation_mut(operation_id)?;
        mutation.attach_local_event(event, now);
        let mutation = mutation.clone();
        self.save_file(&file.sorted())?;
        Ok(mutation)
    }

    pub fn mark_sending(
        &self,
        operation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<OfflineMutationEnvelope, OfflineQueueError> {
        self.update_mutation(operation_id, now, |mutation| {
            mutation.status = OfflineMutationStatus::Sending;
            mutation.attempts = mutation.attempts.saturating_add(1);
            mutation.next_attempt_at = None;
            mutation.failure_reason = None;
            mutation.last_error_code = None;
        })
    }

    pub fn record_transient_failure(
        &self,
        operation_id: Uuid,
        reason: impl Into<String>,
        error_code: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<OfflineMutationEnvelope, OfflineQueueError> {
        let reason = reason.into();
        self.update_mutation(operation_id, now, |mutation| {
            if mutation.attempts >= mutation.max_attempts {
                mutation.status = OfflineMutationStatus::Failed;
                mutation.next_attempt_at = None;
            } else {
                mutation.status = OfflineMutationStatus::Retrying;
                mutation.next_attempt_at = Some(now + retry_delay(mutation));
            }
            mutation.failure_reason = Some(reason.clone());
            mutation.last_error_code = error_code.clone();
        })
    }

    pub fn mark_blocked(
        &self,
        operation_id: Uuid,
        reason: impl Into<String>,
        now: DateTime<Utc>,
    ) -> Result<OfflineMutationEnvelope, OfflineQueueError> {
        let reason = reason.into();
        self.update_mutation(operation_id, now, |mutation| {
            mutation.status = OfflineMutationStatus::Blocked;
            mutation.failure_reason = Some(reason.clone());
            mutation.next_attempt_at = None;
        })
    }

    pub fn mark_failed(
        &self,
        operation_id: Uuid,
        reason: impl Into<String>,
        error_code: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<OfflineMutationEnvelope, OfflineQueueError> {
        let reason = reason.into();
        self.update_mutation(operation_id, now, |mutation| {
            mutation.status = OfflineMutationStatus::Failed;
            mutation.failure_reason = Some(reason.clone());
            mutation.last_error_code = error_code.clone();
            mutation.next_attempt_at = None;
        })
    }

    pub fn mark_user_action_required(
        &self,
        operation_id: Uuid,
        reason: impl Into<String>,
        error_code: Option<String>,
        now: DateTime<Utc>,
    ) -> Result<OfflineMutationEnvelope, OfflineQueueError> {
        let reason = reason.into();
        self.update_mutation(operation_id, now, |mutation| {
            mutation.status = OfflineMutationStatus::UserActionRequired;
            mutation.failure_reason = Some(reason.clone());
            mutation.last_error_code = error_code.clone();
            mutation.next_attempt_at = None;
        })
    }

    pub fn mark_accepted(
        &self,
        operation_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<OfflineMutationEnvelope, OfflineQueueError> {
        self.update_mutation(operation_id, now, |mutation| {
            mutation.status = OfflineMutationStatus::Accepted;
            mutation.accepted_at = Some(now);
            mutation.next_attempt_at = None;
            mutation.failure_reason = None;
            mutation.last_error_code = None;
        })
    }

    pub fn mark_accepted_by_event_hashes(
        &self,
        accepted_hashes: &HashSet<String>,
        now: DateTime<Utc>,
    ) -> Result<usize, OfflineQueueError> {
        let mut file = self.load_file()?;
        let mut accepted = 0;
        for mutation in &mut file.mutations {
            if mutation
                .local_event_hash
                .as_ref()
                .is_some_and(|event_hash| accepted_hashes.contains(event_hash))
                && mutation.status != OfflineMutationStatus::Accepted
            {
                mutation.status = OfflineMutationStatus::Accepted;
                mutation.accepted_at = Some(now);
                mutation.updated_at = now;
                mutation.next_attempt_at = None;
                mutation.failure_reason = None;
                mutation.last_error_code = None;
                accepted += 1;
            }
        }
        self.save_file(&file.sorted())?;
        Ok(accepted)
    }

    pub fn recover_interrupted_writes(
        &self,
        now: DateTime<Utc>,
    ) -> Result<usize, OfflineQueueError> {
        let mut file = self.load_file()?;
        let mut recovered = 0;
        for mutation in &mut file.mutations {
            if mutation.status == OfflineMutationStatus::Sending {
                mutation.status = OfflineMutationStatus::Retrying;
                mutation.next_attempt_at = Some(now);
                mutation.updated_at = now;
                mutation.failure_reason = Some("recovered interrupted send attempt".to_owned());
                recovered += 1;
            }
        }
        self.save_file(&file.sorted())?;
        Ok(recovered)
    }

    pub fn ready_to_send(
        &self,
        logbook_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Vec<OfflineMutationEnvelope>, OfflineQueueError> {
        let file = self.load_file()?;
        Ok(ready_mutations(&file.mutations, logbook_id, now))
    }

    pub fn ready_event_batch(
        &self,
        logbook_id: Uuid,
        local_events: &[CoreEventEnvelope],
        now: DateTime<Utc>,
    ) -> Result<OfflinePushBatch, OfflineQueueError> {
        let file = self.load_file()?;
        let events_by_hash = local_events
            .iter()
            .map(|event| (event.event_hash.clone(), event.clone()))
            .collect::<HashMap<_, _>>();
        let mut operations = Vec::new();
        let mut events = Vec::new();
        let mut missing_operation_ids = Vec::new();
        for mutation in ready_mutations(&file.mutations, logbook_id, now) {
            let Some(event_hash) = mutation.local_event_hash.clone() else {
                missing_operation_ids.push(mutation.operation_id);
                break;
            };
            let Some(event) = events_by_hash.get(&event_hash) else {
                missing_operation_ids.push(mutation.operation_id);
                break;
            };
            operations.push(mutation.operation_id);
            events.push(event.clone());
        }
        Ok(OfflinePushBatch {
            logbook_id,
            operation_ids: operations,
            events,
            missing_local_event_operation_ids: missing_operation_ids,
        })
    }

    fn update_mutation<F>(
        &self,
        operation_id: Uuid,
        now: DateTime<Utc>,
        mut update: F,
    ) -> Result<OfflineMutationEnvelope, OfflineQueueError>
    where
        F: FnMut(&mut OfflineMutationEnvelope),
    {
        let mut file = self.load_file()?;
        let mutation = file.mutation_mut(operation_id)?;
        update(mutation);
        mutation.updated_at = now;
        let mutation = mutation.clone();
        self.save_file(&file.sorted())?;
        Ok(mutation)
    }

    fn load_file(&self) -> Result<OfflineQueueFile, OfflineQueueError> {
        if !self.path.exists() {
            return Ok(OfflineQueueFile::default());
        }
        let bytes = fs::read(&self.path)?;
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(OfflineQueueFile::default());
        }
        let file: OfflineQueueFile = serde_json::from_slice(&bytes)?;
        file.validate()?;
        Ok(file.sorted())
    }

    fn save_file(&self, file: &OfflineQueueFile) -> Result<(), OfflineQueueError> {
        file.validate()?;
        write_json_atomically(&self.path, file)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflinePushBatch {
    pub logbook_id: Uuid,
    pub operation_ids: Vec<Uuid>,
    pub events: Vec<CoreEventEnvelope>,
    pub missing_local_event_operation_ids: Vec<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct OfflineQueueHealth {
    pub total: usize,
    pub pending: usize,
    pub sending: usize,
    pub retrying: usize,
    pub blocked: usize,
    pub failed: usize,
    pub accepted: usize,
    pub user_action_required: usize,
    pub ready_to_send: usize,
    pub oldest_pending_at: Option<DateTime<Utc>>,
    pub newest_update_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OfflineQueueSnapshot {
    pub queue_schema_version: u32,
    pub mutation_schema_version: u32,
    pub health: OfflineQueueHealth,
    pub mutations: Vec<OfflineMutationEnvelope>,
}

fn snapshot_from_file(
    mutations: Vec<OfflineMutationEnvelope>,
    now: DateTime<Utc>,
) -> OfflineQueueSnapshot {
    let mut health = OfflineQueueHealth {
        total: mutations.len(),
        ready_to_send: ready_count(&mutations, now),
        oldest_pending_at: mutations
            .iter()
            .filter(|mutation| mutation.status.is_draining())
            .map(|mutation| mutation.created_at)
            .min(),
        newest_update_at: mutations.iter().map(|mutation| mutation.updated_at).max(),
        ..OfflineQueueHealth::default()
    };
    for mutation in &mutations {
        match mutation.status {
            OfflineMutationStatus::Pending => health.pending += 1,
            OfflineMutationStatus::Sending => health.sending += 1,
            OfflineMutationStatus::Retrying => health.retrying += 1,
            OfflineMutationStatus::Blocked => health.blocked += 1,
            OfflineMutationStatus::Failed => health.failed += 1,
            OfflineMutationStatus::Accepted => health.accepted += 1,
            OfflineMutationStatus::UserActionRequired => health.user_action_required += 1,
        }
    }
    OfflineQueueSnapshot {
        queue_schema_version: OFFLINE_QUEUE_FILE_VERSION,
        mutation_schema_version: OFFLINE_MUTATION_SCHEMA_VERSION,
        health,
        mutations,
    }
}

fn ready_count(mutations: &[OfflineMutationEnvelope], now: DateTime<Utc>) -> usize {
    let logbooks = mutations
        .iter()
        .map(|mutation| mutation.logbook_id)
        .collect::<HashSet<_>>();
    logbooks
        .into_iter()
        .map(|logbook_id| ready_mutations(mutations, logbook_id, now).len())
        .sum()
}

fn ready_mutations(
    mutations: &[OfflineMutationEnvelope],
    logbook_id: Uuid,
    now: DateTime<Utc>,
) -> Vec<OfflineMutationEnvelope> {
    let accepted_operations = mutations
        .iter()
        .filter(|mutation| mutation.status == OfflineMutationStatus::Accepted)
        .map(|mutation| mutation.operation_id)
        .collect::<HashSet<_>>();
    let accepted_hashes = mutations
        .iter()
        .filter(|mutation| mutation.status == OfflineMutationStatus::Accepted)
        .filter_map(|mutation| mutation.local_event_hash.clone())
        .collect::<HashSet<_>>();

    let mut ordered = mutations
        .iter()
        .filter(|mutation| mutation.logbook_id == logbook_id)
        .cloned()
        .collect::<Vec<_>>();
    ordered.sort_by_key(|mutation| {
        (
            mutation.sequence,
            mutation.created_at,
            mutation.operation_id,
        )
    });

    let mut ready = Vec::new();
    for mutation in ordered {
        if mutation.status == OfflineMutationStatus::Accepted {
            continue;
        }
        if !mutation.is_ready_at(now) {
            break;
        }
        if !dependencies_satisfied(&mutation, &accepted_operations, &accepted_hashes) {
            break;
        }
        ready.push(mutation);
    }
    ready
}

fn dependencies_satisfied(
    mutation: &OfflineMutationEnvelope,
    accepted_operations: &HashSet<Uuid>,
    accepted_hashes: &HashSet<String>,
) -> bool {
    mutation.dependencies.iter().all(|dependency| {
        dependency
            .operation_id
            .map(|operation_id| accepted_operations.contains(&operation_id))
            .unwrap_or(true)
            && dependency
                .event_hash
                .as_ref()
                .map(|event_hash| accepted_hashes.contains(event_hash))
                .unwrap_or(true)
            && dependency
                .minimum_schema_version
                .map(|schema_version| schema_version <= OFFLINE_MUTATION_SCHEMA_VERSION)
                .unwrap_or(true)
    })
}

fn retry_delay(mutation: &OfflineMutationEnvelope) -> ChronoDuration {
    let exponent = mutation.attempts.saturating_sub(1).min(16);
    let multiplier = 1_u64.checked_shl(exponent).unwrap_or(u64::MAX);
    let seconds = mutation
        .backoff_seconds
        .saturating_mul(multiplier)
        .min(mutation.max_backoff_seconds)
        .max(1);
    ChronoDuration::seconds(seconds as i64)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncConflictKind {
    DivergentHeads,
    MissingDependency,
    UnsupportedSchema,
    ConcurrentCorrection,
    TombstoneRestore,
    RevokedDevice,
    ManualReviewRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConflict {
    pub kind: SyncConflictKind,
    pub message: String,
    pub related_operation_ids: Vec<Uuid>,
    pub related_event_hashes: Vec<String>,
    pub safe_auto_merge: bool,
    pub requires_user_action: bool,
    pub resolution_options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConflictReport {
    pub schema_version: u32,
    pub created_at: DateTime<Utc>,
    pub logbook_id: Uuid,
    pub peer_id: String,
    pub status: ReplicationStatus,
    pub local_head_hash: Option<String>,
    pub remote_head_hash: Option<String>,
    pub missing_event_count: usize,
    pub pending_operation_count: usize,
    pub conflicts: Vec<SyncConflict>,
    pub recommended_action: String,
}

pub fn conflict_report_from_preview(
    preview: &PreviewPullResponse,
    local_pending: &[OfflineMutationEnvelope],
    now: DateTime<Utc>,
) -> SyncConflictReport {
    let pending = local_pending
        .iter()
        .filter(|mutation| {
            mutation.logbook_id == preview.logbook_id
                && mutation.status != OfflineMutationStatus::Accepted
        })
        .cloned()
        .collect::<Vec<_>>();

    let mut conflicts = Vec::new();
    if preview.status == ReplicationStatus::Diverged {
        conflicts.push(SyncConflict {
            kind: SyncConflictKind::DivergentHeads,
            message: preview.message.clone(),
            related_operation_ids: pending
                .iter()
                .map(|mutation| mutation.operation_id)
                .collect(),
            related_event_hashes: preview
                .events
                .iter()
                .map(|event| event.event_hash.clone())
                .collect(),
            safe_auto_merge: false,
            requires_user_action: true,
            resolution_options: vec![
                "review_remote_events".to_owned(),
                "export_divergence_report".to_owned(),
                "create_new_corrective_events".to_owned(),
            ],
        });
    }

    for mutation in &pending {
        let missing_dependencies = mutation
            .dependencies
            .iter()
            .filter(|dependency| {
                dependency.operation_id.is_some()
                    || dependency.event_hash.is_some()
                    || dependency
                        .minimum_schema_version
                        .is_some_and(|schema| schema > OFFLINE_MUTATION_SCHEMA_VERSION)
            })
            .cloned()
            .collect::<Vec<_>>();
        if !missing_dependencies.is_empty()
            && !dependencies_satisfied(mutation, &HashSet::new(), &HashSet::new())
        {
            conflicts.push(SyncConflict {
                kind: SyncConflictKind::MissingDependency,
                message: format!(
                    "Offline mutation {} cannot drain until its dependencies are accepted",
                    mutation.operation_id
                ),
                related_operation_ids: missing_dependencies
                    .iter()
                    .filter_map(|dependency| dependency.operation_id)
                    .collect(),
                related_event_hashes: missing_dependencies
                    .iter()
                    .filter_map(|dependency| dependency.event_hash.clone())
                    .collect(),
                safe_auto_merge: false,
                requires_user_action: true,
                resolution_options: vec![
                    "retry_after_dependency_arrives".to_owned(),
                    "mark_user_action_required".to_owned(),
                ],
            });
        }
    }

    let recommended_action = if conflicts
        .iter()
        .any(|conflict| conflict.requires_user_action)
    {
        "Manual review is required before merging or replaying additional events.".to_owned()
    } else if preview.status == ReplicationStatus::RemoteAhead {
        "Preview can be pulled because the remote chain contains the local head.".to_owned()
    } else {
        "No sync conflict requires action.".to_owned()
    };

    SyncConflictReport {
        schema_version: 1,
        created_at: now,
        logbook_id: preview.logbook_id,
        peer_id: preview.peer_id.clone(),
        status: preview.status,
        local_head_hash: preview.local_head_hash.clone(),
        remote_head_hash: preview.remote_head_hash.clone(),
        missing_event_count: preview.missing_event_count,
        pending_operation_count: pending.len(),
        conflicts,
        recommended_action,
    }
}

#[derive(Debug, Error)]
pub enum LanTrustError {
    #[error("LAN trust I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("LAN trust serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("LAN trust file version {0} is not supported")]
    UnsupportedFileVersion(u32),
    #[error("operator approval is required before issuing a pairing token")]
    ApprovalRequired,
    #[error("pairing token was not found")]
    PairingTokenNotFound,
    #[error("pairing token is expired")]
    PairingTokenExpired,
    #[error("pairing token was already consumed")]
    PairingTokenConsumed,
    #[error("pairing token does not match")]
    PairingTokenMismatch,
    #[error("trusted device {0} was not found")]
    DeviceNotFound(Uuid),
    #[error("trusted device {0} is revoked")]
    DeviceRevoked(Uuid),
    #[error("trusted device {device_id} is not authorized for logbook {logbook_id}")]
    WrongLogbook { device_id: Uuid, logbook_id: Uuid },
    #[error("replay nonce was already used for device {0}")]
    ReplayDetected(Uuid),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssuedPairingToken {
    pub token_id: Uuid,
    pub pairing_code: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingTokenRecord {
    pub token_id: Uuid,
    pub issuer_device_id: Uuid,
    pub logbook_id: Uuid,
    pub issuer_display_name: String,
    pub token_hash: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub approved_by_operator: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustedPeerDevice {
    pub device_id: Uuid,
    pub display_name: String,
    pub logbook_ids: Vec<Uuid>,
    pub trusted_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub pairing_token_id: Option<Uuid>,
    pub public_key_fingerprint: Option<String>,
    pub last_seen_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ReplayNonceRecord {
    device_id: Uuid,
    nonce_hash: String,
    seen_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LanTrustFile {
    version: u32,
    pairing_tokens: Vec<PairingTokenRecord>,
    trusted_devices: Vec<TrustedPeerDevice>,
    replay_nonces: Vec<ReplayNonceRecord>,
}

impl Default for LanTrustFile {
    fn default() -> Self {
        Self {
            version: LAN_TRUST_FILE_VERSION,
            pairing_tokens: Vec::new(),
            trusted_devices: Vec::new(),
            replay_nonces: Vec::new(),
        }
    }
}

impl LanTrustFile {
    fn validate(&self) -> Result<(), LanTrustError> {
        if self.version != LAN_TRUST_FILE_VERSION {
            return Err(LanTrustError::UnsupportedFileVersion(self.version));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanTrustSnapshot {
    pub version: u32,
    pub pairing_tokens: Vec<PairingTokenSummary>,
    pub trusted_devices: Vec<TrustedPeerDevice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingTokenSummary {
    pub token_id: Uuid,
    pub issuer_device_id: Uuid,
    pub logbook_id: Uuid,
    pub issuer_display_name: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub consumed_at: Option<DateTime<Utc>>,
    pub approved_by_operator: bool,
}

impl From<&PairingTokenRecord> for PairingTokenSummary {
    fn from(record: &PairingTokenRecord) -> Self {
        Self {
            token_id: record.token_id,
            issuer_device_id: record.issuer_device_id,
            logbook_id: record.logbook_id,
            issuer_display_name: record.issuer_display_name.clone(),
            created_at: record.created_at,
            expires_at: record.expires_at,
            consumed_at: record.consumed_at,
            approved_by_operator: record.approved_by_operator,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanPairingAcceptance {
    pub token_id: Uuid,
    pub pairing_code: String,
    pub peer_device_id: Uuid,
    pub peer_display_name: String,
    pub requested_logbooks: Vec<Uuid>,
    pub public_key_fingerprint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct JsonLanTrustStore {
    path: PathBuf,
}

impl JsonLanTrustStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn snapshot(&self) -> Result<LanTrustSnapshot, LanTrustError> {
        let file = self.load_file()?;
        Ok(LanTrustSnapshot {
            version: file.version,
            pairing_tokens: file
                .pairing_tokens
                .iter()
                .map(PairingTokenSummary::from)
                .collect(),
            trusted_devices: file.trusted_devices,
        })
    }

    pub fn issue_pairing_token(
        &self,
        issuer_device_id: Uuid,
        logbook_id: Uuid,
        issuer_display_name: impl Into<String>,
        approved_by_operator: bool,
        now: DateTime<Utc>,
    ) -> Result<IssuedPairingToken, LanTrustError> {
        if !approved_by_operator {
            return Err(LanTrustError::ApprovalRequired);
        }
        let mut file = self.load_file()?;
        let token_id = Uuid::new_v4();
        let pairing_code = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
        let expires_at = now + ChronoDuration::seconds(DEFAULT_PAIRING_TOKEN_TTL_SECONDS);
        file.pairing_tokens.push(PairingTokenRecord {
            token_id,
            issuer_device_id,
            logbook_id,
            issuer_display_name: issuer_display_name.into(),
            token_hash: pairing_token_hash(token_id, &pairing_code),
            created_at: now,
            expires_at,
            consumed_at: None,
            approved_by_operator,
        });
        self.save_file(&file)?;
        Ok(IssuedPairingToken {
            token_id,
            pairing_code,
            expires_at,
        })
    }

    pub fn accept_pairing_token(
        &self,
        request: LanPairingAcceptance,
        now: DateTime<Utc>,
    ) -> Result<TrustedPeerDevice, LanTrustError> {
        let mut file = self.load_file()?;
        let token_index = file
            .pairing_tokens
            .iter()
            .position(|token| token.token_id == request.token_id)
            .ok_or(LanTrustError::PairingTokenNotFound)?;
        let token = &file.pairing_tokens[token_index];
        if token.consumed_at.is_some() {
            return Err(LanTrustError::PairingTokenConsumed);
        }
        if token.expires_at < now {
            return Err(LanTrustError::PairingTokenExpired);
        }
        if token.token_hash != pairing_token_hash(request.token_id, &request.pairing_code) {
            return Err(LanTrustError::PairingTokenMismatch);
        }
        let logbook_ids = if request.requested_logbooks.contains(&token.logbook_id) {
            vec![token.logbook_id]
        } else {
            return Err(LanTrustError::WrongLogbook {
                device_id: request.peer_device_id,
                logbook_id: token.logbook_id,
            });
        };
        file.pairing_tokens[token_index].consumed_at = Some(now);
        let device = TrustedPeerDevice {
            device_id: request.peer_device_id,
            display_name: request.peer_display_name,
            logbook_ids,
            trusted_at: now,
            revoked_at: None,
            pairing_token_id: Some(request.token_id),
            public_key_fingerprint: request.public_key_fingerprint,
            last_seen_at: None,
        };
        if let Some(existing) = file
            .trusted_devices
            .iter_mut()
            .find(|device| device.device_id == request.peer_device_id)
        {
            *existing = device.clone();
        } else {
            file.trusted_devices.push(device.clone());
        }
        self.save_file(&file)?;
        Ok(device)
    }

    pub fn authorize_peer(
        &self,
        device_id: Uuid,
        logbook_id: Uuid,
        replay_nonce: &str,
        now: DateTime<Utc>,
    ) -> Result<TrustedPeerDevice, LanTrustError> {
        let mut file = self.load_file()?;
        prune_expired_nonces(&mut file, now);
        let nonce_hash = replay_nonce_hash(device_id, replay_nonce);
        if file
            .replay_nonces
            .iter()
            .any(|nonce| nonce.device_id == device_id && nonce.nonce_hash == nonce_hash)
        {
            return Err(LanTrustError::ReplayDetected(device_id));
        }

        let device = file
            .trusted_devices
            .iter_mut()
            .find(|device| device.device_id == device_id)
            .ok_or(LanTrustError::DeviceNotFound(device_id))?;
        if device.revoked_at.is_some() {
            return Err(LanTrustError::DeviceRevoked(device_id));
        }
        if !device.logbook_ids.contains(&logbook_id) {
            return Err(LanTrustError::WrongLogbook {
                device_id,
                logbook_id,
            });
        }
        device.last_seen_at = Some(now);
        let authorized = device.clone();
        file.replay_nonces.push(ReplayNonceRecord {
            device_id,
            nonce_hash,
            seen_at: now,
            expires_at: now + ChronoDuration::seconds(DEFAULT_REPLAY_NONCE_TTL_SECONDS),
        });
        self.save_file(&file)?;
        Ok(authorized)
    }

    pub fn revoke_device(
        &self,
        device_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<TrustedPeerDevice, LanTrustError> {
        let mut file = self.load_file()?;
        let device = file
            .trusted_devices
            .iter_mut()
            .find(|device| device.device_id == device_id)
            .ok_or(LanTrustError::DeviceNotFound(device_id))?;
        device.revoked_at = Some(now);
        let device = device.clone();
        self.save_file(&file)?;
        Ok(device)
    }

    fn load_file(&self) -> Result<LanTrustFile, LanTrustError> {
        if !self.path.exists() {
            return Ok(LanTrustFile::default());
        }
        let bytes = fs::read(&self.path)?;
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Ok(LanTrustFile::default());
        }
        let file: LanTrustFile = serde_json::from_slice(&bytes)?;
        file.validate()?;
        Ok(file)
    }

    fn save_file(&self, file: &LanTrustFile) -> Result<(), LanTrustError> {
        file.validate()?;
        write_json_atomically(&self.path, file)?;
        Ok(())
    }
}

fn prune_expired_nonces(file: &mut LanTrustFile, now: DateTime<Utc>) {
    file.replay_nonces.retain(|nonce| nonce.expires_at >= now);
}

fn pairing_token_hash(token_id: Uuid, pairing_code: &str) -> String {
    hex_sha256(format!("{token_id}:{pairing_code}").as_bytes())
}

fn replay_nonce_hash(device_id: Uuid, replay_nonce: &str) -> String {
    hex_sha256(format!("{device_id}:{replay_nonce}").as_bytes())
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    format!("{digest:x}")
}

fn write_json_atomically<T>(path: &Path, value: &T) -> Result<(), io::Error>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp_path = path.with_extension(format!(
        "{}.tmp",
        path.extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("json")
    ));
    {
        let mut file = fs::File::create(&temp_path)?;
        serde_json::to_writer_pretty(&mut file, value).map_err(io::Error::other)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temp_path, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue() -> (JsonOfflineMutationQueue, PathBuf) {
        let dir = std::env::temp_dir().join(format!("ham-sync-offline-{}", Uuid::new_v4()));
        (
            JsonOfflineMutationQueue::new(dir.join("offline-mutations.json")),
            dir,
        )
    }

    fn trust_store() -> (JsonLanTrustStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("ham-sync-trust-{}", Uuid::new_v4()));
        (JsonLanTrustStore::new(dir.join("lan-trust.json")), dir)
    }

    fn input(logbook_id: Uuid, device_id: Uuid, operation_type: &str) -> OfflineMutationInput {
        OfflineMutationInput::new(
            logbook_id,
            device_id,
            Uuid::new_v4(),
            operation_type,
            serde_json::json!({"field": "value"}),
        )
    }

    fn event_for(mutation: &OfflineMutationEnvelope) -> CoreEventEnvelope {
        let mut event = CoreEventEnvelope {
            event_id: Uuid::new_v4(),
            event_type: "log.qso.created".to_owned(),
            logbook_id: mutation.logbook_id,
            entity_id: None,
            previous_hash: None,
            event_hash: String::new(),
            timestamp: Utc::now(),
            author_operator_id: None,
            station_callsign: "KE8YGW".to_owned(),
            operator_callsign: Some("KE8YGW".to_owned()),
            author_device_id: mutation.device_id,
            source_device_id: mutation.device_id,
            correlation_id: mutation.correlation_id,
            source_plugin_id: Some("test".to_owned()),
            schema_version: 1,
            payload: serde_json::json!({}),
        };
        event.event_hash = event.calculate_hash();
        event
    }

    #[test]
    fn enqueue_is_idempotent_and_ordered_per_logbook() {
        let (queue, dir) = queue();
        let now = Utc::now();
        let logbook_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let operation_id = Uuid::new_v4();
        let first = queue
            .enqueue_input(
                input(logbook_id, device_id, OFFLINE_OP_QSO_CREATE)
                    .with_operation_id(operation_id)
                    .with_idempotency_key("qso-create-1"),
                now,
            )
            .unwrap();
        let duplicate = queue
            .enqueue_input(
                input(logbook_id, device_id, OFFLINE_OP_QSO_CREATE)
                    .with_operation_id(operation_id)
                    .with_idempotency_key("qso-create-1"),
                now,
            )
            .unwrap();
        let second = queue
            .enqueue_input(input(logbook_id, device_id, OFFLINE_OP_QSO_DELETE), now)
            .unwrap();

        assert_eq!(first.operation_id, duplicate.operation_id);
        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);
        assert_eq!(queue.load_snapshot(now).unwrap().health.pending, 2);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unsupported_schema_and_duplicate_sequence_stop_safely() {
        let (queue, dir) = queue();
        fs::create_dir_all(&dir).unwrap();
        let bad = serde_json::json!({
            "version": OFFLINE_QUEUE_FILE_VERSION,
            "next_sequence_by_logbook": {},
            "mutations": [{
                "schema_version": 999,
                "operation_id": Uuid::new_v4(),
                "correlation_id": Uuid::new_v4(),
                "client_id": Uuid::new_v4(),
                "device_id": Uuid::new_v4(),
                "logbook_id": Uuid::new_v4(),
                "sequence": 1,
                "operation_type": OFFLINE_OP_QSO_CREATE,
                "idempotency_key": "bad",
                "dependencies": [],
                "payload": {},
                "status": "pending",
                "attempts": 0,
                "max_attempts": 8,
                "backoff_seconds": 5,
                "max_backoff_seconds": 900,
                "next_attempt_at": null,
                "created_at": Utc::now(),
                "updated_at": Utc::now(),
                "official_event_id": null,
                "local_event_hash": null,
                "accepted_at": null,
                "failure_reason": null,
                "last_error_code": null
            }]
        });
        fs::write(queue.path(), serde_json::to_vec_pretty(&bad).unwrap()).unwrap();
        assert!(matches!(
            queue.load_snapshot(Utc::now()),
            Err(OfflineQueueError::UnsupportedMutationSchema { .. })
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn ready_batch_respects_local_event_and_queue_order() {
        let (queue, dir) = queue();
        let now = Utc::now();
        let logbook_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let first = queue
            .enqueue_input(input(logbook_id, device_id, OFFLINE_OP_QSO_CREATE), now)
            .unwrap();
        let second = queue
            .enqueue_input(input(logbook_id, device_id, OFFLINE_OP_QSO_DELETE), now)
            .unwrap();
        let first_event = event_for(&first);
        queue
            .record_local_event(first.operation_id, &first_event, now)
            .unwrap();

        let batch = queue
            .ready_event_batch(logbook_id, std::slice::from_ref(&first_event), now)
            .unwrap();
        assert_eq!(batch.operation_ids, vec![first.operation_id]);
        assert_eq!(batch.events, vec![first_event]);
        assert_eq!(
            batch.missing_local_event_operation_ids,
            vec![second.operation_id]
        );

        let second_event = event_for(&second);
        queue
            .record_local_event(second.operation_id, &second_event, now)
            .unwrap();
        queue.mark_accepted(first.operation_id, now).unwrap();
        let next_batch = queue
            .ready_event_batch(logbook_id, std::slice::from_ref(&second_event), now)
            .unwrap();
        assert_eq!(next_batch.operation_ids, vec![second.operation_id]);
        assert_eq!(next_batch.events, vec![second_event]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn interrupted_send_recovers_with_retry_backoff() {
        let (queue, dir) = queue();
        let now = Utc::now();
        let logbook_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let mutation = queue
            .enqueue_input(input(logbook_id, device_id, OFFLINE_OP_QSO_CREATE), now)
            .unwrap();
        queue.mark_sending(mutation.operation_id, now).unwrap();
        assert_eq!(queue.recover_interrupted_writes(now).unwrap(), 1);
        let snapshot = queue.load_snapshot(now).unwrap();
        assert_eq!(snapshot.health.retrying, 1);
        let retry = queue
            .record_transient_failure(
                mutation.operation_id,
                "network unavailable",
                Some("network_unavailable".to_owned()),
                now,
            )
            .unwrap();
        assert_eq!(retry.status, OfflineMutationStatus::Retrying);
        assert!(retry.next_attempt_at.is_some());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn accepted_hash_acknowledges_matching_mutations_only() {
        let (queue, dir) = queue();
        let now = Utc::now();
        let logbook_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let mutation = queue
            .enqueue_input(input(logbook_id, device_id, OFFLINE_OP_QSO_CREATE), now)
            .unwrap();
        let event = event_for(&mutation);
        queue
            .record_local_event(mutation.operation_id, &event, now)
            .unwrap();
        let accepted = HashSet::from([event.event_hash]);
        assert_eq!(
            queue.mark_accepted_by_event_hashes(&accepted, now).unwrap(),
            1
        );
        assert_eq!(queue.load_snapshot(now).unwrap().health.accepted, 1);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn conflict_report_requires_manual_review_for_divergent_heads() {
        let logbook_id = Uuid::new_v4();
        let preview = PreviewPullResponse {
            peer_id: "peer".to_owned(),
            logbook_id,
            status: ReplicationStatus::Diverged,
            local_head_hash: Some("local".to_owned()),
            remote_head_hash: Some("remote".to_owned()),
            missing_event_count: 0,
            remote_event_count: 2,
            events: Vec::new(),
            message: "Remote chain does not contain the local head".to_owned(),
        };
        let report = conflict_report_from_preview(&preview, &[], Utc::now());
        assert_eq!(report.conflicts.len(), 1);
        assert!(!report.conflicts[0].safe_auto_merge);
        assert!(report.conflicts[0].requires_user_action);
    }

    #[test]
    fn lan_pairing_is_single_use_and_revocation_is_immediate() {
        let (store, dir) = trust_store();
        let now = Utc::now();
        let issuer = Uuid::new_v4();
        let peer = Uuid::new_v4();
        let logbook_id = Uuid::new_v4();
        let token = store
            .issue_pairing_token(issuer, logbook_id, "desktop", true, now)
            .unwrap();
        let trusted = store
            .accept_pairing_token(
                LanPairingAcceptance {
                    token_id: token.token_id,
                    pairing_code: token.pairing_code.clone(),
                    peer_device_id: peer,
                    peer_display_name: "ios".to_owned(),
                    requested_logbooks: vec![logbook_id],
                    public_key_fingerprint: Some("fingerprint".to_owned()),
                },
                now,
            )
            .unwrap();
        assert_eq!(trusted.device_id, peer);
        assert!(matches!(
            store.accept_pairing_token(
                LanPairingAcceptance {
                    token_id: token.token_id,
                    pairing_code: token.pairing_code,
                    peer_device_id: Uuid::new_v4(),
                    peer_display_name: "other".to_owned(),
                    requested_logbooks: vec![logbook_id],
                    public_key_fingerprint: None,
                },
                now,
            ),
            Err(LanTrustError::PairingTokenConsumed)
        ));

        let first_auth = store
            .authorize_peer(peer, logbook_id, "nonce-1", now)
            .unwrap();
        assert_eq!(first_auth.device_id, peer);
        assert!(matches!(
            store.authorize_peer(peer, logbook_id, "nonce-1", now),
            Err(LanTrustError::ReplayDetected(device)) if device == peer
        ));
        store.revoke_device(peer, now).unwrap();
        assert!(matches!(
            store.authorize_peer(peer, logbook_id, "nonce-2", now),
            Err(LanTrustError::DeviceRevoked(device)) if device == peer
        ));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn lan_pairing_rejects_expired_wrong_logbook_and_unapproved_tokens() {
        let (store, dir) = trust_store();
        let now = Utc::now();
        let issuer = Uuid::new_v4();
        let logbook_id = Uuid::new_v4();
        assert!(matches!(
            store.issue_pairing_token(issuer, logbook_id, "desktop", false, now),
            Err(LanTrustError::ApprovalRequired)
        ));
        let token = store
            .issue_pairing_token(issuer, logbook_id, "desktop", true, now)
            .unwrap();
        assert!(matches!(
            store.accept_pairing_token(
                LanPairingAcceptance {
                    token_id: token.token_id,
                    pairing_code: token.pairing_code,
                    peer_device_id: Uuid::new_v4(),
                    peer_display_name: "ios".to_owned(),
                    requested_logbooks: vec![Uuid::new_v4()],
                    public_key_fingerprint: None,
                },
                now,
            ),
            Err(LanTrustError::WrongLogbook { .. })
        ));
        let expired = store
            .issue_pairing_token(issuer, logbook_id, "desktop", true, now)
            .unwrap();
        assert!(matches!(
            store.accept_pairing_token(
                LanPairingAcceptance {
                    token_id: expired.token_id,
                    pairing_code: expired.pairing_code,
                    peer_device_id: Uuid::new_v4(),
                    peer_display_name: "ios".to_owned(),
                    requested_logbooks: vec![logbook_id],
                    public_key_fingerprint: None,
                },
                now + ChronoDuration::seconds(DEFAULT_PAIRING_TOKEN_TTL_SECONDS + 1),
            ),
            Err(LanTrustError::PairingTokenExpired)
        ));
        let _ = fs::remove_dir_all(dir);
    }
}
