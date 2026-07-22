# Project State

Last audited: 2026-07-22

Canonical product version: `0.3.0` from `Cargo.toml`
`[workspace.package].version`.

Locked v1 release target: November 24, 2026 with hosted web, native iOS, and
Windows/macOS/Linux desktop. The locked scope is issue #2. v1.1 adds a TUI.
Awards, rig control, and weather do not block v1.

## Baseline Status

This repository is a v1 foundation baseline, not a complete v1 product. It has
the shared architecture, native-client foundations, release-policy baseline,
governance, and cross-platform automation needed to start full v1
implementation.

Open baseline items after this branch merges should move to the remaining v1
epics for sync, providers, hosted web, desktop, iOS, maps, contesting, EmComm,
operations, and release qualification.

## Implemented

- Shared Rust workspace with `ham-core`, `ham-plugin-sdk`, `ham-sync`,
  `ham-sync-server`, `ham-server`, `ham-cli`, `ham-gui`, `ham-desktop`,
  `ham-api-contract`, `ham-ios-ffi`, and `src-tauri`.
- Append-only official events, deterministic event hashing, QSO proposals,
  tombstone/restore/note flows, projections, ADIF import/export, station and
  equipment support state, awards/search foundations, upload queue foundation,
  maps/GIS foundations, diagnostics, runtime JSONL logs, and support storage.
- POTA/SOTA activation proposals/projections and Net Control official events,
  proposals, projection, and report export events.
- Hosted `/api/v1` route slices for server-admin bootstrap, hosting
  configuration, invitation management, registration, verified email, recovery,
  session/device/logbook, QSO, station/equipment, ADIF, providers, uploads,
  activations, Net Control, maps, backups, divergence review, and sync.
- Durable hosted SurrealDB metadata for server admins, users, token hashes,
  sessions, devices, logbooks, memberships, invites, verification/recovery
  tokens, rate limits, audits, and support state; durable self-hosted
  sync/report metadata, JSONL official-event storage, and filesystem diagnostic
  report payloads.
- Durable offline mutation queue schema and JSON support store in `ham-sync`,
  including operation/device/client/logbook IDs, optional target entity IDs,
  deterministic per-logbook ordering, idempotency keys, dependency checks,
  retries/backoff, recovery of interrupted sends, deterministic desktop
  restart/reconnect queue-drain coverage, desktop cloud reconnect auto-drain
  when auto-push is enabled, native iOS Swift retry-plan/result bridge wiring
  plus typed queue health, Rust-planned official-event envelope decoding,
  self-hosted/logbook-scoped push execution coordination, hosted
  `/api/v1/sync/push` request construction, partial-acceptance retry-result
  handling, and conflict-review display, v0.2 absent/legacy queue migration,
  corrupt queue quarantine, interrupted atomic-write promotion, redacted queue
  health, structured conflict reports for divergent heads, missing
  dependencies, unsupported schemas, concurrent QSO corrections, and
  tombstone/restore overlaps, durable manual conflict-review records, explicit
  recovery-path decisions, and LAN trust records with short-lived single-use
  pairing tokens, replay nonce rejection, immediate revocation, and
  HMAC-SHA256 signed LAN read endpoint authorization for logbook/head/event
  APIs.
- Tauri v2 desktop wrapper with bundled web assets, native dialog commands, and
  restricted `/api/*` proxying.
- Native iOS SwiftUI project, SwiftData cache/projection models, Rust FFI bridge,
  public header/module map, Apple build/link scripts, shared scheme, unit tests,
  and macOS/iOS simulator workflow.
- Repository governance, MIT license, issue/PR templates, CODEOWNERS, release
  policy, branch/channel policy, private vulnerability reporting guidance,
  Dependabot config, security workflow, Scorecard workflow, and pinned
  workflow/build supply-chain dependencies from PR #101.
- Deterministic checks for API contract, product version consistency, Markdown
  links, governance/license/secrets, release-artifact naming, and production tag
  policy.

## Partial

- Hosted accounts have a v1 foundation for personal/public/self-hosted modes:
  invite-only registration by default, administrator open/disabled switches,
  hashed expiring single-use invite/verification/recovery tokens, verified
  email gating, Turnstile fail-closed public registration, secure-cookie/bearer
  sessions, refresh rotation, logout-all, device revocation, account deletion,
  request IDs, audits, and durable configurable rate limits. Production hosted
  web UI wiring, external email deliverability, infrastructure sizing,
  retention, monitoring, and deployment secrets remain incomplete.
