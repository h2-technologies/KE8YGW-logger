# Compressed Roadmap

Each pass should ship a vertical slice across core, GUI or client models, tests, documentation, and CI health. Future prompts should reference `docs/MASTER_BLUEPRINT.md` and preserve the locked decisions.

Release scope correction: v0.2 is the almost-v1 beta, and v1.0 is hosted web plus installable desktop with a
shared Rust core, shared hosted/self-hosted API, sync, production providers, and
production credential storage. v1.0 must not ship or document a PWA as the iOS
client. v1.1 is the native SwiftUI iOS release. See `V1_RELEASE_PLAN.md`,
`V0_2_RELEASE_PLAN.md`, `V1_1_IOS_NATIVE_PLAN.md`, `API_CLIENT_CONTRACT.md`,
`HOSTED_WEB_RELEASE.md`, `DESKTOP_RELEASE.md`, and `IOS_APPSTORE_READINESS.md`.

## Passes

1. **Core Platform + CI**
   - Workspace, shared core, plugin SDK, official/runtime event types, proposal interfaces, CI/release workflows, local commands, README architecture.
   - Status: implemented.

2. **GUI Shell + Runtime Event Monitor**
   - Web/Tauri-ready shell, workspaces, panel registry, command palette, settings, plugin manager placeholder, runtime event bridge, JSONL rotating logs, monitor filters/export.
   - Status: implemented.

3. **Official Log Store + QSO Workflow**
   - Append-only official event store, hash chain, QSO create/correct/delete/restore/note proposals, projections, Casual Logger, Recent QSOs, tests.
   - Status: implemented.

4. **Durable Storage + ADIF**
   - Durable local event storage, startup chain verification/rebuild, ADIF parser/exporter, duplicate detection, GUI/CLI import/export, docs/tests.
   - Status: implemented with JSONL official event storage.

5. **Sync Foundation**
   - IPv4/IPv6 LAN discovery, peer registry, handshake, head comparison, safe preview pull and pull missing events over LAN, GUI Sync Status, tests.
   - Status: implemented as protocol/model layer with MVP GUI/demo paths; real peer-to-peer HTTP transport remains high priority.

6. **Cloud/Self-Hosted Sync**
   - Sync server/client, pairing-token MVP auth, push/pull/preview via cloud, self-hosted config/Docker, GUI cloud settings, tests.
   - Status: implemented with durable SurrealDB support metadata and JSONL
     official event storage for hosted/self-hosted server paths.

7. **POTA/SOTA Vertical Slice**
   - POTA/SOTA plugin model, activation events/proposals/projections, QSO activation links, activation GUI, ADIF fields, tests.
   - Status: implemented as static plugin-manifest workflow inside current crates.

8. **Smart Logging Integrations**
   - Callsign lookup/enrichment plugin, cache, offline prefix/entity/grid helpers, rig control foundation, mock rig, band inference, logger autofill, tests.
   - Status: implemented with offline/mock providers and Hamlib stub.

9. **Diagnostics + Support Reports**
   - Diagnostic ZIP bundles, redaction, recent action timeline, authenticated report upload endpoint, report IDs/status, GUI Report a Problem, tests.
   - Status: implemented with in-memory server-side report storage.

10. **Permissions + Roles Hardening**
    - Typed permission registry, manifests, grant storage, plugin permission UI, operator role checks, centralized authorization, tests.
    - Status: implemented for current proposal/runtime paths; scopes and role UI remain partial.

11. **Unified Service Framework**
    - Shared provider registry, provider metadata, provider selection/fallback, service cache, service permissions, lookup refactor, upload/spotting/map/weather/propagation provider skeletons, GUI Service Providers screen, docs/tests.
    - Status: implemented as the next architecture layer. Live external providers and secure credential storage remain planned.

12. **Daily Driver Logging**
    - Station/equipment profiles, award engine foundation, advanced search/filtering, upload queue foundation, keyboard-first logging improvements.
    - Status: implemented at foundation level. Real online providers, durable upload queue settings, and full award databases remain planned.

13. **Secure Credentials + Net Control MVP**
    - Credential store abstraction, OS keychain placeholder, explicit dev fallback, credential manager UI, net roster/check-ins/traffic queue, tactical callsigns, and reports.
    - Status: implemented at foundation level. ICS exports and EmComm workspace remain planned.

14. **Contesting MVP**
    - Contest framework, exchange templates, dupes, scoring, multiplier projections, Cabrillo export, keyboard-first contest workspace, Field Day template.
    - Status: planned.

15. **Maps/Propagation + Polish**
    - Map plugin framework, Maidenhead overlays, great-circle/bearing, grayline placeholder, layout editor, theme polish, performance pass, migrations, v1.0 hardening.
    - Status: implemented at foundation level. Full tile/vector renderer and live providers remain planned.

16. **Online Services Ecosystem**
    - LoTW/eQSL/Club Log/QRZ/HRDLog provider metadata, QRZ/HamQTH/FCC lookup metadata, DX Cluster/RBN/POTA/SOTA spot models, NOAA/Open-Meteo/OSM provider metadata, upload/download engine models, confirmation import events, automation, notifications, and Online Services workspace.
    - Status: implemented at foundation level. Live network adapters, durable scheduler execution, and production keychain backends remain planned.

## Dependency Order

The next high-impact work should minimize future rewrites:

1. Extract shared plugin/UI manifests only when static plugin definitions become a blocker.
2. Wire the real Tauri runtime commands, package validation, browser tests, and
   CI release checks before public tester use.
3. Add real peer-to-peer LAN transport and trust pairing before unattended sync.
4. Add role/account/session models before broad multi-operator workflows.
5. Build live network adapters on top of the Online Services foundation: QRZ XML API, HamQTH, LoTW, eQSL, Club Log, QRZ Logbook, DX Cluster, POTA spots, SOTAWatch, real propagation/weather providers, automatic upload processing, and OS keychain/secret-store credentials.
