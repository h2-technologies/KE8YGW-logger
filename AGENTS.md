# AGENTS.md

Instructions for Claude Code, Codex and any other AI agent working in this
repository. Read this file and `docs/V1_DESIGN_SPEC.md` before editing.

## 1. Authority

1. Direct user instructions.
2. This file.
3. `docs/V1_DESIGN_SPEC.md`, the master design spec. Where it says
   "decision", build it that way.
4. Current code and tests.
5. `PROJECT_STATE.md`.

Rules:
- Treat the spec's decisions and invariants as binding unless the user asks
  for an architecture change. An architecture change updates the spec in the
  same pull request.
- Inspect the code, tests and spec before editing. Do not work from memory or
  from old summaries.
- The v0.5.1 codebase (hosted server, iOS, Tauri client) was retired on
  Oct 30, 2026. It lives on the `archive/v0` branch. Do not treat its docs as
  current. Its code may be ported when it fits the v1 design, after review and
  with new tests.
- Update this file when repository structure, workflow or validation changes.

## 2. Project

KE8YGW Logger v1 is a peer-to-peer, offline-first logger that lets 2 to 10
stations at one Winter Field Day site log into one merged, scored, exportable
log. It must be on the air for **Winter Field Day, Jan 23-24, 2027**.
Single-operator use works as a one-node event.

v1 scope: signed event log, sync, the WFD 2027 engine, Cabrillo/ADIF export,
N3FJP (Winter Field Day Contest Log) and FLRig connectors, a desktop GUI
(Tauri 2 + Yew) and a minimal TUI. Non-goals for v1 are listed in section 1 of
the spec. Milestones and their status are in `PROJECT_STATE.md`.

## 3. Repository map

| Path | Purpose |
| --- | --- |
| `crates/ke8ygw-types` | Ids, event envelope, QSO and HLC types; no I/O |
| `crates/ke8ygw-hlc` | Hybrid logical clock and time sources (NTP, GPS, master offset) |
| `crates/ke8ygw-identity` | Ed25519 keys, signing, operator accounts, admin/checker PIN verifiers |
| `crates/ke8ygw-eventlog` | Per-station JSONL chains: append + fsync, verify, repair, DAG heads |
| `crates/ke8ygw-merge` | Merge, tombstones, conflicts, role checks |
| `crates/ke8ygw-sync` | mDNS, TLS transport, gossip, join flow, relay client |
| `crates/ke8ygw-netsetup` | Per-OS hotspot/ad-hoc helper; the only crate that may use `unsafe` |
| `crates/ke8ygw-projector` | Replays events into embedded SurrealDB (disposable projection) |
| `crates/ke8ygw-contest` | Definition loader, rule kinds, WFD 2027 engine, section list |
| `crates/ke8ygw-export` | Cabrillo, ADIF, submission summary, preflight |
| `crates/ke8ygw-proposal` | Proposal types shared by connectors and core; no runtime dependencies |
| `crates/ke8ygw-connector-n3fjp`, `crates/ke8ygw-connector-flrig` | First-party connectors |
| `crates/ke8ygw-node` | Wires everything together, serves the Node API; `ke8ygw-node` headless binary |
| `crates/ke8ygw-gui` | `ke8ygw` binary: desktop GUI and CLI subcommands |
| `crates/ke8ygw-tui` | `ke8ygw-tui` binary |
| `crates/ke8ygw-relay` | `ke8ygw-relay` binary |
| `crates/ke8ygw-testkit` | Simulated network, fake N3FJP/FLRig servers, fixtures, layering test |
| `docs/V1_DESIGN_SPEC.md` | Master design spec |
| `docs/BRANCHING_AND_RELEASE_CHANNELS.md` | Branch and release channel policy |
| `.github/workflows/` | CI, security scanning, Scorecard, promotion policy |
| `scripts/check-promotion-policy.sh` | Shared check for pull requests into `main` |

Allowed dependencies between crates are written down once, in
`crates/ke8ygw-testkit/tests/layering.rs`. That test fails on any edge the spec
does not allow. Adding an edge is an architecture change.

## 4. Invariants (from the spec)

- **Single writer.** Each station runs one node, and only the node's core
  appends to the log. Screens send requests; connectors send proposals; the
  core validates, checks roles, signs and appends. AI features may only propose.
- **Source of truth.** Per-station hash-chained JSONL logs, merged as a DAG,
  are the only authoritative data. Events are immutable and signed. Never edit
  or rewrite an existing event; corrections, tombstones and restores are new
  events.
- **Durability.** A QSO is shown as logged only after its line is fsynced.
- **Derived, never stored.** Dupe, out-of-window, not-creditable and
  unauthorized flags are computed by the projector, never written into events.
- **Disposable projection.** SurrealDB can be deleted and rebuilt from the logs
  at any time. UIs never treat it as authoritative and never poll it.
- **Roles at merge time.** Every node enforces the role matrix using the role
  in force at the event's HLC. Violating events are kept, marked
  `unauthorized`, and left out of projections, scores and exports.
- **Warnings, not blockers.** WFD rule checks at entry warn; an operator can
  always log what actually happened. Export preflight is the one place that
  blocks.
- **Contest rules are data.** Each rules year is its own versioned TOML file
  plus named Rust rule kinds. Do not hard-code a year's rules in code paths.
- **Network input never panics.** Return an error and keep the peer at arm's
  length.
- **No `unsafe`** outside the OS-specific modules of `ke8ygw-netsetup`.
  Workspace lints enforce this.
- **Offline first.** Everything works on a LAN with no Internet. The relay is
  optional and carries the same signed events.
- **Serialized formats are compatibility surface.** Renaming an event type,
  JSON field, TOML key or Node API method is a migration, not a cleanup.

## 5. Conventions

- Rust 1.96 (pinned in `rust-toolchain.toml`), edition 2024.
- Tokio for async, `thiserror` for errors, `tracing` for logging.
- Every crate builds and tests on Linux, Windows and macOS; the workspace also
  checks for `aarch64-unknown-linux-gnu` (Raspberry Pi).
- Ordinary tests never need the network, real radios or real N3FJP. Use the
  fakes in `ke8ygw-testkit`; hardware checks are manual and listed in the spec.
- No secrets, keys, PINs or real club logs in the repository, tests or fixtures.

## 6. Validation

Run before pushing (the `justfile` wraps these):

```sh
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
cargo check --locked --workspace --all-targets --target aarch64-unknown-linux-gnu
git diff --check
```

Never claim a command passed unless you ran it. If a command cannot run, say
which one, why, and what remains unverified.

## 7. Documentation

Update docs in the same pull request as the change:

| Change | Update |
| --- | --- |
| Design decision, event type, protocol, schema, crate boundary | `docs/V1_DESIGN_SPEC.md` (and the layering test for boundaries) |
| Milestone progress, known gaps | `PROJECT_STATE.md` |
| User-visible change | `CHANGELOG.md` |
| Workflow, structure, validation | This file |

## 8. Git

- Branch from `dev` and open pull requests into `dev`. `main` is updated only
  by promotion from `dev` (see `docs/BRANCHING_AND_RELEASE_CHANNELS.md`).
- Inspect `git status` before editing; preserve unrelated changes.
- Commit or push only when the user asks. Never force-push without explicit
  authorization. Never rewrite published history.
- Keep pull requests focused; do not mix formatting sweeps or dependency bumps
  into functional changes.

## 9. Final response format

End substantive work with these sections, kept short: Summary · Files changed ·
Tests added or changed · Validation performed · Validation not performed ·
Compatibility (events, files, Node API) · Remaining risks · Git status.
