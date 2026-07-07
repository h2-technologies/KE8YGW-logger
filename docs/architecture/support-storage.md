# Durable Support Storage

Support storage holds local application state that is useful across restarts but
is not part of the official append-only log. It is intentionally separate from
official log events, runtime diagnostic logs, and credential secrets.

## Stored Data

The MVP uses versioned JSON support files under the app data support directory
for:

- service provider enablement, priority, and preferred-provider state
- service cache entries and safe metadata
- upload queue jobs and upload targets
- map layer ordering and visibility
- lookup and rig UI configuration
- online automation tasks and notification state

Station profiles, saved searches, permission grants, and credential metadata
already use dedicated JSON stores in the same support area.

## Format

Each generic support file uses this envelope:

```json
{
  "version": 1,
  "data": {}
}
```

Unknown versions are rejected so migrations can be explicit. Missing files load
as defaults. Corrupted files produce `support.storage.error` runtime events and
the GUI falls back to safe defaults for that session.

## Security

Support storage must never contain credentials, tokens, passwords, API keys, or
official QSO log data. Provider configuration stores `credential_id` references
only. Secret values must go through `CredentialStore`.

## Runtime Events

The GUI emits:

- `support.storage.opened`
- `support.storage.loaded`
- `support.storage.saved`
- `support.storage.error`

`support.storage.migration_applied` is reserved for the first schema migration.

## Current Limitations

- Support storage is JSON-file based for MVP simplicity.
- Writes are synchronous and small; SQLite can be introduced later if support
  state grows or concurrent writers are needed.
- Browser-local card layout preferences remain in local storage until a native
  desktop settings bridge is added.