- Sync has discovery, handshake, preview/pull/push verification models, durable
  self-hosted backend, desktop queue integration for QSO/activation/Net Control
  official mutations and station-profile support state, iOS queue integration
  for QSO/activation/Net Control/station/equipment commands, queue-aware cloud
  push acknowledgment, LAN trust persistence/endpoints, structured divergence
  reports, durable manual conflict-review create/resolve commands, and GUI
  manual direct LAN HTTP preview/pull transport, HMAC-SHA256 proof-of-possession
  for protected LAN read endpoints, explicit LAN auth credential
  rotation/recovery through the GUI trust endpoint, desktop/iOS corrective-event
  commands that submit explicit proposals and resolve reviews with generated
  official event hashes, guided browser conflict-review selection, structured
  conflict summaries, explicit recovery-path buttons, form-based corrective QSO
  note events, and automatic IPv4/IPv6 multicast discovery that probes peer
  identity before recording reachable peers, plus a guided browser LAN
  pairing/trust panel for issuing one-time codes, completing reciprocal pairing,
  rotating LAN auth credentials, and revoking trusted peers without prompt-only
  handling. Native iOS Swift decodes saved conflict-review records, displays
  open-review status, recommended actions, peer IDs, and structured conflict
  messages in the Sync workspace. Native iOS also exposes Rust-owned LAN trust
  snapshots, one-time local pairing-code issue, direct peer trust, LAN auth
  credential rotation, and revocation through FFI commands and a minimal Sync
  workspace trust section; generated LAN auth secrets stay in Keychain and Rust
  support state stores only credential IDs. Native iOS can plan through Rust,
  execute the configured sync-token push path through Swift transport, split an
  accepted server prefix from a rejected tail, record accepted/auth/divergence
  outcomes back through Rust-owned retry results, fetch
  self-hosted/logbook-scoped pull responses through Swift transport, and apply
  pulled official envelopes through `sync.remote_events.apply` without owning
  event validation.
  Simulator-safe fallback tests cover review creation/decoding, selected
  recovery-path resolution, retry execution acceptance, auth-failure
  user-action stops without token leakage, pull request construction, pull
  fetch/apply coordination, pulled-event apply decoding, partial divergence
  result recording, and LAN trust snapshot/issue/trust/rotate/revoke decoding
  without pairing-code persistence. Production iOS reciprocal pairing completion UX,
  stronger LAN key-exchange hardening, end-to-end cross-device
  reconciliation workflow qualification, physical-device LAN/iOS local-network
  validation, and iOS background scheduler validation are incomplete.
- Providers have metadata, fake/default execution, credential references,
  hosted QRZ XML/HamQTH lookup, POTA spot fetch, bounded DX Cluster controls,
  and gated Club Log/QRZ Logbook/eQSL live uploads; LoTW/TQSL, SOTAWatch live,
  RBN, propagation/weather/map production adapters, confirmation download, and
  full provider release qualification are incomplete.
- Desktop has a real Tauri wrapper and native dialog bridge; signed packaging,
  updater behavior, notarization, Trusted Signing, and cross-runner installer
  qualification are incomplete.
- iOS has native SwiftUI/Rust bridge foundations; App Store signing,
  TestFlight/App Store distribution, full offline/sync reconciliation, cached
  maps, provider setup, contesting, EmComm, device/archive validation, and
  production privacy review are incomplete.
- Maps have reusable GIS, layer, marker, grayline, weather, and propagation
  models; interactive tile/vector rendering and cached/offline regions are
  incomplete.

## Test-Only, Mock, Fake, Or Stub

- Mock lookup and rig providers.
- Placeholder map/weather/propagation providers.
- Deterministic fake/default online provider execution for ordinary tests.
- In-memory hosted metadata and sync backends used by tests.
- GUI demo LAN peer and demo runtime events.
- iOS Rust-bridge fallback payloads used when the Rust library is unavailable.

## Deferred Or Unimplemented For v1

- Production iOS reciprocal pairing completion UX, stronger LAN key-exchange
  hardening, and physical-device validation beyond the current browser pairing
  panel, HMAC-SHA256 signed LAN read endpoints, GUI auth-rotation path, and
  native iOS LAN trust snapshot/issue/trust/rotate/revoke bridge.
