//! Public SDK types shared by plugins and the core proposal validator.
//!
//! This crate intentionally does not load or execute plugins. It defines the
//! stable data model plugins use to declare capabilities and submit proposals.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use uuid::Uuid;

pub const PROPOSAL_QSO_CREATE: &str = "proposal.qso.create";
pub const PROPOSAL_QSO_CORRECT: &str = "proposal.qso.correct";
pub const PROPOSAL_QSO_DELETE: &str = "proposal.qso.delete";
pub const PROPOSAL_QSO_RESTORE: &str = "proposal.qso.restore";
pub const PROPOSAL_QSO_NOTE_ADD: &str = "proposal.qso.note.add";
pub const PROPOSAL_ACTIVATION_CREATE: &str = "proposal.activation.create";
pub const PROPOSAL_ACTIVATION_UPDATE: &str = "proposal.activation.update";
pub const PROPOSAL_ACTIVATION_START: &str = "proposal.activation.start";
pub const PROPOSAL_ACTIVATION_END: &str = "proposal.activation.end";
pub const PROPOSAL_ACTIVATION_CANCEL: &str = "proposal.activation.cancel";
pub const PROPOSAL_ACTIVATION_NOTE_ADD: &str = "proposal.activation.note.add";
pub const PROPOSAL_QSO_ACTIVATION_LINK: &str = "proposal.qso.activation.link";
pub const PROPOSAL_QSO_ACTIVATION_UNLINK: &str = "proposal.qso.activation.unlink";

pub const OFFICIAL_LOG_QSO_CREATED: &str = "official.log.qso.created";
pub const OFFICIAL_LOG_QSO_CORRECTED: &str = "official.log.qso.corrected";
pub const OFFICIAL_LOG_QSO_DELETED: &str = "official.log.qso.deleted";
pub const OFFICIAL_LOG_QSO_RESTORED: &str = "official.log.qso.restored";
pub const OFFICIAL_LOG_QSO_NOTE_ADDED: &str = "official.log.qso.note_added";
pub const OFFICIAL_LOG_ACTIVATION_CREATED: &str = "official.log.activation.created";
pub const OFFICIAL_LOG_ACTIVATION_UPDATED: &str = "official.log.activation.updated";
pub const OFFICIAL_LOG_ACTIVATION_STARTED: &str = "official.log.activation.started";
pub const OFFICIAL_LOG_ACTIVATION_ENDED: &str = "official.log.activation.ended";
pub const OFFICIAL_LOG_ACTIVATION_CANCELLED: &str = "official.log.activation.cancelled";
pub const OFFICIAL_LOG_ACTIVATION_NOTE_ADDED: &str = "official.log.activation.note_added";
pub const OFFICIAL_LOG_QSO_ACTIVATION_LINKED: &str = "official.log.qso.activation_linked";
pub const OFFICIAL_LOG_QSO_ACTIVATION_UNLINKED: &str = "official.log.qso.activation_unlinked";

/// A capability granted to a plugin by the host application.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PluginCapability {
    QsoView,
    QsoCreate,
    QsoCorrect,
    QsoDelete,
    QsoRestore,
    QsoNoteAdd,
    QsoViewDeleted,
    ActivationCreate,
    ActivationUpdate,
    ActivationEnd,
    ActivationCancel,
    ActivationView,
    AdifImport,
    AdifExport,
    SyncLanDiscovery,
    SyncLanPull,
    SyncLanPush,
    SyncCloudConnect,
    SyncCloudPull,
    SyncCloudPush,
    LookupCallsign,
    LookupEntity,
    LookupGrid,
    LookupCacheRead,
    LookupCacheWrite,
    NetworkExternalLookup,
    QsoSuggestFields,
    RigView,
    RigControlFrequency,
    RigControlMode,
    RigControlPtt,
    RigControlSplit,
    RigReadState,
    RigConfigure,
    DiagnosticsViewLogs,
    DiagnosticsExport,
    DiagnosticsUpload,
    UiPanelRegister,
    UiCommandRegister,
    SettingsRead,
    SettingsWrite,
    Other(String),
}

