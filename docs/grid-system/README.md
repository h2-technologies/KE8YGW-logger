# Grid System

The platform uses Maidenhead grid squares as the primary amateur-radio location
encoding. `ham-core::map` provides a reusable grid engine for logging, maps,
awards, propagation, weather, POTA/SOTA, APRS, and future satellite workflows.

## Supported Operations

- Validate grid strings
- Normalize grid strings
- Determine precision
- Decode grid center to latitude/longitude
- Encode latitude/longitude to grid
- Calculate grid bounding boxes
- Calculate neighboring grids
- Calculate distance and bearing between grids through great-circle helpers

## Precision

The engine supports even-length Maidenhead locators from 2 through 10
characters. Common user-facing values include 4-character fields/squares and
6-character subsquares. The MVP tests cover sample grids including `FN20`,
`EN80`, `EM00`, `IO91`, and `JN58`.

## Distance and Bearing

Distance and bearing calculations use spherical great-circle math. Results
include kilometers, miles, nautical miles, initial bearing, and final bearing.
This is sufficient for daily logging, award context, and map visualization.
Future work can add ellipsoid-aware GIS calculations if needed.

## Current Limitations

- Regional band-plan or award-specific grid validation is outside this module.
- Antimeridian-aware polygon clipping is deferred to a future map renderer.
