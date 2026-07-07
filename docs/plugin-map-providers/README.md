# Map Provider Development

Map, weather, and propagation providers are plugins that register services with
the Unified Service Framework. They should not write official log events and
should not own business workflows.

## Provider Metadata

Providers declare:

- `provider_id`
- `display_name`
- source plugin ID
- service type
- capabilities
- priority
- health
- offline support
- network requirement
- required permissions
- required config keys
- required credential references

Map capabilities include online/offline tiles, vector/raster output, geocoding,
reverse geocoding, terrain, elevation, routing placeholder, and overlays.

## Permissions

Typical permissions:

- `map.view`
- `map.configure`
- `service.provider.register`
- `service.provider.configure`
- `service.cache.read`
- `service.cache.write`
- `network.external.lookup` for online lookup/geocoding-style providers
- `weather.view`
- `propagation.view`

External providers must also satisfy operator role permission checks and
credential/config requirements.

## Implementation Rules

- Use core GIS models rather than provider-specific coordinate structures at the
  application boundary.
- Return safe metadata only.
- Do not log API keys, tokens, raw credentials, or private profile fields.
- Cache provider data in support/service cache storage, not official events.
- Publish runtime events for request start/completion/failure, health changes,
  cache hits/misses, and fallback use.

## Current Provider Stubs

The MVP includes offline, OpenStreetMap placeholder, and mock map provider
metadata. Real providers should replace these stubs incrementally without
changing the map workspace contract.
