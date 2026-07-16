fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

check:
    cargo check --workspace --all-targets

clippy:
    cargo clippy --workspace --all-targets -- -D warnings

test:
    cargo test --workspace

api-contract:
    python scripts/check_api_contract.py

governance-check:
    pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/governance-check.ps1

build:
    cargo build --workspace

release:
    cargo build --release --workspace

gui:
    cargo run -p ham-gui --bin ham-gui

sync-server:
    cargo run -p ham-sync-server --bin ham-sync-server

ci: fmt-check clippy test api-contract build governance-check
