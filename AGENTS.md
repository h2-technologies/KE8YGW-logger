# AGENTS.md

## 1. Document Purpose And Authority

This file is the primary repository instruction document for Codex and other AI coding agents working in this repository.

Authority order:
1. Direct user instructions.
2. This `AGENTS.md`.
3. More-specific nested `AGENTS.md` files, if they are added later, for their own directory trees.
4. Accepted architecture decisions and locked blueprint documents.
5. Actual code and tests.

Mandatory rules:
- Treat architecture decisions and security constraints as binding unless the user explicitly asks for architectural change.
- Inspect the relevant code, tests, and docs before editing. Do not infer repository behavior from memory.
- Update this file whenever repository structure, workflow policy, architecture boundaries, or validation expectations materially change.
- Do not rely solely on old task summaries, issue descriptions, or prior-agent reports. Use them only as hints, then verify against current code and tests.
- As of July 16, 2026, no nested `AGENTS.md`, `CLAUDE.md`, `.cursorrules`, or Copilot-instruction files are present in the repository.

## 2. Project Overview

KE8YGW Logger is a local-first amateur-radio operations platform built around a shared Rust core, append-only official logbook history, proposal-validated writes, rebuildable projections, provider-backed online services, LAN-first synchronization, a hosted API, and a desktop Tauri wrapper.

Implemented functionality:
- Casual QSO logging with create, correct, delete, restore, and note flows.
- POTA/SOTA activation workflows and QSO activation linking.
- Net Control sessions, check-ins, traffic, tombstones, and report export events.
- ADIF import/export through the same proposal and projection pipeline.
- Station profiles, equipment, and station configurations as support state.
- Projection-backed awards and advanced search foundations.
- Upload queue foundation plus append-only upload status events.
- Callsign lookup, grid validation, distance/bearing, map-derived QSO and station objects.
- Runtime diagnostics, rotating runtime JSONL logs, diagnostic bundle export, and diagnostic report upload.
- Hosted `/api/v1` routes for auth, logbooks, QSOs, activations, Net Control, station/equipment, providers, uploads, maps, backups, divergence review, and sync.
- Self-hosted sync/report server with durable local metadata, durable JSONL official-event storage, and filesystem report payloads.
- Tauri desktop wrapper with native dialog commands and restricted `/api/*` proxying.

Foundational or partial functionality:
- Unified Service Framework with provider metadata, selection, health, capability, config, caching, and permission enforcement.
- Online Services runtime for uploads, lookups, spots, weather, propagation, maps, automation, and notifications.
- Hosted provider execution for QRZ XML, HamQTH, POTA spots, and bounded DX Cluster controls.
- Gated live upload transports for Club Log, QRZ Logbook, and eQSL.
- Cloud sync pairing-token auth and durable self-hosted backend.
- GUI shell workspaces, panel models, command registry, and browser-served static frontend.

Planned but not implemented end-to-end:
- Real runtime plugin loading, sandboxing, and signatures.
- Production iOS reciprocal LAN transport completion UX, stronger LAN key-exchange
  hardening, and physical-device validation beyond the automatic
  discovery/manual direct LAN HTTP peer paths, HMAC-SHA256 signed LAN read
  endpoint authorization, browser pairing/auth-credential rotation/recovery
  panel, and native iOS LAN trust snapshot/issue/accept/trust/rotate/revoke
  bridge.
- Conflict resolution UI and automatic merge policy.
- Full production provider coverage for LoTW/TQSL, SOTAWatch live access, NOAA/Open-Meteo, FCC ULS, RBN, and other placeholder providers.
- Swift projection cache and production App Store packaging.
- Interactive tile/vector map renderer.
- Full EmComm and contesting product surfaces.

Test-only, mock, fake, or stub functionality:
- Mock lookup providers, mock rig provider, placeholder map providers, mock weather/propagation data.
- Fake/default provider execution remains the ordinary test path for online adapters.
- In-memory hosted metadata stores and in-memory cloud sync server remain test helpers.
- Demo LAN peer and demo runtime events in `ham-gui` are development scaffolding.

Provider reality check:
- Do not call a provider “live” unless real transport exists in code and is explicitly enabled.
- Club Log, QRZ Logbook, and eQSL have real gated live HTTP upload paths.
- QRZ XML and HamQTH have real hosted lookup execution paths.
- POTA has real hosted spot-fetch execution.
- DX Cluster has bounded connect/read/disconnect/status runtime controls.
- LoTW live upload is deferred.
- SOTAWatch live access is deferred.
- Many map, weather, propagation, and spotting providers are metadata-only, mock, or placeholder implementations.

Native iOS status:
- Native SwiftUI, Rust FFI, Xcode project, Apple build scripts, and iOS workflow files are present as of July 21, 2026.
- iOS release hardening, signing, TestFlight/App Store distribution, and full production validation remain incomplete.

## 3. Source-Of-Truth Hierarchy

Consult repository information in this order:
1. Direct user request.
2. This `AGENTS.md`.
3. Accepted ADRs in `docs/adr/` and `docs/MASTER_BLUEPRINT.md`.
4. Actual current code and tests.
5. `PROJECT_STATE.md`.
6. Subsystem architecture and security documents under `docs/`.
7. Release plans and roadmaps.
8. `README.md` summaries.
9. Historical issue text, prior-agent reports, and old task summaries.

When sources disagree:
- Code and tests are evidence of what currently exists.
- ADRs and the Master Blueprint define intended boundaries and mandatory architecture unless the task explicitly changes them.
- If code violates the intended architecture and the user did not request architectural work, do not spread the violation. Implement the smallest safe change, document the discrepancy, and preserve the long-term boundary.
- Do not present planned functionality as complete just because a roadmap or release plan says it should exist.

## 4. Repository Map

### Top Level

