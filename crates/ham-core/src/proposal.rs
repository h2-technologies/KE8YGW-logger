use chrono::{DateTime, Utc};
use ham_plugin_sdk::{
    PluginCapability, PluginManifest, ProposalEnvelope, OFFICIAL_LOG_ACTIVATION_CANCELLED,
    OFFICIAL_LOG_ACTIVATION_CREATED, OFFICIAL_LOG_ACTIVATION_ENDED,
    OFFICIAL_LOG_ACTIVATION_NOTE_ADDED, OFFICIAL_LOG_ACTIVATION_STARTED,
    OFFICIAL_LOG_ACTIVATION_UPDATED, OFFICIAL_LOG_EMCOMM_ACTIVITY_LOGGED,
    OFFICIAL_LOG_EMCOMM_ASSIGNMENT_CREATED, OFFICIAL_LOG_EMCOMM_ASSIGNMENT_RELEASED,
    OFFICIAL_LOG_EMCOMM_ASSIGNMENT_UPDATED, OFFICIAL_LOG_EMCOMM_INCIDENT_CLOSED,
    OFFICIAL_LOG_EMCOMM_INCIDENT_OPENED, OFFICIAL_LOG_EMCOMM_INCIDENT_UPDATED,
    OFFICIAL_LOG_EMCOMM_MESSAGE_ACKNOWLEDGED, OFFICIAL_LOG_EMCOMM_MESSAGE_CANCELLED,
    OFFICIAL_LOG_EMCOMM_MESSAGE_CREATED, OFFICIAL_LOG_EMCOMM_MESSAGE_RECEIVED,
    OFFICIAL_LOG_EMCOMM_MESSAGE_TRANSMITTED, OFFICIAL_LOG_EMCOMM_MESSAGE_UPDATED,
    OFFICIAL_LOG_EMCOMM_PERIOD_CLOSED, OFFICIAL_LOG_EMCOMM_PERIOD_OPENED,
    OFFICIAL_LOG_EMCOMM_PERSON_CHECKED_IN, OFFICIAL_LOG_EMCOMM_PERSON_CHECKED_OUT,
    OFFICIAL_LOG_EMCOMM_PERSON_UPDATED, OFFICIAL_LOG_NET_CHECKIN_CREATED,
    OFFICIAL_LOG_NET_CHECKIN_DELETED, OFFICIAL_LOG_NET_CHECKIN_UPDATED,
    OFFICIAL_LOG_NET_REPORT_EXPORTED, OFFICIAL_LOG_NET_SESSION_CANCELLED,
    OFFICIAL_LOG_NET_SESSION_ENDED, OFFICIAL_LOG_NET_SESSION_STARTED,
    OFFICIAL_LOG_NET_TEMPLATE_CREATED, OFFICIAL_LOG_NET_TEMPLATE_UPDATED,
    OFFICIAL_LOG_NET_TRAFFIC_CREATED, OFFICIAL_LOG_NET_TRAFFIC_UPDATED,
    OFFICIAL_LOG_QSO_ACTIVATION_LINKED, OFFICIAL_LOG_QSO_ACTIVATION_UNLINKED,
    OFFICIAL_LOG_QSO_CORRECTED, OFFICIAL_LOG_QSO_CREATED, OFFICIAL_LOG_QSO_DELETED,
    OFFICIAL_LOG_QSO_NOTE_ADDED, OFFICIAL_LOG_QSO_RESTORED, PROPOSAL_ACTIVATION_CANCEL,
    PROPOSAL_ACTIVATION_CREATE, PROPOSAL_ACTIVATION_END, PROPOSAL_ACTIVATION_NOTE_ADD,
    PROPOSAL_ACTIVATION_START, PROPOSAL_ACTIVATION_UPDATE, PROPOSAL_EMCOMM_ACTIVITY_LOG,
    PROPOSAL_EMCOMM_ASSIGNMENT_CREATE, PROPOSAL_EMCOMM_ASSIGNMENT_RELEASE,
    PROPOSAL_EMCOMM_ASSIGNMENT_UPDATE, PROPOSAL_EMCOMM_INCIDENT_CLOSE,
    PROPOSAL_EMCOMM_INCIDENT_OPEN, PROPOSAL_EMCOMM_INCIDENT_UPDATE,
    PROPOSAL_EMCOMM_MESSAGE_ACKNOWLEDGE, PROPOSAL_EMCOMM_MESSAGE_CANCEL,
    PROPOSAL_EMCOMM_MESSAGE_CREATE, PROPOSAL_EMCOMM_MESSAGE_RECEIVE,
    PROPOSAL_EMCOMM_MESSAGE_TRANSMIT, PROPOSAL_EMCOMM_MESSAGE_UPDATE, PROPOSAL_EMCOMM_PERIOD_CLOSE,
    PROPOSAL_EMCOMM_PERIOD_OPEN, PROPOSAL_EMCOMM_PERSON_CHECK_IN, PROPOSAL_EMCOMM_PERSON_CHECK_OUT,
    PROPOSAL_EMCOMM_PERSON_UPDATE, PROPOSAL_NET_CHECKIN_CREATE, PROPOSAL_NET_CHECKIN_DELETE,
    PROPOSAL_NET_CHECKIN_UPDATE, PROPOSAL_NET_REPORT_EXPORT, PROPOSAL_NET_SESSION_CANCEL,
    PROPOSAL_NET_SESSION_END, PROPOSAL_NET_SESSION_START, PROPOSAL_NET_TEMPLATE_CREATE,
    PROPOSAL_NET_TEMPLATE_UPDATE, PROPOSAL_NET_TRAFFIC_CREATE, PROPOSAL_NET_TRAFFIC_UPDATE,
    PROPOSAL_QSO_ACTIVATION_LINK, PROPOSAL_QSO_ACTIVATION_UNLINK, PROPOSAL_QSO_CORRECT,
    PROPOSAL_QSO_CREATE, PROPOSAL_QSO_DELETE, PROPOSAL_QSO_NOTE_ADD, PROPOSAL_QSO_RESTORE,
};
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    bus::{BusEvent, EventBus, EventBusError, RuntimeEventEnvelope, RuntimeEventSeverity},
    event::{CoreEventEnvelope, NewLogbookEvent},
    net::NetControlProjection,
    permissions::{check_plugin_permission, PermissionGrantSet},
    projection::Projection,
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
                    | PROPOSAL_ACTIVATION_CREATE
                    | PROPOSAL_ACTIVATION_UPDATE
                    | PROPOSAL_ACTIVATION_START
                    | PROPOSAL_ACTIVATION_END
                    | PROPOSAL_ACTIVATION_CANCEL
                    | PROPOSAL_ACTIVATION_NOTE_ADD
                    | PROPOSAL_QSO_ACTIVATION_LINK
                    | PROPOSAL_QSO_ACTIVATION_UNLINK
                    | PROPOSAL_NET_TEMPLATE_CREATE
                    | PROPOSAL_NET_TEMPLATE_UPDATE
                    | PROPOSAL_NET_SESSION_START
                    | PROPOSAL_NET_SESSION_END
                    | PROPOSAL_NET_SESSION_CANCEL
                    | PROPOSAL_NET_CHECKIN_CREATE
                    | PROPOSAL_NET_CHECKIN_UPDATE
                    | PROPOSAL_NET_CHECKIN_DELETE
                    | PROPOSAL_NET_TRAFFIC_CREATE
                    | PROPOSAL_NET_TRAFFIC_UPDATE
                    | PROPOSAL_NET_REPORT_EXPORT
                    | PROPOSAL_EMCOMM_INCIDENT_OPEN
                    | PROPOSAL_EMCOMM_INCIDENT_UPDATE
                    | PROPOSAL_EMCOMM_INCIDENT_CLOSE
                    | PROPOSAL_EMCOMM_PERIOD_OPEN
                    | PROPOSAL_EMCOMM_PERIOD_CLOSE
                    | PROPOSAL_EMCOMM_PERSON_CHECK_IN
                    | PROPOSAL_EMCOMM_PERSON_UPDATE
                    | PROPOSAL_EMCOMM_PERSON_CHECK_OUT
                    | PROPOSAL_EMCOMM_ASSIGNMENT_CREATE
                    | PROPOSAL_EMCOMM_ASSIGNMENT_UPDATE
                    | PROPOSAL_EMCOMM_ASSIGNMENT_RELEASE
                    | PROPOSAL_EMCOMM_MESSAGE_CREATE
                    | PROPOSAL_EMCOMM_MESSAGE_UPDATE
                    | PROPOSAL_EMCOMM_MESSAGE_TRANSMIT
                    | PROPOSAL_EMCOMM_MESSAGE_RECEIVE
                    | PROPOSAL_EMCOMM_MESSAGE_ACKNOWLEDGE
                    | PROPOSAL_EMCOMM_MESSAGE_CANCEL
                    | PROPOSAL_EMCOMM_ACTIVITY_LOG
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProposalContext {
    pub plugin_manifest: PluginManifest,
    pub operator_role: OperatorRole,
    pub permission_grants: PermissionGrantSet,
}

