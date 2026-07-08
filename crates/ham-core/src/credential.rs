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
    #[error("credential backend command failed: {0}")]
    BackendCommand(String),
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
    fn delete_credential(&mut self, credential_id: Uuid) -> Result<(), CredentialError> {
        self.revoke_credential(credential_id).map(|_| ())
    }
    fn credential_exists(&self, credential_id: Uuid) -> bool {
        self.test_credential(credential_id).unwrap_or(false)
    }
    fn test_credential(&self, credential_id: Uuid) -> Result<bool, CredentialError>;
    fn list_metadata(&self) -> Vec<CredentialMetadata>;
    fn rotate_credential_placeholder(
        &mut self,
        credential_id: Uuid,
    ) -> Result<CredentialMetadata, CredentialError>;
}

#[derive(Debug, Clone)]
pub struct OsCredentialStore {
    metadata_path: PathBuf,
    records: HashMap<Uuid, CredentialMetadata>,
    service_prefix: String,
}

impl OsCredentialStore {
    pub fn open(metadata_path: impl Into<PathBuf>) -> Result<Self, CredentialError> {
        Self::open_with_service_prefix(metadata_path, "ke8ygw-logger")
    }

    pub fn open_with_service_prefix(
        metadata_path: impl Into<PathBuf>,
        service_prefix: impl Into<String>,
    ) -> Result<Self, CredentialError> {
        let metadata_path = metadata_path.into();
        let records = if metadata_path.exists() {
            let text = fs::read_to_string(&metadata_path)?;
            serde_json::from_str::<Vec<CredentialMetadata>>(&text)?
                .into_iter()
                .map(|metadata| (metadata.credential_id, metadata.sanitized()))
                .collect()
        } else {
            HashMap::new()
        };
        Ok(Self {
            metadata_path,
            records,
            service_prefix: service_prefix.into(),
        })
    }

    pub fn metadata_path(&self) -> &Path {
        &self.metadata_path
    }

    fn persist_metadata(&self) -> Result<(), CredentialError> {
        if let Some(parent) = self.metadata_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut records = self
            .records
            .values()
            .cloned()
            .map(CredentialMetadata::sanitized)
            .collect::<Vec<_>>();
        records.sort_by(|a, b| a.label.cmp(&b.label));
        fs::write(&self.metadata_path, serde_json::to_vec_pretty(&records)?)?;
        Ok(())
    }

    fn target_name(&self, credential_id: Uuid) -> String {
        format!("{}:{}", self.service_prefix, credential_id)
    }

    fn write_secret(
        &self,
        credential_id: Uuid,
        account: &str,
        secret: &str,
    ) -> Result<(), CredentialError> {
        platform_write_secret(&self.target_name(credential_id), account, secret)
    }

    fn read_secret(&self, credential_id: Uuid) -> Result<String, CredentialError> {
        platform_read_secret(&self.target_name(credential_id))
    }

    fn delete_secret(&self, credential_id: Uuid) -> Result<(), CredentialError> {
        platform_delete_secret(&self.target_name(credential_id))
    }
}

impl CredentialStore for OsCredentialStore {
    fn backend_status(&self) -> CredentialBackendStatus {
        CredentialBackendStatus {
            backend_name: os_backend_name().to_owned(),
            available: platform_backend_available(),
            secure: true,
            dev_only: false,
            message: platform_backend_message(),
        }
    }

    fn store_credential(
        &mut self,
        metadata: CredentialMetadata,
        secret: &str,
    ) -> Result<CredentialMetadata, CredentialError> {
        if !platform_backend_available() {
            return Err(CredentialError::BackendUnavailable(
                self.backend_status().message,
            ));
        }
        let mut metadata = metadata.sanitized();
        metadata.status = CredentialStatus::Active;
        metadata.updated_at = Utc::now();
        self.write_secret(metadata.credential_id, &metadata.account_id, secret)?;
        self.records
            .insert(metadata.credential_id, metadata.clone());
        self.persist_metadata()?;
        Ok(metadata)
    }

    fn retrieve_secret(&mut self, credential_id: Uuid) -> Result<String, CredentialError> {
        let mut metadata = self
            .records
            .get(&credential_id)
            .cloned()
            .ok_or(CredentialError::NotFound(credential_id))?;
        if metadata.status != CredentialStatus::Active {
            return Err(CredentialError::NotFound(credential_id));
        }
        let secret = self.read_secret(credential_id)?;
        metadata.last_used_at = Some(Utc::now());
        self.records.insert(credential_id, metadata);
        self.persist_metadata()?;
        Ok(secret)
    }

    fn update_credential(
        &mut self,
        credential_id: Uuid,
        secret: &str,
        metadata: Option<Value>,
    ) -> Result<CredentialMetadata, CredentialError> {
        let mut record = self
            .records
            .get(&credential_id)
            .cloned()
            .ok_or(CredentialError::NotFound(credential_id))?;
        self.write_secret(credential_id, &record.account_id, secret)?;
        if let Some(metadata) = metadata {
            record.metadata = redact_payload(metadata);
        }
        record.status = CredentialStatus::Active;
        record.updated_at = Utc::now();
        self.records.insert(credential_id, record.clone());
        self.persist_metadata()?;
        Ok(record)
    }

