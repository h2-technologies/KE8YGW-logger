# Ham Radio Operations Platform

This workspace is the first foundation for a local-first, plugin-based amateur
radio operations platform. The MVP direction is casual logging, POTA/SOTA, and
sync, with room for emergency communications, net control, and contesting.

## Start Here

The project is now blueprint-driven. Contributors and future implementation
passes should start with these documents:

- [Master Blueprint](docs/MASTER_BLUEPRINT.md): locked architecture decisions,
  system model, MVP scope, and crate migration notes.
- [Roadmap](docs/ROADMAP.md): compressed vertical implementation passes and
  current pass status.
- [Event Catalog](docs/EVENT_CATALOG.md): official event, proposal, and runtime
  event names.
- [Plugin SDK](docs/PLUGIN_SDK.md): manifests, permissions, proposals, panels,
  and commands.
- [Sync Protocol](docs/SYNC_PROTOCOL.md): LAN discovery, handshake, replication,
  cloud relay, and divergence behavior.
- [Security Model](docs/SECURITY_MODEL.md): plugin permissions, operator roles,
  scopes, diagnostics, and auth posture.
- [Service Framework](docs/architecture/service-framework.md): shared provider
  registry, provider selection, service cache, and integration skeletons.
- [Station Profiles](docs/architecture/station-profiles.md): station/equipment
  support storage and logger defaults.
- [Award Engine](docs/architecture/award-engine.md): projection-backed award
  progress foundation.
- [Advanced Search](docs/architecture/search.md): QSO projection query syntax
  and saved-search model.
- [Upload Queue](docs/architecture/upload-queue.md): provider-backed ADIF upload
  queue foundation.
- [Maps](docs/maps/README.md): GIS service framework, map layers, markers, and
  QSO/station visualization.
- [Grid System](docs/grid-system/README.md): Maidenhead validation, conversion,
  distance, bearing, and bounds helpers.
- [Propagation](docs/propagation/README.md): provider-backed solar, band
  condition, grayline, and MUF placeholders.
- [Weather](docs/weather/README.md): provider-backed current weather and
  forecast model.
- [Map Providers](docs/plugin-map-providers/README.md): plugin guide for map,
  weather, propagation, geocoding, and overlay providers.
- [Provider Development](docs/plugins/provider-development.md): how plugins add
  lookup, upload, spotting, map, weather, and propagation providers.
- [Credentials and Redaction](docs/security/credentials-and-redaction.md):
  credential handling, config schemas, runtime redaction, and report safety.
- [Developer Guide](docs/DEVELOPER_GUIDE.md): local commands, completion
  checklist, feature workflow, and testing expectations.
- [Root Roadmap](ROADMAP.md): current milestone summary and next milestone.
- [Project State](PROJECT_STATE.md): authoritative implementation status,
  technical debt, test coverage, and next recommended milestone.
- [Architecture Decisions](docs/adr/0001-append-only-official-log.md): accepted
  ADRs for the foundational design choices.

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

## Unified Service Framework

`ham-core` now includes a shared service/provider framework so plugins do not
invent separate provider abstractions. Providers register metadata in a central
`ServiceRegistry`, declare service type, capabilities, permissions, config keys,
network/offline behavior, health, and priority, then application code consumes
provider-agnostic services.

Implemented service categories include callsign/entity/grid lookup, log upload,
spotting, map tiles, geocoding, weather, propagation, award data, AI tools,
authentication, storage, and notifications. The MVP includes local/mock lookup
providers, QRZ/HamQTH lookup stubs, LoTW/eQSL/Club Log/QRZ Logbook upload
stubs, mock spotting, and placeholder map/weather/propagation providers.

Service requests are allowed only when plugin permission, operator role
permission, provider permission/config requirements, enablement, and health all
pass. Shared `ServiceCache` entries are support data, not official log data, not
append-only, expirable, clearable, and not synced by default.

