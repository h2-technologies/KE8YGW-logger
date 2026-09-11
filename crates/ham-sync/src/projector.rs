//! Projects the append-only JSONL official event log into SurrealDB.
//!
//! The JSONL official log is the only source of truth. This module reads it in
//! one direction only — JSONL to SurrealDB — and never writes to the log, never
//! mutates a past entry, and never treats a projected row as authoritative.
//! Every projected table is disposable: a full rebuild replays the log from the
//! first byte and reproduces the projection exactly.
//!
//! Two operating modes:
//!
//! * [`ProjectionMode::FullRebuild`] wipes the projection tables and replays the
//!   whole log. It is only ever triggered explicitly by an operator.
//! * [`ProjectionMode::Incremental`] resumes from the durable checkpoint and
//!   projects newly appended entries. [`SurrealProjector::tail`] polls for them.
//!
//! Every entry is verified against the per-logbook hash chain before it is
//! projected. A broken chain halts the projector at the break: the offending
//! entry and everything after it stay unprojected, the checkpoint records the
//! halt durably, and the caller gets a typed error.

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use chrono::{DateTime, Utc};
use ham_core::{
    projection_touch, ActivationProjection, ChainVerificationError, CoreEventEnvelope, Projection,
    QsoCurrentStateProjection,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use surrealdb::types::Value as SurrealDbValue;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    cloud_store_error, CloudSyncError, SurrealCloudClient, SurrealCloudConfig,
    SurrealCloudMetadataStore,
};

/// Projected QSO rows. One row per QSO id, tombstones included as removals.
pub const PROJECTION_QSO_TABLE: &str = "projection_qso";
/// Projected activation rows, including replay-derived counters.
pub const PROJECTION_ACTIVATION_TABLE: &str = "projection_activation";
/// Durable projector checkpoint. A full rebuild resets it.
pub const PROJECTION_CHECKPOINT_TABLE: &str = "projection_checkpoint";
/// Replay anomalies that are surfaced rather than swallowed.
pub const PROJECTION_ANOMALY_TABLE: &str = "projection_anomaly";
/// Single-writer lease over the projection tables.
pub const PROJECTION_LOCK_TABLE: &str = "projection_writer_lock";
/// Record id shared by the checkpoint and the writer lease.
pub const PROJECTION_STREAM_ID: &str = "official_log";
/// Bumped when the projected row shape changes in a way that needs a rebuild.
pub const PROJECTION_SCHEMA_VERSION: u32 = 1;
/// Events per SurrealDB transaction. See `docs/PROJECTION_PIPELINE.md` for the
/// measurements behind this value.
pub const DEFAULT_PROJECTION_BATCH_SIZE: usize = 1_000;
/// Tail poll interval, matching the polling cadence used elsewhere in the crate.
pub const DEFAULT_PROJECTION_POLL_INTERVAL_SECONDS: u64 = 1;
/// How long a writer lease stays valid without a refresh.
pub const DEFAULT_PROJECTION_LOCK_TTL_SECONDS: i64 = 60;

/// Anomaly kind recorded when a tombstone names an entity that has never been created.
pub const ANOMALY_ORPHAN_TOMBSTONE: &str = "orphan_tombstone";

#[derive(Debug, Error)]
pub enum ProjectorError {
    #[error("projection storage error: {0}")]
    Store(#[from] CloudSyncError),
    #[error("official event log I/O error at byte offset {offset}: {source}")]
    Io {
        offset: u64,
        #[source]
        source: std::io::Error,
    },
    #[error(
        "PROJECTION HALTED: official event log entry {sequence} (byte offset {offset}) is not \
         valid JSON: {message}. Nothing from this entry onward has been projected."
    )]
    MalformedEvent {
        sequence: u64,
        offset: u64,
        message: String,
    },
    #[error(
        "PROJECTION HALTED: official event log hash chain is broken at entry {sequence} (byte \
         offset {offset}): {source}. Nothing from this entry onward has been projected; the \
         official log needs investigation before the projection can advance."
    )]
    BrokenChain {
        sequence: u64,
        offset: u64,
        #[source]
        source: ChainVerificationError,
    },
    #[error(
        "PROJECTION HALTED at entry {sequence}: {reason}. Investigate the official log, then \
         trigger an explicit full rebuild to clear the halt."
    )]
    Halted { sequence: u64, reason: String },
    #[error(
        "PROJECTION HALTED: the official event log no longer matches the checkpoint ({detail}). \
         The log was replaced or rewritten under a live projection; only an explicit full rebuild \
         can recover."
    )]
    LogRewritten { detail: String },
    #[error(
        "another projector holds the projection writer lease (holder {holder}, expires at \
         {expires_at}); only one writer may write the projection tables"
    )]
    WriterLeaseHeld {
        holder: String,
        expires_at: DateTime<Utc>,
    },
    #[error("projection replay failed: {0}")]
    Replay(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionMode {
    /// Wipe the projection and replay the whole log. Operator-triggered only.
    FullRebuild,
    /// Resume from the checkpoint and project newly appended entries.
    Incremental,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionStatus {
    Ok,
    Halted,
}

/// Per-logbook chain head, so a resumed run can keep verifying the chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionHead {
    pub logbook_id: Uuid,
    pub head_hash: String,
}

/// Durable record of how far the projector has verified and projected.
///
/// A full rebuild resets this record, which is why it is safe to keep it in the
/// projection database rather than alongside the official log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionCheckpoint {
    pub stream: String,
    pub schema_version: u32,
    /// Byte offset just past the last fully projected entry.
    pub byte_offset: u64,
    /// Count of entries verified and projected so far (1-based for the last one).
    pub sequence: u64,
    pub heads: Vec<ProjectionHead>,
    pub last_event_id: Option<Uuid>,
    pub last_event_hash: Option<String>,
    pub status: ProjectionStatus,
    pub halt_reason: Option<String>,
    pub halt_sequence: Option<u64>,
    /// Incremented by every full rebuild, so operators can tell replays apart.
    pub rebuild_generation: u64,
    pub updated_at: DateTime<Utc>,
}

impl ProjectionCheckpoint {
    fn empty() -> Self {
        Self {
            stream: PROJECTION_STREAM_ID.to_owned(),
            schema_version: PROJECTION_SCHEMA_VERSION,
            byte_offset: 0,
            sequence: 0,
            heads: Vec::new(),
            last_event_id: None,
            last_event_hash: None,
            status: ProjectionStatus::Ok,
            halt_reason: None,
            halt_sequence: None,
            rebuild_generation: 0,
            updated_at: Utc::now(),
        }
    }

    pub fn is_halted(&self) -> bool {
        self.status == ProjectionStatus::Halted
    }
}

/// What a single projector run did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectionRunReport {
    pub mode: ProjectionMode,
    /// Entries whose hash and chain link were verified this run.
    pub events_verified: u64,
    /// Entries processed and advanced past this run. Rows are written only for
    /// entries a projection consumes; see `unprojected_events` for the rest,
    /// which still advance the checkpoint.
    pub events_projected: u64,
    /// Entries replayed only to rebuild in-memory state, already projected before.
    pub events_rehydrated: u64,
    pub qso_rows_written: u64,
    pub activation_rows_written: u64,
    pub batches_committed: u64,
    pub anomalies_recorded: u64,
    /// Entries whose type no projection consumes; counted rather than hidden.
    pub unprojected_events: u64,
    /// Bytes of a trailing entry that has no terminating newline yet.
    pub partial_trailing_bytes: u64,
    pub bytes_read: u64,
    pub elapsed_ms: u64,
    pub checkpoint: ProjectionCheckpoint,
}

#[derive(Debug, Clone)]
pub struct ProjectorConfig {
    /// The JSONL official event log to read. Never written by the projector.
    pub log_path: PathBuf,
    pub batch_size: usize,
    pub poll_interval: Duration,
    /// Identifies this writer in the single-writer lease.
    pub writer_id: String,
    pub lock_ttl_seconds: i64,
}

impl ProjectorConfig {
    pub fn new(log_path: impl Into<PathBuf>) -> Self {
        Self {
            log_path: log_path.into(),
            batch_size: DEFAULT_PROJECTION_BATCH_SIZE,
            poll_interval: Duration::from_secs(DEFAULT_PROJECTION_POLL_INTERVAL_SECONDS),
            writer_id: format!("ham-sync-projector-{}", Uuid::new_v4()),
            lock_ttl_seconds: DEFAULT_PROJECTION_LOCK_TTL_SECONDS,
        }
    }

    fn effective_batch_size(&self) -> usize {
        self.batch_size.max(1)
    }
}

/// Handle to the SurrealDB instance holding the projection.
///
/// The embedded SurrealKV datastore allows one instance per path, so a projector
/// running inside the sync server shares the server's client rather than opening
/// the same path twice.
#[derive(Debug, Clone)]
pub struct ProjectionStore {
    /// Held behind an `Arc` because `SurrealCloudMetadataStore` closes the
    /// shared client when a handle drops; every holder shares one handle.
    metadata: Arc<SurrealCloudMetadataStore>,
}

impl ProjectionStore {
    /// Opens a standalone projection datastore. Use
    /// [`crate::DurableCloudSyncServer::projector`] instead when the sync server
    /// already holds the datastore open.
    pub fn open(config: SurrealCloudConfig) -> Result<Self, ProjectorError> {
        Ok(Self {
            metadata: Arc::new(SurrealCloudMetadataStore::open(config)?),
        })
    }

    pub(crate) fn from_metadata(metadata: Arc<SurrealCloudMetadataStore>) -> Self {
        Self { metadata }
    }
}

