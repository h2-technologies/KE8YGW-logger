# v0.3 Release Plan

Last audited: 2026-07-22

`0.3.0` is the offline-sync foundation baseline for the locked November 24,
2026 v1 release. It is not the complete v1 product.

## Implemented In v0.3.0

- Versioned offline mutation envelopes in `ham-sync` with operation, device,
  client, logbook, optional target entity, ordering, dependency, idempotency,
  correlation, retry, and queue-health metadata.
- Crash-recoverable JSON queue support store with safe rejection of unsupported
  queue and mutation schema versions.
- Shared queue recovery report for desktop and iOS that initializes absent
  v0.2 queue state, migrates conservative legacy `version: 0` queue records,
  promotes interrupted atomic writes, and quarantines corrupt queue JSON without
  exposing machine-specific paths.
- Desktop queue integration for QSO, activation, Net Control, and
  station-profile support-state mutations.
- iOS FFI queue integration for QSO, activation, Net Control, station-profile,
  and equipment commands.
- iOS FFI background retry planning and result classification commands that
  recover interrupted writes, bound native background batches, mark planned
  work `sending`, return official event envelopes/hashes for native transport,
  back off transient failures, and stop retry on auth, validation, divergence,
  missing-local-event, and permanent failures.
- Native Swift bridge methods and typed sync snapshots now expose queue
  recovery, retry planning, retry results, queue health, affected mutations,
  Rust-planned official event envelopes, self-hosted/logbook-scoped push
  execution coordination, hosted `/api/v1/sync/*` endpoint routing, and
  partial-acceptance retry-result handling to the iOS Sync workspace without
  moving queue ordering, event creation, or failure classification out of Rust.
- Additive `sync_endpoint_style` settings support for native iOS manual and
  background retry so self-hosted/logbook-scoped sync remains the default while
  hosted `/api/v1/sync/*` execution can be selected without URL guessing.
- Native iOS background retry registration for a permitted
  `BGProcessingTask` identifier, with bundle background-processing
  declarations and a scheduler eligibility policy that requires enabled Rust
  settings, a valid sync URL, a Keychain sync token, and either pending Rust
  queue work or Auto Pull before scheduling the same Rust-plan ->
  Swift-transport -> Rust-result executor. Background Auto Pull runs only after
  a clean accepted push or no-ready-events push plan and applies fetched remote
  events through `sync.remote_events.apply`. Simulator tests cover both Auto
  Pull paths: after a clean accepted push and after a no-ready queue plan where
  no push transport should run.
- Governance validation now enforces the iOS Local Network usage string,
  local-network ATS allowance, background-processing mode, permitted sync retry
  task identifier matching the Swift runtime constant, and absence of tracked
  generated Xcode/iOS artifacts.
- Native iOS manual hosted/self-hosted pull, trusted LAN pull, and background
  Auto Pull refresh the SwiftData QSO cache from the Rust `qso.list`
  projection after Rust accepts remote events. Swift still treats SwiftData as
  a cache and does not validate, create, or merge official history.
- Shared sync golden coverage proves a partial push can accept a valid prefix,
  stop the rejected tail as `user_action_required`, avoid local or cloud
  duplicates, and complete the reviewed tail by acknowledged event hash.
- Shared sync golden coverage proves a revoked cloud-auth push appends nothing
  remotely, stops queued work as `user_action_required`, plans no unattended
  retry, and resumes only after re-pairing plus accepted-hash acknowledgment.
- Shared sync golden coverage proves bounded cloud sync sessions carry
  `expires_at`, expire safely, append nothing remotely after expiry, stop queued
  work as `user_action_required`, and resume only after re-pairing plus
  accepted-hash acknowledgment.
- Shared pull application accepts verified missing-tail responses that directly
  follow the actual local head as well as full remote chains, and iOS exposes
  `sync.remote_events.apply` so native transports can apply pulled official
  envelopes through Rust-owned hash-chain verification and projection/sync
  refresh. Swift can build self-hosted/logbook-scoped and hosted pull requests,
  fetch missing events through native transport, and pass the response back to
  Rust for verification and local application.
- Queue-aware cloud push acknowledgment for queued official events.
- Deterministic desktop restart/reconnect queue-drain coverage for interrupted
  sends, ordered queued official events, accepted-by-hash handling, and
  duplicate cloud replay.
- Desktop cloud reconnect auto-drain when `auto_push_enabled` is set, with a
  queue-only guard so accepted but unqueued local history is not pushed
  implicitly.
- Deterministic shared sync golden scenarios for desktop-style crash recovery,
  transient network retry, duplicate replay, reordered delivery rejection,
  iOS-style pull/projection replay, partial push accepted-prefix/rejected-tail
  queue recovery, revoked and expired cloud-auth user-action recovery,
  clock-skewed event timestamps ordered by hashes, divergent heads, concurrent
  correction and tombstone/restore review, v0.2 legacy queue migration, and LAN
  revocation.
- Self-hosted sync-server route and loopback TCP wire coverage for pairing,
  scoped logbook listing, durable canonical-event push, duplicate replay,
  missing-event pull, invalid tokens, and expired sync-token sessions.
- Hosted ham-server binary loopback TCP wire coverage for admin bootstrap,
  proposal-backed QSO creation, hosted sync pull, duplicate hosted sync push,
  and durable JSONL official-event storage without duplicate replay.
- Structured conflict reports for divergent previews, dependency-blocked queued
  mutations, unsupported remote schema versions, concurrent QSO corrections, and
  remote QSO tombstone/restore events that overlap local pending mutations.
