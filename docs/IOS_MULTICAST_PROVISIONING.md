# iOS Multicast Provisioning

Last audited: 2026-08-28

Native iOS LAN discovery uses `NWMulticastGroup` for the same secret-free
IPv4/IPv6 discovery packets used by desktop LAN discovery. Apple multicast
networking is a controlled entitlement. That entitlement is now approved and
provisioned for the app, and it is declared on the signed app target.

## Current State

- The Apple Multicast Networking entitlement is approved for bundle ID
  `com.h2technologiesllc.ke8ygw-logger`, the capability is enabled on the App
  ID, and the provisioning profile used by App Store Connect and release-device
  builds has been regenerated to include it.
- `ios/KE8YGWLogger/KE8YGWLogger/KE8YGWLogger.entitlements` declares
  `com.apple.developer.networking.multicast`.
- The iOS app target sets `CODE_SIGN_ENTITLEMENTS` on both the Debug and
  Release configurations.
- `Info.plist` declares Local Network usage for paired-device sync and allows
  local networking.
- The Sync workspace can scan discovery packets, probe `/api/sync/state`, and
  list only peers whose probed identity matches the packet.
- `scripts/governance-check.ps1` validates the Local Network usage string,
  local-network ATS allowance, background retry declarations, Swift/plist
  background task identifier consistency, generated Xcode artifact hygiene, the
  multicast entitlement value, and the Debug/Release app-target entitlement
  references.

A direct app-target entitlement wiring attempt on PR #113 failed the App Store
Connect archive check on July 22, 2026, because account approval and
provisioning were not yet in place. That precondition has since been satisfied,
so the entitlement wiring has been restored.

## Remaining Validation

1. Require the remote iOS simulator and App Store Connect archive checks to
   pass on a build produced with the refreshed provisioning profile. This is
   the gating signal that approval and provisioning took effect.
2. On physical devices, validate Local Network prompt behavior, multicast peer
   discovery, identity probing, reciprocal pairing, signed LAN reads,
   revocation rejection, and no silent event merge.

Record the evidence for both against the Apple multicast provisioning and
physical LAN trust rows in
[v0.3 Sync Qualification Runbook](V0_3_SYNC_QUALIFICATION.md).

## Local Validation

```powershell
just governance-check
python scripts/check_versions.py
python scripts/check_docs_links.py
just ci
```

Until the archive check and physical-device validation above pass, release-device
iOS LAN discovery remains a qualification gate. Manual peer URL pairing/pull and
hosted/self-hosted sync remain the supported CI-verifiable paths.
