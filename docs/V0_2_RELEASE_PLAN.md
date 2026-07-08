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
- [x] Hosted backup export and restore dry-run foundation.
- [x] Hosted divergence review API with no automatic merge.
- [ ] Production OS credential backend wiring.
- [ ] Live Tier 1 provider adapters.
- [ ] Upload queue execution against live providers.
- [ ] Confirmation download/reconciliation UI.
- [ ] Tauri desktop packaging.
- [ ] Native desktop file dialogs.
- [ ] Full backup restore/import.
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
- Divergence review reports safe pull/push/diverged states without automatic
  merge.
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
- Native credential backends are still placeholders.
- Live provider adapters are still mostly metadata/stub-backed; hosted provider
  tests and uploads use fake/stub behavior for deterministic CI.
- Backup import is dry-run only; full restore remains a v0.2 gap.
- GUI browser tests are not yet present.
- Desktop packaging has not been added yet.
- Permission scopes are enforced in the implemented hosted slices but not yet
  consistently across every older GUI/local route.

## v1.0 Delta After v0.2

- Finish production credential backends.
- Complete live provider adapters and provider error handling.
- Complete full backup restore/import and user-facing divergence review UX.
- Finish desktop packaging/signing/notarization decisions.
- Add browser-level GUI tests and release artifact checks.
- Tighten documentation and operator-facing setup guides.
- Fix beta bugs found during real hosted/self-hosted testing.
