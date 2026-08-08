# CLI command reference

`ham-cli` is an offline-first interface to the same append-only event store and
ADIF implementation used by the Rust core. The CLI package version is `0.3.0`.
It does not prompt, contact a provider, or require a server for the commands
currently implemented.

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

## Current limitations

Initialization/configuration, logbook management, individual QSO CRUD/search,
station/operator/equipment CRUD, backup inspection/restore, sync and conflict
commands, provider diagnostics, diagnostic bundles, completions, and an ADIF
dry-run are not implemented in this pass. The CLI must not be described as
feature-complete until those commands and their integration tests exist.
