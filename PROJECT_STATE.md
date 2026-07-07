# Project Status

Current milestone: v0.2 almost-v1 beta

Current version: 0.2.0

Last update timestamp: 2026-07-16T00:00:00-04:00

Repository health status: Healthy for this v0.2 provider validation-hardening slice. Hosted QRZ XML/HamQTH lookup, POTA spot fetch, DX Cluster bounded runtime controls, and Club Log/QRZ Logbook/eQSL uploads remain fake/offline by default, with ignored live validation hooks gated by explicit environment variables and credentials. Runtime responses and persisted provider health now carry stable redacted error codes for common credential, auth, malformed-response, provider-rejection, rate-limit, timeout, and transport failures. SOTAWatch and LoTW remain explicitly deferred where provider/API safety requires it. Formatting, Rust check, Clippy with warnings denied, full workspace tests, GUI JavaScript syntax check, package builds, diff whitespace check, and Tauri package build passed after this slice. The Rust test suite currently reports 212 passed tests and 7 ignored live validation hooks.

---

# Completed Features

## Core

- [x] Workspace package version bumped to 0.2.0 for the v0.2 beta line
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
- [x] Secure credential storage abstraction, production OS backend wiring, and
  explicit dev fallback
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
- [x] Tier 1 provider adapter boundary with fake/mock execution and credential validation
- [x] Gated live Club Log, QRZ Logbook, and eQSL upload transports
- [x] QRZ XML/HamQTH lookup parsers, POTA fixture/request foundation, and DX Cluster read-once Telnet foundation
- [x] Hosted QRZ XML and HamQTH lookup runtime routes with fake default, live gating, credential-reference resolution, structured errors, redaction, and provider health persistence
- [x] Hosted POTA spot fetch runtime route with fake fixture default, live gating, normalized spot output, and provider health persistence
- [x] DX Cluster bounded runtime controls for connect, read-once, disconnect, and status, with fake stream tests and support-metadata health/status recording
- [x] Ignored release-runner live upload validation hooks for Club Log, QRZ Logbook, and eQSL gated by `HAM_LIVE_PROVIDER_TESTS=1` and `HAM_LIVE_PROVIDER_ALLOW_UPLOAD=1`
- [x] Versioned durable support storage abstraction for non-official sidecar state
- [x] Durable provider settings, service cache metadata, upload queue, map layer preferences, lookup/rig UI config, and online automation/notification state loading
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
- [x] Online Services provider panels now surface fake/live/deferred runtime status and lightweight lookup/spot/DX runtime controls
- [x] Service provider enable/disable and priority controls persisted through support storage
- [x] Keyboard-first logging command foundation
- [ ] Interactive dockable panel movement
- [x] Native file dialog command helpers and browser fallback bridge
- [x] Tauri desktop runtime wrapper and Windows package validation

## iOS

- [x] Native SwiftUI Xcode project skeleton
- [x] SwiftData local persistence models for QSO, station profile, and settings
- [x] Home, New QSO, Logbook, QSO Detail, Station Profile, Export, and Settings screens
- [x] ADIF and CSV export services
- [x] Shared Xcode scheme
- [x] Unit tests for callsign utilities, RST defaults, ADIF, CSV, and date formatting
- [ ] Manual validation in Xcode on macOS
- [ ] Swift bridge to Rust/core event model
- [ ] iOS sync parity

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
- [x] Durable sync server storage

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
- [x] Tier 1 LoTW/eQSL/Club Log/QRZ provider adapter boundaries
- [x] Gated live Club Log, QRZ Logbook, and eQSL upload transports
- [ ] LoTW TQSL/certificate-signing upload flow
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
- [x] v0.2/v1.0/v1.1 release plan documentation
- [x] Hosted web release documentation
- [x] Desktop release documentation
- [x] Dedicated `ham-server` hosted API crate added to workspace
- [x] Hosted API route tests for auth, scoping, roles, revoked devices, and QSO lifecycle
- [x] Durable hosted metadata storage for `ham-server`
- [x] Durable sync/report storage for `ham-sync-server`
- [ ] Docs link checker
- [ ] Coverage reporting

---

# Current Architecture

The repository implements a local-first Rust workspace centered on `ham-core`. The core owns official events, proposal validation, runtime diagnostics, projections, ADIF, lookup helpers, rig helpers, diagnostics, permissions, the unified service framework, station/equipment support models, award computation, projection search, upload queue logic, and durable JSONL official storage.

The plugin system is currently static. Plugins are represented by manifests, requested permissions, optional permissions, contributed services, contributed panels, commands, and validation paths. Real runtime loading and sandboxing remain planned.

The unified service framework provides provider metadata, registry, selection, fallback, shared service cache, service authorization, and typed service request/response models for lookup, upload, spotting, map, weather, propagation, online services, and future integrations.

Credential storage is isolated behind `CredentialStore`. Credential metadata is support/security state and secret values stay behind the selected backend. The v0.2 backend selection uses Windows Credential Manager, macOS Keychain through `security`, or Linux Secret Service through `secret-tool` when available. The insecure development file fallback remains explicit opt-in only.

Station/equipment data is support/config state, not official event state. QSO official event payloads may reference profile/config/equipment IDs, but profile changes do not rewrite historical QSOs.

The award engine computes rebuildable progress from QSO projections. Search also reads projections rather than raw official event storage. Upload jobs select projected QSOs and generate ADIF for service-framework upload providers.

Net Control is a plugin-style workflow using the normal proposal pipeline. Net sessions, check-ins, traffic, tombstones, and report exports are official append-only events. `NetControlProjection` derives current roster/session/report state.

The mapping framework is implemented as a core GIS/service layer. `ham-core::map` owns coordinate, grid, distance, bearing, layer, marker, grayline, weather, and propagation models. Maps consume QSO projections, station profiles, and service providers; they do not own official event writes or business workflows.

The Online Services ecosystem is implemented as a provider-backed service layer. `ham-core::online` owns connected provider metadata, upload/download engine models, confirmation records, DX/POTA/SOTA spot normalization, provider health states, automation tasks, notifications, safe cache helpers, and Tier 1 adapter contracts. The Tier 1 layer covers QRZ XML, HamQTH, POTA spots, SOTAWatch, Club Log, QRZ Logbook, eQSL, LoTW, and DX Cluster with fake/mock test paths, credential-reference validation, redacted diagnostics, and fail-closed live limitations where provider-specific transports are not safe to enable. Club Log, QRZ Logbook, and eQSL have gated live HTTP upload transports. QRZ XML/HamQTH hosted lookup execution, POTA hosted spot fetching, and DX Cluster bounded read-once lifecycle controls are wired through provider adapters and hosted routes. Provider runtime state is support metadata in SurrealDB provider settings, not official QSO state. SOTAWatch live access is deferred pending explicit API approval/terms handling, and LoTW production upload is deferred until a safe TQSL/certificate-signing flow is modeled. Live network transports remain behind provider boundaries and must use `CredentialStore`.

Sync is split between `ham-sync` for protocol, peer, LAN/cloud models, safe replication, in-memory test server logic, and durable sync/report server logic, plus `ham-sync-server` for the self-hosted HTTP-like server binary. LAN is preferred. Cloud/self-hosted sync is a fallback and uses the same verification rules.

