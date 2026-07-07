# Project Status

Current milestone: Daily Driver Logging foundation

Current version: 0.1.0

Last update timestamp: 2026-07-06T19:49:27.7782842-04:00

Repository health status: Healthy. Formatting, Rust check, Clippy with warnings denied, full workspace tests, GUI JavaScript syntax check, and release build passed during this session.

---

# Completed Features

## Core

- [x] Typed event bus
- [x] Runtime event model, filtering, replay, and JSONL log rotation
- [x] Official event envelope and deterministic SHA-256 hashing
- [x] Append-only official event store trait
- [x] In-memory and durable JSONL official event stores
- [x] Proposal validation pipeline
- [x] QSO create/correct/delete/restore/note proposals
- [x] Activation proposals and activation/QSO linking
- [x] QSO and activation projections
- [x] ADIF import/export and duplicate detection
- [x] Callsign lookup, grid, and entity helpers
- [x] Rig model, mock rig, and band inference
- [x] Diagnostic bundle generation and report upload models
- [x] Typed permission registry and grant checks
- [x] Unified service/provider registry
- [x] Shared service cache and provider fallback
- [x] Upload provider skeletons
- [x] Spotting provider skeleton and mock spots
- [x] Map/weather/propagation provider skeletons
- [x] Station/equipment profile models and JSON support storage
- [x] Award engine foundation
- [x] Projection-backed advanced search parser
- [x] Upload queue foundation and official upload status event constants
- [ ] Conflict resolution UI/model
- [ ] Cryptographic signatures
- [ ] Durable projection cache

## Plugin SDK

- [x] Plugin manifest model
- [x] Plugin capability/permission model
- [x] Proposal envelope
- [x] Public proposal and official event constants
- [x] Service type vocabulary for provider contributions
- [x] Station/equipment and upload queue permissions
- [ ] Real plugin loading
- [ ] Plugin sandboxing
- [ ] Signed plugin packages

## GUI

- [x] Workspace shell and static panel registry
- [x] Command palette
- [x] Settings screen
- [x] Plugin Manager permission UI
- [x] Service Providers screen
- [x] Event Bus Monitor
- [x] Casual Logger QSO form
- [x] Recent QSOs panel
- [x] POTA/SOTA workspace panels
- [x] Sync Status panel
- [x] Rig Control panel
- [x] Diagnostic Report workflow
- [x] Station Summary, Station Profiles, and Equipment Manager panels
- [x] Awards workspace and award summary panel
- [x] Advanced Search panel
- [x] Uploads panel
- [x] Keyboard-first logging command foundation
- [ ] Interactive dockable panel movement
- [ ] Native file dialogs
- [ ] Full Tauri packaging

## Sync

- [x] LAN discovery packet model
- [x] IPv4/IPv6 multicast defaults
- [x] Peer registry
- [x] Handshake models
- [x] Head comparison
- [x] Safe preview pull
- [x] Safe pull missing events
- [x] Cloud/self-hosted sync API models
- [x] Pairing-token MVP auth
- [x] Cloud push/pull/preview
- [x] Self-hosted sync server binary
- [x] Dockerfile for sync server
- [ ] Real LAN peer-to-peer HTTP transport
- [ ] Trust pairing UX
- [ ] Durable sync server storage

## Plugins and Plugin-Like Features

- [x] POTA/SOTA activation workflow
- [x] Callsign lookup/enrichment workflow
- [x] Rig control foundation
- [x] Diagnostics report workflow
- [x] Sync controls
- [x] Unified service providers for lookup/upload/spotting/map/weather/propagation skeletons
- [x] Station/equipment profile support
- [x] Awards foundation
- [x] Upload queue foundation
- [ ] Real LoTW/eQSL/Club Log/QRZ providers
- [ ] Net Control
- [ ] EmComm
- [ ] Contesting
- [ ] Maps/propagation
- [ ] AI assistant enforcement model

## Tooling and Release

- [x] `justfile`
- [x] CI workflow
- [x] Tagged release workflow
- [x] Cross-platform artifact packaging
- [x] Self-hosted sync Dockerfile
- [ ] Docs link checker
- [ ] Coverage reporting

---

# Current Architecture

The repository implements a local-first Rust workspace centered on `ham-core`. The core owns official events, proposal validation, runtime diagnostics, projections, ADIF, lookup helpers, rig helpers, diagnostics, permissions, the unified service framework, station/equipment support models, award computation, projection search, upload queue logic, and durable JSONL official storage.

The plugin system is currently static. Plugins are represented by manifests, requested permissions, optional permissions, contributed services, contributed panels, commands, and validation paths. Real runtime loading and sandboxing remain planned.

The unified service framework provides provider metadata, registry, selection, fallback, shared service cache, service authorization, and typed service request/response models for lookup, upload, spotting, map, weather, propagation, and future integrations.

