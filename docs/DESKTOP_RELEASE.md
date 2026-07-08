# Desktop Release

v1.0 includes an installable desktop app. v0.2 should prepare the desktop
architecture and leave signing/notarization polish for v1.0 if it would slow the
functional beta.

## Target

- Installable desktop app for Windows x64 first.
- Linux x64 package where feasible.
- macOS x64/arm64 package where feasible.
- Shared Rust core.
- Shared web UI embedded without requiring a dev server.
- Local official event storage.
- Local support storage.
- OS credential backend.
- Hosted/self-hosted sync configuration.
- Native file dialogs for ADIF, diagnostics, backup, and restore.

## Current Status

The repository now includes a `ham-desktop` crate and root `src-tauri`
configuration as the v0.2 desktop packaging foundation. The full Tauri runtime
dependency and installer build are not yet wired into CI, but the package
metadata, bundled shared web UI target, release-mode no-dev-server requirement,
and native dialog command contract are represented in source.

The shared web UI now detects Tauri native dialog commands when available and
falls back to browser/server prompts when running outside desktop mode.

## v0.2 Work

- [x] Add `ham-desktop` and `src-tauri` structure.
- [x] Document release mode with bundled shared web UI and no dev-server
  requirement.
- [x] Add native dialog bridge contract for ADIF import/export, diagnostic ZIP
  export, backup import/export, divergence report export, and app data
  directory selection.
- [x] Keep browser/server mode independent from native dialogs.
- [x] Use local official/support storage paths in `ham-gui`.
- [ ] Wire the real Tauri Rust runtime dependency and commands.
- [ ] Wire platform credential backend selection.
- Document dev and release desktop modes.

## Development Commands

Static local GUI:

```powershell
cargo run -p ham-gui --bin ham-gui
```

Desktop foundation check:

```powershell
cargo build -p ham-desktop
```

The future Tauri package command should run from `src-tauri` once the Tauri CLI
and runtime dependency are installed. It is intentionally not required by the
current workspace quality gates.

## App Data and Storage

- Local official event history uses the append-only JSONL store from
  `ham-core`.
- Local support state uses versioned JSON support stores under the app support
  directory.
- `HAM_DESKTOP_APP_DATA_DIR` is reserved for selecting an explicit desktop app
  data directory.
- `HAM_DESKTOP_SERVER_URL` is reserved for hosted/self-hosted sync
  configuration.

## Native Dialog Contract

The web UI calls these Tauri commands when present:

- `desktop_dialog_open` for ADIF import and backup import.
- `desktop_dialog_save` for ADIF export, backup export, diagnostic bundle
  export, and divergence report export.

When the commands are unavailable, the same UI falls back to the existing
browser/server path prompt behavior.

## v1.0 Polish

- Signing and notarization.
- Installer polish.
- Auto-update policy.
- Release artifact checksums and SBOM if practical.
