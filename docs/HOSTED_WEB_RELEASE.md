# Hosted Web Release

v1 includes a hosted web app with login and the same API contract used by
desktop and native iOS clients.

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

The `ham-server` crate owns the hosted API boundary, account/session models,
device registration/revocation, logbook membership roles, and proposal-backed
QSO routes. It now also exposes one-time server-admin bootstrap, durable hosting
configuration, invite-only/open/disabled registration modes, hashed expiring
single-use invite/email-verification/recovery tokens, Turnstile verification
for public registration, secure-cookie/bearer session transport, session
rotation, logout-all, account deletion, audit records, station/equipment
support metadata routes, ADIF import/export, provider settings/test routes,
upload queue execution foundation, activation routes, Net Control routes, map
summary/settings routes, backup export/dry-run/import routes, sync pull, and
divergence review. Hosted metadata has a SurrealDB-backed durable store for
server use, with the in-memory store retained only for focused unit tests and
development fixtures. Surreal-backed hosted server mode also opens a JSONL
official event store so official history remains append-only and durable.

The self-hosted sync/report service now uses durable local storage by default:
SurrealDB for sync/support metadata, append-only JSONL for official replicated
events, and filesystem-backed diagnostic report payloads.

## Implemented API Slice

- `GET /health`
- `GET /api/v1/status`
- `POST /api/v1/admin/bootstrap`
- `GET /api/v1/admin/hosting`
- `PATCH /api/v1/admin/hosting`
- `GET /api/v1/admin/invitations`
- `POST /api/v1/admin/invitations`
- `GET /api/v1/admin/invitations/:id`
- `POST /api/v1/admin/invitations/:id/resend`
- `POST /api/v1/admin/invitations/:id/expire`
- `POST /api/v1/admin/invitations/:id/revoke`
- `GET /api/v1/admin/audits`
- `POST /api/v1/auth/register`
- `POST /api/v1/auth/verify-email`
- `POST /api/v1/auth/recovery/start`
- `POST /api/v1/auth/recovery/complete`
- `POST /api/v1/auth/login`
- `POST /api/v1/auth/logout`
- `POST /api/v1/auth/logout-all`
- `POST /api/v1/auth/session/rotate`
- `POST /api/v1/auth/account/delete`
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
- `GET /api/v1/station-profiles`
- `POST /api/v1/station-profiles`
- `GET /api/v1/station-profiles/:id`
- `PATCH /api/v1/station-profiles/:id`
- `POST /api/v1/station-profiles/:id/archive`
- `POST /api/v1/station-profiles/:id/set-default`
- `GET /api/v1/equipment`
- `POST /api/v1/equipment`
- `GET /api/v1/equipment/:id`
- `PATCH /api/v1/equipment/:id`
- `POST /api/v1/equipment/:id/archive`
- `POST /api/v1/adif/import`
- `GET /api/v1/adif/export`
- `GET /api/v1/activations`
- `POST /api/v1/activations`
- `GET /api/v1/activations/:id`
- `PATCH /api/v1/activations/:id`
- `POST /api/v1/activations/:id/end`
- `GET /api/v1/activations/:id/qsos`
- `GET /api/v1/net-control/sessions`
- `POST /api/v1/net-control/sessions`
- `GET /api/v1/net-control/sessions/:id`
- `PATCH /api/v1/net-control/sessions/:id`
- `POST /api/v1/net-control/sessions/:id/start`
- `POST /api/v1/net-control/sessions/:id/end`
- `POST /api/v1/net-control/sessions/:id/checkins`
- `PATCH /api/v1/net-control/sessions/:id/checkins/:checkin_id`
- `POST /api/v1/net-control/sessions/:id/traffic`
- `GET /api/v1/maps/qsos`
- `GET /api/v1/maps/stations`
- `GET /api/v1/maps/paths`
- `GET /api/v1/maps/settings`
- `PATCH /api/v1/maps/settings`
- `POST /api/v1/backups/export`
- `GET /api/v1/backups`
- `GET /api/v1/backups/:id`
- `GET /api/v1/backups/:id/download`
- `POST /api/v1/backups/import/dry-run`
- `POST /api/v1/backups/import`
- `GET /api/v1/providers`
- `GET /api/v1/providers/:id`
- `PATCH /api/v1/providers/:id`
- `POST /api/v1/providers/:id/test`
- `POST /api/v1/providers/qrz-xml/lookup`
- `POST /api/v1/providers/hamqth/lookup`
- `GET /api/v1/providers/pota-spots/spots`
- `POST /api/v1/providers/dx-cluster/connect`
- `POST /api/v1/providers/dx-cluster/read`
- `POST /api/v1/providers/dx-cluster/disconnect`
- `GET /api/v1/providers/dx-cluster/status`
- `GET /api/v1/uploads`
- `POST /api/v1/uploads/run`
- `POST /api/v1/uploads/:id/retry`
- `GET /api/v1/sync/status`
- `POST /api/v1/sync/preview`
- `POST /api/v1/sync/push`
- `POST /api/v1/sync/pull`
- `POST /api/v1/sync/divergence/review`
- `GET /api/v1/sync/divergence/:id`
- `POST /api/v1/sync/divergence/:id/export`
- `GET /api/v1/devices`
- `POST /api/v1/devices`
- `POST /api/v1/devices/revoke-all`
- `POST /api/v1/devices/:id/revoke`

