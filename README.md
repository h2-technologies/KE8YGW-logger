# Ham Radio Operations Platform

This workspace is the first foundation for a local-first, plugin-based amateur
radio operations platform. The MVP direction is casual logging, POTA/SOTA, and
sync, with room for emergency communications, net control, and contesting.

## Workspace

- `ham-core`: append-only logbook events, event bus, proposal validation, event store, and projections.
- `ham-plugin-sdk`: public plugin manifest, capability, proposal, and event constant types.
- `ham-sync`: local-first discovery, handshake, head comparison, and safe pull replication models.
- `ham-sync-server`: self-hostable cloud relay/sync service binary using the shared safe replication protocol.
- `ham-cli`: placeholder command-line entry point.
- `ham-gui`: initial GUI shell, workspace model, panel registry, command registry,
  and static web shell served by a small Rust binary.

## Architecture

The official logbook is a global append-only event stream. Each official event
contains its own SHA-256 hash and the previous event hash for the same logbook,
similar to a Git commit chain. Deletes are tombstone events. The original events
remain in history, and read models decide how to display current state.

Official log events are separate from runtime diagnostic events. Official events
are permanent, hash-chained logbook history. Runtime events are operational
telemetry for the UI, plugin runtime, sync, diagnostics, and proposal processing.

Plugins cannot write official events directly. A plugin submits a proposal such
as `proposal.qso.create`, `proposal.qso.correct`, or `proposal.qso.delete`.
`ham-core` validates the plugin manifest capability, the operator role, the
proposal event type, and the basic payload schema. Only then does the core append
an official event such as `official.log.qso.created`.

The event bus is a first-class typed interface. This initial implementation
provides an async in-memory bus that publishes official logbook events and
diagnostic runtime events. Runtime events are diagnostic only and are not part of
the official logbook stream.

Projections are derived state. The QSO current-state projection rebuilds from the
event stream and hides tombstoned QSOs while preserving the delete event in the
append-only history.

## Official QSO Workflow

Plugins and GUI panels submit proposals; they do not write official events. The
core validates plugin capabilities, operator role permissions, QSO schema, and
existing QSO references before appending an official event.

Supported proposal types:

- `proposal.qso.create`
- `proposal.qso.correct`
- `proposal.qso.delete`
- `proposal.qso.restore`
- `proposal.qso.note.add`

Supported official event types:

- `official.log.qso.created`
- `official.log.qso.corrected`
- `official.log.qso.deleted`
- `official.log.qso.restored`
- `official.log.qso.note_added`

Each official event includes logbook metadata, station/operator callsigns, source
device and plugin IDs, a correlation ID, payload, `previous_hash`, and
`event_hash`. The first event in a logbook has a null previous hash. Each later
event references the prior head hash. Chain verification recalculates event
hashes and confirms each previous hash link.

QSO deletes append a tombstone event; they never remove earlier events. Restores
append a restore event. Corrections append partial field updates. Notes append
note history instead of overwriting existing notes.

The QSO projection is disposable state rebuilt from official events. It can list
visible QSOs, include deleted QSOs when requested, show current corrected field
values, show note history, and show deleted/restored state.

The GUI Casual Logger submits QSO create proposals through `/api/qso/create`.
Recent QSOs are loaded from `/api/qsos`, which rebuilds the QSO projection from
the official event store. Delete, restore, and note actions also go through
proposal endpoints.

## POTA/SOTA Activation Plugin

The first operating-mode plugin is `plugin.pota-sota`. It uses the normal plugin
manifest/capability model and does not write official events directly. The GUI
panels submit activation and QSO proposals to `ham-core`; the core validates
permissions and schemas, then appends official activation/QSO-link events to the
same hash-chained official log.

Requested plugin permissions:

- `activation.create`
- `activation.update`
- `activation.end`
- `activation.view`
- `log.qso.create`
- `log.qso.correct`
- `log.qso.note.add`
- `adif.export`

