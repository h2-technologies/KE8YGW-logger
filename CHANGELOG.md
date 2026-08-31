# Changelog

## 0.4.0

### Added

- Added `ham_sync::push_replication_status`, the single classifier that hosted,
  self-hosted, and in-memory sync servers use to report a push as `pulled`,
  `diverged`, or `rejected`.
- Added `ham_sync::account`, the shared hosted account and session client used by
  every platform: bounded action vocabulary, hosted request planning, response
  interpretation, stable outcome classification, transport-failure
  classification, and a versioned JSON support store with atomic writes and
  corrupt-file quarantine.
- Added a shared blocking HTTPS hosted account transport behind the new
  `ham-sync` `hosted-http` feature, used by the desktop/hosted web GUI and the
  CLI. Native iOS keeps its own URLSession transport.
- Added `/api/account/*` GUI endpoints for hosted account state, endpoint
  configuration, registration, email verification, recovery start/complete,
  sign-in, session read/rotate, sign-out, sign-out-everywhere, device
  list/revoke/revoke-all, and account deletion.
- Added a browser Account screen, an Account settings summary card, an Account
  toolbar entry, and `account.*` command-palette commands for hosted web and
  desktop.
- Added redacted `account.*` runtime events for every hosted account action.
- Added `account.snapshot`, `account.configure`, `account.plan`,
  `account.apply`, and `account.transport_failure` iOS bridge commands.
- Added typed Swift hosted account bridge methods, a URLSession hosted account
  transport, Keychain storage of Rust-assigned session/refresh credentials, and
  an iOS Account workspace with a dashboard quick action.
- Added `ham-cli account` subcommands with stable `--json` output for status,
  configure, register, verify-email, recovery-start, recovery-complete, login,
  session, rotate, logout, logout-all, devices, revoke-device,
  revoke-all-devices, and delete.
- Added `docs/V0_4_RELEASE_PLAN.md`.
- Added `IOS_GAP_ANALYSIS.md`.
- Added `ham-ios-ffi`, a Rust FFI crate for iOS JSON bridge calls backed by `ham-core` and `ham-sync`.
- Added a hardened byte-buffer Rust FFI command ABI with structured envelopes, ABI/schema versions, correlation IDs, panic containment, bounded input checks, and explicit deallocation.
- Added public iOS FFI header/module map and macOS scripts for Apple Rust targets, static libraries, XCFramework assembly, and linkage verification.
- Added Xcode pre-link Rust build phase and relative static-library linkage for reproducible iOS Rust linking.
- Added iOS bridge client, bridge fallback contract, and bridge fallback tests.
- Added Swift typed bridge DTOs for QSO, station, activation, Net Control, diagnostics, and bridge self-test operations.
- Added SwiftData projection metadata and `ProjectionRefreshService`.
- Added macOS GitHub Actions workflow for Rust FFI, XCFramework, and iOS simulator validation.
- Added `docs/IOS_BUILD_AND_LINKING.md`.
- Added native iOS split-view shell and feature workspaces for Dashboard, Logging, Callsign Lookup, Stations, Providers, Maps, POTA, SOTA, Net Control, Emergency, Sync, Backup/Restore, Diagnostics, and Settings.
- Added Keychain credential storage and local notification authorization plumbing.
- Added SwiftData station equipment cache model.
- Added a recoverable iOS projection cache: a SwiftData store that cannot be opened is quarantined and rebuilt from the Rust event store, with an in-memory fallback and a recovery screen instead of a launch crash.
- Added `ProjectionStoreTests` covering the projection cache recovery ladder, quarantine, and quarantine pruning.

### Changed

- The `ham-gui` listener now serves only the LAN sync read endpoints
  (`GET /api/sync/state`, `/api/sync/list-logbooks`, `/api/sync/get-head`,
  `/api/sync/events-since`, `/api/sync/event-metadata`) and reciprocal
  `POST /api/sync/lan/pairing-accept` to non-loopback requesters. The browser UI and every unauthenticated control endpoint,
  including QSO/Net Control writes, credential, backup, LAN pairing, and cloud
  controls, now require a loopback requester or the explicit
  `HAM_GUI_ALLOW_REMOTE_CONTROL_API=1` opt-in. Automatic LAN discovery needs a
  LAN-reachable bind, which previously exposed all of that surface to the
  network. Rejections return `403` and publish a redacted
  `sync.lan.control_api.rejected` runtime event.
- Hosted `POST /api/v1/sync/push` now rejects a push whose event envelopes carry
  a `logbook_id` other than the authorized request `logbook_id` with
  `403 forbidden`. The route previously authorized only the request field, so a
  session with write access to one logbook could append official events into
  another logbook.