Storage uses append-only JSONL official events for the MVP. Hosted server metadata uses SurrealDB for users, sessions, devices, logbooks, memberships, API tokens, invites, and schema migrations. The sync/report server uses SurrealDB for sync/support metadata, append-only JSONL for replicated official events, and filesystem-backed diagnostic report payloads. Runtime logs are separate rotating JSONL files. Local GUI support/config state still uses lightweight versioned JSON files for map layer preferences, lookup/rig UI config, station profiles, saved searches, permission grants, and credential metadata. Secret values remain outside support storage behind `CredentialStore`.

The GUI is `ham-gui`, a web-first shell served by Rust and embedded by the Tauri desktop wrapper. It consumes core bridge APIs, projections, runtime events, service framework state, and proposal endpoints. The shared web UI detects desktop native-dialog commands and falls back to browser/server file flows outside desktop mode. In packaged desktop mode, bundled assets call a restricted Tauri `/api/*` proxy pointed at `HAM_DESKTOP_SERVER_URL` or the default local GUI API at `http://127.0.0.1:9467`; the full local backend is not embedded in-process yet.

The Maps workspace consumes `/api/maps/state` and `/api/maps/layer/toggle`. It displays derived QSO/station map objects, layer state, selected-object context, grayline, propagation, weather, and map status fields.

The Online Services workspace consumes `/api/online-services`. It displays accounts, providers, upload queue stats, confirmation downloads, DX/POTA/SOTA spots, weather, propagation, provider health, credential manager, service cache, automation, and notifications.

Permissions are centralized in `ham-core::permissions`. Protected actions require plugin permission and operator role permission. Scope support is modeled but not fully enforced everywhere yet.

Diagnostics include runtime event logs, redaction helpers, report ZIP generation, action timelines, and authenticated upload through the sync server.

---

# Current Workspace Structure

- `crates/ham-core`: event bus, official events, stores, proposals, projections, ADIF, lookup, rig, diagnostics, permissions, service framework, online services, credential storage, Net Control, station profiles, awards, search, upload queue.
- `crates/ham-core::credential`: credential metadata, store abstraction, OS credential backends, explicit insecure development fallback.
- `crates/ham-core::map`: GIS models, Maidenhead grid engine, great-circle engine, map layers, markers, provider metadata, grayline, propagation, and weather models.
- `crates/ham-core::net`: Net Control projection, session/check-in/traffic models, and report export.
- `crates/ham-core::online`: online provider metadata, Tier 1 adapter contracts, upload/download engine models, confirmation records, spot parsing, automation, notification, and provider health helpers.
- `crates/ham-plugin-sdk`: plugin manifest, capabilities, service types, proposal envelope, public event constants.
- `crates/ham-sync`: LAN discovery/handshake models, peer registry, safe replication, cloud API/client/server models, durable sync/report storage, and report upload models.
- `crates/ham-sync-server`: self-hosted sync/report HTTP-like server binary using durable local storage by default.
- `crates/ham-server`: hosted web/server API boundary with durable SurrealDB account, session, device, logbook, role, station/equipment, provider, upload, sync, and QSO lifecycle metadata/API routes.
- `crates/ham-gui`: Rust bridge/server, GUI shell models, command registry, static web UI.
- `crates/ham-desktop`: desktop runtime configuration and testable native-dialog command helpers for the Tauri wrapper.
- `src-tauri`: Tauri v2 desktop runtime wrapper, capabilities, icons, bundled web UI config, native dialog command bridge, and restricted desktop API proxy.
- `crates/ham-cli`: CLI commands for ADIF and chain/projection operations.
- `ios/KE8YGWLogger`: native SwiftUI/SwiftData iOS project for manual Xcode builds.
- `docs/architecture`: service framework, online services, station profiles, award engine, search, and upload queue architecture notes.
- `docs/plugins`: provider and online provider development guides.
- `docs/maps`, `docs/grid-system`, `docs/propagation`, `docs/weather`, `docs/plugin-map-providers`: mapping and provider framework documentation.
- `docs/security`: credential and redaction guidance.
- `justfile`: local development commands aligned with CI.
- `ROADMAP.md`: root milestone roadmap.

---

# Outstanding TODOs

## Critical

- [x] Replace `ham-server` in-memory account/session/device/logbook scaffolding with durable storage before hosted beta use.
- [x] Add durable storage to the self-hosted sync/report server before real hosted use.
- [ ] Implement trust pairing/authentication for LAN peers before unattended sync.
- [ ] Validate production OS keychain/secret-store backends on clean release runners before real online upload/lookup provider credentials are enabled for testers.

## High

- [x] Add Tier 1 LoTW/eQSL/Club Log/QRZ upload adapter boundaries through the service framework.
- [x] Add QRZ/HamQTH lookup adapter boundaries using secure credential storage.
- [x] Add gated provider-specific live uploads for Club Log, QRZ Logbook, and eQSL.
- [x] Add hosted live lookup execution for QRZ XML and HamQTH.
- [ ] Enforce permission scopes consistently across account, logbook, and station boundaries.
- [ ] Add role/account/session models and UI beyond the MVP local-admin assumption.
- [ ] Implement real LAN peer-to-peer transport for replication endpoints.
- [x] Add conflict/divergence review UX foundation.
- [ ] Add real tile/geocoding/weather/propagation providers through the map service framework.
- [ ] Implement LoTW/TQSL, HRDLog, and confirmation download/reconciliation clients.
- [ ] Implement hosted/runtime transports for remaining FCC ULS, RBN, approved SOTAWatch access, NOAA, Open-Meteo, and OSM adapters. QRZ XML, HamQTH, POTA, and DX Cluster bounded runtime wiring are complete for v0.2 validation.
- [ ] Replace the map preview shell with a full interactive tile/vector renderer.

## Medium

- [ ] Add station profile and equipment create/edit GUI forms.
- [ ] Add full award rule databases and needed-list computation.
- [ ] Add saved-search GUI persistence flow.
- [x] Add native file dialog command helpers for import/export/report bundles.
- [x] Add actual Tauri runtime wrapper and Windows package validation.
- [ ] Add projection cache persistence and startup optimization.
- [ ] Extend support storage to provider account state and notification read/unread state.

## Low

- [ ] Add richer visual polish and saved layouts.
- [ ] Add coverage reporting.
- [ ] Add screenshot attachment support for diagnostic reports.
- [ ] Add terrain/elevation, APRS, satellite, and radar overlays.

---

# Known Technical Debt

- Several blueprint-recommended crates are currently implemented as modules inside `ham-core`; this is acceptable for MVP but should be revisited as APIs grow.
- The GUI backend remains a local/hosted HTTP API; the Tauri wrapper bundles the web UI but does not yet embed or sidecar the local backend.
- `ham-server` now defines the hosted API boundary and persists account/session/device/logbook metadata in SurrealDB.
- Plugin loading is static; no sandbox, signature verification, or process isolation exists yet.
- The sync server uses durable SurrealDB metadata/support storage, JSONL official event storage, and filesystem report payload storage; production migration/retention policy still needs hardening.
- Embedded local SurrealDB currently uses SurrealKV. On Windows, SurrealKV holds
  an exclusive in-process file lock, so unit tests verify durable reloads
  through the storage abstraction instead of opening a second embedded handle to
  the same directory in one process.