    fn revoke_credential(
        &mut self,
        credential_id: Uuid,
    ) -> Result<CredentialMetadata, CredentialError> {
        let mut record = self
            .records
            .get(&credential_id)
            .cloned()
            .ok_or(CredentialError::NotFound(credential_id))?;
        let _ = self.delete_secret(credential_id);
        record.status = CredentialStatus::Revoked;
        record.updated_at = Utc::now();
        self.records.insert(credential_id, record.clone());
        self.persist_metadata()?;
        Ok(record)
    }

    fn delete_credential(&mut self, credential_id: Uuid) -> Result<(), CredentialError> {
        if !self.records.contains_key(&credential_id) {
            return Err(CredentialError::NotFound(credential_id));
        }
        let _ = self.delete_secret(credential_id);
        self.records.remove(&credential_id);
        self.persist_metadata()?;
        Ok(())
    }

    fn test_credential(&self, credential_id: Uuid) -> Result<bool, CredentialError> {
        let Some(metadata) = self.records.get(&credential_id) else {
            return Ok(false);
        };
        if metadata.status != CredentialStatus::Active {
            return Ok(false);
        }
        Ok(self.read_secret(credential_id).is_ok())
    }

    fn list_metadata(&self) -> Vec<CredentialMetadata> {
        let mut metadata = self
            .records
            .values()
            .cloned()
            .map(CredentialMetadata::sanitized)
            .collect::<Vec<_>>();
        metadata.sort_by(|a, b| a.label.cmp(&b.label));
        metadata
    }

