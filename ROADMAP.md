# Roadmap

This root roadmap summarizes the current implementation plan. Detailed architecture references live under `docs/`.

## Completed Milestones

- Shared Rust workspace and append-only official event foundation.
- Runtime diagnostic event bus and rotating JSONL runtime logs.
- GUI shell, workspace model, panel registry, command palette, settings, plugin manager.
- Durable JSONL official event storage and ADIF import/export.
- LAN discovery, handshake, safe pull replication, and cloud/self-hosted sync foundation.
- POTA/SOTA activation workflow.
- Callsign lookup/enrichment and rig control foundations.
- Diagnostic report bundles and authenticated upload.
- Plugin permission registry, grants, and enforcement hardening.
- Unified Service Framework for lookup, upload, spotting, map, weather, propagation, and future providers.
- Daily Driver Logging foundation: station/equipment profiles, award engine, advanced search, upload queue, and keyboard-first logging commands.
- Secure Credential Storage abstraction with OS-keychain placeholder, explicit dev fallback, provider credential references, and Credential Manager UI.
- Net Control MVP: sessions, check-ins, traffic queue, tombstone deletes, report export events, projection, workspace panels, and commands.
- Mapping and Propagation Framework: GIS models, Maidenhead grid engine, great-circle math, map provider model, map layers, markers, QSO/station visualization, grayline, mock propagation/weather, and Maps workspace panels.
- Online Services Ecosystem foundation: connected provider metadata, upload/download engine models, confirmation import events, DX/POTA/SOTA spot normalization, provider health, automation tasks, notifications, and Online Services workspace.
- Durable Support Storage MVP: versioned JSON sidecar storage for service provider settings, service cache metadata, upload queue state, map layer preferences, lookup/rig UI config, online automation/notification support state, and support-storage runtime events.
- Native iOS SwiftUI skeleton: Xcode project, SwiftData local persistence, QSO logging screens, station profile, ADIF/CSV export, settings, shared scheme, and unit-test targets for manual Xcode builds.
- Native iOS functional parity pass: repository gap analysis, Rust `ham-ios-ffi` bridge crate, Swift bridge client, iPhone/iPad split-view shell, Dashboard, expanded QSO logging, station/equipment management, provider/callsign/credential screens, MapKit screen, POTA/SOTA workspaces, Net Control, Emergency, Sync, Backup/Restore, Diagnostics, Keychain plumbing, local notification plumbing, and bridge fallback tests.
- Native iOS Rust-authority bridge pass: hardened byte-buffer FFI command ABI, public header/module map, Apple target build scripts, deterministic XCFramework packaging path, Xcode framework reference/build phase, typed Swift bridge wrappers, QSO/station/activation/Net Control Rust mutation routes, SwiftData projection metadata, Diagnostics self-test, and macOS CI workflow scaffolding.
- Offline Sync v0.3 foundation: durable versioned mutation envelopes, JSON queue
  store, desktop and iOS queue hooks for implemented mutations, optional target
  entity metadata, queue-aware cloud push acknowledgments, structured conflict
  reports for unsupported schemas/concurrent QSO corrections/tombstone-restore
  overlaps, and durable LAN trust records with single-use pairing tokens,
  HMAC-SHA256 signed LAN read endpoint authorization, replay nonce rejection,
  and revocation.

## Current Milestone

The current `0.3.0` workspace is the offline-sync v1 foundation baseline, not the complete
v1 product. The locked v1 release ships on November 24, 2026 with hosted web,
native iOS, and Windows/macOS/Linux desktop. A PWA, pinned hosted website, or
thin web wrapper is not the iOS client. v1.1 adds a TUI.

Implemented foundations include the hosted `/api/v1` route slices, durable
hosted and self-hosted metadata, proposal-backed QSO/POTA/SOTA/Net Control
workflows, Tauri desktop wrapper, native iOS SwiftUI/Rust bridge, provider
framework, maps/GIS foundation, diagnostics, governance, version validation,
and cross-platform CI/security automation.

Partial or incomplete v1 areas include hosted web/desktop/iOS account UX,
production email/Turnstile deployment configuration, production reciprocal LAN
pairing UX, LAN auth credential rotation/recovery, corrective-event
conflict-resolution UX, physical-device LAN/iOS local-network validation,
release-device iOS background retry qualification, production provider
qualification, cached/offline maps, contesting, EmComm forms, signed desktop
updater, Apple signing/TestFlight/App Store distribution, operations, and
release-candidate qualification.

## Recommended Next Milestone

See [docs/V1_EXECUTION_PLAN.md](docs/V1_EXECUTION_PLAN.md) for the
dependency-ordered critical path. The next three implementation goals are:

- Finish sync/reconciliation hardening: production reciprocal LAN pairing UX,
  LAN auth credential rotation/recovery, corrective-event
  conflict-resolution UX, physical-device LAN/iOS local-network validation, and
  release-device iOS background retry qualification.
- Production provider qualification for QRZ, QRZ Logbook, LoTW, eQSL, Club
  Log, POTA, SOTAWatch, DX Cluster/RBN, maps, and propagation.
- Hosted web, desktop, and iOS UI flows for the implemented account/session,
  recovery, device, and admin APIs.

## Future Milestones

- v1 platform completion: accounts, sync/reconciliation, providers, hosted web,
  signed desktop, native iOS, maps, contesting, EmComm, operations, and release
  qualification for November 24, 2026.
- v1.1 TUI release.
- Post-v1 enhancements such as award rule databases, rig-control expansion,
  weather expansion, AI assistant workflows, and plugin marketplace/sandboxing.
