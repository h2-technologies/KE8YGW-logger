# Hosted Web Release

v1.0 includes a hosted web app with login and the same API contract used by
desktop and future native iOS clients.

## Target

- Browser-accessible hosted web app.
- Login/session/device identity.
- Account and logbook membership scoping.
- Shared `/api/v1` contract.
- Hosted/self-hosted deployment compatibility.
- Durable server storage.
- Cloud/self-hosted sync.
- Provider configuration without secret leakage.

## Current Status

The new `ham-server` crate introduces the hosted API boundary, account/session
models, device registration/revocation, logbook membership roles, and
proposal-backed QSO routes. Hosted metadata now has a SurrealDB-backed durable
store for beta server use, with the in-memory store retained only for focused
unit tests and development fixtures.

The self-hosted sync/report service now uses durable local storage by default:
SurrealDB for sync/support metadata, append-only JSONL for official replicated
events, and filesystem-backed diagnostic report payloads.

## Implemented API Slice

- `GET /health`
- `GET /api/v1/status`
- `POST /api/v1/auth/login`
- `POST /api/v1/auth/logout`
- `GET /api/v1/auth/session`
- `GET /api/v1/logbooks`
- `POST /api/v1/logbooks`
- `GET /api/v1/logbooks/:id`
- `PATCH /api/v1/logbooks/:id`
- `GET /api/v1/qsos`
- `POST /api/v1/qsos`
- `GET /api/v1/qsos/:id`
- `PATCH /api/v1/qsos/:id`
- `POST /api/v1/qsos/:id/delete`
- `POST /api/v1/qsos/:id/restore`
- `POST /api/v1/qsos/:id/notes`
- `GET /api/v1/providers`
- `GET /api/v1/sync/status`
- `POST /api/v1/sync/preview`
- `POST /api/v1/sync/push`
- `GET /api/v1/devices`
- `POST /api/v1/devices`
- `POST /api/v1/devices/:id/revoke`

Additional v0.2 routes are reserved and return scaffolded JSON until their
domain implementation lands.

## Required Before Production Hosted Use

- Token expiry/refresh/revocation policies.
- Hosted deployment configuration.
- Rate limiting and request IDs.
- Provider adapter hardening.
- Full contract tests against hosted and self-hosted modes.

## Local Server Startup

Hosted beta API:

```powershell
cargo run -p ham-server --bin ham-server
```

Self-hosted sync/report service:

```powershell
cargo run -p ham-sync-server --bin ham-sync-server
```

Default local addresses:

- `ham-server`: `127.0.0.1:9750`
- `ham-sync-server`: `127.0.0.1:9740`

## Storage Paths

The local development defaults use the platform log/data directory returned by
`ham-core::default_log_directory()` unless an environment variable overrides
the path.

- `HAM_SERVER_BIND`: hosted API bind address, default `127.0.0.1:9750`.
- `HAM_SERVER_SURREAL_PATH`: embedded local SurrealDB path. Stores users,
  login sessions, devices, logbooks, memberships, API tokens, invites, and
  schema migrations.
- `HAM_SERVER_SURREAL_ENDPOINT`: optional remote SurrealDB WebSocket endpoint
  for hosted deployments.
- `HAM_SERVER_SURREAL_USER`, `HAM_SERVER_SURREAL_PASS`,
  `HAM_SERVER_SURREAL_NAMESPACE`, `HAM_SERVER_SURREAL_DATABASE`: remote/local
  SurrealDB credentials and namespace/database settings.
- `HAM_SYNC_SERVER_BIND`: sync/report service bind address, default
  `127.0.0.1:9740`.
- `HAM_SYNC_PUBLIC_URL`: public sync service URL returned to clients.
- `HAM_SYNC_SERVICE_MODE`: `self_hosted` or `hosted`.
- `HAM_SYNC_PAIRING_CODE`: development pairing code.
- `HAM_SYNC_SURREAL_PATH`: embedded local SurrealDB path. Stores sync sessions,
  known devices, revocation state, logbook access, pairing token records, sync
  heads, sync event references, report metadata, provider settings without
  secrets, upload queue/history metadata, and schema migrations.
- `HAM_SYNC_SURREAL_ENDPOINT`: optional remote SurrealDB WebSocket endpoint.
- `HAM_SYNC_SURREAL_USER`, `HAM_SYNC_SURREAL_PASS`,
  `HAM_SYNC_SURREAL_NAMESPACE`, `HAM_SYNC_SURREAL_DATABASE`: remote/local
  SurrealDB credentials and namespace/database settings.
- `HAM_SYNC_EVENT_LOG`: append-only JSONL official event log used by the sync
  service.
- `HAM_SYNC_REPORT_DIR`: filesystem directory for diagnostic report payloads.

## Backup Considerations

For v0.2 hosted/self-hosted beta backups, copy these files and directories while
the service is stopped or after taking a filesystem/database snapshot:

- Hosted SurrealDB metadata directory or remote SurrealDB backup.
- Sync SurrealDB metadata directory or remote SurrealDB backup.
- Sync official event JSONL file.
- Diagnostic report payload directory.
- Any separately configured official/support storage paths used by desktop or
  local profiles.

Credential secret values are not stored in SurrealDB records and must be
handled through the selected credential backend.

## Migration Notes

SurrealDB schema initialization is automatic at startup through checked
`DEFINE TABLE` and `DEFINE INDEX` statements. The first migration is recorded in
`schema_migrations` with version `1`. Future migrations should be additive and
must preserve append-only official event history.

The embedded local backend uses SurrealKV. On Windows, SurrealKV holds an
exclusive in-process file lock, so unit tests verify durable store reloads
through the storage abstraction rather than opening a second embedded handle to
the same directory inside one process. A real process restart releases the
embedded lock.

## Current Limitations

- Session expiry/refresh policy is still beta-level and not production hardened.
- The hosted API still has reserved scaffold routes for several v0.2 workflows.
- Provider adapters, upload execution, backup/restore UX, and desktop packaging
  remain separate v0.2 work.
