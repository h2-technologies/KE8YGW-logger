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

client:
    cargo run -p ham-client --bin ham-client -- serve

server:
    cargo run -p ham-server --bin ham-server

ci: fmt-check clippy test api-contract version-check docs-link-check governance-check
