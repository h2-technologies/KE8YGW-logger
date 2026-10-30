# Contributing

KE8YGW Logger is an owner-led open-source project. Contributions are welcome when
they fit the v1 design, preserve operators' logs, and keep security boundaries
explicit.

By contributing, you agree that your contribution is licensed under the
repository's MIT license. This project does not require a contributor license
agreement.

## Start Here

Read [the design spec](docs/V1_DESIGN_SPEC.md) before proposing changes. It is
the master copy of the architecture and every locked decision.
[PROJECT_STATE.md](PROJECT_STATE.md) says which milestone is in progress. v1
is scoped to Winter Field Day 2027; anything in the spec's non-goals waits
until after the event.

## Workspace

The workspace is 18 crates under `crates/`, listed in section 3 of the spec and
in [AGENTS.md](AGENTS.md). The allowed dependencies between them are enforced by
`crates/ke8ygw-testkit/tests/layering.rs`. A pull request that needs a new edge
is an architecture change: update the spec and that table together.

## Local Validation

Rust 1.96 is pinned in `rust-toolchain.toml`. Before opening a pull request:

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
git diff --check
```

`just ci` runs the same set plus the Raspberry Pi target check. CI runs all of
it on Linux, Windows and macOS.

Ordinary tests must not need network access, a real radio or a real N3FJP
install. Use the fakes in `ke8ygw-testkit`.

## Branches And Pull Requests

Branch from `dev` and open pull requests into `dev`. `main` is updated only by
promotion from `dev`; see
[docs/BRANCHING_AND_RELEASE_CHANNELS.md](docs/BRANCHING_AND_RELEASE_CHANNELS.md).

Every pull request uses the repository PR template and includes:

- a linked issue, using `Closes #<number>` when the PR fully resolves it
- a summary and explicit scope
- architecture, data and security impact
- the validation commands actually run
- documentation updates, or why none were needed
- confirmation that no credentials, keys, PINs or real logs are included

Keep commits focused, and do not mix formatting sweeps or dependency updates
into functional changes.

## The Log Is Append-Only

Events are immutable and signed, and each station appends only to its own
hash-chained log. Never edit or delete an existing event. Corrections,
tombstones and restores are new events, created by the core from validated
proposals.

Changes to event types, JSON fields, contest definition keys, the sync
protocol or the Node API are compatibility changes. Document the migration and
add tests that read old data.

## Security-Sensitive Changes

Request maintainer review for changes involving station keys, signing, operator
PINs, roles and the permission matrix, join tokens, sync transport and
pinning, the relay, network setup helpers, installer packaging and signing, or
export correctness. Follow [SECURITY.md](SECURITY.md) and do not publish
vulnerability details before maintainers have coordinated a fix.

## Secrets And Local Files

Never commit credentials, station keys, PINs or PIN verifiers, join tokens,
signing certificates, real club logs, local `.env` files, database files,
user-specific paths or build output. Redact logs before attaching them to
issues or pull requests.
