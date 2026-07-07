# ADR 0005: Local-First Sync

## Status

Accepted

## Context

Ham radio operations often happen offline, in parks, field deployments, events, and emergency conditions.

## Decision

The app is local-first. LAN discovery and replication are preferred. Cloud relay and self-hosted sync are fallback paths when LAN peers cannot connect.

## Consequences

- The local official event store remains authoritative for the device.
- Cloud support must remain self-hostable.
- Runtime logs and credentials are not synced.
- Divergent official event chains are detected and reported, not automatically merged in MVP.
- Replication must verify hashes and chain continuity before accepting remote events.
