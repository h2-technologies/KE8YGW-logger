# Provider Runtime Hardening Audit

Issue #32 audit date: 2026-07-16.

This audit preserves the existing provider architecture:

- `ham-core::service` remains the shared service/provider registry.
- `ham-core::online` remains the Tier 1 provider adapter/runtime boundary.
- `ham-core::credential` remains the only secret-value access layer.
- `ham-core::upload` remains the local upload queue model.
- `ham-server` hosted provider/upload routes continue to persist support metadata outside official QSO history.

## Gap Matrix

| Requirement | Status | Evidence / disposition |
| --- | --- | --- |
| Shared service/provider framework | Implemented | `crates/ham-core/src/service.rs` registry, provider metadata, cache, selection, and permission checks retained. |
| `ham-core::online` provider adapter boundary | Implemented | Tier 1 upload, lookup, spot, DX, parser, fake, and gated live paths retained. |
| Credential references only in settings | Implemented | Hosted provider settings store `credential_id`; settings validation rejects secret-looking config fields. |
| OS credential-store abstraction | Implemented | `CredentialStore`, OS backends, unsupported backend, explicit insecure dev store retained. |
| Secret wrappers/redaction | Partially implemented | Metadata redaction and provider error redaction exist; this slice adds stable redacted outcome diagnostics and tests for provider echo in HTTP errors. A dedicated secret newtype is deferred because current public metadata shapes already serialize without raw secrets. |
| Stable provider outcome model | Implemented | Added `ProviderOutcomeKind`, `ProviderRetryClass`, and `ProviderOutcome` with stable code, message, retry class, retry-after, provider ID, correlation/request IDs, queue item ID, and redacted diagnostics. |
| Typed timeouts | Implemented | Added `ProviderHttpRuntimeConfig` with connect/request/total deadline values and per-request conversion. Existing adapters now inherit the bounded helper. |
| Bounded response handling | Implemented | `send_provider_http_request_with_config` rejects response bodies above `max_response_body_bytes`. |
| TLS verification | Implemented with guard | TLS remains enabled through `ureq` TLS; attempts to disable verification fail closed in this build. |
| Redirect policy | Implemented | HTTP runtime uses a typed `max_redirects`. |
| Compression handling | Partially implemented | Config records `accept_compression`; transport relies on `ureq` defaults. Provider adapters must not assume unlimited decompression. |
| Correlation IDs | Implemented | HTTP runtime sends `X-Correlation-ID` and returns the correlation ID in `ProviderHttpRuntimeResult`; outcome/event models carry IDs. |
| Cancellation | Partially implemented | Timeouts and bounded DX reads prevent unbounded waits. No async cancellation token is wired because current provider transports are synchronous. |
| Structured timing metrics | Implemented | HTTP runtime returns start, finish, and duration timing. Provider runtime event model includes duration, attempt, status, queue, retry, rate, and circuit fields. |
| Test injection | Implemented | HTTP runtime has config injection; tests use local loopback fixtures. Provider adapters already support fake fixtures. |
| Safe response capture | Implemented | Status/error bodies are bounded and redacted before surfacing. Raw response bodies are not exposed through hosted route responses. |
| Retry classification | Implemented | Added HTTP retry classification for temporary DNS/reset/timeout, 408, 429, 502, 503, 504, and permanent 4xx/user-action failures. |
| Exponential backoff with jitter | Implemented | Existing bounded exponential backoff retained; deterministic jitter helper added for tests/policies. |
| Retry-After bound | Implemented | Added `provider_retry_after_seconds` cap helper. |
| Rate limiting | Implemented for shared runtime model | Added per-provider/per-account/global/burst/queue-limit runtime limiter and snapshots. Limiter is instance-local and documented as such. Durable distributed enforcement is deferred. |
| Circuit breaker | Implemented | Added closed/open/half-open circuit with threshold, cooldown, bounded probes, recovery, and no opening for auth/user-action failures. |
| Provider health distinct from credential validity | Implemented | Added `ProviderRuntimeHealth` with health state, credential reference/validation status, rate/circuit/queue/last-success/failure metadata. Existing public health enums retained for API compatibility. |
| Provider health persistence | Partially implemented | Hosted provider settings persist safe health metadata and upload/lookup/spot status. The richer runtime health struct is available for durable storage; migration of every field into hosted metadata is deferred to avoid issue #20 route-schema overlap. |
| Queue state | Implemented | `UploadQueueState` and hosted `HostedQueueState` cover pending, running, retry scheduled, needs user action, succeeded, cancelled, dead-letter, and uncertain. |
| Queue leasing/concurrency | Implemented | Local queue has claim tokens, leases, expiry recovery, retry scheduling, and uncertain state after worker timeout. Hosted jobs persist claim/lease metadata around execution. |
| Queue limits/overflow | Implemented | Local queue limit defaults to 1000 and returns `QueueFull`; rate limiter exposes overflow state. |
| Idempotency | Implemented | Stable upload operation keys are generated from provider/logbook/sorted QSO IDs. Hosted duplicate detection uses the key; provider-side IDs and uncertain outcomes are persisted. |
| No false uploaded status | Implemented | Hosted jobs are only marked succeeded from provider execution success. Timeout-like retryable failures are marked uncertain rather than uploaded. |
| Mock-server fixtures | Partially implemented | Existing fake fixtures cover provider parsers and live gates. This slice adds loopback oversized/secret-echo HTTP fixtures. Full matrix of every HTTP fault across every provider is deferred. |
| DX Cluster special handling | Implemented with exception | DX remains connection-oriented and does not use HTTP runtime. It shares retry classification/error types, redaction, bounded read/connect, lifecycle state, and hosted status. Persistent streaming is intentionally not forced into HTTP. |
| Observability | Implemented model, partially wired | Added structured `ProviderRuntimeEvent`; existing runtime logs and hosted provider config continue to record safe summaries. Full metrics exporter is deferred. |
| Diagnostics | Partially implemented | Diagnostic bundles already redact secret fields. Runtime config, retry, circuit, rate-limit, queue, last failure, and freshness are represented by shared structs; broad bundle rendering is deferred to avoid schema churn. |
| Backups exclude secrets | Implemented | Hosted backup strips credential references on restore and tests assert secret sentinel absence. |
| Hosted provider routes | Implemented | Provider list/detail/update/test/lookup/spots/DX/upload routes retained. Only hosted upload metadata was extended. |
| QRZ and HamQTH lookup paths | Implemented | Fake default, credential-resolved live mode, hosted lookup routes, XML parsers, redacted errors. |
| POTA and SOTA paths | Provider-specific exception | POTA hosted fake-default/live-gated spot fetch exists. SOTAWatch live remains deferred pending approved API/terms. |
| Club Log, QRZ Logbook, eQSL paths | Implemented | Gated live HTTP upload paths retained and now use bounded HTTP helper. |
| LoTW path | Intentionally deferred | Live LoTW upload remains deferred until TQSL/certificate signing threat model is complete. |
| Public API error coordination | Intentionally minimized | Provider-internal outcome/error types were kept in provider modules. Existing hosted route envelopes were not broadly redesigned. Concurrent issue #20 files may conflict. |

