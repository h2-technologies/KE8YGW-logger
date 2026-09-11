# SurrealDB Projection Pipeline

The append-only JSONL official event log is the only source of truth. The
projector reads that log and writes a fast, queryable copy of current state into
SurrealDB. That copy is a projection in the sense of
[ADR 0004](adr/0004-global-event-log-plus-projections.md): disposable,
rebuildable, and never authoritative.

## One-Way Data Flow

```text
official-events.jsonl  --(read, verify, replay)-->  SurrealDB projection tables
        ^                                                      |
        |                                                      v
  proposals / sync                                   GUI, plugins, queries
  (the only writers)                                      (read only)
```

- The projector opens the official log read-only and never writes to it.
- No SurrealDB read or query can cause a write to the official log.
- No code path mutates a past JSONL entry.
- Only the projector writes the projection tables. Everything else reads them.

## Where The Projector Lives

`crates/ham-sync/src/projector.rs`, behind the existing `surreal-storage`
feature. `ham-sync` already owns the sync server's SurrealDB client and its
JSONL official log, and the embedded SurrealKV datastore allows one instance per
path, so the projector shares the sync server's client rather than opening the
same datastore twice. `DurableCloudSyncServer::projector` builds that shared
instance; `ProjectionStore::open` builds a standalone one.

Row content comes from `ham-core`'s `QsoCurrentStateProjection` and
`ActivationProjection` and from `ham_core::projection_touch`, so the projected
copy always matches canonical replay semantics and the event vocabulary stays
owned by `ham-core`.

## Projection Tables

| Table | Contents |
| --- | --- |
| `projection_qso` | One row per QSO id, keyed by the QSO id. Carries the replayed payload, note history, linked activation ids, denormalized query columns (`contacted_callsign`, `station_callsign`, `operator_callsign`, `mode`, `band`, `started_at`), the originating event, and `removed`. |
| `projection_activation` | One row per activation id, with status, linked QSO ids, and the replay-derived counters (`qso_count`, `unique_callsign_count`, `band_summary`, `mode_summary`). |
| `projection_checkpoint` | The single `official_log` record describing how far the projector has verified and projected. |
| `projection_anomaly` | Replay anomalies that are surfaced rather than swallowed. |
| `projection_writer_lock` | The single-writer lease over the projection tables. |

Net Control and upload events are not projected here; they have their own
`ham-core` projections. The run report counts them as `unprojected_events` so an
unconsumed event type is visible rather than silent.

### Tombstones

A tombstone (`official.log.qso.deleted`) projects as **projection-level
removal**: the row stays and its `removed` field becomes `true`. The projector
never issues a row `DELETE` for a tombstone, because a deleted row in the
projection could be mistaken for deleted log history — and log history is never
deleted. Readers filter on `removed = false`.

Applying a tombstone costs exactly one upsert, the same as any other entry: it
is still just "replay the next entry". A restore
(`official.log.qso.restored`) is likewise just the next entry, flipping
`removed` back to `false`.

## Hash-Chain Verification

Every entry is verified before it is projected:

1. The entry's `event_hash` must match its recomputed canonical hash.
2. Its `previous_hash` must match the running head **for its own logbook** — the
   chain is per-logbook, and one JSONL file may interleave logbooks.

On a broken chain the projector **halts at the break**:

- The offending entry and everything after it are not projected.
- Entries verified before the break stay projected, and the checkpoint holds the
  last good entry so a repaired log resumes from the right place.
- The checkpoint record is marked `halted` with the reason and entry number, so
  the halt is durable and survives a restart.
- The error is typed (`ProjectorError::BrokenChain`) and its message starts with
  `PROJECTION HALTED`.
- `ham-sync-server` prints a boxed banner naming the failure and saying the
  projection is now stale.
- Incremental runs refuse to advance while the checkpoint is halted. Clearing a
  halt is an explicit operator action (a full rebuild).

An entry that is complete but not valid JSON is corruption, not a partial
append, and halts the same way.

The sync server keeps serving after a projection halt. A frozen read model does
not affect the append-only official log or the sync protocol, and taking the
server down would remove the access an operator needs to investigate.

## The Checkpoint

**Where it lives:** the SurrealDB record `projection_checkpoint:official_log`,
in the same database as the projection it describes. Keeping it there is safe
precisely because a full rebuild wipes the projection *and* the checkpoint
together, so the two can never disagree about which log prefix has been applied.

It records:

| Field | Meaning |
| --- | --- |
| `byte_offset` | Byte offset just past the last projected entry. |
| `sequence` | Count of entries verified and projected so far. |
| `heads` | Per-logbook chain head hashes, so a resumed run keeps verifying. |
| `last_event_id`, `last_event_hash` | Identify the entry the offset points at. |
| `status`, `halt_reason`, `halt_sequence` | `ok`, or a durable halt and its cause. |
| `rebuild_generation` | Incremented by each full rebuild. |
| `schema_version` | Bumped when the projected row shape needs a rebuild. |

### Idempotence And Restart Safety

Each batch writes its rows **and** its checkpoint in one SurrealDB transaction.
Either both land or neither does. Every row write is an upsert keyed by entity
id, so re-applying a batch after a crash produces the same rows. A projector
restarted mid-run therefore never duplicates and never skips a row.

