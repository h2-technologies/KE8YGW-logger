# Advanced Search

Search reads QSO projections, not the official event store directly. The event stream remains the source of truth; search is disposable derived behavior.

## Query Syntax

Structured filters use `field:value` tokens:

- `callsign:K1ABC`
- `band:20m mode:FT8`
- `entity:Japan`
- `dxcc:339`
- `state:OH`
- `grid:EN91`
- `park:US-1234`
- `summit:W1/AA-001`
- `operator:KE8YGW`
- `station:W1AW`
- `profile:<station_profile_id>`
- `tag:portable`
- `date:2026-07-01..2026-07-06`
- `deleted:false`
- `confirmed:true`
- `source:manual`

Plain text terms search callsign, notes, name, QTH, and tags.

## Saved Searches

`JsonSavedSearchStore` persists saved searches as support/config state. Saved searches are not official events and are not synced by default for MVP.

## Current Limitations

- Parser is intentionally simple and whitespace-token based.
- No indexed search backend yet.
- No fuzzy search or query language quoting yet.