The GUI Service Providers screen lists providers by service type with plugin
source, enabled state, health, priority, online/offline behavior, missing config
warnings, capabilities, and required permissions. Provider config should point
to credential IDs rather than raw secrets.

## Secure Credential Storage

`ham-core` now defines a `CredentialStore` abstraction for provider secrets.
Credential metadata is support/security state; secret values are isolated behind
the credential backend and are never written to official events, runtime logs,
diagnostic reports, or provider config.

The MVP includes an OS keychain placeholder for Windows Credential Manager,
macOS Keychain, and Linux Secret Service, plus an explicit opt-in insecure
development fallback. The fallback is only enabled when:

```powershell
$env:HAM_PLATFORM_ALLOW_INSECURE_DEV_CREDENTIALS = "1"
```

See [Credential Storage](docs/security/credential-storage.md).

## Daily Driver Logging Foundation

The current milestone adds the first daily-logger layer on top of the core and
Unified Service Framework:

- Station/equipment profiles live in support storage and provide logger defaults
  for station callsign, operator callsign, grid, QTH, power, and selected
  equipment references.
- QSO official events may reference `station_profile_id`,
  `station_configuration_id`, and equipment IDs. Historical QSOs are not
  rewritten when support records change.
- The Awards workspace computes DXCC, WAS, POTA, SOTA, and grid progress from
  QSO projections. Deleted QSOs do not count; restored QSOs count after replay.
- Advanced Search reads QSO projections and supports filters such as
  `callsign:K1ABC`, `band:20m`, `mode:FT8`, `date:2026-07-01..2026-07-06`,
  `tag:portable`, and plain text terms.
- Upload queue scaffolding selects projected, visible QSOs, generates ADIF, and
  targets service-framework upload providers such as LoTW/eQSL/Club Log/QRZ
  stubs.
- Keyboard-first logging commands include focus callsign entry, submit QSO,
  clear form, use rig frequency, accept lookup suggestions, open recent QSOs,
  and open advanced search.

Run the GUI with `cargo run -p ham-gui`, open the local URL printed by the
process, choose the Casual Logger workspace, and use the Station Summary,
Callsign Entry, Recent QSOs, Advanced Search, Awards, and Uploads panels.

## Net Control MVP

The Net Control workspace is implemented as a built-in plugin-style workflow.
It submits proposals for net sessions, check-ins, traffic, tombstones, and report
exports. The core validates plugin permissions, operator role permissions, active
session rules, and schemas before appending hash-chained official events.

The workspace includes:

- Net Session Control
- Check-In Entry
- Check-In Roster
- Traffic Queue
- Net Report

Deleted check-ins are tombstone events and are hidden from normal roster
projection views by default. See [Net Control Plugin](docs/plugins/net-control.md).

## Mapping and Propagation Framework

The Maps workspace is now backed by `ham-core::map`, a reusable GIS foundation
for logging, POTA/SOTA, awards, APRS, satellites, weather, propagation, Net
Control, EmComm, and remote-station workflows.

The map consumes core projections and service providers. It does not own
business logic and does not write official log events. QSO markers and paths are
derived from QSO projections, station markers come from station profiles, and
weather/propagation data flows through provider-backed service models.

Implemented foundations:

- Maidenhead grid validation, normalization, encode/decode, bounds, precision,
  and neighbors.
- Great-circle distance, initial/final bearing, midpoint, and path generation.
- Map providers for offline placeholder, OpenStreetMap placeholder, and mock
  data through the Unified Service Framework.
- Map layers for stations, QSOs, routes, POTA, SOTA, grid overlay, grayline,
  propagation, weather, and satellite placeholders.
- Marker models for station, operator, QSO, park, summit, repeater, incident,
  weather, and satellite objects.
- Grayline snapshot model and mock propagation/weather providers.
- Maps workspace panels for Interactive Map, Layers, Selected Object,
  Propagation, Weather, Search, Filters, and Station Summary.

Run `cargo run -p ham-gui`, open the printed local URL, and choose the Maps
workspace. The status bar shows current grid, coordinates, distance, bearing,
zoom, and selected layer.

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

