# v1.1 Native iOS Plan

v1.1 is the first iOS release. It must be a real native iOS app, not a PWA and
not a pinned hosted web app.

## Target

- Native SwiftUI frontend.
- App Store-ready Xcode project.
- Same API contracts as hosted web and desktop.
- Local offline queue.
- Keychain credential storage.
- Native document picker/share sheet for ADIF import/export.
- Native Maps integration.
- Native iPhone and iPad layouts.
- Optional Rust core sharing through UniFFI.

Rust FFI must not block the first native iOS version. If UniFFI is not ready,
the iOS app should ship against the documented HTTP/API contract and implement
client-side queueing in Swift.

## Architecture

The iOS app should use the same server-side contracts as the hosted web and
desktop clients. It should not receive a special mobile-only API unless that API
is also documented as part of the stable client contract.

Recommended app layers:

- SwiftUI views for iPhone and iPad.
- View models with explicit loading, error, empty, dirty, and offline states.
- API client generated or hand-written from `docs/API_CLIENT_CONTRACT.md`.
- Local persistence for offline pending mutations and cached read models.
- Keychain wrapper for tokens and provider secrets.
- Native document import/export services for ADIF.
- Native Maps adapter for map views, station/QSO visualization, and related
  workflows.
- Optional UniFFI bridge to shared Rust code where it is stable and lowers
  duplicated logic.

## Required Workflows

- Log in and restore a saved session.
- Select an existing logbook.
- Create a logbook where account permissions allow it.
- Create, edit, delete, and restore QSOs.
- Add QSO notes where supported by the API.
- Work while offline by adding mutations to a pending queue.
- Retry pending mutations when connectivity returns.
- Import ADIF through native document picker flows.
- Export ADIF through native document/share flows.
- Use POTA/SOTA workflows.
- Use Net Control workflows.
- Store secrets in Keychain.
- Show native iPhone and iPad layouts.
- Produce a TestFlight build.

## Offline Queue

The offline queue is native iOS application state. It must not depend on a
service-worker queue.

Queue requirements:

- Persist pending operations locally.
- Preserve operation order per logbook where ordering affects official event
  history.
- Store enough request metadata to retry safely.
- Avoid storing raw provider secrets in queue payloads.
- Surface pending, failed, retrying, and synced states in the UI.
- Reconcile accepted server events back into the local read model.
- Stop automatic retry on authorization, schema, or divergence errors that need
  user action.

## ADIF Import/Export

ADIF must use native iOS document flows:

- Import through document picker.
- Export through document picker or share sheet.
- Preserve file names where possible.
- Report parse/import errors in app-native UI.
- Never require the user to paste ADIF text into a web form.

## Maps

Native Maps integration should support:

- Station and QSO location display where coordinates/grid data are available.
- POTA/SOTA related map workflows when supported by the API.
- iPhone and iPad-appropriate map layouts.
- Privacy-aware location permission prompts only when the app needs device
  location.

## Acceptance

- Xcode project builds.
- SwiftUI app logs in.
- User can select/create logbook.
- User can create/edit/delete/restore QSOs.
- Offline pending queue works.
- ADIF import/export works through native iOS document flows.
- POTA/SOTA and Net Control work.
- Keychain stores secrets.
- App has privacy manifest/review checklist prepared.
- TestFlight build can be produced.