- LAN discovery/replication has strong models but needs real multi-instance transport wiring.
- Permission scopes are mostly recorded rather than fully enforced.
- Station profile editing exists in core models but not yet as full GUI forms.
- External provider implementations remain fake-first by default; live provider validation is gated behind explicit settings, credentials, and ignored tests.
- Online Services has production-shaped provider metadata, hosted QRZ XML/HamQTH lookup execution, POTA spot fetching, DX Cluster bounded runtime controls, and gated upload transports. Real-account/provider validation remains before production use.
- Automation tasks are modeled but not executed by a durable scheduler.
- Confirmation downloads append official events, but provider-specific matching to historical QSOs needs deeper reconciliation.
- OS credential backend wiring is implemented, but it still needs release-runner and packaged-app validation on Windows, macOS, and Linux.
- The insecure dev credential fallback is plaintext and only suitable for local testing when explicitly enabled.
- Net Control template create/edit UI is not complete; sessions/check-ins/traffic/report are implemented first.
- The Maps workspace currently renders a structured preview rather than full tile/vector map rendering.
- Browser-local card layouts are stored in local storage rather than the support-store layer.
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
- Native credential storage has Windows/macOS/Linux backend implementations, but packaged release-runner validation is still required before real external credentials are safe for production use.

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
- v0.2 Release Plan: added for the almost-v1 beta checklist, risks, acceptance, and v1.0 delta.
- v1.0 Release Plan: updated to clarify hosted web + desktop only and no PWA-as-iOS.
- v1.1 Native iOS Plan: updated to require a real native SwiftUI app, not a web wrapper.
- API Client Contract: updated with `/api/v1`, bearer sessions, device identity, logbook scoping, and proposal-based mutation rules.
- Hosted Web Release: added for `ham-server` route coverage and production hosted gaps.
- Desktop Release: added for Tauri/native dialog packaging expectations.
- Developer Guide: current local workflow; daily-driver examples could be expanded.
- API docs: Rust public item docs are partial and should be improved as APIs stabilize.

---

# Test Coverage

- Core event hashing, chain verification, QSO proposals, projections, ADIF, lookup, rig, diagnostics, permissions, service framework, credential store, Net Control, station profiles, awards, search, upload queue, sync models, GIS models, grid conversion, great-circle math, map layers, marker serialization, provider metadata, grayline calculations, online provider metadata, retry logic, confirmation parsing, spot parsing, cache stats, and notification models have unit coverage.
- GUI model serialization and command/panel foundations have partial coverage.
- JavaScript UI behavior is mostly manually verified and should gain browser-level tests.
- Current test run: `cargo test --workspace` passed with 200 total Rust tests across crates.

---

# Current Milestone

Current objective: finish v0.2 almost-v1 beta by hardening hosted API/storage, desktop packaging, provider execution, sync trust, and GUI tests.

Completed work:

- Added `ham-server` as a dedicated hosted API crate and binary.
- Added hosted API models for `UserAccount`, `LoginSession`, `DeviceIdentity`, `LogbookMembership`, `LogbookRole`, `ServerInvite`, and `ApiToken`.
- Added `/api/v1` hosted API boundary for health/status, login/logout/session, logbooks, QSO lifecycle, station/equipment profiles, ADIF import/export, activations, Net Control, maps, backups, providers, upload queue, sync status/preview/push/pull/divergence review, devices, and route catalog.
- Removed the remaining v0.2 scaffolded workflow route responses; unknown routes now return not found.
- Enforced account/logbook/device scoping and role checks in the implemented hosted QSO, station/equipment, ADIF, activation, Net Control, map, backup, provider, upload, and sync slices.
- Kept QSO create/edit/delete/restore/note writes on the existing proposal pipeline.
- Added route-level tests for cross-logbook rejection, role behavior, logout invalidation, revoked device sync rejection, route catalog coverage, and QSO create/list/edit/delete/restore/note lifecycle.
- Replaced SQLite-backed hosted metadata persistence with SurrealDB-backed persistence for users, login sessions, devices, logbooks, memberships, API tokens, server invites, and schema migrations.
- Updated `ham-server` to use durable metadata storage by default, with in-memory storage retained for focused tests.
- Added durable sync/report/support storage using SurrealDB metadata, append-only JSONL official event storage, filesystem diagnostic report payloads, provider settings without secrets, and upload queue/history metadata.
- Updated `ham-sync-server` to start with durable local storage by default.
- Added restart tests for hosted metadata persistence, sync state persistence, device revocation persistence, invalid chain rejection after restart, and diagnostic report metadata/payload persistence.
- Added hosted station profile routes for list/create/get/update/archive/set-default with durable SurrealDB support metadata.
- Added hosted equipment routes for list/create/get/update/archive and assignment to station profiles when scoped correctly.
- Added hosted ADIF import/export routes. Import parses ADIF and appends proposal-backed official QSO events; export reads official projections.
- Added hosted provider list/detail/update/test routes. Provider settings persist credential references only, reject secret-looking config fields, and support fake/mock provider tests.
- Added hosted upload list/run/retry routes. Upload jobs select QSOs from official projections, generate ADIF snapshots, persist queue/history metadata in SurrealDB, prevent duplicate successful jobs, and expose retryable failure details.
- Added hosted sync pull route returning scoped missing official events; duplicate push replay remains ignored safely.
- Added hosted activation routes for list/create/get/update/end and linked-QSO reads. Activation writes use core proposal validation and official activation events.
- Added hosted Net Control routes for session list/create/get/end, check-ins, check-in update, and traffic records. Net Control writes use core proposal validation and official Net Control events.
- Added hosted map QSO/station/path/settings routes. QSO and path data are projection-derived; map settings persist in SurrealDB support metadata.
- Added hosted backup export/list/get/download, import dry-run, and safe import routes. Backup export includes official events and support metadata without secrets; dry-run validates manifest scope and event-chain integrity; import appends only verified missing official events, skips exact duplicates, restores scoped support metadata, strips provider credential references, and blocks divergent targets.
- Added hosted sync divergence review/get/export routes. Reviews report safe pull/push/diverged states and persist report metadata without automatic merge.
- Added backup/restore GUI screens with export, dry-run review, import result, and sensitive-section disclosure.
- Added divergence review GUI screens with local/remote heads, safe pull/push flags, recommended action, and report export.
- Added `ham-desktop` desktop foundation crate and root `src-tauri` packaging configuration.
- Added desktop-native dialog bridge detection in the web UI for ADIF import/export, backup import/export, diagnostic bundle export, and divergence report export.
- Added testable `ham-desktop` native-dialog command helpers for ADIF import/export, backup import/export, diagnostic bundle export, divergence report export, and app data directory selection.
- Added the real `src-tauri` Tauri v2 runtime crate as a workspace member.
- Added Tauri command wrappers for `desktop_runtime`, `desktop_api_request`, `import_adif_dialog`, `export_adif_dialog`, `export_backup_dialog`, `import_backup_dialog`, `export_diagnostic_bundle_dialog`, `export_divergence_report_dialog`, and `select_app_data_directory_dialog`.
- Wired Tauri native-dialog commands to the existing `ham-desktop` helper layer without duplicating domain logic.
- Configured release bundling to embed `crates/ham-gui/web` directly without a frontend dev server.
- Added relative web asset paths for Tauri asset-protocol compatibility.
- Added a restricted Tauri `/api/*` proxy so bundled desktop assets can talk to a configured local/hosted API without broad browser CORS changes.
- Added Tauri v2 capability metadata and deterministic placeholder icon assets required for Windows packaging.
- Added Windows Credential Manager, macOS Keychain, and Linux Secret Service/libsecret credential backend wiring behind `OsCredentialStore`.
- Updated GUI/local credential selection to use OS credential storage when available and keep the insecure file fallback explicit opt-in only.
- Added credential safety tests covering the `TEST_SECRET_SHOULD_NOT_APPEAR` sentinel across metadata sidecars, diagnostics, provider responses, upload history, and backup export.
- Added provider test response fields for credential-reference status/resolution without returning secret values.
- Updated Surreal-backed `ham-server` mode to use a JSONL official event store path while preserving in-memory stores for focused tests.
- Added `docs/V0_2_RELEASE_PLAN.md`, `docs/DESKTOP_RELEASE.md`, and `docs/HOSTED_WEB_RELEASE.md`.
- Updated `README.md`, `ROADMAP.md`, `docs/ROADMAP.md`, `docs/V1_RELEASE_PLAN.md`, `docs/V1_1_IOS_NATIVE_PLAN.md`, and `docs/API_CLIENT_CONTRACT.md`.
- Bumped workspace package version to 0.2.0.
- Connected provider metadata for LoTW, eQSL, Club Log, QRZ Logbook, HRDLog, QRZ XML, HamQTH, FCC ULS, prefix fallback, DX Cluster, RBN, POTA, SOTAWatch, NOAA Space Weather, NOAA Weather, Open-Meteo, OpenStreetMap, offline tile cache, and reverse geocoder.
- Upload engine retry policy, execution result, upload stats, provider health, and notification foundation.
- Confirmation download model and official append-only confirmation status event path.
- DX Cluster parser and POTA/SOTA spot normalization into the shared spot model.
- Online automation task and notification models.
- Online Services workspace and `/api/online-services` dashboard.
- Versioned support storage for provider settings, service cache metadata, upload queue state, map layer preferences, lookup/rig UI config, and online support state.

