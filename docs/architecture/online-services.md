# Online Services Ecosystem

Online Services turns the local-first logger into a connected operating
platform while preserving the core architecture:

Core -> Event Bus -> Unified Service Framework -> CredentialStore -> Providers -> GUI

No provider writes official log events directly. Uploads, confirmation imports,
lookups, spots, weather, propagation, and map services are provider-owned
integrations surfaced through shared core models.

## Provider Families

Registered provider metadata now covers:

- Logbooks: LoTW, eQSL, Club Log, QRZ Logbook, HRDLog
- Lookups: QRZ XML, HamQTH, FCC ULS, offline prefix fallback
- Spotting: DX Cluster, Reverse Beacon Network, POTA Spots, SOTAWatch
- Propagation: NOAA Space Weather and solar indices
- Weather: NOAA and Open-Meteo
- Maps: OpenStreetMap tiles, offline tile cache, reverse geocoder

The current implementation remains offline-testable by default. Club Log, QRZ
Logbook, and eQSL have gated live HTTP upload transports behind provider
settings and `CredentialStore` credential references. QRZ XML and HamQTH have
live response parsers, POTA has a live request builder and fixture parser, and
DX Cluster has a read-once Telnet client foundation. SOTAWatch live access is
deferred pending explicit API approval/terms handling. LoTW live upload remains
deferred until a safe TQSL/certificate-signing flow is modeled.

## Upload Engine

The upload engine builds on the existing upload queue and ADIF generation. It
adds:

- retry policy and bounded exponential backoff
- upload execution result model
- upload statistics
- provider health and missing credential states
- notification generation for upload outcomes

Providers expose upload capability through the service framework. Live provider
adapters resolve credentials through `CredentialStore`, submit ADIF generated
from official projections, and return redacted structured results. Confirmation
download remains fixture/foundation work until safe provider-specific matching
is modeled.

## Download Engine

Confirmation downloads are represented by `ConfirmationDownloadResponse` and
`ConfirmationRecord`. Downloaded confirmations append official status events;
they do not mutate QSO records in place.

Supported foundation targets:

- LoTW confirmations
- QRZ confirmation placeholder
- eQSL confirmation placeholder
- Club Log confirmation placeholder

## Spotting

The DX Cluster parser handles standard telnet spot lines and converts them into
the common `Spot` model. The live foundation is read-once connect/login/read
without an always-on daemon. POTA/SOTA spots also map into the same model,
allowing future click actions to center the map, run callsign lookup, tune a
rig, or start logging without provider-specific UI code.

## Automation and Notifications

Automation tasks are modeled for:

- upload every 10 minutes
- download confirmations hourly
- refresh propagation and weather every 30 minutes
- refresh DX spots continuously
- refresh POTA and SOTA spots every minute

Notifications are advisory runtime/support state. They are not official log
events and are safe for AI/tooling consumption.

## Durable Support State

Provider enablement, provider priority, service cache metadata, upload queue
state, online automation tasks, and notification support state are persisted in
versioned JSON support files. These files are sidecar state only: they are not
synced as official log events and must not contain credential secrets.

## Security

- Raw credentials never appear in provider config, runtime events, cache
  entries, diagnostic reports, or official events.
- Provider config references credential IDs.
- Network permissions are separated by service family.
- Automation management is admin-gated.
- Confirmation imports append official events through the core store API.

## Current Limitations

- Live network tests are not enabled in CI and require explicit provider env
  vars plus credentials.
- Hosted QRZ XML/HamQTH lookup and POTA/SOTA spot fetch routes are still pending.
- Telnet reconnect loops are not run as background services yet.
- Upload queue persistence and automatic scheduling remain future work.
- LoTW production upload is fake/scaffold only until TQSL signing is modeled.
