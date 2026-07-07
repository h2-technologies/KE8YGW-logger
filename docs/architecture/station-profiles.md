# Station And Equipment Profiles

Station and equipment data is support/config state, not official log state. The official event stream may reference a station profile, station configuration, or equipment IDs, but changing a profile or equipment record never rewrites historical QSOs.

## Models

- `StationProfile`: callsign defaults, grid/QTH, power, tags, and active state.
- `EquipmentItem`: radio, antenna, amplifier, tuner, rotor, interface, power supply, or accessory records.
- `StationConfiguration`: a named combination of profile and selected equipment.
- `StationBook`: local support-state container with active profile/configuration selection.

## Logger Flow

The GUI loads the active station profile from support storage and applies defaults to QSO proposal payloads before submitting to `ham-core`. User-entered fields and accepted lookup/rig suggestions are preserved; profile defaults only fill missing station fields.

## Persistence

`JsonStationBookStore` stores support state under the app support directory. This data is not append-only, not synced by default, and not part of hash-chain verification.

## Current Limitations

- No station profile editor POST flow beyond active-profile selection yet.
- No account-scoped multi-user policy UI.
- Equipment snapshots are referenced by ID; richer immutable equipment snapshots can be added when upload/award workflows need them.