| Path | Purpose | Put Here | Do Not Put Here | Key Entry Points | Tests / Validation | May Call |
| --- | --- | --- | --- | --- | --- | --- |
| `Cargo.toml` | Workspace manifest | Workspace members, shared deps, version | Crate-specific business rules | Workspace members include `src-tauri` | `cargo check --workspace --all-targets` | N/A |
| `justfile` | Canonical local commands | Shared validation and launch commands | Task-specific ad hoc scripts | `just ci`, `just gui`, `just sync-server` | Mirrors CI baseline | Cargo binaries |
| `README.md` | Contributor and product overview | Current high-level behavior and entry docs | Detailed operating rules better suited to `AGENTS.md` | Start-here guide | Manual review | Docs only |
| `PROJECT_STATE.md` | Implementation-state ledger | Verified state, debt, recent validation history | Aspirational claims not proven by code | Current milestone snapshot | Manual review | Docs only |
| `ROADMAP.md` | Root milestone summary | High-level milestone direction | Detailed architecture | Milestone summary | Manual review | Docs only |
| `.github/workflows/` | CI, security, scorecard, iOS, and release automation | Actual enforced validation and release steps | Local-only experiments | `ci.yml`, `security.yml`, `ios.yml`, `release.yml`, `scorecard.yml` | GitHub Actions | Cargo / just / Semgrep / actionlint |
| `.github/dependabot.yml` | Dependency update automation | Scheduled update policy for Cargo, GitHub Actions, and Docker | Auto-merge policy or invented labels | Weekly updates targeting `dev` | Dependabot | N/A |
| `docs/` | Architecture, security, protocols, release plans | Stable subsystem docs | Runtime code, generated outputs | See sections below | Manual review | N/A |
| `crates/` | Workspace crates | Rust implementation | Generated assets | See crate table below | Cargo tests/builds | Inter-crate deps only |
| `src-tauri/` | Tauri v2 desktop runtime wrapper | Tauri config, command bridge, packaging assets | Domain logic | `src-tauri/src/main.rs`, `tauri.conf.json` | `cargo tauri info`, `cargo tauri build` | `ham-desktop` |
| `.env.example` | Runtime env reference | Supported server env vars | Secrets | Sync/server env names | Manual review | N/A |
| `Dockerfile.sync-server` | Sync-server container build | Self-hosted sync packaging with digest-pinned base images | Hosted API containerization for unrelated services | `ham-sync-server` release binary | `docker build -f Dockerfile.sync-server .` | `ham-sync-server` |
| `deny.toml` | Cargo advisory policy | Narrow, documented advisory exceptions with review dates | Broad vulnerability suppressions or dependency hiding | `cargo deny check advisories` | cargo-deny | N/A |
| `target/` | Generated build artifacts | Nothing by hand | Source, docs, fixtures | None | Ignore in reviews unless build artifact debugging is requested | N/A |

Repository absences that matter:
- No `migrations/` directory is present.

### Workspace Crates And Runtime Directories

| Path | Purpose | Logic That Belongs Here | Logic That Must Not Be Here | Important Entry Points | Significant Dependencies | Testing Locations | May Call |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `crates/ham-core` | Authoritative domain and infrastructure core | Official events, proposal validation, projections, ADIF, lookup, rig, diagnostics, permissions, service framework, credentials, maps, stations, Net Control, upload queue, support storage, JSONL official store | GUI-only behavior, Tauri commands, hosted account/session ownership, JS business rules | `src/lib.rs`; modules `proposal.rs`, `store.rs`, `projection.rs`, `service.rs`, `credential.rs`, `online.rs` | `ham-plugin-sdk`, `tokio`, `serde`, `sha2`, `ureq`, OS credential APIs/tools | Module tests plus `src/tests.rs` | `ham-plugin-sdk` only |
| `crates/ham-plugin-sdk` | Stable public SDK vocabulary | Plugin manifests, permission enums, proposal envelopes, official/proposal event constants, service-type vocabulary | Plugin loading, domain validation, app-specific logic | `src/lib.rs` | `serde`, `chrono`, `uuid` | Inline tests via downstream crates; compile-time usage across workspace | No app crates |
| `crates/ham-sync` | Sync protocol and sync/report backend logic | Discovery, handshake, head comparison, preview/pull/push, pairing auth, report-upload models, optional durable sync/report metadata | GUI shell behavior, hosted account logic, domain rule duplication | `src/lib.rs` | `ham-core`, `serde`, `tokio`; optional `surreal-storage` enables `surrealdb` and `sha2` | Inline tests in `src/lib.rs` | `ham-core` |
| `crates/ham-sync-server` | Self-hosted sync/report binary | Process startup, env loading, serving the `ham-sync` backend | Core sync protocol models, GUI, hosted logbook business rules | `src/main.rs` | `ham-sync` | Build/run validation; behavior mostly tested in `ham-sync` | `ham-sync` |
| `crates/ham-server` | Hosted web/server API boundary | Auth/session/device/logbook metadata, role checks, thin routes, hosted support metadata persistence, proposal delegation, backup/divergence/sync endpoints | Reimplementing domain validation already in `ham-core`, Tauri UI code | `src/lib.rs`, `src/main.rs` | `ham-core`, `ham-sync`, `surrealdb`, `serde`, `tokio` | Large inline route/integration tests in `src/lib.rs` | `ham-core`, `ham-sync` |
| `crates/ham-cli` | CLI operations on local data | ADIF import/export, chain verification, projection rebuild | New domain rules, hosted route logic, desktop-only behaviors | `src/main.rs` | `ham-core`, `ham-plugin-sdk` | Build/run validation | `ham-core` |
| `crates/ham-gui` | Local Rust GUI shell server and web frontend bundle | Shell state, command registry, runtime bridge, HTTP endpoints for GUI, static HTML/CSS/JS, support-state persistence wiring | Authoritative domain validation in JS, direct official-event writes from frontend | `src/main.rs`, `src/bridge.rs`, `src/shell.rs`, `web/app.js`, `web/index.html`, `web/styles.css` | `ham-core`, `ham-sync`, `ham-plugin-sdk`, `tokio` | `src/commands.rs`, `src/shell.rs`, JS syntax check | `ham-core`, `ham-sync`, `ham-plugin-sdk` |
| `crates/ham-desktop` | Desktop-native dialog contract and helpers | Typed dialog request/result models, redaction of user paths, backend-agnostic helper functions | QSO/business logic, Tauri app bootstrap, HTTP API proxy rules | `src/lib.rs`, `src/main.rs` | `serde` | Inline tests in `src/lib.rs` | No domain crates required |
| `src-tauri` | Tauri v2 desktop wrapper | Tauri commands, runtime payload, restricted `/api/*` proxy, packaging metadata, capability files, icons | Core business rules, direct credential handling in JS, provider logic | `src-tauri/src/main.rs`, `src-tauri/tauri.conf.json`, `src-tauri/README.md` | `ham-desktop`, `tauri`, `rfd`, `ureq` | Inline tests in `src-tauri/src/main.rs`; Tauri build commands | `ham-desktop` |