impl ProposalContext {
    pub fn local_admin(plugin_manifest: PluginManifest, operator_role: OperatorRole) -> Self {
        let permission_grants = PermissionGrantSet::grants_for_manifest(&plugin_manifest);
        Self {
            plugin_manifest,
            operator_role,
            permission_grants,
        }
    }
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
    #[error("plugin permission denied: {0}")]
    PluginPermissionDenied(String),
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
    S: LogbookEventStore + ?Sized,
    B: EventBus + ?Sized,
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
    S: LogbookEventStore + ?Sized,
    B: EventBus + ?Sized,
{
    if proposal.source_plugin_id != context.plugin_manifest.plugin_id {
        return Err(ProposalValidationError::PluginMismatch {
            proposal_plugin_id: proposal.source_plugin_id,
            manifest_plugin_id: context.plugin_manifest.plugin_id.clone(),
        });
    }

    let required_capability = required_capability(&proposal.proposal_type)?;
    if !context.plugin_manifest.has_capability(&required_capability) {
        publish_permission_check_event(
            bus,
            &proposal,
            "plugin.permission.check.denied",
            RuntimeEventSeverity::Warn,
            &required_capability,
            "plugin manifest does not request required permission",
        )
        .await?;
        return Err(ProposalValidationError::MissingPluginCapability(
            required_capability,
        ));
    }
    if let Err(error) = check_plugin_permission(
        &context.plugin_manifest,
        &context.permission_grants,
        &required_capability,
    ) {
        publish_permission_check_event(
            bus,
            &proposal,
            "plugin.permission.check.denied",
            RuntimeEventSeverity::Warn,
            &required_capability,
            "plugin permission grant is missing or denied",
        )
        .await?;
        return Err(ProposalValidationError::PluginPermissionDenied(
            error.to_string(),
        ));
    }
    publish_permission_check_event(
        bus,
        &proposal,
        "plugin.permission.check.allowed",
        RuntimeEventSeverity::Debug,
        &required_capability,
        "plugin permission check allowed",
    )
    .await?;

    if !context.operator_role.can_submit(&proposal.proposal_type) {
        publish_permission_check_event(
            bus,
            &proposal,
            "plugin.permission.check.denied",
            RuntimeEventSeverity::Warn,
            &required_capability,
            "operator role cannot submit this proposal",
        )
        .await?;
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
        PROPOSAL_ACTIVATION_CREATE | PROPOSAL_ACTIVATION_START => {
            Ok(PluginCapability::ActivationCreate)
        }
        PROPOSAL_ACTIVATION_UPDATE | PROPOSAL_ACTIVATION_NOTE_ADD => {
            Ok(PluginCapability::ActivationUpdate)
        }
        PROPOSAL_ACTIVATION_END => Ok(PluginCapability::ActivationEnd),
        PROPOSAL_ACTIVATION_CANCEL => Ok(PluginCapability::ActivationCancel),
        PROPOSAL_QSO_ACTIVATION_LINK | PROPOSAL_QSO_ACTIVATION_UNLINK => {
            Ok(PluginCapability::QsoCorrect)
        }
        PROPOSAL_NET_TEMPLATE_CREATE => Ok(PluginCapability::NetTemplateCreate),
        PROPOSAL_NET_TEMPLATE_UPDATE => Ok(PluginCapability::NetTemplateUpdate),
        PROPOSAL_NET_SESSION_START => Ok(PluginCapability::NetSessionStart),
        PROPOSAL_NET_SESSION_END | PROPOSAL_NET_SESSION_CANCEL => {
            Ok(PluginCapability::NetSessionEnd)
        }
        PROPOSAL_NET_CHECKIN_CREATE => Ok(PluginCapability::NetCheckinCreate),
        PROPOSAL_NET_CHECKIN_UPDATE => Ok(PluginCapability::NetCheckinUpdate),
        PROPOSAL_NET_CHECKIN_DELETE => Ok(PluginCapability::NetCheckinDelete),
        PROPOSAL_NET_TRAFFIC_CREATE | PROPOSAL_NET_TRAFFIC_UPDATE => {
            Ok(PluginCapability::NetTrafficManage)
        }
        PROPOSAL_NET_REPORT_EXPORT => Ok(PluginCapability::NetReportExport),
        PROPOSAL_EMCOMM_INCIDENT_OPEN
        | PROPOSAL_EMCOMM_INCIDENT_UPDATE
        | PROPOSAL_EMCOMM_INCIDENT_CLOSE => Ok(PluginCapability::EmCommIncidentManage),
        PROPOSAL_EMCOMM_PERIOD_OPEN | PROPOSAL_EMCOMM_PERIOD_CLOSE => {
            Ok(PluginCapability::EmCommPeriodManage)
        }
        PROPOSAL_EMCOMM_PERSON_CHECK_IN
        | PROPOSAL_EMCOMM_PERSON_UPDATE
        | PROPOSAL_EMCOMM_PERSON_CHECK_OUT => Ok(PluginCapability::EmCommPersonManage),
        PROPOSAL_EMCOMM_ASSIGNMENT_CREATE
        | PROPOSAL_EMCOMM_ASSIGNMENT_UPDATE
        | PROPOSAL_EMCOMM_ASSIGNMENT_RELEASE => Ok(PluginCapability::EmCommAssignmentManage),
        PROPOSAL_EMCOMM_MESSAGE_CREATE
        | PROPOSAL_EMCOMM_MESSAGE_UPDATE
        | PROPOSAL_EMCOMM_MESSAGE_TRANSMIT
        | PROPOSAL_EMCOMM_MESSAGE_RECEIVE
        | PROPOSAL_EMCOMM_MESSAGE_ACKNOWLEDGE
        | PROPOSAL_EMCOMM_MESSAGE_CANCEL => Ok(PluginCapability::EmCommMessageManage),
        PROPOSAL_EMCOMM_ACTIVITY_LOG => Ok(PluginCapability::EmCommActivityLog),
        other => Err(ProposalValidationError::UnsupportedProposalType(
            other.to_owned(),
        )),
    }
}

async fn publish_permission_check_event<B>(
    bus: &B,
    proposal: &ProposalEnvelope,
    event_type: &str,
    severity: RuntimeEventSeverity,
    permission: &PluginCapability,
    summary: &str,
) -> Result<(), ProposalValidationError>
where
    B: EventBus + ?Sized,
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
        Some(serde_json::json!({
            "plugin_id": proposal.source_plugin_id,
            "permission_id": permission.as_str(),
            "proposal_type": proposal.proposal_type,
        })),
        None,
    )))
    .await?;
    Ok(())
}