/// Reads the JSONL official log and writes the SurrealDB projection.
#[derive(Debug)]
pub struct SurrealProjector {
    store: ProjectionStore,
    config: ProjectorConfig,
    state: Mutex<ProjectorRuntimeState>,
}

#[derive(Debug)]
struct ProjectorRuntimeState {
    projections: ReplayState,
    checkpoint: ProjectionCheckpoint,
    /// True once `projections` reflects the checkpoint, so later runs can seek
    /// straight to the checkpoint offset instead of replaying from the start.
    hydrated: bool,
}

impl ProjectorRuntimeState {
    fn reset(&mut self) {
        self.projections = ReplayState::default();
        self.hydrated = false;
    }
}

impl SurrealProjector {
    pub fn open(store: ProjectionStore, config: ProjectorConfig) -> Result<Self, ProjectorError> {
        let projector = Self {
            store,
            config,
            state: Mutex::new(ProjectorRuntimeState {
                projections: ReplayState::default(),
                checkpoint: ProjectionCheckpoint::empty(),
                hydrated: false,
            }),
        };
        projector.initialize_schema()?;
        Ok(projector)
    }

    pub fn config(&self) -> &ProjectorConfig {
        &self.config
    }

    /// Reads the durable checkpoint. Returns a zeroed checkpoint if none exists.
    pub fn checkpoint(&self) -> Result<ProjectionCheckpoint, ProjectorError> {
        self.read_checkpoint()
    }

    /// Resumes from the checkpoint and projects everything appended since.
    pub fn project_incremental(&self) -> Result<ProjectionRunReport, ProjectorError> {
        self.project(ProjectionMode::Incremental)
    }

    /// Wipes the projection and replays the whole log.
    ///
    /// This is destructive to the projection (never to the log) and is only
    /// called from an explicit operator action.
    pub fn rebuild(&self) -> Result<ProjectionRunReport, ProjectorError> {
        self.project(ProjectionMode::FullRebuild)
    }

    /// Projects newly appended entries until `shutdown` is set.
    ///
    /// The official log is append-only, so polling its length is enough to know
    /// when there is work; the crate has no filesystem-watch dependency.
    pub fn tail(&self, shutdown: &Arc<AtomicBool>) -> Result<(), ProjectorError> {
        while !shutdown.load(Ordering::Relaxed) {
            self.project_incremental()?;
            let deadline = Instant::now() + self.config.poll_interval;
            while Instant::now() < deadline {
                if shutdown.load(Ordering::Relaxed) {
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(50).min(self.config.poll_interval));
            }
        }
        Ok(())
    }
}

/// In-memory replay state.
///
/// Row content comes from `ham-core`'s projections so the projected copy always
/// matches the canonical replay semantics; this struct only adds the bookkeeping
/// SurrealDB rows need (owning logbook, originating event, chain heads).
#[derive(Debug, Default)]
struct ReplayState {
    qsos: QsoCurrentStateProjection,
    activations: ActivationProjection,
    heads: HashMap<Uuid, String>,
    entities: HashMap<Uuid, EntityMeta>,
}

#[derive(Debug, Clone)]
struct EntityMeta {
    logbook_id: Uuid,
    last_event_id: Uuid,
    last_event_type: String,
    sequence: u64,
}

impl ReplayState {
    /// Verifies one entry against the per-logbook hash chain and advances the head.
    fn verify(&mut self, event: &CoreEventEnvelope) -> Result<(), ChainVerificationError> {
        if !event.hash_is_valid() {
            return Err(ChainVerificationError::InvalidHash {
                event_id: event.event_id,
            });
        }
        let expected_previous_hash = self.heads.get(&event.logbook_id).cloned();
        if event.previous_hash != expected_previous_hash {
            return Err(ChainVerificationError::PreviousHashMismatch {
                event_id: event.event_id,
                expected: expected_previous_hash,
                actual: event.previous_hash.clone(),
            });
        }
        self.heads
            .insert(event.logbook_id, event.event_hash.clone());
        Ok(())
    }

    fn apply(&mut self, event: &CoreEventEnvelope, sequence: u64) -> Result<(), ProjectorError> {
        self.qsos
            .apply(event)
            .map_err(|error| ProjectorError::Replay(error.to_string()))?;
        self.activations
            .apply(event)
            .map_err(|error| ProjectorError::Replay(error.to_string()))?;
        let touch = projection_touch(event);
        if let Some(qso_id) = touch.qso_id {
            self.note_entity(qso_id, event, sequence);
        }
        if let Some(activation_id) = touch.activation_id {
            self.note_entity(activation_id, event, sequence);
        }
        Ok(())
    }

    fn note_entity(&mut self, entity_id: Uuid, event: &CoreEventEnvelope, sequence: u64) {
        self.entities.insert(
            entity_id,
            EntityMeta {
                logbook_id: event.logbook_id,
                last_event_id: event.event_id,
                last_event_type: event.event_type.clone(),
                sequence,
            },
        );
    }

    fn checkpoint_heads(&self) -> Vec<ProjectionHead> {
        let mut heads = self
            .heads
            .iter()
            .map(|(logbook_id, head_hash)| ProjectionHead {
                logbook_id: *logbook_id,
                head_hash: head_hash.clone(),
            })
            .collect::<Vec<_>>();
        heads.sort_by_key(|head| head.logbook_id);
        heads
    }
}

/// Rows dirtied/// Rows dirtied by the entries in the current batch.
#[derive(Debug, Default)]
struct DirtySet {
    qsos: HashSet<Uuid>,
    activations: HashSet<Uuid>,
    anomalies: Vec<ProjectionWrite>,
    events: u64,
}

impl DirtySet {
    fn is_empty(&self) -> bool {
        self.qsos.is_empty() && self.activations.is_empty() && self.anomalies.is_empty()
    }

    fn clear(&mut self) {
        self.qsos.clear();
        self.activations.clear();
        self.anomalies.clear();
        self.events = 0;
    }
}

/// One SurrealDB record write. `data` replaces the record's content.
#[derive(Debug, Clone, Serialize)]
struct ProjectionWrite {
    #[serde(rename = "tb")]
    table: String,
    id: String,
    data: JsonValue,
}

/// Streams the JSONL log line by line while tracking exact byte offsets.
///
/// The log is only ever read here; nothing in this module opens it for writing.
struct EventLogReader {
    reader: BufReader<File>,
    offset: u64,
}

struct RawLine {
    bytes: Vec<u8>,
    start_offset: u64,
    end_offset: u64,
    /// False for a trailing entry with no newline yet — a crash mid-append.
    complete: bool,
}

impl EventLogReader {
    /// Opens the log positioned at `start_offset`. Returns `None` when the log
    /// does not exist yet, which is an empty projection rather than an error.
    fn open(path: &Path, start_offset: u64) -> Result<Option<Self>, ProjectorError> {
        if !path.exists() {
            return Ok(None);
        }
        let file = File::open(path).map_err(|source| ProjectorError::Io {
            offset: start_offset,
            source,
        })?;
        let file_len = file
            .metadata()
            .map_err(|source| ProjectorError::Io {
                offset: start_offset,
                source,
            })?
            .len();
        if file_len < start_offset {
            return Err(ProjectorError::LogRewritten {
                detail: format!(
                    "log is {file_len} bytes but the checkpoint is at byte offset {start_offset}"
                ),
            });
        }
        let mut reader = BufReader::with_capacity(256 * 1024, file);
        if start_offset > 0 {
            reader
                .seek(SeekFrom::Start(start_offset))
                .map_err(|source| ProjectorError::Io {
                    offset: start_offset,
                    source,
                })?;
        }
        Ok(Some(Self {
            reader,
            offset: start_offset,
        }))
    }

    fn next_line(&mut self) -> Result<Option<RawLine>, ProjectorError> {
        let start_offset = self.offset;
        let mut bytes = Vec::new();
        let read = self
            .reader
            .read_until(b'\n', &mut bytes)
            .map_err(|source| ProjectorError::Io {
                offset: start_offset,
                source,
            })?;
        if read == 0 {
            return Ok(None);
        }
        let complete = bytes.last() == Some(&b'\n');
        self.offset += read as u64;
        Ok(Some(RawLine {
            bytes,
            start_offset,
            end_offset: self.offset,
            complete,
        }))
    }
}

