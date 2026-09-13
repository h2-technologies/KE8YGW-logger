# Contest Rule and Exchange Schema

The contest rule schema is the data contract behind v1 contest logging. A
contest is described by data, not by code, so a corrected or newly published
rule set can reach operators without a new application build.

Canonical implementation: `crates/ham-core/src/contest.rs`.
Bundled definitions: `crates/ham-core/assets/contest-definitions-v1.json`.

## Two independent versions

| Version | Meaning | On mismatch |
| --- | --- | --- |
| `schema_version` | The shape of the pack document. Currently `1`, exported as `CONTEST_RULE_SCHEMA_VERSION`. | The pack is rejected whole with `UnsupportedSchemaVersion`. Nothing is partially loaded. |
| `rule_version` | The version of one contest's rules. A monotone integer per `contest_id`. | The catalog keeps the highest version it has seen. An older pack cannot downgrade a contest. |

Definition objects use `deny_unknown_fields`. A pack that carries a rule
concept this build does not implement fails to load instead of loading with
that rule silently ignored, which is the only safe behavior for scoring data.

## Pack document

```json
{
  "kind": "ke8ygw.contest.definition-pack",
  "schema_version": 1,
  "pack_id": "ke8ygw.builtin.generic",
  "pack_version": 1,
  "generated_at": "2026-08-31T00:00:00Z",
  "definitions": [ ... ]
}
```

`kind` is checked before anything else, so an unrelated JSON document with a
`definitions` array cannot be mistaken for a contest pack.

## Definition fields

| Field | Purpose |
| --- | --- |
| `contest_id`, `name`, `sponsor` | Identity shown to the operator. |
| `rule_version`, `rule_source` | Version and the published source the rules were encoded from. |
| `bands`, `modes` | The bands and modes the contest uses. |
| `time_windows` | Optional windows during which contacts count. No window means always available, which is how the generic templates work. |
| `max_operating_minutes` | Optional operating time limit. |
| `exchange` | `sent` and `received` field lists. |
| `categories` | Accepted entry-category values per dimension. An empty list means the contest does not use that dimension. |
| `duplicate_rule` | The scope in which a repeat contact is a duplicate. |
| `serial_policy` | Whether serials are allocated, and per what. |
| `multipliers` | What the contest counts as a multiplier and in what scope. |
| `scoring` | `default_points` plus ordered `rules`; the first matching rule wins. |
| `export` | The official export identity, for example the Cabrillo contest name. |

### Exchange field kinds

`serial`, `rst`, `callsign`, `grid`, `text` (with `max_len`), `integer` (with
`min` and `max`), and `choice` (with `options`). Values are validated and
normalized in one place, so the duplicate, scoring, and export paths all read
the same text:

```json
{ "key": "grid_received", "label": "Grid Received", "kind": { "type": "grid" } }
```

### Consistency rules enforced at load time

- Both exchange directions declare at least one field, and keys are unique.
- A serial policy other than `none` requires a `serial` exchange field, and a
  `serial` field requires a serial policy. A definition cannot promise serials
  it never allocates.
- A multiplier reading a received field must name a field the exchange
  declares.
- Grid multipliers use an even precision between 2 and 8.
- Scoring rules may only reference bands and modes the contest uses.
- Time windows must end after they start.

## Signed updates

A distributed pack is an envelope: the pack plus a detached signature over its
canonical bytes.

```json
{
  "pack": { ... },
  "signature": {
    "key_id": "release-2026",
    "algorithm": "hmac_sha256",
    "digest": "<sha256 of canonical bytes>",
    "signature": "<hmac-sha256 of canonical bytes>"
  }
}
```

`ContestPackTrustStore::new()` accepts only packs signed by a key it holds; an
unsigned pack, an unknown key id, a changed digest, or a bad signature are each
a distinct typed error. `ContestPackTrustStore::allowing_unsigned()` exists for
the built-in pack and tests, and is not the production update path.

## Catalog

`ContestDefinitionCatalog::with_builtin()` starts from the compiled-in
definitions. `install_pack` applies an update and returns the contest ids it
actually changed. Each entry records its origin — `builtin`, or the `pack_id`
and `pack_version` it came from — and `summary()` reports that provenance for
runtime events and support bundles without including rule bodies.

## Scope

This document covers the schema and its loader only. The contest session and
logging engine, the Field Day and Winter Field Day templates, the
December/January definition packs, and Cabrillo export are separate v1
deliverables and are not implemented here.
