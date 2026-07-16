# iOS Rust Authority Model

Last updated: 2026-07-10

The native iOS app follows the same domain authority model as desktop and
hosted clients:

```text
SwiftUI form
  -> typed Swift bridge DTO
  -> ham-ios-ffi JSON command ABI
  -> ham-core proposal/support storage
  -> official event or Rust station-book mutation
  -> Rust projection response
  -> SwiftData projection/cache refresh
  -> UI update
```

SwiftData must not be treated as canonical domain storage for QSOs, station
profiles, equipment, activations, Net Control sessions, sync state, or backup
restore acceptance.

SwiftData may store:

- UI projection/cache rows returned by Rust.
- Local draft form state before submission.
- Local iOS preferences that are not shared domain state.
- Legacy cache rows during migration, marked without a Rust canonical ID.

Projection/cache rows include:

- canonical entity ID from Rust
- projection/source marker
- Rust revision or event hash where available
- schema/projection version
- tombstone status
- last projection refresh timestamp

Current Rust-routed iOS mutations:

- QSO create/delete
- station profile create/select
- station equipment create
- POTA/SOTA activation start/end
- Net Control session start/end
- Net Control check-in create
- Net Control traffic create

Known remaining Swift-local behavior:

- QSO edit/correction UI
- POTA/SOTA spotting queue actions
- emergency assignment scratch state
- provider upload queue actions
- full sync push/pull/merge/conflict resolution
- JSON/ZIP backup inspect/dry-run/apply restore
- some local app settings
