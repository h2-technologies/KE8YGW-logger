# Security Model

Security is centered on centralized authorization, append-only official data, and separation between official history and runtime diagnostics.

## Authorization Rule

Every protected action requires all three checks:

```text
plugin_has_required_permission
AND operator_role_allows_permission
AND scope_allows_target_account_logbook_or_station
```

Denied actions must not append official events. High-risk denials should publish runtime audit events with a correlation ID.

## Permission Risk Levels

- `low` - read-only or UI-only actions with low data exposure.
- `medium` - actions that expose operational data or write support cache.
- `high` - actions that affect official logs, sync, uploads, network calls, or rig control.
- `critical` - future actions with destructive, privileged, account-wide, or safety-sensitive impact.

## Plugin vs Operator Permissions

Plugin permissions and operator role permissions are independent:

- A granted plugin cannot act if the operator role lacks permission.
- A powerful operator cannot use a plugin for an action the plugin did not request and receive.
- UI panel registration does not imply data access.
- External network lookup is separate from local lookup.
- Diagnostics upload is separate from diagnostics export.
- Rig read is separate from rig write and PTT.
- Sync pull and sync push are separate.
- Service provider registration/configuration/enablement is separate from
  provider data access.
- Service cache clear is separate from service cache read/write.
- Upload, spotting, map, weather, and propagation providers use separate
  permissions.
- Credential metadata, credential use, credential update, credential delete, and
  credential testing are separate permissions.
- Net Control permissions are separate for viewing, templates, sessions,
  check-ins, traffic, and report export.

## Official Log Protection

Official events are append-only and hash chained. Corrections, deletes, restores, notes, activation links, imports, and synced events all append official events rather than mutating prior records.

The server or sync peer may add relay metadata outside the hash input, but it must not rewrite official event metadata or payload.

## Runtime Diagnostics

Runtime events are diagnostic only. They are persisted to rotating JSONL logs and may be included in diagnostic bundles after redaction.

Runtime logs must not contain:

- credentials
- API keys
- passwords
- session tokens
- sync tokens
- full official logs by default
- full AI prompts/responses by default
- raw provider metadata that may contain secrets

## Server Metrics

The `/metrics` endpoint on `ham-server` and `ham-sync-server` exports aggregated
counts and timing distributions only. Metric names, label names, and label
values must never contain:

- callsigns or operator names
- e-mail addresses
- account, user, device, session, logbook, QSO, or report identifiers
- session tokens, sync tokens, API tokens, or scrape tokens
- credentials of any kind
- request or response bodies
- filesystem paths

Route labels are taken from the route catalogs in `ham-api-contract`, so a path
carrying an identifier is reported as its route pattern
(`GET /api/v1/logbooks/:logbook_id/head`) and an unrecognized path collapses to
`unmatched`. Every metric family is capped at 512 label sets so a mislabelled
call site cannot exhaust server memory.

The endpoint still exposes operational shape: traffic volumes, error rates,
tenant counts, and storage size. Treat it as privileged. It is enabled and
unauthenticated by default for local development; a deployment that cannot
restrict the scrape port to a trusted network must set
`HAM_SYNC_METRICS_TOKEN` or `HAM_SERVER_METRICS_TOKEN`, which requires
`Authorization: Bearer <token>` on every scrape. The configured token is
compared in constant time and is never echoed in a response or a label.
Setting `HAM_SYNC_METRICS_ENABLED=0` or `HAM_SERVER_METRICS_ENABLED=0` removes
the endpoint entirely.

## Credential Storage

Provider credentials are support/security state and must never be stored in
official log events. Provider configuration should reference `credential_id`
values. Secret values are retrieved only through `CredentialStore` after plugin
permission and operator role checks pass.

The current implementation includes native OS credential backends for Windows Credential Manager, macOS Keychain, and Linux Secret Service/libsecret tooling, plus an explicit opt-in insecure development fallback. Production online integrations must continue to use native OS credential backends for real provider secrets.

Hosted account session and refresh tokens follow the same rule. The shared
`ham_sync::account` client returns an issued token exactly once, and the
durable hosted account record stores only the credential identifier for it.
Desktop, hosted web, and the CLI write the secret through `CredentialStore`
under the `hosted-account` provider; native iOS writes it to the Keychain under
the same Rust-assigned identifier. The account record, GUI JSON responses, CLI
output, and runtime events are token-free, and a hosted authentication failure
clears both the stored identifiers and the stored secrets.

## Net Control Safety

Net Control is an official append-only workflow. Sessions, check-ins, traffic,
and report exports are written through proposals. Deleted check-ins are
tombstone events and are hidden by projections by default.

## Authentication

Hosted `/api/v1` account authentication uses explicit registration modes,
verified email, bearer sessions, and secure session cookies. The first server
administrator is created only through one-time bootstrap. Registration is
invite-only by default; public open registration is administrator-enabled and
fails closed behind Cloudflare Turnstile when configured. Raw session, refresh,
invite, email-verification, recovery, and API tokens are returned only at
creation/consumption time and are persisted by hash in hosted SurrealDB
metadata.

Self-hosted sync and support upload routes still use pairing-code/token
sessions for compatibility-only sync/report flows.

Client account flows go through the shared hosted account contract. Rust plans
each hosted request, bounds and normalizes the base URL, email address, device
label, and token inputs, and rejects a session-scoped action when no session
credential is stored. Outcome classification reads the stable hosted `code`
field before the HTTP status, so a revoked or expired session, an unverified
email, a closed registration, a replayed token, and a failed Turnstile check
are distinguishable without parsing human-readable messages.

GUI LAN sync read endpoints for logbook lists, heads, event ranges, and event
metadata require trusted-device, replay-nonce, signature-version, and
HMAC-SHA256 signature headers. The serving peer verifies those headers against a
stored endpoint-auth credential created during pairing or rotation through `CredentialStore`, durable LAN trust
records, logbook scope, revocation state, and replay history before returning
logbook or event data. LAN trust JSON stores only credential references, not raw
pairing codes. The GUI LAN auth-rotation endpoint stores the replacement secret
through `CredentialStore`, updates the trust record to the new credential ID,
and deletes the previous credential reference after the trust update succeeds.
The discovery identity endpoint remains unauthenticated and must stay
secret-free.

Future work:

- Apple Developer account approval/provisioning and release-device iOS LAN
  validation for the declared multicast entitlement on top of the durable LAN
  trust store
- formal asymmetric LAN key exchange beyond the current distinct endpoint-auth
  code plus HMAC request-proof model
- signed official events
- end-to-end encrypted relay
- organization-managed policies
- plugin signatures and sandboxing

## Current Limitations

- Plugin loading is static and not sandboxed.
- Grant scopes are recorded but not fully enforced across every subsystem.
- The GUI assumes a local-admin posture for permission review.
- The self-hosted sync/report server now uses durable local storage by default; production migration, retention, and hosted-operations hardening still remain.
- LAN sync writes are trust-gated and protected LAN reads require HMAC-SHA256 request proof after pairing, but the LAN HTTP transport is not encrypted and must stay on trusted local networks.
- Native OS credential backends are implemented, but clean release-runner and packaged-app validation still remain.
- The metrics scrape endpoint is unauthenticated unless a scrape token is configured, and it is served over the same plain HTTP listener as the rest of the server; production deployments must terminate TLS in front of it and restrict the scrape network.
- Net Control template UI and ICS-style exports are not complete.
