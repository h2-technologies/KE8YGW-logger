# Online Provider Development

Online providers must register through the Unified Service Framework and use
the shared permission, credential, cache, runtime event, and proposal systems.

## Required Rules

- Do not write official events directly.
- Do not log passwords, tokens, certificates, API keys, or raw provider
  responses containing secrets.
- Use `CredentialStore` and credential IDs for secrets.
- Publish runtime events for auth, upload, download, lookup, spot, weather,
  propagation, cache, and health activity.
- Use service cache for expirable support data.
- Keep provider-specific raw metadata safe and redacted.

## Provider Capabilities

Examples:

- `upload.adif`
- `upload.incremental`
- `upload.confirmation_pull`
- `lookup.callsign.full`
- `spotting.dx_cluster`
- `spotting.pota`
- `spotting.sota`
- `spotting.rbn`
- `weather.current`
- `weather.forecast`
- `propagation.solar_indices`
- `map.tiles.online`
- `map.tiles.offline`

## Permissions

Network access is separate from view permissions:

- `network.external.upload`
- `network.external.lookup`
- `network.external.spotting`
- `network.external.map`
- `network.external.weather`
- `network.external.propagation`

Automation and notifications use:

- `automation.manage`
- `notification.view`

## Testing Expectations

Each provider should include offline tests for:

- request/response serialization
- credential missing and auth failure paths
- rate limiting and retry metadata
- parser behavior
- cache hit/miss/expiration
- runtime event payload redaction
- official confirmation event append path where applicable