Station/equipment data is support/config state, not official event state. QSO official event payloads may reference profile/config/equipment IDs, but profile changes do not rewrite historical QSOs.

The award engine computes rebuildable progress from QSO projections. Search also reads projections rather than raw official event storage. Upload jobs select projected QSOs and generate ADIF for service-framework upload providers.

Sync is split between `ham-sync` for protocol, peer, LAN/cloud models, safe replication, and in-memory server logic, plus `ham-sync-server` for the self-hosted HTTP-like server binary. LAN is preferred. Cloud/self-hosted sync is a fallback and uses the same verification rules.

Storage uses append-only JSONL official events for the MVP. Runtime logs are separate rotating JSONL files. Support/config state uses lightweight JSON or in-memory state depending on subsystem maturity.

The GUI is `ham-gui`, a web-first shell served by Rust. It is Tauri-ready but not yet a packaged Tauri app. It consumes core bridge APIs, projections, runtime events, service framework state, and proposal endpoints.

Permissions are centralized in `ham-core::permissions`. Protected actions require plugin permission and operator role permission. Scope support is modeled but not fully enforced everywhere yet.

Diagnostics include runtime event logs, redaction helpers, report ZIP generation, action timelines, and authenticated upload through the sync server.

---

# Current Workspace Structure

- `crates/ham-core`: event bus, official events, stores, proposals, projections, ADIF, lookup, rig, diagnostics, permissions, service framework, station profiles, awards, search, upload queue.
- `crates/ham-plugin-sdk`: plugin manifest, capabilities, service types, proposal envelope, public event constants.
- `crates/ham-sync`: LAN discovery/handshake models, peer registry, safe replication, cloud API/client/server models, report upload models.
- `crates/ham-sync-server`: self-hosted sync/report HTTP-like server binary.
- `crates/ham-gui`: Rust bridge/server, GUI shell models, command registry, static web UI.
- `crates/ham-cli`: CLI commands for ADIF and chain/projection operations.
- `docs/architecture`: service framework, station profiles, award engine, search, and upload queue architecture notes.
- `docs/plugins`: provider development guide.
- `docs/security`: credential and redaction guidance.
- `justfile`: local development commands aligned with CI.
- `ROADMAP.md`: root milestone roadmap.

---

# Outstanding TODOs

## Critical

- [ ] Add durable storage to the self-hosted sync/report server before real hosted use.
- [ ] Implement trust pairing/authentication for LAN peers before unattended sync.
- [ ] Add secure credential storage before real online upload/lookup providers.

## High

- [ ] Persist upload queue state and provider settings.
- [ ] Add real LoTW/eQSL/Club Log/QRZ upload providers through the service framework.
- [ ] Add QRZ/HamQTH real lookup providers using secure credential storage.
- [ ] Enforce permission scopes consistently across account, logbook, and station boundaries.
- [ ] Add role/account/session models and UI beyond the MVP local-admin assumption.
- [ ] Implement real LAN peer-to-peer transport for replication endpoints.
- [ ] Add conflict/divergence review UX.

## Medium

- [ ] Add station profile and equipment create/edit GUI forms.
- [ ] Add full award rule databases and needed-list computation.
- [ ] Add saved-search GUI persistence flow.
- [ ] Extract support/cache storage behind a durable abstraction.
- [ ] Add native file dialogs for import/export/report bundles.
- [ ] Add Tauri packaging.
- [ ] Add projection cache persistence and startup optimization.

## Low

- [ ] Add richer visual polish and saved layouts.
- [ ] Add coverage reporting.
- [ ] Add screenshot attachment support for diagnostic reports.

---

# Known Technical Debt

- Several blueprint-recommended crates are currently implemented as modules inside `ham-core`; this is acceptable for MVP but should be revisited as APIs grow.
- The GUI is a static web shell served by Rust rather than a packaged Tauri desktop app.
- Plugin loading is static; no sandbox, signature verification, or process isolation exists yet.
- The sync server uses in-memory storage for cloud sync events and uploaded reports.
- LAN discovery/replication has strong models but needs real multi-instance transport wiring.
- Permission scopes are mostly recorded rather than fully enforced.
- Service provider enablement and priority are currently in-memory GUI/server state.
- Upload queue state is currently in-memory in the GUI.
- Station profile editing exists in core models but not yet as full GUI forms.
- External provider implementations are stubs until credential storage and live integrations are added.
- JavaScript UI behavior has limited automated test coverage.

---

# Known Bugs

- No reproducible bugs are currently documented in this status file.

---

# Breaking Changes

- Added new SDK permission variants and official upload status event constants. Existing source using exhaustive `PluginCapability` matches may need to handle the new variants.

---

# Performance Improvements

