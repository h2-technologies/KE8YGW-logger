# Project Status

Current milestone: Online Services Ecosystem

Current version: 0.1.0

Last update timestamp: 2026-07-06T21:33:00-04:00

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
- [x] Secure credential storage abstraction and explicit dev fallback
- [x] Net Control official events, proposals, validation, projection, and report export
- [x] GIS coordinate, bounds, path, polygon, layer, marker, and overlay models
- [x] Maidenhead grid validation, normalization, encode/decode, bounds, precision, and neighbors
- [x] Great-circle distance, bearing, midpoint, and path generation
- [x] QSO/station map projection helpers
- [x] Grayline snapshot model
- [x] Mock weather and propagation models
- [x] Online Services provider metadata for logbooks, lookups, spotting, propagation, weather, and maps
- [x] Upload engine retry, statistics, and execution result models
- [x] Confirmation download model and official append-only confirmation status event path
- [x] DX Cluster parser and POTA/SOTA spot normalization
- [x] Online automation task and notification models
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
- [x] Credential and Net Control permissions/event constants
- [x] Online network, automation, and notification permissions
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
- [x] Credential Manager screen/panel
- [x] Net Control workspace panels
- [x] Maps workspace panels
- [x] Map status bar fields
- [x] Online Services workspace panels
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
- [x] Net Control MVP
- [ ] Real LoTW/eQSL/Club Log/QRZ providers
- [x] Net Control MVP
- [x] Maps/propagation framework
- [x] Online Services ecosystem foundation
- [ ] EmComm
- [ ] Contesting
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

The unified service framework provides provider metadata, registry, selection, fallback, shared service cache, service authorization, and typed service request/response models for lookup, upload, spotting, map, weather, propagation, online services, and future integrations.

Credential storage is isolated behind `CredentialStore`. Credential metadata is support/security state and secret values stay behind the selected backend. The MVP has an OS keychain placeholder and an explicit opt-in insecure development file fallback.

Station/equipment data is support/config state, not official event state. QSO official event payloads may reference profile/config/equipment IDs, but profile changes do not rewrite historical QSOs.

The award engine computes rebuildable progress from QSO projections. Search also reads projections rather than raw official event storage. Upload jobs select projected QSOs and generate ADIF for service-framework upload providers.

Net Control is a plugin-style workflow using the normal proposal pipeline. Net sessions, check-ins, traffic, tombstones, and report exports are official append-only events. `NetControlProjection` derives current roster/session/report state.

The mapping framework is implemented as a core GIS/service layer. `ham-core::map` owns coordinate, grid, distance, bearing, layer, marker, grayline, weather, and propagation models. Maps consume QSO projections, station profiles, and service providers; they do not own official event writes or business workflows.

The Online Services ecosystem is implemented as a provider-backed service layer. `ham-core::online` owns connected provider metadata, upload/download engine models, confirmation records, DX/POTA/SOTA spot normalization, provider health states, automation tasks, notifications, and safe cache helpers. Live network adapters remain behind provider boundaries and must use `CredentialStore`.

Sync is split between `ham-sync` for protocol, peer, LAN/cloud models, safe replication, and in-memory server logic, plus `ham-sync-server` for the self-hosted HTTP-like server binary. LAN is preferred. Cloud/self-hosted sync is a fallback and uses the same verification rules.

Storage uses append-only JSONL official events for the MVP. Runtime logs are separate rotating JSONL files. Support/config state uses lightweight JSON or in-memory state depending on subsystem maturity.

The GUI is `ham-gui`, a web-first shell served by Rust. It is Tauri-ready but not yet a packaged Tauri app. It consumes core bridge APIs, projections, runtime events, service framework state, and proposal endpoints.

The Maps workspace consumes `/api/maps/state` and `/api/maps/layer/toggle`. It displays derived QSO/station map objects, layer state, selected-object context, grayline, propagation, weather, and map status fields.

