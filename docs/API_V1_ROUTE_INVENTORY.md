# API v1 Route Inventory

This inventory is frozen for `/api/v1` clients and is backed by
`crates/ham-api-contract`, `openapi/api-v1.yaml`, and conformance tests.
Hosted routes use bearer sessions. Self-hosted sync/report routes keep the
documented query-token and request-body `auth.sync_token` compatibility
transport until a new major API version replaces them.

All JSON errors keep the compatibility field `error` and add `code`,
`request_id`, and `retryable`. Clients that cannot use compatibility query
tokens should use hosted bearer-session routes where equivalent behavior exists.
The self-hosted sync server intentionally has no nonstandard `QUERY` method.

## Hosted Routes

| Surface | Routes | Auth | Authorization | Request data | Success | Stability |
| --- | --- | --- | --- | --- | --- | --- |
| Health/status/catalog | `GET /health`, `GET /api/v1/status`, `GET /api/v1/routes` | none | none | `Accept`, `X-Request-ID` | 200 JSON | stable |
| Admin/account | bootstrap, hosting config, invitation create/list/inspect/resend/expire/revoke, audit routes | none for first bootstrap; bearer after bootstrap | server admin | hosting/invitation JSON, path UUIDs | 200 config/invitation/audit/login | stable |
| Auth/session | register, verify-email, recovery start/complete, login, logout, logout-all, session rotate, account delete, session discovery | none or bearer | verified account and active session where required | JSON account/session bodies | 200 login/session/ok | stable |
| Logbooks | `GET/POST /api/v1/logbooks`, `GET/PATCH /api/v1/logbooks/{id}` | bearer | visible/read/admin | path UUIDs, create/update JSON | 200 list/logbook | stable |
| QSOs | `GET/POST /api/v1/qsos`, `GET/PATCH /api/v1/qsos/{id}`, `POST /api/v1/qsos/{id}/delete`, `/restore`, `/notes` | bearer | read/operator | `logbook_id`, `include_deleted`, QSO/action JSON | 200 list/QSO/proposal | stable |
| Station/equipment | station profile and equipment list/create/get/patch/archive/default routes | bearer | read/admin | `logbook_id`, path UUIDs, profile/equipment/action JSON | 200 list/item | stable |
| ADIF | `POST /api/v1/adif/import`, `GET /api/v1/adif/export` | bearer | operator/read | import JSON, `logbook_id`, `include_deleted` | 200 import/export payload | stable |
| Activations | activation list/create/get/patch/end/linked-QSO routes | bearer | read/admin | `logbook_id`, `include_ended`, activation JSON | 200 list/activation/proposal | stable |
| Net Control | session list/create/get/patch/start/end/checkins/traffic routes | bearer | read/admin | `logbook_id`, path UUIDs, net workflow JSON | 200 list/session/proposal | stable |
| Maps | `GET /api/v1/maps/qsos`, `/stations`, `/paths`, `/settings`, `PATCH /api/v1/maps/settings` | bearer | read/admin | `logbook_id`, settings JSON | 200 map data/settings | stable |
| Backups | export/list/get/download/import dry-run/import routes | bearer | read/admin | `logbook_id`, path UUIDs, backup JSON | 200 backup/import result | stable |
| Providers | list/detail/update/test/lookup/spots plus DX Cluster connect/read/disconnect/status | bearer | read/admin | provider IDs, `logbook_id`, provider JSON | 200 provider/runtime result | stable |
| Uploads | `GET /api/v1/uploads`, `POST /api/v1/uploads/run`, `POST /api/v1/uploads/{id}/retry` | bearer | read/admin | `logbook_id`, run/retry JSON | 200 upload list/job | stable |
| Sync/divergence | status/preview/push/pull/divergence review/get/export | bearer | read/operator | sync/divergence JSON, report IDs | 200 sync/report result | stable |
| Devices | `GET/POST /api/v1/devices`, `POST /api/v1/devices/revoke-all`, `POST /api/v1/devices/{id}/revoke` | bearer | active session/owner | register/revoke JSON | 200 devices/device/ok | stable |

## Self-Hosted Sync/Report Routes

| Method | Path | Auth | Authorization | Request data | Success | Stability |
| --- | --- | --- | --- | --- | --- | --- |
| GET | `/health` | none | none | none | 200 `CloudHealthResponse` | stable |
| POST | `/api/v1/auth/pair` | pairing code | pairing code grants requested logbooks | JSON `PairDeviceRequest` | 200 `PairDeviceResponse` | compatibility-only |
| GET | `/api/v1/logbooks` | query `token` | authorized sync token | `token` | 200 logbook heads | compatibility-only |
| GET | `/api/v1/logbooks/{logbook_id}/head` | query `token` | authorized logbook | path UUID, `token` | 200 head | compatibility-only |
| GET | `/api/v1/logbooks/{logbook_id}/events` | query `token` | authorized logbook | path UUID, `token`, optional `after_hash` | 200 event metadata | compatibility-only |
| POST | `/api/v1/logbooks/{logbook_id}/preview-pull`, `/pull`, `/push` | body `auth.sync_token` | authorized logbook | JSON sync request | 200 preview/pull/push response | compatibility-only |
| GET | `/api/v1/sync/status` | optional query `token` | optional authorized token | `token` | 200 status | compatibility-only |
| POST | `/api/v1/reports` | body `auth.sync_token` | authorized session | JSON redacted report upload | 200 report receipt | stable |
| GET | `/api/v1/reports/{report_id}` | query `token` | report owner/session | path report ID, `token` | 200 report metadata | stable |

List endpoints are not cursor-paginated in the current implementation. Ordering
is deterministic where the server sorts records; clients must tolerate additive
pagination fields in future `/api/v1` responses. Idempotency is guaranteed for
safe reads and for official event replay/deduplication where implemented; other
mutations should be treated as non-idempotent unless the endpoint response
explicitly reports deduplication.
