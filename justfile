set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

check:
    cargo check --locked --workspace --all-targets

clippy:
    cargo clippy --locked --workspace --all-targets -- -D warnings

test:
    cargo test --locked --workspace

feature-matrix:
    cargo check --locked -p ham-sync --no-default-features --all-targets
    cargo test --locked -p ham-sync --features surreal-storage

api-contract:
    python scripts/check_api_contract.py

version-check:
    python scripts/check_versions.py

docs-link-check:
    python scripts/check_docs_links.py

governance-check:
    pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/governance-check.ps1

build:
    cargo build --locked --workspace

release:
    cargo build --locked --release --workspace

gui:
    cargo run -p ham-gui --bin ham-gui

sync-server:
    cargo run -p ham-sync-server --bin ham-sync-server

ci: fmt-check clippy test feature-matrix api-contract version-check docs-link-check governance-check
