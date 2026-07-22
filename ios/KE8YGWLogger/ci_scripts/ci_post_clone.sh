#!/usr/bin/env bash
set -euo pipefail

# Xcode Cloud images do not include Rust by default. Install a minimal stable
# toolchain before xcodebuild starts so the Xcode build phase can build
# HamIOSFFI through scripts/ios/build-xcframework.sh.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export CARGO_HOME RUSTUP_HOME
export CARGO_NET_RETRY="${CARGO_NET_RETRY:-5}"
export PATH="$CARGO_HOME/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

retry_command() {
  local attempts="$1"
  shift
  local attempt=1
  local delay=5

  until "$@"; do
    local status="$?"
    if [[ "$attempt" -ge "$attempts" ]]; then
      return "$status"
    fi

    echo "warning: command failed on attempt ${attempt}/${attempts}; retrying in ${delay}s: $*" >&2
    sleep "$delay"
    attempt="$((attempt + 1))"
    if [[ "$delay" -lt 60 ]]; then
      delay="$((delay * 2))"
    fi
  done
}

if ! command -v rustup >/dev/null 2>&1; then
  RUSTUP_VERSION="1.29.0"
  case "$(uname -m)" in
    arm64)
      rustup_arch="aarch64-apple-darwin"
      rustup_sha256="aeb4105778ca1bd3c6b0e75768f581c656633cd51368fa61289b6a71696ac7e1"
      ;;
    x86_64)
      rustup_arch="x86_64-apple-darwin"
      rustup_sha256="33cf85df9142bc6d29cbc62fa5ca1d4c29622cddb55213a4c1a43c457fb9b2d7"
      ;;
    *)
      echo "unsupported macOS architecture for rustup-init: $(uname -m)" >&2
      exit 1
      ;;
  esac

  rustup_init="$(mktemp "${TMPDIR:-/tmp}/rustup-init.XXXXXX")"
  trap 'rm -f "$rustup_init"' EXIT
  curl --proto '=https' --tlsv1.2 -fL \
    "https://static.rust-lang.org/rustup/archive/${RUSTUP_VERSION}/${rustup_arch}/rustup-init" \
    -o "$rustup_init"
  actual_sha256="$(shasum -a 256 "$rustup_init" | awk '{print $1}')"
  if [[ "$actual_sha256" != "$rustup_sha256" ]]; then
    echo "rustup-init checksum mismatch for ${rustup_arch}" >&2
    exit 1
  fi
  chmod +x "$rustup_init"
  "$rustup_init" -y --profile minimal --default-toolchain 1.96.0
  rm -f "$rustup_init"
  trap - EXIT
fi

# shellcheck source=/dev/null
. "$CARGO_HOME/env"

rustup target add aarch64-apple-ios
rustup target add aarch64-apple-ios-sim
if rustup target add x86_64-apple-ios; then
  echo "Installed Intel simulator target x86_64-apple-ios."
else
  echo "warning: x86_64-apple-ios target unavailable; Apple Silicon simulator builds will still be produced." >&2
fi

rustc --version
cargo --version

cd "$REPO_ROOT"
retry_command 5 cargo fetch --locked
