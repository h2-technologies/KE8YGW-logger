//! Durable support/config storage helpers.
//!
//! Support storage is for local MVP state such as provider settings, upload
//! queues, map preferences, and GUI preferences. It is not the official
//! append-only log and must never contain secrets.

use std::{
    fs,
    marker::PhantomData,
    path::{Path, PathBuf},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

pub const SUPPORT_FILE_VERSION: u32 = 1;

#[derive(Debug, Error)]
pub enum SupportStoreError {
    #[error("support storage I/O failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("support storage JSON failed: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("unsupported support storage version {found}; expected {expected}")]
    UnsupportedVersion { found: u32, expected: u32 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportEnvelope<T> {
    pub version: u32,
    pub data: T,
}

#[derive(Debug, Clone)]
pub struct JsonSupportStore<T> {
    path: PathBuf,
    _marker: PhantomData<T>,
}

impl<T> JsonSupportStore<T>
where
    T: Serialize + DeserializeOwned + Default,
{
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            _marker: PhantomData,
        }
    }

    pub fn load(&self) -> Result<T, SupportStoreError> {
        if !self.path.exists() {
            return Ok(T::default());
        }
        let envelope: SupportEnvelope<T> = serde_json::from_str(&fs::read_to_string(&self.path)?)?;
        if envelope.version != SUPPORT_FILE_VERSION {
            return Err(SupportStoreError::UnsupportedVersion {
                found: envelope.version,
                expected: SUPPORT_FILE_VERSION,
            });
        }
        Ok(envelope.data)
    }

    pub fn save(&self, data: &T) -> Result<(), SupportStoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let envelope = SupportEnvelope {
            version: SUPPORT_FILE_VERSION,
            data,
        };
        fs::write(&self.path, serde_json::to_vec_pretty(&envelope)?)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use chrono::Utc;
    use ham_plugin_sdk::ServiceType;

    use crate::{
        default_service_registry, MapLayerStack, NetTemplate, ServiceCacheEntry, ServiceRegistry,
        UploadQueue, UploadTarget,
    };

    use super::*;

    #[test]
    fn support_store_saves_and_reloads_upload_queue() {
        let path = std::env::temp_dir().join(format!("ham-upload-{}.json", uuid::Uuid::new_v4()));
        let store = JsonSupportStore::<UploadQueue>::new(&path);
        let mut queue = UploadQueue::new(vec![UploadTarget {
            target_id: "mock".to_owned(),
            display_name: "Mock".to_owned(),
            provider_id: "mock".to_owned(),
            enabled: true,
            required_config_keys: vec![],
        }]);
        let _ = queue
            .create_job("mock", uuid::Uuid::new_v4(), vec![uuid::Uuid::new_v4()])
            .unwrap();

        store.save(&queue).unwrap();
        let reloaded = store.load().unwrap();

        assert_eq!(reloaded.jobs.len(), 1);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn support_store_saves_and_reloads_map_preferences() {
        let path = std::env::temp_dir().join(format!("ham-map-{}.json", uuid::Uuid::new_v4()));
        let store = JsonSupportStore::<MapLayerStack>::new(&path);
        let mut layers = MapLayerStack::default_layers();
        layers.set_enabled("weather", true).unwrap();
        layers.set_order("weather", 5).unwrap();

        store.save(&layers).unwrap();
        let reloaded = store.load().unwrap();

        assert!(reloaded
            .layers
            .iter()
            .any(|layer| layer.layer_id == "weather" && layer.enabled && layer.order == 5));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn support_store_saves_and_reloads_provider_settings() {
        let path = std::env::temp_dir().join(format!("ham-services-{}.json", uuid::Uuid::new_v4()));
        let store = JsonSupportStore::<ServiceRegistry>::new(&path);
        let mut registry = default_service_registry();
        registry.set_enabled("qrz-xml", false).unwrap();
        registry.set_priority("local-prefix", 99).unwrap();
        registry
            .set_preferred_provider(ham_plugin_sdk::ServiceType::CallsignLookup, "local-prefix")
            .unwrap();

        store.save(&registry).unwrap();
        let reloaded = store.load().unwrap();
        let snapshot = reloaded.snapshot();

        assert!(
            !snapshot
                .providers
                .iter()
                .find(|provider| provider.metadata.provider_id == "qrz-xml")
                .unwrap()
                .enabled
        );
        assert_eq!(
            snapshot
                .preferred_providers
                .get(&ham_plugin_sdk::ServiceType::CallsignLookup),
            Some(&"local-prefix".to_owned())
        );
        let _ = fs::remove_file(path);
    }

    #[test]
    fn support_store_saves_and_reloads_service_cache_metadata() {
        let path = std::env::temp_dir().join(format!("ham-cache-{}.json", uuid::Uuid::new_v4()));
        let store = JsonSupportStore::<Vec<ServiceCacheEntry>>::new(&path);
        let entries = vec![ServiceCacheEntry {
            service_type: ServiceType::CallsignLookup,
            provider_id: "local-prefix".to_owned(),
            cache_key: "k1abc".to_owned(),
            fetched_at: Utc::now(),
            expires_at: None,
            confidence: Some(0.7),
            value: serde_json::json!({"callsign":"K1ABC"}),
            safe_metadata: Some(serde_json::json!({"source":"test"})),
        }];

        store.save(&entries).unwrap();
        let reloaded = store.load().unwrap();

        assert_eq!(reloaded.len(), 1);
        assert_eq!(reloaded[0].provider_id, "local-prefix");
        let _ = fs::remove_file(path);
    }

    #[test]
    fn support_store_saves_and_reloads_net_templates() {
        let path = std::env::temp_dir().join(format!("ham-net-{}.json", uuid::Uuid::new_v4()));
        let store = JsonSupportStore::<Vec<NetTemplate>>::new(&path);
        let now = Utc::now();
        let templates = vec![NetTemplate {
            net_template_id: uuid::Uuid::new_v4(),
            account_id: "local".to_owned(),
            name: "ARES Weekly Net".to_owned(),
            description: Some("Weekly directed net".to_owned()),
            default_frequency_hz: Some(146_940_000),
            default_band: Some("2m".to_owned()),
            default_mode: Some("FM".to_owned()),
            default_schedule: Some("Tuesday 20:00 local".to_owned()),
            default_script: None,
            default_roster: vec!["KE8YGW".to_owned()],
            tags: vec!["ares".to_owned()],
            created_at: now,
            updated_at: now,
        }];

        store.save(&templates).unwrap();
        let reloaded = store.load().unwrap();

        assert_eq!(reloaded[0].name, "ARES Weekly Net");
        assert_eq!(reloaded[0].default_band.as_deref(), Some("2m"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn support_store_reports_corrupted_file() {
        let path = std::env::temp_dir().join(format!("ham-corrupt-{}.json", uuid::Uuid::new_v4()));
        fs::write(&path, "{not valid json").unwrap();
        let store = JsonSupportStore::<UploadQueue>::new(&path);

        assert!(matches!(store.load(), Err(SupportStoreError::Serde(_))));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn support_store_rejects_unknown_version() {
        let path = std::env::temp_dir().join(format!("ham-version-{}.json", uuid::Uuid::new_v4()));
        fs::write(&path, r#"{"version":999,"data":{"targets":[],"jobs":[]}}"#).unwrap();
        let store = JsonSupportStore::<UploadQueue>::new(&path);

        assert!(matches!(
            store.load(),
            Err(SupportStoreError::UnsupportedVersion { found: 999, .. })
        ));
        let _ = fs::remove_file(path);
    }
}