impl PluginCapability {
    pub fn as_str(&self) -> &str {
        match self {
            Self::QsoView => "log.qso.view",
            Self::QsoCreate => "log.qso.create",
            Self::QsoCorrect => "log.qso.correct",
            Self::QsoDelete => "log.qso.delete",
            Self::QsoRestore => "log.qso.restore",
            Self::QsoNoteAdd => "log.qso.note.add",
            Self::QsoViewDeleted => "log.qso.view_deleted",
            Self::ActivationCreate => "activation.create",
            Self::ActivationUpdate => "activation.update",
            Self::ActivationEnd => "activation.end",
            Self::ActivationCancel => "activation.cancel",
            Self::ActivationView => "activation.view",
            Self::AdifImport => "adif.import",
            Self::AdifExport => "adif.export",
            Self::SyncLanDiscovery => "sync.lan.discovery",
            Self::SyncLanPull => "sync.lan.pull",
            Self::SyncLanPush => "sync.lan.push",
            Self::SyncCloudConnect => "sync.cloud.connect",
            Self::SyncCloudPull => "sync.cloud.pull",
            Self::SyncCloudPush => "sync.cloud.push",
            Self::LookupCallsign => "lookup.callsign",
            Self::LookupEntity => "lookup.entity",
            Self::LookupGrid => "lookup.grid",
            Self::LookupCacheRead => "cache.lookup.read",
            Self::LookupCacheWrite => "cache.lookup.write",
            Self::NetworkExternalLookup => "network.external.lookup",
            Self::QsoSuggestFields => "log.qso.suggest_fields",
            Self::RigView => "rig.view",
            Self::RigControlFrequency => "rig.control.frequency",
            Self::RigControlMode => "rig.control.mode",
            Self::RigControlPtt => "rig.control.ptt",
            Self::RigControlSplit => "rig.control.split",
            Self::RigReadState => "rig.read.state",
            Self::RigConfigure => "rig.configure",
            Self::DiagnosticsViewLogs => "diagnostics.view_logs",
            Self::DiagnosticsExport => "diagnostics.export",
            Self::DiagnosticsUpload => "diagnostics.upload",
            Self::UiPanelRegister => "ui.panel.register",
            Self::UiCommandRegister => "ui.command.register",
            Self::SettingsRead => "settings.read",
            Self::SettingsWrite => "settings.write",
            Self::Other(value) => value,
        }
    }
}

impl Serialize for PluginCapability {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for PluginCapability {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "log.qso.view" => Self::QsoView,
            "qso:create" | "log.qso.create" => Self::QsoCreate,
            "qso:correct" | "log.qso.correct" => Self::QsoCorrect,
            "qso:delete" | "log.qso.delete" => Self::QsoDelete,
            "qso:restore" | "log.qso.restore" => Self::QsoRestore,
            "qso:note:add" | "log.qso.note.add" => Self::QsoNoteAdd,
            "qso:view-deleted" | "log.qso.view_deleted" => Self::QsoViewDeleted,
            "activation.create" => Self::ActivationCreate,
            "activation.update" => Self::ActivationUpdate,
            "activation.end" => Self::ActivationEnd,
            "activation.cancel" => Self::ActivationCancel,
            "activation.view" => Self::ActivationView,
            "adif.import" => Self::AdifImport,
            "adif.export" => Self::AdifExport,
            "sync.lan.discovery" => Self::SyncLanDiscovery,
            "sync.lan.pull" => Self::SyncLanPull,
            "sync.lan.push" => Self::SyncLanPush,
            "sync.cloud.connect" => Self::SyncCloudConnect,
            "sync.cloud.pull" => Self::SyncCloudPull,
            "sync.cloud.push" => Self::SyncCloudPush,
            "lookup.callsign" => Self::LookupCallsign,
            "lookup.entity" => Self::LookupEntity,
            "lookup.grid" => Self::LookupGrid,
            "cache.lookup.read" => Self::LookupCacheRead,
            "cache.lookup.write" => Self::LookupCacheWrite,
            "network.external.lookup" => Self::NetworkExternalLookup,
            "log.qso.suggest_fields" => Self::QsoSuggestFields,
            "rig.view" => Self::RigView,
            "rig.control.frequency" => Self::RigControlFrequency,
            "rig.control.mode" => Self::RigControlMode,
            "rig.control.ptt" => Self::RigControlPtt,
            "rig.control.split" => Self::RigControlSplit,
            "rig.read.state" => Self::RigReadState,
            "rig.configure" => Self::RigConfigure,
            "diagnostics.view_logs" => Self::DiagnosticsViewLogs,
            "diagnostics.export" => Self::DiagnosticsExport,
            "diagnostics.upload" => Self::DiagnosticsUpload,
            "ui.panel.register" => Self::UiPanelRegister,
            "ui.command.register" => Self::UiCommandRegister,
            "settings.read" => Self::SettingsRead,
            "settings.write" => Self::SettingsWrite,
            _ => Self::Other(value),
        })
    }
}

