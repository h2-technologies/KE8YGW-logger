# CLI command reference

`ham-client` is an offline-first interface to the same append-only event store
and ADIF implementation used by the Rust core. Its `serve` subcommand runs the
local web UI; every other subcommand documented here is a one-shot command-line
operation. The client package version is `0.5.1`.
It never prompts. The logging, ADIF, and integrity commands do not contact a
provider or require a server; the `account` and `admin` commands contact the
hosted server
that has been configured for this machine.

## Global behavior

```text
ham-client help
ham-client version [--json]
ham-client serve [bind-address]
```

`serve` starts the local web UI server (default `127.0.0.1:9467`) and runs
until interrupted; it is documented in the README rather than here.

`--json` may appear before or after the command and writes one JSON object to
standard output. Exit code `0` means success, `1` means an operational or data
error, and `2` means invalid usage.
Paths and parse failures are written to standard error in human mode. No
command prints provider credentials or tokens.

## ADIF

```text
ham-client import-adif contacts.adi [--json]
ham-client export-adif contacts.adi [--json]
```

Import uses the shared proposal validator and official append-only event store.
Export rebuilds the current projection before writing. Keep an independent
backup before importing untrusted or large files. Dry-run import is not yet
implemented and remains a CLI v1 release blocker.

## Integrity and projections

```text
ham-client verify-chain [--json]
ham-client rebuild-projections [--json]
```

`verify-chain` verifies the configured logbook's hash chain without mutation.
`rebuild-projections` replays official events and reports the visible QSO count.

## Hosted account and session

```text
ham-client account status [--json]
ham-client account configure <server-url> [device-name] [--json]
ham-client account register <email> [display-name] [invitation-token] [--json]
ham-client account verify-email <token> [--json]
ham-client account recovery-start <email> [--json]
ham-client account recovery-complete <token> [--json]
ham-client account bootstrap <email> [display-name] [--json]
ham-client account login <email> [display-name] [--json]
ham-client account session [--json]
ham-client account rotate [--json]
ham-client account logout [--json]
ham-client account logout-all [--json]
ham-client account devices [--json]
ham-client account revoke-device <device-id> [--json]
ham-client account revoke-all-devices [--json]
ham-client account delete --confirm [--json]
```

`account status` and `account configure` are local: they read and write the
durable hosted account record without contacting a server. Every other
subcommand runs the shared `ham_core::sync` hosted account contract, which plans the
hosted `/api/v1` request in Rust, sends it, and classifies the response.

Session and refresh tokens are written to the operating-system credential
backend under the `hosted-account` provider and are never printed, logged, or
written to the account record; the record stores only credential identifiers.
`ham-client account status` reports the stored connection state, server URL,
device label, and account email.

Exit code `0` means the hosted server accepted the request. Exit code `1` means
the request was rejected, could not be planned (for example, a session command
with no stored session), or failed in transport; the `--json` output reports
`outcome`, `retryable`, `user_action_required`, `error_code`, and `request_id`
so scripts can distinguish a retryable failure from one that needs the
operator. `account rotate` requires a stored refresh token, and
`account delete` requires the explicit `--confirm` flag.

`account bootstrap` claims the one-time instance-administrator bootstrap on a
fresh server. The hosted route refuses once any account exists, so it succeeds
exactly once per server and issues a normal session for the new administrator.

## Hosted server administration

```text
ham-client admin status [--json]
ham-client admin hosting [--json]
ham-client admin set-hosting <field> <value> [--json]
ham-client admin invitations [--json]
ham-client admin invite <logbook-id> <email> <role> [--json]
ham-client admin invitation <invite-id> [--json]
ham-client admin resend <invite-id> [--json]
ham-client admin expire <invite-id> [--json]
ham-client admin revoke <invite-id> --confirm [--json]
ham-client admin audits [--json]
```

The `admin` commands administer the server the `account` commands are signed in
to. There is no separate endpoint setting: the server URL and the session
credential both come from the hosted account record, so an operator can only
administer the server they are signed in to. Every subcommand except
`admin status` needs a signed-in session belonging to a server administrator.

`admin status` is local: it reports the cached administrator rights, server URL,
invitation and audit counts without contacting a server. Rights are reported as
`administrator`, `not-an-administrator`, or `unchecked` when no administration
call has been made yet.

`set-hosting` takes one field per call: `operation_mode`, `registration_mode`,
`session_ttl_seconds`, `refresh_ttl_seconds`, `invitation_ttl_seconds`,
`verification_ttl_seconds`, or `recovery_ttl_seconds`. Fields you do not name
are left exactly as the server has them, so an update never rewrites a hosting
value you did not look at. Unknown modes and non-positive lifetimes are rejected
with exit code `2` before a request is sent.

`invite` and `resend` print a single-use invitation token on one
`invitation_token=` line. That token is returned by the hosted server exactly
once, is never written to the durable record or the credential backend, and does
not appear in `--json` output beyond the same single field. The hosted server
also emails it to the invitee; capture it from that one output only if you are
delivering it yourself.

Exit codes match the `account` commands: `0` accepted, `1` rejected or failed in
transport, `2` invalid usage. `admin revoke` requires the explicit `--confirm`
flag.

## Current limitations

Initialization/configuration, logbook management, individual QSO CRUD/search,
station/operator/equipment CRUD, backup inspection/restore, sync and conflict
commands, provider diagnostics, diagnostic bundles, completions, and an ADIF
dry-run are not implemented in this pass. Hosted server administration covers
hosting mode, invitations, and audit review; editing the hosted email and
Turnstile configuration is deliberately not exposed, because those blocks carry
secrets and belong with the operations work. The CLI must not be described as
feature-complete until those commands and their integration tests exist.
