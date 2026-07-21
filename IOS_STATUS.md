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
- Unit tests for ham-radio utilities, export helpers, and bridge fallback
  decoding.
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
