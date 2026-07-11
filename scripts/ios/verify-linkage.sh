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
while IFS= read -r -d '' lib; do
  found=$((found + 1))
  echo "Inspecting $lib"
  xcrun lipo -info "$lib"
  if ! xcrun nm -gU "$lib" | grep -q "_ham_ios_call_json_bytes"; then
    echo "error: $lib does not export ham_ios_call_json_bytes" >&2
    exit 1
  fi
  if ! xcrun nm -gU "$lib" | grep -q "_ham_ios_free_string"; then
    echo "error: $lib does not export ham_ios_free_string" >&2
    exit 1
  fi
done < <(find "$XCFRAMEWORK" -name "libham_ios_ffi.a" -print0)

if [[ "$found" -eq 0 ]]; then
  echo "error: no libham_ios_ffi.a slices found inside $XCFRAMEWORK" >&2
  exit 1
fi

echo "HamIOSFFI linkage verification passed for $found slice(s)."