    fn rotate_credential_placeholder(
        &mut self,
        credential_id: Uuid,
    ) -> Result<CredentialMetadata, CredentialError> {
        let mut record = self
            .records
            .get(&credential_id)
            .cloned()
            .ok_or(CredentialError::NotFound(credential_id))?;
        record.updated_at = Utc::now();
        record.metadata["rotation_status"] = Value::String("rotation_required".to_owned());
        self.records.insert(credential_id, record.clone());
        self.persist_metadata()?;
        Ok(record.sanitized())
    }
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

pub fn default_credential_store(
    support_dir: impl AsRef<Path>,
    allow_insecure_dev_fallback: bool,
) -> Box<dyn CredentialStore> {
    let support_dir = support_dir.as_ref();
    if platform_backend_available() {
        match OsCredentialStore::open(support_dir.join("credential-metadata.json")) {
            Ok(store) => return Box::new(store),
            Err(_) => return Box::new(UnsupportedOsCredentialStore),
        }
    }
    if allow_insecure_dev_fallback {
        match InsecureDevCredentialStore::open(support_dir.join("dev-credentials.json"), true) {
            Ok(store) => return Box::new(store),
            Err(_) => return Box::new(UnsupportedOsCredentialStore),
        }
    }
    Box::new(UnsupportedOsCredentialStore)
}

fn platform_backend_available() -> bool {
    #[cfg(target_os = "windows")]
    {
        true
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("security")
            .arg("-h")
            .output()
            .is_ok()
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("secret-tool")
            .arg("--version")
            .output()
            .is_ok()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

fn platform_backend_message() -> String {
    #[cfg(target_os = "windows")]
    {
        "Windows Credential Manager backend is available".to_owned()
    }
    #[cfg(target_os = "macos")]
    {
        if platform_backend_available() {
            "macOS Keychain backend is available through the security command".to_owned()
        } else {
            "macOS security command was not found".to_owned()
        }
    }
    #[cfg(target_os = "linux")]
    {
        if platform_backend_available() {
            "Linux Secret Service backend is available through secret-tool".to_owned()
        } else {
            "Linux secret-tool was not found; install libsecret tooling".to_owned()
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        "No supported OS credential backend for this platform".to_owned()
    }
}

#[cfg(target_os = "windows")]
fn platform_write_secret(target: &str, account: &str, secret: &str) -> Result<(), CredentialError> {
    use windows_sys::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    let mut target = wide(target);
    let mut account = wide(account);
    let mut secret = secret.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: target.as_mut_ptr(),
        UserName: account.as_mut_ptr(),
        CredentialBlobSize: secret.len() as u32,
        CredentialBlob: secret.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    let ok = unsafe { CredWriteW(&credential, 0) };
    if ok == 0 {
        return Err(CredentialError::BackendCommand(
            "CredWriteW failed for Windows Credential Manager".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn platform_read_secret(target: &str) -> Result<String, CredentialError> {
    use std::slice;
    use windows_sys::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let target = wide(target);
    let mut credential: *mut CREDENTIALW = std::ptr::null_mut();
    let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut credential) };
    if ok == 0 || credential.is_null() {
        return Err(CredentialError::BackendUnavailable(
            "credential was not found in Windows Credential Manager".to_owned(),
        ));
    }
    let bytes = unsafe {
        let credential_ref = &*credential;
        slice::from_raw_parts(
            credential_ref.CredentialBlob,
            credential_ref.CredentialBlobSize as usize,
        )
        .to_vec()
    };
    unsafe { CredFree(credential as _) };
    String::from_utf8(bytes).map_err(|error| CredentialError::BackendCommand(error.to_string()))
}

#[cfg(target_os = "windows")]
fn platform_delete_secret(target: &str) -> Result<(), CredentialError> {
    use windows_sys::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    let target = wide(target);
    let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
    if ok == 0 {
        return Err(CredentialError::BackendCommand(
            "CredDeleteW failed for Windows Credential Manager".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(target_os = "macos")]
fn platform_write_secret(target: &str, account: &str, secret: &str) -> Result<(), CredentialError> {
    let status = std::process::Command::new("security")
        .args([
            "add-generic-password",
            "-a",
            account,
            "-s",
            target,
            "-w",
            secret,
            "-U",
        ])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(CredentialError::BackendCommand(
            "security add-generic-password failed".to_owned(),
        ))
    }
}

#[cfg(target_os = "macos")]
fn platform_read_secret(target: &str) -> Result<String, CredentialError> {
    let output = std::process::Command::new("security")
        .args(["find-generic-password", "-s", target, "-w"])
        .output()?;
    if !output.status.success() {
        return Err(CredentialError::BackendUnavailable(
            "credential was not found in macOS Keychain".to_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_owned())
}

#[cfg(target_os = "macos")]
fn platform_delete_secret(target: &str) -> Result<(), CredentialError> {
    let status = std::process::Command::new("security")
        .args(["delete-generic-password", "-s", target])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(CredentialError::BackendCommand(
            "security delete-generic-password failed".to_owned(),
        ))
    }
}

#[cfg(target_os = "linux")]
fn platform_write_secret(target: &str, account: &str, secret: &str) -> Result<(), CredentialError> {
    let mut child = std::process::Command::new("secret-tool")
        .args([
            "store", "--label", target, "service", target, "account", account,
        ])
        .stdin(std::process::Stdio::piped())
        .spawn()?;
    if let Some(stdin) = child.stdin.as_mut() {
        use std::io::Write;
        stdin.write_all(secret.as_bytes())?;
    }
    let status = child.wait()?;
    if status.success() {
        Ok(())
    } else {
        Err(CredentialError::BackendCommand(
            "secret-tool store failed".to_owned(),
        ))
    }
}

#[cfg(target_os = "linux")]
fn platform_read_secret(target: &str) -> Result<String, CredentialError> {
    let output = std::process::Command::new("secret-tool")
        .args(["lookup", "service", target])
        .output()?;
    if !output.status.success() {
        return Err(CredentialError::BackendUnavailable(
            "credential was not found in Secret Service".to_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout)
        .trim_end()
        .to_owned())
}

#[cfg(target_os = "linux")]
fn platform_delete_secret(target: &str) -> Result<(), CredentialError> {
    let status = std::process::Command::new("secret-tool")
        .args(["clear", "service", target])
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(CredentialError::BackendCommand(
            "secret-tool clear failed".to_owned(),
        ))
    }
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn platform_write_secret(
    _target: &str,
    _account: &str,
    _secret: &str,
) -> Result<(), CredentialError> {
    Err(CredentialError::BackendUnavailable(
        platform_backend_message(),
    ))
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn platform_read_secret(_target: &str) -> Result<String, CredentialError> {
    Err(CredentialError::BackendUnavailable(
        platform_backend_message(),
    ))
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn platform_delete_secret(_target: &str) -> Result<(), CredentialError> {
    Err(CredentialError::BackendUnavailable(
        platform_backend_message(),
    ))
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
        store
            .store_credential(metadata, "TEST_SECRET_SHOULD_NOT_APPEAR")
            .unwrap();
        let listed = serde_json::to_value(store.list_metadata()).unwrap();
        assert!(!listed.to_string().contains("TEST_SECRET_SHOULD_NOT_APPEAR"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn os_metadata_sidecar_does_not_expose_secret_metadata() {
        let path = std::env::temp_dir().join(format!(
            "ham-os-credential-metadata-{}.json",
            Uuid::new_v4()
        ));
        let mut store = OsCredentialStore::open(&path).unwrap();
        let mut metadata =
            CredentialMetadata::new("clublog-stub", "acct", ServiceType::LogUpload, "Club Log");
        metadata.metadata = serde_json::json!({
            "client_secret": "TEST_SECRET_SHOULD_NOT_APPEAR",
            "username": "KE8YGW"
        });
        store.records.insert(metadata.credential_id, metadata);
        store.persist_metadata().unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(!text.contains("TEST_SECRET_SHOULD_NOT_APPEAR"));
        assert!(text.contains("[REDACTED]"));
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