### Documentation Areas

| Path | Purpose |
| --- | --- |
| `docs/MASTER_BLUEPRINT.md` | Locked architecture decisions and long-term crate-migration guidance. |
| `docs/adr/` | Accepted architecture decisions that overrule informal summaries. |
| `docs/EVENT_CATALOG.md` | Stable official/proposal/runtime event vocabulary. |
| `docs/PLUGIN_SDK.md` | Public plugin manifest, permission, and proposal contract. |
| `docs/SYNC_PROTOCOL.md` | Sync rules and transport expectations. |
| `docs/SECURITY_MODEL.md` and `docs/security/*` | Security constraints, credential handling, and redaction rules. |
| `docs/architecture/*` | Subsystem architecture notes for services, support storage, stations, search, awards, uploads, and online services. |
| `docs/plugins/*` and `docs/plugin-map-providers/*` | Provider and plugin-development guidance. |
| `docs/maps`, `docs/grid-system`, `docs/propagation`, `docs/weather` | GIS, Maidenhead, propagation, and weather model guidance. |
| `docs/*RELEASE*.md`, `docs/ROADMAP.md`, `docs/V*_PLAN.md`, `docs/IOS_APPSTORE_READINESS.md` | Release sequencing and gap tracking. |

## 5. Core Architectural Invariants

### Rust-Authoritative Domain Model

- Rust is the primary implementation language.
- `ham-core` owns reusable domain logic and business rules.
- Swift, JavaScript, HTML, hosted route handlers, and platform wrappers must not become the authoritative owner of QSO, activation, Net Control, provider-permission, or sync-validation rules.
- Platform-specific layers must call shared Rust/core APIs wherever practical.

### Official State

- Official logbook state is append-only.
- Official events are hash chained per logbook.
- Existing official events must not be edited in place.
- Deletion uses tombstone events.
- Corrections and restoration use new official events.
- Historical integrity is mandatory.

### Proposal Pipeline

- Plugins and UI layers do not append official events directly.
- They submit typed proposals.
- Core validation checks proposal type, payload, plugin permission, permission grant, operator role, applicable state, and domain invariants.
- Only validated core logic creates official events and appends them through `LogbookEventStore`.

### Projections

- Projections are rebuildable derived state.
- UI code must not mutate projections as though they were authoritative storage.
- Projection changes require replay tests.
- Deleted and restored entities must derive from event history, not side edits.

### Runtime Events

- Runtime events are operational diagnostics, not official history.
- They are not a substitute for official events.
- They must use safe, redacted summaries.
- They are not synchronized unless an explicit architecture change says otherwise.

### Support State Classes

| State Class | Meaning In This Repo | Examples |
| --- | --- | --- |
| Official append-only state | Permanent hash-chained history | QSO events, activation events, Net Control events, upload status events |
| Derived projections | Rebuildable read models from official events | `QsoCurrentStateProjection`, `ActivationProjection`, `NetControlProjection` |
| Support/configuration state | Durable local or hosted sidecar state not in official history | service registry settings, service cache entries, upload queue metadata, map layers, station book, lookup/rig UI config, online automation state, provider settings |
| Secrets | Raw credential material behind approved credential backends only | QRZ password, Club Log API key, future sync secrets |
| Runtime diagnostics | Operational telemetry and bundles | rotating runtime JSONL logs, diagnostic ZIPs, runtime event replay buffer |
| Hosted account/auth metadata | Hosted/server-side account and authorization records | users, login sessions, devices, memberships, API tokens, invites, pairing tokens, report metadata |

### Service / Provider Framework

- Replaceable external services use the Unified Service Framework.
- Providers register typed metadata, permissions, configuration schema, health, capability, priority, online/offline behavior, and credential references.
- Features must not create parallel ad hoc provider abstractions.
- Provider-independent application code should depend on service interfaces and provider metadata, not vendor-specific branches scattered across UI code.

### Credentials

- Secrets must be accessed only through `CredentialStore` or the approved Rust credential path.
- Store credential IDs or references in configuration, never raw secrets.
- Never put secrets in official events, support JSON, browser local storage, source code, diagnostic bundles, runtime logs, tests, snapshots, or commits.
- The insecure development credential backend is explicit opt-in only through `HAM_PLATFORM_ALLOW_INSECURE_DEV_CREDENTIALS=1`.
- Validate credentials through the provider boundary before enabling operations that require them.
- Preserve offline operation where product requirements allow it.

### Synchronization

- LAN is preferred over hosted/cloud synchronization.
- All transports must use the same verification and append-only safety rules.
- Sync must not accept malformed, incorrectly chained, unauthorized, unsupported, or conflicting official events without review.
- Do not bypass shared sync protocol models with platform-specific replication rules.

### Maps And GIS

- Maps consume projections, profile data, and service providers.
- Maps do not own official business state.
- Maidenhead, distance, bearing, grayline, and related calculations belong in reusable core modules.

## 6. Layering And Dependency Rules

Allowed dependency direction:

```text
Web JS / HTML / CSS / Tauri UI / CLI / Hosted HTTP requests
                    |
                    v
       ham-gui / src-tauri / ham-desktop / ham-cli / ham-server / ham-sync-server
                    |
                    v
                ham-core ---- ham-plugin-sdk
                    |
                    v
                 ham-sync
```

Practical interpretation:
- `ham-core` is the shared owner of domain behavior.
- `ham-plugin-sdk` is a shared vocabulary crate and may be depended on by `ham-core`, `ham-gui`, `ham-cli`, and `ham-server`.
- `ham-sync` depends on `ham-core` for official-event validation and storage semantics.
- `ham-server` depends on `ham-core` and `ham-sync`; its routes must stay thin.
- `ham-gui` depends on `ham-core` and `ham-sync` but only as a client/bridge.
- `ham-desktop` is a narrow helper crate; `src-tauri` depends on it.