- End-to-end cross-device branch review and reconciliation workflow
  qualification beyond the current guided browser review surface, plus
  release-device iOS background task and poor-network validation.
- LoTW/TQSL managed certificate/signing mode, SOTAWatch approved live access,
  RBN/DX background lifecycle, production maps/offline caching, and propagation
  provider qualification.
- Contesting: Field Day, Winter Field Day, generic serial/grid templates,
  release-adjacent December/January contest packs, scoring, dupes, multipliers,
  and Cabrillo export.
- EmComm: ICS 211, 213, 213RR, 214, personnel, assignments, and
  message/communications records.
- Signed desktop updater, package signing/notarization, TestFlight/App Store
  release, production infrastructure, operations runbooks, and release-candidate
  soak.
- Runtime plugin loading, sandboxing, and signatures.

## Validation Baseline

Local commands:

```powershell
just fmt-check
just clippy
just test
just feature-matrix
just api-contract
just version-check
just docs-link-check
just governance-check
just ci
```

CI coverage:

- `.github/workflows/ci.yml` runs change-aware Rust formatting, Clippy, tests,
  feature matrix, API contract, version consistency, documentation links,
  governance/license checks, JavaScript syntax, Windows/macOS platform checks,
  Tauri validation, sync-server container build/smoke, and internal/beta channel
  manifests.
- `.github/workflows/ios.yml` runs Rust FFI and iOS simulator validation on
  macOS.
- `.github/workflows/security.yml` runs Cargo advisory checks, cargo-deny
  advisories, local Semgrep SAST/SARIF upload, and actionlint.
- `.github/workflows/release.yml` validates production tags, requires the tag to
  match the workspace version and be contained in `main`, checks for successful
  main CI, builds versioned release artifacts, generates checksums, attests
  archives/checksums, and publishes only from validated production tags.

Known manual repository/external settings remain in
`docs/security/REPOSITORY_SECURITY_SETTINGS.md` and
`docs/V1_EXECUTION_PLAN.md`.

## Baseline Issue Audit

| Issue | Status after this branch merges | Evidence |
| --- | --- | --- |
| #15 Native iOS integration | Satisfied for baseline integration; release hardening remains in v1 iOS work | Merged PR #1 and PR #96, `ios/KE8YGWLogger`, `crates/ham-ios-ffi`, `scripts/ios`, `.github/workflows/ios.yml`, `.gitignore`, PR #101 passing iOS checks |
| #16 Scope/docs consistency | Satisfied for baseline docs | `README.md`, `ROADMAP.md`, `docs/ROADMAP.md`, `docs/V1_RELEASE_PLAN.md`, `docs/V1_IOS_NATIVE_PLAN.md`, `docs/IOS_APPSTORE_READINESS.md`, `docs/V1_EXECUTION_PLAN.md`, `AGENTS.md` |
| #17 Version and channels | Satisfied for baseline version/channel policy | `scripts/check_versions.py`, `justfile`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `RELEASE.md`, `docs/BRANCHING_AND_RELEASE_CHANNELS.md`, OpenAPI `x-product-version` |
| #18 Governance/license | Already closed by PR #88; still verified | `LICENSE`, `GOVERNANCE.md`, `CONTRIBUTING.md`, `SECURITY.md`, `SUPPORT.md`, `.github/CODEOWNERS`, templates, `scripts/governance-check.ps1` |
| #19 Cross-platform CI baseline | Satisfied for baseline automation when this branch merges with PR #101 work | CI, iOS, security, scorecard, release workflows; dependency/security docs; version/docs-link/governance checks; container smoke; Tauri/platform validation |

## v1 Account Foundation Issue Audit

