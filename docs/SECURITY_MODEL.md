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

## Credential Storage

Provider credentials are support/security state and must never be stored in
official log events. Provider configuration should reference `credential_id`
values. Secret values are retrieved only through `CredentialStore` after plugin
permission and operator role checks pass.

The current implementation includes native OS credential backends for Windows Credential Manager, macOS Keychain, and Linux Secret Service/libsecret tooling, plus an explicit opt-in insecure development fallback. Production online integrations must continue to use native OS credential backends for real provider secrets.

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

GUI LAN sync read endpoints for logbook lists, heads, event ranges, and event
metadata require trusted-device, replay-nonce, signature-version, and
HMAC-SHA256 signature headers. The serving peer verifies those headers against a
pairing-derived credential stored through `CredentialStore`, durable LAN trust
records, logbook scope, revocation state, and replay history before returning
logbook or event data. LAN trust JSON stores only credential references, not raw
pairing codes. The GUI LAN auth-rotation endpoint stores the replacement secret
through `CredentialStore`, updates the trust record to the new credential ID,
and deletes the previous credential reference after the trust update succeeds.
The discovery identity endpoint remains unauthenticated and must stay
secret-free.

Future work:

- production reciprocal pairing UX on top of the durable LAN trust store
- stronger LAN key-exchange hardening
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
- Net Control template UI and ICS-style exports are not complete.