Activation proposal types:

- `proposal.activation.create`
- `proposal.activation.update`
- `proposal.activation.start`
- `proposal.activation.end`
- `proposal.activation.cancel`
- `proposal.activation.note.add`
- `proposal.qso.activation.link`
- `proposal.qso.activation.unlink`

Official activation event types:

- `official.log.activation.created`
- `official.log.activation.updated`
- `official.log.activation.started`
- `official.log.activation.ended`
- `official.log.activation.cancelled`
- `official.log.activation.note_added`
- `official.log.qso.activation_linked`
- `official.log.qso.activation_unlinked`

The activation projection rebuilds from official events. It lists activations,
finds the active activation for a station/operator, tracks status, notes, linked
QSOs, QSO count, unique callsigns, and band/mode summaries. Deleted QSOs stop
counting after projection replay; restored QSOs count again.

The POTA/SOTA workspace includes Activation Setup, Activation Progress,
Activation Recent QSOs, Portable Logger Entry, and a Spots/Alerts placeholder.
Start an activation, then log contacts from Portable Logger Entry. New portable
QSOs include activation metadata and are linked back to the active activation
through a proposal. Recent QSOs display the activation reference when available.

Activation ADIF export includes `MY_SIG` and `MY_SIG_INFO` for POTA/SOTA QSOs.
POTA exports `MY_SIG=POTA` and the park reference; SOTA exports `MY_SIG=SOTA`
and the summit reference. Deleted/cancelled QSOs are excluded by default.

Current MVP limitations: one active activation per station/operator is assumed;
multi-op, multi-park/multi-summit, spotting integration, online reference
lookups, GPS auto-detection, offline reference caches, award tracking, and
conflict review UX are future work.

## Callsign Lookup And Smart Autofill Plugin

`plugin.callsign-lookup` provides advisory lookup/enrichment suggestions for the
Casual Logger and POTA/SOTA Portable Logger. It does not write official QSO
events directly. Lookup results are shown to the operator as suggestions; only
accepted fields included in a submitted QSO form become part of a normal
`proposal.qso.create` payload.

Provider architecture:

- `CallsignLookupProvider` defines callsign, grid, entity, and provider-status
  methods.
- `LocalPrefixProvider` provides offline prefix/entity inference.
- `MockLookupProvider` supports tests and development.
- `QrzLookupProviderStub` documents the future QRZ/HamQTH integration point
  without requiring paid credentials.

Current plugin permissions:

- `lookup.callsign`
- `lookup.entity`
- `lookup.grid`
- `cache.lookup.read`
- `cache.lookup.write`
- `log.qso.suggest_fields`
- `network.external.lookup` for future online providers

Lookup cache entries live outside official log storage and are not synced by
default. Cache entries include provider, fetched time, expiry, and confidence.
The default TTL is 30 days. Clearing the lookup cache publishes
`lookup.cache.cleared`.

Runtime lookup events include:

- `lookup.callsign.started`
- `lookup.callsign.cache_hit`
- `lookup.callsign.cache_miss`
- `lookup.callsign.completed`
- `lookup.callsign.failed`
- `lookup.entity.inferred`
- `lookup.grid.validated`
- `lookup.suggestion.created`
- `lookup.cache.cleared`

Privacy model: the MVP offline provider does not contact external services.
Future online providers must avoid sending full logs, must not log API keys, and
must redact secret-like fields before diagnostic persistence. Raw provider
responses are not stored in official QSO events by default.

Logger workflow:

1. Enter a callsign in Casual Logger or Portable Logger Entry.
2. Click `Lookup`, or use the `Lookup Callsign` command palette action.
3. Review suggested name, QTH, grid, country/entity, DXCC, CQ zone, and ITU zone.
4. Click `Accept Suggestions` to fill the pending form payload.
5. Submit the QSO. Accepted fields flow through the regular QSO proposal path.

