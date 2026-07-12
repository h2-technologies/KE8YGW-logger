# iOS Status

Last updated: 2026-07-10T21:15:00-04:00

## Implemented In This Pass

- Hardened `ham-ios-ffi` with a byte-buffer JSON command ABI, structured
  response envelope, ABI/schema versions, panic containment, null/UTF-8/size
  validation, correlation IDs, and explicit Rust string deallocation.
- Added Rust FFI commands for QSO create/delete, station profile/equipment
  create/select, activation start/end, Net Control session/check-in/traffic
  proposals, station-book snapshots, diagnostics, and bridge self-test.
- Added public FFI header/module map under `crates/ham-ios-ffi/include`.
- Added macOS scripts to install Apple Rust targets, build Rust static
  libraries, assemble `artifacts/HamIOSFFI.xcframework`, and verify exported
  symbols/architectures.
- Integrated the Rust static library into the Xcode project with a reproducible
  pre-link build phase and relative linker search paths; the generated
  `HamIOSFFI.xcframework` remains the packaging/verification artifact.
- Reworked Swift bridge calls through a centralized typed bridge client using
  the Rust byte-buffer ABI off the main actor.
- Routed iOS QSO create/delete, station profile/equipment create/select,
  POTA/SOTA activation start/end, and Net Control session/check-in/traffic
  actions through Rust bridge commands.
- Added SwiftData projection metadata and `ProjectionRefreshService`; QSO,
  station, and equipment rows are now cache/projection records for Rust
  accepted state.
- Added Diagnostics bridge self-test UI and expanded diagnostics export fields.
- Added `.github/workflows/ios.yml` for macOS Rust FFI/XCFramework/iOS
  simulator validation.
- Added `docs/IOS_BUILD_AND_LINKING.md`.
- `IOS_GAP_ANALYSIS.md` documents desktop/web/Rust versus iOS capability gaps from repository evidence.
- `crates/ham-ios-ffi` exposes Rust C ABI JSON functions for version, dashboard, station book, provider status, map, sync, diagnostics, callsign lookup, grid info, ADIF parse, and ADIF export.
- Swift `Shared/RustBridge` adds a bridge client, decoded snapshot models, live symbol loading, and fallback data for manual Xcode builds where the Rust library is not linked yet.
- The app root now uses a native SwiftUI `NavigationSplitView` app shell for iPhone/iPad feature navigation.
- Dashboard shows operator, active station, GPS/grid, profile, recent QSOs, pending uploads, provider summary, sync/offline state, battery, and network status.
- QSO logging now includes voice, CW, digital, satellite, contest, net, emergency, POTA, and SOTA fields.
- Station management supports multiple local station profiles and equipment cache rows.
- Added provider status, callsign lookup, Keychain credential entry, MapKit map, POTA, SOTA, Net Control, Emergency, Sync, Backup/Restore, Diagnostics, and richer Settings screens.
- ADIF export now prefers the Rust bridge and falls back to the legacy Swift formatter only when the bridge is unavailable.
- Added bridge fallback unit tests.

## Remaining Work

- Run `scripts/ios/build-xcframework.sh` on macOS and confirm Apple target
  compilation.
- Run Xcode simulator/device/archive validation.
- Add QSO correction/update through Rust proposals.
- Expose full hosted sync push/pull/merge/conflict resolution commands to iOS.
- Route JSON/ZIP backup inspect/dry-run/apply restore through Rust once the
  Rust restore API is available.
- Implement real hosted sync transport from iOS.
- Replace local provider action placeholders with live provider adapters once the Rust providers are implemented.
- Add Xcode UI, snapshot, offline, provider mock, sync, and map tests.

## Known Issues

- The Swift bridge currently falls back when Rust FFI symbols are not bundled. This is intentional for manual Xcode builds but is not final production behavior.
- POTA/SOTA spotting, emergency assignments, provider uploads, and some local
  settings remain UI-local until corresponding Rust commands/providers are
  available.
- Legacy SwiftData cache rows created before Rust authority may not have
  canonical IDs; delete hides them locally and should be replaced by a migration.
- Xcode project validation could not be run in this Windows workspace.
- Xcode archive logs from 2026-07-12 showed `rustup` missing from the archive
  shell `PATH`; the iOS build scripts now bootstrap Rust/Homebrew paths before
  invoking `rustup`, `cargo`, or `rustc`.
- No screenshots were captured because no iOS simulator is available here.

## Verification Performed

- `cargo fmt`
- `cargo check`
- `cargo test -p ham-ios-ffi`
- `cargo test` (177 Rust tests passed)
- Verified `xcodebuild` and `swift` are not installed in this environment.

Not run here: `scripts/ios/build-xcframework.sh`, `xcodebuild build`,
`xcodebuild test`, device archive, code signing, TestFlight upload.
