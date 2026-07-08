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
- Unknown response fields must be ignored by clients.
- Unknown enum values must be handled as unsupported values by clients.
- Official log mutations must flow through validated proposals or verified
  event replication paths.

## Versioned API Strategy

`/api/v1` is the stable beta contract for hosted web, desktop, self-hosted
server, and the future native iOS client. Backward-compatible fields may be
added under `/api/v1`; breaking changes require a new API version.

Current `ham-server` routes reserve the broader v0.2 surface while implementing
auth, sessions, devices, logbooks, QSO lifecycle, provider listing, and sync
preview/push status slices first.

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

Hosted beta sessions are persisted in the server metadata database. A server
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
  "retryable": false
}
```

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
per-account logbook access, current sync heads, and report metadata in SQLite.
Official replicated event envelopes are stored append-only in JSONL. Clients
should expect preview, push, pull, and status responses to survive server
restart without requiring re-pairing, unless the device or token was revoked.

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
- Read POTA/SOTA activation state and submit activation proposals.
- Read Net Control sessions and submit Net Control proposals.
- ADIF import job upload, validation, status, and result retrieval.
- ADIF export job creation and download.
- Provider metadata, credential reference configuration, and health checks.
- Sync divergence reporting suitable for native UI.

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
