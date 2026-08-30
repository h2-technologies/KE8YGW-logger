# iOS Status

Last audited: 2026-07-21

Native iOS is part of the locked v1 release on November 24, 2026. It ships with
hosted web and Windows/macOS/Linux desktop; it is not a v1.1 deliverable.

## Implemented

- Native SwiftUI app under `ios/KE8YGWLogger`.
- SwiftData cache/projection models for QSO, station profile, station equipment,
  and application settings.
- Rust FFI bridge crate under `crates/ham-ios-ffi` with public header/module
  map, byte-buffer JSON command ABI, explicit response deallocation, schema/ABI
  version payloads, and panic containment.
- Apple build/link scripts under `scripts/ios`.
- Xcode project with shared scheme and relative Rust library linkage.
- Rust-backed bridge paths for version/self-test, settings, QSO create/delete,
  station profile/equipment/select, POTA/SOTA activation start/end, Net Control
  session/check-in/traffic, diagnostics, and snapshot/fallback flows.
- Hosted account and session workspace: `account.snapshot`,
  `account.set_server`, `account.plan`, `account.record_result`, and
  `account.record_credentials` bridge commands; a `URLSession` transport that
  classifies nothing; Keychain-stored session and refresh tokens referenced by
  credential ID in Rust support state; and register, verify-email, sign-in,
  session refresh/rotate, sign-out, sign-out-everywhere, recovery, device
  list/revoke/revoke-all, and confirmed account deletion flows.
- Recoverable SwiftData projection cache: a store that cannot be opened is
  quarantined and rebuilt from the Rust event store, with an in-memory fallback
  and a recovery screen instead of a launch crash.
- Unit tests for ham-radio utilities, export helpers, bridge fallback decoding,
  projection cache recovery, and hosted account action encoding, snapshot/plan
  wire-key decoding, Keychain token storage, bearer injection, sign-out token
  clearing, and transport-failure handling.
- `.github/workflows/ios.yml` simulator workflow on macOS.

## Partial

- iOS ADIF export prefers Rust and can fall back to Swift; broader native import,
  backup restore, and diagnostic export flows still need v1 hardening.
- Sync UI consumes Rust snapshots, but full push/pull/reconciliation/conflict
  commands are not exposed end-to-end.
- MapKit surfaces exist, but cached/offline regions and production map providers
  remain incomplete.
- Keychain plumbing exists, but production provider setup and privacy review are
  incomplete.
- The hosted account workspace is implemented against the hosted account APIs
  but has not been qualified on a release device or against a production hosted
  deployment, and administrator hosting/invitation management has no iOS
  surface.

## Test-Only Or Fallback

- Swift bridge fallback data allows development/tests without a linked Rust
  library.
- Provider, map, sync, and diagnostics views still display snapshot/mock state
  where production commands are not implemented.

## Remaining v1 Work

- Run and keep passing iOS simulator, archive, device, and TestFlight
  validation.
- Configure Apple signing, provisioning, App Store Connect metadata, privacy
  manifest, support URL, privacy policy URL, and account deletion path.
- Finish offline queue/reconciliation, sync push/pull/conflict handling,
  cached/offline maps, provider setup, contesting, and EmComm.
- Add Xcode UI/snapshot/offline/provider/sync/map tests.
