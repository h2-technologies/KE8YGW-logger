use chrono::{DateTime, Utc};
use ham_plugin_sdk::{
    PluginCapability, PluginManifest, ProposalEnvelope, OFFICIAL_LOG_QSO_CORRECTED,
    OFFICIAL_LOG_QSO_CREATED, OFFICIAL_LOG_QSO_DELETED, OFFICIAL_LOG_QSO_NOTE_ADDED,
    OFFICIAL_LOG_QSO_RESTORED, PROPOSAL_QSO_CORRECT, PROPOSAL_QSO_CREATE, PROPOSAL_QSO_DELETE,
    PROPOSAL_QSO_NOTE_ADD, PROPOSAL_QSO_RESTORE,
};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    bus::{BusEvent, EventBus, EventBusError, RuntimeEventEnvelope, RuntimeEventSeverity},
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
                PROPOSAL_QSO_CREATE
                    | PROPOSAL_QSO_CORRECT
                    | PROPOSAL_QSO_DELETE
                    | PROPOSAL_QSO_RESTORE
                    | PROPOSAL_QSO_NOTE_ADD
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
    publish_proposal_runtime_event(
        bus,
        &proposal,
        &format!("{}.received", proposal.proposal_type),
        RuntimeEventSeverity::Info,
        "QSO proposal received",
        None,
    )
    .await?;

    let result = submit_proposal_inner(store, bus, context, proposal.clone()).await;

    match &result {
        Ok(outcome) => {
            publish_proposal_runtime_event(
                bus,
                &proposal,
                &format!("{}.accepted", proposal.proposal_type),
                RuntimeEventSeverity::Info,
                "QSO proposal accepted",
                None,
            )
            .await?;
            publish_proposal_runtime_event(
                bus,
                &proposal,
                "official.log.event.appended",
                RuntimeEventSeverity::Info,
                &format!(
                    "Official event appended: {}",
                    outcome.official_event.event_type
                ),
                None,
            )
            .await?;
            publish_proposal_runtime_event(
                bus,
                &proposal,
                "projection.qso.updated",
                RuntimeEventSeverity::Debug,
                "QSO projection can be rebuilt from official events",
                None,
            )
            .await?;
        }
        Err(error) => {
            publish_proposal_runtime_event(
                bus,
                &proposal,
                &format!("{}.rejected", proposal.proposal_type),
                RuntimeEventSeverity::Warn,
                "QSO proposal rejected",
                Some(error.to_string()),
            )
            .await?;
        }
    }

    result
}

async fn submit_proposal_inner<S, B>(
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

    validate_qso_schema(store, &proposal).await?;

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
        PROPOSAL_QSO_RESTORE => Ok(PluginCapability::QsoRestore),
        PROPOSAL_QSO_NOTE_ADD => Ok(PluginCapability::QsoNoteAdd),
        other => Err(ProposalValidationError::UnsupportedProposalType(
            other.to_owned(),
        )),
    }
}

async fn validate_qso_schema<S>(
    store: &S,
    proposal: &ProposalEnvelope,
) -> Result<(), ProposalValidationError>
where
    S: LogbookEventStore,
{
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
            for field in [
                "contacted_callsign",
                "station_callsign",
                "mode",
                "started_at",
            ] {
                if !payload.get(field).is_some_and(Value::is_string) {
                    return Err(ProposalValidationError::InvalidSchema(format!(
                        "qso create requires string field `{field}`"
                    )));
                }
            }
            validate_started_at(payload)?;
            validate_frequency(payload)?;
            Ok(())
        }
        PROPOSAL_QSO_CORRECT => {
            require_existing_qso(store, proposal).await?;
            if payload.is_empty() {
                return Err(ProposalValidationError::InvalidSchema(
                    "qso correction payload must not be empty".to_owned(),
                ));
            }
            validate_frequency(payload)?;
            Ok(())
        }
        PROPOSAL_QSO_DELETE => {
            require_existing_qso(store, proposal).await?;
            Ok(())
        }
        PROPOSAL_QSO_RESTORE => {
            require_existing_qso(store, proposal).await?;
            Ok(())
        }
        PROPOSAL_QSO_NOTE_ADD => {
            require_existing_qso(store, proposal).await?;
            if !payload.get("note").is_some_and(Value::is_string) {
                return Err(ProposalValidationError::InvalidSchema(
                    "qso note_add requires string field `note`".to_owned(),
                ));
            }
            Ok(())
        }
        other => Err(ProposalValidationError::UnsupportedProposalType(
            other.to_owned(),
        )),
    }
}