impl SurrealProjector {
    fn project(&self, mode: ProjectionMode) -> Result<ProjectionRunReport, ProjectorError> {
        let started = Instant::now();
        let mut guard = self
            .state
            .lock()
            .expect("projector state mutex should not be poisoned");

        self.acquire_writer_lease()?;

        // Where to start reading, and the offset below which entries were
        // already projected by an earlier run.
        let (start_offset, resume_offset) = match mode {
            ProjectionMode::FullRebuild => {
                let previous = self.read_checkpoint()?;
                self.wipe_projection()?;
                guard.reset();
                guard.checkpoint = ProjectionCheckpoint {
                    rebuild_generation: previous.rebuild_generation.saturating_add(1),
                    ..ProjectionCheckpoint::empty()
                };
                (0, 0)
            }
            ProjectionMode::Incremental => {
                if guard.hydrated {
                    let offset = guard.checkpoint.byte_offset;
                    (offset, offset)
                } else {
                    let checkpoint = self.read_checkpoint()?;
                    if checkpoint.is_halted() {
                        return Err(ProjectorError::Halted {
                            sequence: checkpoint.halt_sequence.unwrap_or(checkpoint.sequence),
                            reason: checkpoint
                                .halt_reason
                                .clone()
                                .unwrap_or_else(|| "unknown halt reason".to_owned()),
                        });
                    }
                    let resume_offset = checkpoint.byte_offset;
                    guard.reset();
                    guard.checkpoint = checkpoint;
                    (0, resume_offset)
                }
            }
        };

        let mut report = ProjectionRunReport {
            mode,
            events_verified: 0,
            events_projected: 0,
            events_rehydrated: 0,
            qso_rows_written: 0,
            activation_rows_written: 0,
            batches_committed: 0,
            anomalies_recorded: 0,
            unprojected_events: 0,
            partial_trailing_bytes: 0,
            bytes_read: 0,
            elapsed_ms: 0,
            checkpoint: guard.checkpoint.clone(),
        };

        let Some(mut reader) = EventLogReader::open(&self.config.log_path, start_offset)? else {
            if resume_offset > 0 {
                return Err(ProjectorError::LogRewritten {
                    detail: format!(
                        "the log is gone but the checkpoint is at byte offset {resume_offset}"
                    ),
                });
            }
            report.elapsed_ms = started.elapsed().as_millis() as u64;
            guard.hydrated = true;
            return Ok(report);
        };

        // Everything below `resume_offset` is replayed to rebuild in-memory
        // state only. Projecting it again would be harmless — the writes are
        // idempotent upserts — but it would be wasted work.
        let mut sequence = if start_offset == resume_offset && start_offset > 0 {
            guard.checkpoint.sequence
        } else {
            0
        };
        let mut dirty = DirtySet::default();
        let mut committed_checkpoint = guard.checkpoint.clone();
        let mut pending_checkpoint = guard.checkpoint.clone();
        let batch_size = self.config.effective_batch_size();
        // Proof that the entries already projected are still the entries this
        // log holds: an entry must end exactly at the checkpoint offset and hash
        // to the recorded hash. Without that proof nothing new may be projected.
        let mut checkpoint_boundary_verified = start_offset >= resume_offset;

        loop {
            let line = match reader.next_line() {
                Ok(Some(line)) => line,
                Ok(None) => break,
                Err(error) => {
                    self.halt(&committed_checkpoint, &error.to_string(), sequence)?;
                    return Err(error);
                }
            };
            report.bytes_read += line.end_offset - line.start_offset;

            if !line.complete {
                // A process died mid-append. The bytes are not an entry yet, so
                // stop cleanly and leave the checkpoint before them; the next
                // run picks the entry up once the writer finishes it.
                report.partial_trailing_bytes = line.end_offset - line.start_offset;
                break;
            }

            if line.bytes.iter().all(u8::is_ascii_whitespace) {
                continue;
            }

            sequence += 1;
            let event: CoreEventEnvelope = match serde_json::from_slice(&line.bytes) {
                Ok(event) => event,
                Err(error) => {
                    let error = ProjectorError::MalformedEvent {
                        sequence,
                        offset: line.start_offset,
                        message: error.to_string(),
                    };
                    self.commit_and_halt(
                        &mut guard,
                        &mut dirty,
                        &committed_checkpoint,
                        &pending_checkpoint,
                        &mut report,
                        &error,
                        sequence,
                    )?;
                    return Err(error);
                }
            };

            if let Err(source) = guard.projections.verify(&event) {
                let error = ProjectorError::BrokenChain {
                    sequence,
                    offset: line.start_offset,
                    source,
                };
                self.commit_and_halt(
                    &mut guard,
                    &mut dirty,
                    &committed_checkpoint,
                    &pending_checkpoint,
                    &mut report,
                    &error,
                    sequence,
                )?;
                return Err(error);
            }
            report.events_verified += 1;

            let already_projected = line.end_offset <= resume_offset;
            if !already_projected && !checkpoint_boundary_verified {
                // The log ran past the checkpoint offset without any entry
                // ending on it, so this is not the log the checkpoint describes.
                return Err(ProjectorError::LogRewritten {
                    detail: format!(
                        "no entry ends at the checkpoint offset {resume_offset}; entry {sequence} \
                         spans bytes {}..{}",
                        line.start_offset, line.end_offset
                    ),
                });
            }

            // Anomalies are detected before the event is applied, while the
            // pre-event state still shows whether the target exists.
            let anomaly = if already_projected {
                None
            } else {
                self.detect_anomaly(&guard.projections, &event, sequence)
            };

            guard.projections.apply(&event, sequence)?;

            if already_projected {
                report.events_rehydrated += 1;
                if line.end_offset == resume_offset {
                    self.assert_checkpoint_matches(&guard.checkpoint, &event)?;
                    checkpoint_boundary_verified = true;
                }
                continue;
            }

            if let Some(anomaly) = anomaly {
                dirty.anomalies.push(anomaly);
            }
            self.mark_dirty(&guard.projections, &event, &mut dirty, &mut report);
            dirty.events += 1;
            report.events_projected += 1;

            pending_checkpoint = ProjectionCheckpoint {
                stream: PROJECTION_STREAM_ID.to_owned(),
                schema_version: PROJECTION_SCHEMA_VERSION,
                byte_offset: line.end_offset,
                sequence,
                heads: guard.projections.checkpoint_heads(),
                last_event_id: Some(event.event_id),
                last_event_hash: Some(event.event_hash.clone()),
                status: ProjectionStatus::Ok,
                halt_reason: None,
                halt_sequence: None,
                rebuild_generation: guard.checkpoint.rebuild_generation,
                updated_at: Utc::now(),
            };

            if dirty.events >= batch_size as u64 {
                self.commit_batch(
                    &guard.projections,
                    &mut dirty,
                    &pending_checkpoint,
                    &mut report,
                )?;
                committed_checkpoint = pending_checkpoint.clone();
                guard.checkpoint = committed_checkpoint.clone();
            }
        }

        if !checkpoint_boundary_verified {
            return Err(ProjectorError::LogRewritten {
                detail: format!(
                    "the log ended before the checkpoint offset {resume_offset}; \
                     {} entries were replayed",
                    report.events_verified
                ),
            });
        }

        if !dirty.is_empty() || dirty.events > 0 {
            self.commit_batch(
                &guard.projections,
                &mut dirty,
                &pending_checkpoint,
                &mut report,
            )?;
            guard.checkpoint = pending_checkpoint.clone();
        }

        guard.hydrated = true;
        report.checkpoint = guard.checkpoint.clone();
        report.elapsed_ms = started.elapsed().as_millis() as u64;
        Ok(report)
    }

    /// Commits the valid entries seen before a halt, then records the halt.
    ///
    /// Entries before the break are legitimate projected state; the offending
    /// entry and everything after it are left unprojected.
    #[allow(clippy::too_many_arguments)]
    fn commit_and_halt(
        &self,
        guard: &mut ProjectorRuntimeState,
        dirty: &mut DirtySet,
        committed_checkpoint: &ProjectionCheckpoint,
        pending_checkpoint: &ProjectionCheckpoint,
        report: &mut ProjectionRunReport,
        error: &ProjectorError,
        sequence: u64,
    ) -> Result<(), ProjectorError> {
        let last_good = if dirty.events > 0 {
            self.commit_batch(&guard.projections, dirty, pending_checkpoint, report)?;
            guard.checkpoint = pending_checkpoint.clone();
            pending_checkpoint
        } else {
            committed_checkpoint
        };
        self.halt(last_good, &error.to_string(), sequence)?;
        // The in-memory state ran past the last committed entry, so force the
        // next run to rebuild it from the log rather than trust it.
        guard.reset();
        report.checkpoint = self.read_checkpoint()?;
        Ok(())
    }

    /// Confirms the replayed prefix is still the log the checkpoint was taken from.
    fn assert_checkpoint_matches(
        &self,
        checkpoint: &ProjectionCheckpoint,
        event: &CoreEventEnvelope,
    ) -> Result<(), ProjectorError> {
        let Some(expected_hash) = checkpoint.last_event_hash.as_deref() else {
            return Ok(());
        };
        if expected_hash != event.event_hash {
            return Err(ProjectorError::LogRewritten {
                detail: format!(
                    "the entry at the checkpoint offset hashes to {} but the checkpoint recorded {expected_hash}",
                    event.event_hash
                ),
            });
        }
        Ok(())
    }

    /// A tombstone naming an entity that was never created is recorded, not swallowed.
    fn detect_anomaly(
        &self,
        state: &ReplayState,
        event: &CoreEventEnvelope,
        sequence: u64,
    ) -> Option<ProjectionWrite> {
        let touch = projection_touch(event);
        if !touch.tombstone {
            return None;
        }
        let entity_id = touch.qso_id?;
        if state.qsos.get_including_deleted(entity_id).is_some() {
            return None;
        }
        Some(ProjectionWrite {
            table: PROJECTION_ANOMALY_TABLE.to_owned(),
            // Deterministic id: re-projecting the same entry updates the same row.
            id: format!("{ANOMALY_ORPHAN_TOMBSTONE}-{sequence}"),
            data: json!({
                "kind": ANOMALY_ORPHAN_TOMBSTONE,
                "sequence": sequence,
                "event_id": event.event_id,
                "event_type": event.event_type,
                "entity_id": entity_id,
                "logbook_id": event.logbook_id,
                "detail": "tombstone applied to an entity that has no prior create event",
                "detected_at": Utc::now(),
            }),
        })
    }

    fn mark_dirty(
        &self,
        state: &ReplayState,
        event: &CoreEventEnvelope,
        dirty: &mut DirtySet,
        report: &mut ProjectionRunReport,
    ) {
        let touch = projection_touch(event);
        if !touch.consumed {
            // Net Control and upload events have their own projections; count
            // them so an unprojected event type is visible rather than silent.
            report.unprojected_events += 1;
            return;
        }
        if let Some(qso_id) = touch.qso_id {
            dirty.qsos.insert(qso_id);
            // A QSO event also changes the counters of every activation that
            // links it.
            for activation_id in state.activations.activations_for_qso(qso_id) {
                dirty.activations.insert(activation_id);
            }
        }
        if let Some(activation_id) = touch.activation_id {
            dirty.activations.insert(activation_id);
        }
    }
}

