# Unified Service Framework

The Unified Service Framework prevents plugins from inventing separate provider systems for lookup, uploads, spotting, maps, weather, propagation, award data, AI tools, authentication, storage, and notifications.

## Core Concepts

`ham-core::ServiceRegistry` stores provider metadata, enablement state, health, priority, and preferred provider settings. The registry can register providers, reject duplicate IDs, enable or disable providers, set priority, select a preferred provider, and fall back to the next usable provider.

Provider selection considers service type, required capability, enabled state, health state, network/offline requirements, preferred provider, and priority.

Each provider declares provider ID, service type, display name, version, source plugin ID, capabilities, required permissions, config keys, priority, offline support, and network requirement.

`ham-core::ServiceCache` stores non-official provider support data. It is not append-only, is not synced by default, can expire, and can be cleared by service type or entirely.

## Implemented Service Types

- `callsign_lookup`
- `entity_lookup`
- `grid_lookup`
- `log_upload`
- `spotting`
- `map_tiles`
- `geocoding`
- `weather`
- `propagation`
- `award_data`
- `ai_tool`
- `authentication`
- `storage`
- `notification`

## Current Providers

- Callsign lookup: local prefix, mock/dev, QRZ/HamQTH stub.
- Log upload: LoTW, eQSL, Club Log, QRZ Logbook stubs.
- Spotting: mock spots plus DX Cluster/POTA/SOTA/RBN capability vocabulary.
- Map: local map and OpenStreetMap placeholders.
- Weather: manual/local placeholder.
- Propagation: mock solar/grayline placeholder.

Live external integrations are deferred until credential storage, provider configuration, and network permissions are hardened.

## Authorization

A service request is allowed only when the source plugin has permission, the active operator role allows the permission, the selected provider's permission/config requirements are satisfied, and the provider is enabled and usable.

Network-backed providers must declare network permissions separately from local lookup/view permissions.

## Runtime Events

Service workflows publish runtime events such as `service.request.started`, `service.request.completed`, `service.request.failed`, `service.request.cache_hit`, `service.request.cache_miss`, `service.provider.fallback_used`, `service.permission.denied`, and `service.config.missing`.

Runtime events are diagnostic only and are not synced.

## GUI Integration

The GUI shell exposes a Service Providers screen showing provider name, service type, plugin source, enabled state, health, priority, offline/network status, missing config warnings, capabilities, and required permissions.

Provider enable/disable and priority editing are model-level placeholders until persisted provider settings are added.
