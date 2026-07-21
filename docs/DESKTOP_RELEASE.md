# Desktop Release

v1 includes signed desktop clients for Windows, macOS, and broad Linux
distribution support. The `0.2.0` workspace now has a real Tauri runtime
wrapper and keeps signing, notarization, updater policy, and installer branding
as v1 work.

## Target

- Installable signed desktop app for Windows.
- Signed/notarized macOS app for supported Intel and Apple Silicon targets.
- Broad Linux distribution packages where the Tauri/WebKitGTK stack supports
  them.
- Shared Rust core and shared web UI.
- Bundled web assets in release mode; no frontend dev server is required.
- Local or hosted/self-hosted API connection through `HAM_DESKTOP_SERVER_URL`
  or the default `http://127.0.0.1:9467`.
- OS credential backend for local GUI/desktop mode.
- Native file dialogs for ADIF, diagnostics, backup, restore, divergence
  reports, and app data directory selection.

## Current Status

`src-tauri` is a workspace member and contains the Tauri v2 runtime package:

- `src-tauri/Cargo.toml`
- `src-tauri/src/main.rs`
- `src-tauri/build.rs`
- `src-tauri/tauri.conf.json`
- `src-tauri/capabilities/default.json`
- `src-tauri/icons/icon.ico`

The runtime depends on `ham-desktop` and delegates native-dialog behavior to the
existing helper layer. The shared `crates/ham-gui/web` assets are bundled
directly by `frontendDist`, so release packaging does not require a frontend dev
server. The previous bad watch-path build failure was caused by a config-only
`src-tauri` directory and an unused dev-server-oriented `devUrl`; the runtime
crate now exists and the config uses the real static asset directory.

The desktop app does not yet embed the full `ham-gui` HTTP backend in-process.
For v0.2 it loads the bundled UI and talks to a configured API endpoint through
a Tauri-only `/api/*` proxy command. Use `HAM_DESKTOP_SERVER_URL` or
`localStorage.ham.desktopServerUrl` to point at a hosted/self-hosted server. For
local development, run:

```powershell
cargo run -p ham-gui --bin ham-gui
```

That starts the local API at `http://127.0.0.1:9467`, which is the desktop
default.

## Commands

The web UI calls these Tauri commands when present:

- `desktop_runtime`
- `desktop_api_request`
- `import_adif_dialog`
- `export_adif_dialog`
- `export_backup_dialog`
- `import_backup_dialog`
- `export_diagnostic_bundle_dialog`
- `export_divergence_report_dialog`
- `select_app_data_directory_dialog`

Dialog commands return the typed `DesktopDialogResult` from `ham-desktop`.
Cancellation is a normal result with `canceled: true`. Full selected paths are
not written to logs by the helper layer; `redacted_path_for_logs` keeps only a
safe placeholder and file name.

When Tauri commands are unavailable, the same web UI falls back to the existing
browser/server path prompt behavior.

## Security Model

Tauri capabilities grant only `core:default` for the main window. The app does
not expose arbitrary filesystem or shell commands. Native dialogs are exposed
only through the seven implemented command wrappers, and all dialog policy,
filters, defaults, cancellation handling, and path redaction live in
`ham-desktop`.

`desktop_api_request` accepts only `/api/*` paths and only `GET`/`POST`. It is a
desktop-only bridge to the configured server URL so packaged assets do not need
browser CORS changes. It does not provide general file access or arbitrary shell
execution.

Credential metadata stays in support storage. Credential secret values stay in
the selected credential backend. The insecure file backend remains explicit
opt-in only with `HAM_PLATFORM_ALLOW_INSECURE_DEV_CREDENTIALS=1`.

## Development Commands

Local GUI/API:

```powershell
cargo run -p ham-gui --bin ham-gui
```

Desktop helper crate:

```powershell
cargo build -p ham-desktop
```

Tauri development run:

```powershell
cargo tauri dev
```

Tauri release/package validation:

```powershell
cargo tauri build
```

## Host Prerequisites

Windows:

- WebView2 Runtime.
- Visual Studio Build Tools or Visual Studio with the MSVC C++ toolchain and
  Windows SDK.
- Tauri CLI available through `cargo tauri`.

Linux:

- WebKitGTK, GTK, libayatana-appindicator, librsvg, and standard build tools as
  required by Tauri v2 for the target distribution.
- `secret-tool`/libsecret for production OS credential storage validation.

macOS:

- Xcode Command Line Tools.
- Keychain access through the system `security` command for credential storage.
- Signing/notarization remains deferred.

## Validation Notes

`cargo check --workspace --all-targets` now includes the Tauri runtime package
and passes in this environment. `cargo tauri info` and `cargo tauri build`
should be attempted on release runners; failures caused by missing WebView2,
MSVC/Windows SDK, Linux WebKitGTK packages, or macOS signing assets are host
prerequisites rather than repository configuration errors.

## Remaining Desktop Work

- Embed or sidecar the local GUI backend if v1 should run fully local without
  an already-running local/hosted API.
- Validate installer/package builds on clean Windows, Linux, and macOS runners.
- Validate OS credential backends on clean release runners.
- Preserve versioned release artifact names, checksums, and attestations.
- Finish signing/notarization/updater decisions, including the issue #2 policy:
  automatic signed downloads on unmetered connections, metered downloads by
  opt-in, and prompt before installation.
