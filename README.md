# Ham Radio Operations Platform

This workspace is the first foundation for a local-first, plugin-based amateur
radio operations platform. The MVP direction is casual logging, POTA/SOTA, and
sync, with room for emergency communications, net control, and contesting.

## Workspace

- `ham-core`: append-only logbook events, event bus, proposal validation, event store, and projections.
- `ham-plugin-sdk`: public plugin manifest, capability, proposal, and event constant types.
- `ham-sync`: placeholder for future local-first synchronization.
- `ham-cli`: placeholder command-line entry point.

## Architecture

The official logbook is a global append-only event stream. Each official event
contains its own SHA-256 hash and the previous event hash for the same logbook,
similar to a Git commit chain. Deletes are tombstone events. The original events
remain in history, and read models decide how to display current state.

Plugins cannot write official events directly. A plugin submits a proposal such
as `proposal.qso.create`, `proposal.qso.correct`, or `proposal.qso.delete`.
`ham-core` validates the plugin manifest capability, the operator role, the
proposal event type, and the basic payload schema. Only then does the core append
an official event such as `official.log.qso.created`.

The event bus is a first-class typed interface. This initial implementation
provides an async in-memory bus that publishes official logbook events and
diagnostic runtime events. Runtime events are diagnostic only and are not part of
the official logbook stream.

Projections are derived state. The QSO current-state projection rebuilds from the
event stream and hides tombstoned QSOs while preserving the delete event in the
append-only history.

## Running Tests

```powershell
cargo test
```

The initial test suite verifies event hash chaining, valid and tampered chain
verification, tombstone projection behavior, plugin capability rejection, and
event bus publication for accepted proposals.