Current limitations: QRZ, HamQTH, FCC/ULS lookup, DXCC database updates,
distance/bearing display, award-needed hints, and auto-accept trusted lookups are
future work.

## Durable Local Persistence

The MVP durable official event store is JSON Lines. Each official event is
serialized as one immutable line in `official-events.jsonl`. The store is still
behind the `LogbookEventStore` trait so it can move to SQLite, SurrealDB, or a
cloud-backed sync store later.

The default official event path is:

```text
<platform log dir>/official/official-events.jsonl
```

Set `HAM_PLATFORM_EVENT_LOG` to use a different file during development or
testing. On GUI startup, the app opens the JSONL store, verifies the hash chain,
rebuilds the QSO projection, and publishes runtime events:

- `storage.opened`
- `official.log.chain.verified`
- `projection.qso.rebuilt`
- `storage.error` if verification/opening fails

To verify a logbook hash chain in code, call:

```rust
store.verify_chain(logbook_id).await?;
```

From the CLI:

```powershell
cargo run -p ham-cli -- verify-chain
cargo run -p ham-cli -- rebuild-projections
```

## ADIF Import And Export

ADIF export is generated from the QSO projection, not by reading mutable UI
state. Deleted QSOs are excluded by default. The exporter includes standard ADIF
fields such as `CALL`, `STATION_CALLSIGN`, `OPERATOR`, `QSO_DATE`, `TIME_ON`,
`TIME_OFF`, `BAND`, `FREQ`, `MODE`, `SUBMODE`, signal reports, grid, name, QTH,
comments, and POTA/SOTA signal fields where available.

ADIF import parses records and converts each QSO into `proposal.qso.create`.
Those proposals go through the same core validation and append-only official
event pipeline used by the GUI. Imported QSOs use `source = imported/adif`, and
raw ADIF fields are preserved in `import_metadata.raw_adif`.

Duplicate detection is intentionally simple for MVP. A QSO is considered a
duplicate when contacted callsign and mode match, band matches when provided,
and `started_at` is within 10 minutes. Duplicates are skipped by default and
reported in the import summary.

GUI import/export currently uses path prompts because native file dialogs are
not wired yet:

- `Import ADIF` reads an ADIF file from a typed path.
- `Export ADIF` writes visible, non-deleted QSOs to a typed path.

CLI commands:

```powershell
cargo run -p ham-cli -- import-adif path\to\log.adi
cargo run -p ham-cli -- export-adif path\to\export.adi
```

## LAN Discovery And Sync Handshake

`ham-sync` defines the first local-first LAN sync layer. The MVP supports
discovery packets, an in-memory peer registry, handshake request/response
models, logbook head comparison, and user-initiated pull replication.

Discovery uses configurable IPv4 and IPv6 multicast defaults:

- IPv4 multicast: `239.73.89.71`
- IPv6 multicast: `ff12::73:5947`
- Discovery port: `9737`
- Local sync API port: `9738`
- Peer timeout: 45 seconds
- Discovery interval: 5 seconds

Discovery packets advertise protocol name/version, device ID, session ID,
optional user hash, display name, capabilities, optional local API port, and
timestamp. They do not include secrets, API keys, profile details, log contents,
or official events.

The first handshake exchanges protocol version, device/session IDs,
capabilities, available logbook IDs, current head hash, and event count hints.
The response reports matching logbooks and head comparison states:

- `unknown`
- `match`
- `local_ahead`
- `remote_ahead`
- `diverged`

Event counts are hints only. If head hashes differ and ancestry has not been
exchanged, the MVP treats the result as unknown or diverged until the later
replication protocol can compare event ancestry safely.

The GUI Sync Status panel can start/stop discovery, refresh peers, handshake with
a selected peer, preview a pull, pull missing events, and copy the local sync
identity. The current implementation keeps peers in memory and includes a demo
refresh path for local testing while the multicast service is finalized for real
multi-device runs.

