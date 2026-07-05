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

build:
    cargo build --workspace

release:
    cargo build --release --workspace

gui:
    cargo run -p ham-gui --bin ham-gui

ci: fmt-check clippy test build