async fn validate_qso_schema<S>(
    store: &S,
    proposal: &ProposalEnvelope,
) -> Result<(), ProposalValidationError>
where
    S: LogbookEventStore + ?Sized,
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
        PROPOSAL_ACTIVATION_CREATE => {
            validate_activation_payload(payload, false)?;
            Ok(())
        }
        PROPOSAL_ACTIVATION_START => {
            validate_activation_payload(payload, true)?;
            Ok(())
        }
        PROPOSAL_ACTIVATION_UPDATE => {
            require_existing_activation(store, proposal).await?;
            validate_ended_after_started(payload)?;
            Ok(())
        }
        PROPOSAL_ACTIVATION_END => {
            require_existing_activation(store, proposal).await?;
            if !payload.get("ended_at").is_some_and(Value::is_string) {
                return Err(ProposalValidationError::InvalidSchema(
                    "activation end requires string field `ended_at`".to_owned(),
                ));
            }
            validate_ended_after_started(payload)?;
            Ok(())
        }
        PROPOSAL_ACTIVATION_CANCEL => {
            require_existing_activation(store, proposal).await?;
            Ok(())
        }
        PROPOSAL_ACTIVATION_NOTE_ADD => {
            require_existing_activation(store, proposal).await?;
            if !payload.get("note").is_some_and(Value::is_string) {
                return Err(ProposalValidationError::InvalidSchema(
                    "activation note_add requires string field `note`".to_owned(),
                ));
            }
            Ok(())
        }
        PROPOSAL_QSO_ACTIVATION_LINK => {
            require_existing_qso(store, proposal).await?;
            let activation_id = payload
                .get("activation_id")
                .and_then(Value::as_str)
                .and_then(|value| Uuid::parse_str(value).ok())
                .ok_or_else(|| {
                    ProposalValidationError::InvalidSchema(
                        "qso activation link requires activation_id".to_owned(),
                    )
                })?;
            let projection = store
                .rebuild_activation_projections(proposal.logbook_id)
                .await?;
            let activation = projection.get(activation_id).ok_or_else(|| {
                ProposalValidationError::InvalidSchema(format!(
                    "activation_id {activation_id} does not exist"
                ))
            })?;
            if matches!(activation.status.as_str(), "ended" | "cancelled") {
                return Err(ProposalValidationError::InvalidSchema(
                    "ended/cancelled activations cannot accept new linked QSOs in MVP".to_owned(),
                ));
            }
            Ok(())
        }
        PROPOSAL_QSO_ACTIVATION_UNLINK => {
            require_existing_qso(store, proposal).await?;
            Ok(())
        }
        PROPOSAL_NET_TEMPLATE_CREATE => {
            require_string(payload, "name", "net template create")?;
            Ok(())
        }
        PROPOSAL_NET_TEMPLATE_UPDATE => {
            require_existing_net_template(store, proposal).await?;
            Ok(())
        }
        PROPOSAL_NET_SESSION_START => {
            for field in [
                "station_callsign",
                "net_control_operator_id",
                "net_name",
                "started_at",
            ] {
                require_string(payload, field, "net session start")?;
            }
            validate_started_at(payload)?;
            Ok(())
        }
        PROPOSAL_NET_SESSION_END => {
            require_existing_active_net_session(store, proposal).await?;
            require_string(payload, "ended_at", "net session end")?;
            validate_ended_after_started(payload)?;
            Ok(())
        }
        PROPOSAL_NET_SESSION_CANCEL => {
            require_existing_net_session(store, proposal).await?;
            Ok(())
        }
        PROPOSAL_NET_CHECKIN_CREATE => {
            let session_id = net_session_id_from_payload(payload)?;
            let projection = rebuild_net_projection(store, proposal.logbook_id).await?;
            let session = projection.get_session(session_id).ok_or_else(|| {
                ProposalValidationError::InvalidSchema(format!(
                    "net_session_id {session_id} does not exist"
                ))
            })?;
            if session.status != crate::NetSessionStatus::Active {
                return Err(ProposalValidationError::InvalidSchema(
                    "active net session required for check-ins".to_owned(),
                ));
            }
            let tactical_only = payload
                .get("tactical_only")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            if !tactical_only && !payload.get("callsign").is_some_and(Value::is_string) {
                return Err(ProposalValidationError::InvalidSchema(
                    "check-in requires callsign unless tactical_only is true".to_owned(),
                ));
            }
            require_string(payload, "checkin_time", "net checkin create")?;
            validate_checkin_time(payload)?;
            Ok(())
        }
        PROPOSAL_NET_CHECKIN_UPDATE | PROPOSAL_NET_CHECKIN_DELETE => {
            require_existing_net_checkin(store, proposal).await?;
            Ok(())
        }
        PROPOSAL_NET_TRAFFIC_CREATE => {
            let session_id = net_session_id_from_payload(payload)?;
            let projection = rebuild_net_projection(store, proposal.logbook_id).await?;
            let session = projection.get_session(session_id).ok_or_else(|| {
                ProposalValidationError::InvalidSchema(format!(
                    "net_session_id {session_id} does not exist"
                ))
            })?;
            if session.status != crate::NetSessionStatus::Active {
                return Err(ProposalValidationError::InvalidSchema(
                    "active net session required for traffic".to_owned(),
                ));
            }
            require_string(payload, "summary", "net traffic create")?;
            Ok(())
        }
        PROPOSAL_NET_TRAFFIC_UPDATE => {
            require_entity_id(proposal)?;
            Ok(())
        }
        PROPOSAL_NET_REPORT_EXPORT => {
            require_existing_net_session(store, proposal).await?;
            Ok(())
        }
        PROPOSAL_EMCOMM_INCIDENT_OPEN => {
            require_string(payload, "incident_name", "an incident")?;
            require_string(payload, "opened_at", "an incident")?;
            Ok(())
        }
        PROPOSAL_EMCOMM_INCIDENT_UPDATE | PROPOSAL_EMCOMM_INCIDENT_CLOSE => {
            require_entity_id(proposal)?;
            require_non_empty_correction(payload, "an incident update")?;
            Ok(())
        }
        PROPOSAL_EMCOMM_PERIOD_OPEN => {
            require_string(payload, "incident_id", "an operational period")?;
            require_string(payload, "started_at", "an operational period")?;
            if !payload
                .get("period_number")
                .is_some_and(|value| value.is_number() || value.is_string())
            {
                return Err(ProposalValidationError::InvalidSchema(
                    "an operational period requires period_number".to_owned(),
                ));
            }
            Ok(())
        }
        PROPOSAL_EMCOMM_PERIOD_CLOSE => {
            require_entity_id(proposal)?;
            require_string(payload, "ended_at", "closing an operational period")?;
            Ok(())
        }
        PROPOSAL_EMCOMM_PERSON_CHECK_IN => {
            for field in ["incident_id", "name", "checked_in_at"] {
                require_string(payload, field, "an ICS 211 check-in")?;
            }
            Ok(())
        }
        PROPOSAL_EMCOMM_PERSON_UPDATE => {
            require_entity_id(proposal)?;
            require_non_empty_correction(payload, "a check-in correction")?;
            Ok(())
        }
        PROPOSAL_EMCOMM_PERSON_CHECK_OUT => {
            require_entity_id(proposal)?;
            require_string(payload, "checked_out_at", "a check-out")?;
            Ok(())
        }
        PROPOSAL_EMCOMM_ASSIGNMENT_CREATE => {
            for field in ["incident_id", "person_id", "assignment"] {
                require_string(payload, field, "an assignment")?;
            }
            Ok(())
        }
        PROPOSAL_EMCOMM_ASSIGNMENT_UPDATE => {
            require_entity_id(proposal)?;
            require_non_empty_correction(payload, "an assignment correction")?;
            Ok(())
        }
        PROPOSAL_EMCOMM_ASSIGNMENT_RELEASE => {
            require_entity_id(proposal)?;
            require_string(payload, "released_at", "releasing an assignment")?;
            Ok(())
        }
        PROPOSAL_EMCOMM_MESSAGE_CREATE => {
            for field in ["incident_id", "message_number", "from", "to", "body"] {
                require_string(payload, field, "an incident message")?;
            }
            validate_message_number(payload)?;
            validate_message_precedence(payload)?;
            Ok(())
        }
        PROPOSAL_EMCOMM_MESSAGE_UPDATE => {
            require_entity_id(proposal)?;
            require_non_empty_correction(payload, "a message correction")?;
            if payload.contains_key("message_number") {
                return Err(ProposalValidationError::InvalidSchema(
                    "a message number is assigned once and cannot be corrected".to_owned(),
                ));
            }
            validate_message_precedence(payload)?;
            Ok(())
        }
        PROPOSAL_EMCOMM_MESSAGE_TRANSMIT => {
            require_entity_id(proposal)?;
            require_string(payload, "transmitted_at", "transmitting a message")?;
            Ok(())
        }
        PROPOSAL_EMCOMM_MESSAGE_RECEIVE => {
            require_entity_id(proposal)?;
            require_string(payload, "received_at", "receiving a message")?;
            Ok(())
        }
        PROPOSAL_EMCOMM_MESSAGE_ACKNOWLEDGE => {
            require_entity_id(proposal)?;
            require_string(payload, "acknowledged_at", "acknowledging a message")?;
            Ok(())
        }
        PROPOSAL_EMCOMM_MESSAGE_CANCEL => {
            require_entity_id(proposal)?;
            require_string(payload, "cancelled_at", "cancelling a message")?;
            require_string(payload, "reason", "cancelling a message")?;
            Ok(())
        }
        PROPOSAL_EMCOMM_ACTIVITY_LOG => {
            for field in ["incident_id", "occurred_at", "summary"] {
                require_string(payload, field, "an ICS 214 activity entry")?;
            }
            Ok(())
        }
        other => Err(ProposalValidationError::UnsupportedProposalType(
            other.to_owned(),
        )),
    }
}

