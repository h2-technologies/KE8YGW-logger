# Project State

Verified state of the v1 build. Update it in the same pull request as the work.
The plan and every decision are in [the design spec](docs/V1_DESIGN_SPEC.md).

As of Oct 30, 2026. Version `1.0.0-alpha.0`.

## Milestones

| Milestone | Dates | Status |
| --- | --- | --- |
| 0 Reset and skeleton | Oct 30 to Nov 1 | Done, pending merge of the reset pull request |
| 1 Core log and clock | Nov 2 to Nov 15 | Not started |
| 2 WFD engine and exports | Nov 16 to Nov 29 | Not started |
| 3 Sync, accounts, join | Nov 30 to Dec 13 | Not started |
| 4 GUI, FLRig, N3FJP inbound | Dec 14 to Dec 27 | Not started |
| 5 N3FJP outbound, TUI, relay | Dec 28 to Jan 10 | Not started |
| 6 Hardening, packaging | Jan 11 to Jan 22 | Not started |

Fixed dates: club dress rehearsal Sat Jan 9; feature freeze Sun Jan 10;
Winter Field Day Jan 23-24, 2027; logs due at winterfieldday.org by 23:59 UTC
on March 1, 2027.

## What exists

- The 18-crate workspace from section 3 of the spec. Library crates hold only
  their responsibility docs.
- Binaries `ke8ygw`, `ke8ygw-node`, `ke8ygw-tui` and `ke8ygw-relay` answer
  `--version` and `--help`, with tests that run each binary.
- `crates/ke8ygw-testkit/tests/layering.rs` enforces the allowed edges between
  crates and that only `ke8ygw-netsetup` may use `unsafe`.
- CI runs format, Clippy and tests on Linux, Windows and macOS, plus an
  aarch64 Linux check for the Raspberry Pi. Security scanning (cargo-audit,
  cargo-deny, Semgrep, actionlint) and Scorecard carry over from v0.

## Open follow-ups

- Push the archive tag. The v0.5.1 code is on the `archive/v0` branch, but the
  session that did the reset could not push tags. The owner runs:
  `git tag -a archive/v0.5.1-final 1bee7803b0c4021b6b74926b7d174573e9c84737 -m "Final v0.5.1 code before the v1 restart" && git push origin archive/v0.5.1-final`.
  The name avoids the `v*.*.*` pattern that would have triggered the old
  release workflow.
- Point DNS for `ke8ygw.h2tech.dev` at the Vultr VPS and get a Let's Encrypt
  certificate before Milestone 5. `.dev` domains are HTTPS-only.
- Start Windows code-signing identity verification (Azure Artifact Signing or
  an OV certificate) in November so it is ready for Milestone 6.
- Build `sections.toml` from the official ARRL and RAC lists and cross-check it
  against the LOCATION choices on WFDA's submission form (Milestone 2).
- Confirm the N3FJP Winter Field Day Contest Log class field follows the 2027
  naming (Milestone 4).
- Branch protection: if `dev` or `main` require the old CI job names, switch
  the required check to `CI result`, which this CI still reports.

## Porting candidates from archive/v0

Porting is allowed when the code fits the v1 design, with new tests. Worth
reviewing before writing from scratch:

| v0 path on `archive/v0` | Could help with |
| --- | --- |
| `crates/ham-core/src/adif.rs` | ADIF import/export (Milestone 2) |
| `crates/ham-core/src/event.rs`, `crates/ham-core/src/store.rs` | Hash chain and JSONL append/verify (Milestone 1) |
| `crates/ham-server/src/projector.rs` | JSONL to SurrealDB projection (Milestone 1) |
| `crates/ham-core/src/contest.rs` | Versioned contest definitions (Milestone 2); v1 uses TOML plus rule kinds |
| `crates/ham-core/src/rig.rs` | Rig state model for FLRig (Milestone 4) |
