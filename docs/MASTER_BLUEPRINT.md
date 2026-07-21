# Ham Radio Platform Master Blueprint

This document is the repository-local copy of the product and architecture blueprint. It is authoritative for future implementation work unless an Architecture Decision Record explicitly updates it.

## Product Vision

Build a local-first, plugin-based amateur radio operations platform that replaces a pile of disconnected tools with one shared core. The platform must serve casual operators, POTA/SOTA activators and hunters, clubs, events, net control, emergency communications, contesters, and lightweight terminal or Raspberry Pi deployments.

The MVP focus is Casual Logging, POTA/SOTA, and Sync.

## v1 Release Scope

The locked v1 release target is November 24, 2026. v1 includes hosted web,
native iOS, and signed desktop clients for Windows, macOS, and broad Linux
distribution support.

v1 scope is bounded by issue #2: hosted web is online-only; desktop and iOS are
offline-capable and reconcile later; personal hosted, public hosted, and
self-hosted deployments are supported; registration is invite-only by default
with an administrator open-registration switch, verified email, and Cloudflare
Turnstile for public registration; required providers are QRZ, QRZ Logbook,
LoTW, eQSL, Club Log, POTA, SOTAWatch, DX Cluster/RBN, maps, and propagation;
maps support cached/offline regions on desktop and iOS; contesting includes the
locked release contest set; EmComm includes the locked ICS/personnel/assignment
and message-record set; desktop update behavior is signed and operator-mediated;
Windows uses Microsoft Trusted Signing; Apple releases use Apple
signing/notarization/App Store distribution. v1.1 adds a TUI.

## Locked Decisions

| Area | Decision |
| --- | --- |
| Architecture | Shared Rust core with plugin-first architecture. |
| Event model | Global append-only official logbook event stream plus entity-specific projections. |
| Deletion model | Deletes are tombstone events; official log records are never physically removed. |
| Plugin writes | Plugins submit proposals; the core validates permissions, schema, roles, and scopes before writing official events. |
| MVP hashing | SHA-256 hash chaining is enough for MVP; cryptographic signatures come later. |
| Runtime events | Runtime bus events are diagnostic only, persisted separately in rotating JSONL logs, not synced, and safe to delete. |
| Runtime log rotation | 10 MB active file, 5 retained rotated files, about 60 MB maximum including active log. |
| Sync | Prefer LAN multicast discovery and LAN replication over IPv4/IPv6; use cloud relay or self-hosted sync when local peers cannot connect. |
| Accounts | Support single operators, clubs, and events with multiple operators logging under one station callsign. |
| GUI | Dockable/workspace-based GUI shell with static default layouts first and future interactive layout editing. |
| Terminal | Keep a lightweight CLI/TUI path for Pi and specialized station use. |
| AI | AI may summarize, suggest, upload, or answer questions; it must not directly mutate official log data in MVP. |

## System Architecture

```text
Core Runtime
  - Event Bus
  - Official Event Store
  - Proposal Validation Pipeline
  - Projection Builders
  - Plugin Runtime + Permission System
  - Unified Service Registry + Provider Framework
  - Operator Roles + Account Scopes
  - Runtime Diagnostics + Rotating Logs
  - Sync Engine
  - Public API / SDK Bridge

Clients
  - Desktop/Web GUI
  - Terminal/CLI client
  - External apps using the shared core/API

Plugins
  - Casual Logger
  - POTA/SOTA
  - Callsign Lookup / Enrichment
  - Rig Control
  - ADIF Import/Export
  - Sync
  - Diagnostics
  - Future Maps, Awards, Contesting, Net Control, EmComm, AI
```

## Recommended Workspace Layout

The long-term crate layout from the blueprint is:

```text
crates/
  ham-core
  ham-plugin-sdk
  ham-storage
  ham-sync
  ham-server
  ham-adif
  ham-rig
  ham-lookup
  ham-pota-sota
  ham-diagnostics
  ham-ui-model
  ham-desktop
  ham-web
  ham-cli
  ham-tests
```

Current implementation differs intentionally while the MVP is small:

| Blueprint crate | Current location | Migration path |
| --- | --- | --- |
| `ham-storage` | `ham-core::store` | Extract when SQLite/server storage needs a larger API surface. |
| `ham-adif` | `ham-core::adif` | Extract when ADIF behavior grows beyond MVP import/export. |
| `ham-rig` | `ham-core::rig` | Extract when real Hamlib/serial/TCP providers land. |
| `ham-lookup` | `ham-core::lookup` | Extract when online provider integrations and datasets expand. |
| `ham-pota-sota` | `ham-core::proposal` and `ham-core::projection` plus GUI panels | Extract once plugin loading is real rather than static manifests. |
| `ham-diagnostics` | `ham-core::diagnostics` and `ham-core::runtime_log` | Extract if report generation grows server/client-specific code. |
| `ham-ui-model` | `ham-gui::shell` and `ham-gui::commands` | Extract before alternate GUI/TUI clients need the same models. |
| `ham-desktop`/`ham-web` | `ham-gui` static web shell | Split when Tauri packaging and web deployment diverge. |
| `ham-server` | `ham-sync-server` | Rename only if a broader server surface replaces the sync-specific binary. |

