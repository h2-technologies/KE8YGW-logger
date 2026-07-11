# iOS Gap Analysis

Last audited: 2026-07-10

Scope: repository state in this checkout. This analysis is based on the Rust workspace, GUI HTTP surface, documentation, and the native iOS project files. It does not infer functionality from external product plans.

## Evidence Reviewed

- Root workspace members in `Cargo.toml`.
- Current implementation status in `PROJECT_STATE.md` and `ROADMAP.md`.
- Shared Rust exports in `crates/ham-core/src/lib.rs`.
- GUI shell workspaces and panels in `crates/ham-gui/src/shell.rs`.
- GUI commands in `crates/ham-gui/src/commands.rs`.
- GUI API routes in `crates/ham-gui/src/main.rs`.
- Native iOS project in `ios/KE8YGWLogger`.
- iOS README and tests in `ios/KE8YGWLogger/README.md` and `ios/KE8YGWLogger/KE8YGWLoggerTests`.

## Summary

The desktop/hosted web side is Rust-backed and feature-rich at the foundation/UI-shell level. `ham-core` owns append-only official events, proposals, projections, ADIF, lookup helpers, service/provider registry, credential abstractions, station/equipment models, upload queue logic, online service dashboard models, maps/GIS helpers, Net Control projections, diagnostics, and support storage. `ham-gui` exposes these through a static web shell and JSON endpoints.

The iOS app is currently a SwiftUI/SwiftData local logger skeleton. It has screens for Home, New QSO, Logbook, QSO Detail, Station Profile, Export, and Settings. It exports ADIF/CSV in Swift and persists QSO/profile/settings entities in SwiftData. The iOS README explicitly says the project is separate from the Rust desktop/server workspace, and `PROJECT_STATE.md` marks the Swift bridge to Rust/core event model and iOS sync parity as incomplete.

## Feature Matrix

