# v0.2 Release Plan

v0.2 is the "almost v1" foundation baseline. The current v1 target is the
November 24, 2026 release with hosted web, native iOS, and Windows/macOS/Linux
desktop.

This document is retained to describe the already-built `0.2.0` foundation. It
is not the v1 product checklist and must not override issue #2 or
`docs/V1_RELEASE_PLAN.md`.

## Feature Checklist

- [x] Release-scope docs distinguish the `0.2.0` foundation from the locked v1
  release.
- [x] Dedicated `ham-server` hosted API crate scaffold.
- [x] Hosted API status, auth, session, logbook, QSO, station/equipment, ADIF,
  provider, upload, sync, and device route boundary.
- [x] Account, session, device, logbook membership, logbook role, invite, and
  token models.
- [x] Proposal-backed QSO create/edit/delete/restore/note API routes.
- [x] Route tests for auth, logbook scoping, roles, logout, revoked devices, and
  QSO lifecycle.
- [x] Durable hosted server account/session/device/logbook metadata storage.
- [x] Durable self-hosted sync/report metadata and payload storage.
- [x] Hosted station/equipment support metadata routes.
- [x] Hosted ADIF import/export routes using official projections/proposals.
- [x] Hosted provider settings/test routes with credential-reference-only
  storage.
- [x] Hosted upload queue execution through Tier 1 provider adapters with
  deterministic fake/mock mode.
- [x] Hosted sync pull route with scoped missing-event responses.
- [x] Hosted activation routes using proposal-backed official events.
- [x] Hosted Net Control routes using proposal-backed official events.
- [x] Hosted map summary and settings routes.
- [x] Hosted backup export, restore dry-run, and safe full import foundation.
- [x] Hosted divergence review API with no automatic merge.
- [x] User-facing backup/restore and divergence review GUI surfaces.
- [x] Desktop packaging foundation with `ham-desktop` and `src-tauri` config.
- [x] Native desktop file dialog bridge contract for import/export flows.
- [x] Desktop/native dialog command helper implementation.
- [x] Actual Tauri runtime wrapper under `src-tauri`.
- [x] Production OS credential backend wiring for Windows Credential Manager,
  macOS Keychain, and Linux Secret Service/libsecret tooling.
- [x] Provider credential validation response hooks and upload missing-credential
  safety checks.
- [x] Tier 1 provider adapter boundaries for QRZ XML, HamQTH, POTA spots,
  SOTAWatch, Club Log, QRZ Logbook, eQSL, LoTW, and DX Cluster.
- [x] Upload queue execution against the provider adapter framework.
- [x] Gated live HTTP upload transports for Club Log, QRZ Logbook, and eQSL.
- [x] QRZ XML/HamQTH response parsers, POTA request/fixture parser, and
  DX Cluster read-once Telnet foundation.
- [x] Hosted lookup route execution for QRZ XML and HamQTH.
- [x] Hosted POTA spot fetch route execution.
- [x] DX Cluster bounded connect/read/disconnect/status runtime controls.
- [x] Ignored live validation hooks for QRZ XML, HamQTH, POTA, DX Cluster, Club
  Log, QRZ Logbook, and eQSL with explicit env gating and redacted output.
- [x] Stable redacted provider runtime error-code mapping for lookup/spot/DX
  validation paths.
- [x] Native iOS SwiftUI/Rust-bridge foundation, Xcode project, Apple build
  scripts, shared scheme, unit tests, and simulator CI.
- [ ] Approved SOTAWatch live endpoint and terms handling.
- [ ] LoTW TQSL/certificate-signing upload flow.
- [ ] Confirmation download/reconciliation UI.
- [x] Tauri desktop runtime and packaging foundation.
- [x] Native desktop file dialog bridge foundation.
- [x] Full backup restore/import foundation for safe same-logbook append/replay.
- [x] Conflict/divergence review API foundation.
- [ ] LAN peer-to-peer transport and trust pairing.
- [ ] Full permission scope enforcement across all workflows.
- [ ] Station/equipment GUI completion.
- [ ] Interactive map renderer.
- [ ] Browser-level GUI tests.
- [x] Cross-platform CI/release baseline with API, governance, version,
  documentation-link, security, container, Tauri, and iOS checks.

## Acceptance Criteria

