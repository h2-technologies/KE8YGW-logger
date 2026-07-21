# Project State

Last audited: 2026-07-21

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
  including operation/device/client/logbook IDs, deterministic per-logbook
  ordering, idempotency keys, dependency checks, retries/backoff, recovery of
  interrupted sends, redacted queue health, structured conflict reports,
  durable manual conflict-review records, explicit recovery-path decisions, and
  LAN trust records with short-lived single-use pairing tokens, replay nonce
  rejection, immediate revocation, and HMAC-SHA256 signed LAN read endpoint
  authorization for logbook/head/event APIs.
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
  for protected LAN read endpoints, and automatic IPv4/IPv6 multicast discovery
  that probes peer identity before recording reachable peers. Production
  reciprocal trust-pairing UX, LAN auth credential rotation/recovery,
  corrective-event conflict UX, physical-device LAN/iOS local-network
  validation, iOS background scheduler validation, and full cross-device
  reconciliation UI are incomplete.
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

- Production reciprocal LAN pairing UX beyond prompt-based token completion,
  LAN auth credential rotation/recovery, and physical-device validation beyond
  the current HMAC-SHA256 signed LAN read endpoints.
- Full cross-device reconciliation UI, corrective-event conflict-resolution UX,
  and release-device iOS background retry qualification.
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
| #27 Persistent desktop offline queue | Partially satisfied | GUI persists queue entries before QSO/activation/Net Control/station support mutations, recovers interrupted sends at startup, exposes queue state/recovery, and cloud push acknowledges queued official event hashes. Full reconnect automation and browser-level desktop recovery tests remain. |
| #28 Persistent iOS offline queue | Partially satisfied | `ham-ios-ffi` queues QSO/activation/Net Control/station/equipment commands and exposes queue snapshots/recovery plus Rust-owned conflict-review create/resolve commands. Release-device background retry, local-network permission behavior, and termination/poor-network validation remain. |
| #29 Push/pull/divergence/manual conflict review | Partially satisfied | Existing verified preview/pull/push remains, queue-aware cloud push was added, structured conflict reports are exposed, and durable manual conflict-review create/resolve commands reject unsafe divergent pulls while allowing explicit recovery-path decisions. Corrective-event creation UX and full cross-client conflict UI remain. |
| #30 Device pairing/trust/revocation/LAN transport decision | Partially satisfied | `JsonLanTrustStore` provides explicit approval, hashed expiring single-use tokens, logbook-scoped trusted devices, auth credential references, replay nonce rejection, and immediate revocation; GUI exposes trust endpoints, reciprocal prompt-based pairing completion, manual direct LAN HTTP peer add/preview/pull, automatic IPv4/IPv6 multicast discovery with reachable identity probing, advertised API-port normalization, HMAC-SHA256 signed LAN list/head/event read endpoints, and LAN pull rejects untrusted/revoked/replayed peers before local append. Production reciprocal pairing UX, LAN auth credential rotation/recovery, and physical LAN/iOS local-network validation remain. |
| #31 Cross-client sync recovery/migration test suite | Partially satisfied | New deterministic queue/trust/recovery/conflict-review tests and iOS FFI queue/conflict-review assertions exist. Full hosted web/desktop/iOS/self-hosted golden scenarios, physical-device tests, and migration matrix remain. |

## Next Recommended Goal

Finish the remaining sync/reconciliation hardening: production reciprocal
pairing UX, LAN auth credential rotation/recovery, corrective-event
conflict-resolution UX, physical LAN/iOS local-network validation, and
release-device iOS background retry qualification. That goal unblocks
unattended desktop/iOS operation, cached map/offline work, contesting, EmComm,
and release qualification.
