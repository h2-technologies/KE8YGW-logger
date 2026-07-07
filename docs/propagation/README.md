# Propagation Framework

Propagation is modeled as a provider-backed service. The MVP establishes shared
data models and UI surfaces without committing to a single external provider.

## Models

- `SolarConditions`: SFI, A index, K index, X-ray class, geomagnetic summary,
  source, and timestamp.
- `BandConditions`: band, day/night rating, confidence, and notes.
- `PropagationForecast`: location, generated timestamp, solar conditions, band
  conditions, MUF placeholder, and provider ID.

## Providers

The current framework includes mock/placeholder providers for:

- Solar data
- NOAA-style future provider
- VOACAP-style future provider
- Grayline data derived locally

Providers should register through the Unified Service Framework and publish
runtime events such as `propagation.updated`, `solar.updated`, and
`band.conditions.updated`.

## GUI

The Maps workspace includes a Propagation panel that displays mock solar and
band-condition data. Future real providers can replace this data source without
changing the panel ownership model.

## Current Limitations

- No live solar feed is implemented yet.
- MUF and VOACAP outputs are placeholders.
- Band-condition scoring is illustrative until real provider integrations are
  added.