Prohibited dependencies and ownership violations:
- `ham-core` must not depend on `ham-gui`, `src-tauri`, Swift, or web assets.
- `ham-core` must not depend on hosted-route implementations from `ham-server`.
- `ham-plugin-sdk` must not depend on application implementations.
- JavaScript, HTML, or Swift must not become the authoritative owner of QSO, activation, Net Control, permission, credential, or synchronization rules.
- `ham-server` routes must not duplicate domain validation when `ham-core` already owns it.
- Providers must not write directly to persistence or official event streams outside the proper core boundary.
- `src-tauri` must not expose raw secrets to JavaScript.

Platform-specific code is acceptable when:
- It adapts UI affordances, dialogs, packaging, OS credential APIs, or transport details.
- It stays narrow and testable.
- It delegates authoritative business behavior back to core/shared APIs.

## 7. State Ownership Matrix

| Data Or Behavior | Authoritative Owner | Persistence | Synced? | Mutation Path |
| --- | --- | --- | --- | --- |
| Official QSO history | `ham-core` official events | Local JSONL official log; durable JSONL on sync server; replayed by hosted APIs | Yes | Proposal -> core validation -> `LogbookEventStore::append_event` |
| QSO current-state projection | `ham-core::projection` | Rebuilt in memory from official events | Rebuildable, not directly synced | Replay only |
| Activations | `ham-core` official events + `ActivationProjection` | Official JSONL | Yes | Proposal -> core validation -> official events |
| Net Control sessions/check-ins/traffic/reports | `ham-core` official events + `NetControlProjection` | Official JSONL | Yes | Proposal -> core validation -> official events |
| Station profiles / equipment / configurations (local app) | `ham-core::station` | JSON support files (`station-book.json`) | No, not through official sync | Core store methods and approved GUI/server APIs |
| Station/equipment hosted metadata | `ham-server` hosted metadata boundary using `ham-core` models | SurrealDB | Not through LAN/cloud official sync | Hosted role/scoping checks -> metadata store |
| Provider settings and service enablement | `ham-core::service` models; `ham-server` hosted metadata for hosted mode | Local support JSON; hosted SurrealDB; sync-server support metadata for server-side operations | No by default | Approved GUI/server settings APIs |
| Credential values | `ham-core::credential::CredentialStore` | OS credential backend or explicit insecure dev file backend | No | Credential APIs only |
| Credential metadata | `ham-core::credential` | Sanitized local metadata JSON | No | Credential APIs only |
| Upload queue operational state | `ham-core::upload` / support state | Local support JSON; hosted SurrealDB history | No by default | Approved GUI/server APIs |
| Upload result status tied to QSOs | Official upload status events in `ham-core` | Official JSONL | Yes | Upload execution -> append official upload status event |
| Map preferences / layer settings | `ham-core::map` support models; hosted map settings for hosted mode | Local support JSON; hosted SurrealDB | No by default | Approved GUI/server APIs |
| Saved searches | `ham-core::search` | Local JSON saved-search store | No | Search-store APIs |
| Runtime events and runtime logs | `ham-core::bus` + `runtime_log` | Rotating JSONL files and replay buffer | No | Runtime bridge publishing only |
| Hosted users, server admins, sessions, devices, memberships, invites, API token hashes, verification/recovery token hashes, rate limits, audits | `ham-server` | SurrealDB | N/A server auth state | Hosted auth and admin routes |
| Sync pairing sessions, relay refs, sync heads, report metadata | `ham-sync` durable server backend behind `surreal-storage` | SurrealDB + filesystem payloads + JSONL official log | N/A infrastructure state | Sync server methods only |
| Diagnostic reports and bundles | `ham-core::diagnostics` bundle model; `ham-sync` report upload storage | ZIP/filesystem local export; server metadata + filesystem payloads | Uploaded only by user action | Build bundle -> optional upload through sync/report API |
| iOS cache / projection records | Native iOS SwiftData cache models backed by Rust/API refresh paths | SwiftData / app container | Rebuildable, not directly official sync | Rust bridge/API refresh; SwiftData remains cache/projection state |

## 8. Event And Proposal Conventions

When adding or changing official state, follow this sequence:
1. Define or update the public proposal vocabulary in `ham-plugin-sdk`.
2. Define payload models and serialized field names.
3. Define required plugin permissions and operator-role requirements.
4. Validate schema and state transitions in `ham-core::proposal` and related modules.
5. Create the official event type and payload.
6. Preserve deterministic serialization and hashing.
7. Append through `LogbookEventStore`, never by mutating prior events.
8. Update projection replay logic and any rebuild helpers.
9. Publish safe runtime diagnostics.
10. Expose the change through application bridges, hosted routes, or desktop/UI layers.
11. Add allow-path and deny-path tests.
12. Update `docs/EVENT_CATALOG.md` and any affected architecture docs.

Naming rules:
- Follow the existing dotted stable names in `docs/EVENT_CATALOG.md` and `ham-plugin-sdk` constants.
- Treat renaming event names, JSON fields, ABI command names, route fields, and persisted schema fields as compatibility work, not cosmetic cleanup.
- Before renaming anything serialized or persisted, search all consumers: Rust core, hosted API, GUI JS, tests, docs, and any desktop command callers.

## 9. Feature Implementation Playbooks

### Adding A Core Domain Feature

- Put shared models, validation, official-event creation, projection replay, and storage abstractions in `ham-core`.
- Add stable constants and proposal/event vocabulary in `ham-plugin-sdk` when the change is part of the public plugin contract.
- Use thin adapters in `ham-server`, `ham-gui`, `ham-cli`, and `src-tauri`.
- Add replay, permission, and serialization tests before marking the feature done.

### Adding Or Modifying A QSO Field

You must evaluate:
- Proposal models and validation.
- Official payload schema and deterministic hashing compatibility.
- ADIF import/export.
- QSO projection models.
- Search parsing and filtering.
- Map derivation if the field is geographic or station-related.
- Upload generation and provider payloads.
- Hosted API request/response shapes.
- GUI form and presentation logic.
- Backup import/export compatibility.
- Any future iOS/API contract implications documented in `docs/API_CLIENT_CONTRACT.md`.
- Backward-compatible reads and migrations where required.
- Regression tests across serialization, proposals, projection replay, and API flows.

### Adding A Provider

