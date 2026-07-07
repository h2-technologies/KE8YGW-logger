//! Secure credential metadata and storage abstraction.
//!
//! Credential metadata is support/config data. Secret values are intentionally
//! isolated behind [`CredentialStore`] and are never written to official events.

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use ham_plugin_sdk::{PluginCapability, ServiceType};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    check_plugin_permission, redact_payload, OperatorRole, PermissionGrantSet,
    ProposalValidationError,
};
use ham_plugin_sdk::PluginManifest;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialStatus {
    Active,
    Revoked,
    Missing,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialMetadata {
    pub credential_id: Uuid,
    pub provider_id: String,
    pub account_id: String,
    pub operator_id: Option<Uuid>,
    pub service_type: ServiceType,
    pub label: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub status: CredentialStatus,
    pub metadata: Value,
}

impl CredentialMetadata {
    pub fn new(
        provider_id: impl Into<String>,
        account_id: impl Into<String>,
        service_type: ServiceType,
        label: impl Into<String>,
    ) -> Self {
        let now = Utc::now();
        Self {
            credential_id: Uuid::new_v4(),
            provider_id: provider_id.into(),
            account_id: account_id.into(),
            operator_id: None,
            service_type,
            label: label.into(),
            created_at: now,
            updated_at: now,
            last_used_at: None,
            status: CredentialStatus::Active,
            metadata: Value::Object(Default::default()),
        }
    }

    pub fn sanitized(mut self) -> Self {
        self.metadata = redact_payload(self.metadata);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CredentialRecord {
    pub metadata: CredentialMetadata,
    pub secret: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CredentialBackendStatus {
    pub backend_name: String,
    pub available: bool,
    pub secure: bool,
    pub dev_only: bool,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("credential backend unavailable: {0}")]
    BackendUnavailable(String),
    #[error("credential {0} not found")]
    NotFound(Uuid),
    #[error("credential permission denied: {0}")]
    PermissionDenied(String),
    #[error("development credential store requires explicit insecure opt-in")]
    InsecureDevStoreNotAllowed,
    #[error("credential I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("credential serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub trait CredentialStore: Send + Sync {
    fn backend_status(&self) -> CredentialBackendStatus;
    fn store_credential(
        &mut self,
        metadata: CredentialMetadata,
        secret: &str,
    ) -> Result<CredentialMetadata, CredentialError>;
    fn retrieve_secret(&mut self, credential_id: Uuid) -> Result<String, CredentialError>;
    fn update_credential(
        &mut self,
        credential_id: Uuid,
        secret: &str,
        metadata: Option<Value>,
    ) -> Result<CredentialMetadata, CredentialError>;
    fn revoke_credential(
        &mut self,
        credential_id: Uuid,
    ) -> Result<CredentialMetadata, CredentialError>;
    fn test_credential(&self, credential_id: Uuid) -> Result<bool, CredentialError>;
    fn list_metadata(&self) -> Vec<CredentialMetadata>;
    fn rotate_credential_placeholder(
        &mut self,
        credential_id: Uuid,
    ) -> Result<CredentialMetadata, CredentialError>;
}

#[derive(Debug, Clone, Default)]
pub struct UnsupportedOsCredentialStore;

impl CredentialStore for UnsupportedOsCredentialStore {
    fn backend_status(&self) -> CredentialBackendStatus {
        CredentialBackendStatus {
            backend_name: os_backend_name().to_owned(),
            available: false,
            secure: true,
            dev_only: false,
            message: "OS keychain integration is modeled but not linked in this MVP build"
                .to_owned(),
        }
    }

    fn store_credential(
        &mut self,
        _metadata: CredentialMetadata,
        _secret: &str,
    ) -> Result<CredentialMetadata, CredentialError> {
        Err(CredentialError::BackendUnavailable(
            self.backend_status().message,
        ))
    }

    fn retrieve_secret(&mut self, credential_id: Uuid) -> Result<String, CredentialError> {
        Err(CredentialError::NotFound(credential_id))
    }

    fn update_credential(
        &mut self,
        credential_id: Uuid,
        _secret: &str,
        _metadata: Option<Value>,
    ) -> Result<CredentialMetadata, CredentialError> {
        Err(CredentialError::NotFound(credential_id))
    }

    fn revoke_credential(
        &mut self,
        credential_id: Uuid,
    ) -> Result<CredentialMetadata, CredentialError> {
        Err(CredentialError::NotFound(credential_id))
    }

    fn test_credential(&self, _credential_id: Uuid) -> Result<bool, CredentialError> {
        Ok(false)
    }

    fn list_metadata(&self) -> Vec<CredentialMetadata> {
        Vec::new()
    }

    fn rotate_credential_placeholder(
        &mut self,
        credential_id: Uuid,
    ) -> Result<CredentialMetadata, CredentialError> {
        Err(CredentialError::NotFound(credential_id))
    }
}

#[derive(Debug, Clone)]
pub struct InsecureDevCredentialStore {
    path: PathBuf,
    records: HashMap<Uuid, CredentialRecord>,
}

impl InsecureDevCredentialStore {
    pub fn open(path: impl Into<PathBuf>, allow_insecure: bool) -> Result<Self, CredentialError> {
        if !allow_insecure {
            return Err(CredentialError::InsecureDevStoreNotAllowed);
        }
        let path = path.into();
        let records = if path.exists() {
            let text = fs::read_to_string(&path)?;
            serde_json::from_str::<Vec<CredentialRecord>>(&text)?
                .into_iter()
                .map(|record| (record.metadata.credential_id, record))
                .collect()
        } else {
            HashMap::new()
        };
        Ok(Self { path, records })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn persist(&self) -> Result<(), CredentialError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut records = self.records.values().cloned().collect::<Vec<_>>();
        records.sort_by(|a, b| a.metadata.label.cmp(&b.metadata.label));
        fs::write(&self.path, serde_json::to_vec_pretty(&records)?)?;
        Ok(())
    }
}

impl CredentialStore for InsecureDevCredentialStore {
    fn backend_status(&self) -> CredentialBackendStatus {
        CredentialBackendStatus {
            backend_name: "insecure-dev-file".to_owned(),
            available: true,
            secure: false,
            dev_only: true,
            message: "Development-only plaintext credential fallback is explicitly enabled"
                .to_owned(),
        }
    }

    fn store_credential(
        &mut self,
        metadata: CredentialMetadata,
        secret: &str,
    ) -> Result<CredentialMetadata, CredentialError> {
        let mut metadata = metadata.sanitized();
        metadata.updated_at = Utc::now();
        self.records.insert(
            metadata.credential_id,
            CredentialRecord {
                metadata: metadata.clone(),
                secret: secret.to_owned(),
            },
        );
        self.persist()?;
        Ok(metadata)
    }

    fn retrieve_secret(&mut self, credential_id: Uuid) -> Result<String, CredentialError> {
        let record = self
            .records
            .get_mut(&credential_id)
            .ok_or(CredentialError::NotFound(credential_id))?;
        record.metadata.last_used_at = Some(Utc::now());
        let secret = record.secret.clone();
        self.persist()?;
        Ok(secret)
    }

    fn update_credential(
        &mut self,
        credential_id: Uuid,
        secret: &str,
        metadata: Option<Value>,
    ) -> Result<CredentialMetadata, CredentialError> {
        let record = self
            .records
            .get_mut(&credential_id)
            .ok_or(CredentialError::NotFound(credential_id))?;
        record.secret = secret.to_owned();
        if let Some(metadata) = metadata {
            record.metadata.metadata = redact_payload(metadata);
        }
        record.metadata.status = CredentialStatus::Active;
        record.metadata.updated_at = Utc::now();
        let metadata = record.metadata.clone();
        self.persist()?;
        Ok(metadata)
    }

    fn revoke_credential(
        &mut self,
        credential_id: Uuid,
    ) -> Result<CredentialMetadata, CredentialError> {
        let record = self
            .records
            .get_mut(&credential_id)
            .ok_or(CredentialError::NotFound(credential_id))?;
        record.metadata.status = CredentialStatus::Revoked;
        record.metadata.updated_at = Utc::now();
        let metadata = record.metadata.clone();
        self.persist()?;
        Ok(metadata)
    }

    fn test_credential(&self, credential_id: Uuid) -> Result<bool, CredentialError> {
        Ok(self
            .records
            .get(&credential_id)
            .is_some_and(|record| record.metadata.status == CredentialStatus::Active))
    }

    fn list_metadata(&self) -> Vec<CredentialMetadata> {
        let mut metadata = self
            .records
            .values()
            .map(|record| record.metadata.clone().sanitized())
            .collect::<Vec<_>>();
        metadata.sort_by(|a, b| a.label.cmp(&b.label));
        metadata
    }

    fn rotate_credential_placeholder(
        &mut self,
        credential_id: Uuid,
    ) -> Result<CredentialMetadata, CredentialError> {
        let record = self
            .records
            .get_mut(&credential_id)
            .ok_or(CredentialError::NotFound(credential_id))?;
        record.metadata.updated_at = Utc::now();
        record.metadata.metadata["rotation_status"] = Value::String("rotation_planned".to_owned());
        let metadata = record.metadata.clone();
        self.persist()?;
        Ok(metadata)
    }
}

pub fn required_credentials_satisfied(
    required_credential_keys: &[String],
    configured_credential_ids: &HashMap<String, Uuid>,
    store: &dyn CredentialStore,
) -> bool {
    required_credential_keys.iter().all(|key| {
        configured_credential_ids
            .get(key)
            .is_some_and(|credential_id| store.test_credential(*credential_id).unwrap_or(false))
    })
}

pub fn authorize_credential_action(
    manifest: &PluginManifest,
    grants: &PermissionGrantSet,
    operator_role: OperatorRole,
    permission: PluginCapability,
) -> Result<(), CredentialError> {
    check_plugin_permission(manifest, grants, &permission)
        .map_err(|error| CredentialError::PermissionDenied(error.to_string()))?;
    if operator_role != OperatorRole::Admin {
        return Err(CredentialError::PermissionDenied(format!(
            "operator role {operator_role:?} cannot use {}",
            permission.as_str()
        )));
    }
    Ok(())
}

pub fn credential_runtime_payload(metadata: &CredentialMetadata) -> Value {
    serde_json::json!({
        "credential_id": metadata.credential_id,
        "provider_id": metadata.provider_id,
        "service_type": metadata.service_type.as_str(),
        "label": metadata.label,
        "status": metadata.status,
    })
}

pub fn os_backend_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows-credential-manager"
    } else if cfg!(target_os = "macos") {
        "macos-keychain"
    } else if cfg!(target_os = "linux") {
        "linux-secret-service"
    } else {
        "unsupported-os-keychain"
    }
}

impl From<CredentialError> for ProposalValidationError {
    fn from(error: CredentialError) -> Self {
        ProposalValidationError::InvalidSchema(error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ham_plugin_sdk::{PluginCapability, PluginManifest};

    #[test]
    fn metadata_list_does_not_expose_secret() {
        let path = std::env::temp_dir().join(format!("ham-credentials-{}.json", Uuid::new_v4()));
        let mut store = InsecureDevCredentialStore::open(&path, true).unwrap();
        let metadata =
            CredentialMetadata::new("qrz-stub", "acct", ServiceType::CallsignLookup, "QRZ token");
        store.store_credential(metadata, "super-secret").unwrap();
        let listed = serde_json::to_value(store.list_metadata()).unwrap();
        assert!(!listed.to_string().contains("super-secret"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn redaction_removes_secret_metadata() {
        let mut metadata =
            CredentialMetadata::new("lotw-stub", "acct", ServiceType::LogUpload, "LoTW cert");
        metadata.metadata = serde_json::json!({"api_key": "abc", "safe": "ok"});
        let sanitized = metadata.sanitized();
        assert_eq!(sanitized.metadata["api_key"], "[REDACTED]");
        assert_eq!(sanitized.metadata["safe"], "ok");
    }

    #[test]
    fn dev_fallback_requires_explicit_opt_in() {
        let path = std::env::temp_dir().join(format!("ham-credentials-{}.json", Uuid::new_v4()));
        assert!(matches!(
            InsecureDevCredentialStore::open(&path, false),
            Err(CredentialError::InsecureDevStoreNotAllowed)
        ));
    }

    #[test]
    fn credential_permission_denial_works() {
        let manifest = PluginManifest::new(
            "plugin.lookup",
            "Lookup",
            "0.1.0",
            vec![PluginCapability::CredentialViewMetadata],
        );
        let grants = PermissionGrantSet::default();
        let error = authorize_credential_action(
            &manifest,
            &grants,
            OperatorRole::Admin,
            PluginCapability::CredentialViewMetadata,
        )
        .unwrap_err();
        assert!(matches!(error, CredentialError::PermissionDenied(_)));
    }
}