- Durable manual conflict-review records for desktop and iOS bridge clients,
  with explicit recovery-path decisions and validation that rejects unsafe
  divergent pulls.
- Corrective-event conflict-review resolution commands for desktop and iOS
  bridge clients. They submit explicit corrective proposals through the normal
  proposal pipeline, persist the offline mutation, append official events, and
  resolve the review with the generated event hashes.
- Guided browser conflict-review surface for saved review selection, structured
  conflict summaries, explicit recovery-path decisions, and form-based
  corrective QSO note events through the Rust desktop endpoints.
- Native Swift conflict-review snapshot decoding and Sync workspace display for
  saved open reviews, recommended actions, peer IDs, structured conflict
  messages, and selected recovery-path results.
- Durable LAN trust store with explicit approval, hashed short-lived single-use
  pairing tokens, logbook-scoped trusted devices, replay nonce rejection, and
  immediate revocation.
- Durable local sync identity store for desktop and iOS so trusted-peer device
  IDs survive restart while discovery sessions remain ephemeral.
- Manual direct LAN HTTP peer add, handshake, preview, and trusted pull between
  GUI instances using `/api/sync/state`, `/api/sync/get-head`, and
  `/api/sync/events-since`.
- Trust-scoped LAN HTTP read endpoint authorization for logbook/head/event
  requests using requester device IDs, fresh replay nonce headers, and
  HMAC-SHA256 request signatures backed by credential-store secrets.
- GUI LAN auth credential rotation/recovery for trusted peers, with replacement
  secrets stored through `CredentialStore` and old credential references
  deleted after trust state updates.
- Guided browser LAN pairing/trust panel for issuing local one-time codes,
  entering peer token/code/fingerprint values, completing reciprocal pairing
  with a generated endpoint auth code distinct from the one-time pairing code,
  generating replacement auth codes, rotating LAN auth, and revoking selected
  trusted peers without prompt-only handling.
- Native iOS LAN trust bridge and Sync workspace controls for Rust-owned
  trust snapshots, local one-time code issue and acceptance, direct peer trust,
  Keychain-backed LAN auth credential rotation, revocation, and reciprocal
  pairing against an operator-entered peer URL. The Sync workspace also has a
  multicast discovery scanner that listens for the same IPv4/IPv6 discovery
  packets, derives peer URLs from sender address plus advertised API port,
  probes `/api/sync/state`, and lists only peers whose probed device/session
  identity matches the packet. `sync.snapshot` returns the
  durable local identity, and the bundle declares Local Network usage plus
  local networking for paired-device sync. Pairing codes are returned only by
  the issue command; snapshots and `lan-trust.json` do not store raw pairing
  codes or LAN auth secrets. The reciprocal URL flow probes `/api/sync/state`,
  posts the peer token/code plus a generated endpoint auth code to the peer
  accept endpoint, and then stores only a local Keychain credential reference
  in Rust trust state after the remote peer accepts. The native Swift LAN pull
  executor probes `/api/sync/state` to verify the selected trusted peer
  identity before it signs protected `get-head` and `events-since` requests
  with the Keychain-backed auth secret, then passes pulled official envelopes
  through `sync.remote_events.apply` for Rust verification before append.
- Automatic IPv4/IPv6 multicast discovery worker that probes reachable peer
  identity before recording peers.
- Older trust records without an `auth_credential_id` remain readable but must
  be re-paired or rotated before protected LAN reads can authorize.

## Still Incomplete For v1

- The remaining epic #5 release-device checks are tracked in
  [v0.3 Sync Qualification Runbook](V0_3_SYNC_QUALIFICATION.md). Keep #28-#31
  open until that runbook has physical-device and endpoint evidence for each
  applicable row.
- Apple multicast entitlement/provisioning and full release-device iOS LAN
  discovery/pairing qualification. See
  [iOS Multicast Provisioning](IOS_MULTICAST_PROVISIONING.md) for the
  maintainer-controlled Apple entitlement gate and validation sequence.
- Physical-device LAN and iOS Local Network permission prompt validation.
- Release-device cross-client branch review and reconciliation workflow beyond
  the current deterministic shared golden tests, guided browser review surface,
  native saved-review display, desktop/iOS review stores, and desktop/iOS
  corrective-event endpoints.
- Release-device iOS background task execution, poor-network behavior, and
  local-network permission validation beyond the current bundle declarations,
  scheduler policy, SwiftData projection-refresh hook, and simulator-safe Swift
  tests, including release-device hosted/self-hosted native push/pull transport
  execution.
- Release-device hosted web/desktop/iOS/self-hosted end-to-end qualification,
  physical-device tests, and full migration matrix.

## Validation Targets

```powershell
cargo test -p ham-sync
cargo test -p ham-sync cross_client_golden
cargo test -p ham-sync desktop_queue_recovers_restart_and_drains_to_cloud_without_duplicates
cargo test -p ham-gui cloud_connect_auto_push
cargo test -p ham-sync recover_or_initialize
cargo test -p ham-sync conflict_report
cargo test -p ham-gui
cargo test -p ham-ios-ffi
cargo test -p ham-ios-ffi sync_retry
cargo test -p ham-ios-ffi sync_retry_plan_recovers_terminated_send_and_blocks_without_network
just version-check
just governance-check
just ci
```

Production release tags still come only from validated semantic-version tags
contained in `main`; this document does not authorize a tag or publication.
