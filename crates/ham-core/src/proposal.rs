use ham_plugin_sdk::{
    PluginCapability, PluginManifest, ProposalEnvelope, OFFICIAL_LOG_QSO_CORRECTED,
    OFFICIAL_LOG_QSO_CREATED, OFFICIAL_LOG_QSO_DELETED, PROPOSAL_QSO_CORRECT, PROPOSAL_QSO_CREATE,
    PROPOSAL_QSO_DELETE,
};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    bus::{BusEvent, EventBus, EventBusError},
    event::{CoreEventEnvelope, NewLogbookEvent},
    store::{LogbookEventStore, StoreError},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperatorRole {
    ReadOnly,
    Logger,
    Admin,
}

impl OperatorRole {
    fn can_submit(self, proposal_type: &str) -> bool {
        match self {
            Self::ReadOnly => false,
            Self::Logger => matches!(proposal_type, PROPOSAL_QSO_CREATE | PROPOSAL_QSO_CORRECT),
            Self::Admin => matches!(
                proposal_type,
                PROPOSAL_QSO_CREATE | PROPOSAL_QSO_CORRECT | PROPOSAL_QSO_DELETE
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProposalContext {
    pub plugin_manifest: PluginManifest,
    pub operator_role: OperatorRole,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProposalOutcome {
    pub official_event: CoreEventEnvelope,
}

#[derive(Debug, Error)]
pub enum ProposalValidationError {
    #[error("plugin id mismatch: proposal came from {proposal_plugin_id}, manifest is {manifest_plugin_id}")]
    PluginMismatch {
        proposal_plugin_id: String,
        manifest_plugin_id: String,
    },
    #[error("plugin is missing required capability {0:?}")]
    MissingPluginCapability(PluginCapability),
    #[error("operator role {role:?} cannot submit {proposal_type}")]
    PermissionDenied {
        role: OperatorRole,
        proposal_type: String,
    },
    #[error("unsupported proposal type {0}")]
    UnsupportedProposalType(String),
    #[error("invalid proposal schema: {0}")]
    InvalidSchema(String),
    #[error("event store error: {0}")]
    Store(#[from] StoreError),
    #[error("event bus error: {0}")]
    EventBus(#[from] EventBusError),
}

pub async fn submit_proposal<S, B>(
    store: &S,
    bus: &B,
    context: &ProposalContext,
    proposal: ProposalEnvelope,
) -> Result<ProposalOutcome, ProposalValidationError>
where
    S: LogbookEventStore,
    B: EventBus,
{
    if proposal.source_plugin_id != context.plugin_manifest.plugin_id {
        return Err(ProposalValidationError::PluginMismatch {
            proposal_plugin_id: proposal.source_plugin_id,
            manifest_plugin_id: context.plugin_manifest.plugin_id.clone(),
        });
    }

    let required_capability = required_capability(&proposal.proposal_type)?;
    if !context.plugin_manifest.has_capability(&required_capability) {
        return Err(ProposalValidationError::MissingPluginCapability(
            required_capability,
        ));
    }

    if !context.operator_role.can_submit(&proposal.proposal_type) {
        return Err(ProposalValidationError::PermissionDenied {
            role: context.operator_role,
            proposal_type: proposal.proposal_type,
        });
    }

    validate_qso_schema(&proposal)?;

    let official_event = store.append_event(to_official_event(proposal)?).await?;
    bus.publish(BusEvent::OfficialLogbookEvent(official_event.clone()))
        .await?;

    Ok(ProposalOutcome { official_event })
}

fn required_capability(proposal_type: &str) -> Result<PluginCapability, ProposalValidationError> {
    match proposal_type {
        PROPOSAL_QSO_CREATE => Ok(PluginCapability::QsoCreate),
        PROPOSAL_QSO_CORRECT => Ok(PluginCapability::QsoCorrect),
        PROPOSAL_QSO_DELETE => Ok(PluginCapability::QsoDelete),
        other => Err(ProposalValidationError::UnsupportedProposalType(
            other.to_owned(),
        )),
    }
}

fn validate_qso_schema(proposal: &ProposalEnvelope) -> Result<(), ProposalValidationError> {
    if proposal.schema_version == 0 {
        return Err(ProposalValidationError::InvalidSchema(
            "schema_version must be greater than zero".to_owned(),
        ));
    }

    let Some(payload) = proposal.payload.as_object() else {
        return Err(ProposalValidationError::InvalidSchema(
            "payload must be a JSON object".to_owned(),
        ));
    };

    match proposal.proposal_type.as_str() {
        PROPOSAL_QSO_CREATE => {
            for field in ["callsign", "band", "mode"] {
                if !payload.get(field).is_some_and(Value::is_string) {
                    return Err(ProposalValidationError::InvalidSchema(format!(
                        "qso create requires string field `{field}`"
                    )));
                }
            }
            Ok(())
        }
        PROPOSAL_QSO_CORRECT => {
            require_entity_id(proposal)?;
            if payload.is_empty() {
                return Err(ProposalValidationError::InvalidSchema(
                    "qso correction payload must not be empty".to_owned(),
                ));
            }
            Ok(())
        }
        PROPOSAL_QSO_DELETE => {
            require_entity_id(proposal)?;
            Ok(())
        }
        other => Err(ProposalValidationError::UnsupportedProposalType(
            other.to_owned(),
        )),
    }
}

fn require_entity_id(proposal: &ProposalEnvelope) -> Result<Uuid, ProposalValidationError> {
    proposal.entity_id.ok_or_else(|| {
        ProposalValidationError::InvalidSchema(
            "qso correction/delete requires entity_id".to_owned(),
        )
    })
}

fn to_official_event(
    proposal: ProposalEnvelope,
) -> Result<NewLogbookEvent, ProposalValidationError> {
    let (event_type, entity_id) = match proposal.proposal_type.as_str() {
        PROPOSAL_QSO_CREATE => (
            OFFICIAL_LOG_QSO_CREATED.to_owned(),
            proposal.entity_id.or_else(|| Some(Uuid::new_v4())),
        ),
        PROPOSAL_QSO_CORRECT => (
            OFFICIAL_LOG_QSO_CORRECTED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_QSO_DELETE => (
            OFFICIAL_LOG_QSO_DELETED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        other => {
            return Err(ProposalValidationError::UnsupportedProposalType(
                other.to_owned(),
            ))
        }
    };

    Ok(NewLogbookEvent {
        event_type,
        logbook_id: proposal.logbook_id,
        entity_id,
        author_operator_id: proposal.author_operator_id,
        author_device_id: proposal.author_device_id,
        source_plugin_id: Some(proposal.source_plugin_id),
        schema_version: proposal.schema_version,
        payload: proposal.payload,
    })
}