fn validate_activation_payload(
    payload: &serde_json::Map<String, Value>,
    require_started_at: bool,
) -> Result<(), ProposalValidationError> {
    for field in ["activation_type", "station_callsign", "operator_callsign"] {
        if !payload.get(field).is_some_and(Value::is_string) {
            return Err(ProposalValidationError::InvalidSchema(format!(
                "activation requires string field `{field}`"
            )));
        }
    }
    if require_started_at && !payload.get("started_at").is_some_and(Value::is_string) {
        return Err(ProposalValidationError::InvalidSchema(
            "starting an activation requires started_at".to_owned(),
        ));
    }
    match payload
        .get("activation_type")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "pota" if !payload.get("park_id").is_some_and(Value::is_string) => {
            return Err(ProposalValidationError::InvalidSchema(
                "POTA activation requires park_id".to_owned(),
            ));
        }
        "sota" if !payload.get("summit_id").is_some_and(Value::is_string) => {
            return Err(ProposalValidationError::InvalidSchema(
                "SOTA activation requires summit_id".to_owned(),
            ));
        }
        _ => {}
    }
    validate_ended_after_started(payload)?;
    Ok(())
}

fn require_string(
    payload: &serde_json::Map<String, Value>,
    field: &str,
    label: &str,
) -> Result<(), ProposalValidationError> {
    if !payload.get(field).is_some_and(Value::is_string) {
        return Err(ProposalValidationError::InvalidSchema(format!(
            "{label} requires string field `{field}`"
        )));
    }
    Ok(())
}

