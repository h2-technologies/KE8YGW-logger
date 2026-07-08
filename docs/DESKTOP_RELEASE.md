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

The existing `ham-gui` crate is web-first and Tauri-ready but is not yet a
packaged Tauri desktop application. Import/export flows still use typed paths.

## v0.2 Work

- Add `ham-desktop` or `src-tauri` structure.
- Run the shared web UI without a dev server.
- Bridge native file dialogs for import/export/report/backup flows.
- Use local official/support storage paths.
- Wire platform credential backend selection.
- Document dev and release desktop modes.

## v1.0 Polish

- Signing and notarization.
- Installer polish.
- Auto-update policy.
- Release artifact checksums and SBOM if practical.
