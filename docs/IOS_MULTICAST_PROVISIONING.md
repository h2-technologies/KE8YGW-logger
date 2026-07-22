# iOS Multicast Provisioning

Last audited: 2026-07-22

Native iOS LAN discovery uses `NWMulticastGroup` for the same secret-free
IPv4/IPv6 discovery packets used by desktop LAN discovery. The code path is
present, but Apple multicast networking is a controlled entitlement and must be
approved and provisioned by the Apple Developer account before it can be wired
into the signed app target.

## Current State

- `Info.plist` declares Local Network usage for paired-device sync and allows
  local networking.
- The Sync workspace can scan discovery packets, probe `/api/sync/state`, and
  list only peers whose probed identity matches the packet.
- `scripts/governance-check.ps1` validates the Local Network usage string,
  local-network ATS allowance, background retry declarations, Swift/plist
  background task identifier consistency, and generated Xcode artifact hygiene.
- A direct app-target multicast entitlement wiring attempt on PR #113 failed the
  App Store Connect archive check on July 22, 2026. Treat that as evidence that
  Apple account approval/provisioning is not yet ready in the current external
  signing environment.

## Maintainer Steps

1. Request or confirm Apple Developer approval for the Multicast Networking
   entitlement on bundle ID `com.h2technologiesllc.ke8ygw-logger`.
2. Enable the capability for the App ID and regenerate or refresh the
   provisioning profile used by App Store Connect and release-device builds.
3. Add `ios/KE8YGWLogger/KE8YGWLogger/KE8YGWLogger.entitlements` with:

   ```xml
   <key>com.apple.developer.networking.multicast</key>
   <true/>
   ```

4. Set `CODE_SIGN_ENTITLEMENTS =
   KE8YGWLogger/KE8YGWLogger.entitlements;` on the iOS app target Debug and
   Release configurations.
5. Extend `scripts/governance-check.ps1` to require the entitlement file and
   app-target reference.
6. Run local validation:

   ```powershell
   just governance-check
   python scripts/check_versions.py
   python scripts/check_docs_links.py
   just ci
   ```

7. Push and require the remote iOS simulator and App Store Connect archive
   checks to pass.
8. On physical devices, validate Local Network prompt behavior, multicast peer
   discovery, identity probing, reciprocal pairing, signed LAN reads, revocation
   rejection, and no silent event merge.

Until these steps pass, native iOS LAN discovery remains a release-device gate.
Manual peer URL pairing/pull and hosted/self-hosted sync remain the supported
CI-verifiable paths.
