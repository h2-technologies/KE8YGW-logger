# v0.3 Sync Qualification Runbook

Last audited: 2026-07-22

This runbook covers the remaining offline-sync qualification gates for epic #5
after the v0.3.0 repository baseline is merged. It does not add product scope.
It converts the incomplete acceptance criteria in #28, #29, #30, and #31 into a
repeatable evidence checklist.

## Preconditions

- PR #113 or equivalent v0.3.0 offline-sync baseline is merged to `dev`.
- The validating build uses workspace version `0.3.0` and passes:

  ```powershell
  just ci
  just release
  cargo tauri build --no-bundle
  ```

- Remote GitHub checks for CI, security, iOS simulator, Tauri, and sync-server
  container validation are green on the tested commit.
- The tested iOS app is installed from the same commit that produced the
  passing remote checks.
- Test logbooks contain only synthetic operator/test data.
- Test credentials are disposable and may be revoked after the run.

## Required Test Matrix

| Gate | Issue | Platforms | Evidence required |
| --- | --- | --- | --- |
| iOS background retry | #28 | Physical iPhone or iPad, hosted/self-hosted sync endpoint | BGTask fires on a release-device build, reads the sync token from Keychain, drains queued accepted events, stops on auth/validation/divergence user-action failures, refreshes SwiftData from Rust projections, and records redacted diagnostics. |
| Poor-network iOS retry | #28 | Physical iPhone or iPad | Airplane-mode, packet-loss, captive-portal, and endpoint-timeout scenarios leave queued work retrying or user-action-required without duplicate official events. |
| Native endpoint execution | #28, #31 | iOS plus hosted and self-hosted endpoints | Manual push, manual pull, and background Auto Pull succeed against both endpoint styles; event chains verify before and after each run. |
| Cross-client conflict review | #29, #31 | Hosted web/server, desktop, iOS | Divergent desktop/iOS histories create conflict reports, browser and iOS surfaces show required action, unsafe pull is rejected, corrective events are appended through proposals, and histories converge without editing old events. |
| Apple multicast provisioning | #30 | Apple Developer, Xcode Cloud, physical iOS devices | Multicast entitlement is approved, provisioned, wired into the app target, and passes App Store Connect archive. Follow `IOS_MULTICAST_PROVISIONING.md`. |
| Physical LAN trust | #30, #31 | Two desktop peers, iOS plus desktop peer, IPv4 and IPv6 networks where available | Pairing tokens are single-use and expire, generated endpoint auth is distinct from one-time pairing codes, trusted peers can read signed LAN event ranges, revoked peers fail immediately, replayed nonces fail, wrong-logbook peers fail, and untrusted peers cannot read or write. |
| Migration and recovery matrix | #31 | Hosted web/server, desktop, iOS, self-hosted server | v0.2 absent/legacy queue state, corrupt queue state, interrupted sending state, partial push accepted-prefix/rejected-tail, expired auth, revoked auth, restore, duplicate delivery, reordered delivery, clock skew, and concurrent edits preserve valid chains and projections. |

## Automated Evidence Already Covered

These rows still need physical-device confirmation, but the repository-side
behavior they depend on is proven by deterministic tests. Re-run them before a
qualification pass and record the output instead of re-deriving the behavior by
hand.

| Behavior | Command | Test |
| --- | --- | --- |
| Hosted push refuses cross-logbook event batches | `cargo test -p ham-server sync_push_` | `sync_push_rejects_events_scoped_to_another_logbook` |
| Hosted push reports divergence with the shared status vocabulary and reconciles after a pull | `cargo test -p ham-server sync_push_` | `sync_push_reports_divergence_with_the_shared_replication_vocabulary` |
| Self-hosted HTTP push refuses a divergent branch, keeps the durable head, and recovers by pull-then-reapply | `cargo test -p ham-server self_hosted_wire` | `self_hosted_wire_endpoint_rejects_divergent_branch_and_reconciles_after_pull` |
| iOS first-launch queue initialization, legacy `version: 0` migration, corrupt-queue quarantine, and interrupted atomic-write promotion | `cargo test -p ham-ios-ffi sync_offline_queue_recover` | `sync_offline_queue_recover_*` |

Known harness limitation: the durable self-hosted metadata store keeps its
SurrealKV lock for the life of the process, so a server restart cannot be
exercised in-process. Restart, upgrade, and restore rows must be qualified with
a real process restart on the target machine.

## Evidence To Capture

- Tested commit SHA and build number.
- Device models, OS versions, network type, and endpoint style.
- Redacted queue-health snapshots before and after each scenario.
- Event counts, head hashes, and `verify_chain` results for every participating
  logbook before and after push/pull/recovery.
- Screenshots or screen recordings for user-action-required conflict and retry
  states.
- App Store Connect archive result for any entitlement or signing change.
- The exact command output for local validation and any remote workflow links.

Do not capture raw sync tokens, LAN auth secrets, pairing codes after use,
provider credentials, private station data, or full unredacted diagnostic
bundles in public issues or pull requests.

## Pass Criteria

- #28 can close only when release-device iOS background retry, poor-network
  behavior, hosted/self-hosted native endpoint execution, and projection refresh
  are proven by physical-device evidence.
- #29 can close only when web/server, desktop, and iOS conflict-review and
  corrective-event workflows are qualified end to end with no silent merge.
- #30 can close only when LAN is either fully qualified with Apple multicast
  provisioning and physical trust/revocation evidence, or the v1 scope is
  formally changed to remove production LAN discovery.
- #31 can close only when the cross-client migration/recovery matrix passes
  across hosted web/server, desktop, iOS, and self-hosted sync endpoints with
  verified chains and no data loss.

Until all rows pass, keep #28-#31 open and reference them as partial.