impl SurrealProjector {
    /// Builds the SurrealDB rows for the entities the batch dirtied.
    fn batch_writes(&self, state: &ReplayState, dirty: &DirtySet) -> Vec<ProjectionWrite> {
        let mut writes = Vec::with_capacity(dirty.qsos.len() + dirty.activations.len() + 1);
        let projected_at = Utc::now();

        for qso_id in &dirty.qsos {
            let Some(record) = state.qsos.get_including_deleted(*qso_id) else {
                continue;
            };
            let meta = state.entities.get(qso_id);
            let activation_ids = state.activations.activations_for_qso(*qso_id);
            writes.push(ProjectionWrite {
                table: PROJECTION_QSO_TABLE.to_owned(),
                id: qso_id.to_string(),
                data: json!({
                    "qso_id": record.qso_id,
                    "logbook_id": meta.map(|meta| meta.logbook_id),
                    "payload": record.payload,
                    "note_history": record.note_history,
                    // Tombstones project as a removal flag, never a row delete:
                    // the projected copy must never look like deleted history.
                    "removed": record.deleted,
                    "activation_ids": activation_ids,
                    "contacted_callsign": payload_string(&record.payload, "contacted_callsign"),
                    "station_callsign": payload_string(&record.payload, "station_callsign"),
                    "operator_callsign": payload_string(&record.payload, "operator_callsign"),
                    "mode": payload_string(&record.payload, "mode"),
                    "band": payload_string(&record.payload, "band"),
                    "started_at": payload_string(&record.payload, "started_at"),
                    "last_event_hash": record.last_event_hash,
                    "last_event_id": meta.map(|meta| meta.last_event_id),
                    "last_event_type": meta.map(|meta| meta.last_event_type.clone()),
                    "sequence": meta.map(|meta| meta.sequence),
                    "projected_at": projected_at,
                }),
            });
        }

        for activation_id in &dirty.activations {
            let Some(record) = state.activations.get(*activation_id) else {
                continue;
            };
            let meta = state.entities.get(activation_id);
            let mut linked_qso_ids = record.linked_qsos.iter().copied().collect::<Vec<_>>();
            linked_qso_ids.sort();
            writes.push(ProjectionWrite {
                table: PROJECTION_ACTIVATION_TABLE.to_owned(),
                id: activation_id.to_string(),
                data: json!({
                    "activation_id": record.activation_id,
                    "logbook_id": meta.map(|meta| meta.logbook_id),
                    "payload": record.payload,
                    "status": record.status,
                    "note_history": record.note_history,
                    "linked_qso_ids": linked_qso_ids,
                    "qso_count": record.qso_count,
                    "unique_callsign_count": record.unique_callsign_count,
                    "band_summary": record.band_summary,
                    "mode_summary": record.mode_summary,
                    "last_event_hash": record.last_event_hash,
                    "last_event_id": meta.map(|meta| meta.last_event_id),
                    "last_event_type": meta.map(|meta| meta.last_event_type.clone()),
                    "sequence": meta.map(|meta| meta.sequence),
                    "projected_at": projected_at,
                }),
            });
        }

        writes.extend(dirty.anomalies.iter().cloned());
        writes
    }

    /// Writes one batch and its checkpoint in a single SurrealDB transaction.
    ///
    /// Committing the rows and the checkpoint together is what makes a restart
    /// safe: either both land or neither does, so a resumed run never skips an
    /// entry and never double-counts one (every write is an idempotent upsert
    /// keyed by entity id).
    fn commit_batch(
        &self,
        state: &ReplayState,
        dirty: &mut DirtySet,
        checkpoint: &ProjectionCheckpoint,
        report: &mut ProjectionRunReport,
    ) -> Result<(), ProjectorError> {
        let writes = self.batch_writes(state, dirty);
        let qso_rows = writes
            .iter()
            .filter(|write| write.table == PROJECTION_QSO_TABLE)
            .count() as u64;
        let activation_rows = writes
            .iter()
            .filter(|write| write.table == PROJECTION_ACTIVATION_TABLE)
            .count() as u64;
        let anomaly_rows = writes
            .iter()
            .filter(|write| write.table == PROJECTION_ANOMALY_TABLE)
            .count() as u64;

        let write_values = serde_json::to_value(&writes).map_err(cloud_store_error)?;
        let checkpoint_value = serde_json::to_value(checkpoint).map_err(cloud_store_error)?;
        let lease_value = self.lease_value();

        self.store.metadata.run(move |client| async move {
            let query = "\
                BEGIN TRANSACTION;\
                FOR $write IN $writes { UPSERT type::record($write.tb, $write.id) CONTENT $write.data; };\
                UPSERT type::record($lock_table, $stream) CONTENT $lease;\
                UPSERT type::record($checkpoint_table, $stream) CONTENT $checkpoint;\
                COMMIT TRANSACTION;";
            match client {
                SurrealCloudClient::Local(db) => db
                    .query(query)
                    .bind(("writes", write_values.clone()))
                    .bind(("lock_table", PROJECTION_LOCK_TABLE))
                    .bind(("checkpoint_table", PROJECTION_CHECKPOINT_TABLE))
                    .bind(("stream", PROJECTION_STREAM_ID))
                    .bind(("lease", lease_value))
                    .bind(("checkpoint", checkpoint_value))
                    .await
                    .map_err(cloud_store_error)?
                    .check()
                    .map_err(cloud_store_error)?,
                SurrealCloudClient::Remote(db) => db
                    .query(query)
                    .bind(("writes", write_values.clone()))
                    .bind(("lock_table", PROJECTION_LOCK_TABLE))
                    .bind(("checkpoint_table", PROJECTION_CHECKPOINT_TABLE))
                    .bind(("stream", PROJECTION_STREAM_ID))
                    .bind(("lease", lease_value))
                    .bind(("checkpoint", checkpoint_value))
                    .await
                    .map_err(cloud_store_error)?
                    .check()
                    .map_err(cloud_store_error)?,
            };
            Ok(())
        })?;

        report.qso_rows_written += qso_rows;
        report.activation_rows_written += activation_rows;
        report.anomalies_recorded += anomaly_rows;
        report.batches_committed += 1;
        dirty.clear();
        Ok(())
    }

    /// Records a halt durably so it cannot scroll past unnoticed.
    ///
    /// The checkpoint keeps the last good offset, so a repaired log resumes from
    /// the right place; the halt itself blocks incremental runs until an
    /// operator clears it with an explicit full rebuild.
    fn halt(
        &self,
        checkpoint: &ProjectionCheckpoint,
        reason: &str,
        sequence: u64,
    ) -> Result<(), ProjectorError> {
        let halted = ProjectionCheckpoint {
            status: ProjectionStatus::Halted,
            halt_reason: Some(reason.to_owned()),
            halt_sequence: Some(sequence),
            updated_at: Utc::now(),
            ..checkpoint.clone()
        };
        let value = serde_json::to_value(&halted).map_err(cloud_store_error)?;
        self.store.metadata.run(move |client| async move {
            upsert_projection_record(
                &client,
                PROJECTION_CHECKPOINT_TABLE,
                PROJECTION_STREAM_ID,
                value,
            )
            .await
        })?;
        eprintln!(
            "ham-sync projector HALTED at official event log entry {sequence}: {reason}\n\
             The SurrealDB projection is frozen at the last verified entry and will not advance. \
             Nothing was projected from the failing entry onward."
        );
        Ok(())
    }

    fn read_checkpoint(&self) -> Result<ProjectionCheckpoint, ProjectorError> {
        let row = self.store.metadata.run(move |client| async move {
            select_projection_record(&client, PROJECTION_CHECKPOINT_TABLE, PROJECTION_STREAM_ID)
                .await
        })?;
        let Some(row) = row else {
            return Ok(ProjectionCheckpoint::empty());
        };
        serde_json::from_value(row).map_err(|error| ProjectorError::Store(cloud_store_error(error)))
    }

    /// Removes every projected row. Only a full rebuild calls this.
    fn wipe_projection(&self) -> Result<(), ProjectorError> {
        self.store.metadata.run(move |client| async move {
            let query = "\
                BEGIN TRANSACTION;\
                DELETE projection_qso;\
                DELETE projection_activation;\
                DELETE projection_anomaly;\
                DELETE projection_checkpoint;\
                COMMIT TRANSACTION;";
            query_projection_checked(&client, query).await
        })?;
        Ok(())
    }

    fn initialize_schema(&self) -> Result<(), ProjectorError> {
        self.store.metadata.run(move |client| async move {
            let schema = r#"
                DEFINE TABLE IF NOT EXISTS projection_qso SCHEMALESS;
                DEFINE TABLE IF NOT EXISTS projection_activation SCHEMALESS;
                DEFINE TABLE IF NOT EXISTS projection_checkpoint SCHEMALESS;
                DEFINE TABLE IF NOT EXISTS projection_anomaly SCHEMALESS;
                DEFINE TABLE IF NOT EXISTS projection_writer_lock SCHEMALESS;
                DEFINE INDEX IF NOT EXISTS projection_qso_logbook_idx ON TABLE projection_qso COLUMNS logbook_id;
                DEFINE INDEX IF NOT EXISTS projection_qso_removed_idx ON TABLE projection_qso COLUMNS removed;
                DEFINE INDEX IF NOT EXISTS projection_qso_callsign_idx ON TABLE projection_qso COLUMNS contacted_callsign;
                DEFINE INDEX IF NOT EXISTS projection_qso_started_idx ON TABLE projection_qso COLUMNS started_at;
                DEFINE INDEX IF NOT EXISTS projection_activation_logbook_idx ON TABLE projection_activation COLUMNS logbook_id;
                DEFINE INDEX IF NOT EXISTS projection_activation_status_idx ON TABLE projection_activation COLUMNS status;
                DEFINE INDEX IF NOT EXISTS projection_anomaly_kind_idx ON TABLE projection_anomaly COLUMNS kind;
                UPSERT schema_migrations:projection_v1 SET version = 1, component = 'ham-sync-projector', applied_at = time::now();
            "#;
            query_projection_checked(&client, schema).await
        })?;
        Ok(())
    }

