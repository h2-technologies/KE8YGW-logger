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

## Current Milestone

v0.2 almost-v1 beta is underway. The hosted API now has durable SurrealDB
metadata/support storage plus beta routes for account/session/device/logbook
scaffolding, role-scoped logbook access, proposal-backed QSO lifecycle,
station/equipment profiles, ADIF import/export, provider settings/test, upload
queue execution foundation, activation/Net Control routes, map summaries,
backup export/dry-run/import, divergence review, sync preview/push/pull, and
route tests. The GUI now has backup/restore and divergence review surfaces, and
the repository has a `ham-desktop` crate plus `src-tauri` packaging foundation
and native dialog bridge contract.

## Release Scope Correction

v1.0 targets hosted web, installable desktop, shared Rust core, shared hosted/self-hosted API, cloud/self-hosted sync, production provider integrations, and production credential storage. iOS is not part of v1.0, and PWA installability is not a release target.

v1.1 is the first native iOS target: SwiftUI, App Store-ready Xcode project, native offline queue, Keychain, native ADIF document flows, native Maps, iPhone/iPad layouts, and TestFlight. See `docs/V1_RELEASE_PLAN.md`, `docs/V1_1_IOS_NATIVE_PLAN.md`, `docs/API_CLIENT_CONTRACT.md`, and `docs/IOS_APPSTORE_READINESS.md`.

## Recommended Next Milestone

Live Provider Adapters, Credentials, and Desktop Runtime:

- Wire real Tauri runtime commands and package validation.
- Implement production OS credential backends.
- Implement live Tier 1 provider adapters.
- Browser-level GUI tests.

Then continue Live Provider Adapters and Production Credential Backends:

- Native OS keychain/secret-store backend implementation.
- Real QRZ XML and HamQTH lookup clients.
- Real LoTW/eQSL/Club Log/QRZ/HRDLog upload clients.
- Real LoTW/eQSL/Club Log/QRZ confirmation download clients.
- DX Cluster Telnet background runtime.
- POTA and SOTAWatch live feed adapters.
- NOAA/Open-Meteo/space-weather live providers.
- Upload queue execution against real providers and scheduler execution.

## Future Milestones

- v0.2 almost-v1 beta completion: live provider/credential hardening, real Tauri packaging commands, LAN trust pairing, station/equipment GUI polish, interactive map polish, browser tests, and CI release hardening.
- v1.0 web + desktop production release: hosted web, installable desktop, stabilized API/storage/sync/providers, production credentials, docs cleanup, and beta bug fixes.
- v1.1 native iOS release: SwiftUI app, native offline queue, Keychain, native ADIF document flows, native Maps, iPhone/iPad layouts, and TestFlight/App Store readiness.
- LoTW upload/download and confirmation pull.
- eQSL upload.
- Club Log upload.
- QRZ Logbook upload.
- QRZ/HamQTH real callsign lookup.
- OS keychain/secret-store production backend wiring.

- Full Tauri runtime packaging and installer/signing polish.
- Award rule databases and needed-list intelligence.
- Durable upload queue and provider settings.
- LAN trust pairing UX and real peer-to-peer transport.
- Conflict/divergence review UI hardening.
- Net Control, EmComm, Contesting, Maps, Propagation, and AI plugins.
