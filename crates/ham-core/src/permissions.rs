use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use ham_plugin_sdk::{PluginCapability, PluginManifest};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionRiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionMetadata {
    pub permission_id: PluginCapability,
    pub category: String,
    pub display_name: String,
    pub description: String,
    pub risk_level: PermissionRiskLevel,
    pub default_allowed_for_builtin_plugins: bool,
    pub requires_admin_approval: bool,
    pub user_visible_reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionGrantStatus {
    Granted,
    Denied,
    Pending,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionGrant {
    pub plugin_id: String,
    pub permission_id: PluginCapability,
    pub status: PermissionGrantStatus,
    pub granted_by_operator_id: Option<Uuid>,
    pub granted_at: Option<DateTime<Utc>>,
    pub reason: Option<String>,
    pub scope: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
}

impl PermissionGrant {
    pub fn granted(plugin_id: impl Into<String>, permission_id: PluginCapability) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            permission_id,
            status: PermissionGrantStatus::Granted,
            granted_by_operator_id: None,
            granted_at: Some(Utc::now()),
            reason: Some("MVP local admin approval".to_owned()),
            scope: Some("all logbooks".to_owned()),
            expires_at: None,
        }
    }

    pub fn pending(plugin_id: impl Into<String>, permission_id: PluginCapability) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            permission_id,
            status: PermissionGrantStatus::Pending,
            granted_by_operator_id: None,
            granted_at: None,
            reason: Some("Plugin requested permission".to_owned()),
            scope: None,
            expires_at: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermissionGrantSet {
    pub grants: Vec<PermissionGrant>,
}

impl PermissionGrantSet {
    pub fn grants_for_manifest(manifest: &PluginManifest) -> Self {
        Self {
            grants: manifest
                .requested_or_capabilities()
                .into_iter()
                .map(|permission| PermissionGrant::granted(manifest.plugin_id.clone(), permission))
                .collect(),
        }
    }

    pub fn status(
        &self,
        plugin_id: &str,
        permission_id: &PluginCapability,
    ) -> Option<PermissionGrantStatus> {
        self.grants
            .iter()
            .find(|grant| grant.plugin_id == plugin_id && &grant.permission_id == permission_id)
            .map(|grant| grant.status)
    }

    pub fn is_granted(&self, plugin_id: &str, permission_id: &PluginCapability) -> bool {
        self.status(plugin_id, permission_id) == Some(PermissionGrantStatus::Granted)
    }

    pub fn upsert(&mut self, grant: PermissionGrant) {
        if let Some(existing) = self.grants.iter_mut().find(|existing| {
            existing.plugin_id == grant.plugin_id && existing.permission_id == grant.permission_id
        }) {
            *existing = grant;
        } else {
            self.grants.push(grant);
        }
    }

    pub fn set_status(
        &mut self,
        plugin_id: &str,
        permission_id: PluginCapability,
        status: PermissionGrantStatus,
        reason: Option<String>,
    ) -> PermissionGrant {
        let mut grant = PermissionGrant {
            plugin_id: plugin_id.to_owned(),
            permission_id,
            status,
            granted_by_operator_id: None,
            granted_at: (status == PermissionGrantStatus::Granted).then(Utc::now),
            reason,
            scope: Some("all logbooks".to_owned()),
            expires_at: None,
        };
        if status != PermissionGrantStatus::Granted {
            grant.granted_at = None;
        }
        self.upsert(grant.clone());
        grant
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionSettings {
    pub auto_grant_builtin_low_risk_permissions: bool,
    pub require_confirmation_for_high_risk_permissions: bool,
    pub allow_external_network_plugins: bool,
    pub permission_audit_logging: bool,
}

impl Default for PermissionSettings {
    fn default() -> Self {
        Self {
            auto_grant_builtin_low_risk_permissions: true,
            require_confirmation_for_high_risk_permissions: true,
            allow_external_network_plugins: false,
            permission_audit_logging: true,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct PermissionRegistry {
    permissions: HashMap<PluginCapability, PermissionMetadata>,
}

impl PermissionRegistry {
    pub fn mvp_default() -> Self {
        let mut registry = Self::default();
        for (permission, category, display, risk, default_builtin, admin, reason) in [
            (
                PluginCapability::QsoView,
                "Logbook",
                "View QSOs",
                PermissionRiskLevel::Low,
                true,
                false,
                "Read visible QSO projections.",
            ),
            (
                PluginCapability::QsoCreate,
                "Logbook",
                "Create QSOs",
                PermissionRiskLevel::Medium,
                true,
                false,
                "Submit QSO create proposals.",
            ),
            (
                PluginCapability::QsoCorrect,
                "Logbook",
                "Correct QSOs",
                PermissionRiskLevel::Medium,
                true,
                false,
                "Submit QSO correction proposals.",
            ),
            (
                PluginCapability::QsoDelete,
                "Logbook",
                "Delete QSOs",
                PermissionRiskLevel::High,
                false,
                true,
                "Create tombstone delete events.",
            ),
            (
                PluginCapability::QsoRestore,
                "Logbook",
                "Restore QSOs",
                PermissionRiskLevel::High,
                false,
                true,
                "Restore tombstoned QSOs.",
            ),
            (
                PluginCapability::QsoNoteAdd,
                "Logbook",
                "Add QSO Notes",
                PermissionRiskLevel::Medium,
                true,
                false,
                "Append QSO note history.",
            ),
            (
                PluginCapability::QsoViewDeleted,
                "Logbook",
                "View Deleted QSOs",
                PermissionRiskLevel::High,
                false,
                true,
                "View tombstoned QSO projections.",
            ),
            (
                PluginCapability::ActivationView,
                "Activation",
                "View Activations",
                PermissionRiskLevel::Low,
                true,
                false,
                "Read activation projections.",
            ),
            (
                PluginCapability::ActivationCreate,
                "Activation",
                "Create Activations",
                PermissionRiskLevel::Medium,
                true,
                false,
                "Create/start portable activations.",
            ),
            (
                PluginCapability::ActivationUpdate,
                "Activation",
                "Update Activations",
                PermissionRiskLevel::Medium,
                true,
                false,
                "Update activation details.",
            ),
            (
                PluginCapability::ActivationEnd,
                "Activation",
                "End Activations",
                PermissionRiskLevel::Medium,
                true,
                false,
                "End active activations.",
            ),
            (
                PluginCapability::ActivationCancel,
                "Activation",
                "Cancel Activations",
                PermissionRiskLevel::High,
                false,
                true,
                "Cancel activation records.",
            ),
            (
                PluginCapability::AdifImport,
                "ADIF",
                "Import ADIF",
                PermissionRiskLevel::High,
                false,
                true,
                "Import external QSO records.",
            ),
            (
                PluginCapability::AdifExport,
                "ADIF",
                "Export ADIF",
                PermissionRiskLevel::High,
                false,
                true,
                "Export visible QSO records.",
            ),
            (
                PluginCapability::SyncLanDiscovery,
                "Sync",
                "LAN Discovery",
                PermissionRiskLevel::Low,
                true,
                false,
                "Advertise/discover local peers.",
            ),
            (
                PluginCapability::SyncLanPull,
                "Sync",
                "LAN Pull",
                PermissionRiskLevel::High,
                false,
                true,
                "Pull official events from LAN peers.",
            ),
            (
                PluginCapability::SyncLanPush,
                "Sync",
                "LAN Push",
                PermissionRiskLevel::High,
                false,
                true,
                "Push official events to LAN peers.",
            ),
            (
                PluginCapability::SyncCloudConnect,
                "Sync",
                "Cloud Connect",
                PermissionRiskLevel::High,
                false,
                true,
                "Authenticate with cloud sync.",
            ),
            (
                PluginCapability::SyncCloudPull,
                "Sync",
                "Cloud Pull",
                PermissionRiskLevel::High,
                false,
                true,
                "Pull official events from cloud.",
            ),
            (
                PluginCapability::SyncCloudPush,
                "Sync",
                "Cloud Push",
                PermissionRiskLevel::High,
                false,
                true,
                "Push official events to cloud.",
            ),
            (
                PluginCapability::LookupCallsign,
                "Lookup",
                "Lookup Callsigns",
                PermissionRiskLevel::Low,
                true,
                false,
                "Lookup advisory callsign data.",
            ),
            (
                PluginCapability::LookupEntity,
                "Lookup",
                "Infer Entity",
                PermissionRiskLevel::Low,
                true,
                false,
                "Infer entity from prefixes.",
            ),
            (
                PluginCapability::LookupGrid,
                "Lookup",
                "Use Grid Tools",
                PermissionRiskLevel::Low,
                true,
                false,
                "Validate grids and estimate position.",
            ),
            (
                PluginCapability::LookupCacheRead,
                "Lookup",
                "Read Lookup Cache",
                PermissionRiskLevel::Low,
                true,
                false,
                "Read local lookup cache.",
            ),
            (
                PluginCapability::LookupCacheWrite,
                "Lookup",
                "Write Lookup Cache",
                PermissionRiskLevel::Medium,
                true,
                false,
                "Write local lookup cache.",
            ),
            (
                PluginCapability::NetworkExternalLookup,
                "Network",
                "External Lookup Network",
                PermissionRiskLevel::High,
                false,
                true,
                "Contact external lookup services.",
            ),
            (
                PluginCapability::RigView,
                "Rig",
                "View Rig",
                PermissionRiskLevel::Low,
                true,
                false,
                "Show configured rig devices.",
            ),
            (
                PluginCapability::RigReadState,
                "Rig",
                "Read Rig State",
                PermissionRiskLevel::Low,
                true,
                false,
                "Read frequency/mode state.",
            ),
            (
                PluginCapability::RigControlFrequency,
                "Rig",
                "Set Frequency",
                PermissionRiskLevel::Critical,
                false,
                true,
                "Change rig frequency.",
            ),
            (
                PluginCapability::RigControlMode,
                "Rig",
                "Set Mode",
                PermissionRiskLevel::High,
                false,
                true,
                "Change rig mode.",
            ),
            (
                PluginCapability::RigControlPtt,
                "Rig",
                "Control PTT",
                PermissionRiskLevel::Critical,
                false,
                true,
                "Key transmitter PTT.",
            ),
            (
                PluginCapability::RigControlSplit,
                "Rig",
                "Control Split",
                PermissionRiskLevel::High,
                false,
                true,
                "Change split operation.",
            ),
            (
                PluginCapability::RigConfigure,
                "Rig",
                "Configure Rig",
                PermissionRiskLevel::High,
                false,
                true,
                "Change rig connection settings.",
            ),
            (
                PluginCapability::DiagnosticsViewLogs,
                "Diagnostics",
                "View Logs",
                PermissionRiskLevel::Medium,
                true,
                false,
                "View diagnostic runtime logs.",
            ),
            (
                PluginCapability::DiagnosticsExport,
                "Diagnostics",
                "Export Diagnostics",
                PermissionRiskLevel::High,
                false,
                true,
                "Export diagnostic bundles.",
            ),
            (
                PluginCapability::DiagnosticsUpload,
                "Diagnostics",
                "Upload Diagnostics",
                PermissionRiskLevel::High,
                false,
                true,
                "Upload diagnostic bundles.",
            ),
            (
                PluginCapability::UiPanelRegister,
                "UI",
                "Register Panels",
                PermissionRiskLevel::Low,
                true,
                false,
                "Contribute GUI panels.",
            ),
            (
                PluginCapability::UiCommandRegister,
                "UI",
                "Register Commands",
                PermissionRiskLevel::Low,
                true,
                false,
                "Contribute command palette actions.",
            ),
            (
                PluginCapability::SettingsRead,
                "Settings",
                "Read Settings",
                PermissionRiskLevel::Medium,
                true,
                false,
                "Read local settings.",
            ),
            (
                PluginCapability::SettingsWrite,
                "Settings",
                "Write Settings",
                PermissionRiskLevel::High,
                false,
                true,
                "Modify local settings.",
            ),
        ] {
            registry.insert(
                permission,
                category,
                display,
                risk,
                default_builtin,
                admin,
                reason,
            );
        }
        registry
    }

    pub fn get(&self, permission_id: &PluginCapability) -> Option<&PermissionMetadata> {
        self.permissions.get(permission_id)
    }

    pub fn all(&self) -> Vec<PermissionMetadata> {
        let mut permissions = self.permissions.values().cloned().collect::<Vec<_>>();
        permissions.sort_by(|a, b| a.permission_id.as_str().cmp(b.permission_id.as_str()));
        permissions
    }

    pub fn validate_manifest(&self, manifest: &PluginManifest) -> Result<(), PermissionError> {
        for permission in manifest
            .requested_or_capabilities()
            .into_iter()
            .chain(manifest.optional_permissions.clone())
        {
            if matches!(permission, PluginCapability::Other(_)) || self.get(&permission).is_none() {
                return Err(PermissionError::UnknownPermission(
                    permission.as_str().to_owned(),
                ));
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn insert(
        &mut self,
        permission_id: PluginCapability,
        category: &str,
        display_name: &str,
        risk_level: PermissionRiskLevel,
        default_allowed_for_builtin_plugins: bool,
        requires_admin_approval: bool,
        user_visible_reason: &str,
    ) {
        self.permissions.insert(
            permission_id.clone(),
            PermissionMetadata {
                permission_id,
                category: category.to_owned(),
                display_name: display_name.to_owned(),
                description: user_visible_reason.to_owned(),
                risk_level,
                default_allowed_for_builtin_plugins,
                requires_admin_approval,
                user_visible_reason: user_visible_reason.to_owned(),
            },
        );
    }
}

#[derive(Debug, Error)]
pub enum PermissionError {
    #[error("unknown permission {0}")]
    UnknownPermission(String),
    #[error("permission {permission} is not granted for plugin {plugin_id}")]
    NotGranted {
        plugin_id: String,
        permission: String,
    },
    #[error("permission storage error: {0}")]
    Io(#[from] io::Error),
    #[error("permission serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

pub fn check_plugin_permission(
    manifest: &PluginManifest,
    grants: &PermissionGrantSet,
    permission: &PluginCapability,
) -> Result<(), PermissionError> {
    if !manifest.has_capability(permission) {
        return Err(PermissionError::NotGranted {
            plugin_id: manifest.plugin_id.clone(),
            permission: permission.as_str().to_owned(),
        });
    }
    if grants.is_granted(&manifest.plugin_id, permission) {
        Ok(())
    } else {
        Err(PermissionError::NotGranted {
            plugin_id: manifest.plugin_id.clone(),
            permission: permission.as_str().to_owned(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct JsonPermissionGrantStore {
    path: PathBuf,
}

impl JsonPermissionGrantStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> Result<PermissionGrantSet, PermissionError> {
        if !self.path.exists() {
            return Ok(PermissionGrantSet::default());
        }
        let text = fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&text)?)
    }

    pub fn save(&self, grants: &PermissionGrantSet) -> Result<(), PermissionError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&self.path, serde_json::to_vec_pretty(grants)?)?;
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub fn grant_builtin_defaults(
    manifests: &[PluginManifest],
    registry: &PermissionRegistry,
    settings: &PermissionSettings,
    grants: &mut PermissionGrantSet,
) {
    for manifest in manifests {
        for permission in manifest.requested_or_capabilities() {
            if grants.status(&manifest.plugin_id, &permission).is_some() {
                continue;
            }
            let Some(metadata) = registry.get(&permission) else {
                continue;
            };
            let is_builtin =
                manifest.plugin_type == "builtin" || manifest.plugin_id.starts_with("core.");
            if settings.auto_grant_builtin_low_risk_permissions
                && is_builtin
                && metadata.default_allowed_for_builtin_plugins
                && matches!(
                    metadata.risk_level,
                    PermissionRiskLevel::Low | PermissionRiskLevel::Medium
                )
            {
                grants.upsert(PermissionGrant::granted(
                    manifest.plugin_id.clone(),
                    permission,
                ));
            } else {
                grants.upsert(PermissionGrant::pending(
                    manifest.plugin_id.clone(),
                    permission,
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ham_plugin_sdk::PluginManifest;

    #[test]
    fn permission_registry_contains_required_permissions() {
        let registry = PermissionRegistry::mvp_default();
        assert!(registry.get(&PluginCapability::QsoCreate).is_some());
        assert!(registry.get(&PluginCapability::DiagnosticsUpload).is_some());
        assert!(registry.get(&PluginCapability::RigControlPtt).is_some());
    }

    #[test]
    fn manifest_validation_rejects_unknown_permissions() {
        let registry = PermissionRegistry::mvp_default();
        let manifest = PluginManifest::new(
            "plugin.bad",
            "Bad",
            "0.1.0",
            vec![PluginCapability::Other("unknown.permission".to_owned())],
        );
        assert!(registry.validate_manifest(&manifest).is_err());
    }

    #[test]
    fn grant_deny_revoke_lifecycle() {
        let mut grants = PermissionGrantSet::default();
        grants.set_status(
            "plugin.test",
            PluginCapability::QsoCreate,
            PermissionGrantStatus::Granted,
            None,
        );
        assert!(grants.is_granted("plugin.test", &PluginCapability::QsoCreate));
        grants.set_status(
            "plugin.test",
            PluginCapability::QsoCreate,
            PermissionGrantStatus::Denied,
            Some("no".to_owned()),
        );
        assert_eq!(
            grants.status("plugin.test", &PluginCapability::QsoCreate),
            Some(PermissionGrantStatus::Denied)
        );
        grants.set_status(
            "plugin.test",
            PluginCapability::QsoCreate,
            PermissionGrantStatus::Revoked,
            None,
        );
        assert_eq!(
            grants.status("plugin.test", &PluginCapability::QsoCreate),
            Some(PermissionGrantStatus::Revoked)
        );
    }

    #[test]
    fn permission_grants_persist_reload() {
        let path = std::env::temp_dir().join(format!("ham-permissions-{}.json", Uuid::new_v4()));
        let store = JsonPermissionGrantStore::new(&path);
        let mut grants = PermissionGrantSet::default();
        grants.set_status(
            "plugin.test",
            PluginCapability::DiagnosticsUpload,
            PermissionGrantStatus::Granted,
            None,
        );
        store.save(&grants).unwrap();
        let loaded = store.load().unwrap();
        assert!(loaded.is_granted("plugin.test", &PluginCapability::DiagnosticsUpload));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn adif_export_permission_is_enforced_by_grants() {
        let manifest = PluginManifest::new(
            "plugin.exporter",
            "Exporter",
            "0.1.0",
            vec![PluginCapability::AdifExport],
        );
        let denied = PermissionGrantSet::default();
        assert!(
            check_plugin_permission(&manifest, &denied, &PluginCapability::AdifExport).is_err()
        );
        let granted = PermissionGrantSet::grants_for_manifest(&manifest);
        assert!(
            check_plugin_permission(&manifest, &granted, &PluginCapability::AdifExport).is_ok()
        );
    }

    #[test]
    fn diagnostics_upload_permission_is_separate() {
        let manifest = PluginManifest::new(
            "core.gui",
            "Core GUI",
            "0.1.0",
            vec![PluginCapability::DiagnosticsExport],
        );
        let grants = PermissionGrantSet::grants_for_manifest(&manifest);
        assert!(
            check_plugin_permission(&manifest, &grants, &PluginCapability::DiagnosticsUpload)
                .is_err()
        );
    }

    #[test]
    fn rig_read_and_write_permissions_are_separate() {
        let manifest = PluginManifest::new(
            "plugin.rig",
            "Rig",
            "0.1.0",
            vec![PluginCapability::RigReadState],
        );
        let grants = PermissionGrantSet::grants_for_manifest(&manifest);
        assert!(
            check_plugin_permission(&manifest, &grants, &PluginCapability::RigReadState).is_ok()
        );
        assert!(check_plugin_permission(
            &manifest,
            &grants,
            &PluginCapability::RigControlFrequency
        )
        .is_err());
    }

    #[test]
    fn external_lookup_requires_network_permission() {
        let manifest = PluginManifest::new(
            "plugin.lookup",
            "Lookup",
            "0.1.0",
            vec![PluginCapability::LookupCallsign],
        );
        let grants = PermissionGrantSet::grants_for_manifest(&manifest);
        assert!(
            check_plugin_permission(&manifest, &grants, &PluginCapability::LookupCallsign).is_ok()
        );
        assert!(check_plugin_permission(
            &manifest,
            &grants,
            &PluginCapability::NetworkExternalLookup
        )
        .is_err());
    }
}