Runtime events include:

- `network.discovery.started`
- `network.discovery.stopped`
- `network.peer.discovered`
- `network.peer.updated`
- `network.peer.expired`
- `sync.handshake.accepted`
- `sync.handshake.error`

Security limitations for MVP: peers are untrusted, no destructive commands are
accepted, automatic replication is disabled, and authentication/trust pairing is
a TODO before unattended sync.

## Safe LAN Event Replication

Official log replication is pull-based for MVP. The GUI or an admin action must
initiate replication from a selected peer. Peers do not push events into the
local official log, and runtime diagnostic logs, credentials, and private config
are never synced.

The sync protocol models include:

- `list_logbooks`
- `get_head`
- `get_events_since`
- `get_event_metadata`
- `preview_pull`
- `pull_events`

`Preview Pull` compares the local head with the remote event chain and reports
how many official events would be pulled. It does not write anything. If the
remote chain does not contain the local head, the preview reports divergence.

`Pull Missing Events` requests full official event envelopes and verifies them
before storage. The verifier checks:

- deterministic event hash validity
- supported schema version
- supported official event type
- matching logbook ID
- first incoming `previous_hash` connects to the local head
- incoming events chain together
- duplicate event IDs are identical before they are ignored
- duplicate event IDs with different content are rejected

Accepted remote events are appended through the official `LogbookEventStore`
replication API, preserving the original event metadata and hash. The store does
not rewrite event IDs, timestamps, authorship, source device IDs, payloads, or
hash input. After a successful pull, the GUI verifies the local chain and
rebuilds QSO projections.

If chains diverge, the MVP does not merge automatically. Divergence is stored in
sync UI state, shown in the Sync Status panel, and emitted as
`sync.divergence.detected`. Branch review, conflict resolution, signed events,
device pairing, and cloud relay support are intentionally deferred.

Replication runtime events include:

- `sync.preview_pull.started`
- `sync.preview_pull.completed`
- `sync.pull.started`
- `sync.pull.progress`
- `sync.remote_event.received`
- `sync.remote_event.accepted`
- `sync.remote_event.rejected`
- `sync.pull.completed`
- `sync.pull.failed`
- `sync.divergence.detected`
- `projection.qso.rebuilt`

To try the current GUI workflow locally:

1. Run the GUI with `just gui`.
2. Open the Dashboard Sync Status panel.
3. Click `Refresh Peers` to add the demo LAN peer.
4. Click `Preview Pull` to inspect available remote events.
5. Click `Pull Missing` to append verified missing events and rebuild QSOs.

For two real local instances, run two GUI processes on different ports and set
separate `HAM_PLATFORM_EVENT_LOG` paths so they do not share the same JSONL
store. Real peer-to-peer HTTP transport and trust pairing are the next sync
tasks; the protocol messages added here are designed to be reused by that
transport.

## Cloud Relay And Self-Hosted Sync

Cloud sync is a fallback when LAN peers cannot reach each other. LAN remains the
preferred path when available. The cloud service reuses the same official event
envelopes and safe replication rules as LAN sync; it does not sync runtime
diagnostic logs, credentials, API keys, private plugin config, or mutable UI
state.

The shared `ham-sync` crate now defines cloud API messages, an MVP pairing
session model, a cloud client abstraction, and an in-memory server backend used
by tests and the self-hosted binary. Hosted and self-hosted deployments use the
same server implementation.

MVP auth uses pairing-code/token sessions. A paired device receives a sync token
scoped to an account, user, device, and explicit logbook IDs. The server rejects
unauthenticated requests, unauthorized logbooks, invalid event hashes,
unsupported schema versions, divergent chains, and duplicate event IDs with
different content. The server may track relay metadata separately, but it never
rewrites official event metadata that participates in the event hash.

Cloud API surface:

- `GET /health`
- `POST /api/v1/auth/pair`
- `GET /api/v1/logbooks?token=...`
- `GET /api/v1/logbooks/{logbook_id}/head?token=...`
- `GET /api/v1/logbooks/{logbook_id}/events?token=...`
- `POST /api/v1/logbooks/{logbook_id}/preview-pull`
- `POST /api/v1/logbooks/{logbook_id}/pull`
- `POST /api/v1/logbooks/{logbook_id}/push`
- `GET /api/v1/sync/status?token=...`

Push behavior:

1. Client sends local official events to the server.
2. Server verifies hashes, schema, event type, duplicates, and chain continuity.
3. Valid events are stored append-only.
4. Divergent or invalid events are rejected without mutation.

Pull behavior:

1. Client previews remote cloud events against its local head.
2. Server returns missing event envelopes.
3. Client verifies hashes and chain continuity again before appending locally.
4. Client rebuilds projections after accepted events.

The GUI Settings screen includes Cloud Sync options for server URL, device name,
enablement, LAN preference, auto push, auto pull, and sync interval. The Sync
Status panel shows cloud connection state, account/device status, server URL,
local/cloud heads, last push/pull time, pending preview count, and divergence
warnings. It also provides `Connect Cloud`, `Push Now`, `Preview Cloud Pull`,
and `Pull Cloud` actions.

Cloud runtime events include:

- `sync.cloud.connect.started`
- `sync.cloud.connect.succeeded`
- `sync.cloud.connect.failed`
- `sync.cloud.push.started`
- `sync.cloud.push.progress`
- `sync.cloud.push.completed`
- `sync.cloud.push.failed`
- `sync.cloud.preview_pull.started`
- `sync.cloud.preview_pull.completed`
- `sync.cloud.pull.started`
- `sync.cloud.pull.completed`
- `sync.cloud.pull.failed`
- `sync.cloud.divergence.detected`

Run the self-hosted sync server:

```powershell
just sync-server
```

Default server settings:

```text
HAM_SYNC_SERVER_BIND=127.0.0.1:9740
HAM_SYNC_PUBLIC_URL=http://127.0.0.1:9740
HAM_SYNC_SERVICE_MODE=self_hosted
HAM_SYNC_PAIRING_CODE=local-dev-pairing-code
```

See `.env.example` for the same settings. The MVP server uses an in-memory
storage backend for development and tests; durable server-side storage is the
next self-hosting task.

Docker build:

```powershell
docker build -f Dockerfile.sync-server -t ke8ygw-sync-server .
docker run --rm -p 9740:9740 -e HAM_SYNC_PAIRING_CODE=change-me ke8ygw-sync-server
```

Current limitations: pairing is token-based MVP auth, events are not signed,
end-to-end encryption is not implemented, automatic merge/conflict resolution is
deferred, and the self-hosted server storage is not durable yet.

## GUI Architecture

The GUI shell is implemented in `ham-gui`. It is intentionally a client of the
shared core rather than an owner of logging rules. The Rust side defines
JSON-serializable workspace layouts, panel registrations, command definitions,
mock plugin data, and a small local web server. The web side renders the shell
using static HTML, CSS, and JavaScript.

This is a web-first foundation that is Tauri-ready: the current `ham-gui` binary
serves the same assets a future Tauri desktop shell can embed. Tauri was not
added yet so CI stays lightweight while the project is still defining the shell
and core integration contracts.

The default shell includes:

- Left activity navigation
- Top toolbar and workspace selector
- Central workspace panel region
- Right inspector/context region
- Bottom panel region
- Bottom status bar
- Command palette with `Ctrl+K` or `Cmd+K`
- Settings placeholder
- Plugin manager placeholder

The default workspaces are Dashboard, Casual Logger, POTA/SOTA, Net Control,
EmComm, and Contesting. Panels have stable IDs, titles, plugin/source labels,
required permissions, and supported workspaces. The first layout is static; the
layout model includes a TODO path for future dockable panel movement and saved
custom layouts.

