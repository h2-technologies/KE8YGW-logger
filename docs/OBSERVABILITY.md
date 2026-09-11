# Server Observability

Both KE8YGW Logger server binaries expose a Prometheus scrape endpoint so a
Prometheus + Grafana stack can monitor a deployment end to end.

| Server | Binary | Default bind | Scrape endpoint | Readiness endpoint |
| --- | --- | --- | --- | --- |
| Self-hosted sync/report server | `ham-sync-server` | `127.0.0.1:9740` | `GET /metrics` | `GET /ready` |
| Hosted API server | `ham-server` | `127.0.0.1:9750` | `GET /metrics` | `GET /ready` |

The exporter lives in `crates/ham-metrics`. It has no external dependencies: it
owns the registry, the Prometheus text encoder, the route-label matcher, and the
scrape-authorization policy that both binaries share.

Ready-made scrape configuration, alert rules, and Grafana dashboards are in
[`deploy/monitoring/`](../deploy/monitoring/README.md).

## Endpoints

### `GET /metrics`

Returns the Prometheus text exposition format (version `0.0.4`) with
`Content-Type: text/plain; version=0.0.4; charset=utf-8`.

Responses:

| Status | Meaning |
| --- | --- |
| `200` | Exposition body. |
| `401` | A scrape token is configured and the request did not present it. |
| `404` | The endpoint is disabled for this deployment. |

### `GET /health`

Unchanged liveness probe. It answers `200` as soon as the process is serving,
and says nothing about storage.

### `GET /ready`

Readiness probe. It answers `200` when durable storage is reachable and `503`
when it is not, with a JSON body naming each check:

```json
{
  "ready": true,
  "service": "ke8ygw-sync-server",
  "version": "0.4.0",
  "checks": {
    "metadata_store": true,
    "official_event_log_directory": true,
    "report_directory": true
  }
}
```

The hosted server reports `durable_metadata_store` and `official_event_store`
instead, the second of which is a real read against the official event store.

## Configuration

| Variable | Server | Default | Effect |
| --- | --- | --- | --- |
| `HAM_SYNC_METRICS_ENABLED` | `ham-sync-server` | enabled | Set to `0`, `false`, `off`, `no`, or `disabled` to make `/metrics` answer `404`. |
| `HAM_SYNC_METRICS_TOKEN` | `ham-sync-server` | unset | When set, `/metrics` requires `Authorization: Bearer <token>`. |
| `HAM_SERVER_METRICS_ENABLED` | `ham-server` | enabled | As above, for the hosted server. |
| `HAM_SERVER_METRICS_TOKEN` | `ham-server` | unset | As above, for the hosted server. |

Both servers print the resolved metrics policy at startup, including a warning
when the endpoint is served without a token.

## Privacy and safety rules

The exporter is bound by the same data rules as the rest of the platform:

- No callsigns, e-mail addresses, account or user identifiers, logbook or QSO
  identifiers, tokens, credentials, request bodies, or file paths are ever
  exported. Only aggregated counts and timing distributions are.
- Route labels come from the API route contract in `ham-api-contract`
  (`SELF_HOSTED_ROUTE_STRINGS` and `HOSTED_ROUTE_STRINGS`), so a request for
  `/api/v1/logbooks/<uuid>/head` is labelled
  `GET /api/v1/logbooks/:logbook_id/head`. Unknown paths collapse to
  `unmatched`, which keeps a scanner from inflating time series cardinality.
- Every metric family is capped at 512 label sets. Series past the cap are
  dropped and counted in `ham_metrics_series_dropped_total`, so a mislabelled
  call site degrades observability instead of exhausting server memory.
- A configured scrape token is compared in constant time and never echoed in a
  response body or a metric label.
- `/metrics` still exposes operational shape (traffic, error rates, tenant
  counts). Treat it as privileged: bind it to a trusted scrape network, set a
  token, or both.

## Metric catalogue

### Shared by both servers

Every series carries a `service` label (`ke8ygw-sync-server` or
`ke8ygw-ham-server`).

| Metric | Type | Labels | Meaning |
| --- | --- | --- | --- |
| `ham_http_requests_total` | counter | `method`, `route`, `status` | Requests served. |
| `ham_http_request_duration_seconds` | histogram | `method`, `route` | Request handling duration. |
| `ham_http_requests_in_flight` | gauge | - | Requests currently being served. |
| `ham_http_request_bytes_total` | counter | `method`, `route` | Request body bytes received. |
| `ham_http_response_bytes_total` | counter | `method`, `route` | Response body bytes written. |
| `ham_http_response_size_bytes` | histogram | `method`, `route` | Response body size distribution. |
| `ham_build_info` | gauge | `version`, `mode` | Always `1`; carries build and deployment identity. |
| `ham_process_start_time_seconds` | gauge | - | Unix timestamp of process start. |
| `ham_process_uptime_seconds` | gauge | - | Seconds since process start. |
| `ham_metrics_series` | gauge | - | Series held by the in-process registry. |
| `ham_metrics_series_dropped_total` | counter | - | Series dropped at the cardinality cap. |