Require all of the following:
- Service type and provider metadata in the Unified Service Framework.
- Provider-specific config schema without raw secret values.
- Permission declaration and capability vocabulary.
- `credential_id` references, never raw secrets.
- Typed client/request boundary.
- Timeout and cancellation behavior.
- Health reporting and stable redacted failure codes.
- Explicit offline behavior and fake/default mode.
- Retry behavior where applicable.
- Redaction of diagnostics, logs, and backups.
- Mock/fake implementation for deterministic tests.
- Ignored or explicit-gated live tests only.
- GUI/server configuration surface and enable/disable behavior.
- Documentation updates in provider docs and release plans.

Ordinary unit tests must not require external network access or real credentials.

### Adding A Hosted API Route

Require all of the following:
- Authentication and session handling.
- Device and logbook scoping.
- Membership role and permission checks.
- Bounded input validation.
- Canonical structured errors when available, while preserving current compatibility.
- Idempotency where appropriate.
- Delegation into core proposal/store logic instead of route-local business logic.
- Durable persistence where applicable.
- Success and failure route tests.
- Documentation or client-contract updates.

Hosted route handlers must remain thin.

### Adding GUI Functionality

Require all of the following:
- Reuse core/application models instead of inventing JS-only domain models.
- Stable command IDs for reusable actions.
- No domain validation that exists only in JavaScript.
- Loading, empty, error, offline, and permission-denied states.
- Accessible controls and keyboard-first behavior where applicable.
- Persistence only through approved APIs or support-state stores.
- Rust model tests and JS syntax validation.

### Adding Desktop / Tauri Functionality

Require all of the following:
- A narrow, testable Rust command boundary.
- Platform-specific code kept in `ham-desktop` and `src-tauri`, not in web JS.
- Browser-safe fallbacks where supported.
- No direct secret exposure to JavaScript.
- Packaging configuration updates when commands/assets change.
- Platform validation with `cargo tauri info` and `cargo tauri build` when prerequisites are available.
- Clear behavior when Tauri prerequisites are unavailable.

### Adding Native iOS Functionality

Current repository status:
- Native SwiftUI source, SwiftData cache models, Rust FFI bridge code, Xcode project files, Apple build scripts, and an iOS simulator workflow exist under `ios/`, `crates/ham-ios-ffi`, `scripts/ios`, and `.github/workflows/ios.yml`.
- Native iOS is part of the locked v1 scope for the November 24, 2026 release, alongside hosted web and Windows/macOS/Linux desktop.
- iOS release hardening, signing, TestFlight/App Store distribution, and complete offline/sync/provider validation remain incomplete.

Mandatory rules are:
- Rust remains authoritative for shared domain behavior.
- SwiftUI views must not reproduce event validation or persistence rules.
- Bridge calls must go through a centralized Rust bridge/store layer.
- Async Rust bridge calls must remain async in Swift and be called with `await`.
- JSON command envelopes, schema versions, ABI versions, correlation IDs, error envelopes, memory ownership, and Rust deallocation rules must remain consistent.
- SwiftData or other local entities are cache/projection records unless explicitly documented otherwise.
- Offline mutations must still go through the authoritative Rust or API path, not a Swift-only event system.
- Bridge and projection-refresh tests must cover success, failure, offline behavior, cancellation, and compatibility.

### Adding Persistence Or Schema Changes

Require all of the following:
- Explicit ownership classification: official, projection cache, support, credentials, runtime, or hosted auth metadata.
- Migration strategy or versioning story.
- Backward-compatible reads when required.
- Atomic or transaction-safe writes.
- Persistence/reload tests.
- Corrupted or partial data handling.
- No silent destructive migration.
- Backup/export-import updates.
- Sync compatibility review when official data changes.

### Adding Synchronization Behavior

Require:
- Protocol compatibility.
- Chain verification and bounded input handling.
- Authorization and logbook scoping.
- Preview before mutation where applicable.
- Conflict/divergence handling.
- Replay tests.
- Transport-independent behavior shared across LAN and cloud/self-hosted sync.

## 10. Compatibility Requirements

| Surface | Requirement |
| --- | --- |
| Persisted JSON support files | Treat version and field-name changes as migrations. Unknown-version rejection is intentional. |
| Official event serialization | Preserve deterministic hash input; do not rewrite prior events. |
| Event hashes | Never alter historical event hashes to fit new code. Fix code or migrate additively. |
| ADIF import/export | Keep field mappings backward compatible; new fields should be additive where possible. |
| Hosted API payloads | Preserve existing field names and error shape compatibility. |
| Sync protocol payloads | Keep protocol versioning explicit; rejected fields or types must fail safely. |
| Provider configuration | Additive fields should be optional or defaulted. Never serialize raw secrets. |
| Database records / Surreal metadata | Field renames require compatibility planning and reload tests. |
| Desktop command names | Tauri command names are API surface for the bundled web UI; renames require coordinated updates. |
| Saved support state | Local JSON stores must keep backward-compatible reads where practical. |

Rules:
- Renaming serialized fields or enum values is a migration, not a cosmetic refactor.
- Additive fields should be optional or have safe defaults unless the migration is explicitly coordinated.
- Do not silently break backups, imports, or sync replay.

## 11. Security And Privacy Rules

Security checklist:
- Use `CredentialStore` for secrets and store only credential references elsewhere.
- Redact runtime events, diagnostic bundles, provider payloads, and hosted responses where secrets could appear.
- Enforce authentication, authorization, device revocation, and logbook scoping on hosted and sync routes.
- Apply least privilege to plugin permissions, operator roles, provider permissions, and network access.
- Keep account, device, station, and logbook scopes separate.
- Validate input shape and size bounds on all hosted and sync endpoints.
- Prevent path traversal and arbitrary path use in import/export/desktop dialog flows.
- Treat backups and diagnostic bundles as sensitive; keep them secret-free by default.
- Use explicit network timeouts and safe transport failure handling for providers.
- Preserve TLS expectations for live HTTP providers; do not add insecure transport shortcuts casually.
- Keep denial-of-service resistance in mind for sync event lists, diagnostics, and import payloads.
- Contain panics and broad exceptions at API and future FFI boundaries.
- Never log secrets, tokens, full credentials, or raw provider payloads that may contain secrets.
- Do not commit test secrets. Use sentinels and gated live tests.
- Do not enable insecure credential fallback in production behavior or documentation.
- Treat callsign, location, profile, and contact data as user-sensitive operational data.
- Add deny-path tests for protected actions.