    fn lease_value(&self) -> JsonValue {
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(self.config.lock_ttl_seconds.max(1));
        json!({
            "stream": PROJECTION_STREAM_ID,
            "holder": self.config.writer_id,
            "acquired_at": now,
            "expires_at": expires_at,
            "expires_at_ms": expires_at.timestamp_millis(),
        })
    }

    /// Takes the single-writer lease over the projection tables.
    ///
    /// Readers (GUI, sync, plugins) never take it; it exists so a second
    /// projector cannot start writing the same tables behind the first one's
    /// back. The conditional upsert and the check that decides the winner run
    /// in one transaction, so two projectors starting together cannot both win.
    fn acquire_writer_lease(&self) -> Result<(), ProjectorError> {
        let lease = self.lease_value();
        let holder = self.config.writer_id.clone();
        let now_ms = Utc::now().timestamp_millis();
        self.store.metadata.run(move |client| async move {
            let query = "\
                BEGIN TRANSACTION;\
                LET $existing = (SELECT * FROM type::record($lock_table, $stream))[0];\
                IF $existing = NONE OR $existing.holder = $holder OR $existing.expires_at_ms <= $now_ms {\
                    UPSERT type::record($lock_table, $stream) CONTENT $lease;\
                };\
                COMMIT TRANSACTION;";
            match client {
                SurrealCloudClient::Local(db) => db
                    .query(query)
                    .bind(("lock_table", PROJECTION_LOCK_TABLE))
                    .bind(("stream", PROJECTION_STREAM_ID))
                    .bind(("holder", holder))
                    .bind(("now_ms", now_ms))
                    .bind(("lease", lease))
                    .await
                    .map_err(cloud_store_error)?
                    .check()
                    .map_err(cloud_store_error)?,
                SurrealCloudClient::Remote(db) => db
                    .query(query)
                    .bind(("lock_table", PROJECTION_LOCK_TABLE))
                    .bind(("stream", PROJECTION_STREAM_ID))
                    .bind(("holder", holder))
                    .bind(("now_ms", now_ms))
                    .bind(("lease", lease))
                    .await
                    .map_err(cloud_store_error)?
                    .check()
                    .map_err(cloud_store_error)?,
            };
            Ok(())
        })?;

        // The conditional upsert only wrote if this projector may hold the
        // lease, so reading it back is what says whether this run won it.
        let held = self.read_lease()?;
        match held {
            Some(lease) if lease.holder == self.config.writer_id => Ok(()),
            Some(lease) => Err(ProjectorError::WriterLeaseHeld {
                holder: lease.holder,
                expires_at: lease.expires_at,
            }),
            None => Err(ProjectorError::WriterLeaseHeld {
                holder: "unknown".to_owned(),
                expires_at: Utc::now(),
            }),
        }
    }

    fn read_lease(&self) -> Result<Option<ProjectionWriterLease>, ProjectorError> {
        let row = self.store.metadata.run(move |client| async move {
            select_projection_record(&client, PROJECTION_LOCK_TABLE, PROJECTION_STREAM_ID).await
        })?;
        let Some(row) = row else {
            return Ok(None);
        };
        serde_json::from_value(row)
            .map(Some)
            .map_err(|error| ProjectorError::Store(cloud_store_error(error)))
    }
}

/// The single-writer lease record, as stored in SurrealDB.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectionWriterLease {
    pub stream: String,
    pub holder: String,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    /// Millisecond epoch of `expires_at`, so expiry compares numerically in SurrealQL.
    pub expires_at_ms: i64,
}

fn payload_string(payload: &JsonValue, field: &str) -> Option<String> {
    payload
        .get(field)
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned)
}

async fn query_projection_checked(
    client: &SurrealCloudClient,
    query: &str,
) -> Result<(), CloudSyncError> {
    match client {
        SurrealCloudClient::Local(db) => {
            db.query(query)
                .await
                .map_err(cloud_store_error)?
                .check()
                .map_err(cloud_store_error)?;
        }
        SurrealCloudClient::Remote(db) => {
            db.query(query)
                .await
                .map_err(cloud_store_error)?
                .check()
                .map_err(cloud_store_error)?;
        }
    }
    Ok(())
}

async fn upsert_projection_record(
    client: &SurrealCloudClient,
    table: &'static str,
    id: &'static str,
    content: JsonValue,
) -> Result<(), CloudSyncError> {
    match client {
        SurrealCloudClient::Local(db) => {
            let _: Option<SurrealDbValue> = db
                .upsert((table, id))
                .content(content)
                .await
                .map_err(cloud_store_error)?;
        }
        SurrealCloudClient::Remote(db) => {
            let _: Option<SurrealDbValue> = db
                .upsert((table, id))
                .content(content)
                .await
                .map_err(cloud_store_error)?;
        }
    }
    Ok(())
}

