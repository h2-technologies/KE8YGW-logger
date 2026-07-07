# ADR 0001: Append-Only Official Log

## Status

Accepted

## Context

The platform needs durable, auditable station history that can survive offline operation, sync, corrections, deletes, imports, and future conflict resolution.

## Decision

Official logbook state is represented as an append-only event stream. Every official state change appends an event. Existing official events are never physically edited or removed by normal workflows.

## Consequences

- Corrections are new events.
- Deletes are tombstone events.
- Restores are new events.
- Projections are rebuildable.
- Sync can compare and replicate immutable events.
- Storage may need compaction or projection caches for performance, but compaction must not replace the official source of truth.
