# Dependency Advisory Ledger

Last reviewed: 2026-07-18

This ledger records RustSec and Dependabot findings that were reviewed during
the first-pass code-scanning remediation. It is intentionally narrow: entries
below are not suppressed as fixed unless the dependency graph no longer contains
the affected package version.

## Fixed In This Pass

| Advisory | Package | Resolution | Evidence |
| --- | --- | --- | --- |
| RUSTSEC-2026-0194 | `quick-xml 0.39.4` | Fixed by updating `rfd` to `0.17.2` and disabling its optional Wayland feature, which removed `wayland-scanner -> quick-xml 0.39.4`. | `cargo tree --locked --target all -i quick-xml@0.39.4` no longer matches; `quick-xml 0.41.0` remains through `plist`. |
| RUSTSEC-2026-0195 | `quick-xml 0.39.4` | Fixed by the same `rfd` dependency and feature update. | `Cargo.lock` contains no `quick-xml` version below `0.41.0`. |
| Yanked crate warning | `spin 0.9.8` | Fixed by a targeted lockfile update to `spin 0.9.9`. | `cargo audit --ignore RUSTSEC-2023-0071` no longer reports yanked crates. |

## Residual Actual Vulnerability

| Advisory | Package | Path | Classification | Reason and next review |
| --- | --- | --- | --- | --- |
| RUSTSEC-2023-0071 | `rsa 0.9.10` | `rsa -> jsonwebtoken -> surrealdb-core -> surrealdb -> ham-server`; also through optional `ham-sync` `surreal-storage` and crates that depend on `ham-sync`. | Actual vulnerability with no upstream fix. | The repository does not directly perform RSA private-key operations; the crate is transitive through SurrealDB's authentication/JWT dependency stack while preserving required `kv-surrealkv`, `kv-mem`, `protocol-ws`, and `rustls` features. Review by 2026-10-18 or when SurrealDB/jsonwebtoken removes RustCrypto `rsa`. |

`cargo audit` is run with `--ignore RUSTSEC-2023-0071` in CI because the
advisory has no patched release. The advisory remains documented here and in
`deny.toml`; it is not considered fixed.

## Residual Informational Or Platform-Specific Advisories

| Advisory | Package family | Path | Classification | Reason and next review |
| --- | --- | --- | --- | --- |
| RUSTSEC-2024-0411 through RUSTSEC-2024-0420 | GTK3 bindings: `atk`, `atk-sys`, `gdk`, `gdk-sys`, `gdkwayland-sys`, `gdkx11`, `gdkx11-sys`, `gtk`, `gtk-sys`, `gtk3-macros` | Tauri Linux desktop backend through `tauri`, `tauri-runtime-wry`, `wry`, `tao`, `muda`, and `webkit2gtk`. | Informational unmaintained, platform-specific to Linux desktop builds. | Tauri v2 currently uses the GTK3/WebKitGTK Linux backend. Removing it requires an upstream Tauri/wry backend migration or dropping Linux desktop support, which is out of scope. Review by 2026-10-18 with Tauri/wry release notes. |
| RUSTSEC-2024-0429 / GHSA-wrw7-89jp-8q8g | `glib 0.18.5` | Same Tauri GTK3/WebKitGTK Linux backend path. | Platform-specific unsoundness; affected function appears unreachable from repository code. | The affected `glib::VariantStrIter` iterator methods are not called by repository code, but they remain available transitively in Linux desktop builds. Dependabot still reports this alert. Review by 2026-10-18 with Tauri/wry release notes. |
| RUSTSEC-2024-0370 | `proc-macro-error 1.0.4` | `proc-macro-error -> glib-macros` and `gtk3-macros` through the Tauri GTK3 Linux backend. | Informational unmaintained, build-time transitive dependency. | No repository direct use. Review by 2026-10-18 with GTK/Tauri macro dependency updates. |
| RUSTSEC-2025-0075, RUSTSEC-2025-0080, RUSTSEC-2025-0081, RUSTSEC-2025-0098, RUSTSEC-2025-0100 | `unic-* 0.9.0` | `unic-* -> urlpattern -> tauri-utils -> tauri`. | Informational unmaintained transitive dependency. | No repository direct use. Review by 2026-10-18 with Tauri/urlpattern updates. |
| RUSTSEC-2023-0089 | `atomic-polyfill 1.0.3` | Target-specific transitive dependency from SurrealDB's dependency graph. | Informational unmaintained. | The required SurrealDB features could not be reduced further without removing embedded SurrealKV, in-memory tests, WebSocket protocol, or rustls. Review by 2026-10-18 or the next SurrealDB 3.x milestone. |
| RUSTSEC-2025-0141 | `bincode 2.0.1` | `bincode -> surrealmx -> surrealdb-core -> surrealdb`. | Informational unmaintained transitive dependency. | The package enters through SurrealDB's storage/indexing stack. Review by 2026-10-18 or the next SurrealDB 3.x milestone. |

## Current Automation

- `cargo audit --ignore RUSTSEC-2023-0071` blocks newly introduced actionable
  vulnerability advisories while preserving the no-fix RSA exception.
- `cargo deny check advisories` uses advisory-specific exceptions in `deny.toml`
  with review dates in each reason.
- The security workflow verifies that `quick-xml 0.39.4` is absent from the
  dependency graph.