The Online Services workspace consumes `/api/online-services`. It displays accounts, providers, upload queue stats, confirmation downloads, DX/POTA/SOTA spots, weather, propagation, provider health, credential manager, service cache, automation, and notifications.

Permissions are centralized in `ham-core::permissions`. Protected actions require plugin permission and operator role permission. Scope support is modeled but not fully enforced everywhere yet.

Diagnostics include runtime event logs, redaction helpers, report ZIP generation, action timelines, and authenticated upload through the sync server.

---

# Current Workspace Structure

- `crates/ham-core`: event bus, official events, stores, proposals, projections, ADIF, lookup, rig, diagnostics, permissions, service framework, online services, credential storage, Net Control, station profiles, awards, search, upload queue.
- `crates/ham-core::credential`: credential metadata, store abstraction, OS placeholder backend, explicit insecure development fallback.
- `crates/ham-core::map`: GIS models, Maidenhead grid engine, great-circle engine, map layers, markers, provider metadata, grayline, propagation, and weather models.
- `crates/ham-core::net`: Net Control projection, session/check-in/traffic models, and report export.
- `crates/ham-core::online`: online provider metadata, upload/download engine models, confirmation records, spot parsing, automation, notification, and provider health helpers.
- `crates/ham-plugin-sdk`: plugin manifest, capabilities, service types, proposal envelope, public event constants.
- `crates/ham-sync`: LAN discovery/handshake models, peer registry, safe replication, cloud API/client/server models, report upload models.
- `crates/ham-sync-server`: self-hosted sync/report HTTP-like server binary.
- `crates/ham-gui`: Rust bridge/server, GUI shell models, command registry, static web UI.
- `crates/ham-cli`: CLI commands for ADIF and chain/projection operations.
- `docs/architecture`: service framework, online services, station profiles, award engine, search, and upload queue architecture notes.
- `docs/plugins`: provider and online provider development guides.
- `docs/maps`, `docs/grid-system`, `docs/propagation`, `docs/weather`, `docs/plugin-map-providers`: mapping and provider framework documentation.
- `docs/security`: credential and redaction guidance.
- `justfile`: local development commands aligned with CI.
- `ROADMAP.md`: root milestone roadmap.

---

# Outstanding TODOs

## Critical

- [ ] Add durable storage to the self-hosted sync/report server before real hosted use.
- [ ] Implement trust pairing/authentication for LAN peers before unattended sync.
- [ ] Implement production OS keychain backends before real online upload/lookup provider credentials.

## High

- [ ] Persist upload queue state and provider settings.
- [ ] Add real LoTW/eQSL/Club Log/QRZ upload providers through the service framework.
- [ ] Add QRZ/HamQTH real lookup providers using secure credential storage.
- [ ] Enforce permission scopes consistently across account, logbook, and station boundaries.
- [ ] Add role/account/session models and UI beyond the MVP local-admin assumption.
- [ ] Implement real LAN peer-to-peer transport for replication endpoints.
- [ ] Add conflict/divergence review UX.
- [ ] Add real tile/geocoding/weather/propagation providers through the map service framework.
- [ ] Implement live network adapters for LoTW/eQSL/Club Log/QRZ/HRDLog uploads and confirmations.
- [ ] Implement live QRZ XML, HamQTH, FCC ULS, DX Cluster, RBN, POTA, SOTAWatch, NOAA, Open-Meteo, and OSM adapters.
- [ ] Replace the map preview shell with a full interactive tile/vector renderer.

## Medium

- [ ] Add station profile and equipment create/edit GUI forms.
- [ ] Add full award rule databases and needed-list computation.
- [ ] Add saved-search GUI persistence flow.
- [ ] Extract support/cache storage behind a durable abstraction.
- [ ] Add native file dialogs for import/export/report bundles.
- [ ] Add Tauri packaging.
- [ ] Add projection cache persistence and startup optimization.
- [ ] Persist map layer ordering and user map preferences.
- [ ] Persist upload history, provider account state, automation tasks, and notification read state.