Remaining work:

- Harden backup restore UX and add browser-level coverage.
- Add LAN peer-to-peer transport and trust pairing.
- Enforce permission scopes across all older GUI/local routes, not only the new hosted QSO slice.
- Add browser-level GUI tests.
- Validate hosted QRZ XML/HamQTH lookup execution, hosted POTA spot fetch, DX
  Cluster bounded runtime controls, and gated Club Log/QRZ Logbook/eQSL live
  uploads with provider-approved accounts or controlled fixtures.
- Add approved SOTAWatch live access and LoTW/TQSL upload only after their
  provider/API safety models are documented.
- Validate OS credential backends on clean Windows/macOS/Linux packaging runners.
- Validate Tauri packaging on clean Linux/macOS runners and decide signing/notarization/updater policy.
- Embed or sidecar the local GUI backend if v1.0 requires fully local packaged operation without a separately running local/hosted API.
- Validate gated Club Log, QRZ Logbook, and eQSL live uploads on explicit
  release-runner credentials, then add provider-specific confirmation clients
  where core supports safe reconciliation.
- Add account settings, scheduler state, and notification read state.
- Add browser-level GUI tests for online service interactions.

Expected completion criteria:

- All required quality gates pass.
- Documentation and project state are updated.
- Hosted API route slices use the existing proposal pipeline, official projections, SurrealDB support metadata, role/scope checks, and redaction rules.

Quality gates from this v0.2 slice:

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace --all-targets`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: passed, 200 Rust tests total.
- `node --check crates/ham-gui/web/app.js`: passed.
- `cargo build -p ham-server`: passed.
- `cargo build -p ham-sync-server`: passed.
- `cargo build -p ham-desktop`: passed.
- `git diff --check`: passed.
- `cargo tauri info`: passed; detected WebView2 149.0.4022.98, Visual Studio Build Tools 2022/MSVC, Tauri 2.11.5, tauri-build 2.6.3, wry 0.55.1, tao 0.35.3, and tauri-cli 2.2.7.
- `cargo tauri build`: passed; built `target/release/ke8ygw-logger-desktop.exe` and produced `target/release/bundle/msi/KE8YGW Logger_0.2.0_x64_en-US.msi` plus `target/release/bundle/nsis/KE8YGW Logger_0.2.0_x64-setup.exe`.
- Browser-level tests: not run; no Playwright/equivalent suite is configured yet.
- Docs link checker: not run; not configured.

---

# Recommended Next Milestone

Provider Runtime Hardening, Desktop Release Hardening, and GUI Tests:

- Validate Tauri packaging and OS credential backends on clean Windows/Linux/macOS release runners.
- Decide whether v1.0 desktop embeds/sidecars the local GUI backend or requires a configured hosted/self-hosted API.
- Validate hosted lookup/spot/DX execution and the gated live upload adapters
  against provider-approved accounts or controlled fixtures.
- Add browser-level GUI tests and CI release artifact hardening.

Then continue Provider Runtime Hardening:

- Hosted QRZ XML and HamQTH lookup execution real-account validation.
- LoTW/TQSL and HRDLog upload clients; confirmation clients for providers with
  safe matching semantics.
- DX Cluster bounded runtime and hosted POTA spot fetch real-provider
  validation; approved SOTAWatch live access and RBN adapters.
- NOAA/Open-Meteo/space-weather live providers.
- Durable scheduler execution for automatic uploads/downloads/refreshes.

This milestone should come next because the provider metadata, credential
references, upload/download models, and GUI surfaces are now ready for
provider-approved live validation and release-runner artifact hardening.

---

# Changelog

## 2026-07-08 Live Validation Hardening

Summary: Hardened the provider live-validation model without changing the
default offline/fake test behavior. Added ignored live hooks for QRZ XML,
HamQTH, POTA, and DX Cluster, tightened the existing Club Log, QRZ Logbook, and
eQSL upload hooks so they skip cleanly without credentials, and added stable
redacted error codes to provider runtime responses and persisted health.

Files/crates changed in this pass:

- `crates/ham-core/src/online.rs`
- `crates/ham-server/src/lib.rs`
- `crates/ham-gui/web/app.js`
- `README.md`
- `ROADMAP.md`
- `docs/API_CLIENT_CONTRACT.md`
- `docs/HOSTED_WEB_RELEASE.md`
- `docs/PROVIDER_LIVE_TRANSPORTS.md`
- `docs/ROADMAP.md`
- `docs/V0_2_RELEASE_PLAN.md`
- `docs/security/credential-storage.md`
- `PROJECT_STATE.md`

Provider validation status:

- Club Log: fake/default tests remain offline; ignored live upload hook now
  requires `HAM_LIVE_PROVIDER_TESTS=1`,
  `HAM_LIVE_PROVIDER_ALLOW_UPLOAD=1`, and provider-specific test credentials.
  Real upload validation is still pending a provider-approved account.
- QRZ Logbook: fake/default tests remain offline; ignored live upload hook uses
  the same global live/upload gates plus `HAM_QRZ_LOGBOOK_TEST_KEY`. Real
  upload validation is pending.
- eQSL: fake/default tests remain offline; ignored live upload hook uses the
  same global live/upload gates plus eQSL test credentials. Real response
  variation validation is pending.
- QRZ XML: ignored live lookup hook added for username/password/callsign
  validation. Hosted runtime already persists health; real-account validation
  is pending.
- HamQTH: ignored live lookup hook added for username/password/callsign
  validation. Hosted runtime already persists health; real-account validation
  is pending.
- POTA: ignored read-only live spot hook added behind the global live gate.
  Fixture tests remain default; real endpoint shape validation is pending.
- DX Cluster: ignored bounded live connect/read/disconnect hook added for
  host/port/callsign/timeout validation. Fake stream tests remain default.
- SOTAWatch: remains deferred pending approved API/terms handling.
- LoTW: remains deferred pending a safe TQSL/certificate-signing model.

Live validation gates:

- All live hooks require `HAM_LIVE_PROVIDER_TESTS=1`.
- Upload hooks additionally require `HAM_LIVE_PROVIDER_ALLOW_UPLOAD=1` because
  provider-side records may be created.
- Missing live credentials produce high-level skipped output rather than default
  CI failures.
- Live hook output is limited to provider name, mode, high-level status,
  retryability, and redacted error codes/summaries.

Provider error mapping:

- Runtime lookup/spot/DX responses now include a stable redacted `error_code`
  where applicable.
- Common mapped categories include `missing_credential`,
  `invalid_credential_reference`, `auth_failure`, `session_failure`,
  `callsign_not_found`, `rate_limited`, `permission_issue`,
  `network_timeout`, `connection_failed`, `transport_failure`,
  `malformed_response`, `provider_rejection`, `provider_disabled`,
  `live_mode_not_configured`, and `provider_error`.
- Persisted provider health records now keep `last_error_code` alongside the
  redacted failure summary.

Credential/redaction safety:

- Provider settings continue to store credential IDs/references only.
- Live adapters and hosted runtime paths resolve secrets through
  `CredentialStore` or explicit test-only environment injection.
- No raw request bodies, credential values, session tokens, XML/HTML response
  bodies, or account/session details are printed by live validation hooks or
  returned in API diagnostics.
- Provider lookup/spot/upload state remains support metadata and does not
  mutate official QSO rows.

Quality gate results for this pass:

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace --all-targets`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: passed with 212 passed Rust tests and 7 ignored
  live-validation hooks.