The GUI loads shell state from `/api/shell` and consumes runtime diagnostics
through `/api/runtime-events`. Plugin data is still static placeholder data until
real plugin loading is implemented.

The runtime event bridge now lives between the GUI server and `ham-core`. GUI
panels never write official logbook events directly. The bridge publishes typed
runtime events to the shared in-memory core event bus, writes redacted runtime
events to rotating JSONL diagnostic logs, and exposes replay/filter/export API
endpoints for the Event Bus Monitor.

Runtime events are diagnostic events, not official logbook history. Official
logbook events remain append-only, hash-chained records owned by `ham-core`.
Runtime events are for UI, plugin, sync, rig, network, proposal, projection,
diagnostics, and app telemetry such as `ui.*`, `plugin.*`, `sync.*`, and
`diagnostics.*`.

Runtime diagnostic logs are written as JSON Lines to the platform log directory:

- Windows: `%LOCALAPPDATA%\KE8YGW Logger\logs`
- macOS: `~/Library/Logs/KE8YGW Logger`
- Linux: `~/.local/state/ke8ygw-logger/logs`

Set `HAM_PLATFORM_LOG_DIR` to override the directory for development. The active
file is `runtime-events.jsonl`. It rotates at 10 MB and keeps 5 rotated files
plus the active file. Secret-like fields such as passwords, tokens, secrets, API
keys, and authorization values are redacted before persistence.

The Event Bus Monitor panel shows live runtime events from the bridge replay
buffer. It supports severity, category, source/plugin, and text filters. Use the
panel buttons to pause or resume live polling, clear the current view, copy the
selected event, or export the visible events as JSONL. The command palette also
includes commands to open the monitor, pause the stream, export visible runtime
events, copy the latest error, and show the diagnostics folder path.

To run the GUI locally:

```powershell
cargo run -p ham-gui --bin ham-gui
```

Then open:

```text
http://127.0.0.1:9467
```

## Running Tests

```powershell
cargo test
```

The initial test suite verifies event hash chaining, valid and tampered chain
verification, tombstone projection behavior, plugin capability rejection, and
event bus publication for accepted proposals.

## Local Development Commands

This repository uses [`just`](https://github.com/casey/just) to keep local
commands aligned with CI.

```powershell
just fmt      # format all Rust code
just check    # cargo check for all workspace crates and targets
just clippy   # clippy for all workspace crates and targets, warnings are errors
just test     # run all workspace tests
just build    # debug build for all workspace crates
just release  # release build for all workspace crates
just gui      # run the local GUI shell at http://127.0.0.1:9467
just sync-server # run the self-hosted sync server at http://127.0.0.1:9740
just ci       # formatting check, clippy, tests, and debug build
```

If `just` is not installed, the underlying commands are standard Cargo commands:

```powershell
cargo fmt --all
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
cargo build --release --workspace
cargo run -p ham-gui --bin ham-gui
cargo run -p ham-sync-server --bin ham-sync-server
```

## CI

GitHub Actions runs on pushes to `main` and on pull requests. The CI workflow
uses a Windows, Linux, and macOS matrix, caches Cargo dependencies, installs
`just`, and runs:

```powershell
just ci
```

CI fails on formatting drift, clippy warnings, test failures, or build failures.

## Release Builds

To build release binaries locally:

```powershell
just release
```

Tagged releases are automated from git tags matching `v*.*.*`, for example:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

The release workflow builds `ham-gui` in release mode on:

- `ubuntu-latest`
- `windows-latest`
- `macos-latest`

It packages the binary as `ham-platform` and uploads archives to the GitHub
Release:

- `ham-platform-linux-x86_64.tar.gz`
- `ham-platform-windows-x86_64.zip`
- `ham-platform-macos-aarch64.tar.gz` or `ham-platform-macos-x86_64.tar.gz`,
  depending on the runner architecture
