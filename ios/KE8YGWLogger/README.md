# KE8YGW Logger for iOS

KE8YGW Logger is a local-first SwiftUI ham radio logging app skeleton for iPhone
and iPad. It is intentionally offline-first and Apple-native for the first iOS
milestone.

## App Identity

- App name: KE8YGW Logger
- Bundle identifier: `com.h2technologiesllc.ke8ygw-logger`
- Organization: H2 Technologies LLC
- Minimum target: iOS 17.0
- Language: Swift
- UI framework: SwiftUI
- Persistence: SwiftData

## Requirements

- macOS with Xcode 15 or newer
- iOS 17 simulator or device

No GitHub Actions, Fastlane, TestFlight upload, iCloud, Push Notifications,
Associated Domains, App Groups, Sign in with Apple, or paid Apple capabilities
are configured yet.

## Open And Run

1. Open `KE8YGWLogger.xcodeproj` in Xcode.
2. Select the shared `KE8YGWLogger` scheme.
3. Choose an iPhone or iPad simulator.
4. Build and run.

Code signing is left at standard Xcode defaults. If you run on a physical
device, select your local development team manually in Xcode.

## Current Features

- Home screen with quick actions
- New QSO form
- SwiftData-backed local logbook
- QSO detail screen with delete
- Station profile defaults
- Settings for default band/mode and callsign behavior
- ADIF export generation
- CSV export generation
- ShareLink-based export sharing
- Unit tests for ham-radio utilities and export formatting

## Future Roadmap

- Maidenhead grid calculation from GPS
- QRZ lookup
- LoTW, eQSL, Club Log, and QRZ Logbook integrations
- POTA/SOTA activation workflow
- Field Day mode
- Net control mode
- Local network event sync
- iCloud sync
- Maps and propagation
- TestFlight, Fastlane, and GitHub Actions setup

## Notes

This project is separate from the Rust desktop/server workspace. The architecture
is intentionally compatible with the larger local-first platform: the iOS app
keeps local data first, avoids paid Apple capabilities for now, and keeps export
logic in pure Swift services for testing and future sync integration.