## Plugin Permission Model

Plugins declare requested permissions in their manifest. A requested permission
does not by itself grant access. The core checks both plugin permission grants
and operator role permissions before writing official events or allowing
privileged runtime actions.

Manifest fields include:

- `plugin_id`
- `name`
- `version`
- `author`
- `description`
- `requested_permissions`
- `optional_permissions`
- `contributed_panels`
- `contributed_commands`
- `plugin_type`
- `minimum_core_version`

Permission metadata includes permission ID, category, display name, description,
risk level, built-in default behavior, admin approval requirement, and a
user-visible reason. Risk levels are `low`, `medium`, `high`, and `critical`.

Important security rules:

- External network lookup is separate from offline lookup.
- Diagnostics upload is separate from diagnostics export and log viewing.
- Rig read/state permissions are separate from rig write/PTT permissions.
- Sync LAN/cloud pull and push permissions are separate.
- UI panel registration does not imply data access.
- Unknown manifest permissions are reported as invalid.
- Permission grants are stored in local support/config storage, not the official
  append-only log.

Grant records include plugin ID, permission ID, status (`granted`, `denied`,
`pending`, or `revoked`), optional operator ID, grant time, reason, scope, and
future expiry. The MVP assumes the local operator is an admin for approval UI,
but the core enforcement interface already separates plugin grants from operator
role checks.

The GUI Plugin Manager shows installed plugins, requested permissions, optional
permissions, risk levels, grant status, contributed panels/commands, and actions
to grant, deny, revoke, or approve low-risk permissions. High and critical
permissions remain visibly risky and are not silently auto-granted. Denied
actions return a clear error to the UI and publish runtime permission events.

Runtime permission events include:

- `plugin.permission.requested`
- `plugin.permission.granted`
- `plugin.permission.denied`
- `plugin.permission.revoked`
- `plugin.permission.check.allowed`
- `plugin.permission.check.denied`
- `plugin.manifest.loaded`
- `plugin.manifest.invalid`
- `plugin.disabled.permission_missing`

Core enforcement currently covers QSO proposals, activation proposals, ADIF
import/export, LAN/cloud sync actions, diagnostics export/upload, rig control
commands, and callsign lookup/cache actions. A denied proposal never appends an
official event, so the hash-chained official log remains protected by the core
validator.

Current MVP limitations: plugin loading is still static, plugins are not signed,
there is no sandbox, grant scopes are mostly recorded rather than enforced, and
organization-managed policy is future work. Future work includes signed plugins,
runtime sandboxing, scoped grants, marketplace review, and managed policies.

## Rig Control And Frequency Autofill Plugin

`plugin.rig-control` provides the first radio/device integration layer. Rig
state is advisory runtime data: it can populate logger forms, but it never writes
official QSO events directly. Submitted QSOs still go through the normal
`proposal.qso.create` validation path.

Provider architecture:

- `RigProvider` defines list/connect/disconnect/state subscription and control
  commands for frequency, mode, and PTT.
- `MockRigProvider` is enabled for GUI/dev/tests and requires no hardware.
- `HamlibProviderStub` documents the Hamlib integration point without requiring
  Hamlib to be installed for builds or CI.

Current plugin permissions:

- `rig.view`
- `rig.control.frequency`
- `rig.control.mode`
- `rig.control.ptt`
- `rig.control.split`
- `rig.read.state`
- `rig.configure`
- `log.qso.suggest_fields`

Runtime rig events include:

- `rig.provider.loaded`
- `rig.connect.started`
- `rig.connect.succeeded`
- `rig.connect.failed`
- `rig.disconnected`
- `rig.state.changed`
- `rig.frequency.changed`
- `rig.mode.changed`
- `rig.ptt.changed`
- `rig.command.sent`
- `rig.command.failed`
- `rig.autofill.suggestion.created`

