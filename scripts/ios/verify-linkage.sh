#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
XCFRAMEWORK="$REPO_ROOT/artifacts/HamIOSFFI.xcframework"

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "error: XCFramework linkage verification requires macOS." >&2
  exit 1
fi

if [[ ! -d "$XCFRAMEWORK" ]]; then
  echo "error: missing $XCFRAMEWORK. Run scripts/ios/build-xcframework.sh first." >&2
  exit 1
fi

if [[ ! -f "$REPO_ROOT/artifacts/ios/include/ham_ios_ffi.h" ]]; then
  echo "error: missing public header in artifacts/ios/include." >&2
  exit 1
fi

found=0
required_symbols=(ham_ios_call_json_bytes ham_ios_free_string)

symbol_exists() {
  local symbol_output="$1"
  local symbol="$2"
  grep -Eq "(^|[[:space:]])_?${symbol}($|[[:space:]])" <<<"$symbol_output"
}

while IFS= read -r -d '' lib; do
  found=$((found + 1))
  echo "Inspecting $lib"
  xcrun lipo -info "$lib"

  for arch in $(xcrun lipo -archs "$lib"); do
    echo "  arch: $arch"
    symbols="$(xcrun nm -arch "$arch" -gU "$lib" 2>/dev/null || true)"
    if [[ -z "$symbols" ]]; then
      symbols="$(xcrun nm -gU "$lib" 2>/dev/null || true)"
    fi

    for symbol in "${required_symbols[@]}"; do
      if ! symbol_exists "$symbols" "$symbol"; then
        echo "error: $lib ($arch) does not export $symbol" >&2
        echo "hint: remove target/ios-universal-sim and artifacts/HamIOSFFI.xcframework, then rerun scripts/ios/build-xcframework.sh." >&2
        exit 1
      fi
    done
  done
done < <(find "$XCFRAMEWORK" -name "libham_ios_ffi.a" -print0)

if [[ "$found" -eq 0 ]]; then
  echo "error: no libham_ios_ffi.a slices found inside $XCFRAMEWORK" >&2
  exit 1
fi

echo "HamIOSFFI linkage verification passed for $found slice(s)."