`mode` is `hosted` or `self_hosted` for the sync server, and
`personal_hosted`, `public_hosted`, or `self_hosted` for the hosted server.

### Sync server only

| Metric | Type | Labels | Meaning |
| --- | --- | --- | --- |
| `ham_sync_pair_requests_total` | counter | `result` (`accepted`, `rejected`) | Device pairing attempts. |
| `ham_sync_events_pushed_total` | counter | `result` (`accepted`, `duplicate`, `rejected`) | Official events received by push. |
| `ham_sync_events_pulled_total` | counter | - | Official events returned by pull. |
| `ham_sync_replication_results_total` | counter | `operation` (`preview`, `pull`, `push`), `status` (`in_sync`, `remote_ahead`, `pulled`, `diverged`, `rejected`) | Replication outcomes. |
| `ham_sync_report_uploads_total` | counter | `status` | Diagnostic report bundles accepted. |
| `ham_sync_errors_total` | counter | `kind` (`invalid_request`, `missing_token`, `unauthenticated`, `unauthorized_logbook`, `validation`, `store`) | Request failures by cause. |
| `ham_sync_ready` | gauge | - | `1` when durable storage is reachable. Refreshed on every scrape. |
| `ham_sync_event_log_bytes` | gauge | - | Size of the durable official event log. |
| `ham_sync_metadata_store_bytes` | gauge | - | Bytes used by the durable metadata store. |
| `ham_sync_report_files` | gauge | - | Stored diagnostic report files. |
| `ham_sync_report_bytes` | gauge | - | Bytes of stored diagnostic report payloads. |

Storage gauges are sampled from the filesystem during the scrape, with a
bounded directory walk so a scrape can never traverse an unbounded tree.

### Hosted server only

| Metric | Type | Labels | Meaning |
| --- | --- | --- | --- |
| `ham_hosted_accounts` | gauge | - | Hosted user accounts. |
| `ham_hosted_logbooks` | gauge | - | Hosted logbooks. |
| `ham_hosted_logbook_memberships` | gauge | - | Membership grants. |
| `ham_hosted_server_admins` | gauge | - | Users holding the instance-admin role. |
| `ham_hosted_sessions` | gauge | `state` (`active`, `expired`, `inactive`) | Login sessions. |
| `ham_hosted_devices` | gauge | `state` (`active`, `revoked`) | Registered devices. |
| `ham_hosted_invitations` | gauge | `state` (`pending`, `accepted`, `revoked`, `expired`) | Server invitations. |
| `ham_hosted_api_tokens` | gauge | `state` (`active`, `revoked`) | Issued API tokens. |
| `ham_hosted_upload_jobs` | gauge | `status` | Upload jobs by status. |
| `ham_hosted_station_profiles` | gauge | - | Stored station profiles. |
| `ham_hosted_equipment_profiles` | gauge | - | Stored equipment profiles. |
| `ham_hosted_provider_settings` | gauge | - | Stored provider settings. |
| `ham_hosted_backups` | gauge | - | Stored backup records. |
| `ham_hosted_divergence_reports` | gauge | - | Stored divergence reports. |
| `ham_hosted_rate_limit_buckets` | gauge | - | Active rate-limit buckets. |
| `ham_hosted_audit_records` | gauge | - | Audit records retained. |
| `ham_hosted_audit_events_total` | counter | `action`, `outcome` | Audited hosted actions. |
| `ham_hosted_durable_metadata_store` | gauge | - | `1` when metadata is durable, `0` for the in-memory store. |
| `ham_hosted_ready` | gauge | - | `1` when hosted storage is reachable. Refreshed on every scrape. |

Hosted gauges are snapshots of server state taken during the scrape. Gauge
families with a state or status label are cleared and recomputed on each scrape,
so a label set that no longer exists stops being reported instead of sticking at
its last value.

`ham_hosted_audit_events_total` is seeded from the persisted audit log at
startup, so the totals survive a restart rather than resetting to zero.

## Useful queries

Request rate per route:

```promql
sum by (service, route) (rate(ham_http_requests_total[5m]))
```

Server-error ratio:

```promql
sum by (service) (rate(ham_http_requests_total{status=~"5.."}[5m]))
  /
clamp_min(sum by (service) (rate(ham_http_requests_total[5m])), 0.001)
```

p95 latency:

```promql
histogram_quantile(
  0.95,
  sum by (service, le) (rate(ham_http_request_duration_seconds_bucket[5m]))
)
```

Rejected pushes over the last hour:

```promql
sum(increase(ham_sync_events_pushed_total{result="rejected"}[1h]))
```

Failed audited hosted actions:

```promql
sum by (action, outcome) (
  rate(ham_hosted_audit_events_total{outcome!="succeeded"}[5m])
)
```

## Validation

```bash
cargo test -p ham-metrics
cargo test -p ham-sync-server
cargo test -p ham-server
```

A manual smoke check:

```bash
cargo run -p ham-sync-server --bin ham-sync-server
curl -s http://127.0.0.1:9740/metrics | head -40
curl -s http://127.0.0.1:9740/ready
```