On a cold start the projector replays the log from the first byte to rebuild its
in-memory projection state, and only *writes* entries past the checkpoint. This
is deliberate: a correction or tombstone appended after the checkpoint targets a
QSO created before it, so resuming with empty in-memory state would merge that
correction onto an empty record and corrupt the row. Replaying the prefix also
re-verifies the whole chain on every start. Once warm, the projector seeks
straight to the checkpoint offset.

If the log is shorter than the checkpoint offset, or the entry at that offset no
longer hashes to `last_event_hash`, the log was replaced or rewritten under a
live projection. The projector refuses to project over it
(`ProjectorError::LogRewritten`); a full rebuild is the recovery path.

### Partial Trailing Entries

A process that dies mid-append leaves trailing bytes with no newline. That is a
normal crash artifact, not corruption: the projector stops cleanly before those
bytes, reports them as `partial_trailing_bytes`, and leaves the checkpoint in
front of them. It does not error and does not block startup. The next run picks
the entry up once the writer finishes it.

### Anomalies

A tombstone naming an entity that was never created should not happen. If it
does it is not swallowed: a `projection_anomaly` row is written with the entry
number, ids, and detail, and the run report counts it. The anomaly id is derived
from the log entry, so re-projecting the same entry updates the same row instead
of accumulating duplicates. An anomaly does not halt replay.

## Single Writer

The projector takes a lease in `projection_writer_lock:official_log` before each
run and refreshes it with every batch commit. The conditional write and the
check that decides the winner run in one transaction, so two projectors starting
together cannot both win; the loser gets `ProjectorError::WriterLeaseHeld`.

The lease expires after `lock_ttl_seconds` (default 60). After an unclean
shutdown a restarting projector with a **new** writer id waits out the remaining
lease. Set `HAM_SYNC_PROJECTION_WRITER_ID` to a stable value for a given
deployment so a restart reclaims its own lease immediately.

## Operating Modes

### Incremental / Tail (default)

On startup the projector resumes from the checkpoint and then polls for newly
appended entries. Polling matches what the rest of the codebase does — the
workspace has no filesystem-watch dependency — and the append-only log makes
polling the file length sufficient. `ham-sync-server` runs this in a background
thread automatically.

### Full Rebuild (explicit only)

A full rebuild wipes the projection tables and the checkpoint, then replays the
whole log from the first byte. It is **never** automatic. Trigger it with:

```bash
ham-sync-server --rebuild-projection
```

The server rebuilds, prints the entry count and the new `rebuild_generation`,
then continues serving and tailing. The official event log is never modified by
a rebuild — only the projection is.

Rebuild after any of these:

- A halt that was investigated and resolved.
- A `PROJECTION_SCHEMA_VERSION` bump (projected row shape changed).
- A log replaced or restored from backup under a live projection.
- Any doubt about whether the projection matches the log.

## Configuration

| Variable | Default | Meaning |
| --- | --- | --- |
| `HAM_SYNC_PROJECTION_ENABLED` | enabled | Set to `0` to run the sync server without the projector. |
| `HAM_SYNC_PROJECTION_BATCH_SIZE` | `1000` | Entries per SurrealDB transaction. |
| `HAM_SYNC_PROJECTION_POLL_SECONDS` | `1` | Tail poll interval. |
| `HAM_SYNC_PROJECTION_WRITER_ID` | random per process | Stable id for the single-writer lease. |

## Measured Throughput

Batch size was chosen from measurement, not assumption. Full rebuild of a
26,876-entry / 21.9 MB log (20,000 QSOs with activation links, corrections, and
tombstones) into embedded SurrealKV, release build:

| Batch size | Batches | Elapsed | Entries/second |
| --- | --- | --- | --- |
| 100 | 269 | 10,988 ms | 2,446 |
| 250 | 108 | 9,789 ms | 2,746 |
| 500 | 54 | 9,492 ms | 2,831 |
| **1000** | **27** | **8,921 ms** | **3,013** |
| 2000 | 14 | 9,013 ms | 2,982 |
| 5000 | 6 | 8,979 ms | 2,993 |

Throughput plateaus at 1000, which is the default. Beyond it the remaining cost
is SurrealDB's per-row write cost, not per-transaction overhead, so larger
batches buy nothing and only widen the work a crash discards. A 100,000-entry
log rebuilds in roughly 33 seconds on this hardware.

Reproduce with:

```bash
cargo test -p ham-sync --features surreal-storage --release \
  projection_batch_size_benchmark -- --ignored --nocapture
```

The projector stream-parses the log: it holds the current projection state and
one batch in memory, never the whole file.

### Related `ham-core` Change

`ActivationProjection::apply` used to recompute counters for *every* activation
on *every* event, which made replay quadratic: 40,050 events took 126 seconds,
against 28 ms for the QSO projection alone. It now recomputes only the
activations an event actually touches, via a QSO-to-activation index exposed as
`ActivationProjection::activations_for_qso`. Same replay of 40,050 events now
takes 1.23 seconds. This also affects every other caller of
`rebuild_activation_projections`, including proposal validation and the hosted
activation routes.

Replay still grows faster than linearly, because linking the *n*th QSO to an
activation recomputes that activation's *n* links. 100,100 events replay in 4.2
seconds, which is well under the SurrealDB write cost and is not currently the
bottleneck.
