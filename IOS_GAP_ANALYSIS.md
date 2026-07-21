# iOS Gap Analysis

Last audited: 2026-07-21

This analysis is based on the current repository, not old PR descriptions. The
native iOS source tree, Rust FFI bridge, Xcode project, tests, and macOS
workflow are present. iOS is v1 scope for the November 24, 2026 release.

## Current Gaps

| Area | Repository evidence | Gap |
| --- | --- | --- |
| Rust authority | `crates/ham-ios-ffi`, `Shared/RustBridge`, bridge tests | QSO correction, richer projection rebuild, backup restore, full sync push/pull/conflict, contesting, and EmComm commands still need end-to-end Rust/API-authoritative paths. |
| Offline/reconciliation | SwiftData cache models and sync snapshots | Full offline queue, retry policy, conflict handling, and reconciliation scenarios are incomplete. |
| Providers | Provider status views, fake/default provider runtime, Keychain plumbing | Production credential setup and live provider qualification for the issue #2 provider set are incomplete. |
| Maps | MapKit view and shared map/GIS models | Cached/offline regions and approved cacheable provider integration are incomplete. |
| ADIF/backup/diagnostics | Rust ADIF export preference, backup/diagnostics views | Native import, backup inspect/dry-run/apply, and diagnostics export need v1 hardening. |
| Contesting | No product surface for v1 contest pack | Contest engine/templates/scoring/Cabrillo are unimplemented. |
| EmComm | Emergency workspace placeholder | ICS 211, 213, 213RR, 214, personnel, assignments, and message/communications records are unimplemented. |
| App Store | Xcode project, Info.plist, icons, simulator workflow | Signing, provisioning, archive/device validation, TestFlight, App Store metadata, privacy manifest, support/privacy URLs, and account deletion flow are incomplete. |

## Non-Gaps

- Native iOS is no longer absent or future-only.
- The repository no longer relies on PWA/Home Screen install behavior as the iOS
  strategy.
- The Rust bridge is present; the remaining work is coverage and completion, not
  first integration.

## Next iOS Goal

Complete desktop/iOS sync and offline reconciliation after v1 account/session
contracts are hardened.
