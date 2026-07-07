# Plugin SDK and Plugin Model

Plugins extend the platform, but they do not own official data writes. A plugin submits proposals, registers UI panels or commands, publishes runtime diagnostics, and receives data through core APIs and event bus subscriptions.

## Manifest

Current manifest fields:

```text
plugin_id
name
version
author
description
requested_permissions
optional_permissions
contributed_panels
contributed_commands
plugin_type
minimum_core_version
capabilities
```

Unknown permissions are invalid unless explicitly represented as a forward-compatible `Other` value during parsing and rejected during manifest validation.

## Permission Declarations

Plugins must declare the permissions they need. A declaration is only a request. Access requires a stored plugin grant and an operator role permission.

Current permission categories include:

- QSO: `log.qso.view`, `log.qso.create`, `log.qso.correct`, `log.qso.delete`, `log.qso.restore`, `log.qso.note.add`, `log.qso.view_deleted`
- Activation: `activation.view`, `activation.create`, `activation.update`, `activation.end`, `activation.cancel`
- ADIF: `adif.import`, `adif.export`
- Sync: `sync.lan.discovery`, `sync.lan.pull`, `sync.lan.push`, `sync.cloud.connect`, `sync.cloud.pull`, `sync.cloud.push`
- Lookup: `lookup.callsign`, `lookup.entity`, `lookup.grid`, `cache.lookup.read`, `cache.lookup.write`, `network.external.lookup`
- Rig: `rig.view`, `rig.read.state`, `rig.control.frequency`, `rig.control.mode`, `rig.control.ptt`, `rig.control.split`, `rig.configure`
- Diagnostics: `diagnostics.view_logs`, `diagnostics.export`, `diagnostics.upload`
- Services: `service.provider.register`, `service.provider.configure`, `service.provider.enable`, `service.provider.disable`, `service.cache.read`, `service.cache.write`, `service.cache.clear`
- Uploads: `upload.log`, `upload.confirmation_pull`, `upload.queue.manage`, `upload.status.view`, `network.external.upload`
- Spotting/maps/weather/propagation: `spotting.view`, `spotting.configure`, `network.external.spotting`, `map.view`, `map.configure`, `network.external.map`, `weather.view`, `network.external.weather`, `propagation.view`, `network.external.propagation`
- Automation/notifications: `automation.manage`, `notification.view`
- Station: `station.profile.view`, `station.profile.manage`, `station.equipment.view`, `station.equipment.manage`, `station.profile.use`
- UI/settings: `ui.panel.register`, `ui.command.register`, `settings.read`, `settings.write`

## Proposal Submission

Plugins submit proposal envelopes to the core. The core:

1. Checks plugin grants.
2. Checks operator role permissions and scope.
3. Validates the proposal schema and domain rules.
4. Creates the official event.
5. Appends the event through the official event store.
6. Publishes runtime events.
7. Updates or rebuilds projections.

Plugins must not write official events, mutate projections directly, or bypass validation through GUI endpoints.

## Panels and Commands

Panels and commands are contributions, not permissions. A plugin can register a panel only if it has `ui.panel.register`, but the panel still needs separate data permissions for any protected action it performs.

Current GUI contributions are static. Future runtime plugin loading must preserve:

- stable panel IDs
- stable command IDs
- declared required permissions
- supported workspace declarations
- runtime event subscriptions through the core event bus or bridge

## Service Providers

Plugins that integrate with replaceable data sources should register service providers through the shared service framework. Provider-capable plugins declare contributed service types in the manifest and expose provider metadata to the core registry.

Provider metadata must include stable provider ID, service type, capabilities, required permissions, config keys, offline/network behavior, health, and priority. Service consumers should request capabilities from the registry rather than hard-coding QRZ, LoTW, POTA spots, maps, weather, or propagation providers.

Provider cache entries are support data, not official events. Provider results remain advisory unless accepted through a proposal-backed workflow.

## Implemented Static Plugins

- `plugin.pota-sota` - activation workflow and portable logging.
- `plugin.callsign-lookup` - advisory callsign/entity/grid lookup and cache.
- `plugin.rig-control` - mock rig state and frequency/mode autofill.
- `plugin.log-upload` - LoTW/eQSL/Club Log/QRZ Logbook provider stubs and upload queue permissions.
- `plugin.online-services` - connected provider metadata, upload/download engine, confirmations, DX/POTA/SOTA spots, weather, propagation, maps, automation, and notifications.
- `core.station` - station/equipment support state and logger defaults.
- `core.awards` - projection-backed award progress.
- `core.search` - projection-backed advanced QSO search.
- `core.sync` - LAN/cloud sync controls exposed as core-owned plugin-like capabilities.
- `core.diagnostics` - runtime logs, bundles, and report uploads.

## Future Plugin Runtime

Open design questions:

- WASM, process isolation, dynamic libraries, or internal crate plugins.
- Plugin signing and marketplace trust.
- Fine-grained scopes and organization-managed policies.
- Sandbox resource limits.
- Stable ABI/API versioning.
