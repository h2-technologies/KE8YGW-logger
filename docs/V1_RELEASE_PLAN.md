# v1.0 Release Plan

v1.0 is a hosted web and desktop release. iOS is not part of v1.0, and the
project must not treat a PWA, pinned website, or Home Screen install as the iOS
client.

## Target

- Hosted web app with login.
- Installable desktop app.
- Shared Rust core.
- Shared hosted/self-hosted API.
- Cloud/self-hosted sync.
- Production provider integrations.
- Production credential storage.
- API contracts clean enough for a future native iOS client.

## Explicit Non-Goals

- No native iOS app work.
- No PWA release target.
- No iOS Home Screen install documentation.
- No service-worker offline queue as the required mobile strategy.
- No claim that a pinned web app is the iOS client.
- No Rust FFI work for iOS in v1.0.

## Product Surfaces

### Hosted Web App

The hosted web app is the browser-accessible client for v1.0. It must support
login, account/session handling, logbook selection, QSO workflows, POTA/SOTA,
Net Control, ADIF import/export, provider configuration, sync status, and
diagnostics from the browser.

The web app may use browser capabilities where appropriate, but PWA
installability is not a release requirement and must not be described as the
mobile strategy.

### Desktop App

The desktop app is the installable local client for v1.0. It must package and
run on the supported desktop platforms, reuse the shared Rust core, use
production credential storage, and expose native desktop affordances where they
matter for file import/export and local operation.

### Shared Rust Core

The Rust core remains the source of truth for official event validation,
append-only logbook storage rules, projections, sync verification, provider
metadata, credential abstractions, ADIF import/export, POTA/SOTA, and Net
Control models.

### Hosted/Self-Hosted API

The hosted and self-hosted server must expose the same API contract. Deployment
mode may change URLs, storage backends, and operational controls, but it must
not create incompatible client behavior.

## v1.0 Engineering Work

- Add production login/session support for the hosted web app.
- Replace in-memory hosted/self-hosted sync storage with a durable server
  backend before real hosted use.
- Keep the existing safe replication checks for all sync paths.
- Add production credential backends for Windows Credential Manager, macOS
  Keychain, and Linux Secret Service.
- Replace mock/provider stubs with production adapters for the v1.0 provider
  set.
- Package the desktop client as an installable app.
- Document and test the API contract in `docs/API_CLIENT_CONTRACT.md`.
- Keep the API shape suitable for a future native iOS client without beginning
  native iOS implementation.

## Acceptance

- Web app works in a browser.
- Hosted web app supports login.
- Desktop app packages and runs.
- Hosted server works.
- Self-hosted server works.
- Cloud/self-hosted sync works against durable storage.
- API is documented and tested.
- API is suitable for a future native iOS client.
- Production provider integrations are available for the v1.0 provider set.
- Production credential storage is wired.
- No PWA-specific iOS deliverables are required.

## Release Blockers

- A required v1.0 workflow depends on PWA installability or iOS Home Screen
  install.
- Hosted and self-hosted APIs diverge.
- Server storage is still in-memory for hosted/self-hosted release use.
- Credentials require the insecure development fallback.
- Provider workflows still rely on mock-only implementations where production
  integration is part of the v1.0 target.
- API behavior is undocumented or lacks contract tests for future clients.