## 12. Error Handling Conventions

Actual repository conventions:
- `ham-core`, `ham-sync`, `ham-server`, and `ham-desktop` use typed Rust errors, primarily via `thiserror` enums.
- Sync uses structured error/status enums such as `ReplicationError`, `CloudSyncError`, and `ReplicationStatus`.
- Desktop/Tauri command errors use `DesktopCommandError { code, message }`.
- Hosted API responses still commonly use `{ "error": "message" }` for compatibility, while provider/runtime paths also expose stable redacted status fields and error codes.
- Runtime diagnostics use structured `RuntimeEventEnvelope` records with severity, source, summary, and optional redacted payload.

Required practice:
- Prefer typed or structured errors internally.
- Preserve stable machine-readable codes at API, provider, and desktop command boundaries where they already exist.
- Keep human-readable messages safe and secret-free.
- Preserve error context without exposing secrets.
- Do not collapse structured errors into generic strings or raw status codes when the boundary already supports better data.
- Distinguish retryable and non-retryable provider failures.
- Preserve correlation/request IDs where the architecture already carries them.
- Do not invent a new global error abstraction unless the task explicitly calls for architecture work.

## 13. Testing Strategy

| Layer | Repository Evidence | Expectations |
| --- | --- | --- |
| Core domain tests | `crates/ham-core/src/tests.rs` and module tests | Proposal validation, permissions, event hashing, chain verification, projections, tombstones, restore, ADIF, upload status, maps, search, credentials, services |
| Sync tests | `crates/ham-sync/src/lib.rs` | Discovery, handshake, preview/pull/push, auth, duplicate handling, divergence, durable sync/report reload |
| Hosted API tests | `crates/ham-server/src/lib.rs` | Auth, scoping, roles, revoked devices, route success/failure, backups, provider routes, sync routes |
| GUI model tests | `crates/ham-gui/src/commands.rs`, `crates/ham-gui/src/shell.rs` | Shell models and command behavior |
| Desktop tests | `crates/ham-desktop/src/lib.rs`, `src-tauri/src/main.rs` | Dialog helpers, path redaction, command boundaries, proxy validation |
| Syntax / packaging checks | `node --check`, Tauri commands, Cargo builds | Frontend syntax and desktop packaging sanity |

At minimum, code changes should add or update tests for:
- Serialization and deserialization.
- Proposal validation.
- Permission allow and deny paths.
- Deterministic event hashing.
- Event chain verification.
- Projection replay.
- Tombstone and restoration behavior.
- Persistence and reload.
- Migrations or versioned support-state compatibility.
- Sync validation and divergence handling.
- Provider selection, fallback, and credential redaction.
- API authentication and scoping.
- Device revocation.
- GUI model behavior when UI structure changes.
- Desktop/bridge behavior when command boundaries change.
- Offline behavior where applicable.
- Backup export/import and malformed input handling.

Rules:
- Fixes should include regression tests whenever practical.
- Live network tests must be ignored or explicitly gated.
- Live tests must be safe for provider terms, rate limits, and user data.
- Live tests must not run accidentally in ordinary CI.
- Live test output must remain redacted.

## 14. Required Validation Commands

### Canonical Commands From The Repository

From `justfile`:

```powershell
just fmt
just fmt-check
just check
just clippy
just test
just feature-matrix
just api-contract
just version-check
just docs-link-check
just governance-check
just build
just release
just gui
just sync-server
just ci
```

Direct equivalents already used by the repo:

```powershell
cargo fmt --all -- --check
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo check --locked -p ham-sync --no-default-features --all-targets
cargo test --locked -p ham-sync --features surreal-storage
cargo build --workspace
cargo build --release --workspace
python scripts/check_api_contract.py
python scripts/check_versions.py
python scripts/check_docs_links.py
pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/governance-check.ps1
cargo run -p ham-gui --bin ham-gui
cargo run -p ham-sync-server --bin ham-sync-server
cargo run -p ham-server --bin ham-server
node --check crates\ham-gui\web\app.js
git diff --check
cargo tauri info
cargo tauri build
cargo audit --ignore RUSTSEC-2023-0071
cargo deny check advisories
actionlint .github/workflows/*.yml
```

### Mandatory Baseline By Change Type

