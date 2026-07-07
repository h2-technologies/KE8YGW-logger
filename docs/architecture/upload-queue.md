# Upload Queue

The upload queue builds on the Unified Service Framework. Upload providers receive ADIF generated from QSO projections; they never mutate QSO records directly.

## Models

- `UploadTarget`: service-provider-backed destination such as LoTW, eQSL, Club Log, or QRZ Logbook.
- `UploadJob`: selected QSO IDs, target, status, and job items.
- `UploadJobItem`: per-QSO upload state.
- `UploadResult`: provider result summary.

## Official Events

The MVP defines official upload status events:

- `official.log.upload.queued`
- `official.log.upload.completed`
- `official.log.upload.failed`

These events record upload status without modifying QSO payloads.

## Provider Flow

1. Select visible, non-deleted QSOs from the QSO projection.
2. Generate ADIF from selected QSOs.
3. Send ADIF to a configured `LogUploadProvider`.
4. Store queue state as support state.
5. Append official upload status events where tied to specific QSOs.

## Current Limitations

- Providers are stubs until real credential storage and network integrations are added.
- Queue state is currently in-memory in the GUI.
- Confirmation pull and per-provider retry policy are future work.