The previous scaffolded workflow routes now have beta implementations. Future
routes should be added explicitly rather than treated as hidden mutable state.

Provider settings store credential IDs/references only. The provider test route
is deterministic in CI through fake/mock mode and reports capability tested,
credential requirement, credential reference presence/status/resolution,
provider health state, redacted diagnostics, and next recommended action without
returning secret values. Upload queue execution generates ADIF from official
projections, stores queue/history metadata in SurrealDB, deduplicates queued,
running, and successful jobs, and executes through the Tier 1 adapter boundary.
Club Log, QRZ Logbook, and eQSL can run gated live HTTP uploads when
`live_test=true` and a credential reference resolves through `CredentialStore`.
QRZ XML/HamQTH lookup execution, POTA spot fetching, and DX Cluster bounded
connect/read/disconnect/status routes are wired with fake mode as the default.
Live mode remains explicit and provider-account validation is still required.
SOTAWatch live access is disabled pending API approval/terms handling, and LoTW
TQSL signing is not production-complete. Hosted deployments must keep raw
provider secrets outside SurrealDB unless a future explicit server-side secret
vault is added.

Live validation is release-runner gated. All live hooks require
`HAM_LIVE_PROVIDER_TESTS=1`; upload hooks also require
`HAM_LIVE_PROVIDER_ALLOW_UPLOAD=1` and may create provider-side records. Runtime
failures expose stable redacted `error_code` values and high-level messages
only, not raw provider XML/HTML bodies.

Activation and Net Control writes go through core proposal validation and append
official events. Current core role policy requires Admin/Owner for those
workflow mutations; Viewer can read only. Map QSO/station/path responses are
derived from official projections and station profile support metadata. Map
settings, backup records, and divergence reports are support metadata in
SurrealDB. Backup export includes official events and support state without
credential secrets. Backup import requires dry-run confirmation, validates the
manifest and event chain, appends only verified missing official events, skips
exact duplicate replay, restores support metadata into the authorized
account/logbook scope, strips provider credential references, and blocks
divergent targets. Divergence review reports safe pull/push states and never
performs automatic merge.

## Required Before Production Hosted Use

- Production email provider/domain configuration and deliverability validation.
- Cloudflare Turnstile site/secret keys for public open registration.
- Hosted web UI wiring for registration, verification, recovery, session
  rotation, device revocation, and account deletion.
- Provider adapter hardening.
- Infrastructure rate-limit sizing, audit retention, monitoring, backups, DNS,
  TLS, and protected deployment environments.