## Low

- [ ] Add richer visual polish and saved layouts.
- [ ] Add coverage reporting.
- [ ] Add screenshot attachment support for diagnostic reports.
- [ ] Add terrain/elevation, APRS, satellite, and radar overlays.

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
- External provider implementations are stubs until production credential storage and live integrations are added.
- Online Services has production-shaped provider metadata and parsers, but live network clients are not enabled yet.
- Automation tasks are modeled but not executed by a durable scheduler.
- Confirmation downloads append official events, but provider-specific matching to historical QSOs needs deeper reconciliation.
- OS keychain support is represented by an unavailable placeholder; real native backends are still required.
- The insecure dev credential fallback is plaintext and only suitable for local testing when explicitly enabled.
- Net Control template create/edit UI is not complete; sessions/check-ins/traffic/report are implemented first.
- The Maps workspace currently renders a structured preview rather than full tile/vector map rendering.
- Map provider enablement/layer ordering is in-memory in the GUI process.
- GIS bounds do not yet include antimeridian-aware clipping.
- JavaScript UI behavior has limited automated test coverage.

---

# Known Bugs

- No reproducible bugs are currently documented in this status file.

---

# Breaking Changes

- Added new SDK permission variants for online network access, automation, and notifications. Existing source using exhaustive `PluginCapability` matches may need to handle the new variants.

---

# Performance Improvements

- Runtime diagnostics use rotating logs to bound disk usage.
- Official projections are rebuildable; projection cache persistence is a future startup optimization.
- Service provider selection is in-memory and cheap.
- Advanced search is projection-scan based for MVP; future large logbooks should add indexing or persisted projection tables.
- Online provider health/cache/dashboard aggregation is in-memory and cheap; future live adapters should add rate-limit-aware scheduling.

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
- Provider config schemas reference credential IDs; raw credential values stay behind `CredentialStore`.
- Online Services separates external network permissions by upload, lookup, spotting, map, weather, and propagation.
- Downloaded confirmations append official status events instead of mutating QSO records.
- Native credential storage needs Windows/macOS/Linux backend implementations before real external credentials are safe for production use.

---

# Documentation Status

- README: updated with online services foundation and links.
- Root ROADMAP: present and updated.
- Master Blueprint: complete local repository copy.
- Event Catalog: updated for online service runtime event names.
- Plugin SDK: updated for online network, automation, and notification permissions.
- Sync Protocol: current for LAN/cloud foundation.
- Security Model: broadly current; future update should add a dedicated upload queue permission example.
- Service Framework: current for provider architecture.
- Station Profiles: complete initial architecture note.
- Award Engine: complete initial architecture note.
- Advanced Search: complete initial architecture note.
- Upload Queue: complete initial architecture note.
- Provider Development: complete initial provider author guide.
- Credentials and Redaction: current.
- Credential Storage: complete MVP architecture note.
- Net Control Plugin: complete MVP plugin/workflow note.
- Maps: complete initial architecture note.
- Grid System: complete initial Maidenhead/great-circle note.
- Propagation: complete initial provider/model note.
- Weather: complete initial provider/model note.
- Plugin Map Providers: complete initial provider author guide.
- Online Services: complete initial architecture note.
- Online Provider Development: complete initial provider author guide.
- Developer Guide: current local workflow; daily-driver examples could be expanded.
- API docs: Rust public item docs are partial and should be improved as APIs stabilize.

---

# Test Coverage

- Core event hashing, chain verification, QSO proposals, projections, ADIF, lookup, rig, diagnostics, permissions, service framework, credential store, Net Control, station profiles, awards, search, upload queue, sync models, GIS models, grid conversion, great-circle math, map layers, marker serialization, provider metadata, grayline calculations, online provider metadata, retry logic, confirmation parsing, spot parsing, cache stats, and notification models have unit coverage.
- GUI model serialization and command/panel foundations have partial coverage.
- JavaScript UI behavior is mostly manually verified and should gain browser-level tests.
- Current test run: `cargo test --workspace` passed with 156 total Rust tests across crates.

