# Provider Live Transports

This note records the v0.2 Tier 1 provider references reviewed on July 8, 2026,
and the implementation status in `ham-core::online`.

Default CI and developer tests use fake mode. Live network execution is gated by
provider settings (`live_test=true`) and external credentials stored behind
`CredentialStore`. Provider settings store credential IDs only.

Provider runtime hardening details, operator queue guidance, and the issue #32
gap matrix live in [`docs/PROVIDER_RUNTIME_HARDENING.md`](PROVIDER_RUNTIME_HARDENING.md).

## Support Matrix

| Provider | Reference | Auth | Request | Response | Status |
| --- | --- | --- | --- | --- | --- |
| Club Log upload | https://clublog.freshdesk.com/support/solutions/articles/54906-how-to-upload-qsos-in-real-time and https://clublog.org/test_realtime.html | Email, application password, callsign, API key | `POST https://clublog.org/realtime.php`, `application/x-www-form-urlencoded`, one ADIF record in `adif` | HTTP status plus body; 200 OK/duplicate/modified is success, 400 reject, 403 auth, 500 retryable | Gated live upload implemented |
| QRZ Logbook upload | https://www.qrz.com/docs/logbook/QRZLogbookAPI.html | Logbook API key; subscriber features required for insert | `POST https://logbook.qrz.com/api`, form fields `KEY`, `ACTION=INSERT`, `ADIF` | Key/value or XML-like result body; `RESULT=OK` is success | Gated live upload implemented |
| eQSL upload | https://www.eqsl.cc/qslcard/Programming.cfm and linked logger interface pages | Username/callsign, password, optional QTH nickname | `POST https://www.eqsl.cc/qslcard/ImportADIF.cfm`, form fields `UserName`, `Password`, `ADIFData`, optional `QTHNickname` | HTML/text status page; parser accepts documented success/error phrases | Gated live upload implemented conservatively |
| QRZ XML lookup | https://www.qrz.com/XML/current_spec.html | Username/password session flow | Session/login and callsign query XML responses | XML callsign fields under `Callsign`; error/session tags indicate auth/not-found | Hosted lookup route wired; fake default, live gated by settings and credential reference |
| HamQTH lookup | https://www.hamqth.com/xml.php | Username/password session flow | Session/login and callsign query XML responses | XML `search` fields; `error` indicates auth/not-found | Hosted lookup route wired; fake default, live gated by settings and credential reference |
| POTA spots | https://api.pota.app/spot/activator | None documented for current activator spots endpoint | `GET https://api.pota.app/spot/activator` | JSON array of spots | Hosted spot fetch route wired; fake fixture default, live gated by settings |
| SOTAWatch spots | https://api-db2.sota.org.uk/docs/index.html | API terms/approval expected | No live endpoint is enabled by default | JSON parser exists for fixtures | Live access deferred pending explicit API approval/terms handling |
| DX Cluster | DX Cluster telnet convention, e.g. `DX de SPOTTER: FREQ CALL COMMENT TIMEZ` | Callsign login; selected clusters may require more | TCP/Telnet host/port, send callsign, read spot lines | Text stream normalized by parser | Bounded connect/read/disconnect/status runtime wired; no always-on daemon |
| LoTW | ARRL/TQSL signing flow | TQSL certificate/private key workflow | Not modeled | Not modeled | Live upload deferred; fake mode only |

## Credential Secrets

Live upload credentials are retrieved from `CredentialStore` only for the current
operation and passed in memory to the provider adapter. Supported secret formats
are JSON objects or `key=value` pairs separated by semicolons/newlines.

Required fields:

- Club Log: `email`, `password` or `app_password`, `callsign`, `api` or `api_key`.
- QRZ Logbook: `key` or `api_key`.
- eQSL: `username` or `callsign`, `password`, optional `qth_nickname`.
- QRZ XML: `username` or `callsign`, `password`.
- HamQTH: `username` or `callsign`, `password`.

## Hosted Runtime Routes

- `POST /api/v1/providers/qrz-xml/lookup`
- `POST /api/v1/providers/hamqth/lookup`
- `GET /api/v1/providers/pota-spots/spots`
- `POST /api/v1/providers/dx-cluster/connect`
- `POST /api/v1/providers/dx-cluster/read`
- `POST /api/v1/providers/dx-cluster/disconnect`
- `GET /api/v1/providers/dx-cluster/status`

Lookup request bodies contain `logbook_id` and `callsign`. DX Cluster connect,
read, and disconnect bodies contain `logbook_id`; read may include
`read_lines` and `timeout_seconds`. POTA and DX status routes use
`?logbook_id=<uuid>`.

Provider runtime responses include `provider_id`, `mode`, `ok`, `status`,
result or spot records, `result_summary`, `failure_reason`, `redacted_error`,
and health/status metadata where applicable. Provider results never mutate QSO
records directly.