The core includes a band inference helper for common HF/VHF/UHF amateur bands:
160m, 80m, 60m, 40m, 30m, 20m, 17m, 15m, 12m, 10m, 6m, 2m, 1.25m, and 70cm.
Regional band-plan validation is future work.

Logger workflow:

1. Open Casual Logger or POTA/SOTA.
2. Use the Rig Control panel to connect the mock rig.
3. Optionally adjust the mock frequency/mode and apply the mock state.
4. Click `Refresh Rig` or `Use Rig Frequency/Mode` in the logger.
5. Submit the QSO. Accepted rig frequency, band, mode, submode, and safe source
   metadata flow through the normal QSO proposal payload.

The Rig Control settings section exposes MVP placeholders for enablement,
default provider, polling interval, auto-fill behavior, Hamlib host/port, and
serial CAT settings. The status bar shows the active rig frequency/mode when a
rig is connected.

Current limitations: Hamlib is a build-safe stub, serial CAT and TCP CAT are not
implemented, and multiple rigs are modeled but the GUI uses one active rig for
autofill. Future work includes full Hamlib support, serial/TCP CAT, rotors,
amplifiers, tuners, SO2R, and band-plan validation.

## Diagnostic Report Bundles

The app can create tester-friendly diagnostic reports without including official
QSO history by default. Reports are ZIP files that can be saved locally or
uploaded to an authenticated support endpoint on the sync server.

Supported report types:

- Basic report: runtime logs, app/core versions, platform/system info, enabled
  plugins, recent errors, action timeline, redaction report, and user notes.
- Sync report: Basic report plus sync status, peer/discovery summary, LAN/cloud
  sync state, and divergence warnings.

Bundle structure:

```text
ham-report-<timestamp>-<short-id>.zip
manifest.json
runtime-events.jsonl
runtime-events.jsonl.1..5 when present
system-info.json
app-info.json
plugins.json
sync-status.json for Sync reports
action-timeline.json
redaction-report.json
user-notes.txt
```

The manifest includes report format version, report type, generated time,
app/core versions, platform, device/session IDs, safe account ID when paired,
included files, bundle hash, and redaction summary.

Privacy behavior:

- Official QSO logs are not included by default.
- Credentials, API keys, passwords, session tokens, and sync tokens are redacted.
- Private profile/address fields are redacted.
- Full AI prompts/responses are excluded by default.
- Raw provider lookup metadata is redacted/excluded by default.
- The redaction report describes removed categories without exposing removed
  values.

The action timeline includes recent important runtime events, usually from the
last 15 minutes, with timestamp, event type, severity, source, plugin ID,
correlation ID, workspace, payload summary, and error summary. It favors concise
event summaries over full payloads.

GUI workflow:

1. Open Diagnostics and click `Report a Problem`, or use the command palette.
2. Choose `Basic` or `Sync`.
3. Add a short description and user notes.
4. Click `Preview` to inspect included files and redaction summary.
5. Click `Export ZIP` to save a local bundle. Native file dialogs are not wired
   yet, so the MVP uses a typed output path.
6. Click `Upload Report` to send the bundle to the support endpoint. Upload
   requires cloud sync pairing/authentication first.

Authenticated upload uses `POST /api/v1/reports` on the sync server. The server
validates the sync token, stores report metadata and bundle bytes in the MVP
in-memory backend, and returns a `report_id`, status, received time, and bundle
hash. Report status starts as `submitted`; future support tooling can move it
through `triaged`, `investigating`, `waiting_on_user`, `fixed`, and `closed`.

Runtime report events include:

- `diagnostics.report.started`
- `diagnostics.bundle.created`
- `diagnostics.redaction.completed`
- `diagnostics.export.started`
- `diagnostics.export.completed`
- `diagnostics.upload.started`
- `diagnostics.upload.completed`
- `diagnostics.upload.failed`

Current limitations: uploaded report storage is in-memory in the MVP sync
server, screenshots are not attached, report status tracking has no dashboard,
and official log excerpts are intentionally deferred until there is an explicit
user-approved workflow.

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