Do not rename crates only for cosmetic alignment. Prefer stable APIs and migration notes until the extraction reduces real coupling.

## Event Model

Official log events are the source of truth. They are permanent, append-only, hash chained, locally persisted, and synced.

```text
OfficialEventEnvelope {
  event_id,
  logbook_id,
  previous_hash,
  event_hash,
  timestamp,
  event_type,
  schema_version,
  author_operator_id,
  station_callsign,
  operator_callsign?,
  source_plugin_id?,
  source_device_id,
  correlation_id,
  payload
}
```

Runtime events are operational diagnostics. They feed the GUI monitor, rotating logs, and diagnostic reports. They are not official history and are never synced.

```text
RuntimeEventEnvelope {
  event_id,
  timestamp,
  event_type,
  severity,
  source,
  source_plugin_id?,
  correlation_id,
  session_id,
  device_id,
  workspace_id?,
  payload_summary,
  redacted_payload?,
  error?
}
```

## Proposal Pipeline

```text
Plugin/UI submits proposal
  -> core checks plugin permission
  -> core checks operator role and scope
  -> core validates schema and domain rules
  -> core creates deterministic official event
  -> core appends event and updates the hash head
  -> core publishes runtime events
  -> projections update or rebuild
  -> sync observes new official events
```

No plugin, GUI panel, sync peer, cloud server, AI helper, import path, or rig/lookup integration may bypass this model for local official state changes.

## Storage Model

| Store | Contains | Synced |
| --- | --- | --- |
| Official log store | Hash-chained official logbook events | Yes |
| Projection cache | Current QSO, activation, award, upload, search views | Rebuildable; optional |
| Support database | Lookup cache, entity data, plugin metadata, settings | Usually no for MVP |
| Runtime logs | Diagnostic JSONL runtime event stream | No |
| Diagnostic bundles | ZIP/report packages | Uploaded only by user action |

## Unified Service Framework

Provider-backed integrations use the shared service framework instead of plugin-specific provider registries. The framework covers callsign/entity/grid lookup, log upload, spotting, maps/geocoding, weather, propagation, award data, AI tools, authentication, storage, and notifications.

Providers declare stable metadata, capabilities, required permissions, config keys, network/offline behavior, health, and priority. Service consumers ask the registry for a provider matching the service type and capability, then the core applies permission, role, config, health, and fallback rules.

Service cache data is support data only. It is not official log data, is not append-only, can expire, can be cleared, and is not synced by default for MVP.

## Sync Architecture

Sync is local-first. LAN discovery and replication are preferred when reachable; cloud relay or self-hosted sync is a fallback. All replication paths must reuse the same safe verification rules.

- Discovery: IPv4 and IPv6 multicast.
- Handshake: exchange protocol version, device/session IDs, capabilities, logbook heads, and count hints.
- Preview Pull: compare heads and estimate missing events without writing.
- Pull: fetch missing official events, verify hashes and continuity, append only if the chain connects.
- Push: upload local official events to a server or peer, preserving original event metadata and hashes.
- Offline Queue: desktop and iOS persist versioned mutation envelopes before
  local acknowledgment; queued official mutations drain in deterministic
  per-logbook order and record accepted local event hashes for transport
  acknowledgment.
- LAN Trust: mutating LAN replication requires durable trust records,
  short-lived single-use pairing tokens, logbook scoping, replay nonce checks,
  and immediate revocation.
- Divergence: detect and report; do not auto-merge in MVP.

## GUI Architecture

The GUI is an operations shell and core client. It must not own business logic. Panels consume event bus state and submit proposals through bridge APIs.

Required shell concepts:

- left activity/sidebar navigation
- top toolbar
- workspace selector
- central workspace area
- right inspector/context panel
- bottom status bar
- command palette
- settings
- plugin manager

Default workspaces are Dashboard, Casual Logger, POTA/SOTA, Net Control, EmComm, and Contesting.

## Security Model

Every protected action requires:

```text
plugin_has_required_permission
AND operator_role_allows_permission
AND scope_allows_target_account_logbook_or_station
```

Plugin permission alone is insufficient. Operator role permission alone is insufficient. UI contribution permission does not grant data access. External network, diagnostics upload, rig write, sync push, and sync pull permissions remain separate from lower-risk read actions.

## MVP Scope

- Casual QSO logging through proposals.
- Append-only official log with corrections, deletes, restores, and notes.
- ADIF import/export.
- POTA/SOTA activation workflow.
- Callsign/entity/grid lookup with offline fallback.
- Rig state autofill using mock rig and Hamlib stub.
- Unified provider framework for lookup, uploads, spotting, maps, weather, and propagation.
- LAN discovery and safe replication.
- Cloud/self-hosted sync.
- Runtime diagnostics and authenticated report upload.
- Plugin permissions and operator role checks.
- GUI shell with workspaces and event monitor.

## Future Roadmap Themes

Later work includes awards/uploads, Net Control and EmComm, Contesting, maps/propagation, real plugin sandboxing, signatures, conflict resolution UI, mobile strategy, and hosted account/tenant design.
