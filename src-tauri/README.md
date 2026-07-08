# KE8YGW Logger Tauri Foundation

This directory is the v0.2 desktop packaging foundation. It prepares a Tauri
shell for the shared `crates/ham-gui/web` frontend without making the workspace
depend on the full Tauri Rust crate during this pass.

Planned Tauri commands consumed by the web UI:

- `desktop_dialog_open({ kind: "adif" })`
- `desktop_dialog_save({ kind: "adif" })`
- `desktop_dialog_open({ kind: "backup" })`
- `desktop_dialog_save({ kind: "backup" })`
- `desktop_dialog_save({ kind: "diagnostic-bundle" })`
- `desktop_dialog_save({ kind: "divergence-report" })`
- `desktop_dialog_open({ kind: "app-data-directory" })`

Release mode must load the static web assets from the bundle and must not
require a dev server. Signing and notarization remain v1.0 polish.