- Hosted `POST /api/v1/sync/push` now reports a branch that does not continue the
  server head as `diverged` instead of `rejected`, matching the self-hosted push
  route so desktop and iOS clients stop unattended retry and open a manual
  conflict review on every transport.
- Hosted account session and refresh tokens are now stored only in the
  operating-system credential backend or the iOS Keychain under Rust-assigned
  credential identifiers; the durable account record, GUI responses, CLI output,
  and runtime events stay token-free.
- `ureq` is now a workspace dependency shared by `ham-server` and the optional
  `ham-sync` `hosted-http` feature.
- Unified every release surface on product version `0.4.0`: Cargo workspace
  metadata, Tauri configuration, iOS marketing version, OpenAPI product
  metadata, the CLI version assertion, iOS Rust-bridge fallback payloads, and
  documentation. The iOS build number and the frozen `/api/v1` `info.version`
  are unchanged.
- Updated `ROADMAP.md`, `docs/V1_EXECUTION_PLAN.md`, `docs/API_CLIENT_CONTRACT.md`,
  `docs/SECURITY_MODEL.md`, `docs/EVENT_CATALOG.md`, `docs/CLI_REFERENCE.md`, and
  `PROJECT_STATE.md` for the hosted account milestone. The remaining
  account-area client gap is server administration UX.
- Expanded iOS QSO, station profile, settings, export, logbook, and detail models/views for MVP parity fields.
- Routed iOS QSO create/delete, station profile/equipment/select, POTA/SOTA activation start/end, and Net Control session/check-in/traffic mutations through Rust bridge commands.
- Reclassified SwiftData QSO/station/equipment state as cache/projection data for Rust-accepted state.
- iOS ADIF export now prefers the Rust bridge and falls back to Swift export if the bridge is unavailable.
- Updated `PROJECT_STATE.md`, `ROADMAP.md`, and iOS documentation for the parity pass.
- Hardened iOS Rust build scripts to load Rust/Homebrew paths in Xcode archive shells and removed a developer-specific Xcode run script path.
- iOS no longer calls `fatalError` when the SwiftData model container cannot be created, which crashed the app at launch (TestFlight 0.3.0 build 149, `EXC_BREAKPOINT` in `App.main()`); the container is now created with staged recovery.
- iOS Diagnostics now reports projection cache health and includes it in the exported diagnostics report.

### Testing

- Added `ham-gui` tests for the LAN read allow-list, the loopback and opt-in
  decision matrix, and the redacted `403` rejection for non-loopback control
  requests.
- Added `ham-server` regression tests for cross-logbook sync push rejection and
  for hosted divergence reporting plus pull-then-reapply reconciliation.
- Added a `ham-sync-server` loopback HTTP test proving the durable self-hosted
  surface refuses a divergent branch without changing the head or official log,
  reports a `diverged` preview for an unknown local head, reconciles after a
  pull, and ignores duplicate replay of the reconciled chain.
- Added `ham-ios-ffi` tests for `sync.offline_queue.recover`: first-launch queue
  initialization, legacy `version: 0` migration, corrupt-queue quarantine with
  the original bytes preserved, and interrupted atomic-write promotion.
- Added `ham-sync` hosted account tests for URL/email normalization, request
  planning, session-required rejection, accepted sign-in with credential-id-only
  persistence, refresh-token rotation, remote session revocation recovery,
  device revocation, transport-failure retryability, error-code classification,
  registration state, sign-out credential clearing, corrupt-record quarantine,
  and unsupported record versions.
- Added `ham-gui` tests for hosted account state defaults, endpoint
  configuration persistence and rejection, session-scoped request rejection, and
  malformed client JSON.
- Added `ham-ios-ffi` tests for configure/plan/apply round-trips, secret
  handover without persistence, session-required rejection, and persisted
  transport failures.
- Added Swift `RustBridgeTests` cases for hosted account snapshots, Keychain
  storage of issued secrets, bearer-token use, refresh-token injection, offline
  transport classification, credential clearing, and missing-token rejection.
- Ran `cargo fmt --all -- --check`.
- Ran `cargo clippy --locked --workspace --all-targets -- -D warnings`.
- Ran `cargo check`.
- Ran `cargo test -p ham-ios-ffi`.
- Ran full `cargo test --locked --workspace` with 345 Rust tests passing.
- Ran a live `ham-cli` account flow against a local `ham-server` binary:
  configure, sign in, session read, device list, session rotation, sign-out, and
  recovery from a remotely revoked session.
- Xcode/iOS simulator tests were not run because this workspace does not provide macOS/Xcode tooling.