- `node --check crates/ham-gui/web/app.js`: passed.
- `cargo build -p ham-server`: passed.
- `cargo build -p ham-sync-server`: passed.
- `cargo build -p ham-desktop`: passed.
- `git diff --check`: passed.
- `cargo tauri info`: exited 0; environment probe reported missing
  WebView2/MSVC/Rust metadata.
- `cargo tauri build`: passed and produced Windows MSI/NSIS installers.

Live validation run status:

- Real live validation was not run in this workspace because the required
  `HAM_LIVE_PROVIDER_TESTS` and provider credential environment variables were
  not present.

Remaining v0.2 gaps:

- Real-account/provider validation for QRZ XML, HamQTH, POTA, DX Cluster, Club
  Log, QRZ Logbook, and eQSL.
- SOTAWatch live API approval/terms handling.
- LoTW TQSL/certificate-signing design and implementation.
- Confirmation reconciliation beyond documented planning and existing official
  confirmation-status event support.
- LAN peer pairing, browser-level GUI tests, clean release-runner credential
  validation, and CI/release artifact hardening.

Recommended next prompt:

Run the ignored provider live validation hooks on a release runner with
provider-approved credentials, capture redacted pass/fail/skip outcomes for QRZ
XML, HamQTH, POTA, DX Cluster, Club Log, QRZ Logbook, and eQSL, and harden any
provider-specific response mapping observed during that run. Keep SOTAWatch and
LoTW deferred until their safety models are approved.

## 2026-07-08 Provider Runtime Wiring

Summary: Wired hosted provider runtime execution for QRZ XML lookup, HamQTH
lookup, POTA spot fetching, and DX Cluster bounded read-once controls. Added
provider health/status persistence in support metadata, release-runner-gated
live upload validation hooks, modest GUI status surfacing, and provider docs.

Files/crates changed in this pass:

- `crates/ham-core/src/online.rs`
- `crates/ham-core/src/lib.rs`
- `crates/ham-server/src/lib.rs`
- `crates/ham-gui/src/main.rs`
- `crates/ham-gui/web/app.js`
- `README.md`
- `ROADMAP.md`
- `docs/API_CLIENT_CONTRACT.md`
- `docs/HOSTED_WEB_RELEASE.md`
- `docs/PROVIDER_LIVE_TRANSPORTS.md`
- `docs/ROADMAP.md`
- `docs/V0_2_RELEASE_PLAN.md`
- `docs/security/credential-storage.md`
- `PROJECT_STATE.md`

Hosted route/runtime behavior:

- `POST /api/v1/providers/qrz-xml/lookup` and
  `POST /api/v1/providers/hamqth/lookup` enforce logbook read scope, provider
  enablement, fake/live mode, credential-reference resolution for live mode,
  structured not-found/auth/malformed/missing-credential failures, and redacted
  runtime diagnostics.
- `GET /api/v1/providers/pota-spots/spots` enforces logbook read scope, keeps
  fake fixture mode as default, supports live mode only when explicitly set,
  returns normalized compact spot records, and records health.
- `POST /api/v1/providers/dx-cluster/connect`,
  `POST /api/v1/providers/dx-cluster/read`,
  `POST /api/v1/providers/dx-cluster/disconnect`, and
  `GET /api/v1/providers/dx-cluster/status` implement a bounded session model
  suitable for v0.2. No always-on daemon is started.
- `GET /api/v1/providers` and `GET /api/v1/providers/:id` include health
  summaries with mode, enablement, credential-reference state, last run,
  last success/failure, redacted last error, and next recommended action.

Provider status:

- Club Log: fake default plus gated live HTTP upload; ignored live validation
  hook added; needs real-account validation.
- QRZ Logbook: fake default plus gated live HTTP upload; ignored live
  validation hook added; needs real-account validation.
- eQSL: fake default plus gated live HTTP upload; ignored live validation hook
  added; response variations still need real-account validation.
- QRZ XML: hosted fake/live lookup execution wired; live uses CredentialStore
  credential references; needs real-account validation.
- HamQTH: hosted fake/live lookup execution wired; live uses CredentialStore
  credential references; needs real-account validation.
- POTA: hosted fake/live spot fetch wired; live uses the modeled request
  builder; needs provider/live validation.
- DX Cluster: bounded connect/read/disconnect/status runtime wired over the
  read-once Telnet foundation; persistent reconnect/background lifecycle
  remains deferred.
- SOTAWatch: fixture/parser status remains; live access deferred pending
  approved API/terms handling.
- LoTW: fake/scaffold status remains; real upload deferred until a safe
  TQSL/certificate-signing model exists.

Credential/redaction safety:

- Provider settings continue to store credential IDs/references only.
- Credential secrets are resolved through `CredentialStore` only for live
  operations that need them.
- Lookup/spot/DX/upload runtime state is support metadata, not official QSO
  state, and provider results do not mutate QSO records directly.
- API responses, provider diagnostics, upload history, backups, and docs do not
  include credential secret values. Live validation tests are ignored by default
  and require both `HAM_LIVE_PROVIDER_TESTS=1` and
  `HAM_LIVE_PROVIDER_ALLOW_UPLOAD=1`.

