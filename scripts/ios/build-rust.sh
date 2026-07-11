#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: ham-ios-ffi Apple builds require macOS." >&2
  exit 1
fi

bash "$SCRIPT_DIR/install-targets.sh"

CONFIGURATION="${CONFIGURATION:-Release}"
CONFIGURATION_LOWER="$(printf '%s' "$CONFIGURATION" | tr '[:upper:]' '[:lower:]')"
PROFILE_DIR="release"
CARGO_PROFILE_ARGS=(--release)
if [[ "$CONFIGURATION_LOWER" == "debug" ]]; then
  PROFILE_DIR="debug"
  CARGO_PROFILE_ARGS=()
fi

TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim)
if rustup target list --installed | grep -qx "x86_64-apple-ios"; then
  TARGETS+=(x86_64-apple-ios)
fi

cd "$REPO_ROOT"
for target in "${TARGETS[@]}"; do
  echo "Building ham-ios-ffi for $target ($CONFIGURATION)"
  cargo build -p ham-ios-ffi --target "$target" "${CARGO_PROFILE_ARGS[@]}"
done

INCLUDE_DIR="$REPO_ROOT/artifacts/ios/include"
mkdir -p "$INCLUDE_DIR"
cp "$REPO_ROOT/crates/ham-ios-ffi/include/ham_ios_ffi.h" "$INCLUDE_DIR/ham_ios_ffi.h"
cp "$REPO_ROOT/crates/ham-ios-ffi/include/module.modulemap" "$INCLUDE_DIR/module.modulemap"

SIM_OUTPUT_DIR="$REPO_ROOT/target/ios-universal-sim/$PROFILE_DIR"
mkdir -p "$SIM_OUTPUT_DIR"
SIM_LIB_ARM="$REPO_ROOT/target/aarch64-apple-ios-sim/$PROFILE_DIR/libham_ios_ffi.a"
SIM_LIB_X86="$REPO_ROOT/target/x86_64-apple-ios/$PROFILE_DIR/libham_ios_ffi.a"
SIM_LIB_OUT="$SIM_OUTPUT_DIR/libham_ios_ffi.a"

if [[ -f "$SIM_LIB_ARM" && -f "$SIM_LIB_X86" ]]; then
  xcrun lipo -create "$SIM_LIB_ARM" "$SIM_LIB_X86" -output "$SIM_LIB_OUT"
elif [[ -f "$SIM_LIB_ARM" ]]; then
  cp "$SIM_LIB_ARM" "$SIM_LIB_OUT"
else
  echo "error: simulator library was not produced at $SIM_LIB_ARM" >&2
  exit 1
fi

DEVICE_LIB="$REPO_ROOT/target/aarch64-apple-ios/$PROFILE_DIR/libham_ios_ffi.a"
if [[ ! -f "$DEVICE_LIB" ]]; then
  echo "error: device library was not produced at $DEVICE_LIB" >&2
  exit 1
fi

echo "Rust iOS libraries ready:"
echo "  device:    $DEVICE_LIB"
echo "  simulator: $SIM_LIB_OUT"
echo "  headers:   $INCLUDE_DIR"
