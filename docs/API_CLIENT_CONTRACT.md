# API Client Contract

This document defines the stable client-facing API contract that hosted web,
desktop, self-hosted deployments, and the future native iOS client must share.
v1.0 must document and test this contract even though the native iOS app ships
in v1.1.

## Compatibility Rules

- Use HTTPS for hosted production. Self-hosted local development may use HTTP.
- Use JSON request and response bodies.
- Use `snake_case` field names.
- Use RFC 3339 timestamps in UTC.
- Use UUID strings for IDs.
- Keep hosted and self-hosted behavior compatible.
- Add fields in a backward-compatible way.
- Do not remove or rename fields without a new API version.
- Do not add required request fields to existing operations under `/api/v1`.
- Optional response fields may be added; clients must ignore unknown fields.
- Enum values may be added; clients must preserve and display unknown enum
  values as unsupported instead of failing deserialization.
- Nullable fields may become populated, but non-null fields must not become null
  without a new major API version.
- Unknown response fields must be ignored by clients.
- Unknown enum values must be handled as unsupported values by clients.
- Unknown request fields are ignored unless an endpoint explicitly validates a
  sealed object; sealed validation must be documented in OpenAPI before use.
- Cursor values are opaque, scoped to the requesting account/logbook, and may
  expire. Invalid cursors return a structured validation error.
- `X-Request-ID` is optional. When present, servers echo it in error bodies and
  diagnostics; otherwise servers generate a request ID.
- `Authorization: Bearer <token>` is the preferred authenticated transport.
  Query tokens and request-body `auth.sync_token` are compatibility-only for the
  self-hosted sync/report API.
- Rate limits use HTTP 429, a stable error code, and `Retry-After` when a retry
  time is known.
- Error codes are stable. Removing or repurposing a code is breaking.
- `/api/v1` does not negotiate minor versions. Breaking changes require a new
  major path such as `/api/v2`, an ADR, migration guidance, and maintainer
  approval.
- Official log mutations must flow through validated proposals or verified
  event replication paths.

The complete route inventory is in `docs/API_V1_ROUTE_INVENTORY.md`. The
machine-readable OpenAPI contract is `openapi/api-v1.yaml`; the compatibility
baseline is `openapi/api-v1-baseline.json` and is checked by
`python scripts/check_api_contract.py`.

## Versioned API Strategy

`/api/v1` is the stable beta contract for hosted web, desktop, self-hosted
server, and the future native iOS client. Backward-compatible fields may be
added under `/api/v1`; breaking changes require a new API version.

Current `ham-server` routes reserve the broader v0.2 surface while implementing
auth, sessions, devices, logbooks, QSO lifecycle, station/equipment support
metadata, ADIF import/export, provider settings/test, upload queue foundation,
and sync preview/push/pull slices first.

## Authentication

The current sync API uses a pairing request that returns a `sync_token`.
Production v1.0 login may replace or supplement pairing for hosted web and
desktop sessions, but the resulting client token contract must remain stable for
future native clients.

The hosted beta API uses bearer sessions:

- `POST /api/v1/auth/login` creates or restores a beta account session.
- `GET /api/v1/auth/session` returns account, session, device, and membership
  data.
- `POST /api/v1/auth/logout` invalidates the session.
- Authenticated requests use `Authorization: Bearer <token>`.

Hosted beta sessions are persisted in the SurrealDB server metadata store. A server
restart must not invalidate an active session by itself. Logout persists the
inactive session state, so a logged-out token remains invalid after restart.

Current token transport:

- `POST /api/v1/auth/pair` returns `session.sync_token`.
- Some `GET` endpoints accept `?token=<sync_token>`.
- Some `POST` endpoints include `{ "auth": { "sync_token": "..." } }`.

v1.0 hardening target:

- Prefer `Authorization: Bearer <token>` for new authenticated endpoints.
- Keep any query-token compatibility documented if retained.
- Never log tokens or include them in diagnostic bundles.
- Tokens must be revocable and scoped to account, user, device, and authorized
  logbooks.

## Account, Device, and Logbook Scope

Client calls are scoped by account, logbook, user, and device:

- `UserAccount` owns the user/account identity.
- `LoginSession` binds a bearer token to account, user, and device.
- `DeviceIdentity` tracks desktop/web/native-client installs and revocation.
- `LogbookMembership` grants a user a role on a logbook.
- `LogbookRole` is `owner`, `admin`, `operator`, or `viewer`.

Cross-logbook access must be rejected. Revoked device sessions must not sync.
Viewer sessions must not mutate official log state.

