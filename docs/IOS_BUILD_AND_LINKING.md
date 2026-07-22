# iOS Build And Rust Linking

Last updated: 2026-07-22

This document describes the reproducible macOS workflow for building the Rust
FFI library and linking it into the native iOS app.

## Requirements

- macOS with Xcode 15 or newer selected by `xcode-select`.
- Rust stable with `rustup`.
- The scripts source `~/.cargo/env` when present and prepend common Rust/Homebrew
  locations (`~/.cargo/bin`, `/opt/homebrew/bin`, `/usr/local/bin`) because
  Xcode archive shells do not load normal interactive shell profiles.
- Xcode Cloud runs `ios/KE8YGWLogger/ci_scripts/ci_post_clone.sh` before
  `xcodebuild`. That script installs the pinned Rust toolchain and targets,
  sets Cargo network retries, and prefetches the locked Cargo dependency graph
  with retry before the archive action starts.
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

This is the production/package validation path. It builds the device slice,
the Apple Silicon simulator slice, and the Intel simulator slice when the Rust
toolchain supports it, then assembles `artifacts/HamIOSFFI.xcframework/`.

## Fast Simulator CI Path

Ordinary pull-request and developer validation should avoid the full
XCFramework path. Build and stage only the Apple Silicon simulator static
library in Debug with low Rust debug info, then let a single `xcodebuild test`
build the Swift target and run unit tests:

```bash
CONFIGURATION=Debug \
IOS_RUST_TARGETS=aarch64-apple-ios-sim \
CARGO_PROFILE_DEV_DEBUG=1 \
CARGO_PROFILE_DEV_SPLIT_DEBUGINFO=off \
bash scripts/ios/build-rust.sh

SKIP_RUST_XCFRAMEWORK_BUILD=1 \
xcodebuild \
  -project "ios/KE8YGWLogger/KE8YGWLogger.xcodeproj" \
  -scheme "KE8YGWLogger" \
  -configuration Debug \
  -destination "platform=iOS Simulator,name=iPhone 16" \
  CODE_SIGNING_ALLOWED=NO \
  test
```

That stages the library at:

```text
artifacts/ios/link/Debug-iphonesimulator/libham_ios_ffi.a
```

Do not add a separate `xcodebuild build` before `xcodebuild test`; the test
action already performs the build.

## Xcode Integration

The `KE8YGWLogger` target has a pre-link build phase named
`Build HamIOSFFI Rust Library`. For Debug simulator builds, it runs:

```bash
IOS_RUST_TARGETS="${IOS_RUST_TARGETS:-aarch64-apple-ios-sim}" \
bash "$SRCROOT/../../scripts/ios/build-rust.sh"
```

For other configurations and platforms, it runs:

```bash
bash "$SRCROOT/../../scripts/ios/build-xcframework.sh"
```

This keeps the fast developer path cheap while preserving the full
XCFramework-producing path for device, archive, and release validation. Xcode
does not directly link a generated `HamIOSFFI.xcframework` file reference,
because a clean checkout has no `artifacts/` directory yet and Xcode can fail
before the build phase creates it. Instead, the app target links
`-lham_ios_ffi` from:

```text
$(SRCROOT)/../../artifacts/ios/link/$(CONFIGURATION)$(EFFECTIVE_PLATFORM_NAME)
```

The build script copies the correct device or simulator static library to that
path before the link step. The production script still assembles
`artifacts/HamIOSFFI.xcframework/` for packaging and architecture inspection,
but the `.xcframework` directory is not declared as an Xcode build phase
output.

Set `SKIP_RUST_XCFRAMEWORK_BUILD=1` when the generated static library has
already been staged for Xcode and you are intentionally testing Xcode without
rebuilding Rust. The GitHub iOS workflow uses this after its one explicit
Rust FFI build step.

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
CONFIGURATION=Debug IOS_RUST_TARGETS=aarch64-apple-ios-sim bash scripts/ios/build-rust.sh
SKIP_RUST_XCFRAMEWORK_BUILD=1 \
xcodebuild \
  -project "ios/KE8YGWLogger/KE8YGWLogger.xcodeproj" \
  -scheme "KE8YGWLogger" \
  -configuration Debug \
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
- Xcode Cloud fails while fetching a Cargo dependency such as `serde`: rerun the
  Xcode Cloud build if the post-clone retry exhausted all attempts, then inspect
  the App Store Connect logs artifact for network, crates.io, or cache-service
  failures before changing repository code.
- `required tool 'rustup' was not found on PATH`: install Rust from
  `https://rustup.rs`, then rerun the build. The build scripts load
  `~/.cargo/env` automatically for Xcode, so do not add developer-specific
  `/Users/.../.cargo/bin` paths to the Xcode project.
- `No XCFramework found at artifacts/hamiosffi.xcframework`: pull the latest
  project file or remove any stale local `HamIOSFFI.xcframework` reference from
  Link Binary With Libraries. The app now links the generated static library
  under `artifacts/ios/link/...`; the XCFramework is still generated but is not
  a direct Xcode file reference or build phase output. If Xcode keeps reporting
  the old path, close Xcode, clean DerivedData for `KE8YGWLogger`, and reopen
  the project.
- Missing symbol at link: rebuild with
  `CONFIGURATION=Release bash scripts/ios/build-xcframework.sh`, then rerun
  `bash scripts/ios/verify-linkage.sh`.
- `does not export ham_ios_call_json_bytes`: remove stale generated archives
  with `rm -rf target/ios-universal-sim artifacts/HamIOSFFI.xcframework
  artifacts/ios/link`, then rerun the XCFramework build script. The build
  script refreshes static archive indexes with `ranlib`; the verifier checks
  each simulator/device architecture independently.
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
- Notifications/background modes: local notification authorization exists, and
  offline sync retry declares a `BGProcessingTask` identifier plus processing
  mode. Unrestricted background execution is not assumed; validate release
  device behavior and required capabilities in Xcode before enabling
  distribution builds.

## Validation Status

This repository pass was performed in a Windows environment. Rust checks ran
locally. Xcode, iOS Simulator, device builds, archives, and TestFlight upload
were not executed here.
