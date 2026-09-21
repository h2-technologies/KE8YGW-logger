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
  workflows, and toolchain changes. It covers the Tauri desktop crate too: that
  crate is a workspace member, so `cargo clippy --workspace --all-targets`
  already checks it, and Clippy subsumes `cargo check`. A separate Tauri job
  used to re-check it, and because every path that selected Tauri also selected
  Rust, it never ran except alongside the Clippy run that had already proven
  the same thing.
- Container validation runs for server container inputs and shared sync
  crates. It depends on change detection alone, not on the preflight job: it
  builds an image and shares nothing with the formatting, documentation and
  governance checks, so gating it on them only delayed the workflow's longest
  job.
- `.github/workflows/ios.yml` carries its own classifier. The macOS job runs
  only for changes to the iOS app, its build scripts, `ham-ios-ffi`,
  `ham-core`, or workspace-wide inputs. macOS minutes bill at ten times the
  Linux rate, so an ungated macOS job was the most expensive thing in the
  repository to leave running on documentation-only pull requests.

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

`Dockerfile.server` uses the same Rust version as `rust-toolchain.toml`,
compiles the dependency graph from the manifests before copying source, and
builds only `ham-server` with `--locked`.

That dependency compile is an ordinary image layer, not a BuildKit cache mount.
The distinction is the whole reason the container job was slow: a cache mount
lives on the builder, not in the image, so nothing exports or restores it
between runs. In CI it was empty every time and each run recompiled every
dependency from source — 8m57s of a 9m42s run. As a layer it is restored from
the Actions cache, which `ci.yml` attaches through Buildx with
`cache-from`/`cache-to: type=gha`.

The layer is built against placeholder sources, so a source-only change reuses
it. `cargo clean --release -p ham-core -p ham-server` then drops the
placeholder-built workspace crates, which is what forces the real build to
recompile them. That removal is explicit rather than timestamp-driven on
purpose: `COPY` preserves mtimes from the build context, so a checkout can look
older than the layer above it, and cargo would otherwise keep the placeholder
objects and ship a `ham-server` whose `main` does nothing.

No container registry is configured in source. CI builds and smokes the image but
does not push dev, beta, or production tags until maintainers configure a
registry and separate channel credentials.