| Issue | Status after this branch merges | Evidence |
| --- | --- | --- |
| #21 Registration and hosting modes | Satisfied for server foundation | `HostingConfig`, `RegistrationMode`, one-time `POST /api/v1/admin/bootstrap`, `GET/PATCH /api/v1/admin/hosting`, invitation create/list/inspect/resend/expire/revoke routes, Surreal `hosting_config`, `server_admins`, `server_invites`, `tests::bootstrap_admin_is_single_use_and_stores_only_token_hashes`, `tests::invite_only_registration_requires_single_use_invite_and_email_verification` |
| #22 Verified email and Turnstile | Satisfied for server foundation | `EmailDeliveryConfig`, deterministic test outbox, webhook boundary, `EmailVerificationRecord`, `verify_turnstile_token`, Turnstile Siteverify path with official test-key behavior, `tests::open_registration_turnstile_fails_closed_and_replays_tokens` |
| #23 Session/token/device hardening | Satisfied for server foundation | Hashed session/refresh/API token persistence, session/refresh expiry fields, secure cookie header, logout/logout-all/session rotate/account delete/device revoke/revoke-all routes, reload tests in `tests::sessions_rotate_revoke_and_survive_reload_with_hashes_only` |
| #24 Hosted authorization boundaries | Satisfied for server foundation | Central `authorize`, `require_instance_admin`, `require_logbook_role`, cross-account/logbook negative tests, provider/backup/sync scoping tests in `crates/ham-server/src/lib.rs` |
| #25 Operational limits, request IDs, audits, safe errors | Satisfied for server foundation | `HostedLimitConfig`, `RateLimitRecord`, request ID success/error propagation, `AuditRecord`, provider/sync/account limit enforcement, stable error codes in `ham-api-contract`, `tests::request_ids_limits_and_audits_are_durable_and_redacted` |

## v0.3 Offline Sync Issue Audit

