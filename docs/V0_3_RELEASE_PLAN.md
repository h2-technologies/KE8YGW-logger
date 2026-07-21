# v0.3 Release Plan

Last audited: 2026-07-21

`0.3.0` is the offline-sync foundation baseline for the locked November 24,
2026 v1 release. It is not the complete v1 product.

## Implemented In v0.3.0

- Versioned offline mutation envelopes in `ham-sync` with operation, device,
  client, logbook, ordering, dependency, idempotency, correlation, retry, and
  queue-health metadata.
- Crash-recoverable JSON queue support store with safe rejection of unsupported
  queue and mutation schema versions.
- Desktop queue integration for QSO, activation, Net Control, and
  station-profile support-state mutations.
- iOS FFI queue integration for QSO, activation, Net Control, station-profile,
  and equipment commands.
- Queue-aware cloud push acknowledgment for queued official events.
- Deterministic desktop restart/reconnect queue-drain coverage for interrupted
  sends, ordered queued official events, accepted-by-hash handling, and
  duplicate cloud replay.
- Structured conflict reports for divergent previews and dependency-blocked
  queued mutations.
- Durable manual conflict-review records for desktop and iOS bridge clients,
  with explicit recovery-path decisions and validation that rejects unsafe
  divergent pulls.
- Durable LAN trust store with explicit approval, hashed short-lived single-use
  pairing tokens, logbook-scoped trusted devices, replay nonce rejection, and
  immediate revocation.
- Manual direct LAN HTTP peer add, handshake, preview, and trusted pull between
  GUI instances using `/api/sync/state`, `/api/sync/get-head`, and
  `/api/sync/events-since`.
- Trust-scoped LAN HTTP read endpoint authorization for logbook/head/event
  requests using requester device IDs, fresh replay nonce headers, and
  HMAC-SHA256 request signatures backed by credential-store secrets.
- Automatic IPv4/IPv6 multicast discovery worker that probes reachable peer
  identity before recording peers.
- Older trust records without an `auth_credential_id` remain readable but must
  be re-paired before protected LAN reads can authorize.

## Still Incomplete For v1

- Production reciprocal pairing UX across desktop and iOS.
- LAN auth credential rotation/recovery and stronger key-exchange hardening.
- Physical-device LAN and iOS Local Network permission validation.
- Corrective-event conflict-resolution UX on top of the durable manual review
  commands.
- Release-device iOS background retry and local-network permission validation.
- Full cross-client recovery/migration scenarios across hosted web, desktop,
  iOS, and self-hosted sync.

## Validation Targets

```powershell
cargo test -p ham-sync
cargo test -p ham-sync desktop_queue_recovers_restart_and_drains_to_cloud_without_duplicates
cargo test -p ham-gui
cargo test -p ham-ios-ffi
just version-check
just ci
```

Production release tags still come only from validated semantic-version tags
contained in `main`; this document does not authorize a tag or publication.
