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
proposal-backed QSO routes. This is beta scaffolding: account/session state is
currently in-memory and must be made durable before real hosted use.

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

- Durable account/session/device/logbook storage.
- Durable sync/report storage.
- Token expiry/refresh/revocation policies.
- Hosted deployment configuration.
- Rate limiting and request IDs.
- Provider adapter hardening.
- Full contract tests against hosted and self-hosted modes.