| Feature | Desktop | Web | iOS | Status |
|---------|---------|-----|-----|--------|
| Workspace shell / navigation | `ham-gui` shell defines Dashboard, Casual Logger, POTA/SOTA, Maps, Awards, Online Services, Net Control, EmComm, Contesting | Static web shell in `crates/ham-gui/web` renders workspaces/panels | `NavigationStack` home with links only | Missing iOS first-class workspace navigation |
| Casual logging | Rust proposal path and QSO projection in `ham-core`; `/api/qso/create`, `/api/qsos` | Callsign Entry and Recent QSOs panels | `NewQSOView`, `LogbookView`, SwiftData QSO | Partial; iOS does not use Rust proposal/event model |
| Portable/mobile logging | POTA/SOTA portable QSO route `/api/qso/portable-create` and activation link path | POTA/SOTA workspace portable logger panel | Not implemented | Missing |
| POTA | Activation proposals/projections and `/api/activation/*` endpoints | POTA/SOTA activation setup/progress/spots panels | Listed as future in iOS README | Missing |
| SOTA | Shared activation workflow and SOTA spots in online service dashboard | SOTA spot panel via POTA/SOTA and Online Services | Listed as future in iOS README | Missing |
| Emergency communications | Workspace exists as EmComm with map/sync/diagnostics placeholder panels | EmComm placeholder workspace | Not implemented | Missing; desktop/web is placeholder |
| Net Control | `ham-core::net`; `/api/net-control`, session/check-in/traffic/report endpoints | Net Control workspace panels | Listed as future in iOS README | Missing |
| Hosted synchronization | `ham-sync` cloud API models and `ham-sync-server`; `/api/sync/cloud/*` endpoints | Sync Status panel and cloud controls | Listed as future local network/iCloud sync | Missing |
| Offline-first operation | Append-only JSONL event store, support storage, in-memory/cloud safe pull models | GUI uses local event/support stores | SwiftData local persistence | Partial; iOS offline storage is not Rust event storage |
| Provider integrations | `ServiceRegistry`, QRZ/HamQTH stubs, upload provider stubs, online metadata | Service Providers and Online Services panels | Listed as future integrations | Missing |
| Mapping | `ham-core::map`; `/api/maps/state`, map layer toggle endpoint | Maps workspace with map/layer/propagation/weather panels | Listed as future | Missing |
| Callsign lookup | `ham-core::lookup`; `/api/lookup/callsign`, cache/status endpoints | Lookup command and form enrichment | Swift callsign validation only | Missing provider-backed lookup |
| Station management | `ham-core::station` profiles/equipment/configurations; `/api/station` | Station Summary, Profiles, Equipment panels | Single `StationProfile` defaults screen | Partial; no multiple profiles/equipment/configurations |
| Equipment management | Rust `EquipmentItem`, `StationConfiguration` | Equipment Manager panel | Not implemented | Missing |
| Backup / Restore | ADIF import/export endpoints; diagnostics ZIP export | Import/export actions | ADIF/CSV export via ShareLink | Partial; no import/restore/ZIP |
| Diagnostics | `ham-core::diagnostics`; `/api/diagnostics/report-*`, runtime logs | Diagnostic Reports and Event Bus Monitor panels | Not implemented | Missing |
| Credential management | `CredentialStore` abstraction and `/api/credentials/*`; docs require no secret logging | Credential Manager panel | Not implemented | Missing Keychain integration |
| Provider health/status | `online_services_dashboard`, provider health and missing credential status | Online Providers, Provider Health panels | Not implemented | Missing |
| Upload queue | `ham-core::upload`, `/api/uploads`, `/api/uploads/queue` | Uploads and Online Upload Queue panels | Not implemented | Missing |
| Awards/search | Award engine and projection search; `/api/awards`, `/api/search` | Awards and Advanced Search panels | Basic callsign search only | Missing |
| Rig control | `ham-core::rig`; `/api/rig/*` | Rig Control panel | Not implemented | Missing |
| ADIF import | `ham-core::import_adif`; `/api/adif/import` | Import ADIF action | Not implemented | Missing |
| ADIF export | `ham-core::export_adif`; `/api/adif/export` | Export ADIF action | Swift `LogExportService.adif` | Partial; iOS duplicates export logic in Swift |
| CSV export | iOS-only service found | No Rust/web CSV evidence found | Swift CSV export | Present on iOS |
| Settings | GUI settings and provider/service settings | Settings screen plus provider screens | Basic logging defaults only | Partial |
| Notifications | `ham-core::online::OnlineNotification` model | Online Notifications panel | Not implemented | Missing local notifications |
| Accessibility / iPad | No specific desktop/web evidence beyond web semantics | Web markup has ARIA labels in places | Basic SwiftUI controls | Not specifically implemented/tested |
| Tests | Rust workspace tests and GUI JS syntax status in docs | Web code included in Rust binary | iOS unit tests for utilities/export | Partial; no UI/offline/provider/sync/map tests |

## Missing iOS Work

High-confidence gaps from repository evidence:

- Add a Rust FFI boundary for iOS so Swift uses `ham-core` for core operations instead of duplicating QSO/export/lookup/station logic.
- Replace the single-screen iOS home with first-class native workspaces/feature screens.
- Add Rust-backed dashboard snapshots for station, recent QSOs, provider state, sync state, offline queue, diagnostics, and map context.
- Add station/equipment/profile management surfaces matching `ham-core::station`.
- Expand QSO logging fields and route save operations through the Rust proposal/event model.
- Add callsign lookup screens using existing lookup/service provider contracts.
- Add provider, credential, upload, sync, map, POTA/SOTA, Net Control, diagnostics, backup/restore, and notification surfaces.
- Store secrets in Keychain only; the Rust credential abstraction exists, but the iOS Keychain adapter is absent.
- Add iOS-specific tests for bridge decoding, offline queue behavior, provider mock state, sync state, map state, and critical view models.

