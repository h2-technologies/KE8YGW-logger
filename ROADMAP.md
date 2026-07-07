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

## Current Milestone

Secure Credential Storage and Net Control MVP are now implemented at foundation level. The app can manage safe credential metadata/backend status and can run a directed net workflow through proposal-backed official events.

## Recommended Next Milestone

Maps + propagation, then real online service integrations:

- Map workspace and provider-backed offline/online tile surfaces.
- Propagation panels using solar/grayline/MUF provider skeletons.
- Weather overlays for portable and EmComm workflows.
- LoTW/eQSL/Club Log/QRZ real upload providers.
- QRZ/HamQTH real lookup providers.
- Native OS keychain/secret-store backend implementation.

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