Logbook membership and role checks are restart-stable. If a user cannot access a
logbook before restart, the same request must still be rejected after restart.
If a device is revoked, that revocation is durable and all sessions/sync tokens
for that device must remain unusable after restart.

## Proposal-Based Official Mutations

Hosted QSO create, edit, delete, restore, and note routes submit
`ProposalEnvelope` values to `ham-core::submit_proposal`. The hosted API must
not directly create or modify official QSO records.

Hosted ADIF import also submits proposal-backed QSO create events through the
same pipeline. Hosted ADIF export reads official projections rebuilt from the
append-only event stream. Station profiles, equipment profiles, provider
settings, and upload queue/history records are support metadata in SurrealDB and
must not be treated as official log history.

Hosted activation and Net Control mutations also submit `ProposalEnvelope`
values to `ham-core::submit_proposal`. Current core role policy requires
Admin/Owner for those workflow mutations. Operator remains allowed for QSO
logging; Viewer remains read-only.

Provider settings must contain credential IDs/references only. API clients must
not send raw password, token, API key, or secret fields in provider config; the
server rejects secret-looking keys and never returns credential secret values.
Provider test responses expose credential-reference status fields such as
`credential_reference_present`, `credential_reference_status`, and
`credential_reference_resolves`, plus `credential_required`,
`credential_resolved`, `capability_tested`, `provider_health_state`,
`redacted_diagnostics`, and `next_recommended_action`, but never include
resolved secret material. Desktop/local clients resolve credential IDs through
`CredentialStore`; hosted server provider settings remain reference-only unless
a future explicit server secret vault is designed.

Upload run responses may represent fake or live execution. Fake mode is the
default and is deterministic for CI. Live mode is enabled only by provider
settings and resolved credential references. Club Log, QRZ Logbook, and eQSL
live uploads return redacted provider summaries/errors; clients must treat
`retryable`, `status`, `failure_reason`, and `provider_error` as advisory
support metadata and must not assume QSO rows were mutated.

Hosted provider runtime routes are also support-metadata operations:

- `POST /api/v1/providers/qrz-xml/lookup`
- `POST /api/v1/providers/hamqth/lookup`
- `GET /api/v1/providers/pota-spots/spots?logbook_id=<uuid>`
- `POST /api/v1/providers/dx-cluster/connect`
- `POST /api/v1/providers/dx-cluster/read`
- `POST /api/v1/providers/dx-cluster/disconnect`
- `GET /api/v1/providers/dx-cluster/status?logbook_id=<uuid>`

Lookup routes accept `{ "logbook_id": "...", "callsign": "K1ABC" }`.
Runtime responses expose structured `status`, `result_summary`,
`failure_reason`, stable redacted `error_code`, `redacted_error`, result/spot
payloads, and provider health metadata. Error codes include categories such as
`missing_credential`, `invalid_credential_reference`, `auth_failure`,
`session_failure`, `callsign_not_found`, `malformed_response`,
`rate_limited`, `permission_issue`, `network_timeout`, `connection_failed`,
`transport_failure`, `provider_disabled`, and `live_mode_not_configured`. They
must not write provider results into official QSO rows; users or future
proposal flows must explicitly apply any suggested fields.

## Error Shape

Current server errors use:

```json
{ "error": "message" }
```

Stable v1.0 clients must handle that shape. New endpoints should prefer an
extended backward-compatible shape:

```json
{
  "error": "message",
  "code": "machine_readable_code",
  "request_id": "uuid-or-server-id",
  "retryable": false,
  "details": {}
}
```

Hosted and self-hosted `/api/v1` handlers now use this compatible shape for
API errors. `error` remains required for old clients. `code` is stable and
machine-readable. `request_id` correlates client failures with diagnostics.
`details` is optional and structured; servers must not expose secrets, raw SQL,
filesystem paths, tokens, or raw provider bodies. Authorization failures must
not reveal inaccessible resource existence.

Validation errors should use stable codes such as `invalid_json`,
`invalid_uuid`, `missing_field`, or `validation_failed`. Equivalent failures
should map to equivalent codes in hosted and self-hosted modes.

## Contract Development

To run contract checks locally:

```sh
just api-contract
cargo test -p ham-server route_catalog_lists_scaffolded_v0_2_api_surface
cargo test -p ham-sync-server self_hosted_errors_keep_stable_shape
```

When adding a route, update `crates/ham-api-contract`, `openapi/api-v1.yaml`,
the route inventory, and conformance tests. Additive changes keep all existing
paths, methods, status codes, response fields, auth requirements, and error
codes. Intentional breaking changes require a new API major version, an ADR,
migration guidance, maintainer approval, and an explicit baseline update.