fn validate_ended_after_started(
    payload: &serde_json::Map<String, Value>,
) -> Result<(), ProposalValidationError> {
    let started_at = payload
        .get("started_at")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc));
    let ended_at = payload
        .get("ended_at")
        .and_then(Value::as_str)
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc));
    if let (Some(started_at), Some(ended_at)) = (started_at, ended_at) {
        if ended_at <= started_at {
            return Err(ProposalValidationError::InvalidSchema(
                "ended_at must be after started_at".to_owned(),
            ));
        }
    }
    Ok(())
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

fn validate_checkin_time(
    payload: &serde_json::Map<String, Value>,
) -> Result<(), ProposalValidationError> {
    let checkin_time = payload
        .get("checkin_time")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            ProposalValidationError::InvalidSchema("checkin_time is required".to_owned())
        })?;
    let _parsed = DateTime::parse_from_rfc3339(checkin_time)
        .map_err(|_| {
            ProposalValidationError::InvalidSchema(
                "checkin_time must be a valid UTC/RFC3339 timestamp".to_owned(),
            )
        })?
        .with_timezone(&Utc);
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
    S: LogbookEventStore + ?Sized,
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

async fn require_existing_activation<S>(
    store: &S,
    proposal: &ProposalEnvelope,
) -> Result<Uuid, ProposalValidationError>
where
    S: LogbookEventStore + ?Sized,
{
    let activation_id = require_entity_id(proposal)?;
    let projection = store
        .rebuild_activation_projections(proposal.logbook_id)
        .await?;
    if projection.get(activation_id).is_none() {
        return Err(ProposalValidationError::InvalidSchema(format!(
            "activation_id {activation_id} does not exist"
        )));
    }
    Ok(activation_id)
}

