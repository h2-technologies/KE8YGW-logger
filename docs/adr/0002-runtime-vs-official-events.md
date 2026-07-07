# ADR 0002: Runtime vs Official Events

## Status

Accepted

## Context

The system needs both permanent operating records and rich diagnostic telemetry.

## Decision

Official events and runtime events are separate streams. Official events are append-only logbook history and are synced. Runtime events are diagnostic telemetry, persisted to rotating JSONL logs, exported in diagnostic reports, and not synced.

## Consequences

- Runtime logs can be deleted without damaging official history.
- Diagnostic bundles can redact and summarize runtime events.
- Event Bus Monitor can observe runtime behavior without becoming a logbook source of truth.
- Sync never transfers runtime logs.