---

# Current Milestone

Current objective: implement the Online Services ecosystem foundation.

Completed work:

- Connected provider metadata for LoTW, eQSL, Club Log, QRZ Logbook, HRDLog, QRZ XML, HamQTH, FCC ULS, prefix fallback, DX Cluster, RBN, POTA, SOTAWatch, NOAA Space Weather, NOAA Weather, Open-Meteo, OpenStreetMap, offline tile cache, and reverse geocoder.
- Upload engine retry policy, execution result, upload stats, provider health, and notification foundation.
- Confirmation download model and official append-only confirmation status event path.
- DX Cluster parser and POTA/SOTA spot normalization into the shared spot model.
- Online automation task and notification models.
- Online Services workspace and `/api/online-services` dashboard.

Remaining work:

- Implement live network adapters and credential validation for the registered providers.
- Add durable upload history, account settings, scheduler state, and notification read state.
- Add browser-level GUI tests for online service interactions.

Expected completion criteria:

- All required quality gates pass.
- Documentation and project state are updated.
- Online services use the Unified Service Framework, CredentialStore, permissions, runtime events, and official append-only confirmation events.

---

# Recommended Next Milestone

Live Provider Adapters and Production Credential Backends:

- Native OS keychain/secret-store credential backends.
- Live QRZ XML and HamQTH lookup clients.
- Live LoTW/eQSL/Club Log/QRZ/HRDLog upload and confirmation clients.
- DX Cluster Telnet background client, POTA spots, SOTAWatch, and RBN adapters.
- NOAA/Open-Meteo/space-weather live providers.
- Durable scheduler execution for automatic uploads/downloads/refreshes.

This milestone should come next because the provider metadata, credential references, upload/download models, and GUI surfaces are now ready for live authenticated adapters.

---

# Changelog

## 2026-07-06

Summary: Added Online Services ecosystem foundation with connected provider metadata, upload/download engine models, confirmation import events, DX/POTA/SOTA spot normalization, provider health, automation, notifications, permissions, workspace panels, and documentation.

Major files changed:

- `crates/ham-core/src/online.rs`
- `crates/ham-core/src/lib.rs`
- `crates/ham-core/src/service.rs`
- `crates/ham-core/src/permissions.rs`
- `crates/ham-plugin-sdk/src/lib.rs`
- `crates/ham-gui/src/main.rs`
- `crates/ham-gui/src/shell.rs`
- `crates/ham-gui/src/commands.rs`
- `crates/ham-gui/web/app.js`
- `crates/ham-gui/web/index.html`
- `README.md`
- `ROADMAP.md`
- `docs/architecture/online-services.md`
- `docs/plugins/online-provider-development.md`
- `docs/security/credential-storage.md`

Architectural decisions:

- Online Services is a provider-backed service layer, not a set of direct GUI integrations.
- Live network providers must use `CredentialStore` credential IDs and provider-specific permissions.
- Downloaded confirmations append official status events; they do not mutate QSO records directly.
- Automation and notifications are support/runtime state, not official log state.

New plugins:

- `plugin.online-services` built-in/static provider ecosystem plugin.

Breaking changes:

- New SDK permission variants were added for external map/weather/propagation network access, automation, and notifications.

## 2026-07-06

Summary: Added Mapping and Propagation Framework foundation with GIS models, Maidenhead and great-circle engines, provider-backed map state, layers, markers, grayline, mock propagation/weather, Maps workspace panels, and documentation.

Major files changed:

