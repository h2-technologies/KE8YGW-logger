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
- [x] Hosted API status, auth, session, logbook, QSO, provider, sync, and device
  route boundary.
- [x] Account, session, device, logbook membership, logbook role, invite, and
  token models.
- [x] Proposal-backed QSO create/edit/delete/restore/note API routes.
- [x] Route tests for auth, logbook scoping, roles, logout, revoked devices, and
  QSO lifecycle.
- [x] Durable hosted server account/session/device/logbook metadata storage.
- [x] Durable self-hosted sync/report metadata and payload storage.
- [ ] Production OS credential backend wiring.
- [ ] Live Tier 1 provider adapters.
- [ ] Upload queue execution against live/fake providers.
- [ ] Confirmation download/reconciliation UI.
- [ ] Tauri desktop packaging.
- [ ] Native desktop file dialogs.
- [ ] Backup/restore.
- [ ] Conflict/divergence review UX.
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

- SQLite schema evolution is intentionally minimal and needs production
  migration policy hardening before v1.0.
- Session expiry/refresh policy is still beta-level.
- Native credential backends are still placeholders.
- Live provider adapters are still mostly metadata/stub-backed.
- GUI browser tests are not yet present.
- Desktop packaging has not been added yet.
- Permission scopes are enforced in the new hosted QSO slice but not yet
  consistently across every older GUI/local route.

## v1.0 Delta After v0.2

- Finish production credential backends.
- Complete provider adapters and provider error handling.
- Finish desktop packaging/signing/notarization decisions.
- Add browser-level GUI tests and release artifact checks.
- Tighten documentation and operator-facing setup guides.
- Fix beta bugs found during real hosted/self-hosted testing.