HTTP transports must use the shared provider runtime helper with typed
timeouts, bounded response bodies, redirect limits, correlation IDs, safe
response capture, retry classification, rate-limit snapshots, and circuit
breaker state. DX Cluster is connection-oriented and keeps its own bounded TCP
read lifecycle while sharing redaction, retry classification, health, and
diagnostic vocabulary.

## Redaction Guarantees

Provider diagnostics, upload history, API responses, support state, official
events, and backups must not contain credential secret values. Live HTTP errors
are redacted before becoming upload results. Official QSO records are never
mutated by provider uploads, lookups, or spots.

## Live Validation Gating

Default quality gates do not require external network access. Live validation
tests are ignored by default and must be run explicitly. All live validation
requires:

- `HAM_LIVE_PROVIDER_TESTS=1`

Any validation that uploads data also requires:

- `HAM_LIVE_PROVIDER_ALLOW_UPLOAD=1`

Upload validation may create provider-side records. Use provider-approved test
accounts, sandbox/test modes where available, or a documented manual validation
account. Do not run upload validation against a personal production log unless
that is an explicit operator decision.

Provider-specific variables:

Club Log upload:

- `HAM_CLUBLOG_TEST_EMAIL`
- `HAM_CLUBLOG_TEST_CALLSIGN`
- `HAM_CLUBLOG_TEST_PASSWORD`
- `HAM_CLUBLOG_TEST_API_KEY`

QRZ Logbook upload:

- `HAM_QRZ_LOGBOOK_TEST_KEY`

eQSL upload:

- `HAM_EQSL_TEST_USERNAME`
- `HAM_EQSL_TEST_PASSWORD`

QRZ XML lookup:

- `HAM_QRZ_XML_TEST_USERNAME`
- `HAM_QRZ_XML_TEST_PASSWORD`
- `HAM_QRZ_XML_TEST_CALLSIGN`

HamQTH lookup:

- `HAM_HAMQTH_TEST_USERNAME`
- `HAM_HAMQTH_TEST_PASSWORD`
- `HAM_HAMQTH_TEST_CALLSIGN`

POTA spot fetch:

- `HAM_LIVE_PROVIDER_TESTS=1` is sufficient. The current POTA activator spots
  endpoint is read-only and unauthenticated.

DX Cluster read-once:

- `HAM_DX_CLUSTER_TEST_HOST`
- `HAM_DX_CLUSTER_TEST_PORT` (optional, defaults to `7300`)
- `HAM_DX_CLUSTER_TEST_CALLSIGN`
- `HAM_DX_CLUSTER_TEST_TIMEOUT_SECONDS` (optional, capped at 30 seconds)

Missing provider-specific live variables cause the ignored live test to print a
high-level skip line and return. Live test output prints provider name, mode,
status, count/retryability where relevant, and redacted error code only. It must
not print raw request bodies or raw provider responses.

## Response and Error Mapping

Provider runtime responses include stable redacted `error_code` values where
practical:

- `missing_credential`
- `invalid_credential_reference`
- `auth_failure`
- `session_failure`
- `callsign_not_found`
- `malformed_response`
- `provider_rejection`
- `rate_limited`
- `permission_issue`
- `network_timeout`
- `connection_failed`
- `transport_failure`
- `provider_disabled`
- `live_mode_not_configured`

Human-readable messages remain intentionally high level. Raw XML/HTML/text
provider bodies are not returned when they may contain account/session data.
Provider-internal outcomes additionally use `ProviderOutcomeKind` and
`ProviderRetryClass` so hosted routes and future adapters can distinguish
authentication-required, authentication-rejected, authorization-denied,
rate-limited, unavailable, malformed-response, invalid-local-configuration,
timeout, transport-failure, cancelled, and uncertain-result cases.

## Confirmation Reconciliation

The current confirmation foundation can parse ADIF confirmation records and
append official confirmation/status events through the safe core path. Full
provider-specific reconciliation remains v0.2 follow-up work. Planned matching
rules should prefer provider QSO IDs where available, then callsign/band/mode
and date/time tolerance. Ambiguous matches must require review and must not
mutate QSO rows directly.

## Remaining Work

- Validate hosted QRZ XML/HamQTH/POTA/DX runtime behavior with real accounts or
  provider-approved test fixtures.
- Add full mock-server coverage for every common transport fixture across every
  provider adapter; current coverage includes parser fixtures, fake executions,
  gated live tests, oversized-body rejection, and secret echo redaction.
- Add any approved SOTA endpoint only after explicit API approval and terms
  handling are recorded.
- Add provider-specific confirmation download/reconciliation once safe matching
  semantics and provider IDs are modeled.
- Model LoTW TQSL/certificate signing before any production LoTW upload.
- Add persistent DX Cluster reconnect/background lifecycle only if needed after
  the v0.2 bounded read-once model.
