# KE8YGW Logger for iOS

KE8YGW Logger is a local-first SwiftUI ham radio logging app for iPhone and
iPad. The native app owns UI, navigation, Apple APIs, Keychain, notifications,
MapKit, and Files/Share Sheet flows. Shared ham radio business logic belongs in
Rust through the `ham-ios-ffi` bridge and `ham-core`.

## App Identity

- App name: KE8YGW Logger
- Bundle identifier: `com.h2technologiesllc.ke8ygw-logger`
- Organization: H2 Technologies LLC
- Minimum target: iOS 17.0
- Language: Swift
- UI framework: SwiftUI
- Persistence: SwiftData local UI cache plus Rust official event bridge path

## Requirements

- macOS with Xcode 15 or newer
- iOS 17 simulator or device
- Rust stable with `rustup`

No GitHub Actions, Fastlane, TestFlight upload, iCloud, Push Notifications,
Associated Domains, App Groups, Sign in with Apple, or paid Apple capabilities
are configured yet. Local notifications use user authorization only.

## Rust Linkage

The iOS target links `ham-ios-ffi` through a generated static library under
`artifacts/ios/link/<Configuration><platform>/libham_ios_ffi.a`. The Xcode
target has a build phase that generates the Rust libraries and assembles
`artifacts/HamIOSFFI.xcframework` for packaging and verification.

Manual build from the repository root:

```bash
bash scripts/ios/install-targets.sh
CONFIGURATION=Release bash scripts/ios/build-xcframework.sh
bash scripts/ios/verify-linkage.sh
```

See `docs/IOS_BUILD_AND_LINKING.md` for troubleshooting, archive notes, and
architecture inspection commands.

## Open And Run

1. Open `KE8YGWLogger.xcodeproj` in Xcode.
2. Select the shared `KE8YGWLogger` scheme.
3. Choose an iPhone or iPad simulator.
4. Build and run.

Code signing is left at standard Xcode defaults. If you run on a physical
device, select your local development team manually in Xcode.

## Current Features

- Native iPhone/iPad split-view shell
- Dashboard with operator, station, GPS/grid, profile, recent QSO, provider,
  sync, offline, battery, and network status
- Expanded New QSO form for casual, portable, POTA, SOTA, satellite, contest,
  net, and emergency logging fields
- SwiftData-backed local QSO cache
- QSO detail screen with delete
- Station/equipment management cache
- Provider status and callsign lookup screens
- Keychain credential entry
- MapKit map screen
- POTA/SOTA activation screens
- Net Control and Emergency screens
- Hosted sync status screen
- Backup/Restore and Diagnostics screens
- Settings for appearance, operator/station defaults, providers, sync, privacy,
  diagnostics, about, and developer toggles
- ADIF export through Rust bridge when linked, with Swift fallback
- CSV export generation
- ShareLink-based export sharing
- Unit tests for ham-radio utilities, export formatting, and bridge fallback

## Rust Bridge Status

The Rust side now includes `crates/ham-ios-ffi`, which exports C ABI JSON
functions and a preferred byte-buffer command ABI for dashboard, station book,
providers, maps, sync, diagnostics, lookup, grid info, ADIF parsing/export,
QSO create/delete, station profile/equipment/select, activation start/end, and
Net Control session/check-in/traffic operations.

Rust official events and station-book storage are authoritative for those
operations. SwiftData rows are projection/cache records with canonical IDs,
Rust revision metadata, tombstone state, projection source, schema version, and
last refresh timestamp.

## Remaining Roadmap

- Build/link `ham-ios-ffi` for iOS.
- Run macOS/Xcode validation for simulator, device, archive, and unit tests.
- Route QSO correct/update, provider queue actions, emergency assignments,
  sync push/pull/merge/conflict resolution, and JSON/ZIP restore through Rust
  FFI.
- Add hosted sync transport on iOS.
- Add real provider adapters as they become available in Rust.
- Add UI, snapshot, offline, provider mock, sync, and map tests in Xcode.

## Notes

The iOS app should not duplicate Rust business logic. SwiftData records are a
local UI/offline projection cache. Secrets must stay in Keychain; do not store
provider tokens in UserDefaults, SwiftData, diagnostics, or logs.
