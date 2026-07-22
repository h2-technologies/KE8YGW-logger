# iOS App Store Readiness

Last audited: 2026-07-22

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
commands are present. Native Swift bridge methods and Sync workspace controls
now expose Rust-owned queue recovery, retry planning, retry result
classification, queue health, no-network planning, user-action retry states,
Rust-planned official event envelopes, self-hosted/logbook-scoped push
execution coordination, hosted `/api/v1/sync/*` execution routing,
accepted-prefix/rejected-tail retry result recording, Rust-owned pulled-event
apply through `sync.remote_events.apply`, self-hosted/logbook-scoped and hosted
pull request construction, native hosted/self-hosted and peer-identity-gated
signed LAN pull fetch -> Rust apply coordination, saved conflict-review records, selected
recovery paths, and structured conflict messages, and LAN trust
snapshot/issue/accept/trust/rotate/revoke controls that keep LAN auth secrets
in Keychain and store only credential IDs in Rust support state.
`sync.snapshot` decodes the durable local sync identity, and the bundle declares
Local Network usage plus local networking for paired-device sync. The Sync
workspace can also scan IPv4/IPv6 LAN discovery packets, derive peer URLs from
sender address plus advertised API port, probe `/api/sync/state`, and list only
peers whose probed device/session identity matches the packet.
The bundle now declares the permitted background retry `BGProcessingTask`
identifier and background processing mode. Scheduling is gated by Rust settings,
a valid sync URL, a Keychain sync token, and pending Rust queue work, and the
handler delegates to the existing Rust-plan -> Swift-transport -> Rust-result
executor.
The Sync API setting is persisted through the Rust settings schema and routes
native manual/background retry to either self-hosted logbook-scoped endpoints or
hosted `/api/v1/sync/*` endpoints.
Signing, provisioning, TestFlight, App Store metadata, privacy manifest,
physical-device validation, release-device BGTask execution, real
hosted/self-hosted native sync endpoint qualification, Apple multicast
entitlement/provisioning, and full v1 offline/sync/provider qualification
remain incomplete.

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
- Background processing is declared for offline sync retry; qualify the behavior
  on release devices before TestFlight.
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
- Offline queue records may include operation IDs, logbook IDs, and target
  entity IDs for recovery/conflict diagnostics; they must still exclude
  credential values, pairing codes, and provider secrets.
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

Do not request contacts, photos, Bluetooth, or other unused permissions
speculatively. Location, Local Network, notifications, and background
processing must remain tied to implemented native features and App Review copy.

## Review Checklist

- App logs in against production or review-safe demo credentials.
- Reviewer can select or create a logbook.
- Reviewer can create, edit, delete, and restore QSOs.
- Offline queue behavior can be demonstrated.
- The Sync workspace can display Rust queue health and request a no-network
  retry plan without marking queued mutations as sending.
- Rust-planned official event envelopes can be pushed through the configured
  sync-token transport path, with accepted/auth/divergence outcomes recorded
  back through Rust retry results and without event creation or validation in
  Swift.
- The Sync workspace can display saved Rust conflict-review records and
  recommended operator actions without merging history in Swift.
- The Sync workspace can issue and accept local LAN pairing codes,
  complete reciprocal pairing against an operator-entered peer URL,
  trust/revoke peers, and rotate LAN auth credentials without storing raw
  pairing codes or LAN auth secrets in Rust support state, logs, diagnostics,
  or SwiftData.
- The Sync workspace can scan LAN discovery packets, derive peer URLs from the
  packet source plus advertised API port, and list only peers whose
  `/api/sync/state` identity matches the discovery packet.
- The Sync workspace can pull from a trusted LAN peer URL by first checking
  the peer's published sync identity, then using signed protected LAN reads and
  Rust-owned event-chain verification before append.
- The app declares the Local Network permission copy used for paired-device
  LAN sync and allows local networking for those connections.
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
- Rust FFI tests cover recovery of a terminated `sending` mutation before
  retry planning, and Swift simulator tests cover no-network retry planning and
  auth-failure user-action classification plus fallback conflict-review
  creation/decoding, selected recovery-path resolution, event-envelope decoding,
  hosted/self-hosted push request construction, accepted retry execution,
  auth-failure result recording without token leakage, and partial-divergence
  retry-result splitting.
- ADIF document import/export is tested with Files and share sheet flows.
- Keychain values survive app restart and are cleared on sign-out.
- Privacy manifest is included in the archive.
- Crash-free smoke test is completed before external testing.
