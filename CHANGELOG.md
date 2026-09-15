# Changelog

## Unreleased

### Changed

- **Breaking (self-hosted API):** the self-hosted sync routes moved under an
  `/api/v1/self-hosted` prefix so the hosted and self-hosted contracts can be
  served by one binary on one port without colliding on `/api/v1/logbooks` and
  `/api/v1/sync/status`. `POST /api/v1/auth/pair`,
  `GET /api/v1/logbooks`, `GET|POST /api/v1/logbooks/:logbook_id/{head,events,preview-pull,pull,push}`,
  `GET /api/v1/sync/status`, `POST /api/v1/reports`, and
  `GET /api/v1/reports/:report_id` are now served at the same paths under that
  prefix. The old paths no longer resolve. `/health` is unchanged and now
  answers for both services. The hosted contract is unchanged.
- Restructured the workspace from eleven crates into one library and two
  binaries. `ham-core` holds everything identical across platforms;
  `ham-server` and `ham-client` are the binaries built on it; `ham-ios-ffi` and
  `src-tauri` remain as the iOS and desktop packaging shells.
- Folded `ham-plugin-sdk`, `ham-api-contract`, `ham-sync`, `ham-desktop`, and
  the `ham-gui` shell models into `ham-core` as the `plugin_sdk`,
  `api_contract`, `sync`, `desktop`, and `gui` modules.
- Merged `ham-sync-server` into `ham-server`. One process now serves both route
  trees on one port behind a shared HTTP layer, replacing two listeners and two
  hand-rolled HTTP stacks. `HAM_SERVER_BIND` replaces `HAM_SYNC_SERVER_BIND`,
  and the default bind is `127.0.0.1:9750`.
- Merged `ham-cli` and `ham-gui` into `ham-client`. `ham-client serve` runs the
  local web UI server; the former `ham-cli` commands are subcommands of the same
  binary.
- Moved the durable SurrealDB sync and report storage out of the shared library
  into `ham-server`, so client and iOS builds no longer link SurrealDB. The
  `ham-sync` `surreal-storage` and `hosted-http` feature flags are gone.
- Renamed `Dockerfile.sync-server` to `Dockerfile.server`; it now builds
  `ham-server` and exposes port 9750.
- Release archives now package the `ham-client` binary instead of `ham-gui`.
## 0.5.1

### Added

- Added switchable shell layouts to the desktop and web GUI. Five layouts —
  Operating Deck, Command Center, Field Notebook, Focus Console, and Tabbed
  Workbench — rearrange the same workspaces and panels rather than replacing
  them, so switching keeps the current workspace, any draft contact, and the
  operator's own card arrangement. The catalog lives in `ham_gui::shell` with
  the density and permanent-entry facts the settings screen shows.
- Added a light and dark theme to the GUI, plus `system` to follow the host.
  Every colour resolves through tokens on `:root[data-theme]`, so a layout never
  has to know which theme is active.
- Added `POST /api/shell/appearance` and `support/shell-appearance.json`, so the
  layout and theme survive a restart. `GET /api/shell` now serves the saved
  appearance along with the layout and theme catalogs. An unrecognized slug is
  rejected without clobbering the saved choice.
- Added `shell.layout.cycle`, `shell.theme.light`, `shell.theme.dark`, and
  `shell.theme.system` to the command registry, and `Ctrl/Cmd+Shift+L` to step
  through the layouts without leaving the log.
- Added three switchable iOS dashboards — Today, Logbook, and Map & Sheet —
  selectable from the dashboard toolbar or Settings, with the appearance mode
  applied above the whole shell so sheets and popovers are themed too.
- Added `display.desktop_shell_layout` and `display.mobile_dashboard_layout` to
  `ham_core::ApplicationSettings`, with `DESKTOP_SHELL_LAYOUTS`,
  `MOBILE_DASHBOARD_LAYOUTS`, and `APPEARANCE_MODES` as the shared vocabulary.
  Both fields default when absent, so settings written before layout switching
  existed still load.

- Added `ham_sync::push_replication_status`, the single classifier that hosted,
  self-hosted, and in-memory sync servers use to report a push as `pulled`,
  `diverged`, or `rejected`.
- Added `ham_core::contest`, the versioned contest rule and exchange schema:
  contest definitions, bands and modes, sent/received exchange fields, entry
  categories, duplicate scope, serial policy, multipliers, ordered scoring
  rules, time windows, and export identity. Definitions are data, so a
  corrected or newly published rule set can reach operators without an
  application build.
- Added signed contest definition packs. A distributed pack carries a detached
  HMAC-SHA256 signature over its canonical bytes; `ContestPackTrustStore`
  accepts only packs signed by a key it holds, and an unsigned pack, an unknown
  key id, a changed digest, and a bad signature are each a distinct error.
- Added `ContestDefinitionCatalog`, which starts from the compiled-in
  definitions, applies installed packs, keeps the highest `rule_version` per
  contest, and records each definition's provenance.