## Current v1 Sync API

### Health

`GET /health`

Returns:

```json
{
  "ok": true,
  "service": "ke8ygw-sync-server",
  "version": "0.1.0",
  "mode": "self_hosted"
}
```

`mode` is `hosted` or `self_hosted`.

### Pair Device

`POST /api/v1/auth/pair`

Request:

```json
{
  "pairing_code": "local-dev-pairing-code",
  "account_id": "account-id",
  "user_id": "user-id",
  "device_id": "00000000-0000-0000-0000-000000000000",
  "device_name": "Station desktop",
  "requested_logbooks": ["00000000-0000-0000-0000-000000000000"],
  "role_hints": ["admin"]
}
```

Response:

```json
{
  "accepted": true,
  "reason": null,
  "session": {
    "account_id": "account-id",
    "user_id": "user-id",
    "device_id": "00000000-0000-0000-0000-000000000000",
    "device_name": "Station desktop",
    "sync_token": "token",
    "authorized_logbooks": ["00000000-0000-0000-0000-000000000000"],
    "issued_at": "2026-07-08T00:00:00Z"
  }
}
```

### List Logbooks

`GET /api/v1/logbooks?token=<sync_token>`

Returns:

```json
{
  "logbooks": [
    {
      "logbook_id": "00000000-0000-0000-0000-000000000000",
      "head_hash": null,
      "event_count": 0
    }
  ]
}
```

### Get Logbook Head

`GET /api/v1/logbooks/{logbook_id}/head?token=<sync_token>`

Returns a single logbook head summary with `logbook_id`, `head_hash`, and
`event_count`.

### Get Event Metadata

`GET /api/v1/logbooks/{logbook_id}/events?token=<sync_token>&after_hash=<hash>`

Returns event metadata for events after the optional hash. This endpoint is for
comparison and preview flows; full event bodies are returned by pull.

### Preview Pull

`POST /api/v1/logbooks/{logbook_id}/preview-pull`

Request:

```json
{
  "auth": { "sync_token": "token" },
  "logbook_id": "00000000-0000-0000-0000-000000000000",
  "local_head_hash": null
}
```

Returns a preview of missing server events without writing local state.

### Pull Events

`POST /api/v1/logbooks/{logbook_id}/pull`

Request:

```json
{
  "auth": { "sync_token": "token" },
  "logbook_id": "00000000-0000-0000-0000-000000000000",
  "local_head_hash": null
}
```

Returns:

```json
{
  "preview": {},
  "events": []
}
```

Clients must validate official event hashes and continuity before appending
pulled events locally.

### Push Events

`POST /api/v1/logbooks/{logbook_id}/push`

Request:

```json
{
  "auth": { "sync_token": "token" },
  "logbook_id": "00000000-0000-0000-0000-000000000000",
  "events": []
}
```

Returns:

```json
{
  "status": "pulled",
  "accepted_count": 0,
  "ignored_duplicate_count": 0,
  "rejected_count": 0,
  "server_head_hash": null,
  "errors": []
}
```

### Sync Status

`GET /api/v1/sync/status?token=<sync_token>`

Returns connection state, account/device identity when authenticated, server
URL, and accessible logbook heads.

The sync/report service persists sync sessions, device revocation state,
per-account logbook access, current sync heads, sync event references, report
metadata, provider settings without secrets, and upload queue/history metadata
in SurrealDB.
Official replicated event envelopes are stored append-only in JSONL. Clients
should expect preview, push, pull, and status responses to survive server
restart without requiring re-pairing, unless the device or token was revoked.

## Hosted Beta Client Routes

The hosted `/api/v1` beta surface uses bearer sessions and currently implements:

- Account/session/device/logbook routes for login, logout, session discovery,
  logbook membership scoping, and device revocation.
- QSO create, list, get, edit, delete, restore, and note routes backed by
  proposals and official projections.
- Station profile and equipment profile support routes scoped by
  `account_id` and `logbook_id`.
- `POST /api/v1/adif/import`, which parses ADIF and appends official QSO create
  events through the proposal pipeline.
- `GET /api/v1/adif/export`, which returns ADIF generated from official
  projections and includes filename/content metadata.
- Provider list/detail/update/test routes. Provider updates persist settings
  without secrets; tests can run in fake/mock mode for CI and return structured
  credential/health diagnostics. Provider list/detail responses include health
  summaries such as mode, enabled state, credential-reference status,
  last success/failure, last redacted error, and next recommended action.
