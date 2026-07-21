# Project State

Last audited: 2026-07-21

Canonical product version: `0.2.0` from `Cargo.toml`
`[workspace.package].version`.

Locked v1 release target: November 24, 2026 with hosted web, native iOS, and
Windows/macOS/Linux desktop. The locked scope is issue #2. v1.1 adds a TUI.
Awards, rig control, and weather do not block v1.

## Baseline Status

This repository is a v1 foundation baseline, not a complete v1 product. It has
the shared architecture, native-client foundations, release-policy baseline,
governance, and cross-platform automation needed to start full v1
implementation.

Open baseline items after this branch merges should move to the remaining v1
epics for accounts, sync, providers, hosted web, desktop, iOS, maps, contesting,
EmComm, operations, and release qualification.

## Implemented

- Shared Rust workspace with `ham-core`, `ham-plugin-sdk`, `ham-sync`,
  `ham-sync-server`, `ham-server`, `ham-cli`, `ham-gui`, `ham-desktop`,
  `ham-api-contract`, `ham-ios-ffi`, and `src-tauri`.
- Append-only official events, deterministic event hashing, QSO proposals,
  tombstone/restore/note flows, projections, ADIF import/export, station and
  equipment support state, awards/search foundations, upload queue foundation,
  maps/GIS foundations, diagnostics, runtime JSONL logs, and support storage.
- POTA/SOTA activation proposals/projections and Net Control official events,
  proposals, projection, and report export events.
- Hosted `/api/v1` route slices for auth/session/device/logbook, QSO,
  station/equipment, ADIF, providers, uploads, activations, Net Control, maps,
  backups, divergence review, and sync.
- Durable hosted SurrealDB metadata and durable self-hosted sync/report metadata,
  JSONL official-event storage, and filesystem diagnostic report payloads.
- Tauri v2 desktop wrapper with bundled web assets, native dialog commands, and
  restricted `/api/*` proxying.
- Native iOS SwiftUI project, SwiftData cache/projection models, Rust FFI bridge,
  public header/module map, Apple build/link scripts, shared scheme, unit tests,
  and macOS/iOS simulator workflow.
- Repository governance, MIT license, issue/PR templates, CODEOWNERS, release
  policy, branch/channel policy, private vulnerability reporting guidance,
  Dependabot config, security workflow, Scorecard workflow, and pinned
  workflow/build supply-chain dependencies from PR #101.
- Deterministic checks for API contract, product version consistency, Markdown
  links, governance/license/secrets, release-artifact naming, and production tag
  policy.

## Partial

- Hosted accounts exist as beta metadata and session/device/logbook route
  scaffolding; production invite/open registration, verified email, Turnstile,
  account deletion/recovery, and operational policy are incomplete.
- Sync has discovery, handshake, preview/pull/push verification models, and
  durable self-hosted backend; real LAN peer-to-peer HTTP transport, trust
  pairing, automatic/user-directed merge policy, and full desktop/iOS
  reconciliation are incomplete.
- Providers have metadata, fake/default execution, credential references,
  hosted QRZ XML/HamQTH lookup, POTA spot fetch, bounded DX Cluster controls,
  and gated Club Log/QRZ Logbook/eQSL live uploads; LoTW/TQSL, SOTAWatch live,
  RBN, propagation/weather/map production adapters, confirmation download, and
  full provider release qualification are incomplete.
- Desktop has a real Tauri wrapper and native dialog bridge; signed packaging,
  updater behavior, notarization, Trusted Signing, and cross-runner installer
  qualification are incomplete.
- iOS has native SwiftUI/Rust bridge foundations; App Store signing,
  TestFlight/App Store distribution, full offline/sync reconciliation, cached
  maps, provider setup, contesting, EmComm, device/archive validation, and
  production privacy review are incomplete.
- Maps have reusable GIS, layer, marker, grayline, weather, and propagation
  models; interactive tile/vector rendering and cached/offline regions are
  incomplete.

## Test-Only, Mock, Fake, Or Stub

