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

pub const OFFICIAL_LOG_QSO_CREATED: &str = "official.log.qso.created";
pub const OFFICIAL_LOG_QSO_CORRECTED: &str = "official.log.qso.corrected";
pub const OFFICIAL_LOG_QSO_DELETED: &str = "official.log.qso.deleted";

/// A capability granted to a plugin by the host application.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PluginCapability {
    QsoCreate,
    QsoCorrect,
    QsoDelete,
    Other(String),
}

impl PluginCapability {
    pub fn as_str(&self) -> &str {
        match self {
            Self::QsoCreate => "qso:create",
            Self::QsoCorrect => "qso:correct",
            Self::QsoDelete => "qso:delete",
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
            "qso:create" => Self::QsoCreate,
            "qso:correct" => Self::QsoCorrect,
            "qso:delete" => Self::QsoDelete,
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
    pub capabilities: Vec<PluginCapability>,
}

impl PluginManifest {
    pub fn has_capability(&self, capability: &PluginCapability) -> bool {
        self.capabilities.iter().any(|held| held == capability)
    }
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
