# iOS App Store Readiness

Last audited: 2026-07-21

This checklist applies to the v1 native SwiftUI iOS app shipping with hosted
web and desktop on November 24, 2026.

## App Target

- Native SwiftUI iPhone and iPad app.
- App Store-ready Xcode project.
- TestFlight build pipeline.
- Same API contracts as hosted web and desktop.
- No PWA, pinned web app, or Home Screen install positioning.

Current repository status: native SwiftUI source, Rust FFI, Xcode project files,
build scripts, simulator CI, and Rust-owned offline queue plus conflict-review
commands are present. Signing, provisioning, TestFlight, App Store metadata,
privacy manifest, physical-device validation, release-safe background retry,
and full v1 offline/sync/provider qualification remain incomplete.

## Bundle and Signing

- Confirm bundle identifier.
- Configure signing team and provisioning profiles.
- Set app display name, version, and build number.
- Add app icons at required sizes.
- Add launch screen.
- Configure supported orientations for iPhone and iPad.
- Verify minimum iOS version.
- Produce archive builds locally and in CI.

## Entitlements

Only request entitlements that the native app uses:

- Keychain access for tokens and secrets.
- Network client access.
- iCloud documents only if explicitly used.
- Push notifications only if notification delivery is implemented.
- Background modes only if offline sync/retry needs them and the behavior is
  review-safe.
- Location only if native maps or station workflows require device location.

## Privacy Manifest

Prepare and maintain the app privacy manifest before TestFlight:

- Declare accessed required-reason APIs.
- Declare data collected by the app.
- Declare whether data is linked to the user.
- Declare whether data is used for tracking.
- Include third-party SDK privacy manifests where required.
- Verify that diagnostics, analytics, and crash reporting match the manifest.

## Data Handling

- Store auth tokens and provider secrets in Keychain.
- Do not store raw secrets in UserDefaults, logs, crash reports, diagnostics, or
  offline queue records.
- Encrypt or otherwise protect local app data where appropriate.
- Redact callsign/provider/account tokens from diagnostic exports.
- Explain cloud/self-hosted sync behavior in privacy text.
- Provide a way to sign out and clear local credentials.
- Ensure deleted/restored QSOs are explained as append-only log history where
  user-facing copy needs it.

## Permissions Copy

Permission prompts must be tied to native features:

- Files/document picker for ADIF import/export.
- Share sheet for ADIF export.
- Location only for features that use current device location.
- Notifications only for implemented reminders/sync/provider notifications.

Do not request location, notifications, contacts, photos, Bluetooth, local
network, or background modes speculatively.

## Review Checklist

- App logs in against production or review-safe demo credentials.
- Reviewer can select or create a logbook.
- Reviewer can create, edit, delete, and restore QSOs.
- Offline queue behavior can be demonstrated.
- ADIF import/export works through native document flows.
- POTA/SOTA and Net Control features are usable or clearly gated by account
  capability.
- Keychain is used for secrets.
- Privacy policy URL is available.
- Support URL is available.
- In-app account deletion or documented account deletion flow exists if
  account creation is available.
- App does not describe itself as a PWA or pinned website.
- TestFlight build can be produced from the Xcode project.

## TestFlight Readiness

- Archive succeeds in Release configuration.
- App launches on current iPhone and iPad simulators.
- App launches on a physical device.
- Login works against the intended environment.
- Offline queue is tested across app quit/relaunch.
- ADIF document import/export is tested with Files and share sheet flows.
- Keychain values survive app restart and are cleared on sign-out.
- Privacy manifest is included in the archive.
- Crash-free smoke test is completed before external testing.
