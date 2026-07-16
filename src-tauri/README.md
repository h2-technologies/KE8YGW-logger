# KE8YGW Logger Tauri Runtime

This directory contains the Tauri v2 desktop runtime for the shared
`crates/ham-gui/web` UI.

The package is a root workspace member so `cargo check --workspace --all-targets`
also checks the desktop wrapper. Release mode bundles `../crates/ham-gui/web`
through `frontendDist` and does not require a frontend dev server.

## Runtime Behavior

The desktop shell loads bundled static assets and connects to a configured API
endpoint:

- `HAM_DESKTOP_SERVER_URL` sets the desktop API base.
- Default API base: `http://127.0.0.1:9467`.
- The web UI can override the base with `localStorage.ham.desktopServerUrl`.

For local development, start the local GUI/API server separately:

```powershell
cargo run -p ham-gui --bin ham-gui
```

Then run the desktop app:

```powershell
cargo tauri dev
```

Full in-process backend embedding or sidecar launch is not complete in v0.2.

## Tauri Commands

The runtime exposes:

- `desktop_runtime`
- `desktop_api_request`
- `import_adif_dialog`
- `export_adif_dialog`
- `export_backup_dialog`
- `import_backup_dialog`
- `export_diagnostic_bundle_dialog`
- `export_divergence_report_dialog`
- `select_app_data_directory_dialog`

Dialog commands call `ham-desktop` helper functions and return typed
`DesktopDialogResult` values. Cancellation is non-fatal. Browser/server mode
continues to use path prompts when Tauri commands are unavailable.

`desktop_api_request` is limited to `GET`/`POST` requests whose paths start with
`/api/`; it is not a general network, filesystem, or shell bridge.

## Packaging

```powershell
cargo tauri info
cargo tauri build
```

Windows package builds require WebView2 Runtime plus Visual Studio Build Tools
or Visual Studio with the MSVC C++ toolchain and Windows SDK. Linux and macOS
builds require the normal Tauri v2 platform toolchains; signing and
notarization are deferred.