- Hosted API has a stable versioned `/api/v1` boundary.
- QSO mutations through hosted API use the existing proposal pipeline.
- Cross-logbook access is rejected.
- Viewer role cannot mutate logs.
- Operator role can log QSOs.
- Logout invalidates sessions.
- Revoked device sessions cannot sync.
- Hosted account/session/device/logbook metadata survives server restart.
- Hosted station/equipment/provider/upload support metadata survives metadata
  store reload.
- ADIF import appends official QSO events through the proposal pipeline and
  export reads official projections.
- Sync pull returns only allowed missing events and revoked devices cannot pull.
- Activation and Net Control hosted writes append official events through core
  proposal validation.
- Map hosted reads are derived from official projections and support metadata.
- Backup export includes official events and support metadata without secrets.
- Backup import dry-run validates manifests and event-chain integrity.
- Backup import appends only verified missing official events, skips exact
  duplicates, restores scoped support metadata, strips provider credential
  references, and blocks divergent targets.
- Divergence review reports safe pull/push/diverged states without automatic
  merge.
- GUI exposes backup export, restore dry-run/import review, import result, sync
  divergence detail, and divergence report export surfaces.
- Desktop packaging foundation exists, release mode bundles the shared web UI,
  and no frontend dev server is required.
- Desktop native dialog command helpers cover ADIF, backup, diagnostics,
  divergence reports, and app data directory selection, with browser fallback
  intact.
- Credential storage uses OS backends when available, keeps plaintext fallback
  explicit/dev-only, and redaction tests cover provider settings, backups,
  diagnostics, upload history, divergence/report surfaces, and API responses.
- Sync events, heads, device revocation, and diagnostic reports survive sync
  server restart.
- Existing app architecture remains intact.
- Workspace compiles and tests pass.
- Remaining gaps are documented in `PROJECT_STATE.md`.

## Intentionally Deferred

- PWA as an iOS release target.
- Tauri mobile.
- Plugin marketplace, signed plugin distribution, and plugin sandboxing.
- AI assistant.
- Full contesting engine.
- Full EmComm package beyond Net Control.
- Full award database completeness.
- APRS, satellite, radar, terrain, and offline tile packs.
- Automatic conflict merge.
- Signing/notarization if it blocks functional release.
- Android.

## Known Risks

- SurrealDB schema evolution is intentionally minimal and needs production
  migration policy hardening before v1.
- Session expiry/refresh policy is still beta-level.
- OS credential backends are implemented through platform APIs/tools, but
  environment-specific validation is still needed on clean Windows, macOS, and
  Linux packaging runners.
- Tier 1 provider adapters are wired into hosted tests/uploads. Club Log, QRZ
  Logbook, and eQSL have gated live upload transports; LoTW is intentionally
  fake/scaffold only until TQSL signing is modeled.
- QRZ XML/HamQTH lookup, POTA spot fetching, and DX Cluster read-once runtime
  are wired in hosted routes with ignored live validation hooks. Real-account
  provider validation still requires explicit credentials or provider-approved
  fixtures. SOTAWatch live access remains deferred pending explicit API
  approval/terms.
- Backup import is intentionally conservative: it supports same-logbook clean
  append/idempotent replay and blocks divergent targets rather than merging.
- GUI browser tests are not yet present.
- Tauri runtime validation is wired into CI; full signed package validation,
  notarization, update-channel validation, and clean release-runner installer
  qualification remain v1 work.
- Permission scopes are enforced in the implemented hosted slices but not yet
  consistently across every older GUI/local route.
- Native iOS exists, but signing, TestFlight/App Store distribution,
  offline/reconciliation parity, cached maps, and full production validation
  remain v1 work.

## v1 Delta After v0.2

- Complete hosted web account/registration modes, verified email, Turnstile,
  and deployment hardening.
- Complete native iOS offline/reconciliation, signing, TestFlight/App Store,
  maps, provider, contesting, and EmComm release requirements.
- Validate production credential backends on release runners.
- Validate hosted lookup/spot/DX runtime on provider-approved accounts or
  fixtures, complete LoTW TQSL signing, and run live-provider release-runner
  validation.
- Harden backup import UX and add browser-level coverage.
- Finish Tauri package validation, signing, notarization, and signed updater
  decisions.
- Add browser-level GUI tests and release artifact checks.
- Tighten documentation and operator-facing setup guides.
- Fix beta bugs found during real hosted/self-hosted testing.