- Added the built-in generic serial and generic grid definitions in
  `crates/ham-core/assets/contest-definitions-v1.json`, loaded through the same
  path as an operator-installed pack.
- Added `ham_core::emcomm`, the append-only incident, operational period,
  personnel, assignment, message, and activity-log model behind ICS 211, 213,
  213RR, and 214, with a rebuildable `EmCommProjection`, per-record event
  history, precedence ordering, unacknowledged-traffic lookup, and a complete
  incident package export.
- Added station-scoped EmComm message numbers (`PREFIX-NNNN`) that two
  disconnected stations can allocate without coordinating, plus
  `next_message_number`, which counts only numbers minted by the asking
  station.
- Added `official.log.emcomm.*` official events, `proposal.emcomm.*` proposals
  with payload validation, and the `emcomm.*` plugin capabilities
  (`view`, `incident.manage`, `period.manage`, `person.manage`,
  `assignment.manage`, `message.manage`, `activity.log`).
- Added `docs/CONTEST_RULE_SCHEMA.md`, `docs/EMCOMM_RECORD_MODEL.md`, and
  `docs/V0_5_1_RELEASE_PLAN.md`.
- Added a desktop network scan: `POST /api/sync/discovery/scan`, a `Scan Network`
  button in the Sync Status panel, and the `sync.discovery.scan` command. One
  scan announces and listens for longer than a peer discovery interval and, in
  parallel, probes the local private/link-local IPv4 subnets on the bound GUI
  port, the default GUI port, and the configured local sync port, so instances
  are found on networks that drop multicast and instances that never started
  discovery. Probed addresses are only recorded after serving a matching
  `/api/sync/state` identity, and `scan_running`/`last_scan` in
  `/api/sync/state` report progress and coverage. The scan summary counts
  distinct instances in `peers_found` and keeps sighting counts separate in
  `multicast_observations` and `probed_responses`.
- Added `ham_sync::LanDiscoveryService::scan_once`, `local_scan_targets`, and
  `local_scan_interface_addresses` for the shared scan primitives.
- Added `network.scan.started`, `network.scan.completed`, and
  `network.scan.multicast_failed` runtime events.

- Added a JSONL-to-SurrealDB projector in `ham-server` (`src/projector.rs`),
  beside the SurrealDB storage it depends on so `ham-core` and the client and
  iOS builds never link SurrealDB for it. It replays the append-only official
  event log one way into `projection_qso` and `projection_activation`, verifies
  the per-logbook hash
  chain before projecting each entry, halts durably at a broken chain, projects
  tombstones as projection-level removal instead of row deletion, commits rows
  and its checkpoint in one transaction per batch, resumes from that checkpoint,
  tails newly appended entries, records orphan-tombstone anomalies, and holds a
  single-writer lease over the projection tables.
- Added `ham-server --rebuild-projection` to wipe and replay the SurrealDB
  projection explicitly. The projector otherwise resumes and tails automatically,
  configured by `HAM_SYNC_PROJECTION_ENABLED`, `HAM_SYNC_PROJECTION_BATCH_SIZE`,
  `HAM_SYNC_PROJECTION_POLL_SECONDS`, and `HAM_SYNC_PROJECTION_WRITER_ID`.
- Added `ham_core::projection_touch` and
  `ActivationProjection::activations_for_qso`, so replay consumers can ask
  `ham-core` which projected entities an official event touches instead of
  re-deriving the event vocabulary.
- Added `docs/PROJECTION_PIPELINE.md` covering the projected tables, checkpoint
  location, failure handling, full-rebuild procedure, and measured throughput.

### Changed

- Dropped the unused `ios` output from CI's `Detect changed areas` job. Nothing
  consumed it: iOS validation runs in its own workflow, which cannot read
  another workflow's job outputs. The other seven scopes are unchanged.
- Replaced the GUI's left activity rail with a top menu bar. Navigation now
  costs vertical space, which the shell has, instead of the horizontal space the
  log, band map, and context panels compete for, and the modes carry readable
  labels instead of two-letter glyphs. The eight toolbar buttons moved into a
  More menu beside the command search.
- Reordered the status bar so sync, discovery, runtime events, and errors come
  first. The six map cursor readouts now appear only in workspaces that show a
  map panel.
- The `ham-gui` listener now serves only the LAN sync read endpoints
  (`GET /api/sync/state`, `/api/sync/list-logbooks`, `/api/sync/get-head`,
  `/api/sync/events-since`, `/api/sync/event-metadata`) and reciprocal
  `POST /api/sync/lan/pairing-accept` to non-loopback requesters. The browser
  UI and every unauthenticated control endpoint,
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
- Contest definition documents reject unknown fields. A pack that carries a
  rule concept this build does not implement fails to load rather than loading
  with that rule silently ignored, and a pack written against a newer
  `schema_version` is rejected whole.
- An older contest definition pack can no longer downgrade a contest that has
  already been updated to a higher `rule_version`.
