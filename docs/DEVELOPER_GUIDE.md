# Developer Guide

This guide explains the day-to-day workflow for contributors working inside the blueprint-driven repository.

## Start With the Blueprint

Before implementing a feature, read:

1. `docs/MASTER_BLUEPRINT.md`
2. `PROJECT_STATE.md`
3. The topic-specific document for the area being touched:
   - `docs/EVENT_CATALOG.md`
   - `docs/PLUGIN_SDK.md`
   - `docs/SYNC_PROTOCOL.md`
   - `docs/SECURITY_MODEL.md`
   - `docs/ROADMAP.md`
   - `ROADMAP.md`

If current code differs from the blueprint, prefer documenting the migration path over risky renames or broad rewrites.

## Architecture Rules

- Rust is the primary implementation language.
- `ham-core` owns business logic.
- Plugins submit proposals.
- Only the core writes official events.
- Official log events are append-only and hash chained.
- Runtime events are diagnostic only and are not synced.
- UI panels consume core state and submit proposals; they do not own domain rules.
- Sync prefers LAN before cloud.
- Cloud/self-hosted sync must share the same safe replication rules.
- Every protected action checks both plugin permission and operator role permission.

## Local Commands

Use `just` when available:

```powershell
just fmt
just check
just clippy
just test
just build
just release
just ci
```

Equivalent Cargo commands:

```powershell
cargo fmt --all
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --workspace
cargo build --release --workspace
```

For the current static GUI JavaScript, also run:

```powershell
node --check crates\ham-gui\web\app.js
```

## Completion Checklist

Before considering an implementation complete:

1. Update `PROJECT_STATE.md`.
2. Update `README.md` if contributor-facing behavior changed.
3. Update architecture docs when architecture, protocols, permissions, or event catalogs change.
4. Update this developer guide when workflow expectations change.
5. Run formatting, linting, tests, and release build.
6. Fix every discovered issue.
7. Report changed files, architecture decisions, risks, TODOs, and the next milestone.

## Adding Features

Ask first whether the feature should be a plugin. If yes:

- Add or update the plugin manifest.
- Declare requested and optional permissions.
- Register service providers through the shared `ServiceRegistry` when the
  feature integrates with a replaceable provider.
- Register panels and commands where appropriate.
- Submit official changes through proposals.
- Publish runtime events for diagnostics.
- Add tests for allow and deny paths.
- Document the user workflow and architecture impact.

## Adding Official State

Official state changes require:

- a proposal type
- validation rules
- permission checks
- an official event type
- deterministic hashing
- append-only storage
- projection rebuild/update behavior
- tests for serialization, validation, hashing, and projection replay

Never update projections directly from UI state.

## Adding Providers

Use the Unified Service Framework for lookup, upload, spotting, map, weather,
propagation, award, AI, authentication, storage, and notification providers.

Provider work should add:

- provider metadata
- provider permissions
- config key schema without credential values
- typed request/response models when needed
- service cache behavior when useful
- runtime events
- tests for selection, fallback, permissions, cache, and serialization

## Adding Runtime Diagnostics

Runtime events should include safe summaries and redacted payloads. Do not log credentials, tokens, API keys, private profile data, full official logs, or raw provider responses that may contain secrets.

Update `docs/EVENT_CATALOG.md` when adding new event categories or stable event names.

## Testing Expectations

At minimum, tests should cover:

- serialization/deserialization for public models
- valid and invalid validation paths
- permission denied paths
- persistence/reload where storage is touched
- projection rebuild behavior
- sync verification behavior where replication is touched
- GUI model logic where UI structure changes

Network and GUI browser tests may be mocked or model-level when CI reliability would otherwise suffer.

## Daily Driver Logging Work

Daily-driver features should keep support state and official state separate:

- Station/equipment profiles are support/config state. Official QSOs may store stable profile/equipment IDs, but profile edits must not rewrite historical QSOs.
- Awards and search read projections. They should not scan or mutate official event storage directly in normal UI flows.
- Upload providers receive ADIF generated from projected QSOs through the Unified Service Framework.
- Upload queue state can be support state, but provider results tied to specific QSOs should use append-only upload status events.
- Keyboard-first UI changes should map to command IDs so future desktop shortcuts can reuse the same actions.
