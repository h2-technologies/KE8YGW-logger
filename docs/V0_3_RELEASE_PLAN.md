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
- Structured conflict reports for divergent previews and dependency-blocked
  queued mutations.
- Durable LAN trust store with explicit approval, hashed short-lived single-use
  pairing tokens, logbook-scoped trusted devices, replay nonce rejection, and
  immediate revocation.

## Still Incomplete For v1

- Real LAN peer-to-peer HTTP transport.
- Production pairing UX across desktop and iOS.
- Manual conflict-resolution commands that create corrective official events or
  select an explicit recovery path.
- Release-device iOS background retry and local-network permission validation.
- Full cross-client recovery/migration scenarios across hosted web, desktop,
  iOS, and self-hosted sync.

## Validation Targets

```powershell
cargo test -p ham-sync
cargo test -p ham-gui
cargo test -p ham-ios-ffi
just version-check
just ci
```

Production release tags still come only from validated semantic-version tags
contained in `main`; this document does not authorize a tag or publication.