- Mock lookup and rig providers.
- Placeholder map/weather/propagation providers.
- Deterministic fake/default online provider execution for ordinary tests.
- In-memory hosted metadata and sync backends used by tests.
- GUI demo LAN peer and demo runtime events.
- iOS Rust-bridge fallback payloads used when the Rust library is unavailable.

## Deferred Or Unimplemented For v1

- Production account registration modes, verified email, Turnstile, and hosting
  operations.
- LAN trust pairing and real peer-to-peer LAN transport.
- Full desktop/iOS offline queue, reconciliation, and conflict review.
- LoTW/TQSL managed certificate/signing mode, SOTAWatch approved live access,
  RBN/DX background lifecycle, production maps/offline caching, and propagation
  provider qualification.
- Contesting: Field Day, Winter Field Day, generic serial/grid templates,
  release-adjacent December/January contest packs, scoring, dupes, multipliers,
  and Cabrillo export.
- EmComm: ICS 211, 213, 213RR, 214, personnel, assignments, and
  message/communications records.
- Signed desktop updater, package signing/notarization, TestFlight/App Store
  release, production infrastructure, operations runbooks, and release-candidate
  soak.
- Runtime plugin loading, sandboxing, and signatures.

## Validation Baseline

Local commands:

```powershell
just fmt-check
just clippy
just test
just feature-matrix
just api-contract
just version-check
just docs-link-check
just governance-check
just ci
```

CI coverage:

- `.github/workflows/ci.yml` runs change-aware Rust formatting, Clippy, tests,
  feature matrix, API contract, version consistency, documentation links,
  governance/license checks, JavaScript syntax, Windows/macOS platform checks,
  Tauri validation, sync-server container build/smoke, and internal/beta channel
  manifests.
- `.github/workflows/ios.yml` runs Rust FFI and iOS simulator validation on
  macOS.
- `.github/workflows/security.yml` runs Cargo advisory checks, cargo-deny
  advisories, local Semgrep SAST/SARIF upload, and actionlint.
- `.github/workflows/release.yml` validates production tags, requires the tag to
  match the workspace version and be contained in `main`, checks for successful
  main CI, builds versioned release artifacts, generates checksums, attests
  archives/checksums, and publishes only from validated production tags.

Known manual repository/external settings remain in
`docs/security/REPOSITORY_SECURITY_SETTINGS.md` and
`docs/V1_EXECUTION_PLAN.md`.

## Baseline Issue Audit

| Issue | Status after this branch merges | Evidence |
| --- | --- | --- |
| #15 Native iOS integration | Satisfied for baseline integration; release hardening remains in v1 iOS work | Merged PR #1 and PR #96, `ios/KE8YGWLogger`, `crates/ham-ios-ffi`, `scripts/ios`, `.github/workflows/ios.yml`, `.gitignore`, PR #101 passing iOS checks |
| #16 Scope/docs consistency | Satisfied for baseline docs | `README.md`, `ROADMAP.md`, `docs/ROADMAP.md`, `docs/V1_RELEASE_PLAN.md`, `docs/V1_IOS_NATIVE_PLAN.md`, `docs/IOS_APPSTORE_READINESS.md`, `docs/V1_EXECUTION_PLAN.md`, `AGENTS.md` |
| #17 Version and channels | Satisfied for baseline version/channel policy | `scripts/check_versions.py`, `justfile`, `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `RELEASE.md`, `docs/BRANCHING_AND_RELEASE_CHANNELS.md`, OpenAPI `x-product-version` |
| #18 Governance/license | Already closed by PR #88; still verified | `LICENSE`, `GOVERNANCE.md`, `CONTRIBUTING.md`, `SECURITY.md`, `SUPPORT.md`, `.github/CODEOWNERS`, templates, `scripts/governance-check.ps1` |
| #19 Cross-platform CI baseline | Satisfied for baseline automation when this branch merges with PR #101 work | CI, iOS, security, scorecard, release workflows; dependency/security docs; version/docs-link/governance checks; container smoke; Tauri/platform validation |

## Next Recommended Goal

Implement v1 accounts and deployment-mode hardening for personal hosted, public
hosted, and self-hosted operation. That goal unblocks production web flows,
native iOS authentication/session behavior, provider credential setup, and
release operations.
