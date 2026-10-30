# Canonical local commands. CI runs the same checks on Linux, Windows and macOS.

set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

default: ci

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

clippy:
    cargo clippy --locked --workspace --all-targets -- -D warnings

test:
    cargo test --locked --workspace

# Raspberry Pi target. Needs `rustup target add aarch64-unknown-linux-gnu` once.
pi-check:
    cargo check --locked --workspace --all-targets --target aarch64-unknown-linux-gnu

ci: fmt-check clippy test pi-check
