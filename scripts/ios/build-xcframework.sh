#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: HamIOSFFI.xcframework can only be assembled on macOS with Xcode." >&2
  exit 1
fi

bash "$SCRIPT_DIR/build-rust.sh"

CONFIGURATION="${CONFIGURATION:-Release}"
CONFIGURATION_LOWER="$(printf '%s' "$CONFIGURATION" | tr '[:upper:]' '[:lower:]')"
PROFILE_DIR="release"
if [[ "$CONFIGURATION_LOWER" == "debug" ]]; then
  PROFILE_DIR="debug"
fi

INCLUDE_DIR="$REPO_ROOT/artifacts/ios/include"
DEVICE_LIB="$REPO_ROOT/target/aarch64-apple-ios/$PROFILE_DIR/libham_ios_ffi.a"
SIM_LIB="$REPO_ROOT/target/ios-universal-sim/$PROFILE_DIR/libham_ios_ffi.a"
XCFRAMEWORK="$REPO_ROOT/artifacts/HamIOSFFI.xcframework"

rm -rf "$XCFRAMEWORK"
xcodebuild -create-xcframework \
  -library "$DEVICE_LIB" -headers "$INCLUDE_DIR" \
  -library "$SIM_LIB" -headers "$INCLUDE_DIR" \
  -output "$XCFRAMEWORK"

echo "Created $XCFRAMEWORK"
bash "$SCRIPT_DIR/verify-linkage.sh"
