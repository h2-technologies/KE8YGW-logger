# v0.3 Release Plan

Last audited: 2026-07-21

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
- Queue-aware cloud push acknowledgment for queued official events.
- Deterministic desktop restart/reconnect queue-drain coverage for interrupted
  sends, ordered queued official events, accepted-by-hash handling, and
  duplicate cloud replay.
- Deterministic shared sync golden scenarios for desktop-style crash recovery,
  transient network retry, duplicate replay, reordered delivery rejection,
  iOS-style pull/projection replay, clock-skewed event timestamps ordered by
  hashes, divergent heads, concurrent correction and tombstone/restore review,
  v0.2 legacy queue migration, and LAN revocation.
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
- Durable LAN trust store with explicit approval, hashed short-lived single-use
  pairing tokens, logbook-scoped trusted devices, replay nonce rejection, and
  immediate revocation.
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
  entering peer token/code/fingerprint values, completing reciprocal pairing,
  generating replacement auth codes, rotating LAN auth, and revoking selected
  trusted peers without prompt-only handling.
- Automatic IPv4/IPv6 multicast discovery worker that probes reachable peer
  identity before recording peers.
- Older trust records without an `auth_credential_id` remain readable but must
  be re-paired or rotated before protected LAN reads can authorize.

## Still Incomplete For v1

- Production iOS reciprocal LAN pairing UX and full release-device pairing
  qualification.
- Stronger LAN key-exchange hardening.
- Physical-device LAN and iOS Local Network permission validation.
- End-to-end cross-client branch review and reconciliation workflow beyond the
  current guided browser review surface and desktop/iOS corrective-event
  endpoints.
- Release-device iOS background task execution, poor-network behavior, and
  local-network permission validation.
- Real hosted web/desktop/iOS/self-hosted end-to-end device qualification,
  physical-device tests, and full migration matrix.

## Validation Targets

```powershell
cargo test -p ham-sync
cargo test -p ham-sync cross_client_golden
cargo test -p ham-sync desktop_queue_recovers_restart_and_drains_to_cloud_without_duplicates
cargo test -p ham-sync recover_or_initialize
cargo test -p ham-sync conflict_report
cargo test -p ham-gui
cargo test -p ham-ios-ffi
cargo test -p ham-ios-ffi sync_retry
just version-check
just ci
```

Production release tags still come only from validated semantic-version tags
contained in `main`; this document does not authorize a tag or publication.