fn validate_started_at(
    payload: &serde_json::Map<String, Value>,
) -> Result<(), ProposalValidationError> {
    let started_at = payload
        .get("started_at")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ProposalValidationError::InvalidSchema("started_at is required".to_owned())
        })?;
    let parsed = DateTime::parse_from_rfc3339(started_at)
        .map_err(|_| {
            ProposalValidationError::InvalidSchema(
                "started_at must be a valid UTC/RFC3339 timestamp".to_owned(),
            )
        })?
        .with_timezone(&Utc);
    if parsed.offset() != &Utc {
        return Err(ProposalValidationError::InvalidSchema(
            "started_at must be UTC".to_owned(),
        ));
    }
    Ok(())
}

fn validate_frequency(
    payload: &serde_json::Map<String, Value>,
) -> Result<(), ProposalValidationError> {
    if let Some(frequency) = payload.get("frequency_hz") {
        let Some(frequency) = frequency.as_u64() else {
            return Err(ProposalValidationError::InvalidSchema(
                "frequency_hz must be a positive integer".to_owned(),
            ));
        };
        if frequency == 0 {
            return Err(ProposalValidationError::InvalidSchema(
                "frequency_hz must be positive".to_owned(),
            ));
        }
    }
    Ok(())
}

async fn require_existing_qso<S>(
    store: &S,
    proposal: &ProposalEnvelope,
) -> Result<Uuid, ProposalValidationError>
where
    S: LogbookEventStore,
{
    let qso_id = require_entity_id(proposal)?;
    let projection = store.rebuild_projections(proposal.logbook_id).await?;
    if projection.get_including_deleted(qso_id).is_none() {
        return Err(ProposalValidationError::InvalidSchema(format!(
            "qso_id {qso_id} does not exist"
        )));
    }
    Ok(qso_id)
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
        PROPOSAL_QSO_RESTORE => (
            OFFICIAL_LOG_QSO_RESTORED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_QSO_NOTE_ADD => (
            OFFICIAL_LOG_QSO_NOTE_ADDED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        other => {
            return Err(ProposalValidationError::UnsupportedProposalType(
                other.to_owned(),
            ))
        }
    };

    let entity_id = entity_id.expect("all QSO official events have a QSO entity id");
    let mut payload = proposal.payload;
    payload["qso_id"] = serde_json::json!(entity_id);
    let station_callsign = payload
        .get("station_callsign")
        .and_then(Value::as_str)
        .unwrap_or("UNKNOWN")
        .to_owned();
    let operator_callsign = payload
        .get("operator_callsign")
        .and_then(Value::as_str)
        .map(str::to_owned);

    Ok(NewLogbookEvent {
        event_type,
        logbook_id: proposal.logbook_id,
        entity_id: Some(entity_id),
        author_operator_id: proposal.author_operator_id,
        station_callsign,
        operator_callsign,
        author_device_id: proposal.author_device_id,
        source_device_id: proposal.author_device_id,
        correlation_id: proposal.proposal_id,
        source_plugin_id: Some(proposal.source_plugin_id),
        schema_version: proposal.schema_version,
        payload,
    })
}

async fn publish_proposal_runtime_event<B>(
    bus: &B,
    proposal: &ProposalEnvelope,
    event_type: &str,
    severity: RuntimeEventSeverity,
    summary: &str,
    error: Option<String>,
) -> Result<(), ProposalValidationError>
where
    B: EventBus,
{
    bus.publish(BusEvent::Runtime(RuntimeEventEnvelope::new(
        event_type,
        severity,
        "ham-core",
        Some(proposal.source_plugin_id.clone()),
        proposal.proposal_id,
        Uuid::new_v4(),
        proposal.author_device_id,
        None,
        summary,
        None,
        error,
    )))
    .await?;
    Ok(())
}
