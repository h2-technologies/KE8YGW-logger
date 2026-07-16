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

## v0.2 Tier 1 Live Transport Status

Provider live transports are documented in
[`docs/PROVIDER_LIVE_TRANSPORTS.md`](../PROVIDER_LIVE_TRANSPORTS.md).
Default tests and CI must stay fake/offline. Live tests require an explicit
environment gate and credentials stored behind `CredentialStore`.

Current live/foundation status:

- Club Log, QRZ Logbook, and eQSL: gated live ADIF upload transports.
- QRZ XML and HamQTH: live XML response parsers; hosted lookup execution still
  pending.
- POTA: live request builder and spot fixture parser; hosted fetch route still
  pending.
- SOTAWatch: fixture parser only; live access deferred pending API
  approval/terms handling.
- DX Cluster: parser and read-once Telnet client foundation; no always-on
  daemon.
- LoTW: fake/scaffold only until TQSL/certificate signing is modeled.

Credential secrets may be JSON objects or `key=value` pairs, but they must be
retrieved only through `CredentialStore` and must never be stored in provider
settings, support metadata, official events, diagnostics, logs, backups, or
test snapshots.
