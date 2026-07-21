# v1 Release Plan

Last audited: 2026-07-21

v1 ships on November 24, 2026 with hosted web, native iOS, and signed desktop
clients for Windows, macOS, and broad Linux distribution support. This plan
preserves the locked scope in issue #2 and does not add features outside that
scope.

## Locked Product Scope

- Hosted web is online-only.
- Native iOS and desktop are fully usable offline and reconcile later.
- Desktop supports Windows, macOS, and Linux.
- Deployments include personal hosted, public hosted, and documented
  self-hosted modes.
- Public registration is invite-only by default, with an administrator switch
  for open registration, verified email, and Cloudflare Turnstile.
- Required providers are QRZ, QRZ Logbook, LoTW, eQSL, Club Log, POTA,
  SOTAWatch, DX Cluster/RBN, maps, and propagation.
- Maps support cached/offline regions on desktop and iOS.
- Contesting includes Field Day, Winter Field Day, generic serial/grid
  templates, and release-adjacent December/January contest packs.
- EmComm includes ICS 211, 213, 213RR, 214, personnel, assignments, and
  message/communications records.
- Desktop automatically downloads signed updates on unmetered connections,
  allows metered downloads by opt-in, and prompts before installation.
- Windows uses Microsoft Trusted Signing. Apple releases use Apple
  signing/notarization/App Store distribution.
- v1.1 adds a TUI. Awards, rig control, and weather do not block v1.

## Current Implementation State

Implemented:
- Shared Rust workspace, append-only official events, proposal validation,
  projections, ADIF import/export, POTA/SOTA activation events, Net Control
  events, maps/GIS foundations, provider framework, upload queue foundation,
  diagnostics, hosted `/api/v1` route slices, self-hosted sync/report server,
  Tauri desktop wrapper, and native iOS SwiftUI/Rust-bridge foundation.

Partial:
- Hosted web/desktop/iOS UX for the implemented account/session/device/admin
  APIs, production email/Turnstile deployment configuration, hosted
  backup/divergence UX, provider runtime execution, cloud/self-hosted sync,
  production LAN pairing UX, stronger LAN key-exchange hardening, desktop
  packaging, native iOS projection/cache flows, and release automation.

Test-only or fake/default paths:
- Mock lookup/rig providers, placeholder map/weather/propagation providers,
  deterministic fake provider execution, in-memory hosted/sync test stores, and
  GUI demo peer/runtime data.

Deferred or unimplemented:
- Production reciprocal LAN pairing UX, stronger LAN key-exchange hardening,
  physical-device LAN/iOS local-network validation, LoTW/TQSL live upload,
  SOTAWatch approved live access, RBN/background DX lifecycle,
  cached/offline map regions, full contesting, full EmComm forms, signed desktop
  updater, production
  signing/notarization/App Store distribution, full production provider
  qualification, and release-candidate operations hardening.

## Schedule Gates

- Repository/release reset: July 31, 2026.
- Platform foundation: August 21, 2026.
- Providers: September 18, 2026.
- Web/desktop feature complete: September 25, 2026.
- iOS feature complete: October 9, 2026.
- Maps/contesting/EmComm complete: October 16, 2026.
- Integrated beta and feature freeze: November 6, 2026.
- Protected stabilization buffer: November 7-20, 2026.
- Final approval: November 21-23, 2026.
- Launch: November 24, 2026.

## Release Gates

- All v1 child epics from issue #2 are complete.
- No v1-required provider is stub-backed.
- Desktop and iOS pass offline/reconciliation scenarios.
- Managed LoTW certificate mode passes security review.
- Offline map source permits caching.
- Signed desktop update path passes upgrade and rollback tests.
- Personal, public, and self-hosted deployments pass backup/restore tests.
- iOS passes TestFlight/App Store review.
- No open critical or high-severity defects.
- Release candidate remains stable for at least seven days.

## Baseline Validation

The v1 baseline uses `Cargo.toml` `[workspace.package].version` as the canonical
product version. Run:

```powershell
just version-check
just api-contract
just docs-link-check
just governance-check
just ci
```

`just version-check` validates Cargo crate versions, Tauri version metadata, iOS
marketing/build versions, OpenAPI product metadata, release artifact names, and
release-tag policy. The OpenAPI `info.version` remains `1.0.0` for the `/api/v1`
contract; `info.x-product-version` tracks the product version.

See [v1 Execution Plan](V1_EXECUTION_PLAN.md) for the dependency-ordered
remaining work.