| Change Type | Minimum Required Validation |
| --- | --- |
| Docs-only changes | `python scripts/check_docs_links.py`, `pwsh -NoProfile -ExecutionPolicy Bypass -File scripts/governance-check.ps1`, `git diff --check` |
| Rust code changes | `cargo fmt --all -- --check`, `cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `git diff --check` |
| GUI web asset changes | Rust baseline if Rust changed; always `node --check crates\ham-gui\web\app.js`; `git diff --check` |
| Desktop/Tauri changes | Relevant Rust baseline plus `cargo tauri info` and `cargo tauri build` when host prerequisites exist |
| Release work | `just ci`, `just release`, `python scripts/check_versions.py`, plus any subsystem-specific packaging commands |

### Targeted Validation By Subsystem

| Subsystem | Targeted Commands |
| --- | --- |
| `ham-core` | `cargo test -p ham-core`, optionally `cargo build -p ham-core` |
| `ham-server` | `cargo test -p ham-server`, `cargo build -p ham-server` |
| `ham-sync` | `cargo test -p ham-sync`, `cargo check --locked -p ham-sync --no-default-features --all-targets`, `cargo test --locked -p ham-sync --features surreal-storage`, `cargo build -p ham-sync` |
| `ham-sync-server` | `cargo build -p ham-sync-server` |
| `ham-gui` | `cargo build -p ham-gui`, `node --check crates\ham-gui\web\app.js` |
| `ham-desktop` | `cargo test -p ham-desktop`, `cargo build -p ham-desktop` |
| `src-tauri` | `cargo tauri info`, `cargo tauri build` |
| Hosted/sync runtime smoke | `cargo run -p ham-server --bin ham-server`, `cargo run -p ham-sync-server --bin ham-sync-server` when manual smoke is needed |

Platform-prerequisite notes:
- `cargo tauri info` and `cargo tauri build` require Tauri host prerequisites and may be unavailable on some machines.
- The release workflow in GitHub Actions currently packages versioned `ham-gui` release archives; full signed Tauri package publishing remains future release work.
- Repository-native docs link checking is `python scripts/check_docs_links.py`; governance validation also checks local Markdown links.
- There are no migration commands because the repo has no dedicated migrations directory.

Validation reporting rules:
- Never claim a command passed unless you actually ran it successfully.
- If a command cannot run, report the exact command, why it could not run, whether the issue is environmental or change-caused, and what remains unverified.

## 15. Documentation Maintenance

Update documentation in the same change as the implementation.

| Change Class | Must Update |
| --- | --- |
| Implementation status, validation status, debt, next steps | `PROJECT_STATE.md` |
| Contributor-facing behavior or startup commands | `README.md` |
| Architecture boundaries or accepted direction | `docs/MASTER_BLUEPRINT.md` and/or relevant ADRs/subsystem docs |
| New or changed official/proposal/runtime event names | `docs/EVENT_CATALOG.md` |
| Plugin manifest, permission, or provider contract changes | `docs/PLUGIN_SDK.md`, `docs/plugins/*`, `docs/plugin-map-providers/*` |
| Sync payloads, auth, replication, or conflict behavior | `docs/SYNC_PROTOCOL.md`, `docs/API_CLIENT_CONTRACT.md` |
| Security, credentials, redaction, auth, or privacy changes | `docs/SECURITY_MODEL.md`, `docs/security/*` |
| Hosted or native API contract changes | `docs/API_CLIENT_CONTRACT.md` |
| Desktop/Tauri behavior or packaging | `docs/DESKTOP_RELEASE.md`, `src-tauri/README.md` |
| Hosted API release readiness | `docs/HOSTED_WEB_RELEASE.md` |
| iOS planning or readiness | `docs/V1_IOS_NATIVE_PLAN.md`, `docs/IOS_APPSTORE_READINESS.md` |
| Release scope or milestone shifts | `ROADMAP.md`, `docs/ROADMAP.md`, release-plan docs |
| Workflow, structure, or policy changes | This `AGENTS.md` |

Rules:
- Do not mark a feature complete merely because scaffolding exists.
- Update docs when behavior changes, not later.

## 16. Git And Change-Management Rules

- Inspect `git status` before editing.
- Preserve unrelated existing changes.
- Do not revert user work.
- Do not delete files merely to make tests pass.
- Keep changes scoped to the requested task.
- Avoid broad rewrites unless the task truly requires them.
- Do not create commits unless explicitly requested.
- Do not push unless explicitly requested.
- Never force-push without explicit authorization.
- Do not rewrite repository history.
- Report pre-existing dirty state separately from your own changes.
- Use focused commits if commit creation is requested.
- Do not mix generated artifacts, dependency updates, formatting sweeps, and functional changes without explicit justification.

No stronger repository-specific commit convention is established by current docs or workflows; prefer small, reviewable commits when asked to commit.

## 17. Agent Workflow

For every substantive task, follow this sequence:
1. Read the user request completely.
2. Inspect repository status and relevant instructions.
3. Identify affected layers and authoritative owners.
4. Read applicable architecture and security documents.
5. Inspect current implementations and tests.
6. Build a dependency and compatibility impact list.
7. Implement the smallest coherent change.
8. Add or update tests.
9. Update documentation.
10. Run focused validation.
11. Run broad validation appropriate to the change.
12. Review the diff for unrelated changes, secrets, generated noise, and compatibility risks.
13. Update `PROJECT_STATE.md` when appropriate.
14. Produce the required completion report.

Additional workflow rules:
- Continue through reasonable build or test failures instead of stopping after the first error.
- Fix errors caused by your work.
- If pre-existing failures block validation, separate them clearly from your own changes.

## 18. Multi-Agent And Codex-Specific Guidance

Codex is the primary AI agent for this repository.

Subagent rules:
- Divide work by independent subsystem or investigation area.
- Do not assign multiple agents to edit the same files concurrently.
- Use exploration agents for architecture review, test inventory, and dependency-impact gathering.
- Keep one primary agent responsible for integration, editing, and final validation.
- Require each subagent to report files inspected, findings, changes, tests, risks, and unresolved items.
- Independently verify subagent claims before accepting them.
- Do not let a subagent silently alter architecture.
- Avoid parallel work where one task depends on another task’s schema or API decision.

The primary agent must reconcile all work and run final repository-wide validation appropriate to the change.

## 19. Prohibited Approaches

Do not:
- Write official events directly from UI, plugins, providers, hosted routes, Swift, or JavaScript.
- Mutate projections as authoritative data.
- Edit or delete historical official events.
- Bypass permission checks.
- Store raw credentials outside approved credential storage.
- Log secrets.
- Create provider-specific architecture when the service framework already applies.
- Duplicate Rust domain logic in platform UIs.
- Relabel mock, fake, or stub providers as production-ready without real transport implementation.
- Introduce network-dependent ordinary tests.
- Silently change persisted serialization.
- Make destructive migrations without an explicit migration and recovery plan.
- Mark planned features complete.
- Claim tests were run when they were not.
- Replace substantial working modules merely to avoid understanding them.
- Suppress compiler, Clippy, or platform errors with blanket allowances or unsafe casts unless justified.
- Expose FFI or cross-boundary memory without defined ownership and deallocation rules.
- Mix unrelated cleanup into a focused fix.
- Update roadmap or status claims without verifying implementation.

## 20. Definition Of Done

Before calling work complete, verify:
- Requested behavior was implemented or investigated as requested.
- Architecture boundaries were preserved.
- Compatibility impacts were considered.
- Security and privacy checks were included.
- Tests were added or updated when practical.
- Focused validation passed.
- Broad validation appropriate to the change passed.
- Documentation was updated.
- `PROJECT_STATE.md` was updated when implementation state materially changed.
- No secrets were added.
- No unrelated work was reverted.
- The diff was reviewed.
- Unsupported claims were removed.
- Remaining limitations and risks were documented clearly.

## 21. Required Final Response Format

Use this structure in the final response:

### Summary
What was implemented or investigated.

### Architecture Impact
Which boundaries, event types, APIs, schemas, providers, or persistence layers changed.

### Files Changed
Group files by subsystem.

### Tests Added Or Changed
List regression, unit, integration, model, UI, bridge, and live-gated tests.

### Validation Performed
List every command and result.

### Validation Not Performed
List anything not tested and why.

### Compatibility And Migrations
Describe serialized, API, event, database, sync, ABI, or support-state implications.

### Security And Privacy
Describe permission, credential, redaction, and data-handling implications.

### Remaining Risks And TODOs
List concrete unresolved items.

### Git Status
State branch, commit status, pre-existing modifications, and whether a commit or push was performed.

Never state that the repository is fully complete unless the evidence supports that claim.

## 22. Current Repository-Specific Notes

This section is a verified snapshot of the repository as inspected on July 22, 2026. Update it whenever implementation status materially changes.

- Current workspace version: `0.3.0`.
- Current release target: v1 ships on November 24, 2026 with hosted web, native iOS, and Windows/macOS/Linux desktop.
- Workspace members: `crates/ham-api-contract`, `crates/ham-core`, `crates/ham-plugin-sdk`, `crates/ham-sync`, `crates/ham-sync-server`, `crates/ham-server`, `crates/ham-cli`, `crates/ham-gui`, `crates/ham-desktop`, `crates/ham-ios-ffi`, and `src-tauri`.
- Actual desktop state: a real Tauri v2 wrapper exists, bundles `crates/ham-gui/web`, exposes native dialog commands plus a restricted `/api/*` proxy, and packages desktop installers. The local backend is not yet embedded in-process or sidecar-launched automatically.
- Actual hosted-server state: `ham-server` is the hosted API boundary with durable SurrealDB metadata, route tests, role-scoped logbook access, provider settings, upload execution foundation, backups, divergence review, and sync endpoints. It is still beta, not production-hardened.
- Actual synchronization state: `ham-sync` implements LAN discovery and verification models, preview/pull/push logic including verified missing-tail pull apply, cloud/self-hosted sync models, durable self-hosted sync/report storage, guarded replay rules, durable offline mutation queue models with optional target-entity metadata, v0.2 absent/legacy queue migration, corrupt queue quarantine, interrupted atomic-write promotion, durable local sync identity records that persist stable device IDs while rotating discovery sessions, desktop/iOS queue hooks, desktop cloud reconnect auto-drain when auto-push is enabled, iOS FFI background retry planning/result classification with native Swift retry-plan/result bridge methods, Rust-planned official-event envelope decoding, Rust-owned pulled-event apply through `sync.remote_events.apply`, self-hosted/logbook-scoped push execution coordination, hosted `/api/v1/sync/push` request construction, self-hosted/logbook-scoped and hosted pull request construction, native pull fetch -> Rust apply coordination, partial-acceptance retry-result handling, typed queue health plus saved conflict-review display and durable identity decoding, Rust-owned iOS LAN trust snapshot/issue/accept/trust/rotate/revoke bridge commands with Keychain-backed credential references, queue-aware cloud push acknowledgment, structured conflict reports for divergent heads, missing dependencies, unsupported schemas, concurrent QSO corrections, and tombstone/restore overlaps, durable manual conflict-review records, explicit recovery-path decisions, deterministic shared sync golden tests for crash recovery, retry, duplicate/reordered delivery, verified missing-tail pull apply, clock-skewed timestamps, divergent heads, review resolution, legacy migration, restore replay, and LAN revocation, desktop/iOS corrective-event commands that submit normal proposals and resolve reviews with generated official event hashes, a guided browser conflict-review surface for saved reviews, structured conflict summaries, explicit recovery choices, and corrective QSO note events, durable LAN trust records with guided browser pairing/trust controls that separate generated endpoint auth codes from one-time pairing codes and support auth-credential rotation, GUI manual direct LAN HTTP preview/pull transport, HMAC-SHA256 signed LAN read endpoint authorization, and a GUI automatic IPv4/IPv6 multicast discovery worker that probes reachable peer identity before recording peers. Production iOS reciprocal LAN transport completion UX, stronger LAN key-exchange hardening, end-to-end cross-client branch review/reconciliation workflow qualification, physical-device LAN/iOS Local Network validation, real hosted web/desktop/iOS/self-hosted migration/recovery qualification, release-device iOS BGTask execution, real endpoint native sync transport qualification, and physical poor-network validation are still missing.
- Actual iOS state: native SwiftUI, SwiftData cache/projection models, Rust FFI bridge, Xcode project, Apple build/link scripts, shared scheme, unit tests, and iOS CI are present. App Store signing, TestFlight/App Store distribution, full offline/sync reconciliation, and production validation remain incomplete.
- Real versus mock providers:
  - Real but gated live transports: Club Log upload, QRZ Logbook upload, eQSL upload, QRZ XML lookup, HamQTH lookup, POTA spot fetch, DX Cluster bounded runtime controls.
  - Deferred live transports: LoTW upload, SOTAWatch live access.
  - Placeholder or fake-first providers remain for many map, weather, propagation, and other online-service categories.
- Known critical and high-priority gaps:
  - Consistent permission-scope enforcement across all workflows.
  - Production iOS reciprocal LAN transport completion UX, stronger LAN key-exchange hardening, physical-device LAN validation, and end-to-end sync branch-review/reconciliation workflow qualification.
  - Cross-OS Tauri package validation and release hardening.
  - Clean release-runner validation for OS credential backends.
  - Browser-level GUI tests.
  - Remaining production provider adapters and confirmation reconciliation.
- Validation currently supported by CI:
  - Change-aware CI on `ubuntu-latest`, `windows-latest`, and `macos-latest` runs formatting, API contract, governance, Clippy, tests, JavaScript syntax, platform builds, Tauri checks, and sync-server container validation as applicable.
  - The Security scanning workflow runs Cargo advisory checks, checked-in Semgrep rules with SARIF upload, and actionlint on pull requests and pushes to `dev`/`main`, weekly, and manually.
  - The Scorecard workflow publishes Scorecard SARIF from `main`.
  - The tagged release workflow builds release `ham-gui` archives and adds GitHub artifact attestations for future release archives and checksums before publishing assets.
- Current release blockers:
  - Production provider completeness and validation.
  - iOS reciprocal LAN transport completion UX, stronger LAN key-exchange hardening, and physical-device validation.
  - Desktop packaging hardening outside local Windows validation.
  - Permission-scope cleanup.
  - Browser-level GUI coverage.
  - Native iOS signing, App Store/TestFlight readiness, and full offline/sync/provider validation.

**Update this section whenever implementation status materially changes.**
