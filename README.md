# KE8YGW Logger

A peer-to-peer, offline-first amateur radio logger. Two to ten stations at one
Winter Field Day site log into one merged, scored, exportable log, with no
Internet needed. Single-operator use works as a one-node event.

v1 targets **Winter Field Day, Jan 23-24, 2027**.

## Status

Milestone 0 (reset and skeleton) is done: the Rust workspace builds and tests
on Linux, Windows and macOS, with a check for Raspberry Pi. Nothing is usable
for logging yet. See [PROJECT_STATE.md](PROJECT_STATE.md) for milestone status
and [the design spec](docs/V1_DESIGN_SPEC.md) for what v1 will do.

The previous v0.5.1 codebase (hosted server, iOS app, Tauri client) is
preserved on the `archive/v0` branch.

## What v1 does

- Every station sees every QSO within about a second on the LAN.
- Dupe checks across all stations, including while the network is split.
- Live WFD 2027 score and objective tracking.
- Cabrillo and ADIF exports that winterfieldday.org accepts.
- Two-way sync with N3FJP's Winter Field Day Contest Log, and rig control
  through FLRig.
- No acknowledged QSO is ever lost to a crash or power cut.

## Building

Rust 1.96 is pinned in `rust-toolchain.toml`.

```sh
cargo build --workspace
cargo test --workspace
```

Binaries: `ke8ygw` (desktop GUI and CLI), `ke8ygw-tui`, `ke8ygw-node`
(headless, for Raspberry Pi) and `ke8ygw-relay` (optional relay). At
Milestone 0 they only answer `--version` and `--help`.

## Documentation

- [Design spec](docs/V1_DESIGN_SPEC.md): architecture, decisions, milestones.
- [Project state](PROJECT_STATE.md): what is built and what is next.
- [AGENTS.md](AGENTS.md): rules for AI coding agents.
- [Contributing](CONTRIBUTING.md), [Security](SECURITY.md),
  [Support](SUPPORT.md), [Governance](GOVERNANCE.md),
  [Release policy](RELEASE.md).

## License

MIT. See [LICENSE](LICENSE).