Quality gate results for this pass:

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace --all-targets`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed after
  approval-backed rerun because the actual checkout target directory is outside
  the configured writable sandbox.
- `cargo test --workspace`: passed with 211 passed Rust tests and 3 ignored
  live-validation hooks.
- `node --check crates/ham-gui/web/app.js`: passed.
- `cargo build -p ham-server`: passed after approval-backed rerun because the
  actual checkout target directory is outside the configured writable sandbox.
- `cargo build -p ham-sync-server`: passed after approval-backed rerun because
  the actual checkout target directory is outside the configured writable
  sandbox.
- `cargo build -p ham-desktop`: passed after approval-backed rerun because the
  actual checkout target directory is outside the configured writable sandbox.
- `git diff --check`: passed.
- `cargo tauri info`: exited 0; environment probe still reported missing
  WebView2/MSVC/rust metadata.
- `cargo tauri build`: passed after approval-backed rerun and produced:
  - `target/release/bundle/msi/KE8YGW Logger_0.2.0_x64_en-US.msi`
  - `target/release/bundle/nsis/KE8YGW Logger_0.2.0_x64-setup.exe`

Remaining v0.2 gaps:

- Real-account/provider validation for QRZ XML, HamQTH, POTA, DX Cluster, Club
  Log, QRZ Logbook, and eQSL.
- SOTAWatch live API approval/terms handling.
- LoTW TQSL/certificate-signing design and implementation.
- Confirmation download/reconciliation beyond the current ADIF confirmation
  foundation.
- LAN peer-to-peer trust pairing, browser-level GUI tests, and CI/release
  artifact hardening.

Recommended next prompt:

Validate the newly wired provider runtime routes against real provider-approved
accounts or fixtures, run the ignored live upload hooks on a release runner with
explicit credentials, and harden any response parsing or provider-specific
error mapping found during validation. Keep SOTAWatch deferred until API/terms
approval is documented and keep LoTW deferred until the TQSL signing model is
designed.

## 2026-07-08

Summary: Added gated Tier 1 live provider transports and provider documentation.
Club Log, QRZ Logbook, and eQSL now have live HTTP upload paths behind explicit
settings and credential references. QRZ XML/HamQTH lookup parsing, POTA spot
request/fixture parsing, SOTA fixture parsing, and DX Cluster read-once Telnet
foundation were added while keeping fake/mock mode as the CI default.

Major files changed:

- `crates/ham-core/Cargo.toml`
- `crates/ham-core/src/online.rs`
- `crates/ham-core/src/lib.rs`
- `crates/ham-server/src/lib.rs`
- `README.md`
- `ROADMAP.md`
- `docs/ROADMAP.md`
- `docs/V0_2_RELEASE_PLAN.md`
- `docs/HOSTED_WEB_RELEASE.md`
- `docs/API_CLIENT_CONTRACT.md`
- `docs/PROVIDER_LIVE_TRANSPORTS.md`
- `docs/architecture/online-services.md`
- `docs/plugins/online-provider-development.md`
- `docs/security/credential-storage.md`
- `PROJECT_STATE.md`

Provider status:

- Club Log: live ADIF upload implemented behind `CredentialStore`; fake mode
  remains; live validation is gated by explicit environment/credential setup.
- QRZ Logbook: live ADIF insert implemented behind `CredentialStore`; fake mode
  remains; live validation is gated by explicit environment/credential setup.
- eQSL: live ADIF upload implemented behind `CredentialStore`; fake mode
  remains; provider response variation needs real-account validation.
- QRZ XML: XML session/lookup response parser implemented; hosted lookup
  execution remains.
- HamQTH: XML session/search response parser implemented; hosted lookup
  execution remains.
- POTA: live activator-spots request builder and fixture parser implemented;
  hosted fetch route remains.
- SOTAWatch: fixture parser implemented; live access deferred pending explicit
  API approval/terms handling.
- DX Cluster: parser plus read-once Telnet connect/login/read foundation
  implemented; no always-on background stream lifecycle yet.
- LoTW: fake/scaffold mode retained; production upload deferred until a safe
  TQSL/certificate-signing flow is modeled.

Upload execution behavior:

- Hosted upload jobs still select official projections, generate ADIF snapshots,
  persist redacted upload history in SurrealDB support metadata, deduplicate
  queued/running/succeeded duplicate jobs, and never mutate QSO records directly.
- Live Club Log, QRZ Logbook, and eQSL uploads are retryable only for mapped
  transport/timeout/5xx-style failures; provider auth/rejection failures are
  returned as non-success redacted results.

Credential and redaction safety:

- Provider settings store credential IDs/references only.
- Live provider adapters resolve secrets through `CredentialStore` and use the
  secret only in memory for the current operation.
- Secrets are not written to official events, SurrealDB provider settings,
  backups, diagnostics, upload history, divergence reports, API responses, logs,
  or test snapshots.
- The insecure credential fallback remains explicit dev/test opt-in only.

Quality gate results for this pass:

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace --all-targets`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed after rerun
  outside the sandbox build-lock limitation.
- `cargo test --workspace`: passed with 210 Rust tests.
- `node --check crates/ham-gui/web/app.js`: passed.
- `cargo build -p ham-server`: passed after rerun outside the sandbox
  build-lock limitation.
- `cargo build -p ham-sync-server`: passed after rerun outside the sandbox
  build-lock limitation.
- `cargo build -p ham-desktop`: passed after rerun outside the sandbox
  build-lock limitation.
- `git diff --check`: passed.
- `cargo tauri info`: exited 0, but the sandboxed environment probe reported
  missing WebView2/MSVC/rust toolchain metadata.
- `cargo tauri build`: passed and produced:
  - `target/release/bundle/msi/KE8YGW Logger_0.2.0_x64_en-US.msi`
  - `target/release/bundle/nsis/KE8YGW Logger_0.2.0_x64-setup.exe`

Remaining v0.2 gaps:

- Hosted QRZ XML/HamQTH lookup execution.
- Hosted POTA spot fetch and approved SOTAWatch live feed behavior.
- DX Cluster stream lifecycle beyond read-once foundation.
- LoTW TQSL/certificate-signing upload model.
- Confirmation reconciliation/download clients beyond fixture/foundation work.
- Live provider validation on release-runner credentials.
- Browser-level GUI tests, CI release artifact hardening, LAN trust pairing, and
  cross-OS package validation.

Recommended next prompt:

Wire hosted QRZ XML/HamQTH lookup routes, hosted POTA spot fetching, DX Cluster
stream lifecycle controls, and release-runner-gated live validation for Club Log,
QRZ Logbook, and eQSL while keeping LoTW deferred until a TQSL signing model is
designed.

## 2026-07-08

Summary: Added the real Tauri v2 desktop runtime wrapper, wired native-dialog
commands to `ham-desktop`, bundled the shared web UI for release mode, added a
restricted desktop API proxy, and validated Windows packaging.

Major files changed:

- `Cargo.toml`
- `Cargo.lock`
- `crates/ham-desktop/src/lib.rs`
- `crates/ham-gui/web/app.js`
- `crates/ham-gui/web/index.html`
- `src-tauri/Cargo.toml`
- `src-tauri/build.rs`
- `src-tauri/src/main.rs`
- `src-tauri/tauri.conf.json`
- `src-tauri/capabilities/default.json`
- `src-tauri/icons/icon.ico`
- `src-tauri/icons/icon.png`
- `src-tauri/README.md`
- `README.md`
- `ROADMAP.md`
- `PROJECT_STATE.md`
- `docs/DESKTOP_RELEASE.md`
- `docs/ROADMAP.md`
- `docs/V0_2_RELEASE_PLAN.md`

