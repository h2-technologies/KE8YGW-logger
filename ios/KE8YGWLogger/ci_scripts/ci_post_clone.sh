#!/usr/bin/env bash
set -euo pipefail

# Xcode Cloud images do not include Rust by default. Install a minimal stable
# toolchain before xcodebuild starts so the Xcode build phase can build
# HamIOSFFI through scripts/ios/build-xcframework.sh.
CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export CARGO_HOME RUSTUP_HOME
export PATH="$CARGO_HOME/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

retry_command() {
  local attempts="$1"
  local delay_seconds="$2"
  shift 2

  local attempt=1
  local status=0
  until "$@"; do
    status=$?
    if [[ "$attempt" -ge "$attempts" ]]; then
      return "$status"
    fi

    echo "warning: command failed, retrying in ${delay_seconds}s (attempt $attempt/$attempts): $*" >&2
    sleep "$delay_seconds"
    delay_seconds=$((delay_seconds * 2))
    attempt=$((attempt + 1))
  done
}

install_rustup() {
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain stable
}

if ! command -v rustup >/dev/null 2>&1; then
  retry_command 4 5 install_rustup
fi

# shellcheck source=/dev/null
. "$CARGO_HOME/env"

retry_command 4 5 rustup target add aarch64-apple-ios
retry_command 4 5 rustup target add aarch64-apple-ios-sim
if retry_command 3 5 rustup target add x86_64-apple-ios; then
  echo "Installed Intel simulator target x86_64-apple-ios."
else
  echo "warning: x86_64-apple-ios target unavailable; Apple Silicon simulator builds will still be produced." >&2
fi

rustc --version
cargo --version
