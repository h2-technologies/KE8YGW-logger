# v0.2 Release Plan

v0.2 is the "almost v1.0" beta. The goal is to make KE8YGW Logger functionally
close to the v1.0 web + desktop release while leaving v1.0 for polish,
packaging/signing, documentation cleanup, provider refinement, and beta bug
fixes.

v0.2 must not start native iOS work and must not use PWA/mobile web as an iOS
substitute. The hosted API should be clean enough for a future native SwiftUI
iOS client in v1.1.

## Feature Checklist

- [x] Release-scope docs split v0.2, v1.0, and v1.1.
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
- [x] Hosted upload queue execution foundation using fake/stub provider mode.
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
- [x] Production OS credential backend wiring for Windows Credential Manager,
  macOS Keychain, and Linux Secret Service/libsecret tooling.
- [x] Provider credential validation response hooks and upload missing-credential
  safety checks.
- [ ] Live Tier 1 provider adapters.
- [ ] Upload queue execution against live providers.
- [ ] Confirmation download/reconciliation UI.
- [x] Tauri desktop packaging foundation.
- [x] Native desktop file dialog bridge foundation.
- [x] Full backup restore/import foundation for safe same-logbook append/replay.
- [x] Conflict/divergence review API foundation.
- [ ] LAN peer-to-peer transport and trust pairing.
- [ ] Full permission scope enforcement across all workflows.
- [ ] Station/equipment GUI completion.
- [ ] Interactive map renderer.
- [ ] Browser-level GUI tests.
- [ ] Hardened CI/release artifacts.

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
- Desktop packaging foundation exists and release mode is documented as
  no-dev-server.
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

- Native iOS app.
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
  migration policy hardening before v1.0.
- Session expiry/refresh policy is still beta-level.
- OS credential backends are implemented through platform APIs/tools, but
  environment-specific validation is still needed on clean Windows, macOS, and
  Linux packaging runners.
- Live provider adapters are still mostly metadata/stub-backed; hosted provider
  tests and uploads use fake/stub behavior for deterministic CI.
- Backup import is intentionally conservative: it supports same-logbook clean
  append/idempotent replay and blocks divergent targets rather than merging.
- GUI browser tests are not yet present.
- Full Tauri runtime packaging is not wired into CI yet; the foundation config,
  desktop crate, and command helpers are present.
- Permission scopes are enforced in the implemented hosted slices but not yet
  consistently across every older GUI/local route.

## v1.0 Delta After v0.2

- Validate production credential backends on release runners.
- Complete live provider adapters and provider error handling.
- Harden backup import UX and add browser-level coverage.
- Add the actual Tauri runtime wrapper, then finish packaging/signing/
  notarization decisions.
- Add browser-level GUI tests and release artifact checks.
- Tighten documentation and operator-facing setup guides.
- Fix beta bugs found during real hosted/self-hosted testing.