Quality gates:

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace --all-targets`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: passed, 200 Rust tests total.
- `node --check crates/ham-gui/web/app.js`: passed.
- `cargo build -p ham-server`: passed.
- `cargo build -p ham-sync-server`: passed.
- `cargo build -p ham-desktop`: passed.
- `git diff --check`: passed.
- `cargo tauri info`: passed; WebView2, Visual Studio Build Tools/MSVC, and Tauri packages detected.
- `cargo tauri build`: passed; produced Windows MSI and NSIS installer artifacts.

Remaining gaps:

- Cross-OS Tauri package validation and release artifact hardening remain.
- OS credential backends need clean release-runner validation.
- Live provider adapters and real upload execution remain.
- Browser-level GUI tests remain.
- LAN peer-to-peer trust pairing remains.

Summary: Added desktop native-dialog command helpers, production OS credential
backend wiring, provider credential validation response hooks, and expanded
secret-redaction tests.

Major files changed:

- `crates/ham-core/Cargo.toml`
- `crates/ham-core/src/credential.rs`
- `crates/ham-core/src/diagnostics.rs`
- `crates/ham-core/src/lib.rs`
- `crates/ham-desktop/src/lib.rs`
- `crates/ham-gui/src/main.rs`
- `crates/ham-server/src/lib.rs`
- `README.md`
- `ROADMAP.md`
- `PROJECT_STATE.md`
- `docs/ROADMAP.md`
- `docs/V0_2_RELEASE_PLAN.md`
- `docs/HOSTED_WEB_RELEASE.md`
- `docs/DESKTOP_RELEASE.md`
- `docs/API_CLIENT_CONTRACT.md`
- `docs/security/credential-storage.md`

Quality gates:

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace --all-targets`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: passed, 197 Rust tests total in that previous pass.
- `node --check crates/ham-gui/web/app.js`: passed.
- `cargo build -p ham-server`: passed.
- `cargo build -p ham-sync-server`: passed.
- `cargo build -p ham-desktop`: passed.
- `git diff --check`: passed.
- Tauri package validation was not yet available in that previous pass.

Remaining gaps:

- Cross-OS Tauri installer/package validation remains.
- Live provider adapters and real upload execution remain.
- OS credential backends need clean release-runner validation.
- Browser-level GUI tests and release artifact hardening remain.
- LAN peer-to-peer trust pairing remains.

Summary: Added safe backup import, backup/restore GUI, divergence review GUI,
desktop packaging foundation, and native dialog bridge contract.

Major files changed:

- `crates/ham-core/src/adif.rs`
- `crates/ham-core/src/proposal.rs`
- `crates/ham-server/src/lib.rs`
- `crates/ham-gui/src/main.rs`
- `crates/ham-gui/src/shell.rs`
- `crates/ham-gui/src/commands.rs`
- `crates/ham-gui/web/app.js`
- `crates/ham-gui/web/index.html`
- `crates/ham-desktop/*`
- `src-tauri/*`
- `README.md`
- `ROADMAP.md`
- `PROJECT_STATE.md`
- `docs/ROADMAP.md`
- `docs/V0_2_RELEASE_PLAN.md`
- `docs/HOSTED_WEB_RELEASE.md`
- `docs/DESKTOP_RELEASE.md`
- `docs/API_CLIENT_CONTRACT.md`

Quality gates:

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace --all-targets`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: passed, 192 Rust tests total.
- `node --check crates/ham-gui/web/app.js`: passed.
- `cargo build -p ham-server`: passed.
- `cargo build -p ham-sync-server`: passed.
- `cargo build -p ham-desktop`: passed.
- `git diff --check`: passed.

Remaining gaps:

- Real Tauri runtime commands and package build validation remain.
- Production OS credential backends remain.
- Live provider adapters and real upload execution remain.
- Browser-level GUI tests and release artifact hardening remain.
- LAN peer-to-peer trust pairing remains.

Summary: Added hosted workflow API slices for activations, Net Control, maps,
backup export/import dry-run, and sync divergence review.

Major files changed:

- `crates/ham-server/src/lib.rs`
- `README.md`
- `ROADMAP.md`
- `PROJECT_STATE.md`
- `docs/ROADMAP.md`
- `docs/V0_2_RELEASE_PLAN.md`
- `docs/HOSTED_WEB_RELEASE.md`
- `docs/API_CLIENT_CONTRACT.md`

Quality gates:

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace --all-targets`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: passed, 189 Rust tests total.
- `node --check crates/ham-gui/web/app.js`: passed.
- `cargo build -p ham-server`: passed.
- `cargo build -p ham-sync-server`: passed.

Remaining gaps:

- Full backup restore/import remains dry-run only.
- User-facing divergence review/export UX remains.
- Live provider adapters and production credential backends remain.
- Upload execution is still fake/stub-provider based.
- Tauri desktop packaging, native file dialogs, browser-level GUI tests, and
  release artifact hardening remain.

Summary: Added hosted v0.2 route slices for station profiles, equipment
profiles, ADIF import/export, provider settings/test, upload queue execution
foundation, and sync pull.

Major files changed:

- `crates/ham-server/src/lib.rs`
- `README.md`
- `ROADMAP.md`
- `PROJECT_STATE.md`
- `docs/ROADMAP.md`
- `docs/V0_2_RELEASE_PLAN.md`
- `docs/HOSTED_WEB_RELEASE.md`
- `docs/API_CLIENT_CONTRACT.md`