## Existing Rust Reuse Targets

- QSO events/proposals/projections: `ham-core::event`, `ham-core::proposal`, `ham-core::projection`.
- ADIF import/export: `ham-core::adif`.
- Lookup: `ham-core::lookup` and `ham-core::service`.
- Station/equipment: `ham-core::station`.
- Upload queue and online services: `ham-core::upload`, `ham-core::online`.
- Maps/GIS: `ham-core::map`.
- Net Control: `ham-core::net`.
- Diagnostics: `ham-core::diagnostics`, `ham-core::runtime_log`.
- Credentials: `ham-core::credential`.
- Sync: `ham-sync`.

## Implementation Direction

The iOS app should move toward:

SwiftUI views -> Observable view models -> Rust FFI bridge -> `ham-core`/shared models -> hosted sync API.

Swift should retain only platform work: UI, navigation, CoreLocation, MapKit, camera/file/share APIs, local notifications, Keychain access, and iOS-specific background scheduling.

## 2026-07-10 Rust-Authority Follow-Up Audit

Evidence reviewed in this follow-up pass:

- `crates/ham-ios-ffi/src/lib.rs`
- `crates/ham-ios-ffi/include/ham_ios_ffi.h`
- `scripts/ios/*.sh`
- `ios/KE8YGWLogger/KE8YGWLogger.xcodeproj/project.pbxproj`
- `ios/KE8YGWLogger/KE8YGWLogger/Shared/RustBridge/RustBridge.swift`
- iOS QSO, station, POTA, SOTA, Net Control, diagnostics, and backup views
- `.github/workflows/ios.yml`

| Area | Repository Evidence | iOS Status | Remaining Gap |
|------|---------------------|------------|---------------|
| Apple target compilation | Scripts install/build `aarch64-apple-ios`, `aarch64-apple-ios-sim`, and optional `x86_64-apple-ios` | Tooling added | Not executed in this Windows environment |
| XCFramework packaging | `scripts/ios/build-xcframework.sh` outputs `artifacts/HamIOSFFI.xcframework` | Tooling added | Artifact not generated here |
| Xcode linkage | Project references `../../artifacts/HamIOSFFI.xcframework` and has a build phase invoking the script | Reproducible linkage path added | Xcode link not validated here |
| ABI safety | FFI now uses byte-buffer command entry, response envelope, deallocator, panic containment, UTF-8/null/size checks | Improved | Legacy C-string snapshot functions remain for compatibility |
| Native tests | `RustBridgeTests.swift` covers fallback envelope/mutation/self-test/error mapping | Expanded | Swift/Xcode tests not run here |
| Mutation authority | QSO create/delete, station profile/equipment/select, POTA/SOTA activation start/end, and Net Control start/check-in/traffic/end route through Rust bridge commands | Partially Rust-authoritative | Emergency assignments, spotting posts, provider queue actions, and some settings remain Swift-local |
| SwiftData cache behavior | QSO/station/equipment models now carry canonical ID, projection source/schema, Rust revision, tombstone, refresh timestamp; `ProjectionRefreshService` upserts Rust records | Cache/projection role implemented for key entities | Existing legacy cache rows without canonical IDs require migration/cleanup |
| Sync authority | iOS sync screen consumes Rust sync snapshot; QSO writes append official events for later sync | Display is Rust-backed | Push/pull/merge/conflict-resolution commands are not fully exposed to iOS yet |
| Backup authority | ADIF export/backup prefers Rust bridge; Rust returns backup schema version in diagnostics | ADIF path improved | JSON/ZIP backup inspect/dry-run/apply restore are not Rust-authoritative yet |
| CI coverage | `.github/workflows/ios.yml` builds XCFramework, verifies symbols, runs FFI tests, builds/tests simulator target | Workflow added | Workflow has not run in this environment |

Do not mark Apple device, simulator, archive, or TestFlight validation complete
until a macOS/Xcode run produces passing logs.
