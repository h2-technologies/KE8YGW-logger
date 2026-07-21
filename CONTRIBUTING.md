# Contributing

KE8YGW Logger is an owner-led open-source project. Contributions are welcome when
they fit the current architecture, preserve operator data, and keep security
boundaries explicit.

By contributing, you agree that your contribution is licensed under the
repository's MIT license. This project does not require a contributor license
agreement.

## Architecture Overview

This repository is a Rust workspace. `ham-core` owns business logic, official
event validation, projections, permissions, provider metadata, credential store
abstractions, and sync-safe data models. Plugins and UI code submit proposals;
they do not write official logbook events directly.

The current workspace includes:

- `crates/ham-core`: append-only official events, projections, services,
  credentials, diagnostics, provider boundaries, and domain rules.
- `crates/ham-plugin-sdk`: public plugin manifest, permissions, proposal, and
  event constants.
- `crates/ham-sync` and `crates/ham-sync-server`: local-first sync protocol and
  self-hosted relay/server foundations.
- `crates/ham-server`: hosted and self-hosted API boundary, including `/api/v1`.
- `crates/ham-gui`: Rust GUI shell server and static web UI.
- `crates/ham-desktop` and `src-tauri`: desktop/Tauri integration.
- `docs/adr`: accepted architecture decision records.

Read [docs/MASTER_BLUEPRINT.md](docs/MASTER_BLUEPRINT.md),
[docs/DEVELOPER_GUIDE.md](docs/DEVELOPER_GUIDE.md), and the ADR index before
changing shared architecture.

## Toolchain

Use the stable Rust toolchain with the workspace's Rust 2021 edition. The CI
workflow installs stable Rust with `rustfmt` and `clippy`, then runs
change-aware Rust, API, version, documentation, governance, platform, Tauri,
container, iOS, and security checks on the appropriate runners.

Package managers and tools used by the repository today:

- Cargo for Rust workspace builds, tests, formatting, and linting.
- `just` for command aliases aligned with CI.
- Node for static JavaScript syntax checks in `crates/ham-gui/web/app.js`.
- Cargo Tauri commands for desktop/Tauri checks when desktop code is touched.

## Local Setup

```powershell
git clone https://github.com/h2-technologies/KE8YGW-logger.git
cd KE8YGW-logger
cargo check --workspace --all-targets
```

If `just` is installed, prefer the recipes in the root `justfile`.

## Required Local Checks

Run the checks that match the files you changed. The baseline repository check
is:

```powershell
just ci
```

The underlying commands are:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
python scripts/check_api_contract.py
python scripts/check_versions.py
python scripts/check_docs_links.py
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/governance-check.ps1
```

For Rust formatting changes, run:

```powershell
just fmt
```

For JavaScript or frontend changes, run:

```powershell
node --check crates\ham-gui\web\app.js
```

For desktop/Tauri changes, run the relevant Tauri checks documented in
[docs/DEVELOPER_GUIDE.md](docs/DEVELOPER_GUIDE.md), such as:

```powershell
cargo tauri info
cargo tauri build
```

For documentation and governance changes, run:

```powershell
just docs-link-check
just governance-check
```

## Branches And Commits

Use descriptive branches:

- `feature/<short-description>` for new user-facing capabilities.
- `fix/<short-description>` for bug fixes.
- `docs/<short-description>` for documentation-only changes.
- `security/<short-description>` for coordinated private security work.
- `codex/<issue-or-task>` for Codex-authored issue work.

Keep commits focused. Commit messages should describe the repository area and
behavioral intent, for example `docs: add repository governance standards`.
Avoid mixing unrelated runtime, documentation, and release-policy changes in one
commit unless the issue explicitly calls for that scope.

## Pull Requests

Every pull request must use the repository PR template and include:

- A linked issue, using `Closes #<number>` when the PR fully resolves it.
- A summary of the change and explicit scope.
- Architecture, API, data, migration, security, and privacy impact.
- Tests and validation commands actually run.
- Documentation updates or an explanation for why none were needed.
- Rollback or recovery considerations.
- Confirmation that no credentials, production data, or user-specific files are
  included.

Small documentation-only PRs may mark non-applicable sections as `N/A`, but they
must still explain test or validation coverage.

## Architecture Decisions

Add or update an ADR when a change establishes, reverses, or materially changes:

- official event semantics
- sync behavior
- credential handling
- provider or plugin boundaries
- `/api/v1` compatibility
- storage, migration, or rollback expectations
- desktop, hosted, self-hosted, or native-client architecture

Use [docs/adr/0000-template.md](docs/adr/0000-template.md). Do not add ADRs for
unmade decisions or speculative policy.

## API Compatibility

The `/api/v1` surface is compatibility-sensitive. PRs that change request or
response shapes, authentication, authorization, error codes, pagination,
provider settings, sync endpoints, or backup/diagnostic formats must document
compatibility impact and migration guidance.

Breaking changes require maintainer approval and should preserve compatibility
through additive fields, versioned routes, migration helpers, or clear release
notes whenever practical.

## Database, Event, And Migration Rules

Official log events are append-only and hash chained. Do not edit or delete
official events in normal workflows. Corrections, deletes, restores, upload
status changes, activation changes, and Net Control changes must append new
official events through core validation.

Storage or schema changes must document:

- forward migration behavior
- rollback or recovery behavior
- compatibility with existing official events
- projection rebuild behavior
- test coverage for old and new data

Provider settings, support state, service caches, diagnostics, and credential
metadata are not substitutes for official log history.

## Security-Sensitive Changes

Request maintainer review for changes involving:

- authentication, sessions, account boundaries, or authorization
- provider credentials, desktop credential stores, LoTW certificates, signing
  keys, or upload transports
- backups, diagnostic bundles, sync authorization, reports, and redaction
- release signing, auto-update metadata, or installer packaging
- user identity, station profile, QSO, provider, or logbook exposure

Do not publish vulnerability details in public issues or PRs before maintainers
have had a chance to coordinate remediation. Follow [SECURITY.md](SECURITY.md).

## Generated Files

Do not commit generated build output, local caches, logs, archives, or installer
artifacts. Generated source files that are already tracked, such as Tauri schema
files under `src-tauri/gen`, may be updated only when the corresponding source
change requires it and the generation command is documented in the PR.

## Secrets And Local Files

Never commit credentials, API keys, tokens, LoTW certificate material, signing
keys, production data, personal logs, diagnostic bundles, local `.env` files,
database files, user-specific paths, or machine-specific configuration. Redact
logs before attaching them to issues or PRs.
