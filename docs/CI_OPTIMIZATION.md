# CI Optimization Notes

The CI workflow now separates fast preflight checks from expensive platform work.

## Change Selection

`.github/workflows/ci.yml` uses a first-party change detector. Workflow, lockfile,
toolchain, workspace manifest, or shared dependency changes conservatively enable
all expensive paths.

- Governance runs for documentation and repository-policy changes.
- API contract validation runs for OpenAPI, contract scripts, contract crates,
  and hosted API changes.
- Rust quality runs for Cargo files, Rust crates, embedded web assets, scripts,
  workflows, and toolchain changes.
- Tauri validation runs for `src-tauri`, desktop helper, GUI, and shared core
  changes.
- iOS selection is exposed for iOS workflow coordination.
- Container validation runs for sync-server container inputs and shared sync
  crates.

Skipped jobs still resolve as successful skipped checks rather than pending
checks.

## Redundant Build Removal

`just ci` no longer runs a full `cargo build --workspace` after Clippy and tests.
Standalone `just build` and `just release` remain available and now use
`--locked`.

## Cache Strategy

CI uses the pinned Rust toolchain in `rust-toolchain.toml`. Pull requests restore
compatible caches, but cache saves are limited to `dev` and `main` to avoid large
branch-private uploads with low reuse. Cache keys are split by OS and debug
profile. Production release builds use the release profile and do not share debug
target directories.

Measure cache restore time, compilation time, and save time from the named
Actions steps before introducing another compiler cache layer such as `sccache`.

## Docker

`Dockerfile.sync-server` uses the same Rust version as `rust-toolchain.toml`,
fetches dependencies from manifests before copying source, uses BuildKit Cargo
cache mounts, and builds only `ham-sync-server` with `--locked`.

No container registry is configured in source. CI builds and smokes the image but
does not push dev, beta, or production tags until maintainers configure a
registry and separate channel credentials.
