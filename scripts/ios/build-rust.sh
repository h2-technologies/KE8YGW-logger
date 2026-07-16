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

CONFIGURATION="${CONFIGURATION:-Release}"
CONFIGURATION_LOWER="$(printf '%s' "$CONFIGURATION" | tr '[:upper:]' '[:lower:]')"
PROFILE_DIR="release"
if [[ "$CONFIGURATION_LOWER" == "debug" ]]; then
  PROFILE_DIR="debug"
fi

if [[ -z "${IOS_RUST_TARGETS:-}" ]]; then
  bash "$SCRIPT_DIR/install-targets.sh"
fi

if [[ -n "${IOS_RUST_TARGETS:-}" ]]; then
  read -r -a TARGETS <<<"$IOS_RUST_TARGETS"
else
  TARGETS=(aarch64-apple-ios aarch64-apple-ios-sim)
  if rustup target list --installed | grep -qx "x86_64-apple-ios"; then
    TARGETS+=(x86_64-apple-ios)
  fi
fi

for target in "${TARGETS[@]}"; do
  case "$target" in
    aarch64-apple-ios|aarch64-apple-ios-sim|x86_64-apple-ios) ;;
    *)
      echo "error: unsupported iOS Rust target '$target'." >&2
      exit 1
      ;;
  esac
done

if [[ -n "${IOS_RUST_TARGETS:-}" ]]; then
  IOS_RUST_TARGETS="${TARGETS[*]}" bash "$SCRIPT_DIR/install-targets.sh"
fi

USE_CARGO_LOCKED=0
if [[ "${CI:-}" == "true" || "${CARGO_LOCKED:-}" == "1" ]]; then
  USE_CARGO_LOCKED=1
fi

cargo_build_ham_ios_ffi() {
  local target="$1"
  shift

  if [[ "$USE_CARGO_LOCKED" == "1" ]]; then
    cargo build --locked -p ham-ios-ffi --target "$target" "$@"
  else
    cargo build -p ham-ios-ffi --target "$target" "$@"
  fi
}

target_requested() {
  local requested="$1"
  local target

  for target in "${TARGETS[@]}"; do
    if [[ "$target" == "$requested" ]]; then
      return 0
    fi
  done

  return 1
}

cd "$REPO_ROOT"
for target in "${TARGETS[@]}"; do
  echo "Building ham-ios-ffi for $target ($CONFIGURATION)"
  if [[ "$PROFILE_DIR" == "debug" ]]; then
    cargo_build_ham_ios_ffi "$target"
  else
    cargo_build_ham_ios_ffi "$target" --release
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

SIM_INPUTS=()
if target_requested "aarch64-apple-ios-sim"; then
  if [[ ! -f "$SIM_LIB_ARM" ]]; then
    echo "error: simulator library was not produced at $SIM_LIB_ARM" >&2
    exit 1
  fi
  SIM_INPUTS+=("$SIM_LIB_ARM")
fi
if target_requested "x86_64-apple-ios"; then
  if [[ ! -f "$SIM_LIB_X86" ]]; then
    echo "error: simulator library was not produced at $SIM_LIB_X86" >&2
    exit 1
  fi
  SIM_INPUTS+=("$SIM_LIB_X86")
fi

if [[ "${#SIM_INPUTS[@]}" -gt 0 ]]; then
  rm -f "$SIM_LIB_OUT"
  if [[ "${#SIM_INPUTS[@]}" -gt 1 ]]; then
    xcrun lipo -create "${SIM_INPUTS[@]}" -output "$SIM_LIB_OUT"
  else
    cp "${SIM_INPUTS[0]}" "$SIM_LIB_OUT"
  fi
  xcrun ranlib "$SIM_LIB_OUT"
fi

DEVICE_LIB="$REPO_ROOT/target/aarch64-apple-ios/$PROFILE_DIR/libham_ios_ffi.a"
if target_requested "aarch64-apple-ios"; then
  if [[ ! -f "$DEVICE_LIB" ]]; then
    echo "error: device library was not produced at $DEVICE_LIB" >&2
    exit 1
  fi
  xcrun ranlib "$DEVICE_LIB"
fi

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
    if ! target_requested "aarch64-apple-ios"; then
      echo "error: EFFECTIVE_PLATFORM_NAME=-iphoneos requires IOS_RUST_TARGETS to include aarch64-apple-ios." >&2
      exit 1
    fi
    copy_link_library "-iphoneos" "$DEVICE_LIB"
    ;;
  -iphonesimulator)
    if [[ "${#SIM_INPUTS[@]}" -eq 0 ]]; then
      echo "error: EFFECTIVE_PLATFORM_NAME=-iphonesimulator requires a simulator target in IOS_RUST_TARGETS." >&2
      exit 1
    fi
    copy_link_library "-iphonesimulator" "$SIM_LIB_OUT"
    ;;
  *)
    if target_requested "aarch64-apple-ios"; then
      copy_link_library "-iphoneos" "$DEVICE_LIB"
    fi
    if [[ "${#SIM_INPUTS[@]}" -gt 0 ]]; then
      copy_link_library "-iphonesimulator" "$SIM_LIB_OUT"
    fi
    ;;
esac

echo "Rust iOS libraries ready:"
if target_requested "aarch64-apple-ios"; then
  echo "  device:    $DEVICE_LIB"
fi
if [[ "${#SIM_INPUTS[@]}" -gt 0 ]]; then
  echo "  simulator: $SIM_LIB_OUT"
fi
echo "  headers:   $INCLUDE_DIR"
