#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: iOS Rust targets can only be installed from macOS." >&2
  exit 1
fi

for tool in xcodebuild xcrun rustup cargo rustc; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: required tool '$tool' was not found on PATH." >&2
    exit 1
  fi
done

if ! xcode-select -p >/dev/null 2>&1; then
  echo "error: Xcode command-line tools are not selected. Run: sudo xcode-select -s /Applications/Xcode.app/Contents/Developer" >&2
  exit 1
fi

echo "Xcode:"
xcodebuild -version
echo "Rust:"
rustc --version
cargo --version

rustup target add aarch64-apple-ios
rustup target add aarch64-apple-ios-sim

if rustup target add x86_64-apple-ios; then
  echo "Installed Intel simulator target x86_64-apple-ios."
else
  echo "warning: x86_64-apple-ios target is unavailable for this Rust toolchain; Apple Silicon simulator builds will still be produced." >&2
fi
