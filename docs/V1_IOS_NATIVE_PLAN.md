# v1 Native iOS Plan

Last audited: 2026-07-22

Native iOS is part of the locked v1 release on November 24, 2026. It must be a
real SwiftUI iPhone/iPad app, not a PWA, pinned hosted website, or thin web
wrapper.

## Target

- Native SwiftUI frontend with iPhone and iPad layouts.
- Rust-backed domain operations through `crates/ham-ios-ffi` where repository
  code already exposes stable bridge commands.
- SwiftData cache/projection records for native UI state, not authoritative
  official event storage.
- Same hosted/self-hosted `/api/v1` contracts as hosted web and desktop.
- Offline operation and later reconciliation.
- Keychain credential storage.
- Native document picker/share sheet for ADIF import/export.
- Native maps and cached/offline regions.
- TestFlight and App Store distribution.

## Implemented In This Repository

- Xcode project, shared scheme, Info.plist, app icons, SwiftUI app shell, and
  feature workspaces under `ios/KE8YGWLogger`.
- Rust FFI crate, public header/module map, and Apple build/link scripts under
  `crates/ham-ios-ffi` and `scripts/ios`.
- Byte-buffer JSON command ABI, version/self-test payloads, panic containment,
  explicit Rust-owned response deallocation, and Swift bridge wrappers.
- Rust-backed QSO create/delete, station profile/equipment/select,
  POTA/SOTA activation start/end, Net Control session/check-in/traffic, settings
  load/create/update, diagnostics, and bridge fallback paths.
- SwiftData QSO/station/equipment/settings records with projection/cache
  metadata.
- Unit tests for ham-radio utilities, export, and bridge fallback decoding.
- GitHub Actions iOS simulator workflow on macOS.

## Partial

- iOS ADIF export prefers the Rust bridge and falls back to Swift export when
  the bridge is unavailable; import/restore still need broader Rust-backed
  native flows.
- Sync views consume Rust snapshots; Rust-owned offline queue records include
  optional target entity metadata, and durable conflict-review create/resolve
  commands are exposed through the bridge. The native Swift bridge also exposes
  typed queue snapshots, recovery reports, retry plans, retry results, affected
  mutations, queue health, Rust-planned official event envelopes,
  self-hosted/logbook-scoped push execution coordination, hosted
  `/api/v1/sync/push` request construction, accepted-prefix/rejected-tail retry
  result recording, saved conflict-review records, selected recovery paths, and
  structured conflict messages so the Sync workspace can ask Rust for
  no-network/user-action retry decisions using native network state and surface
  open review actions. Full pull/reconciliation workflows and real
  hosted/self-hosted endpoint qualification are not exposed end-to-end to iOS.
- MapKit surfaces exist, but cached/offline map regions and production map
  provider integration are not complete.
- Keychain plumbing exists, but production provider credential setup and App
  Store privacy validation are incomplete.
- Apple simulator CI exists; device, archive, TestFlight, App Store review, and
  signing/notarization distribution remain incomplete.

## Test-Only Or Fallback Paths

- Bridge fallback data allows Windows/Linux development and Swift unit tests
  without a linked Rust library.
- Provider, lookup, sync, map, and diagnostics surfaces still rely on mock,
  placeholder, or snapshot state where production provider commands are not
  implemented.

## Required Remaining Work

- Complete offline queue and reconciliation behavior through shared Rust/API
  validation.
- Finish native sync pull execution, full divergence review decisions,
  corrective-event conflict handling, release-device BGTask execution,
  hosted/self-hosted endpoint qualification for the native push path, and
  physical poor-network validation through the Rust bridge/API contract.
- Finish native ADIF import/restore, backup inspect/dry-run/apply, and
  diagnostic export flows without storing secrets.
- Add cached/offline map region selection and validation against an approved
  cacheable map source.
- Add native contesting and EmComm product surfaces required by issue #2.
- Add Xcode UI/snapshot/offline/provider/sync/map tests.
- Configure signing team, provisioning profiles, privacy manifest, TestFlight,
  App Store metadata, support URL, privacy policy URL, and account deletion
  workflow.

## Acceptance

- Xcode project builds and tests on a current macOS runner.
- App launches on current iPhone and iPad simulators and at least one physical
  device.
- User can authenticate, select/create a logbook where permitted, and perform
  v1 QSO/POTA/SOTA/Net Control workflows through Rust/API-authoritative paths.
- Desktop/iOS offline/reconciliation scenarios pass.
- ADIF import/export uses native document flows and remains secret-free.
- Provider credentials are stored in Keychain or approved Rust credential paths.
- Cached/offline maps work within approved provider terms.
- TestFlight build can be produced and App Store review blockers are resolved.
