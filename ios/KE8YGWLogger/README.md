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

The Rust build scripts automatically load `~/.cargo/env` and common
Rust/Homebrew binary locations for Xcode archive builds. Do not add local
`/Users/.../.cargo/bin` paths to the Xcode project.

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
- Editable Settings for appearance, operator identity, station/equipment
  defaults, manual/GPS Maidenhead behavior, provider credentials, provider
  behavior, sync/server configuration, logging defaults, activation
  preferences, Net Control defaults, privacy, diagnostics, about, and developer
  toggles. The Settings screen loads, creates, and saves the canonical
  application settings record through the Rust bridge; SwiftData stores only a
  UI cache of the latest bridge result.
- Core Location "When In Use" permission request for GPS-derived Maidenhead
  grid calculation, plus a Use Device Location opt-out and manual grid override
- Provider credential forms under Settings for QRZ XML, QRZ Logbook, HamQTH,
  POTA, SOTAWatch, Club Log, eQSL, LoTW, HRDLog, and DX Cluster. Secrets are
  stored in iOS Keychain; Rust-backed Settings stores only enablement,
  non-secret metadata, and validation status.
- Sync credentials are managed from Settings and stored in iOS Keychain.
  Rust-backed Settings stores the sync server URL, device name, and non-secret
  account label, but not the sync token.
- Provider enable/disable controls in the Providers view. Disabling a provider
  preserves credentials and pauses new automatic use by iOS workflows.
- POTA/SOTA activation start gating based on provider enablement, credential
  validation metadata, and `NWPathMonitor` connectivity. When the device has no
  usable network path, the app allows an explicitly labeled local-only
  activation and records that provider validation was skipped.
- Dedicated station profile and equipment creation sheets that submit through
  the Rust station-book bridge and refresh SwiftData projections.
- Net Control roster classifications: Emergency, Priority, Routine, Health and
  Welfare, and No Traffic. Roster and traffic lists sort by classification with
  stable timestamp/callsign fallbacks, and traffic rows render immediately after
  creation.
- Durable SwiftData-backed UI drafts for QSO entry, POTA/SOTA activation state,
  and Net Control session/check-in/traffic fields. Drafts are cleared after
  successful canonical submission or explicit workflow end.
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

Rust official events, station-book storage, and application settings support
storage are authoritative for those operations. SwiftData rows are
projection/cache records with canonical IDs, Rust revision metadata, tombstone
state, projection source, schema version, and last refresh timestamp. The
SwiftData `AppSettings` row is likewise a cache of the Rust
`application-settings.json` support record.

Draft state remains local iOS support state. It is not a substitute for Rust
official events or Rust-backed application settings. Net Control accepted
check-ins and traffic still go through Rust proposals, but the current iOS ABI
does not expose check-in update or Net Control projection-list commands.
Classification edits after check-in are therefore persisted as iOS
draft/support state until a Rust update command is available.

Provider credential validation currently verifies that the required iOS
Keychain-backed fields are configured and records a timestamp used by iOS
activation gating. Live provider authentication remains pending because the
current Rust provider adapters are still stubs and the iOS ABI does not expose a
provider validation command. Online POTA/SOTA activation is blocked unless the
provider is enabled and the local validation record is fresh; genuine no-network
state is detected with `NWPathMonitor` and allows local-only activation.

Location precedence on iOS is:

1. Manual grid override.
2. Current GPS-derived grid when Use Device Location is enabled and permission
   allows it.
3. Last GPS-derived grid cached in Settings.
4. Active/default station grid.
5. Unknown.

Manual grid entry supports 4- and 6-character Maidenhead locators and preserves
the user's manual value when GPS updates arrive.

## Remaining Roadmap

- Run macOS/Xcode validation for simulator, physical device, archive, and unit
  tests.
- Qualify release-device BGTask execution, hosted/self-hosted native sync
  endpoints selected through the Sync API setting, poor-network behavior, and
  Local Network permission prompts.
- Route QSO correct/update, provider queue actions, live provider credential
  validation, Net Control check-in classification updates, Net Control snapshot
  recovery, emergency assignments, remaining sync merge/conflict workflows, and
  JSON/ZIP restore through Rust FFI.
- Add real provider adapters as they become available in Rust.
- Add UI, snapshot, offline, provider mock, sync, and map tests in Xcode.

## Notes

The iOS app should not duplicate Rust business logic. SwiftData records are a
local UI/offline projection cache. Secrets must stay in Keychain; do not store
provider tokens in UserDefaults, SwiftData, diagnostics, or logs.
