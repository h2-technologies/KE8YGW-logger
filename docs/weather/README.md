# Weather Framework

Weather is a provider-backed service for portable operation, EmComm, station
safety, maps, and future incident workflows.

## Models

- `CurrentWeather`: coordinate, timestamp, temperature, conditions, wind,
  provider ID, lightning placeholder, and radar placeholder.
- `Forecast`: coordinate, generated timestamp, forecast entries, and provider
  ID.
- `Wind`: speed, direction, and gust information.

## Providers

The MVP registers mock/placeholder providers for:

- NOAA-style weather
- Open-Meteo-style weather
- Local/manual weather

Providers must declare permissions, network requirements, health, config keys,
and credential requirements through the service framework.

## GUI

The Maps workspace includes a Weather panel fed by mock weather data near the
active station/profile coordinate. The model is ready for overlays and incident
weather views when real providers are implemented.

## Current Limitations

- No live weather API calls are made.
- Radar and lightning are placeholders.
- Credentialed weather providers must wait for real provider integrations and
  production credential storage.
