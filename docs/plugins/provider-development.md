# Provider Development Guide

Providers are plugin-owned integrations exposed through the shared service framework.

## When to Create a Provider

Create a provider when a plugin integrates with an external or replaceable data source: lookup services, upload destinations, spotting feeds, map/geocoding sources, weather services, propagation sources, AI tools, or storage/authentication backends.

Do not create a separate provider registry. Use `ham-core::ServiceRegistry`.

## Provider Metadata Checklist

Every provider must declare:

- stable provider ID
- service type
- display name
- semantic version
- source plugin ID
- capabilities
- required permissions
- required config keys
- optional config keys
- priority
- offline support
- network requirement

Provider IDs should be stable and namespaced enough to avoid collisions, for example `qrz-lookup`, `lotw-upload`, or `pota-spots`.

## Permissions

Providers must request only the permissions they need. Offline prefix lookup should not request external network access. QRZ/HamQTH lookup requires `network.external.lookup`. Log uploads require `adif.export` and `upload.log`. Confirmation pulls require `upload.confirmation_pull`. Live spotting requires `spotting.view` plus `network.external.spotting`.

UI panel registration does not grant data access.

## Requests and Responses

Use typed request/response structs where possible:

- `CallsignLookupRequest` / `CallsignLookupResponse`
- `LogUploadRequest` / `LogUploadResponse`
- `SpotQueryRequest` / `SpotQueryResponse`
- `WeatherRequest` / `WeatherResponse`
- `PropagationRequest` / `PropagationResponse`

Provider-specific metadata may use JSON only for safe, redacted, non-authoritative details.

## Official Data Rules

Providers must not mutate official events or projections directly. Lookup, weather, map, propagation, spotting, and AI providers are advisory unless a user submits or accepts data through a proposal-backed workflow.

Upload completion may eventually append official upload status events. Until that integration exists, upload provider stubs publish runtime events only and document that official upload status events are deferred.

## Testing Expectations

Provider tests should cover metadata serialization, permission allow/deny paths, missing config behavior, health status, cache hit/miss/expiry, fallback behavior, runtime event emission, request/response serialization, and no official event mutation unless explicitly routed through proposals.