async fn select_projection_record(
    client: &SurrealCloudClient,
    table: &'static str,
    id: &'static str,
) -> Result<Option<JsonValue>, CloudSyncError> {
    let record: Option<SurrealDbValue> = match client {
        SurrealCloudClient::Local(db) => db.select((table, id)).await.map_err(cloud_store_error)?,
        SurrealCloudClient::Remote(db) => {
            db.select((table, id)).await.map_err(cloud_store_error)?
        }
    };
    Ok(record.map(SurrealDbValue::into_json_value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ham_core::{JsonlLogbookEventStore, LogbookEventStore, NewLogbookEvent};
    use ham_plugin_sdk::{
        OFFICIAL_LOG_ACTIVATION_STARTED, OFFICIAL_LOG_QSO_ACTIVATION_LINKED,
        OFFICIAL_LOG_QSO_CORRECTED, OFFICIAL_LOG_QSO_CREATED, OFFICIAL_LOG_QSO_DELETED,
        OFFICIAL_LOG_QSO_NOTE_ADDED, OFFICIAL_LOG_QSO_RESTORED,
    };
    use std::fs::{self, OpenOptions};
    use std::io::Write;

    struct TestLog {
        root: PathBuf,
        path: PathBuf,
        logbook_id: Uuid,
        device_id: Uuid,
        previous_hash: Option<String>,
        /// Opened once: the embedded SurrealKV datastore allows a single
        /// instance per path, and a real restart keeps the datastore anyway.
        store: ProjectionStore,
        /// Stable across simulated restarts, like a redeployed projector.
        writer_id: String,
    }

    impl TestLog {
        fn new(label: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("ke8ygw-projector-{label}-{}", Uuid::new_v4()));
            fs::create_dir_all(&root).expect("test root");
            let store = ProjectionStore::open(SurrealCloudConfig::local(root.join("surrealdb")))
                .expect("projection store");
            Self {
                path: root.join("official-events.jsonl"),
                root,
                logbook_id: Uuid::new_v4(),
                device_id: Uuid::new_v4(),
                previous_hash: None,
                store,
                writer_id: format!("test-projector-{label}"),
            }
        }

        /// Builds a projector over this fixture's datastore.
        ///
        /// Calling it again models a projector restart: the SurrealDB contents
        /// survive the restart, the projector's in-memory replay state does not.
        /// The writer id is stable, as it would be for a redeployed projector,
        /// so the restart reclaims its own lease instead of waiting it out.
        fn projector(&self) -> SurrealProjector {
            let mut config = ProjectorConfig::new(&self.path);
            config.batch_size = 4;
            config.poll_interval = Duration::from_millis(20);
            config.writer_id = self.writer_id.clone();
            SurrealProjector::open(self.store.clone(), config).expect("projector opens")
        }

        fn event(
            &mut self,
            event_type: &str,
            entity_id: Uuid,
            payload: JsonValue,
        ) -> CoreEventEnvelope {
            let event = CoreEventEnvelope::from_new(
                NewLogbookEvent {
                    event_type: event_type.to_owned(),
                    logbook_id: self.logbook_id,
                    entity_id: Some(entity_id),
                    author_operator_id: None,
                    station_callsign: "KE8YGW".to_owned(),
                    operator_callsign: Some("KE8YGW".to_owned()),
                    author_device_id: self.device_id,
                    source_device_id: self.device_id,
                    correlation_id: Uuid::new_v4(),
                    source_plugin_id: None,
                    schema_version: 1,
                    payload,
                },
                self.previous_hash.clone(),
            );
            self.previous_hash = Some(event.event_hash.clone());
            event
        }

        fn append(
            &mut self,
            event_type: &str,
            entity_id: Uuid,
            payload: JsonValue,
        ) -> CoreEventEnvelope {
            let event = self.event(event_type, entity_id, payload);
            self.append_event(&event);
            event
        }

        fn append_event(&mut self, event: &CoreEventEnvelope) {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .expect("open log");
            file.write_all(serde_json::to_string(event).expect("serialize").as_bytes())
                .expect("write event");
            file.write_all(b"\n").expect("write newline");
        }

        fn append_raw(&mut self, raw: &str) {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)
                .expect("open log");
            file.write_all(raw.as_bytes()).expect("write raw");
        }

        fn create_qso(&mut self, callsign: &str) -> Uuid {
            let qso_id = Uuid::new_v4();
            self.append(
                OFFICIAL_LOG_QSO_CREATED,
                qso_id,
                json!({
                    "contacted_callsign": callsign,
                    "station_callsign": "KE8YGW",
                    "mode": "SSB",
                    "band": "20m",
                    "started_at": "2026-07-06T00:00:00Z",
                }),
            );
            qso_id
        }
    }

    impl Drop for TestLog {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    /// Reads one projected row back out of SurrealDB.
    fn read_row(
        projector: &SurrealProjector,
        table: &'static str,
        id: String,
    ) -> Option<JsonValue> {
        projector
            .store
            .metadata
            .run(move |client| async move {
                let record: Option<SurrealDbValue> = match client {
                    SurrealCloudClient::Local(db) => db
                        .select((table, id.as_str()))
                        .await
                        .map_err(cloud_store_error)?,
                    SurrealCloudClient::Remote(db) => db
                        .select((table, id.as_str()))
                        .await
                        .map_err(cloud_store_error)?,
                };
                Ok(record.map(SurrealDbValue::into_json_value))
            })
            .expect("select row")
    }

    fn count_rows(projector: &SurrealProjector, table: &'static str) -> usize {
        projector
            .store
            .metadata
            .run(move |client| async move {
                let query = format!("SELECT * FROM {table};");
                let rows: Vec<SurrealDbValue> = match client {
                    SurrealCloudClient::Local(db) => {
                        let mut response =
                            db.query(query.as_str()).await.map_err(cloud_store_error)?;
                        response.take(0).map_err(cloud_store_error)?
                    }
                    SurrealCloudClient::Remote(db) => {
                        let mut response =
                            db.query(query.as_str()).await.map_err(cloud_store_error)?;
                        response.take(0).map_err(cloud_store_error)?
                    }
                };
                Ok(rows.len())
            })
            .expect("count rows")
    }

    /// Round trip: build a log fixture, project it, and check the projected
    /// state matches what replaying the log through `ham-core` produces.
    #[test]
    fn projects_log_fixture_and_matches_core_replay() {
        let mut log = TestLog::new("roundtrip");
        let activation_id = Uuid::new_v4();
        log.append(
            OFFICIAL_LOG_ACTIVATION_STARTED,
            activation_id,
            json!({
                "activation_type": "pota",
                "park_id": "US-1234",
                "station_callsign": "KE8YGW",
                "operator_callsign": "KE8YGW",
                "started_at": "2026-07-06T00:00:00Z",
            }),
        );
        let first = log.create_qso("W1AW");
        let second = log.create_qso("VE3ABC");
        let third = log.create_qso("G0ABC");
        log.append(
            OFFICIAL_LOG_QSO_ACTIVATION_LINKED,
            first,
            json!({ "activation_id": activation_id.to_string() }),
        );
        log.append(
            OFFICIAL_LOG_QSO_ACTIVATION_LINKED,
            second,
            json!({ "activation_id": activation_id.to_string() }),
        );
        log.append(
            OFFICIAL_LOG_QSO_CORRECTED,
            second,
            json!({ "band": "40m", "contacted_callsign": "VE3XYZ" }),
        );
        log.append(
            OFFICIAL_LOG_QSO_DELETED,
            third,
            json!({ "reason": "duplicate" }),
        );

        let projector = log.projector();
        let report = projector.rebuild().expect("full rebuild");
        assert_eq!(report.mode, ProjectionMode::FullRebuild);
        assert_eq!(report.events_verified, 8);
        assert_eq!(report.events_projected, 8);
        assert_eq!(report.events_rehydrated, 0);
        assert_eq!(report.checkpoint.sequence, 8);
        assert_eq!(report.checkpoint.status, ProjectionStatus::Ok);
        assert_eq!(report.anomalies_recorded, 0);

        // Compare against a straight `ham-core` replay of the same log.
        let store = JsonlLogbookEventStore::open(&log.path).expect("open jsonl store");
        let expected = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(store.rebuild_projections(log.logbook_id))
            .expect("core replay");

        for qso in expected.list(true) {
            let row = read_row(&projector, PROJECTION_QSO_TABLE, qso.qso_id.to_string())
                .unwrap_or_else(|| panic!("row for {} projected", qso.qso_id));
            assert_eq!(row["removed"].as_bool(), Some(qso.deleted));
            assert_eq!(row["payload"], qso.payload);
            assert_eq!(
                row["last_event_hash"].as_str(),
                Some(qso.last_event_hash.as_str())
            );
        }

        // The corrected QSO carries the merged payload, not the original.
        let corrected =
            read_row(&projector, PROJECTION_QSO_TABLE, second.to_string()).expect("corrected row");
        assert_eq!(corrected["band"].as_str(), Some("40m"));
        assert_eq!(corrected["contacted_callsign"].as_str(), Some("VE3XYZ"));

        // The tombstoned QSO is still a row, flagged as removed.
        let tombstoned =
            read_row(&projector, PROJECTION_QSO_TABLE, third.to_string()).expect("tombstoned row");
        assert_eq!(tombstoned["removed"].as_bool(), Some(true));

        let activation = read_row(
            &projector,
            PROJECTION_ACTIVATION_TABLE,
            activation_id.to_string(),
        )
        .expect("activation row");
        assert_eq!(activation["status"].as_str(), Some("active"));
        assert_eq!(activation["qso_count"].as_i64(), Some(2));
        assert_eq!(activation["unique_callsign_count"].as_i64(), Some(2));
    }

    /// A tombstone is a projection-level removal: the row stays, and applying it
    /// costs one ordinary upsert like any other entry.
    #[test]
    fn tombstone_projects_as_removal_not_row_delete() {
        let mut log = TestLog::new("tombstone");
        let kept = log.create_qso("W1AW");
        let removed = log.create_qso("VE3ABC");
        let projector = log.projector();
        projector.rebuild().expect("rebuild");
        assert_eq!(count_rows(&projector, PROJECTION_QSO_TABLE), 2);

        log.append(
            OFFICIAL_LOG_QSO_DELETED,
            removed,
            json!({ "reason": "duplicate" }),
        );
        let report = projector.project_incremental().expect("incremental");
        assert_eq!(report.events_projected, 1);

        // The row count is unchanged: nothing was deleted, only flagged.
        assert_eq!(count_rows(&projector, PROJECTION_QSO_TABLE), 2);
        let row = read_row(&projector, PROJECTION_QSO_TABLE, removed.to_string())
            .expect("tombstoned row still present");
        assert_eq!(row["removed"].as_bool(), Some(true));
        assert_eq!(
            row["last_event_type"].as_str(),
            Some(OFFICIAL_LOG_QSO_DELETED)
        );
        let kept_row =
            read_row(&projector, PROJECTION_QSO_TABLE, kept.to_string()).expect("kept row");
        assert_eq!(kept_row["removed"].as_bool(), Some(false));

        // A restore is likewise just the next entry.
        log.append(
            OFFICIAL_LOG_QSO_RESTORED,
            removed,
            json!({ "reason": "operator restore" }),
        );
        projector
            .project_incremental()
            .expect("incremental restore");
        let restored =
            read_row(&projector, PROJECTION_QSO_TABLE, removed.to_string()).expect("restored row");
        assert_eq!(restored["removed"].as_bool(), Some(false));
    }

    /// A process that died mid-append leaves a trailing entry with no newline.
    /// That is a normal crash artifact, not corruption: it must not be projected,
    /// must not raise an error, and must not block startup.
    #[test]
    fn truncated_trailing_line_is_not_corruption_and_does_not_block_startup() {
        let mut log = TestLog::new("truncated");
        let first = log.create_qso("W1AW");
        let second = log.create_qso("VE3ABC");

        // Append a well-formed event, then cut it short mid-JSON.
        let partial = log.event(
            OFFICIAL_LOG_QSO_CREATED,
            Uuid::new_v4(),
            json!({
                "contacted_callsign": "G0ABC",
                "station_callsign": "KE8YGW",
                "mode": "CW",
                "band": "20m",
                "started_at": "2026-07-06T00:02:00Z",
            }),
        );
        let serialized = serde_json::to_string(&partial).expect("serialize");
        log.append_raw(&serialized[..serialized.len() / 2]);

        let projector = log.projector();
        let report = projector
            .rebuild()
            .expect("truncated tail must not fail the run");
        assert_eq!(report.events_projected, 2);
        assert!(report.partial_trailing_bytes > 0);
        assert_eq!(report.checkpoint.status, ProjectionStatus::Ok);
        // The checkpoint stops before the partial bytes so the finished entry
        // gets picked up once the writer completes it.
        assert_eq!(
            report.checkpoint.byte_offset,
            fs::metadata(&log.path).expect("log metadata").len() - report.partial_trailing_bytes
        );
        assert!(read_row(&projector, PROJECTION_QSO_TABLE, first.to_string()).is_some());
        assert!(read_row(&projector, PROJECTION_QSO_TABLE, second.to_string()).is_some());
        assert!(read_row(
            &projector,
            PROJECTION_QSO_TABLE,
            partial.entity_id.expect("entity").to_string()
        )
        .is_none());

        // Once the writer finishes the entry, the next run picks it up.
        log.previous_hash = Some(partial.event_hash.clone());
        log.append_raw(&serialized[serialized.len() / 2..]);
        log.append_raw("\n");
        let report = projector
            .project_incremental()
            .expect("completed entry projects");
        assert_eq!(report.events_projected, 1);
        assert_eq!(report.partial_trailing_bytes, 0);
        assert!(read_row(
            &projector,
            PROJECTION_QSO_TABLE,
            partial.entity_id.expect("entity").to_string()
        )
        .is_some());
    }

    /// A tombstone whose target has never been created should not happen, but if
    /// it does it is recorded as an anomaly rather than silently swallowed.
    #[test]
    fn orphan_tombstone_is_recorded_not_swallowed() {
        let mut log = TestLog::new("orphan");
        log.create_qso("W1AW");
        let never_created = Uuid::new_v4();
        log.append(
            OFFICIAL_LOG_QSO_DELETED,
            never_created,
            json!({ "reason": "out of order" }),
        );

        let projector = log.projector();
        let report = projector.rebuild().expect("rebuild");
        assert_eq!(report.anomalies_recorded, 1);
        assert_eq!(count_rows(&projector, PROJECTION_ANOMALY_TABLE), 1);

        let anomaly = read_row(
            &projector,
            PROJECTION_ANOMALY_TABLE,
            format!("{ANOMALY_ORPHAN_TOMBSTONE}-2"),
        )
        .expect("anomaly recorded");
        assert_eq!(anomaly["kind"].as_str(), Some(ANOMALY_ORPHAN_TOMBSTONE));
        assert_eq!(
            anomaly["entity_id"].as_str(),
            Some(never_created.to_string().as_str())
        );
        assert_eq!(anomaly["sequence"].as_i64(), Some(2));

        // The anomaly does not halt the projector: replay continues.
        assert_eq!(report.checkpoint.status, ProjectionStatus::Ok);
        assert_eq!(report.events_projected, 2);

        // Re-projecting is idempotent: the anomaly id is derived from the entry.
        projector.rebuild().expect("second rebuild");
        assert_eq!(count_rows(&projector, PROJECTION_ANOMALY_TABLE), 1);
    }

    /// A deliberately broken hash chain must halt the projector at the break,
    /// leave the offending entry and everything after it unprojected, and record
    /// the halt where an operator will see it.
    #[test]
    fn broken_hash_chain_halts_at_the_break_and_reports_it() {
        let mut log = TestLog::new("broken-chain");
        let good = log.create_qso("W1AW");

        // Forge an entry whose previous_hash does not connect to the head.
        let mut forged = log.event(
            OFFICIAL_LOG_QSO_CREATED,
            Uuid::new_v4(),
            json!({
                "contacted_callsign": "VE3ABC",
                "station_callsign": "KE8YGW",
                "mode": "SSB",
                "band": "20m",
                "started_at": "2026-07-06T00:01:00Z",
            }),
        );
        forged.previous_hash = Some("0".repeat(64));
        forged.event_hash = forged.calculate_hash();
        let forged_qso_id = forged.entity_id.expect("entity");
        log.append_event(&forged);
        log.previous_hash = Some(forged.event_hash.clone());
        let after_break = log.create_qso("G0ABC");

        // Scope the projector: the embedded SurrealKV datastore allows one
        // instance per path, so the restart below needs this one dropped first.
        let checkpoint = {
            let projector = log.projector();
            let error = projector
                .rebuild()
                .expect_err("broken chain must halt the run");
            let message = error.to_string();
            assert!(
                matches!(
                    error,
                    ProjectorError::BrokenChain {
                        sequence: 2,
                        ref source,
                        ..
                    } if matches!(source, ChainVerificationError::PreviousHashMismatch { .. })
                ),
                "unexpected error: {error:?}"
            );
            // The message is unambiguous rather than a line that scrolls by.
            assert!(message.contains("PROJECTION HALTED"), "{message}");
            assert!(message.contains("hash chain is broken"), "{message}");

            // The entry at the break and everything after it stayed unprojected.
            assert!(read_row(&projector, PROJECTION_QSO_TABLE, good.to_string()).is_some());
            assert!(
                read_row(&projector, PROJECTION_QSO_TABLE, forged_qso_id.to_string()).is_none()
            );
            assert!(read_row(&projector, PROJECTION_QSO_TABLE, after_break.to_string()).is_none());

            // The halt is durable, so it survives a restart of the projector.
            let checkpoint = projector.checkpoint().expect("checkpoint");
            assert_eq!(checkpoint.status, ProjectionStatus::Halted);
            assert_eq!(checkpoint.halt_sequence, Some(2));
            assert!(checkpoint
                .halt_reason
                .as_deref()
                .expect("halt reason")
                .contains("hash chain is broken"));
            assert_eq!(
                checkpoint.sequence, 1,
                "checkpoint holds the last good entry"
            );
            checkpoint
        };
        assert_eq!(checkpoint.status, ProjectionStatus::Halted);

        // A halted projection refuses to advance until an operator intervenes.
        let restarted = log.projector();
        let error = restarted
            .project_incremental()
            .expect_err("halted projection must not silently resume");
        assert!(matches!(error, ProjectorError::Halted { sequence: 2, .. }));
    }

    /// An entry that is complete but not valid JSON is corruption, not a partial
    /// append, and must halt rather than be skipped.
    #[test]
    fn malformed_complete_line_halts_instead_of_being_skipped() {
        let mut log = TestLog::new("malformed");
        let good = log.create_qso("W1AW");
        log.append_raw("{\"event_id\": \"not-a-valid-event\"}\n");

        let projector = log.projector();
        let error = projector.rebuild().expect_err("malformed entry must halt");
        assert!(
            matches!(error, ProjectorError::MalformedEvent { sequence: 2, .. }),
            "unexpected error: {error:?}"
        );
        assert!(error.to_string().contains("PROJECTION HALTED"));
        assert!(read_row(&projector, PROJECTION_QSO_TABLE, good.to_string()).is_some());
        assert_eq!(
            projector.checkpoint().expect("checkpoint").status,
            ProjectionStatus::Halted
        );
    }

    /// Restarting the projector between batches must leave the projection
    /// identical to an uninterrupted run: no duplicated rows, no missing rows,
    /// and no state corrupted by resuming with cold in-memory projections.
    ///
    /// The entries appended after the interruption deliberately correct and
    /// tombstone QSOs created *before* it, so a resume that failed to rebuild
    /// its in-memory state would visibly corrupt the projected payloads.
    #[test]
    fn restarting_between_batches_leaves_no_duplicate_or_missing_rows() {
        let mut log = TestLog::new("restart");
        let first = log.create_qso("W1AW");
        let second = log.create_qso("VE3ABC");
        let third = log.create_qso("G0ABC");
        let fourth = log.create_qso("JA1ABC");

        // First run projects one full batch (batch_size is 4), then the process
        // "crashes" when the projector is dropped.
        let interrupted_checkpoint = {
            let projector = log.projector();
            let report = projector.rebuild().expect("first run");
            assert_eq!(report.events_projected, 4);
            assert_eq!(report.batches_committed, 1);
            report.checkpoint
        };
        assert_eq!(interrupted_checkpoint.sequence, 4);

        // More history lands, including changes to entities projected before
        // the interruption.
        log.append(
            OFFICIAL_LOG_QSO_CORRECTED,
            second,
            json!({ "band": "40m", "contacted_callsign": "VE3XYZ" }),
        );
        log.append(
            OFFICIAL_LOG_QSO_DELETED,
            third,
            json!({ "reason": "duplicate" }),
        );
        log.append(
            OFFICIAL_LOG_QSO_NOTE_ADDED,
            first,
            json!({ "note": "thanks for the contact" }),
        );
        let fifth = log.create_qso("VK2ABC");

        let projector = log.projector();
        let report = projector.project_incremental().expect("resumed run");
        assert_eq!(
            report.events_rehydrated, 4,
            "already-projected entries are replayed for state, not rewritten"
        );
        assert_eq!(report.events_projected, 4);
        assert_eq!(report.checkpoint.sequence, 8);
        assert_eq!(report.checkpoint.status, ProjectionStatus::Ok);

        // Exactly one row per QSO: no duplicates, none missing.
        assert_eq!(count_rows(&projector, PROJECTION_QSO_TABLE), 5);

        // Every projected row matches a straight `ham-core` replay of the log.
        let store = JsonlLogbookEventStore::open(&log.path).expect("open jsonl store");
        let expected = tokio::runtime::Runtime::new()
            .expect("runtime")
            .block_on(store.rebuild_projections(log.logbook_id))
            .expect("core replay");
        for qso in expected.list(true) {
            let row = read_row(&projector, PROJECTION_QSO_TABLE, qso.qso_id.to_string())
                .unwrap_or_else(|| panic!("row for {} projected", qso.qso_id));
            assert_eq!(
                row["payload"], qso.payload,
                "payload drift for {}",
                qso.qso_id
            );
            assert_eq!(row["removed"].as_bool(), Some(qso.deleted));
            assert_eq!(
                row["note_history"].as_array().map(Vec::len),
                Some(qso.note_history.len())
            );
        }

        // The correction merged onto the full pre-interruption payload rather
        // than onto an empty record.
        let corrected =
            read_row(&projector, PROJECTION_QSO_TABLE, second.to_string()).expect("corrected row");
        assert_eq!(corrected["band"].as_str(), Some("40m"));
        assert_eq!(corrected["contacted_callsign"].as_str(), Some("VE3XYZ"));
        assert_eq!(corrected["mode"].as_str(), Some("SSB"));
        assert_eq!(
            corrected["started_at"].as_str(),
            Some("2026-07-06T00:00:00Z")
        );

        let tombstoned =
            read_row(&projector, PROJECTION_QSO_TABLE, third.to_string()).expect("tombstoned row");
        assert_eq!(tombstoned["removed"].as_bool(), Some(true));
        assert!(read_row(&projector, PROJECTION_QSO_TABLE, fourth.to_string()).is_some());
        assert!(read_row(&projector, PROJECTION_QSO_TABLE, fifth.to_string()).is_some());

        // Re-running with nothing new appended is a no-op, not a re-projection.
        let idle = projector.project_incremental().expect("idle run");
        assert_eq!(idle.events_projected, 0);
        assert_eq!(idle.batches_committed, 0);
        assert_eq!(count_rows(&projector, PROJECTION_QSO_TABLE), 5);
    }

    /// A full rebuild wipes the projection and replays from the first byte, so
    /// rows left over from a previous shape of the projection cannot survive.
    #[test]
    fn full_rebuild_wipes_and_replays_from_the_start() {
        let mut log = TestLog::new("rebuild");
        let kept = log.create_qso("W1AW");
        let projector = log.projector();
        projector.rebuild().expect("first rebuild");

        // A stale row that no longer corresponds to any log entry.
        let stale_id = Uuid::new_v4().to_string();
        projector
            .store
            .metadata
            .run({
                let stale_id = stale_id.clone();
                move |client| async move {
                    let query = "UPSERT type::record($tb, $id) CONTENT { qso_id: $id };";
                    match client {
                        SurrealCloudClient::Local(db) => db
                            .query(query)
                            .bind(("tb", PROJECTION_QSO_TABLE))
                            .bind(("id", stale_id))
                            .await
                            .map_err(cloud_store_error)?
                            .check()
                            .map_err(cloud_store_error)?,
                        SurrealCloudClient::Remote(db) => db
                            .query(query)
                            .bind(("tb", PROJECTION_QSO_TABLE))
                            .bind(("id", stale_id))
                            .await
                            .map_err(cloud_store_error)?
                            .check()
                            .map_err(cloud_store_error)?,
                    };
                    Ok(())
                }
            })
            .expect("insert stale row");
        assert_eq!(count_rows(&projector, PROJECTION_QSO_TABLE), 2);

        let report = projector.rebuild().expect("second rebuild");
        assert_eq!(report.events_rehydrated, 0, "a rebuild replays everything");
        assert_eq!(report.events_projected, 1);
        assert_eq!(
            report.checkpoint.rebuild_generation, 2,
            "each rebuild is a new generation"
        );
        assert_eq!(count_rows(&projector, PROJECTION_QSO_TABLE), 1);
        assert!(read_row(&projector, PROJECTION_QSO_TABLE, stale_id).is_none());
        assert!(read_row(&projector, PROJECTION_QSO_TABLE, kept.to_string()).is_some());
    }

    /// Only one projector may write the projection tables.
    #[test]
    fn second_writer_is_refused_the_projection_lease() {
        let mut log = TestLog::new("lease");
        log.create_qso("W1AW");
        let projector = log.projector();
        projector.rebuild().expect("first writer projects");

        // A second projector against the same datastore, still within the lease.
        let mut config = ProjectorConfig::new(&log.path);
        config.batch_size = 4;
        let second = SurrealProjector::open(
            ProjectionStore::from_metadata(Arc::clone(&projector.store.metadata)),
            config,
        )
        .expect("second projector opens");
        let error = second
            .project_incremental()
            .expect_err("a second writer must be refused");
        assert!(
            matches!(error, ProjectorError::WriterLeaseHeld { .. }),
            "unexpected error: {error:?}"
        );
    }

    /// Replacing the log under a live projection is not something the projector
    /// can silently absorb: it demands an explicit rebuild.
    #[test]
    fn replaced_log_is_refused_rather_than_projected_over() {
        let mut log = TestLog::new("replaced");
        log.create_qso("W1AW");
        log.create_qso("VE3ABC");
        let checkpoint = {
            let projector = log.projector();
            projector.rebuild().expect("first run").checkpoint
        };
        assert_eq!(checkpoint.sequence, 2);

        // A different log with a different history takes the same path.
        fs::remove_file(&log.path).expect("remove log");
        log.previous_hash = None;
        log.create_qso("G0ABC");

        let projector = log.projector();
        let error = projector
            .project_incremental()
            .expect_err("a replaced log must be refused");
        assert!(
            matches!(error, ProjectorError::LogRewritten { .. }),
            "unexpected error: {error:?}"
        );

        // An explicit rebuild is the documented recovery path.
        let report = projector.rebuild().expect("rebuild recovers");
        assert_eq!(report.events_projected, 1);
        assert_eq!(count_rows(&projector, PROJECTION_QSO_TABLE), 1);
    }

    /// A replacement log that is *longer* than the checkpoint offset cannot be
    /// caught by a length check, and its entries may form a perfectly valid
    /// chain of their own. It is still not the log the checkpoint describes, so
    /// the projector must refuse it instead of projecting from the middle.
    #[test]
    fn replaced_log_that_outgrows_the_checkpoint_is_still_refused() {
        let mut log = TestLog::new("replaced-longer");
        log.create_qso("W1AW");
        log.create_qso("VE3ABC");
        let checkpoint = {
            let projector = log.projector();
            projector.rebuild().expect("first run").checkpoint
        };
        assert!(checkpoint.byte_offset > 0);

        // A different, internally valid history, long enough to pass the
        // checkpoint offset, with entries that do not land on that boundary.
        fs::remove_file(&log.path).expect("remove log");
        log.previous_hash = None;
        for index in 0..6 {
            log.create_qso(&format!("LONGER{index}"));
        }
        assert!(
            fs::metadata(&log.path).expect("log metadata").len() > checkpoint.byte_offset,
            "the replacement log must outgrow the checkpoint offset"
        );

        let projector = log.projector();
        let error = projector
            .project_incremental()
            .expect_err("a replaced log must be refused even when it is longer");
        assert!(
            matches!(error, ProjectorError::LogRewritten { .. }),
            "unexpected error: {error:?}"
        );
        // Nothing from the replacement history leaked into the projection.
        assert_eq!(count_rows(&projector, PROJECTION_QSO_TABLE), 2);

        let report = projector.rebuild().expect("rebuild recovers");
        assert_eq!(report.events_projected, 6);
        assert_eq!(count_rows(&projector, PROJECTION_QSO_TABLE), 6);
    }

    /// Tail mode picks up entries appended while it is running.
    #[test]
    fn tail_projects_newly_appended_entries() {
        let mut log = TestLog::new("tail");
        let first = log.create_qso("W1AW");
        let projector = Arc::new(log.projector());
        projector.rebuild().expect("initial rebuild");

        let shutdown = Arc::new(AtomicBool::new(false));
        let worker = std::thread::spawn({
            let projector = Arc::clone(&projector);
            let shutdown = Arc::clone(&shutdown);
            move || projector.tail(&shutdown)
        });

        let second = log.create_qso("VE3ABC");
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if read_row(&projector, PROJECTION_QSO_TABLE, second.to_string()).is_some() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "tail did not project the new entry"
            );
            std::thread::sleep(Duration::from_millis(25));
        }

        shutdown.store(true, Ordering::Relaxed);
        worker.join().expect("tail thread joins").expect("tail run");
        assert!(read_row(&projector, PROJECTION_QSO_TABLE, first.to_string()).is_some());
    }

    /// Measures full-rebuild throughput across batch sizes.
    ///
    /// Ignored by default because it writes a multi-thousand-entry log and runs
    /// for tens of seconds. Run it with:
    /// `cargo test -p ham-sync --features surreal-storage --release
    ///  projection_batch_size_benchmark -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn projection_batch_size_benchmark() {
        const EVENTS: usize = 20_000;
        for batch_size in [100usize, 250, 500, 1_000, 2_000, 5_000] {
            let mut log = TestLog::new(&format!("bench-{batch_size}"));
            let activation_id = Uuid::new_v4();
            log.append(
                OFFICIAL_LOG_ACTIVATION_STARTED,
                activation_id,
                json!({
                    "activation_type": "pota",
                    "park_id": "US-1234",
                    "station_callsign": "KE8YGW",
                    "operator_callsign": "KE8YGW",
                    "started_at": "2026-07-06T00:00:00Z",
                }),
            );
            for index in 0..EVENTS {
                let qso_id = log.create_qso(&format!("W1AW/{}", index % 512));
                if index % 4 == 0 {
                    log.append(
                        OFFICIAL_LOG_QSO_ACTIVATION_LINKED,
                        qso_id,
                        json!({ "activation_id": activation_id.to_string() }),
                    );
                }
                if index % 16 == 0 {
                    log.append(OFFICIAL_LOG_QSO_CORRECTED, qso_id, json!({ "band": "40m" }));
                }
                if index % 32 == 0 {
                    log.append(
                        OFFICIAL_LOG_QSO_DELETED,
                        qso_id,
                        json!({ "reason": "dupe" }),
                    );
                }
            }

            let mut config = ProjectorConfig::new(&log.path);
            config.batch_size = batch_size;
            config.writer_id = log.writer_id.clone();
            let projector = SurrealProjector::open(log.store.clone(), config).expect("projector");
            let report = projector.rebuild().expect("rebuild");
            let per_second = if report.elapsed_ms > 0 {
                report.events_projected as f64 * 1000.0 / report.elapsed_ms as f64
            } else {
                f64::INFINITY
            };
            println!(
                "batch_size={batch_size:>5} events={:>6} bytes={:>9} batches={:>5} \
                 elapsed_ms={:>6} events_per_second={per_second:>10.0}",
                report.events_projected,
                report.bytes_read,
                report.batches_committed,
                report.elapsed_ms,
            );
        }
    }
}
