//! GIS, mapping, propagation, and weather foundation.
//!
//! The map is a core service consumer. It derives markers, paths, layers, and
//! advisory overlays from projections and provider data; it does not own
//! official logbook business logic.

use std::f64::consts::PI;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Timelike, Utc};
use ham_plugin_sdk::{PluginCapability, ServiceType};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    bus::{BusEvent, EventBus, EventBusError, RuntimeEventEnvelope, RuntimeEventSeverity},
    projection::{QsoCurrentStateProjection, QsoRecord},
    service::{
        MapTileRequest, MapTileResponse, ProviderHealth, ServiceError, ServiceProvider,
        ServiceProviderMetadata, CAP_MAP_TILES_OFFLINE, CAP_MAP_TILES_ONLINE,
    },
};

const EARTH_RADIUS_KM: f64 = 6_371.008_8;
const KM_TO_MILES: f64 = 0.621_371_192;
const KM_TO_NAUTICAL_MILES: f64 = 0.539_956_803;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Coordinate {
    pub latitude: f64,
    pub longitude: f64,
}

impl Coordinate {
    pub fn new(latitude: f64, longitude: f64) -> Result<Self, MapError> {
        if !(-90.0..=90.0).contains(&latitude) {
            return Err(MapError::InvalidCoordinate(
                "latitude out of range".to_owned(),
            ));
        }
        if !(-180.0..=180.0).contains(&longitude) {
            return Err(MapError::InvalidCoordinate(
                "longitude out of range".to_owned(),
            ));
        }
        Ok(Self {
            latitude,
            longitude,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GridSquare {
    pub value: String,
    pub precision: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeoPoint {
    pub coordinate: Coordinate,
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeoBounds {
    pub south_west: Coordinate,
    pub north_east: Coordinate,
}

impl GeoBounds {
    pub fn contains(&self, coordinate: Coordinate) -> bool {
        coordinate.latitude >= self.south_west.latitude
            && coordinate.latitude <= self.north_east.latitude
            && coordinate.longitude >= self.south_west.longitude
            && coordinate.longitude <= self.north_east.longitude
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeoPath {
    pub points: Vec<Coordinate>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeoPolygon {
    pub vertices: Vec<Coordinate>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistanceUnit {
    Kilometers,
    Miles,
    NauticalMiles,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DistanceResult {
    pub kilometers: f64,
    pub miles: f64,
    pub nautical_miles: f64,
}

impl DistanceResult {
    pub fn value(&self, unit: DistanceUnit) -> f64 {
        match unit {
            DistanceUnit::Kilometers => self.kilometers,
            DistanceUnit::Miles => self.miles,
            DistanceUnit::NauticalMiles => self.nautical_miles,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BearingResult {
    pub initial_degrees: f64,
    pub final_degrees: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElevationResult {
    pub meters: Option<f64>,
    pub provider_id: String,
    pub placeholder: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapMarkerType {
    Station,
    Operator,
    Qso,
    Park,
    Summit,
    Repeater,
    Incident,
    Weather,
    Satellite,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapMarker {
    pub marker_id: String,
    pub marker_type: MapMarkerType,
    pub title: String,
    pub description: String,
    pub icon: String,
    pub layer_id: String,
    pub coordinate: Coordinate,
    pub click_action: String,
    pub context_menu: Vec<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MapLayerKind {
    QsoMarkers,
    StationMarkers,
    PotaParks,
    SotaSummits,
    GridOverlay,
    DxccOverlay,
    Grayline,
    Routes,
    PropagationOverlay,
    WeatherOverlay,
    SatelliteOverlay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapLayer {
    pub layer_id: String,
    pub title: String,
    pub kind: MapLayerKind,
    pub enabled: bool,
    pub order: i32,
    pub source_plugin_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapOverlay {
    pub overlay_id: String,
    pub layer_id: String,
    pub title: String,
    pub geometry: Value,
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapLayerStack {
    pub layers: Vec<MapLayer>,
}

impl MapLayerStack {
    pub fn default_layers() -> Self {
        Self {
            layers: vec![
                layer(
                    "station",
                    "Stations",
                    MapLayerKind::StationMarkers,
                    10,
                    true,
                ),
                layer("qso", "QSOs", MapLayerKind::QsoMarkers, 20, true),
                layer("routes", "Paths", MapLayerKind::Routes, 30, true),
                layer("pota", "POTA Parks", MapLayerKind::PotaParks, 40, true),
                layer("sota", "SOTA Summits", MapLayerKind::SotaSummits, 50, true),
                layer("grid", "Grid Overlay", MapLayerKind::GridOverlay, 60, true),
                layer("grayline", "Grayline", MapLayerKind::Grayline, 70, true),
                layer(
                    "propagation",
                    "Propagation",
                    MapLayerKind::PropagationOverlay,
                    80,
                    false,
                ),
                layer(
                    "weather",
                    "Weather",
                    MapLayerKind::WeatherOverlay,
                    90,
                    false,
                ),
                layer(
                    "satellite",
                    "Satellites",
                    MapLayerKind::SatelliteOverlay,
                    100,
                    false,
                ),
            ],
        }
    }

    pub fn set_enabled(&mut self, layer_id: &str, enabled: bool) -> Result<(), MapError> {
        let layer = self
            .layers
            .iter_mut()
            .find(|layer| layer.layer_id == layer_id)
            .ok_or_else(|| MapError::LayerNotFound(layer_id.to_owned()))?;
        layer.enabled = enabled;
        Ok(())
    }

    pub fn set_order(&mut self, layer_id: &str, order: i32) -> Result<(), MapError> {
        let layer = self
            .layers
            .iter_mut()
            .find(|layer| layer.layer_id == layer_id)
            .ok_or_else(|| MapError::LayerNotFound(layer_id.to_owned()))?;
        layer.order = order;
        self.layers.sort_by_key(|layer| layer.order);
        Ok(())
    }
}

fn layer(layer_id: &str, title: &str, kind: MapLayerKind, order: i32, enabled: bool) -> MapLayer {
    MapLayer {
        layer_id: layer_id.to_owned(),
        title: title.to_owned(),
        kind,
        enabled,
        order,
        source_plugin_id: "plugin.maps".to_owned(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QsoMapObject {
    pub marker: MapMarker,
    pub path: Option<GeoPath>,
    pub distance: Option<DistanceResult>,
    pub bearing: Option<BearingResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QsoMapFilter {
    pub band: Option<String>,
    pub mode: Option<String>,
    pub operator: Option<String>,
    pub station: Option<String>,
    pub entity: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GraylineSnapshot {
    pub calculated_at: DateTime<Utc>,
    pub sun_subpoint: Coordinate,
    pub terminator: GeoPath,
    pub day_center: Coordinate,
    pub night_center: Coordinate,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolarConditions {
    pub provider_id: String,
    pub observed_at: DateTime<Utc>,
    pub solar_flux_index: Option<f32>,
    pub a_index: Option<f32>,
    pub k_index: Option<f32>,
    pub xray_class: Option<String>,
    pub geomagnetic_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BandConditions {
    pub band: String,
    pub rating: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropagationForecast {
    pub provider_id: String,
    pub generated_at: DateTime<Utc>,
    pub solar: SolarConditions,
    pub bands: Vec<BandConditions>,
    pub muf_mhz: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Wind {
    pub speed_kph: Option<f32>,
    pub direction_degrees: Option<u16>,
    pub gust_kph: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CurrentWeather {
    pub provider_id: String,
    pub observed_at: DateTime<Utc>,
    pub temperature_c: Option<f32>,
    pub condition: String,
    pub wind: Option<Wind>,
    pub lightning_placeholder: bool,
    pub radar_placeholder: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Forecast {
    pub provider_id: String,
    pub generated_at: DateTime<Utc>,
    pub hours: Vec<CurrentWeather>,
}

#[async_trait]
pub trait MapProvider: ServiceProvider {
    async fn fetch_tile(&self, request: MapTileRequest) -> Result<MapTileResponse, ServiceError>;
    async fn geocode(&self, query: &str) -> Result<Vec<GeoPoint>, ServiceError>;
    async fn reverse_geocode(&self, coordinate: Coordinate)
        -> Result<Option<String>, ServiceError>;
}

#[derive(Debug, Clone)]
pub struct PlaceholderMapProvider {
    metadata: ServiceProviderMetadata,
}

impl PlaceholderMapProvider {
    pub fn offline() -> Self {
        Self {
            metadata: map_provider_metadata(
                "offline-placeholder",
                "Offline Placeholder Map",
                vec![
                    CAP_MAP_TILES_OFFLINE.to_owned(),
                    "map.vector.placeholder".to_owned(),
                    "map.overlays".to_owned(),
                ],
                100,
                true,
                false,
            ),
        }
    }

    pub fn open_street_map() -> Self {
        let mut metadata = map_provider_metadata(
            "openstreetmap-placeholder",
            "OpenStreetMap Placeholder",
            vec![
                CAP_MAP_TILES_ONLINE.to_owned(),
                "map.raster.placeholder".to_owned(),
                "map.geocoding.placeholder".to_owned(),
            ],
            50,
            false,
            true,
        );
        metadata.required_config_keys = vec!["osm.tile_url_template".to_owned()];
        Self { metadata }
    }

    pub fn mock() -> Self {
        Self {
            metadata: map_provider_metadata(
                "mock-map",
                "Mock Map Provider",
                vec![
                    CAP_MAP_TILES_OFFLINE.to_owned(),
                    "map.geocoding".to_owned(),
                    "map.reverse_geocoding".to_owned(),
                    "map.elevation.placeholder".to_owned(),
                ],
                90,
                true,
                false,
            ),
        }
    }
}

#[async_trait]
impl ServiceProvider for PlaceholderMapProvider {
    fn metadata(&self) -> ServiceProviderMetadata {
        self.metadata.clone()
    }

    async fn health(&self) -> ProviderHealth {
        ProviderHealth::healthy(self.metadata.provider_id.clone(), "Map provider ready")
    }
}

#[async_trait]
impl MapProvider for PlaceholderMapProvider {
    async fn fetch_tile(&self, request: MapTileRequest) -> Result<MapTileResponse, ServiceError> {
        Ok(MapTileResponse {
            provider_id: self.metadata.provider_id.clone(),
            mime_type: "application/octet-stream".to_owned(),
            tile_bytes: format!("placeholder:{}/{}/{}", request.zoom, request.x, request.y)
                .into_bytes(),
        })
    }

    async fn geocode(&self, query: &str) -> Result<Vec<GeoPoint>, ServiceError> {
        let coordinate = maidenhead_to_coordinate(query)
            .or_else(|_| Coordinate::new(0.0, 0.0))
            .map_err(|error| ServiceError::Provider(error.to_string()))?;
        Ok(vec![GeoPoint {
            coordinate,
            label: Some(query.to_owned()),
        }])
    }

    async fn reverse_geocode(
        &self,
        coordinate: Coordinate,
    ) -> Result<Option<String>, ServiceError> {
        Ok(Some(encode_maidenhead(coordinate, 6).map_err(|error| {
            ServiceError::Provider(error.to_string())
        })?))
    }
}

pub fn map_provider_metadata(
    provider_id: &str,
    display_name: &str,
    capabilities: Vec<String>,
    priority: i32,
    offline: bool,
    network: bool,
) -> ServiceProviderMetadata {
    ServiceProviderMetadata::new(
        provider_id,
        ServiceType::MapTiles,
        display_name,
        "0.1.0",
        "plugin.maps",
        capabilities,
        vec![PluginCapability::MapView],
        priority,
        offline,
        network,
    )
}

pub fn validate_maidenhead(grid: &str) -> bool {
    normalize_maidenhead(grid).is_ok()
}

pub fn normalize_maidenhead(grid: &str) -> Result<String, MapError> {
    let grid = grid.trim();
    if !(2..=10).contains(&grid.len()) || !grid.len().is_multiple_of(2) {
        return Err(MapError::InvalidGrid(grid.to_owned()));
    }
    let chars = grid.chars().collect::<Vec<_>>();
    for (index, pair) in chars.chunks(2).enumerate() {
        let valid = match index {
            0 => pair
                .iter()
                .all(|ch| matches!(ch.to_ascii_uppercase(), 'A'..='R')),
            1 | 3 => pair.iter().all(|ch| ch.is_ascii_digit()),
            2 | 4 => pair
                .iter()
                .all(|ch| matches!(ch.to_ascii_uppercase(), 'A'..='X')),
            _ => false,
        };
        if !valid {
            return Err(MapError::InvalidGrid(grid.to_owned()));
        }
    }
    let mut normalized = String::with_capacity(grid.len());
    for (index, ch) in chars.into_iter().enumerate() {
        if index < 2 || index >= 4 && !ch.is_ascii_digit() {
            normalized.push(ch.to_ascii_uppercase());
        } else {
            normalized.push(ch);
        }
    }
    Ok(normalized)
}

pub fn grid_precision(grid: &str) -> Result<usize, MapError> {
    Ok(normalize_maidenhead(grid)?.len())
}

pub fn maidenhead_to_coordinate(grid: &str) -> Result<Coordinate, MapError> {
    let bounds = maidenhead_bounds(grid)?;
    Coordinate::new(
        (bounds.south_west.latitude + bounds.north_east.latitude) / 2.0,
        (bounds.south_west.longitude + bounds.north_east.longitude) / 2.0,
    )
}

pub fn maidenhead_bounds(grid: &str) -> Result<GeoBounds, MapError> {
    let normalized = normalize_maidenhead(grid)?;
    let chars = normalized.chars().collect::<Vec<_>>();
    let mut lon = -180.0;
    let mut lat = -90.0;
    let mut lon_size = 20.0;
    let mut lat_size = 10.0;

    for pair_index in 0..(chars.len() / 2) {
        let (pair_lon_size, pair_lat_size) = maidenhead_cell_size(pair_index);
        lon_size = pair_lon_size;
        lat_size = pair_lat_size;
        let lon_ch = chars[pair_index * 2];
        let lat_ch = chars[pair_index * 2 + 1];
        let (lon_value, lat_value) = match pair_index {
            0 => ((lon_ch as u8 - b'A') as f64, (lat_ch as u8 - b'A') as f64),
            1 | 3 => (
                lon_ch
                    .to_digit(10)
                    .ok_or_else(|| MapError::InvalidGrid(normalized.clone()))?
                    as f64,
                lat_ch
                    .to_digit(10)
                    .ok_or_else(|| MapError::InvalidGrid(normalized.clone()))?
                    as f64,
            ),
            2 | 4 => ((lon_ch as u8 - b'A') as f64, (lat_ch as u8 - b'A') as f64),
            _ => unreachable!("normalize_maidenhead limits precision"),
        };
        lon += lon_value * lon_size;
        lat += lat_value * lat_size;
    }

    Ok(GeoBounds {
        south_west: Coordinate::new(lat, lon)?,
        north_east: Coordinate::new(lat + lat_size, lon + lon_size)?,
    })
}

pub fn encode_maidenhead(coordinate: Coordinate, precision: usize) -> Result<String, MapError> {
    if !(2..=10).contains(&precision) || !precision.is_multiple_of(2) {
        return Err(MapError::InvalidPrecision(precision));
    }
    let mut lon = coordinate.longitude + 180.0;
    let mut lat = coordinate.latitude + 90.0;
    let mut grid = String::with_capacity(precision);

    for pair_index in 0..(precision / 2) {
        let (lon_size, lat_size) = maidenhead_cell_size(pair_index);
        let radix = match pair_index {
            0 => 18.0,
            1 | 3 => 10.0,
            2 | 4 => 24.0,
            _ => unreachable!("precision is bounded"),
        };
        let lon_value = (lon / lon_size).floor().clamp(0.0, radix - 1.0) as u8;
        let lat_value = (lat / lat_size).floor().clamp(0.0, radix - 1.0) as u8;
        match pair_index {
            0 | 2 | 4 => {
                grid.push((b'A' + lon_value) as char);
                grid.push((b'A' + lat_value) as char);
            }
            1 | 3 => {
                grid.push((b'0' + lon_value) as char);
                grid.push((b'0' + lat_value) as char);
            }
            _ => {}
        }
        lon -= lon_value as f64 * lon_size;
        lat -= lat_value as f64 * lat_size;
    }

    Ok(grid)
}

fn maidenhead_cell_size(pair_index: usize) -> (f64, f64) {
    match pair_index {
        0 => (20.0, 10.0),
        1 => (2.0, 1.0),
        2 => (2.0 / 24.0, 1.0 / 24.0),
        3 => (2.0 / 240.0, 1.0 / 240.0),
        4 => (2.0 / 5_760.0, 1.0 / 5_760.0),
        _ => unreachable!("normalize_maidenhead limits precision"),
    }
}

pub fn neighbor_grids(grid: &str) -> Result<Vec<String>, MapError> {
    let normalized = normalize_maidenhead(grid)?;
    let bounds = maidenhead_bounds(&normalized)?;
    let precision = normalized.len();
    let lat_step = bounds.north_east.latitude - bounds.south_west.latitude;
    let lon_step = bounds.north_east.longitude - bounds.south_west.longitude;
    let center = maidenhead_to_coordinate(&normalized)?;
    let mut neighbors = Vec::new();
    for dlat in [-1.0, 0.0, 1.0] {
        for dlon in [-1.0, 0.0, 1.0] {
            if dlat == 0.0 && dlon == 0.0 {
                continue;
            }
            let lat = (center.latitude + dlat * lat_step).clamp(-89.999_999, 89.999_999);
            let lon = wrap_longitude(center.longitude + dlon * lon_step);
            neighbors.push(encode_maidenhead(Coordinate::new(lat, lon)?, precision)?);
        }
    }
    neighbors.sort();
    neighbors.dedup();
    Ok(neighbors)
}

pub fn great_circle_distance(from: Coordinate, to: Coordinate) -> DistanceResult {
    let from_lat = deg_to_rad(from.latitude);
    let to_lat = deg_to_rad(to.latitude);
    let delta_lat = deg_to_rad(to.latitude - from.latitude);
    let delta_lon = deg_to_rad(to.longitude - from.longitude);
    let a = (delta_lat / 2.0).sin().powi(2)
        + from_lat.cos() * to_lat.cos() * (delta_lon / 2.0).sin().powi(2);
    let kilometers = EARTH_RADIUS_KM * 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    DistanceResult {
        kilometers,
        miles: kilometers * KM_TO_MILES,
        nautical_miles: kilometers * KM_TO_NAUTICAL_MILES,
    }
}

pub fn initial_bearing(from: Coordinate, to: Coordinate) -> f64 {
    let from_lat = deg_to_rad(from.latitude);
    let to_lat = deg_to_rad(to.latitude);
    let delta_lon = deg_to_rad(to.longitude - from.longitude);
    let y = delta_lon.sin() * to_lat.cos();
    let x = from_lat.cos() * to_lat.sin() - from_lat.sin() * to_lat.cos() * delta_lon.cos();
    normalize_degrees(rad_to_deg(y.atan2(x)))
}

pub fn final_bearing(from: Coordinate, to: Coordinate) -> f64 {
    normalize_degrees(initial_bearing(to, from) + 180.0)
}

pub fn bearing_between(from: Coordinate, to: Coordinate) -> BearingResult {
    BearingResult {
        initial_degrees: initial_bearing(from, to),
        final_degrees: final_bearing(from, to),
    }
}

pub fn great_circle_midpoint(from: Coordinate, to: Coordinate) -> Result<Coordinate, MapError> {
    let lat1 = deg_to_rad(from.latitude);
    let lon1 = deg_to_rad(from.longitude);
    let lat2 = deg_to_rad(to.latitude);
    let delta_lon = deg_to_rad(to.longitude - from.longitude);
    let bx = lat2.cos() * delta_lon.cos();
    let by = lat2.cos() * delta_lon.sin();
    let lat3 = (lat1.sin() + lat2.sin()).atan2(((lat1.cos() + bx).powi(2) + by.powi(2)).sqrt());
    let lon3 = lon1 + by.atan2(lat1.cos() + bx);
    Coordinate::new(rad_to_deg(lat3), wrap_longitude(rad_to_deg(lon3)))
}

pub fn great_circle_path(
    from: Coordinate,
    to: Coordinate,
    segments: usize,
) -> Result<GeoPath, MapError> {
    let segments = segments.max(1);
    let mut points = Vec::with_capacity(segments + 1);
    for index in 0..=segments {
        let fraction = index as f64 / segments as f64;
        points.push(interpolate_great_circle(from, to, fraction)?);
    }
    Ok(GeoPath { points })
}

fn interpolate_great_circle(
    from: Coordinate,
    to: Coordinate,
    fraction: f64,
) -> Result<Coordinate, MapError> {
    let lat1 = deg_to_rad(from.latitude);
    let lon1 = deg_to_rad(from.longitude);
    let lat2 = deg_to_rad(to.latitude);
    let lon2 = deg_to_rad(to.longitude);
    let distance = great_circle_distance(from, to).kilometers / EARTH_RADIUS_KM;
    if distance.abs() < f64::EPSILON {
        return Ok(from);
    }
    let a = ((1.0 - fraction) * distance).sin() / distance.sin();
    let b = (fraction * distance).sin() / distance.sin();
    let x = a * lat1.cos() * lon1.cos() + b * lat2.cos() * lon2.cos();
    let y = a * lat1.cos() * lon1.sin() + b * lat2.cos() * lon2.sin();
    let z = a * lat1.sin() + b * lat2.sin();
    let lat = z.atan2((x.powi(2) + y.powi(2)).sqrt());
    let lon = y.atan2(x);
    Coordinate::new(rad_to_deg(lat), wrap_longitude(rad_to_deg(lon)))
}

pub fn qso_map_objects(
    projection: &QsoCurrentStateProjection,
    station_coordinate: Option<Coordinate>,
    filter: Option<&QsoMapFilter>,
) -> Vec<QsoMapObject> {
    projection
        .current_qsos()
        .into_iter()
        .filter(|qso| qso_matches_map_filter(qso, filter))
        .filter_map(|qso| qso_to_map_object(qso, station_coordinate))
        .collect()
}

fn qso_matches_map_filter(qso: &QsoRecord, filter: Option<&QsoMapFilter>) -> bool {
    let Some(filter) = filter else {
        return true;
    };
    field_matches(&qso.payload, "band", filter.band.as_deref())
        && field_matches(&qso.payload, "mode", filter.mode.as_deref())
        && field_matches(
            &qso.payload,
            "operator_callsign",
            filter.operator.as_deref(),
        )
        && field_matches(&qso.payload, "station_callsign", filter.station.as_deref())
        && (field_matches(&qso.payload, "entity", filter.entity.as_deref())
            || field_matches(&qso.payload, "country", filter.entity.as_deref()))
}

fn field_matches(payload: &Value, key: &str, expected: Option<&str>) -> bool {
    expected.is_none_or(|expected| {
        payload
            .get(key)
            .and_then(Value::as_str)
            .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
    })
}

fn qso_to_map_object(
    qso: &QsoRecord,
    station_coordinate: Option<Coordinate>,
) -> Option<QsoMapObject> {
    let coordinate = qso
        .payload
        .get("latitude")
        .and_then(Value::as_f64)
        .zip(qso.payload.get("longitude").and_then(Value::as_f64))
        .and_then(|(lat, lon)| Coordinate::new(lat, lon).ok())
        .or_else(|| {
            qso.payload
                .get("grid")
                .and_then(Value::as_str)
                .and_then(|grid| maidenhead_to_coordinate(grid).ok())
        })?;
    let title = qso
        .payload
        .get("contacted_callsign")
        .and_then(Value::as_str)
        .unwrap_or("QSO")
        .to_owned();
    let path =
        station_coordinate.and_then(|station| great_circle_path(station, coordinate, 32).ok());
    let distance = station_coordinate.map(|station| great_circle_distance(station, coordinate));
    let bearing = station_coordinate.map(|station| bearing_between(station, coordinate));
    Some(QsoMapObject {
        marker: MapMarker {
            marker_id: format!("qso-{}", qso.qso_id),
            marker_type: MapMarkerType::Qso,
            title: title.clone(),
            description: format!(
                "{} {} {}",
                qso.payload
                    .get("band")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                qso.payload
                    .get("mode")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                qso.payload
                    .get("grid")
                    .and_then(Value::as_str)
                    .unwrap_or("")
            )
            .trim()
            .to_owned(),
            icon: "radio".to_owned(),
            layer_id: "qso".to_owned(),
            coordinate,
            click_action: format!("open-qso:{}", qso.qso_id),
            context_menu: vec!["Open QSO".to_owned(), "Center map".to_owned()],
            metadata: json!({
                "qso_id": qso.qso_id,
                "entity": qso.payload.get("entity").or_else(|| qso.payload.get("country")),
                "park": qso.payload.get("park_id"),
                "summit": qso.payload.get("summit_id"),
                "station_profile_id": qso.payload.get("station_profile_id"),
            }),
        },
        path,
        distance,
        bearing,
    })
}

pub fn station_markers_from_profiles(profiles: &[Value]) -> Vec<MapMarker> {
    profiles
        .iter()
        .filter_map(|profile| {
            let grid = profile.get("default_grid").and_then(Value::as_str)?;
            let coordinate = maidenhead_to_coordinate(grid).ok()?;
            let station_profile_id = profile.get("station_profile_id")?;
            Some(MapMarker {
                marker_id: format!("station-{station_profile_id}"),
                marker_type: MapMarkerType::Station,
                title: profile
                    .get("display_name")
                    .and_then(Value::as_str)
                    .unwrap_or("Station")
                    .to_owned(),
                description: profile
                    .get("station_callsign")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_owned(),
                icon: "home".to_owned(),
                layer_id: "station".to_owned(),
                coordinate,
                click_action: format!("switch-station:{station_profile_id}"),
                context_menu: vec!["Switch active station".to_owned()],
                metadata: profile.clone(),
            })
        })
        .collect()
}

pub fn grayline_snapshot(at: DateTime<Utc>) -> Result<GraylineSnapshot, MapError> {
    let subpoint = solar_subpoint(at)?;
    let mut points = Vec::new();
    for lon in (-180..=180).step_by(5) {
        let lon_rad = deg_to_rad(lon as f64 - subpoint.longitude);
        let decl = deg_to_rad(subpoint.latitude);
        let lat = rad_to_deg((-lon_rad.cos() / decl.tan()).atan());
        if lat.is_finite() {
            points.push(Coordinate::new(lat.clamp(-90.0, 90.0), lon as f64)?);
        }
    }
    Ok(GraylineSnapshot {
        calculated_at: at,
        sun_subpoint: subpoint,
        terminator: GeoPath { points },
        day_center: subpoint,
        night_center: Coordinate::new(
            -subpoint.latitude,
            wrap_longitude(subpoint.longitude + 180.0),
        )?,
    })
}

fn solar_subpoint(at: DateTime<Utc>) -> Result<Coordinate, MapError> {
    let day = at.ordinal() as f64;
    let hour = at.hour() as f64 + at.minute() as f64 / 60.0 + at.second() as f64 / 3600.0;
    let gamma = 2.0 * PI / 365.0 * (day - 1.0 + (hour - 12.0) / 24.0);
    let decl = 0.006_918 - 0.399_912 * gamma.cos() + 0.070_257 * gamma.sin()
        - 0.006_758 * (2.0 * gamma).cos()
        + 0.000_907 * (2.0 * gamma).sin()
        - 0.002_697 * (3.0 * gamma).cos()
        + 0.001_48 * (3.0 * gamma).sin();
    let equation = 229.18
        * (0.000_075 + 0.001_868 * gamma.cos()
            - 0.032_077 * gamma.sin()
            - 0.014_615 * (2.0 * gamma).cos()
            - 0.040_849 * (2.0 * gamma).sin());
    let subsolar_lon = wrap_longitude(180.0 - (hour * 15.0 + equation / 4.0));
    Coordinate::new(rad_to_deg(decl), subsolar_lon)
}

pub fn mock_propagation_forecast() -> PropagationForecast {
    let solar = SolarConditions {
        provider_id: "mock-propagation".to_owned(),
        observed_at: Utc::now(),
        solar_flux_index: Some(145.0),
        a_index: Some(8.0),
        k_index: Some(2.0),
        xray_class: Some("C1.0".to_owned()),
        geomagnetic_summary: Some("Quiet to unsettled".to_owned()),
    };
    PropagationForecast {
        provider_id: "mock-propagation".to_owned(),
        generated_at: Utc::now(),
        solar,
        bands: vec![
            BandConditions {
                band: "20m".to_owned(),
                rating: "good".to_owned(),
                summary: "Daytime DX likely".to_owned(),
            },
            BandConditions {
                band: "40m".to_owned(),
                rating: "fair".to_owned(),
                summary: "Regional evening coverage".to_owned(),
            },
        ],
        muf_mhz: Some(21.0),
    }
}

pub fn mock_weather(coordinate: Coordinate) -> CurrentWeather {
    CurrentWeather {
        provider_id: "mock-weather".to_owned(),
        observed_at: Utc::now(),
        temperature_c: Some(21.0),
        condition: format!(
            "Mock clear weather near {:.2},{:.2}",
            coordinate.latitude, coordinate.longitude
        ),
        wind: Some(Wind {
            speed_kph: Some(10.0),
            direction_degrees: Some(270),
            gust_kph: None,
        }),
        lightning_placeholder: true,
        radar_placeholder: true,
    }
}

pub async fn publish_map_runtime_event<B: EventBus>(
    bus: &B,
    event_type: &str,
    severity: RuntimeEventSeverity,
    summary: impl Into<String>,
    payload: Option<Value>,
    device_id: Uuid,
) -> Result<(), EventBusError> {
    bus.publish(BusEvent::Runtime(RuntimeEventEnvelope::new(
        event_type,
        severity,
        "ham-core.maps",
        Some("plugin.maps".to_owned()),
        Uuid::new_v4(),
        Uuid::new_v4(),
        device_id,
        Some("maps".to_owned()),
        summary,
        payload,
        None,
    )))
    .await
    .map(|_| ())
}

fn deg_to_rad(value: f64) -> f64 {
    value.to_radians()
}

fn rad_to_deg(value: f64) -> f64 {
    value.to_degrees()
}

fn normalize_degrees(value: f64) -> f64 {
    ((value % 360.0) + 360.0) % 360.0
}

fn wrap_longitude(value: f64) -> f64 {
    let mut wrapped = ((value + 180.0) % 360.0 + 360.0) % 360.0 - 180.0;
    if wrapped == -180.0 {
        wrapped = 180.0;
    }
    wrapped
}

#[derive(Debug, Error)]
pub enum MapError {
    #[error("invalid coordinate: {0}")]
    InvalidCoordinate(String),
    #[error("invalid Maidenhead grid {0}")]
    InvalidGrid(String),
    #[error("invalid Maidenhead precision {0}")]
    InvalidPrecision(usize),
    #[error("map layer {0} not found")]
    LayerNotFound(String),
    #[error("map service error: {0}")]
    Service(#[from] ServiceError),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(left: f64, right: f64, tolerance: f64) {
        assert!(
            (left - right).abs() <= tolerance,
            "{left} not within {tolerance} of {right}"
        );
    }

    #[test]
    fn maidenhead_validation_and_normalization_work() {
        assert_eq!(normalize_maidenhead("fn20").unwrap(), "FN20");
        assert!(validate_maidenhead("EN80"));
        assert!(validate_maidenhead("EM00"));
        assert!(validate_maidenhead("IO91"));
        assert!(validate_maidenhead("JN58"));
        assert!(!validate_maidenhead("ZZ99"));
    }

    #[test]
    fn maidenhead_decodes_known_grid_centers() {
        let fn20 = maidenhead_to_coordinate("FN20").unwrap();
        close(fn20.latitude, 40.5, 0.1);
        close(fn20.longitude, -75.0, 0.1);
        let jn58 = maidenhead_to_coordinate("JN58").unwrap();
        close(jn58.latitude, 48.5, 0.1);
        close(jn58.longitude, 11.0, 0.1);
    }

    #[test]
    fn maidenhead_encode_round_trips_grid_area() {
        for grid in ["FN20", "EN80", "EM00", "IO91", "JN58"] {
            let center = maidenhead_to_coordinate(grid).unwrap();
            let encoded = encode_maidenhead(center, grid.len()).unwrap();
            assert_eq!(encoded, grid);
            assert!(maidenhead_bounds(grid).unwrap().contains(center));
        }
    }

    #[test]
    fn maidenhead_neighbors_and_precision_work() {
        assert_eq!(grid_precision("EN91").unwrap(), 4);
        let neighbors = neighbor_grids("EN91").unwrap();
        assert_eq!(neighbors.len(), 8);
        assert!(neighbors.contains(&"EN80".to_owned()));
    }

    #[test]
    fn great_circle_distance_bearing_midpoint_and_path_work() {
        let cleveland = Coordinate::new(41.4993, -81.6944).unwrap();
        let london = Coordinate::new(51.5074, -0.1278).unwrap();
        let distance = great_circle_distance(cleveland, london);
        close(distance.kilometers, 6_000.0, 250.0);
        let bearing = bearing_between(cleveland, london);
        assert!((35.0..60.0).contains(&bearing.initial_degrees));
        let midpoint = great_circle_midpoint(cleveland, london).unwrap();
        assert!(midpoint.latitude > 45.0);
        assert_eq!(
            great_circle_path(cleveland, london, 8)
                .unwrap()
                .points
                .len(),
            9
        );
    }

    #[test]
    fn markers_layers_and_provider_metadata_serialize() {
        let marker = MapMarker {
            marker_id: "station-1".to_owned(),
            marker_type: MapMarkerType::Station,
            title: "Home".to_owned(),
            description: "KE8YGW".to_owned(),
            icon: "home".to_owned(),
            layer_id: "station".to_owned(),
            coordinate: Coordinate::new(41.0, -81.0).unwrap(),
            click_action: "switch-station:1".to_owned(),
            context_menu: vec!["Switch".to_owned()],
            metadata: json!({}),
        };
        serde_json::to_string(&marker).unwrap();
        let mut stack = MapLayerStack::default_layers();
        stack.set_enabled("weather", true).unwrap();
        stack.set_order("weather", 5).unwrap();
        assert_eq!(stack.layers.first().unwrap().layer_id, "weather");
        let provider = PlaceholderMapProvider::offline().metadata();
        assert!(provider.supports_offline);
    }

    #[test]
    fn qso_projection_math_builds_marker_path_distance_and_bearing() {
        let mut projection = QsoCurrentStateProjection::new();
        let qso_id = Uuid::new_v4();
        projection.upsert_record(QsoRecord {
            qso_id,
            payload: json!({
                "qso_id": qso_id,
                "contacted_callsign": "G1ABC",
                "grid": "IO91",
                "band": "20m",
                "mode": "SSB",
                "entity": "England"
            }),
            note_history: Vec::new(),
            deleted: false,
            last_event_hash: "test".to_owned(),
        });
        let station = maidenhead_to_coordinate("EN91").unwrap();
        let objects = qso_map_objects(&projection, Some(station), None);
        assert_eq!(objects.len(), 1);
        assert!(objects[0].distance.as_ref().unwrap().kilometers > 5_000.0);
        assert!(objects[0].bearing.is_some());
        assert!(objects[0].path.is_some());
    }

    #[test]
    fn grayline_calculation_returns_terminator() {
        let snapshot = grayline_snapshot(Utc::now()).unwrap();
        assert!(!snapshot.terminator.points.is_empty());
        assert!(snapshot.sun_subpoint.latitude.abs() <= 24.0);
    }
}
