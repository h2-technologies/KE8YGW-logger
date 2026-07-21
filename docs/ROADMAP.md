# Compressed Roadmap

Each pass should ship a vertical slice across core, GUI or client models, tests, documentation, and CI health. Future prompts should reference `docs/MASTER_BLUEPRINT.md` and preserve the locked decisions.

Release scope correction: v1 ships November 24, 2026 with hosted web, native
iOS, and Windows/macOS/Linux desktop. `Cargo.toml` `[workspace.package].version`
is the canonical product version. A PWA, pinned hosted website, or thin web
wrapper is not the iOS client. See `V1_RELEASE_PLAN.md`,
`V1_EXECUTION_PLAN.md`, `V1_IOS_NATIVE_PLAN.md`, `API_CLIENT_CONTRACT.md`,
`HOSTED_WEB_RELEASE.md`, `DESKTOP_RELEASE.md`, and
`IOS_APPSTORE_READINESS.md`.

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
   - Status: implemented as protocol/model layer with MVP GUI/demo paths. A
     durable LAN trust store with single-use tokens, replay nonce checks, and
     revocation is implemented in `ham-sync` and exposed through GUI endpoints;
     LAN list/head/event read endpoints are guarded by trust-scoped requester
     device ID, replay nonce, and HMAC-SHA256 signature headers;
     manual direct LAN HTTP preview/pull is available between GUI instances;
     the GUI also runs IPv4/IPv6 multicast discovery with reachable identity
     probing, plus GUI LAN auth credential rotation/recovery. Production
     reciprocal pairing UX, stronger LAN key-exchange hardening, and
     physical-device LAN/iOS local-network validation remain high priority.

6. **Cloud/Self-Hosted Sync**
   - Sync server/client, pairing-token MVP auth, push/pull/preview via cloud, self-hosted config/Docker, GUI cloud settings, tests.
   - Status: implemented with durable SurrealDB support metadata and JSONL
     official event storage for hosted/self-hosted server paths. Desktop cloud
     push is queue-aware for local official mutations.

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
    - Status: implemented as the next architecture layer. Live external providers remain planned; credential storage now has OS backend wiring and still needs release-runner validation.

12. **Daily Driver Logging**
    - Station/equipment profiles, award engine foundation, advanced search/filtering, upload queue foundation, keyboard-first logging improvements.
    - Status: implemented at foundation level. Real online providers, durable upload queue settings, and full award databases remain planned.

13. **Secure Credentials + Net Control MVP**
    - Credential store abstraction, OS keychain/secret-store backend wiring, explicit dev fallback, credential manager UI, net roster/check-ins/traffic queue, tactical callsigns, and reports.
    - Status: implemented at foundation level. Clean-platform credential validation, ICS exports, and EmComm workspace remain planned.

14. **Contesting**
   - Contest framework, exchange templates, dupes, scoring, multiplier projections, Cabrillo export, keyboard-first contest workspace, Field Day template.
   - Status: unimplemented for the v1 product surface; Field Day, Winter Field
     Day, generic serial/grid templates, and December/January contest packs are
     locked v1 scope.

15. **Maps/Propagation + Polish**
    - Map plugin framework, Maidenhead overlays, great-circle/bearing, grayline placeholder, layout editor, theme polish, performance pass, migrations, v1 hardening.
    - Status: implemented at foundation level. Full tile/vector renderer and live providers remain planned.

16. **Online Services Ecosystem**
    - LoTW/eQSL/Club Log/QRZ/HRDLog provider metadata, QRZ/HamQTH/FCC lookup metadata, DX Cluster/RBN/POTA/SOTA spot models, NOAA/Open-Meteo/OSM provider metadata, upload/download engine models, confirmation import events, automation, notifications, and Online Services workspace.
    - Status: implemented at foundation level with Tier 1 adapter boundaries,
      fake/mock execution, hosted upload execution, and credential-reference
      validation. Club Log, QRZ Logbook, and eQSL have gated live HTTP upload
      transports with ignored release-runner validation hooks. QRZ XML/HamQTH
      hosted lookup execution, POTA hosted spot fetch, and DX Cluster bounded
      connect/read/disconnect/status runtime controls are wired with redacted
      error-code mapping. LoTW TQSL signing, SOTAWatch approval/terms handling,
      and durable scheduler
      execution remain planned.

17. **Offline Mutation Queue + Reconciliation Foundation**
   - Versioned offline mutation envelopes, deterministic per-logbook queue
     ordering, idempotency keys, dependency checks, retry/backoff state,
     interrupted-send recovery, desktop/iOS mutation hooks, optional target
     entity metadata, queue health snapshots, structured conflict reports for
     divergent heads, missing dependencies, unsupported schemas, concurrent QSO
     corrections, and tombstone/restore overlaps, durable manual
     conflict-review records, direct LAN HTTP preview/pull, automatic LAN
     discovery, durable LAN trust state, and desktop/iOS corrective-event
     commands that submit explicit proposals and resolve reviews with generated
     official event hashes. Deterministic shared sync golden tests cover crash
     recovery, transient retry, duplicate/reordered delivery, iOS-style pull
     replay, clock-skewed timestamps, divergent heads, conflict-review
     resolution, legacy queue migration, restore replay, and LAN revocation.
   - Status: implemented as a v0.3 foundation with HMAC-SHA256 signed LAN read
     endpoint authorization, GUI LAN auth credential rotation/recovery, and
     iOS FFI background retry planning/result classification.
     Production reciprocal pairing UX, stronger LAN key-exchange hardening,
     full guided cross-client branch review and reconciliation UI,
     physical-device LAN/iOS local-network validation, and release-device iOS
     background task/poor-network qualification remain planned.

## Dependency Order

The dependency-ordered v1 critical path is tracked in
`V1_EXECUTION_PLAN.md`. The next high-impact work should minimize future
rewrites:

1. Finish the remaining sync/reconciliation hardening: production reciprocal
   LAN pairing UX, stronger LAN key-exchange hardening,
   full guided cross-client branch review and reconciliation UI,
   physical-device LAN/iOS local-network validation, and release-device iOS
   background task/poor-network qualification before unattended desktop/iOS
   operation.
2. Complete provider runtime hardening and production provider qualification
   before release-candidate data migration or operations work.
3. Build the remaining client surfaces on top of stable account, sync, provider,
   and API contracts.
4. Complete maps, contesting, EmComm, signed updater, signing/notarization,
   App Store, operations, and release qualification after the shared foundations
   are stable.
