#!/bin/sh
set -euo pipefail
if [ "${SKIP_RUST_XCFRAMEWORK_BUILD:-}" = "1" ]; then
  echo "Skipping Rust XCFramework build because SKIP_RUST_XCFRAMEWORK_BUILD=1"
  exit 0
fi
bash "$SRCROOT/../../scripts/ios/build-xcframework.sh"

