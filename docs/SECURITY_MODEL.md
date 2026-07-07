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

## Authentication

MVP cloud sync and support upload use pairing-code/token sessions. This is intentionally temporary but keeps hosted and self-hosted modes usable.

Future work:

- stronger account authentication
- device pairing with explicit trust
- signed official events
- end-to-end encrypted relay
- organization-managed policies
- plugin signatures and sandboxing

## Current Limitations

- Plugin loading is static and not sandboxed.
- Grant scopes are recorded but not fully enforced across every subsystem.
- The GUI assumes a local-admin posture for permission review.
- Cloud server storage is in-memory in MVP.
- LAN peers are not yet authenticated.
