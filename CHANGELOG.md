# Changelog

## Unreleased

### Added

- Added `IOS_GAP_ANALYSIS.md`.
- Added `ham-ios-ffi`, a Rust FFI crate for iOS JSON bridge calls backed by `ham-core` and `ham-sync`.
- Added a hardened byte-buffer Rust FFI command ABI with structured envelopes, ABI/schema versions, correlation IDs, panic containment, bounded input checks, and explicit deallocation.
- Added public iOS FFI header/module map and macOS scripts for Apple Rust targets, static libraries, XCFramework assembly, and linkage verification.
- Added Xcode `HamIOSFFI.xcframework` reference and pre-link build phase for reproducible Rust linkage.
- Added iOS bridge client, bridge fallback contract, and bridge fallback tests.
- Added Swift typed bridge DTOs for QSO, station, activation, Net Control, diagnostics, and bridge self-test operations.
- Added SwiftData projection metadata and `ProjectionRefreshService`.
- Added macOS GitHub Actions workflow for Rust FFI, XCFramework, and iOS simulator validation.
- Added `docs/IOS_BUILD_AND_LINKING.md`.
- Added native iOS split-view shell and feature workspaces for Dashboard, Logging, Callsign Lookup, Stations, Providers, Maps, POTA, SOTA, Net Control, Emergency, Sync, Backup/Restore, Diagnostics, and Settings.
- Added Keychain credential storage and local notification authorization plumbing.
- Added SwiftData station equipment cache model.

### Changed

- Expanded iOS QSO, station profile, settings, export, logbook, and detail models/views for MVP parity fields.
- Routed iOS QSO create/delete, station profile/equipment/select, POTA/SOTA activation start/end, and Net Control session/check-in/traffic mutations through Rust bridge commands.
- Reclassified SwiftData QSO/station/equipment state as cache/projection data for Rust-accepted state.
- iOS ADIF export now prefers the Rust bridge and falls back to Swift export if the bridge is unavailable.
- Updated `PROJECT_STATE.md`, `ROADMAP.md`, and iOS documentation for the parity pass.

### Testing

- Ran `cargo fmt`.
- Ran `cargo check`.
- Ran `cargo test -p ham-ios-ffi`.
- Ran full `cargo test` with 177 Rust tests passing.
- Xcode/iOS simulator tests were not run because this workspace does not provide macOS/Xcode tooling.