- `crates/ham-core/src/map.rs`
- `crates/ham-core/src/lib.rs`
- `crates/ham-core/src/service.rs`
- `crates/ham-gui/src/main.rs`
- `crates/ham-gui/src/shell.rs`
- `crates/ham-gui/src/commands.rs`
- `crates/ham-gui/web/app.js`
- `crates/ham-gui/web/index.html`
- `crates/ham-gui/web/styles.css`
- `README.md`
- `ROADMAP.md`
- `docs/maps/README.md`
- `docs/grid-system/README.md`
- `docs/propagation/README.md`
- `docs/weather/README.md`
- `docs/plugin-map-providers/README.md`

Architectural decisions:

- The map is a core service consumer, not a business-logic owner.
- QSO and station visualization derive from projections/support state.
- Map, weather, and propagation providers register through the Unified Service Framework.
- Full tile/vector rendering is deferred; the MVP exposes structured map state and a GUI shell.

New plugins:

- No runtime-loaded plugin was added. `plugin.maps`, `plugin.weather`, and `plugin.propagation` are static plugin-style manifests contributing panels and services.

Breaking changes:

- No breaking public API changes are expected beyond newly exported map/GIS types and service capabilities.

## 2026-07-06

Summary: Added Secure Credential Storage foundation and Net Control MVP.

Major files changed:

- `crates/ham-core/src/credential.rs`
- `crates/ham-core/src/net.rs`
- `crates/ham-core/src/proposal.rs`
- `crates/ham-core/src/permissions.rs`
- `crates/ham-core/src/service.rs`
- `crates/ham-plugin-sdk/src/lib.rs`
- `crates/ham-gui/src/main.rs`
- `crates/ham-gui/src/shell.rs`
- `crates/ham-gui/src/commands.rs`
- `crates/ham-gui/src/mock.rs`
- `crates/ham-gui/web/app.js`
- `README.md`
- `ROADMAP.md`
- `docs/security/credential-storage.md`
- `docs/plugins/net-control.md`

Architectural decisions:

- Credential metadata is support/security state; secrets are accessible only through `CredentialStore`.
- Production OS keychain support is modeled but not linked yet; insecure local file storage requires explicit opt-in.
- Net Control is implemented as a plugin-style workflow using proposals and official append-only events.
- Net check-in deletion is a tombstone event hidden by projections by default.

New plugins:

- `plugin.net-control` built-in/static MVP plugin.

Breaking changes:

- New SDK permission variants, proposal constants, and official event constants were added; exhaustive downstream matches may need updates.

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

## 2026-07-06

Summary: Pushed the Online Services ecosystem foundation and fixed the GUI logger callsign entry clearing during runtime event refreshes.

Major files changed:

- `crates/ham-gui/web/app.js`
- `PROJECT_STATE.md`

Architectural decisions:

- The Casual Logger form now keeps an explicit in-progress QSO draft in GUI state so diagnostic event-bus refreshes do not destroy operator-entered fields.
- Runtime event polling skips full shell re-rendering while logger forms are focused; the event stream catches up after the operator leaves or submits the form.
- Rig autofill suggestions update the QSO draft, but manually typed QSO fields remain authoritative during proposal submission.

New plugins:

- None.

Breaking changes:

- None.

## 2026-07-07

Summary: Fixed Casual Logger QSO submission visibility, settings scrolling, kHz frequency entry, and added client-side workspace card close/reopen/move controls.

Major files changed:

- `crates/ham-core/src/permissions.rs`
- `crates/ham-gui/web/app.js`
- `crates/ham-gui/web/styles.css`
- `README.md`
- `PROJECT_STATE.md`

Architectural decisions:

- Built-in safe default permissions now self-heal from stale pending states on startup, while explicit denied/revoked permissions remain respected.
- Logger and net frequency inputs are presented in kHz at the GUI boundary and converted to `frequency_hz` before proposals reach the core.
- Workspace card layout customization is implemented as a browser-local MVP using stable panel IDs and existing workspace placements.

New plugins:

- None.

Breaking changes:

- None.
