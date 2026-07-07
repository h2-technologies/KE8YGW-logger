use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StationStoreError {
    #[error("station storage I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("station storage JSON failed: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("station profile {0} was not found")]
    ProfileNotFound(Uuid),
    #[error("equipment item {0} was not found")]
    EquipmentNotFound(Uuid),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentType {
    Radio,
    Antenna,
    Amplifier,
    Tuner,
    Rotor,
    Interface,
    PowerSupply,
    Accessory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EquipmentStatus {
    Active,
    Inactive,
    Retired,
    Repair,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StationProfile {
    pub station_profile_id: Uuid,
    pub account_id: Option<String>,
    pub display_name: String,
    pub station_callsign: String,
    pub operator_callsign: Option<String>,
    pub default_grid: Option<String>,
    pub default_qth: Option<String>,
    pub default_power_watts: Option<u32>,
    pub notes: Option<String>,
    pub tags: Vec<String>,
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl StationProfile {
    pub fn new(display_name: impl Into<String>, station_callsign: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            station_profile_id: Uuid::new_v4(),
            account_id: None,
            display_name: display_name.into(),
            station_callsign: station_callsign.into().trim().to_ascii_uppercase(),
            operator_callsign: None,
            default_grid: None,
            default_qth: None,
            default_power_watts: None,
            notes: None,
            tags: Vec::new(),
            active: false,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EquipmentItem {
    pub equipment_id: Uuid,
    pub account_id: Option<String>,
    pub equipment_type: EquipmentType,
    pub display_name: String,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    pub capabilities: Vec<String>,
    pub notes: Option<String>,
    pub status: EquipmentStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EquipmentItem {
    pub fn new(equipment_type: EquipmentType, display_name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            equipment_id: Uuid::new_v4(),
            account_id: None,
            equipment_type,
            display_name: display_name.into(),
            manufacturer: None,
            model: None,
            serial_number: None,
            capabilities: Vec::new(),
            notes: None,
            status: EquipmentStatus::Active,
            created_at: now,
            updated_at: now,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StationConfiguration {
    pub configuration_id: Uuid,
    pub station_profile_id: Uuid,
    pub radio_id: Option<Uuid>,
    pub antenna_id: Option<Uuid>,
    pub amplifier_id: Option<Uuid>,
    pub tuner_id: Option<Uuid>,
    pub rotor_id: Option<Uuid>,
    pub interface_id: Option<Uuid>,
    pub power_supply_id: Option<Uuid>,
    pub name: String,
    pub band_hint: Option<String>,
    pub mode_hint: Option<String>,
    pub default_power_watts: Option<u32>,
    pub notes: Option<String>,
}

impl StationConfiguration {
    pub fn new(station_profile_id: Uuid, name: impl Into<String>) -> Self {
        Self {
            configuration_id: Uuid::new_v4(),
            station_profile_id,
            radio_id: None,
            antenna_id: None,
            amplifier_id: None,
            tuner_id: None,
            rotor_id: None,
            interface_id: None,
            power_supply_id: None,
            name: name.into(),
            band_hint: None,
            mode_hint: None,
            default_power_watts: None,
            notes: None,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct StationBook {
    pub profiles: Vec<StationProfile>,
    pub equipment: Vec<EquipmentItem>,
    pub configurations: Vec<StationConfiguration>,
    pub active_profile_id: Option<Uuid>,
    pub active_configuration_id: Option<Uuid>,
}

impl StationBook {
    pub fn create_profile(&mut self, mut profile: StationProfile) -> StationProfile {
        profile.updated_at = Utc::now();
        if self.profiles.is_empty() || profile.active {
            self.set_all_profiles_inactive();
            profile.active = true;
            self.active_profile_id = Some(profile.station_profile_id);
        }
        self.profiles.push(profile.clone());
        profile
    }

    pub fn update_profile(
        &mut self,
        profile_id: Uuid,
        mut update: StationProfile,
    ) -> Result<StationProfile, StationStoreError> {
        let Some(existing) = self
            .profiles
            .iter_mut()
            .find(|profile| profile.station_profile_id == profile_id)
        else {
            return Err(StationStoreError::ProfileNotFound(profile_id));
        };
        update.station_profile_id = profile_id;
        update.created_at = existing.created_at;
        update.updated_at = Utc::now();
        *existing = update.clone();
        if update.active {
            self.select_profile(profile_id)?;
        }
        Ok(update)
    }

    pub fn create_equipment(&mut self, mut item: EquipmentItem) -> EquipmentItem {
        item.updated_at = Utc::now();
        self.equipment.push(item.clone());
        item
    }

    pub fn update_equipment(
        &mut self,
        equipment_id: Uuid,
        mut update: EquipmentItem,
    ) -> Result<EquipmentItem, StationStoreError> {
        let Some(existing) = self
            .equipment
            .iter_mut()
            .find(|item| item.equipment_id == equipment_id)
        else {
            return Err(StationStoreError::EquipmentNotFound(equipment_id));
        };
        update.equipment_id = equipment_id;
        update.created_at = existing.created_at;
        update.updated_at = Utc::now();
        *existing = update.clone();
        Ok(update)
    }

    pub fn create_configuration(
        &mut self,
        config: StationConfiguration,
    ) -> Result<StationConfiguration, StationStoreError> {
        if !self
            .profiles
            .iter()
            .any(|profile| profile.station_profile_id == config.station_profile_id)
        {
            return Err(StationStoreError::ProfileNotFound(
                config.station_profile_id,
            ));
        }
        self.configurations.push(config.clone());
        Ok(config)
    }

    pub fn select_profile(&mut self, profile_id: Uuid) -> Result<(), StationStoreError> {
        if !self
            .profiles
            .iter()
            .any(|profile| profile.station_profile_id == profile_id)
        {
            return Err(StationStoreError::ProfileNotFound(profile_id));
        }
        self.set_all_profiles_inactive();
        if let Some(profile) = self
            .profiles
            .iter_mut()
            .find(|profile| profile.station_profile_id == profile_id)
        {
            profile.active = true;
            profile.updated_at = Utc::now();
        }
        self.active_profile_id = Some(profile_id);
        self.active_configuration_id = self
            .configurations
            .iter()
            .find(|config| config.station_profile_id == profile_id)
            .map(|config| config.configuration_id);
        Ok(())
    }

    pub fn select_configuration(
        &mut self,
        configuration_id: Uuid,
    ) -> Result<(), StationStoreError> {
        let Some(config) = self
            .configurations
            .iter()
            .find(|config| config.configuration_id == configuration_id)
        else {
            return Err(StationStoreError::ProfileNotFound(configuration_id));
        };
        self.select_profile(config.station_profile_id)?;
        self.active_configuration_id = Some(configuration_id);
        Ok(())
    }

    pub fn active_profile(&self) -> Option<&StationProfile> {
        self.active_profile_id.and_then(|id| {
            self.profiles
                .iter()
                .find(|profile| profile.station_profile_id == id)
        })
    }

    pub fn active_configuration(&self) -> Option<&StationConfiguration> {
        self.active_configuration_id.and_then(|id| {
            self.configurations
                .iter()
                .find(|config| config.configuration_id == id)
        })
    }

    pub fn apply_defaults_to_qso_payload(&self, payload: &mut Value) {
        let Some(profile) = self.active_profile() else {
            return;
        };
        ensure_object(payload);
        insert_if_missing(
            payload,
            "station_profile_id",
            json!(profile.station_profile_id),
        );
        insert_if_empty(payload, "station_callsign", json!(profile.station_callsign));
        if let Some(operator) = &profile.operator_callsign {
            insert_if_empty(payload, "operator_callsign", json!(operator));
        }
        if let Some(grid) = &profile.default_grid {
            insert_if_empty(payload, "grid", json!(grid));
        }
        if let Some(qth) = &profile.default_qth {
            insert_if_empty(payload, "qth", json!(qth));
        }
        if let Some(power) = profile.default_power_watts {
            insert_if_missing(payload, "power_watts", json!(power));
        }
        if let Some(config) = self.active_configuration() {
            insert_if_missing(
                payload,
                "station_configuration_id",
                json!(config.configuration_id),
            );
            insert_if_missing(payload, "radio_id", json!(config.radio_id));
            insert_if_missing(payload, "antenna_id", json!(config.antenna_id));
            insert_if_missing(payload, "amplifier_id", json!(config.amplifier_id));
            if let Some(power) = config.default_power_watts {
                insert_if_missing(payload, "power_watts", json!(power));
            }
        }
    }

    fn set_all_profiles_inactive(&mut self) {
        for profile in &mut self.profiles {
            profile.active = false;
        }
    }
}

#[derive(Debug, Clone)]
pub struct JsonStationBookStore {
    path: PathBuf,
}

impl JsonStationBookStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> Result<StationBook, StationStoreError> {
        if !self.path.exists() {
            return Ok(StationBook::default());
        }
        let input = fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&input)?)
    }

    pub fn save(&self, book: &StationBook) -> Result<(), StationStoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_json::to_vec_pretty(book)?)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

fn ensure_object(payload: &mut Value) {
    if !payload.is_object() {
        *payload = json!({});
    }
}

fn insert_if_missing(payload: &mut Value, key: &str, value: Value) {
    if payload.get(key).is_none() || payload.get(key) == Some(&Value::Null) {
        payload[key] = value;
    }
}

fn insert_if_empty(payload: &mut Value, key: &str, value: Value) {
    if payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .is_empty()
    {
        payload[key] = value;
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn profile_defaults_are_applied_without_overwriting_manual_fields() {
        let mut book = StationBook::default();
        let mut profile = StationProfile::new("Home", "KE8YGW");
        profile.operator_callsign = Some("N0CALL".to_owned());
        profile.default_grid = Some("EN91".to_owned());
        profile.active = true;
        let profile = book.create_profile(profile);
        let mut config = StationConfiguration::new(profile.station_profile_id, "HF");
        config.radio_id = Some(Uuid::new_v4());
        config.default_power_watts = Some(100);
        book.create_configuration(config.clone()).unwrap();
        book.select_configuration(config.configuration_id).unwrap();

        let mut payload = json!({"station_callsign": "W1AW", "mode": "SSB"});
        book.apply_defaults_to_qso_payload(&mut payload);

        assert_eq!(payload["station_callsign"], "W1AW");
        assert_eq!(payload["operator_callsign"], "N0CALL");
        assert_eq!(payload["grid"], "EN91");
        assert_eq!(
            payload["station_profile_id"],
            json!(profile.station_profile_id)
        );
        assert_eq!(
            payload["station_configuration_id"],
            json!(config.configuration_id)
        );
        assert_eq!(payload["power_watts"], 100);
    }

    #[test]
    fn station_book_persists_and_reloads_active_profile() {
        let path = std::env::temp_dir().join(format!("station-{}.json", Uuid::new_v4()));
        let store = JsonStationBookStore::new(&path);
        let mut book = StationBook::default();
        let profile = book.create_profile(StationProfile::new("Portable", "K1ABC"));
        book.select_profile(profile.station_profile_id).unwrap();
        store.save(&book).unwrap();

        let loaded = store.load().unwrap();
        assert_eq!(
            loaded
                .active_profile()
                .map(|profile| &profile.station_callsign),
            Some(&"K1ABC".to_owned())
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn historical_payload_keeps_snapshot_after_equipment_changes() {
        let mut book = StationBook::default();
        let profile = book.create_profile(StationProfile::new("Home", "KE8YGW"));
        let mut radio = EquipmentItem::new(EquipmentType::Radio, "Old Rig");
        radio.manufacturer = Some("Icom".to_owned());
        let radio = book.create_equipment(radio);
        let mut config = StationConfiguration::new(profile.station_profile_id, "HF");
        config.radio_id = Some(radio.equipment_id);
        book.create_configuration(config.clone()).unwrap();
        book.select_configuration(config.configuration_id).unwrap();

        let mut payload = json!({"contacted_callsign": "K1ABC"});
        book.apply_defaults_to_qso_payload(&mut payload);
        let historical_radio_id = payload["radio_id"].clone();

        let mut changed_radio = radio;
        changed_radio.display_name = "New Rig Name".to_owned();
        book.update_equipment(changed_radio.equipment_id, changed_radio)
            .unwrap();

        assert_eq!(payload["radio_id"], historical_radio_id);
    }
}
