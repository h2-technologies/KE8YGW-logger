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

Online Services Ecosystem is implemented at foundation level. The app can inspect connected provider metadata, credential requirements, upload queue stats, confirmation download models, spot feeds, provider health, automation tasks, notifications, weather, and propagation from a dedicated workspace.

## Recommended Next Milestone

Live Provider Adapters and Production Credential Backends:

- Native OS keychain/secret-store backend implementation.
- Real QRZ XML and HamQTH lookup clients.
- Real LoTW/eQSL/Club Log/QRZ/HRDLog upload clients.
- Real LoTW/eQSL/Club Log/QRZ confirmation download clients.
- DX Cluster Telnet background runtime.
- POTA and SOTAWatch live feed adapters.
- NOAA/Open-Meteo/space-weather live providers.
- Upload queue execution against real providers and scheduler execution.

## Future Milestones

- LoTW upload/download and confirmation pull.
- eQSL upload.
- Club Log upload.
- QRZ Logbook upload.
- QRZ/HamQTH real callsign lookup.
- OS keychain/secret-store production backend wiring.

- Full Tauri packaging and native file dialogs.
- Award rule databases and needed-list intelligence.
- Durable upload queue and provider settings.
- LAN trust pairing UX and real peer-to-peer transport.
- Conflict/divergence review UI.
- Net Control, EmComm, Contesting, Maps, Propagation, and AI plugins.