- Runtime diagnostics use rotating logs to bound disk usage.
- Official projections are rebuildable; projection cache persistence is a future startup optimization.
- Service provider selection is in-memory and cheap.
- Advanced search is projection-scan based for MVP; future large logbooks should add indexing or persisted projection tables.

---

# Security Notes

- Official event writes are centralized through proposal validation.
- Runtime diagnostics redact secret-like fields before persistence/report export.
- Diagnostic upload requires an authenticated sync token.
- Cloud sync uses MVP pairing-token auth; stronger account/device auth is future work.
- LAN peers are treated as untrusted; automatic merge is not allowed.
- Plugin permissions are checked with operator role permissions before protected actions.
- Service requests also check provider-required permissions and operator role permissions.
- Upload/network permissions are separate from ADIF export and status viewing.
- Provider config schemas are present, but credential values are not stored until secure secret storage is added.

---

# Documentation Status

- README: updated with daily-driver foundation and links.
- Root ROADMAP: present and updated.
- Master Blueprint: complete local repository copy.
- Event Catalog: updated for upload status event constants and daily-driver runtime event names.
- Plugin SDK: updated for new station/upload permissions and static plugin-like foundations.
- Sync Protocol: current for LAN/cloud foundation.
- Security Model: broadly current; future update should add a dedicated upload queue permission example.
- Service Framework: current for provider architecture.
- Station Profiles: complete initial architecture note.
- Award Engine: complete initial architecture note.
- Advanced Search: complete initial architecture note.
- Upload Queue: complete initial architecture note.
- Provider Development: complete initial provider author guide.
- Credentials and Redaction: complete initial credential handling guidance.
- Developer Guide: current local workflow; daily-driver examples could be expanded.
- API docs: Rust public item docs are partial and should be improved as APIs stabilize.

---

# Test Coverage

- Core event hashing, chain verification, QSO proposals, projections, ADIF, lookup, rig, diagnostics, permissions, service framework, station profiles, awards, search, upload queue, and sync models have unit coverage.
- GUI model serialization and command/panel foundations have partial coverage.
- JavaScript UI behavior is mostly manually verified and should gain browser-level tests.
- Current test run: `cargo test --workspace` passed with 126 total Rust tests across crates.

---

# Current Milestone

Current objective: make the application useful enough for a normal operator to seriously test as a daily logger.

Completed work:

- Station/equipment profile models and default application to QSO proposals.
- Award engine foundation with DXCC/WAS and POTA/SOTA/Grid placeholders.
- Projection-backed search parser and search execution.
- Upload queue foundation and ADIF job generation.
- GUI panels/workspace/commands for station, awards, search, uploads, and keyboard-first logging.

Remaining work:

- Add real provider integrations and durable provider/upload settings.

Expected completion criteria:

- All required quality gates pass.
- Documentation and project state are updated.
- Daily-driver workflows are accessible in the GUI without bypassing proposal validation.

---

# Recommended Next Milestone

Real online service integrations:

- LoTW real upload/download.
- eQSL real upload.
- Club Log real upload.
- QRZ Logbook upload.
- QRZ/HamQTH real lookup.
- Provider credential storage through OS keychain/secret store.

This milestone should come before deeper award/confirmation work because confirmation and upload status need real provider identities, durable credentials, and provider result semantics.

---

# Changelog

## 2026-07-06

Summary: Added Daily Driver Logging foundation with station/equipment profiles, award engine, advanced search, upload queue, and keyboard-first GUI logging commands.

Major files changed:

- `crates/ham-core/src/station.rs`
- `crates/ham-core/src/awards.rs`
- `crates/ham-core/src/search.rs`
- `crates/ham-core/src/upload.rs`
- `crates/ham-gui/src/main.rs`
- `crates/ham-gui/src/shell.rs`
- `crates/ham-gui/src/commands.rs`
- `crates/ham-gui/web/app.js`
- `crates/ham-gui/web/index.html`
- `crates/ham-plugin-sdk/src/lib.rs`
- `README.md`
- `ROADMAP.md`
- `docs/architecture/station-profiles.md`
- `docs/architecture/award-engine.md`
- `docs/architecture/search.md`
- `docs/architecture/upload-queue.md`

Architectural decisions:

- Station/equipment data is support/config state, with immutable references copied into QSO official event payloads when submitted.
- Awards and search compute from projections, not official event storage directly.
- Upload queue uses the Unified Service Framework and ADIF generated from projected visible QSOs.
- Upload status can be represented as official append-only events without mutating QSO records.

New plugins:

- No new runtime-loaded plugin was added. Station, awards, search, and upload queue are implemented as core-supported plugin-like foundations until real plugin loading exists.

Breaking changes:

- New SDK permissions and official upload event constants were added; exhaustive downstream matches may need updates.