Quality gates:

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace --all-targets`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `cargo test --workspace`: passed, 184 Rust tests total.
- `node --check crates/ham-gui/web/app.js`: passed.
- `cargo build -p ham-server`: passed.
- `cargo build -p ham-sync-server`: passed.

Remaining gaps:

- Hosted activation, Net Control, maps, backup/restore, and divergence review
  routes remain.
- Live provider adapters and production credential backends remain.
- Upload execution is still fake/stub-provider based.
- Tauri desktop packaging, native file dialogs, browser-level GUI tests, and
  release artifact hardening remain.

Summary: Added durable hosted metadata storage and durable sync/report storage
for the v0.2 beta server path.

Major files changed:

- `Cargo.toml`
- `Cargo.lock`
- `.env.example`
- `crates/ham-server/Cargo.toml`
- `crates/ham-server/src/lib.rs`
- `crates/ham-server/src/main.rs`
- `crates/ham-sync/Cargo.toml`
- `crates/ham-sync/src/lib.rs`
- `crates/ham-sync-server/src/main.rs`
- `README.md`
- `docs/V0_2_RELEASE_PLAN.md`
- `docs/API_CLIENT_CONTRACT.md`
- `docs/HOSTED_WEB_RELEASE.md`
- `PROJECT_STATE.md`

Storage schema added:

- `ham-server` SurrealDB metadata: `schema_migrations`, `users`,
  `login_sessions`, `devices`, `logbooks`, `logbook_memberships`,
  `api_tokens`, and `server_invites`.
- `ham-sync` SurrealDB metadata/support state: `schema_migrations`,
  `sync_sessions`, `sync_devices`, `sync_logbook_access`, `pairing_tokens`,
  `sync_heads`, `sync_event_refs`, `diagnostic_reports`, `provider_settings`,
  and `upload_queue_history`.
- `ham-sync` durable payloads: append-only JSONL official event log and
  filesystem diagnostic report bundles.

Architectural decisions:

- `ham-server` uses SurrealDB metadata by default in the binary; in-memory metadata
  remains available for focused route tests.
- `ham-sync-server` uses durable local storage by default; the in-memory cloud
  sync server remains available for deterministic unit tests.
- Official QSO/log mutation semantics remain proposal-backed or verified
  append-only event replication; no direct official-state mutation was added.

Summary from previous v0.2 pass: Started the v0.2 almost-v1 beta by adding a dedicated hosted API crate,
release planning docs, account/session/device/logbook scaffolding, and
proposal-backed hosted QSO lifecycle routes.

Major files changed in previous v0.2 pass:

- `Cargo.toml`
- `Cargo.lock`
- `crates/ham-server/Cargo.toml`
- `crates/ham-server/src/lib.rs`
- `crates/ham-server/src/main.rs`
- `README.md`
- `ROADMAP.md`
- `docs/ROADMAP.md`
- `docs/V0_2_RELEASE_PLAN.md`
- `docs/V1_RELEASE_PLAN.md`
- `docs/V1_1_IOS_NATIVE_PLAN.md`
- `docs/API_CLIENT_CONTRACT.md`
- `docs/DESKTOP_RELEASE.md`
- `docs/HOSTED_WEB_RELEASE.md`
- `PROJECT_STATE.md`

Architectural decisions from previous v0.2 pass:

- `ham-server` is the hosted web/server API boundary for v0.2 work.
- Hosted QSO create/edit/delete/restore/note routes submit proposals through
  `ham-core::submit_proposal`; they do not mutate official log state directly.
- v1.0 remains hosted web + desktop only; native SwiftUI iOS stays v1.1.

New crates:

- `ham-server`

Breaking changes:

- Workspace package version changed from `0.1.0` to `0.2.0`.

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

## 2026-07-08

Summary: Added Tier 1 provider adapter boundaries and hosted upload execution
through the adapter framework while preserving fake/mock testability and
credential redaction.

Major files changed:

- `crates/ham-core/src/online.rs`
- `crates/ham-core/src/lib.rs`
- `crates/ham-server/src/lib.rs`
- `README.md`
- `ROADMAP.md`
- `docs/API_CLIENT_CONTRACT.md`
- `docs/HOSTED_WEB_RELEASE.md`
- `docs/V0_2_RELEASE_PLAN.md`
- `docs/ROADMAP.md`
- `docs/security/credential-storage.md`
- `PROJECT_STATE.md`

Provider/upload status:

- QRZ XML and HamQTH now have Tier 1 lookup adapter boundaries, credential
  validation, fake provider tests, redacted diagnostics, and explicit
  fail-closed live-transport limitations.
- POTA spots, SOTAWatch, and DX Cluster have spot/parser/scaffold coverage and
  fake/test-safe provider health paths; persistent live feed/telnet runtimes
  remain.
- Club Log, QRZ Logbook, eQSL, and LoTW uploads execute through the Tier 1
  adapter boundary. Fake mode succeeds deterministically, forced fake failures
  are retryable, missing/invalid credentials are retryable, and live transports
  fail closed where provider-specific request/signing behavior is not modeled.
- Hosted upload jobs select official projections, generate ADIF snapshots, store
  redacted upload history in SurrealDB support metadata, deduplicate queued,
  running, and successful duplicate jobs, and never mutate QSO records directly.

Credential safety status:

- Provider settings store credential IDs/references only.
- Live-mode credential checks resolve references through `CredentialStore`.
- API responses, upload history, provider diagnostics, backups, and docs remain
  secret-free; tests continue to use the `TEST_SECRET_SHOULD_NOT_APPEAR`
  sentinel.
- The insecure credential backend remains explicit opt-in only through
  `HAM_PLATFORM_ALLOW_INSECURE_DEV_CREDENTIALS=1`.

Quality gate results:

- `cargo fmt --all -- --check`: passed.
- `cargo check --workspace --all-targets`: passed.
- `cargo clippy --workspace --all-targets -- -D warnings`: passed after rerun outside the sandbox build-lock limitation.
- `cargo test --workspace`: passed with 204 Rust tests.
- `node --check crates/ham-gui/web/app.js`: passed.
- `cargo build -p ham-server`: passed after rerun outside the sandbox build-lock limitation.
- `cargo build -p ham-sync-server`: passed after rerun outside the sandbox build-lock limitation.
- `cargo build -p ham-desktop`: passed after rerun outside the sandbox build-lock limitation.
- `git diff --check`: passed.
- `cargo tauri info`: passed; WebView2, MSVC Build Tools 2022, and Tauri packages were detected.
- `cargo tauri build`: passed and produced:
  - `target/release/bundle/msi/KE8YGW Logger_0.2.0_x64_en-US.msi`
  - `target/release/bundle/nsis/KE8YGW Logger_0.2.0_x64-setup.exe`

Remaining v0.2 gaps:

- Provider-specific live HTTP/telnet/TQSL transports.
- Confirmation reconciliation beyond the existing ADIF confirmation foundation.
- LAN peer-to-peer trust pairing.
- Browser-level GUI tests.
- CI/release artifact hardening.
- Clean OS credential backend validation on release runners.
- Cross-OS Tauri package validation.

Recommended next prompt:

Complete provider-specific live transports for Club Log, QRZ Logbook, eQSL, QRZ
XML, HamQTH, POTA spots, SOTAWatch, and DX Cluster using official provider API
references, with real-network tests gated behind explicit credentials and
release-runner secrets.

## 2026-07-07

Summary: Added a native iOS SwiftUI/SwiftData Xcode project for KE8YGW Logger on the `apple-swift` branch.

Major files changed:

- `ios/KE8YGWLogger/KE8YGWLogger.xcodeproj/project.pbxproj`
- `ios/KE8YGWLogger/KE8YGWLogger.xcodeproj/xcshareddata/xcschemes/KE8YGWLogger.xcscheme`
- `ios/KE8YGWLogger/KE8YGWLogger/App/KE8YGWLoggerApp.swift`
- `ios/KE8YGWLogger/KE8YGWLogger/Models/QSO.swift`
- `ios/KE8YGWLogger/KE8YGWLogger/Models/StationProfile.swift`
- `ios/KE8YGWLogger/KE8YGWLogger/Models/AppSettings.swift`
- `ios/KE8YGWLogger/KE8YGWLogger/Services/HamRadioUtilities.swift`
- `ios/KE8YGWLogger/KE8YGWLogger/Services/LogExportService.swift`
- `ios/KE8YGWLogger/KE8YGWLogger/Views/*.swift`
- `ios/KE8YGWLogger/KE8YGWLoggerTests/*.swift`
- `ios/KE8YGWLogger/KE8YGWLogger/Resources/Info.plist`
- `ios/KE8YGWLogger/README.md`
- `README.md`
- `ROADMAP.md`
- `PROJECT_STATE.md`

Architectural decisions:

- The iOS app is a separate Apple-native project under `ios/` and does not alter the Rust workspace build.
- SwiftData is used for local-first iOS persistence on iOS 17+.
- ADIF/CSV export and ham-radio utility logic are pure Swift services with unit tests.
- Apple paid capabilities, CI, TestFlight, iCloud, push notifications, and associated domains are intentionally not configured.

New plugins:

- None.

Breaking changes:

- None.

## 2026-07-07

Summary: Added durable support storage for MVP sidecar state and persisted service provider settings, service cache metadata, upload queue state, map layer preferences, lookup/rig UI config, and online support state.

Major files changed:

- `crates/ham-core/src/support.rs`
- `crates/ham-core/src/service.rs`
- `crates/ham-core/src/map.rs`
- `crates/ham-core/src/lib.rs`
- `crates/ham-gui/src/main.rs`
- `crates/ham-gui/web/app.js`
- `README.md`
- `ROADMAP.md`
- `docs/architecture/support-storage.md`
- `PROJECT_STATE.md`

Architectural decisions:

- Support storage is versioned JSON sidecar state, not official log state.
- Support storage stores provider settings, cache metadata, upload jobs, map preferences, and UI config, but never credential secret values.
- The GUI publishes `support.storage.*` runtime events for support storage open/load/save/error activity.
- Service provider enablement and priority changes now flow through a persisted core-backed endpoint.

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