/// Static plugin metadata supplied by a plugin package.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub requested_permissions: Vec<PluginCapability>,
    #[serde(default)]
    pub optional_permissions: Vec<PluginCapability>,
    #[serde(default)]
    pub contributed_panels: Vec<String>,
    #[serde(default)]
    pub contributed_commands: Vec<String>,
    #[serde(default)]
    pub plugin_type: String,
    #[serde(default)]
    pub minimum_core_version: String,
    pub capabilities: Vec<PluginCapability>,
}

impl PluginManifest {
    pub fn new(
        plugin_id: impl Into<String>,
        name: impl Into<String>,
        version: impl Into<String>,
        capabilities: Vec<PluginCapability>,
    ) -> Self {
        let capabilities = normalize_permissions(capabilities);
        Self {
            plugin_id: plugin_id.into(),
            name: name.into(),
            version: version.into(),
            author: String::new(),
            description: String::new(),
            requested_permissions: capabilities.clone(),
            optional_permissions: Vec::new(),
            contributed_panels: Vec::new(),
            contributed_commands: Vec::new(),
            plugin_type: "builtin".to_owned(),
            minimum_core_version: "0.1.0".to_owned(),
            capabilities,
        }
    }

    pub fn has_capability(&self, capability: &PluginCapability) -> bool {
        self.capabilities.iter().any(|held| held == capability)
            || self
                .requested_permissions
                .iter()
                .any(|held| held == capability)
    }

    pub fn requested_or_capabilities(&self) -> Vec<PluginCapability> {
        if self.requested_permissions.is_empty() {
            self.capabilities.clone()
        } else {
            self.requested_permissions.clone()
        }
    }
}

fn normalize_permissions(capabilities: Vec<PluginCapability>) -> Vec<PluginCapability> {
    let mut normalized = Vec::new();
    for capability in capabilities {
        if !normalized.contains(&capability) {
            normalized.push(capability);
        }
    }
    normalized
}

/// A proposed operation submitted by a plugin.
///
/// Proposals are not official logbook history. The core must validate plugin
/// capabilities, user permissions, event type, and payload schema before it
/// converts a proposal into an official append-only logbook event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProposalEnvelope {
    pub proposal_id: Uuid,
    pub proposal_type: String,
    pub logbook_id: Uuid,
    pub entity_id: Option<Uuid>,
    pub timestamp: DateTime<Utc>,
    pub author_operator_id: Option<Uuid>,
    pub author_device_id: Uuid,
    pub source_plugin_id: String,
    pub schema_version: u32,
    pub payload: Value,
}

impl ProposalEnvelope {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        proposal_type: impl Into<String>,
        logbook_id: Uuid,
        entity_id: Option<Uuid>,
        author_operator_id: Option<Uuid>,
        author_device_id: Uuid,
        source_plugin_id: impl Into<String>,
        schema_version: u32,
        payload: Value,
    ) -> Self {
        Self {
            proposal_id: Uuid::new_v4(),
            proposal_type: proposal_type.into(),
            logbook_id,
            entity_id,
            timestamp: Utc::now(),
            author_operator_id,
            author_device_id,
            source_plugin_id: source_plugin_id.into(),
            schema_version,
            payload,
        }
    }
}
