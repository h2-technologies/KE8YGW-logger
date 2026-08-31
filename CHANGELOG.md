# Changelog

## 0.5.0

### Added

- Added `ham_sync::admin`, the shared hosted server administration client used by
  every platform: bounded action vocabulary, hosted request planning, response
  interpretation, and a versioned JSON support store with atomic writes and
  corrupt-file quarantine. Administration is scoped to the server the hosted
  account is signed in to, so the endpoint and the session credential both come
  from the account record and there is no second endpoint setting to drift.
- Added a shared blocking HTTPS hosted administration transport behind the
  existing `ham-sync` `hosted-http` feature, used by the desktop/hosted web GUI
  and the CLI. Native iOS keeps its own URLSession transport.
- Added `/api/admin/*` GUI endpoints for administration state, hosting
  read/update, invitation list/create/inspect/resend/expire/revoke, and audit
  review.
- Added a browser Admin screen, an Admin toolbar entry, and `admin.*`
  command-palette commands for hosted web and desktop.
- Added redacted `admin.*` runtime events for every hosted administration action.
- Added `admin.snapshot`, `admin.plan`, `admin.apply`, and
  `admin.transport_failure` iOS bridge commands.
- Added typed Swift hosted administration bridge methods, a URLSession
  administration transport, and an iOS Admin workspace with a dashboard quick
  action.
- Added `ham-cli admin` subcommands with stable `--json` output for status,
  hosting, set-hosting, invitations, invite, invitation, resend, expire, revoke,
  and audits.
- Added a one-time instance-administrator bootstrap on every surface:
  the `account.bootstrap` shared action, `ham-cli account bootstrap`, a
  `POST /api/account/bootstrap` GUI endpoint, and a browser "Claim server
  administrator" form. An operator can now create the first administrator on a
  fresh server from a client instead of by hand.
- Added `docs/V0_5_RELEASE_PLAN.md`.

### Changed

- Hosted administration records whether the signed-in account is a server
  administrator rather than assuming it. An accepted response on any admin-gated
  route proves rights, a `forbidden` response records that the account is not an
  administrator and drops the cached hosting/invitation/audit state, and an
  authentication failure returns rights to unknown. A transport failure keeps
  what the operator already loaded.
- Hosting configuration updates send only the fields the operator set, so an
  update never rewrites a hosting value they did not look at. Empty updates and
  non-positive lifetimes are rejected before a request is sent.
- The single-use invitation token issued by invitation create and resend is
  returned exactly once and is excluded from every serialized form of the result
  and the durable record. It is never written to the support record, to platform
  secure storage, or to a runtime event.
- Changing the hosted account endpoint resets the cached administration record,
  so hosting, invitation, and audit state cannot be shown for the wrong server.
- Invitations and audit records retained in the durable administration record
  are stored newest-first and bounded, so a long-lived server cannot grow the
  support file without limit.
- Unified every release surface on product version `0.5.0`: Cargo workspace
  metadata, Tauri configuration, iOS marketing version, OpenAPI product
  metadata, the CLI version assertion, iOS Rust-bridge fallback payloads, the
  governance version pin, and documentation. The iOS build number and the frozen
  `/api/v1` `info.version` are unchanged.
- Updated `ROADMAP.md`, `RELEASE.md`, `PROJECT_STATE.md`,
  `docs/V1_EXECUTION_PLAN.md`, and `docs/CLI_REFERENCE.md` for the server
  administration milestone.

### Testing

- Added `ham-sync` hosted administration tests for session-scoped planning,
  admin route construction, partial and rejected hosting updates, mode/role
  parsing, accepted hosting reads, invitation-token single-use handling and
  non-persistence, invitation lifecycle status, cached-invitation replacement,
  `forbidden` and revoked-session handling, missing stored secrets, transport
  failures, bounded newest-first audit listings, endpoint-change resets, corrupt
  record quarantine, and unsupported record versions.
- Added `ham-gui` tests for administration state defaults, session-scoped
  request rejection, unknown hosting modes, empty and non-positive hosting
  updates, unknown invitation roles, and malformed client JSON.
- Added `ham-ios-ffi` tests for admin plan/apply round-trips, invitation-token
  handover without persistence, session-required rejection, `forbidden`
  classification, and persisted transport failures.
- Added Swift `RustBridgeTests` cases for administration snapshots, bearer-token
  use, missing stored tokens, single-use invitation-token handling, offline
  transport classification, invitation lifecycle rules, and partial hosting
  update encoding.
- Ran `cargo fmt --all -- --check`.
- Ran `cargo clippy` with `-D warnings` across every crate that builds in this
  workspace.
- Ran the Rust test suite with 363 tests passing.
- Ran a live `ham-cli admin` flow against a local `ham-server` binary: bootstrap,
  hosting read, two single-field hosting updates, invitation create, list,
  inspect, resend, expire, revoke, a server-refused resend after revocation, and
  audit review; then read the same durable record back through the `ham-gui`
  `/api/admin/state` endpoint and created an invitation and a hosting update
  through the browser endpoints.
- Verified no invitation token reaches any support file or runtime log.
- Xcode/iOS simulator tests were not run because this workspace does not provide
  macOS/Xcode tooling.
- `cargo clippy --workspace --all-targets` and `cargo test --workspace` could not
  include `ham-desktop`: this container has no `gdk-3.0`, so its `gdk-sys` build
  script fails. That failure reproduces unchanged on `dev` without these changes.

## 0.4.0

### Added

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
- The Tauri desktop app now sets `app.withGlobalTauri`, which injects
  `window.__TAURI__`. Without it the web UI never found `invoke`, so its
  desktop `/api/*` bridge and native file dialogs were never installed and
  `/api/shell` fell through to Tauri's `index.html` asset fallback; the app
  opened on "GUI failed to start" with `SyntaxError: Unexpected token '<'`.
  The bridge now also accepts `window.__TAURI_INTERNALS__.invoke`, falls back
  to the default `http://127.0.0.1:9467` API when `desktop_runtime` reports no
  server URL, and reports a non-JSON `/api/shell` response by endpoint,
  status, and content type. The startup failure screen escapes the error text
  so markup in a message is no longer swallowed by `innerHTML`.

### Testing

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
