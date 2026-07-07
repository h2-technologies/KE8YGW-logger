# ADR 0004: Global Event Log Plus Projections

## Status

Accepted

## Context

The platform must support several operating modes without fragmenting state into disconnected databases.

## Decision

The official logbook is a global event stream. Entity-specific views such as current QSOs, activations, awards, uploads, nets, and contest scores are projections rebuilt from official events.

## Consequences

- Projections are disposable and rebuildable.
- UI should read projections rather than mutable official records.
- New workflows should add event types and projection logic instead of separate authoritative stores.
- Projection caches may be persisted for startup performance but must remain recoverable from official events.