- EmComm corrections append rather than overwrite: each record keeps the merged
  current payload and the ordered history of every event that produced it, so a
  transmitted message keeps its transmission entry after it is cancelled. An
  empty correction is rejected, and a message number is assigned once and
  cannot be changed by a correction.
- Unified every release surface on product version `0.5.1`: Cargo workspace
  metadata, Tauri configuration, iOS marketing version, OpenAPI product
  metadata, the CLI version assertion, iOS Rust-bridge fallback payloads, the
  governance version pin, and documentation. The iOS build number moves to `3`
  and the frozen `/api/v1` `info.version` is unchanged.
- Closed the repository/architecture baseline (#3) and the accounts, API
  contract, and hosting-modes epic (#4) after auditing their remaining child
  issues against the shipped code.

- `ActivationProjection::apply` now recomputes derived counters only for the
  activations an event touches, instead of every activation on every event.
  Replaying 40,050 events drops from about 126 seconds to about 1.2 seconds.
  This also speeds up every other caller of `rebuild_activation_projections`,
  including proposal validation and the hosted activation routes. Projected
  counters are unchanged.

### Fixed

- CI no longer reports success when change detection fails. The `CI result`
  gate did not depend on the `Detect changed areas` job, and every validation
  job does. A failure there — an unreachable `github.event.before` after a force
  push is enough — skipped all of them, and the gate counted those skips as
  passes, so a run that validated nothing reported green and still published an
  internal or beta artifact. The gate now requires change detection to succeed
  outright, while genuine skips from an unaffected scope still pass.
- CI, iOS Native, and Security scanning no longer cancel in-progress runs for
  pushes to `dev` and `main`. `cancel-in-progress` applied to every event, so
  merging two pull requests in quick succession killed the first commit's run
  and left that commit on a shared branch with no verdict; three commits on
  `dev` are in that state today. It would also have blocked the production
  release of such a commit, because `Release` only publishes a tag whose commit
  has a completed successful `main` CI run. Superseded pull request runs are
  still cancelled.
- The documented `hotfix/*` exception for pull requests into `main` works
  again. `Main promotion policy` honored it, but the separate `Branch promotion
  policy` check rejected every head branch other than `dev`, so an emergency
  hotfix could not be merged by the route
  `docs/BRANCHING_AND_RELEASE_CHANNELS.md` prescribes. Both checks now call one
  shared `scripts/check-promotion-policy.sh`, so they cannot drift apart again.
  `Main promotion policy` also gained the head-repository check it was missing,
  which the other check already had.
- `Ubuntu preflight and policy` no longer reports "Only files outside CI
  validation scopes changed" when Tauri or container validation is running; its
  condition covered only the scopes checked inside that job.
- iOS LAN discovery no longer fails with `Network.NWError error 48 - Address
  already in use`. The scanner bound one `NWConnectionGroup` per address family,
  so the IPv4 and IPv6 groups fought over the same UDP discovery port, and a
  stop/start toggle rebound the port before the cancelled group had released it.
  A single group now joins both multicast endpoints on one socket, single-family
  groups are only a fallback, a restart waits for the outgoing group to report
  `cancelled`, and an address-in-use failure is reported as an actionable
  message instead of the raw `NWError`.
- iOS Sync `Issue Code`, `Accept Code`, `Pair With URL`, `Trust Peer`,
  `Rotate Auth`, and `Revoke` no longer fire together. A SwiftUI `List` row makes
  its whole area one tap target, so every button sharing a row ran from a single
  tap and `Pair With URL` or `Trust Peer` reissued a pairing code instead. The
  rows now use the borderless button style, which gives each button its own hit
  region, and the LAN actions are split across rows so the labels are reachable
  on a phone.

### Testing

- Added `ham-sync` scan tests for local subnet target generation, the target
  budget, and the private/link-local-only sweep policy.
- Added `ham-gui` scan tests for the probed port set, targets staying inside the
  manual LAN peer address policy, the short-timeout identity probe against a
  responding and a closed address, and repeated sightings of one instance
  counting as a single peer.
- Added iOS tests for the single-socket multicast bind candidates and the
  address-in-use failure message.
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
- Added 22 `ham-core` contest schema tests covering built-in pack loading,
  newer-schema and unknown-kind rejection, unknown-field rejection, serial and
  multiplier consistency, duplicate contest ids, signed and tampered packs,
  unsigned and unknown-key refusal, rule-version upgrade and downgrade
  protection, exchange validation and normalization, duplicate keys, serial
  sequences, scoring precedence, time windows, and digest stability.
- Added 10 `ham-core` EmComm tests covering the incident/period/person/
  assignment lifecycle, corrections that append history, message delivery
  states, message-number assignment and malformed-number rejection, offline
  station-scoped numbering, precedence ordering and unacknowledged traffic, the
  ICS 214 activity log, the incident package and its per-record history, and
  capability enforcement.

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
