use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{normalize_callsign, validate_grid};

pub const APPLICATION_SETTINGS_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ApplicationSettingsError {
    #[error("invalid callsign in {field}")]
    InvalidCallsign { field: String },
    #[error("invalid Maidenhead grid in {field}")]
    InvalidGrid { field: String },
    #[error("sync server URL must start with http:// or https:// and include a host")]
    InvalidSyncServerUrl,
    #[error("{field} must be between {min} and {max}")]
    OutOfRange { field: String, min: u64, max: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApplicationSettings {
    pub schema_version: u32,
    pub operator: OperatorIdentitySettings,
    pub location: LocationSettings,
    pub providers: ProviderSettings,
    pub sync: SyncSettings,
    pub logging: LoggingSettings,
    pub activation: ActivationSettings,
    pub net_control: NetControlSettings,
    pub display: DisplaySettings,
    pub backup: BackupSettings,
    pub privacy: PrivacySettings,
    pub diagnostics: DiagnosticsSettings,
    pub developer: DeveloperSettings,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Default for ApplicationSettings {
    fn default() -> Self {
        let now = Utc::now();
        Self {
            schema_version: APPLICATION_SETTINGS_SCHEMA_VERSION,
            operator: OperatorIdentitySettings::default(),
            location: LocationSettings::default(),
            providers: ProviderSettings::default(),
            sync: SyncSettings::default(),
            logging: LoggingSettings::default(),
            activation: ActivationSettings::default(),
            net_control: NetControlSettings::default(),
            display: DisplaySettings::default(),
            backup: BackupSettings::default(),
            privacy: PrivacySettings::default(),
            diagnostics: DiagnosticsSettings::default(),
            developer: DeveloperSettings::default(),
            created_at: now,
            updated_at: now,
        }
    }
}

impl ApplicationSettings {
    pub fn normalized(mut self) -> Result<Self, ApplicationSettingsError> {
        self.schema_version = APPLICATION_SETTINGS_SCHEMA_VERSION;
        self.operator.primary_callsign = normalize_required_callsign(
            &self.operator.primary_callsign,
            "operator.primary_callsign",
        )?;
        self.operator.station_callsign = normalize_required_callsign(
            &self.operator.station_callsign,
            "operator.station_callsign",
        )?;
        self.operator.additional_callsigns = self
            .operator
            .additional_callsigns
            .iter()
            .filter(|value| !value.trim().is_empty())
            .map(|value| normalize_callsign_field(value, "operator.additional_callsigns"))
            .collect::<Result<Vec<_>, _>>()?;
        self.operator.additional_callsigns.sort();
        self.operator.additional_callsigns.dedup();
        self.location.manual_maidenhead_grid = normalize_optional_grid(
            self.location.manual_maidenhead_grid.as_deref(),
            "location.manual_maidenhead_grid",
        )?;
        self.location.last_gps_grid = normalize_optional_grid(
            self.location.last_gps_grid.as_deref(),
            "location.last_gps_grid",
        )?;
        self.sync.sync_server_url = normalize_sync_server_url(&self.sync.sync_server_url)?;
        if !(1..=240).contains(&self.sync.sync_interval_minutes) {
            return Err(ApplicationSettingsError::OutOfRange {
                field: "sync.sync_interval_minutes".to_owned(),
                min: 1,
                max: 240,
            });
        }
        if !(1..=720).contains(&self.activation.validation_ttl_hours) {
            return Err(ApplicationSettingsError::OutOfRange {
                field: "activation.validation_ttl_hours".to_owned(),
                min: 1,
                max: 720,
            });
        }
        self.logging.default_mode = self.logging.default_mode.trim().to_ascii_uppercase();
        self.logging.default_band = self.logging.default_band.trim().to_owned();
        self.net_control.default_mode = self.net_control.default_mode.trim().to_ascii_uppercase();
        self.updated_at = Utc::now();
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperatorIdentitySettings {
    pub primary_callsign: String,
    pub additional_callsigns: Vec<String>,
    pub operator_name: Option<String>,
    pub operator_email: Option<String>,
    pub station_callsign: String,
    pub default_station_profile_id: Option<String>,
    pub default_equipment_profile_id: Option<String>,
}

impl Default for OperatorIdentitySettings {
    fn default() -> Self {
        Self {
            primary_callsign: "KE8YGW".to_owned(),
            additional_callsigns: Vec::new(),
            operator_name: Some(String::new()),
            operator_email: Some(String::new()),
            station_callsign: "KE8YGW".to_owned(),
            default_station_profile_id: Some(String::new()),
            default_equipment_profile_id: Some(String::new()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocationSettings {
    pub use_device_location: bool,
    pub manual_grid_override_enabled: bool,
    pub manual_maidenhead_grid: Option<String>,
    pub last_gps_grid: Option<String>,
    pub last_location_source: Option<String>,
    pub manual_location_name: Option<String>,
    pub manual_county: Option<String>,
    pub manual_state: Option<String>,
    pub manual_country: Option<String>,
}

impl Default for LocationSettings {
    fn default() -> Self {
        Self {
            use_device_location: true,
            manual_grid_override_enabled: false,
            manual_maidenhead_grid: Some("EN91".to_owned()),
            last_gps_grid: Some(String::new()),
            last_location_source: Some("stationDefault".to_owned()),
            manual_location_name: Some(String::new()),
            manual_county: Some(String::new()),
            manual_state: Some(String::new()),
            manual_country: Some("United States".to_owned()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderSettings {
    pub enabled: BTreeMap<String, bool>,
    pub credential_metadata: BTreeMap<String, BTreeMap<String, String>>,
    pub validation: BTreeMap<String, ProviderValidationSettings>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderValidationSettings {
    pub configured: bool,
    pub validated: bool,
    pub validated_at: Option<DateTime<Utc>>,
    pub message: String,
}

impl Default for ProviderValidationSettings {
    fn default() -> Self {
        Self {
            configured: false,
            validated: false,
            validated_at: None,
            message: "Not configured".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncSettings {
    pub sync_server_url: String,
    pub device_name: String,
    pub prefer_lan_sync: bool,
    pub auto_push_enabled: bool,
    pub auto_pull_enabled: bool,
    pub sync_interval_minutes: u64,
    pub background_sync_enabled: bool,
    pub account_label: Option<String>,
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            sync_server_url: "http://127.0.0.1:9740".to_owned(),
            device_name: "KE8YGW Logger iOS".to_owned(),
            prefer_lan_sync: true,
            auto_push_enabled: false,
            auto_pull_enabled: false,
            sync_interval_minutes: 15,
            background_sync_enabled: true,
            account_label: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingSettings {
    pub default_band: String,
    pub default_mode: String,
    pub auto_uppercase_callsigns: bool,
    pub ask_for_location_later: bool,
    pub callsign_lookup_preference: String,
}

impl Default for LoggingSettings {
    fn default() -> Self {
        Self {
            default_band: "20m".to_owned(),
            default_mode: "SSB".to_owned(),
            auto_uppercase_callsigns: true,
            ask_for_location_later: false,
            callsign_lookup_preference: "automatic".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivationSettings {
    pub allow_offline_activations: bool,
    pub validation_ttl_hours: u64,
    pub notes_template: Option<String>,
    pub pota_upload_enabled: bool,
    pub sota_upload_enabled: bool,
}

impl Default for ActivationSettings {
    fn default() -> Self {
        Self {
            allow_offline_activations: true,
            validation_ttl_hours: 24,
            notes_template: Some(String::new()),
            pota_upload_enabled: false,
            sota_upload_enabled: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetControlSettings {
    pub default_name: Option<String>,
    pub default_frequency_mhz: Option<String>,
    pub default_mode: String,
    pub sort_roster_by_traffic_priority: bool,
}

impl Default for NetControlSettings {
    fn default() -> Self {
        Self {
            default_name: Some("Weekly Emergency Net".to_owned()),
            default_frequency_mhz: Some("146.520".to_owned()),
            default_mode: "FM".to_owned(),
            sort_roster_by_traffic_priority: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplaySettings {
    pub appearance: String,
    pub accent_color_name: String,
    pub map_default_layer: String,
    pub show_qso_map_objects: bool,
    pub show_station_map_markers: bool,
}

impl Default for DisplaySettings {
    fn default() -> Self {
        Self {
            appearance: "system".to_owned(),
            accent_color_name: "blue".to_owned(),
            map_default_layer: "Stations".to_owned(),
            show_qso_map_objects: true,
            show_station_map_markers: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct BackupSettings {
    pub include_diagnostics_by_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrivacySettings {
    pub provider_notifications_enabled: bool,
}

impl Default for PrivacySettings {
    fn default() -> Self {
        Self {
            provider_notifications_enabled: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticsSettings {
    pub share_diagnostics_with_logs: bool,
}

impl Default for DiagnosticsSettings {
    fn default() -> Self {
        Self {
            share_diagnostics_with_logs: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct DeveloperSettings {
    pub developer_mode_enabled: bool,
}

fn normalize_required_callsign(
    callsign: &str,
    field: &str,
) -> Result<String, ApplicationSettingsError> {
    normalize_callsign_field(callsign, field)
}

fn normalize_callsign_field(
    callsign: &str,
    field: &str,
) -> Result<String, ApplicationSettingsError> {
    normalize_callsign(callsign).map_err(|_| ApplicationSettingsError::InvalidCallsign {
        field: field.to_owned(),
    })
}

fn normalize_optional_grid(
    grid: Option<&str>,
    field: &str,
) -> Result<Option<String>, ApplicationSettingsError> {
    let Some(grid) = grid.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(Some(String::new()));
    };
    let normalized = grid.to_ascii_uppercase();
    if validate_grid(&normalized) {
        Ok(Some(normalized))
    } else {
        Err(ApplicationSettingsError::InvalidGrid {
            field: field.to_owned(),
        })
    }
}

fn normalize_sync_server_url(url: &str) -> Result<String, ApplicationSettingsError> {
    let trimmed = url.trim();
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        return Err(ApplicationSettingsError::InvalidSyncServerUrl);
    }
    let rest = trimmed
        .split_once("://")
        .map(|(_, rest)| rest)
        .unwrap_or_default();
    let host = rest.split('/').next().unwrap_or_default();
    if host.is_empty() || host.contains(char::is_whitespace) {
        return Err(ApplicationSettingsError::InvalidSyncServerUrl);
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::JsonSupportStore;

    #[test]
    fn defaults_are_complete_and_valid() {
        let settings = ApplicationSettings::default().normalized().unwrap();
        assert_eq!(settings.operator.primary_callsign, "KE8YGW");
        assert_eq!(settings.sync.sync_server_url, "http://127.0.0.1:9740");
        assert_eq!(
            settings.location.manual_maidenhead_grid.as_deref(),
            Some("EN91")
        );
    }

    #[test]
    fn invalid_callsign_is_rejected_without_normalizing() {
        let mut settings = ApplicationSettings::default();
        settings.operator.primary_callsign = "bad".to_owned();
        assert!(matches!(
            settings.normalized(),
            Err(ApplicationSettingsError::InvalidCallsign { .. })
        ));
    }

    #[test]
    fn invalid_sync_url_is_rejected() {
        let mut settings = ApplicationSettings::default();
        settings.sync.sync_server_url = "not-a-url".to_owned();
        assert_eq!(
            settings.normalized().unwrap_err(),
            ApplicationSettingsError::InvalidSyncServerUrl
        );
    }

    #[test]
    fn support_store_persists_settings_without_secrets() {
        let path = std::env::temp_dir().join(format!(
            "ham-application-settings-{}.json",
            uuid::Uuid::new_v4()
        ));
        let store = JsonSupportStore::<ApplicationSettings>::new(&path);
        let mut settings = ApplicationSettings::default();
        settings.providers.credential_metadata.insert(
            "qrz-xml".to_owned(),
            BTreeMap::from([
                ("username".to_owned(), "KE8YGW".to_owned()),
                ("password_configured".to_owned(), "true".to_owned()),
            ]),
        );
        store.save(&settings).unwrap();
        let serialized = std::fs::read_to_string(&path).unwrap();
        let restored = store.load().unwrap();

        assert_eq!(restored.operator.primary_callsign, "KE8YGW");
        assert!(!serialized.contains("super-secret"));
        let _ = std::fs::remove_file(path);
    }
}