async fn rebuild_net_projection<S>(
    store: &S,
    logbook_id: Uuid,
) -> Result<NetControlProjection, ProposalValidationError>
where
    S: LogbookEventStore + ?Sized,
{
    let events = store.list_events(logbook_id).await?;
    let mut projection = NetControlProjection::new();
    projection
        .rebuild(&events)
        .map_err(|error| ProposalValidationError::InvalidSchema(error.to_string()))?;
    Ok(projection)
}

async fn require_existing_net_template<S>(
    store: &S,
    proposal: &ProposalEnvelope,
) -> Result<Uuid, ProposalValidationError>
where
    S: LogbookEventStore + ?Sized,
{
    let template_id = require_entity_id(proposal)?;
    let projection = rebuild_net_projection(store, proposal.logbook_id).await?;
    let template_id_string = template_id.to_string();
    if !projection.templates().iter().any(|template| {
        template.get("net_template_id").and_then(Value::as_str) == Some(template_id_string.as_str())
    }) {
        return Err(ProposalValidationError::InvalidSchema(format!(
            "net_template_id {template_id} does not exist"
        )));
    }
    Ok(template_id)
}

async fn require_existing_net_session<S>(
    store: &S,
    proposal: &ProposalEnvelope,
) -> Result<Uuid, ProposalValidationError>
where
    S: LogbookEventStore + ?Sized,
{
    let session_id = require_entity_id(proposal)?;
    let projection = rebuild_net_projection(store, proposal.logbook_id).await?;
    if projection.get_session(session_id).is_none() {
        return Err(ProposalValidationError::InvalidSchema(format!(
            "net_session_id {session_id} does not exist"
        )));
    }
    Ok(session_id)
}

async fn require_existing_active_net_session<S>(
    store: &S,
    proposal: &ProposalEnvelope,
) -> Result<Uuid, ProposalValidationError>
where
    S: LogbookEventStore + ?Sized,
{
    let session_id = require_existing_net_session(store, proposal).await?;
    let projection = rebuild_net_projection(store, proposal.logbook_id).await?;
    let session = projection.get_session(session_id).ok_or_else(|| {
        ProposalValidationError::InvalidSchema(format!(
            "net_session_id {session_id} does not exist"
        ))
    })?;
    if session.status != crate::NetSessionStatus::Active {
        return Err(ProposalValidationError::InvalidSchema(
            "active net session required".to_owned(),
        ));
    }
    Ok(session_id)
}

async fn require_existing_net_checkin<S>(
    store: &S,
    proposal: &ProposalEnvelope,
) -> Result<Uuid, ProposalValidationError>
where
    S: LogbookEventStore + ?Sized,
{
    let checkin_id = require_entity_id(proposal)?;
    let projection = rebuild_net_projection(store, proposal.logbook_id).await?;
    if !projection.sessions(true).iter().any(|session| {
        projection
            .checkins_for_session(session.net_session_id, true)
            .iter()
            .any(|checkin| checkin.checkin_id == checkin_id)
    }) {
        return Err(ProposalValidationError::InvalidSchema(format!(
            "checkin_id {checkin_id} does not exist"
        )));
    }
    Ok(checkin_id)
}

