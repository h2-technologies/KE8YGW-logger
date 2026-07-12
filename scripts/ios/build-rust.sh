#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Xcode archive shells do not reliably inherit shell profile PATH changes.
# shellcheck source=rust-env.sh
. "$SCRIPT_DIR/rust-env.sh"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: ham-ios-ffi Apple builds require macOS." >&2
  exit 1
fi

bash "$SCRIPT_DIR/install-targets.sh"

CONFIGURATION="${CONFIGURATION:-Release}"
CONFIGURATION_LOWER="$(printf '%s' "$CONFIGURATION" | tr '[:upper:]' '[:lower:]')"
PROFILE_DIR="release"
if [[ "$CONFIGURATION_LOWER" == "debug" ]]; then
  PROFILE_DIR="debug"
fi

TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim)
if rustup target list --installed | grep -qx "x86_64-apple-ios"; then
  TARGETS+=(x86_64-apple-ios)
fi

cd "$REPO_ROOT"
for target in "${TARGETS[@]}"; do
  echo "Building ham-ios-ffi for $target ($CONFIGURATION)"
  if [[ "$PROFILE_DIR" == "debug" ]]; then
    cargo build -p ham-ios-ffi --target "$target"
  else
    cargo build -p ham-ios-ffi --target "$target" --release
  fi
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

rm -f "$SIM_LIB_OUT"
if [[ -f "$SIM_LIB_ARM" && -f "$SIM_LIB_X86" ]]; then
  xcrun lipo -create "$SIM_LIB_ARM" "$SIM_LIB_X86" -output "$SIM_LIB_OUT"
elif [[ -f "$SIM_LIB_ARM" ]]; then
  cp "$SIM_LIB_ARM" "$SIM_LIB_OUT"
else
  echo "error: simulator library was not produced at $SIM_LIB_ARM" >&2
  exit 1
fi
xcrun ranlib "$SIM_LIB_OUT"

DEVICE_LIB="$REPO_ROOT/target/aarch64-apple-ios/$PROFILE_DIR/libham_ios_ffi.a"
if [[ ! -f "$DEVICE_LIB" ]]; then
  echo "error: device library was not produced at $DEVICE_LIB" >&2
  exit 1
fi
xcrun ranlib "$DEVICE_LIB"

LINK_ROOT="$REPO_ROOT/artifacts/ios/link"
copy_link_library() {
  local platform_suffix="$1"
  local source_lib="$2"
  local destination_dir="$LINK_ROOT/${CONFIGURATION}${platform_suffix}"
  mkdir -p "$destination_dir"
  cp "$source_lib" "$destination_dir/libham_ios_ffi.a"
  xcrun ranlib "$destination_dir/libham_ios_ffi.a"
  echo "  link:      $destination_dir/libham_ios_ffi.a"
}

case "${EFFECTIVE_PLATFORM_NAME:-}" in
  -iphoneos)
    copy_link_library "-iphoneos" "$DEVICE_LIB"
    ;;
  -iphonesimulator)
    copy_link_library "-iphonesimulator" "$SIM_LIB_OUT"
    ;;
  *)
    copy_link_library "-iphoneos" "$DEVICE_LIB"
    copy_link_library "-iphonesimulator" "$SIM_LIB_OUT"
    ;;
esac

echo "Rust iOS libraries ready:"
echo "  device:    $DEVICE_LIB"
echo "  simulator: $SIM_LIB_OUT"
echo "  headers:   $INCLUDE_DIR"