## Runtime Policy

Default HTTP runtime values are conservative:

- connect timeout: 5 seconds
- request timeout: 20 seconds
- total operation deadline field: 25 seconds
- maximum response body: 512 KiB
- redirects: 5
- TLS verification: required
- user agent: `KE8YGW-logger/0.2 provider-transport`

Provider-specific overrides must be typed through `ProviderHttpRuntimeConfig`.
Adapters must not call `ureq` directly unless they are implementing a capability
that cannot use HTTP, such as DX Cluster.

Retryable conditions are limited to temporary transport failures, timeout,
HTTP 408, HTTP 429, HTTP 502, HTTP 503, HTTP 504, and explicit temporary
provider failures. Invalid credentials, authorization denial, invalid local
configuration, malformed local requests, permanent provider rejections, and
most non-rate-limit 4xx responses are not retried automatically.

Rate limiting and circuit breaker state are currently instance-local. Hosted
deployments that run multiple server instances need a shared limiter store
before relying on these limits as distributed enforcement.

## Operator Guidance

Inspect hosted uploads through `GET /api/v1/uploads?logbook_id=<id>`. Relevant
safe fields are `status`, `queue_state`, `attempt_count`, `retry_count`,
`last_attempt_at`, `next_attempt_at`, `safe_failure_code`,
`provider_side_identifier`, and `uncertain_outcome`.

Dead-letter recovery should use manual retry only after confirming provider
status and credential validity. Uncertain jobs mean the local runtime did not
receive enough evidence to mark the provider-side operation as failed or
succeeded; do not mark the QSO uploaded solely because a request was sent.

Credential-store failures should be diagnosed by checking backend status and
credential-reference validity. Diagnostic bundles may report
`credential_configured: true` or `credential_reference_status: valid`; they
must never contain secret values.

Live validation tests remain ignored and gated. Do not run upload live tests
without provider-approved test accounts and `HAM_LIVE_PROVIDER_ALLOW_UPLOAD=1`.

## Developer Rules

New provider adapters must:

- register through `ServiceRegistry`/provider metadata
- store only credential references in support metadata
- retrieve secrets only through `CredentialStore`
- use `ProviderHttpRuntimeConfig` and `send_provider_http_request_with_config`
  for HTTP transports
- use shared outcome, retry classification, rate-limit, circuit, and health
  models for cross-cutting state
- keep DX Cluster or other connection-oriented protocols capability-specific
  while sharing redaction, retry classification, health, cancellation bounds,
  and diagnostics
- avoid appending official QSO events until the runtime has enough evidence
  that the provider operation actually succeeded