fn net_session_id_from_payload(
    payload: &serde_json::Map<String, Value>,
) -> Result<Uuid, ProposalValidationError> {
    payload
        .get("net_session_id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| {
            ProposalValidationError::InvalidSchema(
                "net proposal requires net_session_id".to_owned(),
            )
        })
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
        PROPOSAL_ACTIVATION_CREATE => (
            OFFICIAL_LOG_ACTIVATION_CREATED.to_owned(),
            proposal.entity_id.or_else(|| Some(Uuid::new_v4())),
        ),
        PROPOSAL_ACTIVATION_START => (
            OFFICIAL_LOG_ACTIVATION_STARTED.to_owned(),
            proposal.entity_id.or_else(|| Some(Uuid::new_v4())),
        ),
        PROPOSAL_ACTIVATION_UPDATE => (
            OFFICIAL_LOG_ACTIVATION_UPDATED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_ACTIVATION_END => (
            OFFICIAL_LOG_ACTIVATION_ENDED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_ACTIVATION_CANCEL => (
            OFFICIAL_LOG_ACTIVATION_CANCELLED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_ACTIVATION_NOTE_ADD => (
            OFFICIAL_LOG_ACTIVATION_NOTE_ADDED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_QSO_ACTIVATION_LINK => (
            OFFICIAL_LOG_QSO_ACTIVATION_LINKED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_QSO_ACTIVATION_UNLINK => (
            OFFICIAL_LOG_QSO_ACTIVATION_UNLINKED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_NET_TEMPLATE_CREATE => (
            OFFICIAL_LOG_NET_TEMPLATE_CREATED.to_owned(),
            proposal.entity_id.or_else(|| Some(Uuid::new_v4())),
        ),
        PROPOSAL_NET_TEMPLATE_UPDATE => (
            OFFICIAL_LOG_NET_TEMPLATE_UPDATED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_NET_SESSION_START => (
            OFFICIAL_LOG_NET_SESSION_STARTED.to_owned(),
            proposal.entity_id.or_else(|| Some(Uuid::new_v4())),
        ),
        PROPOSAL_NET_SESSION_END => (
            OFFICIAL_LOG_NET_SESSION_ENDED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_NET_SESSION_CANCEL => (
            OFFICIAL_LOG_NET_SESSION_CANCELLED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_NET_CHECKIN_CREATE => (
            OFFICIAL_LOG_NET_CHECKIN_CREATED.to_owned(),
            proposal.entity_id.or_else(|| Some(Uuid::new_v4())),
        ),
        PROPOSAL_NET_CHECKIN_UPDATE => (
            OFFICIAL_LOG_NET_CHECKIN_UPDATED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_NET_CHECKIN_DELETE => (
            OFFICIAL_LOG_NET_CHECKIN_DELETED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_NET_TRAFFIC_CREATE => (
            OFFICIAL_LOG_NET_TRAFFIC_CREATED.to_owned(),
            proposal.entity_id.or_else(|| Some(Uuid::new_v4())),
        ),
        PROPOSAL_NET_TRAFFIC_UPDATE => (
            OFFICIAL_LOG_NET_TRAFFIC_UPDATED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_NET_REPORT_EXPORT => (
            OFFICIAL_LOG_NET_REPORT_EXPORTED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_EMCOMM_INCIDENT_OPEN => (
            OFFICIAL_LOG_EMCOMM_INCIDENT_OPENED.to_owned(),
            proposal.entity_id.or_else(|| Some(Uuid::new_v4())),
        ),
        PROPOSAL_EMCOMM_INCIDENT_UPDATE => (
            OFFICIAL_LOG_EMCOMM_INCIDENT_UPDATED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_EMCOMM_INCIDENT_CLOSE => (
            OFFICIAL_LOG_EMCOMM_INCIDENT_CLOSED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_EMCOMM_PERIOD_OPEN => (
            OFFICIAL_LOG_EMCOMM_PERIOD_OPENED.to_owned(),
            proposal.entity_id.or_else(|| Some(Uuid::new_v4())),
        ),
        PROPOSAL_EMCOMM_PERIOD_CLOSE => (
            OFFICIAL_LOG_EMCOMM_PERIOD_CLOSED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_EMCOMM_PERSON_CHECK_IN => (
            OFFICIAL_LOG_EMCOMM_PERSON_CHECKED_IN.to_owned(),
            proposal.entity_id.or_else(|| Some(Uuid::new_v4())),
        ),
        PROPOSAL_EMCOMM_PERSON_UPDATE => (
            OFFICIAL_LOG_EMCOMM_PERSON_UPDATED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_EMCOMM_PERSON_CHECK_OUT => (
            OFFICIAL_LOG_EMCOMM_PERSON_CHECKED_OUT.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_EMCOMM_ASSIGNMENT_CREATE => (
            OFFICIAL_LOG_EMCOMM_ASSIGNMENT_CREATED.to_owned(),
            proposal.entity_id.or_else(|| Some(Uuid::new_v4())),
        ),
        PROPOSAL_EMCOMM_ASSIGNMENT_UPDATE => (
            OFFICIAL_LOG_EMCOMM_ASSIGNMENT_UPDATED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_EMCOMM_ASSIGNMENT_RELEASE => (
            OFFICIAL_LOG_EMCOMM_ASSIGNMENT_RELEASED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_EMCOMM_MESSAGE_CREATE => (
            OFFICIAL_LOG_EMCOMM_MESSAGE_CREATED.to_owned(),
            proposal.entity_id.or_else(|| Some(Uuid::new_v4())),
        ),
        PROPOSAL_EMCOMM_MESSAGE_UPDATE => (
            OFFICIAL_LOG_EMCOMM_MESSAGE_UPDATED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_EMCOMM_MESSAGE_TRANSMIT => (
            OFFICIAL_LOG_EMCOMM_MESSAGE_TRANSMITTED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_EMCOMM_MESSAGE_RECEIVE => (
            OFFICIAL_LOG_EMCOMM_MESSAGE_RECEIVED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_EMCOMM_MESSAGE_ACKNOWLEDGE => (
            OFFICIAL_LOG_EMCOMM_MESSAGE_ACKNOWLEDGED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_EMCOMM_MESSAGE_CANCEL => (
            OFFICIAL_LOG_EMCOMM_MESSAGE_CANCELLED.to_owned(),
            Some(require_entity_id(&proposal)?),
        ),
        PROPOSAL_EMCOMM_ACTIVITY_LOG => (
            OFFICIAL_LOG_EMCOMM_ACTIVITY_LOGGED.to_owned(),
            proposal.entity_id.or_else(|| Some(Uuid::new_v4())),
        ),
        other => {
            return Err(ProposalValidationError::UnsupportedProposalType(
                other.to_owned(),
            ))
        }
    };

    let entity_id = entity_id.expect("official proposal events have an entity id");
    let mut payload = proposal.payload;
    let entity_key = match proposal.proposal_type.as_str() {
        PROPOSAL_NET_TEMPLATE_CREATE | PROPOSAL_NET_TEMPLATE_UPDATE => "net_template_id",
        PROPOSAL_ACTIVATION_CREATE
        | PROPOSAL_ACTIVATION_START
        | PROPOSAL_ACTIVATION_UPDATE
        | PROPOSAL_ACTIVATION_END
        | PROPOSAL_ACTIVATION_CANCEL
        | PROPOSAL_ACTIVATION_NOTE_ADD => "activation_id",
        PROPOSAL_NET_SESSION_START | PROPOSAL_NET_SESSION_END | PROPOSAL_NET_SESSION_CANCEL => {
            "net_session_id"
        }
        PROPOSAL_NET_CHECKIN_CREATE | PROPOSAL_NET_CHECKIN_UPDATE | PROPOSAL_NET_CHECKIN_DELETE => {
            "checkin_id"
        }
        PROPOSAL_NET_TRAFFIC_CREATE | PROPOSAL_NET_TRAFFIC_UPDATE => "traffic_id",
        PROPOSAL_EMCOMM_INCIDENT_OPEN
        | PROPOSAL_EMCOMM_INCIDENT_UPDATE
        | PROPOSAL_EMCOMM_INCIDENT_CLOSE => "incident_id",
        PROPOSAL_EMCOMM_PERIOD_OPEN | PROPOSAL_EMCOMM_PERIOD_CLOSE => "period_id",
        PROPOSAL_EMCOMM_PERSON_CHECK_IN
        | PROPOSAL_EMCOMM_PERSON_UPDATE
        | PROPOSAL_EMCOMM_PERSON_CHECK_OUT => "person_id",
        PROPOSAL_EMCOMM_ASSIGNMENT_CREATE
        | PROPOSAL_EMCOMM_ASSIGNMENT_UPDATE
        | PROPOSAL_EMCOMM_ASSIGNMENT_RELEASE => "assignment_id",
        PROPOSAL_EMCOMM_MESSAGE_CREATE
        | PROPOSAL_EMCOMM_MESSAGE_UPDATE
        | PROPOSAL_EMCOMM_MESSAGE_TRANSMIT
        | PROPOSAL_EMCOMM_MESSAGE_RECEIVE
        | PROPOSAL_EMCOMM_MESSAGE_ACKNOWLEDGE
        | PROPOSAL_EMCOMM_MESSAGE_CANCEL => "message_id",
        PROPOSAL_EMCOMM_ACTIVITY_LOG => "activity_id",
        _ => "qso_id",
    };
    payload[entity_key] = serde_json::json!(entity_id);
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
    B: EventBus + ?Sized,
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

fn require_non_empty_correction(
    payload: &serde_json::Map<String, Value>,
    label: &str,
) -> Result<(), ProposalValidationError> {
    if payload.is_empty() {
        return Err(ProposalValidationError::InvalidSchema(format!(
            "{label} must change at least one field"
        )));
    }
    Ok(())
}

fn validate_message_number(
    payload: &serde_json::Map<String, Value>,
) -> Result<(), ProposalValidationError> {
    let raw = payload
        .get("message_number")
        .and_then(Value::as_str)
        .unwrap_or_default();
    crate::emcomm::MessageNumber::parse(raw).map_err(|_| {
        ProposalValidationError::InvalidSchema(
            "message_number must be a station-scoped number such as `KE8YGW-0007`".to_owned(),
        )
    })?;
    Ok(())
}

fn validate_message_precedence(
    payload: &serde_json::Map<String, Value>,
) -> Result<(), ProposalValidationError> {
    let Some(value) = payload.get("precedence") else {
        return Ok(());
    };
    let precedence = value.as_str().unwrap_or_default().to_ascii_lowercase();
    if matches!(
        precedence.as_str(),
        "emergency" | "priority" | "immediate" | "routine"
    ) {
        Ok(())
    } else {
        Err(ProposalValidationError::InvalidSchema(
            "precedence must be emergency, priority, immediate, or routine".to_owned(),
        ))
    }
}
