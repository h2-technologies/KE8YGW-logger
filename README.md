# Ham Radio Operations Platform

This workspace is the first foundation for a local-first, plugin-based amateur
radio operations platform. The MVP direction is casual logging, POTA/SOTA, and
sync, with room for emergency communications, net control, and contesting.

## Workspace

- `ham-core`: append-only logbook events, event bus, proposal validation, event store, and projections.
- `ham-plugin-sdk`: public plugin manifest, capability, proposal, and event constant types.
- `ham-sync`: placeholder for future local-first synchronization.
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
models, and logbook head comparison. Full event replication and merge are
intentionally deferred.

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
a selected peer, and copy the local sync identity. The current implementation
keeps peers in memory and includes a demo refresh path for local testing while
the multicast service is finalized for real multi-device runs.

Runtime events include:

- `network.discovery.started`
- `network.discovery.stopped`
- `network.peer.discovered`
- `network.peer.updated`
- `network.peer.expired`
- `sync.handshake.accepted`
- `sync.handshake.error`

Security limitations for MVP: peers are untrusted, no destructive commands are
accepted, no remote official events are merged, and authentication/trust pairing
is a TODO before automatic replication.

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
