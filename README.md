# Ham Radio Operations Platform

This workspace is the first foundation for a local-first, plugin-based amateur
radio operations platform. The MVP direction is casual logging, POTA/SOTA, and
sync, with room for emergency communications, net control, and contesting.

## Workspace

- `ham-core`: append-only logbook events, event bus, proposal validation, event store, and projections.
- `ham-plugin-sdk`: public plugin manifest, capability, proposal, and event constant types.
- `ham-sync`: placeholder for future local-first synchronization.
- `ham-cli`: placeholder command-line entry point.

## Architecture

The official logbook is a global append-only event stream. Each official event
contains its own SHA-256 hash and the previous event hash for the same logbook,
similar to a Git commit chain. Deletes are tombstone events. The original events
remain in history, and read models decide how to display current state.

Plugins cannot write official events directly. A plugin submits a proposal such
as `proposal.qso.create`, `proposal.qso.correct`, or `proposal.qso.delete`.
`ham-core` validates the plugin manifest capability, the operator role, the
proposal event type, and the basic payload schema. Only then does the core append
an official event such as `official.log.qso.created`.

The event bus is a first-class typed interface. This initial implementation
provides an async in-memory bus that publishes official logbook events and
diagnostic runtime events. Runtime events are diagnostic only and are not part of
the official logbook stream.

Projections are derived state. The QSO current-state projection rebuilds from the
event stream and hides tombstoned QSOs while preserving the delete event in the
append-only history.

## Running Tests

```powershell
cargo test
```

The initial test suite verifies event hash chaining, valid and tampered chain
verification, tombstone projection behavior, plugin capability rejection, and
event bus publication for accepted proposals.

## Local Development Commands

This repository uses [`just`](https://github.com/casey/just) to keep local
commands aligned with CI.

```powershell
just fmt      # format all Rust code
just check    # cargo check for all workspace crates and targets
just clippy   # clippy for all workspace crates and targets, warnings are errors
just test     # run all workspace tests
just build    # debug build for all workspace crates
just release  # release build for all workspace crates
just ci       # formatting check, clippy, tests, and debug build
```

If `just` is not installed, the underlying commands are standard Cargo commands:

```powershell
cargo fmt --all
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
cargo build --release --workspace
```

## CI

GitHub Actions runs on pushes to `main` and on pull requests. The CI workflow
uses a Windows, Linux, and macOS matrix, caches Cargo dependencies, installs
`just`, and runs:

```powershell
just ci
```

CI fails on formatting drift, clippy warnings, test failures, or build failures.

## Release Builds

To build release binaries locally:

```powershell
just release
```

Tagged releases are automated from git tags matching `v*.*.*`, for example:

```powershell
git tag v0.1.0
git push origin v0.1.0
```

The release workflow builds `ham-cli` in release mode on:

- `ubuntu-latest`
- `windows-latest`
- `macos-latest`

It packages the binary as `ham-platform` and uploads archives to the GitHub
Release:

- `ham-platform-linux-x86_64.tar.gz`
- `ham-platform-windows-x86_64.zip`
- `ham-platform-macos-aarch64.tar.gz` or `ham-platform-macos-x86_64.tar.gz`,
  depending on the runner architecture
