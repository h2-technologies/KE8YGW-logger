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

## Current Milestone

Daily Driver Logging is now implemented at foundation level. The app can be seriously tested as a local-first logger with station defaults, QSO entry, lookup/rig suggestions, POTA/SOTA activation links, projection-backed awards/search, and provider-backed upload queue scaffolding.

## Recommended Next Milestone

Real online service integrations:

- LoTW upload/download and confirmation pull.
- eQSL upload.
- Club Log upload.
- QRZ Logbook upload.
- QRZ/HamQTH real callsign lookup.
- OS keychain/secret-store credential storage.

## Future Milestones

- Full Tauri packaging and native file dialogs.
- Award rule databases and needed-list intelligence.
- Durable upload queue and provider settings.
- LAN trust pairing UX and real peer-to-peer transport.
- Conflict/divergence review UI.
- Net Control, EmComm, Contesting, Maps, Propagation, and AI plugins.