- Upload list/run/retry routes. Upload jobs select QSOs from official
  projections, generate ADIF, persist queue/history metadata, expose retry
  state, deduplicate queued/running/successful duplicates, and execute through
  the Tier 1 provider adapter boundary. Fake mode is deterministic; Club Log,
  QRZ Logbook, and eQSL live uploads are gated by explicit settings and
  credentials. QRZ XML/HamQTH hosted lookup execution, POTA hosted spot fetch,
  and DX Cluster bounded read-once lifecycle controls are implemented. SOTAWatch
  live access remains deferred pending approved API/terms handling; LoTW live
  upload remains deferred pending a TQSL/certificate-signing model.
- Activation list/create/get/update/end routes. Writes are proposal-backed
  official activation events; reads are projection-derived.
- Net Control session/check-in/traffic routes. Writes are proposal-backed
  official Net Control events; reads are projection-derived.
- Map QSO/station/path/settings routes. QSO and path data are derived from
  official projections; map settings are SurrealDB support metadata.
- Backup export, import dry-run, and import routes. Export includes official
  events and support metadata without credential secrets. Dry-run validates
  manifest, target scope, event hash integrity, event-chain continuity,
  duplicate event IDs, and missing credential references. Import requires
  explicit dry-run confirmation, appends only verified missing official events,
  skips exact duplicate replay, restores scoped support metadata, strips
  provider credential references, and blocks divergent targets.
- Sync status/preview/push/pull routes. Pull returns only events missing after
  the requested local head for logbooks the bearer session may read.
- Sync divergence review routes. Reviews report local/client head,
  remote/server head, missing local/remote events, safe pull/push booleans, and
  recommended action. The server does not auto-merge divergent histories.

### Diagnostic Reports

`POST /api/v1/reports`

Uploads a diagnostic report bundle. Diagnostic bundles must be redacted and must
not contain credentials, API tokens, or provider secrets.

`GET /api/v1/reports/{report_id}?token=<sync_token>`

Returns diagnostic report metadata for an authorized user/session.

Diagnostic report metadata is durable. Report payloads are stored in the
configured report directory and must remain available after restart. Report
metadata and payloads are scoped to the owning account/session.

## Official Event Envelope

Pulled and pushed official events use this envelope:

```json
{
  "event_id": "00000000-0000-0000-0000-000000000000",
  "event_type": "official.log.qso.created",
  "logbook_id": "00000000-0000-0000-0000-000000000000",
  "entity_id": "00000000-0000-0000-0000-000000000000",
  "previous_hash": null,
  "event_hash": "sha256-hex",
  "timestamp": "2026-07-08T00:00:00Z",
  "author_operator_id": null,
  "station_callsign": "KE8YGW",
  "operator_callsign": null,
  "author_device_id": "00000000-0000-0000-0000-000000000000",
  "source_device_id": "00000000-0000-0000-0000-000000000000",
  "correlation_id": "00000000-0000-0000-0000-000000000000",
  "source_plugin_id": null,
  "schema_version": 1,
  "payload": {}
}
```

Clients must treat official events as append-only history. Deletes and restores
are events, not physical row removal.

## Required Future Client API Before v1.1

The current v1 sync API is enough for replication, but the native iOS app also
needs a stable CRUD-oriented client surface. Before v1.1 starts, v1.0 must
define and test endpoints or equivalent proposal APIs for:

- Hosted login/session refresh/logout.
- Account, operator role, and permission discovery.
- List, create, and select logbooks.
- Read QSO projections with pagination and filters.
- Submit QSO create, edit, delete, restore, and note proposals.
- Pagination/filter hardening for QSO, activation, Net Control, map, backup, and
  provider history endpoints.
- Backup restore/import UX hardening after the conservative v0.2 same-logbook
  import foundation.
- Provider-specific credential setup flows, health checks, and server-side
  secret-vault design if hosted deployments need to resolve provider
  credentials directly.
- Native-client divergence report presentation.

Those endpoints must use the same semantics as the Rust proposal pipeline and
official event model. They must not bypass append-only official history.

## Contract Tests

v1.0 acceptance requires tests that cover:

- Hosted and self-hosted servers return compatible responses.
- Authentication rejects missing, invalid, expired, and unauthorized tokens.
- Logbook authorization is enforced.
- Push rejects invalid hashes, unsupported schemas, wrong logbook IDs,
  duplicate IDs with different content, and divergent chains.
- Pull returns only events missing after the requested local head.
- Error responses keep a stable JSON shape.
- Future-client proposal endpoints preserve the same validation rules as local
  Rust proposal submission.
