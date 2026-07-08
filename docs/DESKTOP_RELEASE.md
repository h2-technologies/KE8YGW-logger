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
configuration as the v0.2 desktop packaging foundation. `ham-desktop` contains
the stable runtime/native-dialog command helpers that a Tauri wrapper should
call. The full Tauri runtime dependency and installer build are not yet wired
into CI, but the package metadata, bundled shared web UI target, release-mode
no-dev-server requirement, and native dialog command behavior are represented
and unit-tested in source.

The shared web UI now detects Tauri native dialog commands when available and
falls back to browser/server prompts when running outside desktop mode.

## v0.2 Work

- [x] Add `ham-desktop` and `src-tauri` structure.
- [x] Document release mode with bundled shared web UI and no dev-server
  requirement.
- [x] Add native dialog bridge command helpers for ADIF import/export,
  diagnostic ZIP export, backup import/export, divergence report export, and
  app data directory selection.
- [x] Keep browser/server mode independent from native dialogs.
- [x] Use local official/support storage paths in `ham-gui`.
- [x] Wire platform credential backend selection for local GUI/desktop mode.
- [ ] Add the actual Tauri Rust runtime wrapper crate under `src-tauri`.
- [ ] Validate full installer/package builds.
- [x] Document dev and release desktop modes.

## Development Commands

Static local GUI:

```powershell
cargo run -p ham-gui --bin ham-gui
```

Desktop foundation check:

```powershell
cargo build -p ham-desktop
```

Tauri package validation should run from the repository root after `src-tauri`
has a Rust runtime crate:

```powershell
cargo tauri build
```

The Tauri CLI is present in the current development environment, but package
validation still requires a real `src-tauri/Cargo.toml` and Tauri runtime
entrypoint. That work remains v0.2/v1.0 desktop packaging follow-up.

## App Data and Storage

- Local official event history uses the append-only JSONL store from
  `ham-core`.
- Local support state uses versioned JSON support stores under the app support
  directory.
- `HAM_DESKTOP_APP_DATA_DIR` is reserved for selecting an explicit desktop app
  data directory.
- `HAM_DESKTOP_SERVER_URL` is reserved for hosted/self-hosted sync
  configuration.
- Credential metadata is stored in local support storage, while credential
  secret values are stored through the selected OS credential backend. The
  insecure file backend requires explicit opt-in with
  `HAM_PLATFORM_ALLOW_INSECURE_DEV_CREDENTIALS=1`.

## Native Dialog Contract

The web UI calls these Tauri commands when present:

- `desktop_dialog_open` for ADIF import and backup import.
- `desktop_dialog_save` for ADIF export, backup export, diagnostic bundle
  export, and divergence report export.
- `desktop_select_app_data_directory` for selecting the local app data
  directory.

When the commands are unavailable, the same UI falls back to the existing
browser/server path prompt behavior.

The helper layer treats user cancellation as a non-fatal canceled result and
redacts full user-selected paths before they are suitable for logs.

## Credential Backends

- Windows: `OsCredentialStore` uses Windows Credential Manager through the
  Windows credential APIs.
- macOS: `OsCredentialStore` uses the system Keychain through the `security`
  command-line interface.
- Linux: `OsCredentialStore` uses Secret Service through `secret-tool`
  from libsecret tooling.
- Unsupported or unavailable platforms return a clear unsupported backend
  status. Production mode must not silently fall back to plaintext storage.

## v1.0 Polish

- Signing and notarization.
- Installer polish.
- Auto-update policy.
- Release artifact checksums and SBOM if practical.
