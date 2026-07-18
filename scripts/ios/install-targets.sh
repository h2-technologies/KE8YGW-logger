#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Xcode archive shells often start with a minimal PATH and do not load a login
# profile, so bootstrap the normal Rust install locations before checking tools.
# shellcheck source=rust-env.sh
. "$SCRIPT_DIR/rust-env.sh"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: iOS Rust targets can only be installed from macOS." >&2
  exit 1
fi

require_tool xcodebuild "Install Xcode and select it with xcode-select."
require_tool xcrun "Install Xcode command-line tools and select Xcode with xcode-select."
require_tool rustup "Install Rust from https://rustup.rs or ensure ~/.cargo/bin is visible to Xcode."
require_tool cargo "Install Rust from https://rustup.rs or ensure ~/.cargo/bin is visible to Xcode."
require_tool rustc "Install Rust from https://rustup.rs or ensure ~/.cargo/bin is visible to Xcode."

if ! xcode-select -p >/dev/null 2>&1; then
  echo "error: Xcode command-line tools are not selected. Run: sudo xcode-select -s /Applications/Xcode.app/Contents/Developer" >&2
  exit 1
fi

echo "Xcode:"
xcodebuild -version
echo "Rust:"
rustc --version
cargo --version

if [[ -n "${IOS_RUST_TARGETS:-}" ]]; then
  read -r -a requested_targets <<<"$IOS_RUST_TARGETS"
else
  requested_targets=(aarch64-apple-ios aarch64-apple-ios-sim x86_64-apple-ios)
fi

for target in "${requested_targets[@]}"; do
  case "$target" in
    aarch64-apple-ios|aarch64-apple-ios-sim)
      rustup target add "$target"
      ;;
    x86_64-apple-ios)
      if rustup target add "$target"; then
        echo "Installed Intel simulator target x86_64-apple-ios."
      else
        echo "warning: x86_64-apple-ios target is unavailable for this Rust toolchain; Apple Silicon simulator builds will still be produced." >&2
      fi
      ;;
    *)
      echo "error: unsupported iOS Rust target '$target'." >&2
      exit 1
      ;;
  esac
done
