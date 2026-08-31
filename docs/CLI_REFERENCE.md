# CLI command reference

`ham-cli` is an offline-first interface to the same append-only event store and
ADIF implementation used by the Rust core. The CLI package version is `0.3.0`.
It never prompts. The logging, ADIF, and integrity commands do not contact a
provider or require a server; the `account` commands contact the hosted server
that has been configured for this machine.

## Global behavior

```text
ham-cli help
ham-cli version [--json]
```

`--json` may appear before or after the command and writes one JSON object to
standard output. Exit code `0` means success, `1` means an operational or data
error, and `2` means invalid usage.
Paths and parse failures are written to standard error in human mode. No
command prints provider credentials or tokens.

## ADIF

```text
ham-cli import-adif contacts.adi [--json]
ham-cli export-adif contacts.adi [--json]
```

Import uses the shared proposal validator and official append-only event store.
Export rebuilds the current projection before writing. Keep an independent
backup before importing untrusted or large files. Dry-run import is not yet
implemented and remains a CLI v1 release blocker.

## Integrity and projections

```text
ham-cli verify-chain [--json]
ham-cli rebuild-projections [--json]
```

`verify-chain` verifies the configured logbook's hash chain without mutation.
`rebuild-projections` replays official events and reports the visible QSO count.

## Hosted account and session

```text
ham-cli account status [--json]
ham-cli account configure <server-url> [device-name] [--json]
ham-cli account register <email> [display-name] [invitation-token] [--json]
ham-cli account verify-email <token> [--json]
ham-cli account recovery-start <email> [--json]
ham-cli account recovery-complete <token> [--json]
ham-cli account login <email> [display-name] [--json]
ham-cli account session [--json]
ham-cli account rotate [--json]
ham-cli account logout [--json]
ham-cli account logout-all [--json]
ham-cli account devices [--json]
ham-cli account revoke-device <device-id> [--json]
ham-cli account revoke-all-devices [--json]
ham-cli account delete --confirm [--json]
```

`account status` and `account configure` are local: they read and write the
durable hosted account record without contacting a server. Every other
subcommand runs the shared `ham-sync` hosted account contract, which plans the
hosted `/api/v1` request in Rust, sends it, and classifies the response.

Session and refresh tokens are written to the operating-system credential
backend under the `hosted-account` provider and are never printed, logged, or
written to the account record; the record stores only credential identifiers.
`ham-cli account status` reports the stored connection state, server URL,
device label, and account email.

Exit code `0` means the hosted server accepted the request. Exit code `1` means
the request was rejected, could not be planned (for example, a session command
with no stored session), or failed in transport; the `--json` output reports
`outcome`, `retryable`, `user_action_required`, `error_code`, and `request_id`
so scripts can distinguish a retryable failure from one that needs the
operator. `account rotate` requires a stored refresh token, and
`account delete` requires the explicit `--confirm` flag.

## Current limitations

Initialization/configuration, logbook management, individual QSO CRUD/search,
station/operator/equipment CRUD, backup inspection/restore, sync and conflict
commands, hosted server administration (hosting mode, invitations, audits),
provider diagnostics, diagnostic bundles, completions, and an ADIF dry-run are
not implemented in this pass. The CLI must not be described as
feature-complete until those commands and their integration tests exist.
