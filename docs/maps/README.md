# Mapping Framework

The mapping framework is a first-class core service consumer. It derives map
state from official projections, station support data, service providers, and
runtime diagnostics. The map does not own logbook business logic and never
writes official events directly.

## Architecture

Map data flows through:

Core -> Event Bus -> Map Service Framework -> Providers -> GUI Panels

`ham-core::map` owns reusable GIS models, Maidenhead grid helpers,
great-circle math, layer models, marker models, grayline snapshots, and mock
weather/propagation data. The GUI requests derived map state through
`/api/maps/state` and can toggle layer visibility through
`/api/maps/layer/toggle`.

## Providers

The MVP registers provider metadata through the Unified Service Framework:

- Offline placeholder map provider
- OpenStreetMap placeholder provider
- Mock map provider
- Mock weather provider
- VOACAP/propagation placeholder provider

Providers declare capabilities, priority, health, offline support, network
requirements, permissions, and any required credentials. Real tile, terrain,
geocoding, weather, and propagation backends should implement the same provider
contracts rather than bypassing the framework.

## Layers and Markers

Default layers include stations, QSOs, paths, POTA parks, SOTA summits, grid
overlay, grayline, propagation, weather, and satellite placeholders. Layers are
plugin-contributed, ordered, and independently enabled or disabled.

Markers support stations, operators, QSOs, parks, summits, repeaters, incidents,
weather, and satellites. Each marker carries a stable ID, title, description,
icon, layer, coordinates, click action, context menu, and safe metadata.

## QSO Map

QSO map objects are derived from `QsoCurrentStateProjection`. If a QSO has a
grid square, it can be shown as a marker. If the active station profile has a
grid, the map derives a great-circle path, distance, and bearing.

Deleted QSOs are excluded by default because the projection enforces normal
current-state visibility. Future map filters will expose deleted/admin views
only through explicit permissioned paths.

## Current Limitations

- The GUI map is a structured shell and preview, not a full interactive tile
  renderer yet.
- Real tile downloads, terrain, routing, satellite overlays, APRS, and live
  weather radar are deferred.
- Layer order is in-memory for the MVP.
- Antimeridian bounds handling is a future GIS refinement.
