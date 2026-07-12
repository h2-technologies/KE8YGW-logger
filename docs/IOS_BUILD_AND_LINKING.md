# iOS Build And Rust Linking

Last updated: 2026-07-10

This document describes the reproducible macOS workflow for building the Rust
FFI library and linking it into the native iOS app.

## Requirements

- macOS with Xcode 15 or newer selected by `xcode-select`.
- Rust stable with `rustup`.
- iOS deployment target: 17.0, matching `KE8YGWLogger.xcodeproj`.
- Supported Rust Apple targets:
  - `aarch64-apple-ios`
  - `aarch64-apple-ios-sim`
  - `x86_64-apple-ios` when the Rust toolchain provides it

## Build The XCFramework

From the repository root on macOS:

```bash
bash scripts/ios/install-targets.sh
CONFIGURATION=Release bash scripts/ios/build-xcframework.sh
bash scripts/ios/verify-linkage.sh
```

The deterministic output path is:

```text
artifacts/HamIOSFFI.xcframework/
artifacts/ios/link/Release-iphoneos/libham_ios_ffi.a
artifacts/ios/link/Release-iphonesimulator/libham_ios_ffi.a
```

`artifacts/` is ignored by Git. Do not commit machine-generated framework
output unless the repository intentionally changes that policy.

## Xcode Integration

The `KE8YGWLogger` target has a pre-link build phase named
`Build HamIOSFFI XCFramework`. It runs:

```bash
bash "$SRCROOT/../../scripts/ios/build-xcframework.sh"
```

This keeps the framework generation documented and reproducible. Xcode does not
directly link a generated `HamIOSFFI.xcframework` file reference, because a
clean checkout has no `artifacts/` directory yet and Xcode can fail before the
build phase creates it. Instead, the app target links `-lham_ios_ffi` from:

```text
$(SRCROOT)/../../artifacts/ios/link/$(CONFIGURATION)$(EFFECTIVE_PLATFORM_NAME)
```

The build script copies the correct device or simulator static library to that
path before the link step. It still assembles
`artifacts/HamIOSFFI.xcframework/` for CI, packaging, and architecture
inspection.

Set `SKIP_RUST_XCFRAMEWORK_BUILD=1` only when the generated static library and
framework already exist and you are intentionally testing Xcode without
rebuilding Rust.

## Build And Test Locally

```bash
open ios/KE8YGWLogger/KE8YGWLogger.xcodeproj
```

Select the shared `KE8YGWLogger` scheme, choose an iOS 17+ simulator, then
build or test.

Command-line simulator build:

```bash
xcodebuild \
  -project "ios/KE8YGWLogger/KE8YGWLogger.xcodeproj" \
  -scheme "KE8YGWLogger" \
  -destination "platform=iOS Simulator,name=iPhone 16" \
  CODE_SIGNING_ALLOWED=NO \
  build
```

Command-line unit tests:

```bash
xcodebuild \
  -project "ios/KE8YGWLogger/KE8YGWLogger.xcodeproj" \
  -scheme "KE8YGWLogger" \
  -destination "platform=iOS Simulator,name=iPhone 16" \
  CODE_SIGNING_ALLOWED=NO \
  test
```

Generic device build without signing:

```bash
xcodebuild \
  -project "ios/KE8YGWLogger/KE8YGWLogger.xcodeproj" \
  -scheme "KE8YGWLogger" \
  -destination "generic/platform=iOS" \
  CODE_SIGNING_ALLOWED=NO \
  build
```

For a signed device archive or TestFlight build, configure an Apple development
team and signing assets in Xcode or a separate manually triggered CI workflow.
Do not put signing credentials in pull-request workflows.

## Inspect Architectures

```bash
find artifacts/HamIOSFFI.xcframework -name libham_ios_ffi.a -print
bash scripts/ios/verify-linkage.sh
```

`verify-linkage.sh` checks that `ham_ios_call_json_bytes` and
`ham_ios_free_string` are exported and prints `lipo` architecture information
for every XCFramework slice.

## Troubleshooting

- `xcode-select` points at command line tools only: run
  `sudo xcode-select -s /Applications/Xcode.app/Contents/Developer`.
- Missing Rust target: run `bash scripts/ios/install-targets.sh`.
- `No XCFramework found at artifacts/hamiosffi.xcframework`: pull the latest
  project file or remove any stale local `HamIOSFFI.xcframework` reference from
  Link Binary With Libraries. The app now links the generated static library
  under `artifacts/ios/link/...`; the XCFramework is still generated but is not
  a direct Xcode file reference.
- Missing symbol at link: rebuild with
  `CONFIGURATION=Release bash scripts/ios/build-xcframework.sh`, then rerun
  `bash scripts/ios/verify-linkage.sh`.
- Stale framework: remove `artifacts/HamIOSFFI.xcframework` and rerun the build
  script.
- Simulator architecture mismatch: confirm the simulator slice contains
  `arm64` on Apple Silicon and `x86_64` when that target is installed.
- Device archive contains simulator slices: archive should consume the
  `ios-arm64` XCFramework library slice only; verify with Xcode archive logs and
  `verify-linkage.sh`.
- Header or module map failure: confirm
  `crates/ham-ios-ffi/include/ham_ios_ffi.h` and
  `crates/ham-ios-ffi/include/module.modulemap` were copied to
  `artifacts/ios/include`.
- Keychain behavior: provider secrets remain in the iOS Keychain layer. Do not
  store tokens in SwiftData, UserDefaults, diagnostics JSON, or Rust debug logs.
- Notifications/background modes: local notification authorization exists, but
  unrestricted background execution is not assumed. Validate required
  capabilities in Xcode before enabling distribution builds.

## Validation Status

This repository pass was performed in a Windows environment. Rust checks ran
locally. Xcode, iOS Simulator, device builds, archives, and TestFlight upload
were not executed here.