| Issue | Status after this branch merges | Evidence |
| --- | --- | --- |
| #26 Durable idempotent offline mutation envelopes | Satisfied for shared contract | `crates/ham-sync/src/offline.rs`, `JsonOfflineMutationQueue`, schema-version rejection, idempotent enqueue, deterministic sequence tests, retry/recovery tests, `docs/SYNC_PROTOCOL.md`, `docs/V0_3_RELEASE_PLAN.md` |
| #27 Persistent desktop offline queue | Satisfied | GUI persists queue entries before QSO/activation/Net Control/station support mutations, recovers/interprets queue state at startup, exposes queue state/recovery, and cloud push acknowledges queued official event hashes. `desktop_queue_recovers_restart_and_drains_to_cloud_without_duplicates` proves a desktop-style restart/reconnect drain path recovers a `sending` operation, drains queued official events in order, marks accepted entries by event hash, and ignores duplicate cloud replay without creating local duplicates. `cloud_connect_auto_push_drains_recovered_desktop_queue` proves the GUI reconnect path with `auto_push_enabled` recovers an interrupted desktop queue, drains ready queued QSO events to cloud, marks the queue accepted, and does not duplicate local official history. `cloud_connect_auto_push_skips_unqueued_local_history` proves reconnect auto-drain is queue-only and does not push unrelated accepted local history when no offline mutation is ready. Shared recovery initializes v0.2 absent queues, migrates legacy `version: 0` records, promotes interrupted atomic writes, and quarantines corrupt queue JSON. |
| #28 Persistent iOS offline queue | Partially satisfied | `ham-ios-ffi` queues QSO/activation/Net Control/station/equipment commands and exposes queue snapshots/recovery plus Rust-owned conflict-review create/resolve commands. The iOS recovery command uses the shared migration/quarantine recovery report. `sync.offline_queue.retry_plan` and `sync.offline_queue.retry_result` give Swift a Rust-owned background retry contract: bounded batches, sending-state recovery, accepted-hash acknowledgment, transient-failure backoff, and user-action stops for auth, validation, divergence, missing-local-event, and permanent failures. `sync_retry_plan_recovers_terminated_send_and_blocks_without_network` proves a terminated `sending` operation is recovered to retrying and no network attempt is planned while native network state is unavailable. Native Swift now has typed retry-plan/retry-result bridge methods, decodes queue health/mutations and Rust-planned official-event envelopes in the Sync workspace, can build self-hosted/logbook-scoped and hosted `/api/v1/sync/push` requests without creating events in Swift, executes the configured sync-token push path through a Rust-plan -> Swift-transport -> Rust-result coordinator, splits accepted server prefixes from rejected tails, builds self-hosted/logbook-scoped and hosted pull requests, executes a native pull fetch -> Rust apply coordinator, applies pulled official envelopes through `sync.remote_events.apply`, surfaces no-network retry/pull plans, exposes LAN trust support state without raw secrets, and has simulator-safe Swift tests for no-network planning, auth-failure user-action classification without token leakage, event-envelope decoding, push/pull request construction, pull fetch/apply coordination, pulled-event apply decoding, accepted retry execution, partial-divergence retry-result recording, and LAN trust snapshot/issue/trust/rotate/revoke decoding. Release-device BGTask execution, real hosted/self-hosted endpoint qualification, local-network permission behavior, and physical poor-network validation remain. |
| #29 Push/pull/divergence/manual conflict review | Partially satisfied | Existing verified preview/pull/push remains, shared pull apply now accepts either a full remote chain or a verified missing tail that directly follows the actual local head, queue-aware cloud push was added, structured conflict reports classify divergent heads, missing dependencies, unsupported remote schemas, concurrent QSO corrections, and tombstone/restore overlaps, durable manual conflict-review create/resolve commands reject unsafe divergent pulls, desktop/iOS corrective-event commands submit explicit proposals through the normal proposal pipeline before resolving reviews with generated event hashes, and the browser divergence screen now lists saved reviews, summarizes conflicts, records explicit recovery choices, and submits corrective QSO note events through Rust endpoints without prompt-only handling. Native iOS Swift decodes saved review records, displays open-review status, recommended actions, peer IDs, and structured conflict messages in the Sync workspace, can fetch self-hosted/logbook-scoped pull responses, can call `sync.remote_events.apply` to apply pulled official envelopes through shared Rust verification, and simulator-safe fallback tests cover review creation/decoding plus selected recovery-path resolution, pull request/coordinator behavior, and pulled-event apply decoding. End-to-end cross-client branch review/reconciliation workflow qualification remains. |
| #30 Device pairing/trust/revocation/LAN transport decision | Partially satisfied | `JsonLanTrustStore` provides explicit approval, hashed expiring single-use tokens, logbook-scoped trusted devices, auth credential references, auth credential rotation, replay nonce rejection, and immediate revocation; GUI exposes trust endpoints, guided browser pairing/trust controls for issuing one-time local codes, entering peer token/code/fingerprint values, completing reciprocal pairing, generating replacement auth codes, rotating LAN auth, and revoking selected trusted peers, manual direct LAN HTTP peer add/preview/pull, automatic IPv4/IPv6 multicast discovery with reachable identity probing, advertised API-port normalization, HMAC-SHA256 signed LAN list/head/event read endpoints, LAN auth rotation/recovery, and LAN pull rejects untrusted/revoked/replayed peers before local append. iOS FFI now exposes LAN trust snapshot, issue-pairing-token, accept-pairing-token with required auth credential ID, trust-peer, rotate-auth, and revoke commands; the Sync workspace can issue a local code, trust a peer, rotate Keychain-backed LAN auth credentials, and revoke trust while Rust persists only credential IDs. Production iOS reciprocal pairing completion UX, stronger key-exchange hardening, and physical LAN/iOS local-network validation remain. |
| #31 Cross-client sync recovery/migration test suite | Partially satisfied | New deterministic `ham-sync` golden scenarios cover desktop-style crash recovery, transient network retry, accepted-by-hash drain, duplicate replay, reordered delivery rejection, iOS-style pull/projection replay, verified missing-tail pull apply, clock-skewed event timestamps ordered by hashes, divergent heads, concurrent correction and tombstone/restore conflict reports, manual corrective-event review resolution, v0.2 legacy queue migration, and LAN revocation. Existing queue/trust/recovery/conflict-review tests, desktop restart/reconnect drain coverage, queued target-entity persistence/backfill tests, unsupported-schema tests, corrupt queue quarantine tests, interrupted atomic-write promotion tests, and iOS FFI queue/conflict-review/remote-event-apply plus Swift pull-transport/coordinator assertions remain in place. Real hosted web/desktop/iOS/self-hosted end-to-end device qualification, physical-device tests, and full migration matrix remain. |

## Next Recommended Goal

Finish the remaining sync/reconciliation hardening: production iOS reciprocal
pairing completion UX, stronger LAN key-exchange hardening, end-to-end
cross-client branch review and reconciliation workflow qualification, physical
LAN/iOS local-network validation, native push/pull endpoint qualification, and
release-device iOS background task and poor-network qualification. That goal unblocks
unattended desktop/iOS operation, cached map/offline work, contesting, EmComm,
and release qualification.