- Full contract tests against deployed hosted and self-hosted modes.

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
- `HAM_SERVER_OPERATION_MODE`: `personal_hosted`, `public_hosted`, or
  `self_hosted`; default `personal_hosted`.
- `HAM_SERVER_REGISTRATION_MODE`: `invite_only`, `open`, or `disabled`; default
  `invite_only`.
- `HAM_SERVER_EMAIL_MODE`: `test`, `webhook`, or `disabled`; default `test`.
- `HAM_SERVER_EMAIL_FROM`, `HAM_SERVER_EMAIL_VERIFICATION_BASE_URL`,
  `HAM_SERVER_EMAIL_RECOVERY_BASE_URL`, `HAM_SERVER_EMAIL_WEBHOOK_URL`, and
  `HAM_SERVER_EMAIL_CREDENTIAL_ID`: hosted email delivery settings. Production
  deployments must not use the test outbox as their delivery mechanism.
- `HAM_SERVER_TURNSTILE_SITE_KEY`, `HAM_SERVER_TURNSTILE_SECRET_KEY`, and
  `HAM_SERVER_TURNSTILE_SITEVERIFY_URL`: Turnstile configuration. Open public
  registration fails closed when Turnstile is enabled and the secret is missing
  or verification fails.
- `HAM_SERVER_SESSION_TTL_SECONDS`: session lifetime override.
- `HAM_SERVER_EVENT_LOG_PATH`: append-only JSONL official event log for
  `ham-server` hosted mode.
- `HAM_SERVER_SURREAL_PATH`: embedded local SurrealDB path. Stores users,
  server admins, login sessions with token hashes only, devices, logbooks,
  memberships, API tokens with hashes only, invites with token hashes only,
  email verification and recovery token hashes, rate-limit buckets, Turnstile
  replay hashes, audit records, station profiles, equipment profiles, provider
  settings without secrets, upload queue/history metadata, map settings, backup
  records, divergence reports, and schema migrations.
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
- `HAM_SYNC_SESSION_TTL_SECONDS`: sync-token session lifetime for new paired
  devices, default `2592000` seconds. Existing legacy session records without
  `expires_at` remain readable for compatibility.
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

The hosted backup API has two import phases:

1. `POST /api/v1/backups/import/dry-run` validates the manifest, format
   version, logbook scope, official event hash chain, duplicate event IDs, and
   missing provider credential references. It does not write state.
2. `POST /api/v1/backups/import` requires `confirm_dry_run: true`, repeats
   validation, blocks divergent target heads, appends only the verified missing
   event suffix, verifies the final chain, rebuilds projections, restores
   support metadata into the target scope, and returns the final head plus
   restored support sections.

Imports never rewrite existing official events and never automatically merge
divergent histories.

## Migration Notes

SurrealDB schema initialization is automatic at startup through checked
`DEFINE TABLE` and `DEFINE INDEX` statements. The first migration is recorded in
`schema_migrations` with version `3` for the hosted API metadata schema. Future
migrations should be additive and must preserve append-only official event
history.

The embedded local backend uses SurrealKV. On Windows, SurrealKV holds an
exclusive in-process file lock, so unit tests verify durable store reloads
through the storage abstraction rather than opening a second embedded handle to
the same directory inside one process. A real process restart releases the
embedded lock.

## Current Limitations

- Session expiry/refresh policy is still beta-level and not production hardened.
- Club Log, QRZ Logbook, and eQSL live uploads are gated behind explicit
  provider settings and credentials; release-runner validation with real
  provider accounts remains.
- Real-account validation for hosted QRZ XML/HamQTH lookup, POTA spot fetch,
  DX Cluster read-once operation, and Club Log/QRZ Logbook/eQSL uploads remains.
- SOTAWatch live route work remains deferred pending approved API/terms handling.
- LoTW TQSL signing remains deferred until a safe certificate-signing model is
  designed.
- Backup import is conservative and same-logbook only. Importing a backup into
  a different logbook would require re-authoring official events and is blocked
  for v0.2.
- Full Tauri runtime packaging is not wired into hosted CI yet.
