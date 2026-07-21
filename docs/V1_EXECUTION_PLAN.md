# v1 Execution Plan

Last audited: 2026-07-21

This plan converts the remaining issue #2 scope into a dependency-ordered
critical path for the November 24, 2026 v1 release. It does not add features
outside issue #2.

## Critical Path

1. Repository and release baseline
   - Complete version validation, documentation consistency, governance, CI,
     security, release-artifact naming, and residual repository-settings
     tracking.
   - Status: completed by the v1 baseline PR when merged.

2. Accounts, registration, and hosting modes
   - Status: server foundation completed when the account-foundation PR merges.
   - Implemented: personal/public/self-hosted hosting config; invite-only
     registration by default; administrator open/disabled switches; verified
     email; provider-neutral test/webhook email boundary; Cloudflare Turnstile
     fail-closed public registration; account recovery/deletion; session
     expiry/rotation/logout-all; device revocation; request IDs; audits; and
     durable rate limits.
   - Remaining: hosted web, desktop, and iOS UX wiring; production email
     provider/domain validation; Turnstile site/secret keys; privacy/support
     URLs; infrastructure sizing; retention/monitoring; and deployment secrets.

3. Offline-first sync and reconciliation
   - Finish desktop/iOS offline operation, queued mutations, replay, safe
     reconciliation, divergence review, and user-directed conflict handling.
   - Add real LAN peer-to-peer HTTP transport and trust pairing while preserving
     the existing verification rules.
   - Blockers: local network permission behavior on iOS, trust-pairing UX, and
     test devices for multi-device scenarios.

4. Production provider qualification
   - Complete QRZ, QRZ Logbook, LoTW/TQSL, eQSL, Club Log, POTA, SOTAWatch, DX
     Cluster/RBN, maps, and propagation.
   - Remove v1-required stub-backed paths or gate them out of release flows.
   - Blockers: provider credentials, LoTW certificate/TQSL signing model,
     SOTAWatch approval/terms, RBN/DX operating limits, map cache licensing, and
     release-runner live-test secrets.

5. Client surfaces
   - Hosted web: finish account/session/logbook UX, provider setup, maps,
     contesting, EmComm, backup/restore, and operations feedback.
   - Desktop: finish signed Windows/macOS/Linux packaging, native update policy,
     offline-first flows, provider credentials, maps, contesting, and EmComm.
   - iOS: finish offline/reconciliation, maps/offline regions, provider setup,
     contesting, EmComm, native ADIF/backup/diagnostics, Keychain, and App Store
     polish.
   - Blockers: Apple signing/provisioning, Microsoft Trusted Signing,
     notarization credentials, update server/channel configuration, and physical
     test devices.

6. Maps, cached regions, and propagation
   - Select cacheable map source, implement desktop/iOS cached region management,
     enforce provider terms, and complete propagation provider integration.
   - Blockers: map tile/vector provider approval and cache/license terms.

7. Contesting
   - Implement contest engine, exchange templates, dupes, scoring, multiplier
     projections, Cabrillo export, Field Day, Winter Field Day, generic
     serial/grid templates, and December/January contest packs.
   - Depends on stable account/logbook/session, offline queue, and keyboard-first
     client flows.

8. EmComm
   - Implement incidents, personnel, assignments, ICS 211, 213, 213RR, 214, and
     message/communications records.
   - Depends on account/logbook roles, offline/reconciliation, forms storage, and
     export/backup rules.

9. Operations and deployment qualification
   - Finish public/personal/self-hosted deployment docs, backup/restore
     validation, rate limits, observability, diagnostic retention, release notes,
     rollback, and support procedures.
   - Blockers: production infrastructure, DNS/TLS, email, Turnstile, storage,
     signing/notarization, and protected GitHub environments.

10. Release qualification
    - Run full CI/security/iOS/container/Tauri/release workflows, live-provider
      gated validation, offline/reconciliation scenarios, signed updater tests,
      backup/restore tests, TestFlight/App Store review, beta soak, and final
      release-candidate approval.
    - Blockers: maintainer-controlled GitHub repository settings, Apple review,
      Microsoft Trusted Signing, notarization, provider approvals, and stable
      seven-day release candidate.

## Parallel Workstreams

- Hosted web, desktop, and iOS account UX can proceed in parallel now that the
  shared auth/session contracts are fixed.
- Desktop signing/updater can proceed in parallel with iOS signing/TestFlight
  after version/artifact validation is stable.
- Maps/provider licensing can proceed in parallel with contesting and EmComm
  domain modeling.
- Browser/iOS UI coverage can proceed in parallel after account, sync, and
  provider contracts stop changing.
- Operations docs and runbooks can proceed continuously, but final values depend
  on production infrastructure, signing, and provider decisions.

## External Blockers

- Apple Developer team, provisioning profiles, App Store Connect app metadata,
  privacy policy URL, support URL, TestFlight, physical-device validation, and
  App Store review.
- Microsoft Trusted Signing and macOS notarization credentials.
- Production DNS/TLS, hosting, storage, backup destination, observability, and
  protected environments.
- Email provider and Cloudflare Turnstile credentials.
- Provider credentials/approval for QRZ, QRZ Logbook, LoTW/TQSL, eQSL, Club Log,
  POTA, SOTAWatch, DX Cluster/RBN, maps, and propagation.
- GitHub branch/ruleset protection, CODEOWNERS enforcement, required checks,
  private vulnerability reporting, Dependabot routing, and protected tag rules.

## Next Three Goals

1. Implement desktop/iOS sync and offline reconciliation, including LAN trust
   pairing.
2. Complete production provider qualification and release-runner live validation
   for the issue #2 provider set.
3. Wire hosted web, desktop, and iOS UI flows to the implemented account,
   session, recovery, device, and admin APIs.
