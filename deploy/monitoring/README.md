# Monitoring Stack

Prometheus scrape configuration, alert rules, and Grafana dashboards for the
two KE8YGW Logger server binaries.

| File | Purpose |
| --- | --- |
| `docker-compose.monitoring.yml` | Local Prometheus + Grafana stack, pre-provisioned. |
| `prometheus/prometheus.yml` | Scrape jobs for `ham-sync-server` and `ham-server`. |
| `prometheus/alerts.yml` | Availability, request-health, replication, and cardinality alerts. |
| `grafana/provisioning/datasources/prometheus.yml` | Grafana data source provisioning. |
| `grafana/provisioning/dashboards/dashboards.yml` | Dashboard provider provisioning. |
| `grafana/dashboards/ke8ygw-servers-overview.json` | Request rate, errors, latency, process health for both servers. |
| `grafana/dashboards/ke8ygw-sync-server.json` | Replication, pairing, reports, durable storage. |
| `grafana/dashboards/ke8ygw-hosted-server.json` | Accounts, sessions, devices, invitations, audits, uploads. |

## Quick start

Start one or both servers:

```bash
cargo run -p ham-sync-server --bin ham-sync-server
cargo run -p ham-server --bin ham-server
```

Then start the monitoring stack:

```bash
docker compose -f deploy/monitoring/docker-compose.monitoring.yml up -d
```

Open Grafana at <http://127.0.0.1:3000> and look in the **KE8YGW Logger**
folder. The three dashboards are provisioned automatically and refresh every
30 seconds.

## Scrape authentication

Both servers serve `/metrics` without authentication by default, which is only
safe when the port is reachable from the scrape network alone. Set a token to
require one:

```bash
export HAM_SYNC_METRICS_TOKEN=$(openssl rand -hex 32)
export HAM_SERVER_METRICS_TOKEN=$(openssl rand -hex 32)
```

Write each token to a file Prometheus can read (never into this repository) and
uncomment the matching `authorization` block in `prometheus/prometheus.yml`:

```yaml
    authorization:
      type: Bearer
      credentials_file: /etc/prometheus/secrets/sync-metrics-token
```

Set `HAM_SYNC_METRICS_ENABLED=0` or `HAM_SERVER_METRICS_ENABLED=0` to turn the
endpoint off entirely; it then answers `404`.

## Metric contract

[`docs/OBSERVABILITY.md`](../../docs/OBSERVABILITY.md) documents every exported
metric, its labels, and the privacy rules the exporter follows.
