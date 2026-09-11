use crate::plugin_sdk::{
    PluginCapability, PluginManifest, ProposalEnvelope, OFFICIAL_LOG_ACTIVATION_STARTED,
    OFFICIAL_LOG_NET_CHECKIN_CREATED, OFFICIAL_LOG_NET_CHECKIN_DELETED,
    OFFICIAL_LOG_NET_REPORT_EXPORTED, OFFICIAL_LOG_NET_SESSION_ENDED,
    OFFICIAL_LOG_NET_SESSION_STARTED, OFFICIAL_LOG_NET_TRAFFIC_CREATED,
    OFFICIAL_LOG_QSO_ACTIVATION_LINKED, OFFICIAL_LOG_QSO_ACTIVATION_UNLINKED,
    OFFICIAL_LOG_QSO_CORRECTED, OFFICIAL_LOG_QSO_CREATED, OFFICIAL_LOG_QSO_DELETED,
    OFFICIAL_LOG_QSO_NOTE_ADDED, OFFICIAL_LOG_QSO_RESTORED, PROPOSAL_ACTIVATION_END,
    PROPOSAL_ACTIVATION_START, PROPOSAL_EMCOMM_ACTIVITY_LOG, PROPOSAL_EMCOMM_ASSIGNMENT_CREATE,
    PROPOSAL_EMCOMM_INCIDENT_CLOSE, PROPOSAL_EMCOMM_INCIDENT_OPEN, PROPOSAL_EMCOMM_INCIDENT_UPDATE,
    PROPOSAL_EMCOMM_MESSAGE_ACKNOWLEDGE, PROPOSAL_EMCOMM_MESSAGE_CREATE,
    PROPOSAL_EMCOMM_MESSAGE_TRANSMIT, PROPOSAL_EMCOMM_MESSAGE_UPDATE, PROPOSAL_EMCOMM_PERIOD_OPEN,
    PROPOSAL_EMCOMM_PERSON_CHECK_IN, PROPOSAL_EMCOMM_PERSON_CHECK_OUT, PROPOSAL_NET_CHECKIN_CREATE,
    PROPOSAL_NET_CHECKIN_DELETE, PROPOSAL_NET_REPORT_EXPORT, PROPOSAL_NET_SESSION_END,
    PROPOSAL_NET_SESSION_START, PROPOSAL_NET_TRAFFIC_CREATE, PROPOSAL_QSO_ACTIVATION_LINK,
    PROPOSAL_QSO_CREATE, PROPOSAL_QSO_DELETE, PROPOSAL_QSO_RESTORE,
};
use chrono::Utc;
use serde_json::json;
use std::{fs, path::PathBuf};
use uuid::Uuid;

use crate::{
    export_net_report_markdown, submit_proposal, BusEvent, EmCommProjection, EventBus, IcsForm,
    InMemoryEventBus, IncidentStatus, MessageNumber, MessagePrecedence, MessageStatus,
    EMCOMM_SCHEMA_VERSION,
};
use crate::{
    ActivationProjection, CoreEventEnvelope, InMemoryLogbookEventStore, LogbookEventStore,
    NetControlProjection, NewLogbookEvent, OperatorRole, PermissionGrantSet, PermissionGrantStatus,
    Projection, ProposalContext, ProposalValidationError, QsoCurrentStateProjection,
};
use crate::{
    ApplicationSettings, APPEARANCE_MODES, DEFAULT_APPEARANCE_MODE, DEFAULT_DESKTOP_SHELL_LAYOUT,
    DEFAULT_MOBILE_DASHBOARD_LAYOUT, DESKTOP_SHELL_LAYOUTS, MOBILE_DASHBOARD_LAYOUTS,
};

fn activation_payload(kind: &str) -> serde_json::Value {
    let mut payload = json!({
        "activation_type": kind,
        "station_callsign": "KE8YGW",
        "operator_callsign": "KE8YGW",
        "started_at": "2026-07-05T12:00:00Z",
        "status": "active",
        "grid": "EN91"
    });
    if kind.eq_ignore_ascii_case("pota") {
        payload["park_id"] = json!("US-1234");
        payload["park_name"] = json!("Test Park");
    }
    if kind.eq_ignore_ascii_case("sota") {
        payload["summit_id"] = json!("W8O/NE-001");
        payload["summit_name"] = json!("Test Summit");
    }
    payload
}

fn activation_context() -> ProposalContext {
    ProposalContext::local_admin(
        plugin_manifest(vec![
            PluginCapability::ActivationCreate,
            PluginCapability::ActivationUpdate,
            PluginCapability::ActivationEnd,
            PluginCapability::ActivationCancel,
            PluginCapability::QsoCreate,
            PluginCapability::QsoCorrect,
            PluginCapability::QsoDelete,
            PluginCapability::QsoRestore,
            PluginCapability::QsoNoteAdd,
            PluginCapability::AdifExport,
        ]),
        OperatorRole::Admin,
    )
}

fn net_context() -> ProposalContext {
    ProposalContext::local_admin(
        plugin_manifest(vec![
            PluginCapability::NetView,
            PluginCapability::NetTemplateCreate,
            PluginCapability::NetTemplateUpdate,
            PluginCapability::NetSessionStart,
            PluginCapability::NetSessionEnd,
            PluginCapability::NetCheckinCreate,
            PluginCapability::NetCheckinUpdate,
            PluginCapability::NetCheckinDelete,
            PluginCapability::NetTrafficManage,
            PluginCapability::NetReportExport,
        ]),
        OperatorRole::Admin,
    )
}

fn net_session_payload() -> serde_json::Value {
    json!({
        "station_callsign": "KE8YGW",
        "net_control_operator_id": Uuid::new_v4().to_string(),
        "net_name": "ARES Weekly Net",
        "started_at": "2026-07-06T00:00:00Z",
        "frequency_hz": 146_940_000_u64,
        "band": "2m",
        "mode": "FM"
    })
}

fn qso_payload() -> serde_json::Value {
    json!({
        "station_callsign": "KE8YGW",
        "operator_callsign": "KE8YGW",
        "contacted_callsign": "K1ABC",
        "started_at": Utc::now().to_rfc3339(),
        "band": "20m",
        "mode": "SSB",
        "rst_sent": "59",
        "rst_received": "59"
    })
}

fn plugin_manifest(capabilities: Vec<PluginCapability>) -> PluginManifest {
    PluginManifest::new("test-plugin", "Test Plugin", "0.1.0", capabilities)
}

fn proposal(proposal_type: &str, entity_id: Option<Uuid>) -> ProposalEnvelope {
    ProposalEnvelope::new(
        proposal_type,
        Uuid::new_v4(),
        entity_id,
        Some(Uuid::new_v4()),
        Uuid::new_v4(),
        "test-plugin",
        1,
        qso_payload(),
    )
}

fn new_log_event(event_type: &str, logbook_id: Uuid, entity_id: Option<Uuid>) -> NewLogbookEvent {
    let device_id = Uuid::new_v4();
    NewLogbookEvent {
        event_type: event_type.to_owned(),
        logbook_id,
        entity_id,
        author_operator_id: None,
        station_callsign: "KE8YGW".to_owned(),
        operator_callsign: Some("KE8YGW".to_owned()),
        author_device_id: device_id,
        source_device_id: device_id,
        correlation_id: Uuid::new_v4(),
        source_plugin_id: None,
        schema_version: 1,
        payload: qso_payload(),
    }
}

fn unique_temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{name}-{}.jsonl", Uuid::new_v4()))
}

fn proposal_for_logbook(
    proposal_type: &str,
    logbook_id: Uuid,
    entity_id: Option<Uuid>,
    payload: serde_json::Value,
) -> ProposalEnvelope {
    ProposalEnvelope::new(
        proposal_type,
        logbook_id,
        entity_id,
        Some(Uuid::new_v4()),
        Uuid::new_v4(),
        "test-plugin",
        1,
        payload,
    )
}

#[tokio::test]
async fn events_append_with_correct_previous_hash() {
    let store = InMemoryLogbookEventStore::new();
    let logbook_id = Uuid::new_v4();

    let first = store
        .append_event(new_log_event(
            OFFICIAL_LOG_QSO_CREATED,
            logbook_id,
            Some(Uuid::new_v4()),
        ))
        .await
        .unwrap();
    let second = store
        .append_event(new_log_event(
            OFFICIAL_LOG_QSO_CREATED,
            logbook_id,
            Some(Uuid::new_v4()),
        ))
        .await
        .unwrap();

    assert_eq!(first.previous_hash, None);
    assert_eq!(second.previous_hash, Some(first.event_hash));
}

#[tokio::test]
async fn official_event_hashing_is_deterministic_and_payload_sensitive() {
    let logbook_id = Uuid::new_v4();
    let event = crate::CoreEventEnvelope::from_new(
        new_log_event(OFFICIAL_LOG_QSO_CREATED, logbook_id, Some(Uuid::new_v4())),
        None,
    );
    let identical = event.clone();
    let mut changed = event.clone();
    changed.payload["contacted_callsign"] = json!("N0DIFF");

    assert_eq!(event.calculate_hash(), identical.calculate_hash());
    assert_ne!(event.calculate_hash(), changed.calculate_hash());
}

#[tokio::test]
async fn chain_verification_passes_for_valid_chains() {
    let store = InMemoryLogbookEventStore::new();
    let logbook_id = Uuid::new_v4();

    for _ in 0..3 {
        store
            .append_event(new_log_event(
                OFFICIAL_LOG_QSO_CREATED,
                logbook_id,
                Some(Uuid::new_v4()),
            ))
            .await
            .unwrap();
    }

    store.verify_chain(logbook_id).await.unwrap();
}

#[tokio::test]
async fn valid_qso_create_proposal_creates_official_event() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let logbook_id = Uuid::new_v4();
    let context = ProposalContext::local_admin(
        plugin_manifest(vec![PluginCapability::QsoCreate]),
        OperatorRole::Logger,
    );

    let outcome = submit_proposal(
        &store,
        &bus,
        &context,
        proposal_for_logbook(PROPOSAL_QSO_CREATE, logbook_id, None, qso_payload()),
    )
    .await
    .unwrap();

    assert_eq!(outcome.official_event.event_type, OFFICIAL_LOG_QSO_CREATED);
    assert!(outcome.official_event.entity_id.is_some());
    assert_eq!(store.list_events(logbook_id).await.unwrap().len(), 1);
}

#[tokio::test]
async fn invalid_qso_create_proposal_is_rejected() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let context = ProposalContext::local_admin(
        plugin_manifest(vec![PluginCapability::QsoCreate]),
        OperatorRole::Logger,
    );
    let mut payload = qso_payload();
    payload
        .as_object_mut()
        .unwrap()
        .remove("contacted_callsign");

    let err = submit_proposal(
        &store,
        &bus,
        &context,
        proposal_for_logbook(PROPOSAL_QSO_CREATE, Uuid::new_v4(), None, payload),
    )
    .await
    .unwrap_err();

    assert!(matches!(err, ProposalValidationError::InvalidSchema(_)));
}

#[tokio::test]
async fn correction_projection_updates_current_qso_state() {
    let store = InMemoryLogbookEventStore::new();
    let logbook_id = Uuid::new_v4();
    let qso_id = Uuid::new_v4();
    store
        .append_event(new_log_event(
            OFFICIAL_LOG_QSO_CREATED,
            logbook_id,
            Some(qso_id),
        ))
        .await
        .unwrap();
    let mut correction = new_log_event(OFFICIAL_LOG_QSO_CORRECTED, logbook_id, Some(qso_id));
    correction.payload = json!({"mode": "CW", "frequency_hz": 14030000_u64});
    store.append_event(correction).await.unwrap();

    let projection = store.rebuild_projections(logbook_id).await.unwrap();
    let record = projection.get(qso_id).unwrap();

    assert_eq!(record.payload["mode"], "CW");
    assert_eq!(record.payload["frequency_hz"], 14030000_u64);
}

#[tokio::test]
async fn restore_makes_tombstoned_qso_visible_again() {
    let store = InMemoryLogbookEventStore::new();
    let logbook_id = Uuid::new_v4();
    let qso_id = Uuid::new_v4();
    store
        .append_event(new_log_event(
            OFFICIAL_LOG_QSO_CREATED,
            logbook_id,
            Some(qso_id),
        ))
        .await
        .unwrap();
    store
        .append_event(new_log_event(
            OFFICIAL_LOG_QSO_DELETED,
            logbook_id,
            Some(qso_id),
        ))
        .await
        .unwrap();
    store
        .append_event(new_log_event(
            OFFICIAL_LOG_QSO_RESTORED,
            logbook_id,
            Some(qso_id),
        ))
        .await
        .unwrap();

    let projection = store.rebuild_projections(logbook_id).await.unwrap();
    assert!(projection.get(qso_id).is_some());
    assert!(!projection.is_tombstoned(qso_id));
}

#[tokio::test]
async fn note_add_preserves_note_history() {
    let store = InMemoryLogbookEventStore::new();
    let logbook_id = Uuid::new_v4();
    let qso_id = Uuid::new_v4();
    store
        .append_event(new_log_event(
            OFFICIAL_LOG_QSO_CREATED,
            logbook_id,
            Some(qso_id),
        ))
        .await
        .unwrap();
    for note in ["first note", "second note"] {
        let mut event = new_log_event(OFFICIAL_LOG_QSO_NOTE_ADDED, logbook_id, Some(qso_id));
        event.payload = json!({"note": note});
        store.append_event(event).await.unwrap();
    }

    let projection = store.rebuild_projections(logbook_id).await.unwrap();
    let record = projection.get(qso_id).unwrap();

    assert_eq!(record.note_history.len(), 2);
    assert_eq!(record.note_history[0]["note"], "first note");
    assert_eq!(record.note_history[1]["note"], "second note");
}

#[tokio::test]
async fn jsonl_storage_reload_rebuilds_projection_and_verifies_chain() {
    let path = unique_temp_file("ham-core-events");
    let logbook_id = Uuid::new_v4();
    let qso_id = Uuid::new_v4();
    {
        let store = crate::JsonlLogbookEventStore::open(&path).unwrap();
        store
            .append_event(new_log_event(
                OFFICIAL_LOG_QSO_CREATED,
                logbook_id,
                Some(qso_id),
            ))
            .await
            .unwrap();
        store.verify_chain(logbook_id).await.unwrap();
    }

    let reloaded = crate::JsonlLogbookEventStore::open(&path).unwrap();
    reloaded.verify_chain(logbook_id).await.unwrap();
    let projection = reloaded.rebuild_projections(logbook_id).await.unwrap();
    assert!(projection.get(qso_id).is_some());

    let _ = fs::remove_file(path);
}

#[tokio::test]
async fn corrupted_jsonl_storage_chain_is_detected() {
    let path = unique_temp_file("ham-core-corrupt-events");
    let logbook_id = Uuid::new_v4();
    {
        let store = crate::JsonlLogbookEventStore::open(&path).unwrap();
        store
            .append_event(new_log_event(
                OFFICIAL_LOG_QSO_CREATED,
                logbook_id,
                Some(Uuid::new_v4()),
            ))
            .await
            .unwrap();
    }
    let mut line = fs::read_to_string(&path).unwrap();
    line = line.replace("K1ABC", "N0BAD");
    fs::write(&path, line).unwrap();

    let reloaded = crate::JsonlLogbookEventStore::open(&path).unwrap();
    assert!(reloaded.verify_chain(logbook_id).await.is_err());

    let _ = fs::remove_file(path);
}

#[tokio::test]
async fn tampering_breaks_verification() {
    let store = InMemoryLogbookEventStore::new();
    let logbook_id = Uuid::new_v4();
    let event = store
        .append_event(new_log_event(
            OFFICIAL_LOG_QSO_CREATED,
            logbook_id,
            Some(Uuid::new_v4()),
        ))
        .await
        .unwrap();

    let mut tampered = event;
    tampered.payload["contacted_callsign"] = json!("N0BAD");
    store.replace_event_for_testing(tampered).await;

    assert!(store.verify_chain(logbook_id).await.is_err());
}

#[tokio::test]
async fn qso_deleted_hides_projection_without_removing_event() {
    let store = InMemoryLogbookEventStore::new();
    let logbook_id = Uuid::new_v4();
    let qso_id = Uuid::new_v4();

    store
        .append_event(new_log_event(
            OFFICIAL_LOG_QSO_CREATED,
            logbook_id,
            Some(qso_id),
        ))
        .await
        .unwrap();
    let mut delete_event = new_log_event(
        crate::plugin_sdk::OFFICIAL_LOG_QSO_DELETED,
        logbook_id,
        Some(qso_id),
    );
    delete_event.payload = json!({"reason": "duplicate"});
    store.append_event(delete_event).await.unwrap();

    let events = store.list_events_after(logbook_id, None).await.unwrap();
    let mut projection = QsoCurrentStateProjection::new();
    projection.rebuild(&events).unwrap();

    assert_eq!(events.len(), 2);
    assert!(projection.get(qso_id).is_none());
    assert!(projection.is_tombstoned(qso_id));
}

#[tokio::test]
async fn plugin_proposals_are_rejected_without_required_capability() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let context = ProposalContext::local_admin(plugin_manifest(vec![]), OperatorRole::Logger);

    let err = submit_proposal(&store, &bus, &context, proposal(PROPOSAL_QSO_CREATE, None))
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        ProposalValidationError::MissingPluginCapability(PluginCapability::QsoCreate)
    ));
}

#[tokio::test]
async fn qso_create_denied_when_plugin_permission_not_granted() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let context = ProposalContext {
        plugin_manifest: plugin_manifest(vec![PluginCapability::QsoCreate]),
        operator_role: OperatorRole::Logger,
        permission_grants: PermissionGrantSet::default(),
    };

    let err = submit_proposal(&store, &bus, &context, proposal(PROPOSAL_QSO_CREATE, None))
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        ProposalValidationError::PluginPermissionDenied(_)
    ));
}

#[tokio::test]
async fn qso_create_allowed_only_when_plugin_and_role_allow() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let context = ProposalContext::local_admin(
        plugin_manifest(vec![PluginCapability::QsoCreate]),
        OperatorRole::Logger,
    );

    let outcome = submit_proposal(&store, &bus, &context, proposal(PROPOSAL_QSO_CREATE, None))
        .await
        .unwrap();

    assert_eq!(outcome.official_event.event_type, OFFICIAL_LOG_QSO_CREATED);
}

#[tokio::test]
async fn runtime_event_is_published_for_denied_permission() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let mut receiver = bus.subscribe();
    let mut grants = PermissionGrantSet::default();
    grants.set_status(
        "test-plugin",
        PluginCapability::QsoCreate,
        PermissionGrantStatus::Denied,
        Some("test deny".to_owned()),
    );
    let context = ProposalContext {
        plugin_manifest: plugin_manifest(vec![PluginCapability::QsoCreate]),
        operator_role: OperatorRole::Logger,
        permission_grants: grants,
    };

    let _ = submit_proposal(&store, &bus, &context, proposal(PROPOSAL_QSO_CREATE, None)).await;
    let mut found = false;
    for _ in 0..8 {
        if let BusEvent::Runtime(event) = receiver.recv().await.unwrap() {
            if event.event_type == "plugin.permission.check.denied" {
                found = true;
                break;
            }
        }
    }
    assert!(found);
}

#[tokio::test]
async fn accepted_proposals_publish_an_event_on_the_event_bus() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let mut receiver = bus.subscribe();
    let context = ProposalContext::local_admin(
        plugin_manifest(vec![PluginCapability::QsoCreate]),
        OperatorRole::Logger,
    );

    let outcome = submit_proposal(&store, &bus, &context, proposal(PROPOSAL_QSO_CREATE, None))
        .await
        .unwrap();
    let mut published_official = None;
    for _ in 0..8 {
        if let BusEvent::OfficialLogbookEvent(event) = receiver.recv().await.unwrap() {
            published_official = Some(event);
            break;
        }
    }

    assert_eq!(outcome.official_event.event_type, OFFICIAL_LOG_QSO_CREATED);
    assert_eq!(
        published_official.map(|event| event.event_id),
        Some(outcome.official_event.event_id)
    );
}

#[tokio::test]
async fn qso_delete_requires_admin_role() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let context = ProposalContext::local_admin(
        plugin_manifest(vec![PluginCapability::QsoDelete]),
        OperatorRole::Logger,
    );

    let err = submit_proposal(
        &store,
        &bus,
        &context,
        proposal(PROPOSAL_QSO_DELETE, Some(Uuid::new_v4())),
    )
    .await
    .unwrap_err();

    assert!(matches!(
        err,
        ProposalValidationError::PermissionDenied { .. }
    ));
}

#[tokio::test]
async fn pota_activation_requires_park_id() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let mut payload = activation_payload("pota");
    payload.as_object_mut().unwrap().remove("park_id");
    let err = submit_proposal(
        &store,
        &bus,
        &activation_context(),
        proposal_for_logbook(PROPOSAL_ACTIVATION_START, Uuid::new_v4(), None, payload),
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("park_id"));
}

#[tokio::test]
async fn sota_activation_requires_summit_id() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let mut payload = activation_payload("sota");
    payload.as_object_mut().unwrap().remove("summit_id");
    let err = submit_proposal(
        &store,
        &bus,
        &activation_context(),
        proposal_for_logbook(PROPOSAL_ACTIVATION_START, Uuid::new_v4(), None, payload),
    )
    .await
    .unwrap_err();

    assert!(err.to_string().contains("summit_id"));
}

#[tokio::test]
async fn activation_start_end_lifecycle_and_projection_rebuild() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let logbook_id = Uuid::new_v4();
    let start = submit_proposal(
        &store,
        &bus,
        &activation_context(),
        proposal_for_logbook(
            PROPOSAL_ACTIVATION_START,
            logbook_id,
            None,
            activation_payload("pota"),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        start.official_event.event_type,
        OFFICIAL_LOG_ACTIVATION_STARTED
    );
    let activation_id = start.official_event.entity_id.unwrap();

    submit_proposal(
        &store,
        &bus,
        &activation_context(),
        proposal_for_logbook(
            PROPOSAL_ACTIVATION_END,
            logbook_id,
            Some(activation_id),
            json!({
                "started_at": "2026-07-05T12:00:00Z",
                "ended_at": "2026-07-05T13:00:00Z"
            }),
        ),
    )
    .await
    .unwrap();

    let projection = store
        .rebuild_activation_projections(logbook_id)
        .await
        .unwrap();
    assert_eq!(projection.get(activation_id).unwrap().status, "ended");
}

#[tokio::test]
async fn activation_end_requires_ended_after_started() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let logbook_id = Uuid::new_v4();
    let start = submit_proposal(
        &store,
        &bus,
        &activation_context(),
        proposal_for_logbook(
            PROPOSAL_ACTIVATION_START,
            logbook_id,
            None,
            activation_payload("pota"),
        ),
    )
    .await
    .unwrap();
    let err = submit_proposal(
        &store,
        &bus,
        &activation_context(),
        proposal_for_logbook(
            PROPOSAL_ACTIVATION_END,
            logbook_id,
            start.official_event.entity_id,
            json!({
                "started_at": "2026-07-05T12:00:00Z",
                "ended_at": "2026-07-05T11:59:00Z"
            }),
        ),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("ended_at"));
}

#[tokio::test]
async fn qso_linking_updates_activation_projection_counts_and_delete_restore() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let logbook_id = Uuid::new_v4();
    let activation = submit_proposal(
        &store,
        &bus,
        &activation_context(),
        proposal_for_logbook(
            PROPOSAL_ACTIVATION_START,
            logbook_id,
            None,
            activation_payload("pota"),
        ),
    )
    .await
    .unwrap();
    let activation_id = activation.official_event.entity_id.unwrap();
    let qso = submit_proposal(
        &store,
        &bus,
        &activation_context(),
        proposal_for_logbook(PROPOSAL_QSO_CREATE, logbook_id, None, qso_payload()),
    )
    .await
    .unwrap();
    let qso_id = qso.official_event.entity_id.unwrap();

    let link = submit_proposal(
        &store,
        &bus,
        &activation_context(),
        proposal_for_logbook(
            PROPOSAL_QSO_ACTIVATION_LINK,
            logbook_id,
            Some(qso_id),
            json!({"activation_id": activation_id}),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        link.official_event.event_type,
        OFFICIAL_LOG_QSO_ACTIVATION_LINKED
    );

    let projection = store
        .rebuild_activation_projections(logbook_id)
        .await
        .unwrap();
    assert_eq!(projection.get(activation_id).unwrap().qso_count, 1);
    assert_eq!(
        projection.get(activation_id).unwrap().unique_callsign_count,
        1
    );

    submit_proposal(
        &store,
        &bus,
        &activation_context(),
        proposal_for_logbook(PROPOSAL_QSO_DELETE, logbook_id, Some(qso_id), json!({})),
    )
    .await
    .unwrap();
    let projection = store
        .rebuild_activation_projections(logbook_id)
        .await
        .unwrap();
    assert_eq!(projection.get(activation_id).unwrap().qso_count, 0);

    submit_proposal(
        &store,
        &bus,
        &activation_context(),
        proposal_for_logbook(PROPOSAL_QSO_RESTORE, logbook_id, Some(qso_id), json!({})),
    )
    .await
    .unwrap();
    let projection = store
        .rebuild_activation_projections(logbook_id)
        .await
        .unwrap();
    assert_eq!(projection.get(activation_id).unwrap().qso_count, 1);
}

#[tokio::test]
async fn activation_adif_export_includes_pota_and_sota_fields() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let logbook_id = Uuid::new_v4();
    for kind in ["pota", "sota"] {
        let activation = submit_proposal(
            &store,
            &bus,
            &activation_context(),
            proposal_for_logbook(
                PROPOSAL_ACTIVATION_START,
                logbook_id,
                None,
                activation_payload(kind),
            ),
        )
        .await
        .unwrap();
        let activation_id = activation.official_event.entity_id.unwrap();
        let qso = submit_proposal(
            &store,
            &bus,
            &activation_context(),
            proposal_for_logbook(PROPOSAL_QSO_CREATE, logbook_id, None, qso_payload()),
        )
        .await
        .unwrap();
        submit_proposal(
            &store,
            &bus,
            &activation_context(),
            proposal_for_logbook(
                PROPOSAL_QSO_ACTIVATION_LINK,
                logbook_id,
                qso.official_event.entity_id,
                json!({"activation_id": activation_id}),
            ),
        )
        .await
        .unwrap();
    }
    let qsos = store.rebuild_projections(logbook_id).await.unwrap();
    let activations = store
        .rebuild_activation_projections(logbook_id)
        .await
        .unwrap();
    let adif = crate::export_adif_with_activations(&qsos, Some(&activations), false);
    assert!(adif.contains("<MY_SIG:4>POTA"));
    assert!(adif.contains("<MY_SIG_INFO:7>US-1234"));
    assert!(adif.contains("<MY_SIG:4>SOTA"));
    assert!(adif.contains("<MY_SIG_INFO:10>W8O/NE-001"));
}

#[tokio::test]
async fn net_session_start_end_lifecycle() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let logbook_id = Uuid::new_v4();
    let started = submit_proposal(
        &store,
        &bus,
        &net_context(),
        proposal_for_logbook(
            PROPOSAL_NET_SESSION_START,
            logbook_id,
            None,
            net_session_payload(),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        started.official_event.event_type,
        OFFICIAL_LOG_NET_SESSION_STARTED
    );
    let session_id = started.official_event.entity_id.unwrap();

    let ended = submit_proposal(
        &store,
        &bus,
        &net_context(),
        proposal_for_logbook(
            PROPOSAL_NET_SESSION_END,
            logbook_id,
            Some(session_id),
            json!({
                "started_at": "2026-07-06T00:00:00Z",
                "ended_at": "2026-07-06T01:00:00Z"
            }),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        ended.official_event.event_type,
        OFFICIAL_LOG_NET_SESSION_ENDED
    );
}

#[tokio::test]
async fn net_checkin_requires_active_net() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let err = submit_proposal(
        &store,
        &bus,
        &net_context(),
        proposal_for_logbook(
            PROPOSAL_NET_CHECKIN_CREATE,
            Uuid::new_v4(),
            None,
            json!({
                "net_session_id": Uuid::new_v4(),
                "callsign": "K1ABC",
                "checkin_time": "2026-07-06T00:01:00Z"
            }),
        ),
    )
    .await
    .unwrap_err();
    assert!(err.to_string().contains("does not exist"));
}

#[tokio::test]
async fn net_projection_duplicate_late_emergency_and_tombstone_behavior() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let logbook_id = Uuid::new_v4();
    let session = submit_proposal(
        &store,
        &bus,
        &net_context(),
        proposal_for_logbook(
            PROPOSAL_NET_SESSION_START,
            logbook_id,
            None,
            net_session_payload(),
        ),
    )
    .await
    .unwrap();
    let session_id = session.official_event.entity_id.unwrap();
    let mut checkin_ids = Vec::new();
    for status in ["checked_in", "late"] {
        let checkin = submit_proposal(
            &store,
            &bus,
            &net_context(),
            proposal_for_logbook(
                PROPOSAL_NET_CHECKIN_CREATE,
                logbook_id,
                None,
                json!({
                    "net_session_id": session_id,
                    "callsign": "K1ABC",
                    "checkin_time": "2026-07-06T00:01:00Z",
                    "status": status,
                    "traffic": "listed"
                }),
            ),
        )
        .await
        .unwrap();
        assert_eq!(
            checkin.official_event.event_type,
            OFFICIAL_LOG_NET_CHECKIN_CREATED
        );
        checkin_ids.push(checkin.official_event.entity_id.unwrap());
    }
    let traffic = submit_proposal(
        &store,
        &bus,
        &net_context(),
        proposal_for_logbook(
            PROPOSAL_NET_TRAFFIC_CREATE,
            logbook_id,
            None,
            json!({
                "net_session_id": session_id,
                "from_callsign": "K1ABC",
                "precedence": "emergency",
                "summary": "Emergency traffic test",
                "status": "listed"
            }),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        traffic.official_event.event_type,
        OFFICIAL_LOG_NET_TRAFFIC_CREATED
    );

    let events = store.list_events(logbook_id).await.unwrap();
    let mut projection = NetControlProjection::new();
    projection.rebuild(&events).unwrap();
    let projected = projection.get_session(session_id).unwrap();
    assert_eq!(projected.checkin_count, 2);
    assert_eq!(projected.late_checkin_count, 1);
    assert_eq!(projected.emergency_traffic_count, 1);
    assert_eq!(projected.duplicate_warnings.len(), 1);

    let deleted = submit_proposal(
        &store,
        &bus,
        &net_context(),
        proposal_for_logbook(
            PROPOSAL_NET_CHECKIN_DELETE,
            logbook_id,
            Some(checkin_ids[0]),
            json!({"net_session_id": session_id, "reason": "duplicate"}),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        deleted.official_event.event_type,
        OFFICIAL_LOG_NET_CHECKIN_DELETED
    );

    let events = store.list_events(logbook_id).await.unwrap();
    let mut projection = NetControlProjection::new();
    projection.rebuild(&events).unwrap();
    assert_eq!(projection.checkins_for_session(session_id, false).len(), 1);
    assert_eq!(projection.checkins_for_session(session_id, true).len(), 2);
}

#[tokio::test]
async fn net_report_export_appends_event_and_report_contains_summary() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let logbook_id = Uuid::new_v4();
    let session = submit_proposal(
        &store,
        &bus,
        &net_context(),
        proposal_for_logbook(
            PROPOSAL_NET_SESSION_START,
            logbook_id,
            None,
            net_session_payload(),
        ),
    )
    .await
    .unwrap();
    let session_id = session.official_event.entity_id.unwrap();
    submit_proposal(
        &store,
        &bus,
        &net_context(),
        proposal_for_logbook(
            PROPOSAL_NET_CHECKIN_CREATE,
            logbook_id,
            None,
            json!({
                "net_session_id": session_id,
                "callsign": "K1ABC",
                "checkin_time": "2026-07-06T00:01:00Z"
            }),
        ),
    )
    .await
    .unwrap();
    let events = store.list_events(logbook_id).await.unwrap();
    let mut projection = NetControlProjection::new();
    projection.rebuild(&events).unwrap();
    let report = export_net_report_markdown(&projection, session_id).unwrap();
    assert!(report.contains("ARES Weekly Net"));
    assert!(report.contains("K1ABC"));

    let exported = submit_proposal(
        &store,
        &bus,
        &net_context(),
        proposal_for_logbook(
            PROPOSAL_NET_REPORT_EXPORT,
            logbook_id,
            Some(session_id),
            json!({"format": "markdown", "summary": report}),
        ),
    )
    .await
    .unwrap();
    assert_eq!(
        exported.official_event.event_type,
        OFFICIAL_LOG_NET_REPORT_EXPORTED
    );
}

#[tokio::test]
async fn net_permission_denial_blocks_checkin() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let context = ProposalContext::local_admin(
        plugin_manifest(vec![PluginCapability::NetSessionStart]),
        OperatorRole::Admin,
    );
    let err = submit_proposal(
        &store,
        &bus,
        &context,
        proposal_for_logbook(
            PROPOSAL_NET_CHECKIN_CREATE,
            Uuid::new_v4(),
            None,
            json!({
                "net_session_id": Uuid::new_v4(),
                "callsign": "K1ABC",
                "checkin_time": "2026-07-06T00:01:00Z"
            }),
        ),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        ProposalValidationError::MissingPluginCapability(PluginCapability::NetCheckinCreate)
    ));
}

/// Independently derives the counters `ActivationProjection` maintains, so the
/// incremental recompute can be checked against a full recomputation.
fn expected_activation_counters(
    linked_qsos: &std::collections::HashSet<Uuid>,
    qsos: &QsoCurrentStateProjection,
) -> (
    usize,
    usize,
    std::collections::HashMap<String, usize>,
    std::collections::HashMap<String, usize>,
) {
    let mut callsigns = std::collections::HashSet::new();
    let mut bands: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut modes: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut count = 0usize;
    for qso_id in linked_qsos {
        let Some(qso) = qsos.get(*qso_id) else {
            continue;
        };
        count += 1;
        if let Some(callsign) = qso
            .payload
            .get("contacted_callsign")
            .and_then(serde_json::Value::as_str)
        {
            callsigns.insert(callsign.to_ascii_uppercase());
        }
        if let Some(band) = qso.payload.get("band").and_then(serde_json::Value::as_str) {
            *bands.entry(band.to_owned()).or_insert(0) += 1;
        }
        if let Some(mode) = qso.payload.get("mode").and_then(serde_json::Value::as_str) {
            *modes.entry(mode.to_owned()).or_insert(0) += 1;
        }
    }
    (count, callsigns.len(), bands, modes)
}

/// `ActivationProjection` only recomputes the activations an event touches.
/// Replaying a stream that exercises every branch must still leave every
/// activation's counters equal to a full recomputation over its links.
#[tokio::test]
async fn activation_projection_incremental_counters_match_full_recompute() {
    let logbook_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();
    let mut previous_hash: Option<String> = None;
    let mut events: Vec<CoreEventEnvelope> = Vec::new();

    let append = |event_type: &str,
                  entity_id: Option<Uuid>,
                  payload: serde_json::Value,
                  previous_hash: &mut Option<String>,
                  events: &mut Vec<CoreEventEnvelope>| {
        let event = CoreEventEnvelope::from_new(
            NewLogbookEvent {
                event_type: event_type.to_owned(),
                logbook_id,
                entity_id,
                author_operator_id: None,
                station_callsign: "KE8YGW".to_owned(),
                operator_callsign: Some("KE8YGW".to_owned()),
                author_device_id: device_id,
                source_device_id: device_id,
                correlation_id: Uuid::new_v4(),
                source_plugin_id: None,
                schema_version: 1,
                payload,
            },
            previous_hash.clone(),
        );
        *previous_hash = Some(event.event_hash.clone());
        let entity_id = event.entity_id;
        events.push(event);
        entity_id
    };

    let activation_a = Uuid::new_v4();
    let activation_b = Uuid::new_v4();
    append(
        OFFICIAL_LOG_ACTIVATION_STARTED,
        Some(activation_a),
        activation_payload("pota"),
        &mut previous_hash,
        &mut events,
    );
    append(
        OFFICIAL_LOG_ACTIVATION_STARTED,
        Some(activation_b),
        activation_payload("pota"),
        &mut previous_hash,
        &mut events,
    );

    let qso_ids: Vec<Uuid> = (0..6).map(|_| Uuid::new_v4()).collect();
    for (index, qso_id) in qso_ids.iter().enumerate() {
        append(
            OFFICIAL_LOG_QSO_CREATED,
            Some(*qso_id),
            json!({
                "contacted_callsign": format!("W1AW/{}", index % 3),
                "station_callsign": "KE8YGW",
                "mode": if index % 2 == 0 { "SSB" } else { "CW" },
                "band": if index % 3 == 0 { "20m" } else { "40m" },
                "started_at": "2026-07-06T00:00:00Z",
            }),
            &mut previous_hash,
            &mut events,
        );
        let activation_id = if index % 2 == 0 {
            activation_a
        } else {
            activation_b
        };
        append(
            OFFICIAL_LOG_QSO_ACTIVATION_LINKED,
            Some(*qso_id),
            json!({ "activation_id": activation_id.to_string() }),
            &mut previous_hash,
            &mut events,
        );
    }

    // Exercise every branch that can invalidate an activation's counters.
    append(
        OFFICIAL_LOG_QSO_CORRECTED,
        Some(qso_ids[0]),
        json!({ "band": "15m", "contacted_callsign": "VE3ABC" }),
        &mut previous_hash,
        &mut events,
    );
    append(
        OFFICIAL_LOG_QSO_DELETED,
        Some(qso_ids[2]),
        json!({ "reason": "duplicate" }),
        &mut previous_hash,
        &mut events,
    );
    append(
        OFFICIAL_LOG_QSO_RESTORED,
        Some(qso_ids[2]),
        json!({ "reason": "operator restore" }),
        &mut previous_hash,
        &mut events,
    );
    append(
        OFFICIAL_LOG_QSO_DELETED,
        Some(qso_ids[4]),
        json!({ "reason": "bust" }),
        &mut previous_hash,
        &mut events,
    );
    append(
        OFFICIAL_LOG_QSO_ACTIVATION_UNLINKED,
        Some(qso_ids[1]),
        json!({ "activation_id": activation_b.to_string() }),
        &mut previous_hash,
        &mut events,
    );
    append(
        OFFICIAL_LOG_QSO_NOTE_ADDED,
        Some(qso_ids[3]),
        json!({ "note": "thanks for the contact" }),
        &mut previous_hash,
        &mut events,
    );
    // Re-creating an activation drops its links; stale index entries must not
    // resurrect counters.
    append(
        OFFICIAL_LOG_ACTIVATION_STARTED,
        Some(activation_a),
        activation_payload("pota"),
        &mut previous_hash,
        &mut events,
    );
    append(
        OFFICIAL_LOG_QSO_ACTIVATION_LINKED,
        Some(qso_ids[5]),
        json!({ "activation_id": activation_a.to_string() }),
        &mut previous_hash,
        &mut events,
    );

    let mut activations = ActivationProjection::new();
    activations.rebuild(&events).unwrap();
    let mut qsos = QsoCurrentStateProjection::new();
    qsos.rebuild(&events).unwrap();

    for activation_id in [activation_a, activation_b] {
        let record = activations
            .get(activation_id)
            .expect("activation projected");
        let (qso_count, unique_callsigns, bands, modes) =
            expected_activation_counters(&record.linked_qsos, &qsos);
        assert_eq!(
            record.qso_count, qso_count,
            "qso_count drifted for {activation_id}"
        );
        assert_eq!(
            record.unique_callsign_count, unique_callsigns,
            "unique_callsign_count drifted for {activation_id}"
        );
        assert_eq!(
            record.band_summary, bands,
            "band_summary drifted for {activation_id}"
        );
        assert_eq!(
            record.mode_summary, modes,
            "mode_summary drifted for {activation_id}"
        );
    }

    // The re-created activation kept only the link appended after it was recreated.
    let recreated = activations.get(activation_a).expect("activation projected");
    assert_eq!(recreated.linked_qsos.len(), 1);
    assert!(recreated.linked_qsos.contains(&qso_ids[5]));
    assert_eq!(
        activations.activations_for_qso(qso_ids[0]),
        Vec::<Uuid>::new()
    );
    // qso_ids[5] was linked to activation_b in the loop and to activation_a
    // after the re-create, so it reports both owners.
    let mut owners = activations.activations_for_qso(qso_ids[5]);
    owners.sort();
    let mut expected_owners = vec![activation_a, activation_b];
    expected_owners.sort();
    assert_eq!(owners, expected_owners);
    // The unlinked QSO no longer reports an owning activation.
    assert_eq!(
        activations.activations_for_qso(qso_ids[1]),
        Vec::<Uuid>::new()
    );
}

fn emcomm_context() -> ProposalContext {
    ProposalContext::local_admin(
        plugin_manifest(vec![
            PluginCapability::EmCommView,
            PluginCapability::EmCommIncidentManage,
            PluginCapability::EmCommPeriodManage,
            PluginCapability::EmCommPersonManage,
            PluginCapability::EmCommAssignmentManage,
            PluginCapability::EmCommMessageManage,
            PluginCapability::EmCommActivityLog,
        ]),
        OperatorRole::Admin,
    )
}

fn emcomm_proposal(
    proposal_type: &str,
    logbook_id: Uuid,
    entity_id: Option<Uuid>,
    payload: serde_json::Value,
) -> ProposalEnvelope {
    ProposalEnvelope::new(
        proposal_type,
        logbook_id,
        entity_id,
        Some(Uuid::new_v4()),
        Uuid::new_v4(),
        "test-plugin",
        1,
        payload,
    )
}

async fn open_test_incident(
    store: &InMemoryLogbookEventStore,
    bus: &InMemoryEventBus,
    logbook_id: Uuid,
) -> Uuid {
    let outcome = submit_proposal(
        store,
        bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_INCIDENT_OPEN,
            logbook_id,
            None,
            json!({
                "station_callsign": "KE8YGW",
                "incident_name": "County Exercise",
                "incident_number": "2026-EX-01",
                "opened_at": "2026-11-24T12:00:00Z"
            }),
        ),
    )
    .await
    .expect("incident opens");
    outcome.official_event.entity_id.expect("incident id")
}

#[tokio::test]
async fn emcomm_incident_lifecycle_projects_from_official_events_only() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::new(64);
    let logbook_id = Uuid::new_v4();

    let incident_id = open_test_incident(&store, &bus, logbook_id).await;

    let period = submit_proposal(
        &store,
        &bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_PERIOD_OPEN,
            logbook_id,
            None,
            json!({
                "station_callsign": "KE8YGW",
                "incident_id": incident_id.to_string(),
                "period_number": 1,
                "started_at": "2026-11-24T12:05:00Z"
            }),
        ),
    )
    .await
    .expect("period opens");
    let period_id = period.official_event.entity_id.expect("period id");

    let person = submit_proposal(
        &store,
        &bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_PERSON_CHECK_IN,
            logbook_id,
            None,
            json!({
                "station_callsign": "KE8YGW",
                "incident_id": incident_id.to_string(),
                "period_id": period_id.to_string(),
                "name": "A. Operator",
                "callsign": "KE8YGW",
                "ics_position": "Radio Operator",
                "checked_in_at": "2026-11-24T12:10:00Z"
            }),
        ),
    )
    .await
    .expect("check-in appends");
    let person_id = person.official_event.entity_id.expect("person id");

    submit_proposal(
        &store,
        &bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_ASSIGNMENT_CREATE,
            logbook_id,
            None,
            json!({
                "station_callsign": "KE8YGW",
                "incident_id": incident_id.to_string(),
                "person_id": person_id.to_string(),
                "assignment": "Net Control, primary repeater"
            }),
        ),
    )
    .await
    .expect("assignment appends");

    let mut projection = EmCommProjection::new();
    let events = store.list_events(logbook_id).await.expect("events");
    projection
        .rebuild(events.iter())
        .expect("projection rebuilds");

    let incident = projection.incident(incident_id).expect("incident");
    assert_eq!(incident.record.status, IncidentStatus::Open);
    assert_eq!(incident.record.text("incident_number"), Some("2026-EX-01"));
    assert_eq!(projection.periods_for_incident(incident_id).len(), 1);
    assert_eq!(
        projection
            .open_period(incident_id)
            .map(|period| period.period_id),
        Some(period_id)
    );
    assert_eq!(projection.people_for_incident(incident_id, false).len(), 1);
    assert_eq!(projection.assignments_for_person(person_id).len(), 1);
}

#[tokio::test]
async fn emcomm_corrections_append_history_and_never_rewrite_it() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::new(64);
    let logbook_id = Uuid::new_v4();
    let incident_id = open_test_incident(&store, &bus, logbook_id).await;

    submit_proposal(
        &store,
        &bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_INCIDENT_UPDATE,
            logbook_id,
            Some(incident_id),
            json!({
                "station_callsign": "KE8YGW",
                "incident_name": "County Exercise (corrected)"
            }),
        ),
    )
    .await
    .expect("correction appends");

    let mut projection = EmCommProjection::new();
    let events = store.list_events(logbook_id).await.expect("events");
    projection
        .rebuild(events.iter())
        .expect("projection rebuilds");

    let incident = projection.incident(incident_id).expect("incident");
    assert_eq!(
        incident.record.text("incident_name"),
        Some("County Exercise (corrected)")
    );
    assert_eq!(incident.record.history().len(), 2);
    assert_eq!(
        incident.record.history()[0].payload["incident_name"],
        json!("County Exercise")
    );
    assert_eq!(incident.record.status, IncidentStatus::Open);

    let error = submit_proposal(
        &store,
        &bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_INCIDENT_UPDATE,
            logbook_id,
            Some(incident_id),
            json!({}),
        ),
    )
    .await
    .expect_err("an empty correction is rejected");
    assert!(matches!(error, ProposalValidationError::InvalidSchema(_)));
}

#[tokio::test]
async fn emcomm_message_lifecycle_keeps_every_delivery_state() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::new(64);
    let logbook_id = Uuid::new_v4();
    let incident_id = open_test_incident(&store, &bus, logbook_id).await;

    let message = submit_proposal(
        &store,
        &bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_MESSAGE_CREATE,
            logbook_id,
            None,
            json!({
                "station_callsign": "KE8YGW",
                "incident_id": incident_id.to_string(),
                "message_number": "KE8YGW-0001",
                "form": "ICS-213",
                "precedence": "priority",
                "from": "Net Control",
                "to": "Shelter 1",
                "body": "Report current occupancy."
            }),
        ),
    )
    .await
    .expect("message drafts");
    let message_id = message.official_event.entity_id.expect("message id");

    for (proposal_type, payload) in [
        (
            PROPOSAL_EMCOMM_MESSAGE_TRANSMIT,
            json!({ "station_callsign": "KE8YGW", "transmitted_at": "2026-11-24T12:20:00Z" }),
        ),
        (
            PROPOSAL_EMCOMM_MESSAGE_ACKNOWLEDGE,
            json!({ "station_callsign": "KE8YGW", "acknowledged_at": "2026-11-24T12:22:00Z" }),
        ),
    ] {
        submit_proposal(
            &store,
            &bus,
            &emcomm_context(),
            emcomm_proposal(proposal_type, logbook_id, Some(message_id), payload),
        )
        .await
        .expect("delivery state appends");
    }

    let mut projection = EmCommProjection::new();
    let events = store.list_events(logbook_id).await.expect("events");
    projection
        .rebuild(events.iter())
        .expect("projection rebuilds");

    let message = projection.message(message_id).expect("message");
    assert_eq!(message.record.status, MessageStatus::Acknowledged);
    assert_eq!(message.precedence, MessagePrecedence::Priority);
    assert_eq!(message.form, IcsForm::Ics213);
    assert_eq!(message.record.history().len(), 3);
    assert_eq!(
        message.record.history()[1].payload["transmitted_at"],
        json!("2026-11-24T12:20:00Z")
    );
    assert!(projection.unacknowledged_messages(incident_id).is_empty());
}

#[tokio::test]
async fn emcomm_message_numbers_cannot_be_reassigned_or_malformed() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::new(64);
    let logbook_id = Uuid::new_v4();
    let incident_id = open_test_incident(&store, &bus, logbook_id).await;

    let error = submit_proposal(
        &store,
        &bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_MESSAGE_CREATE,
            logbook_id,
            None,
            json!({
                "station_callsign": "KE8YGW",
                "incident_id": incident_id.to_string(),
                "message_number": "7",
                "from": "Net Control",
                "to": "Shelter 1",
                "body": "Report current occupancy."
            }),
        ),
    )
    .await
    .expect_err("an unscoped message number is rejected");
    assert!(matches!(error, ProposalValidationError::InvalidSchema(_)));

    let message = submit_proposal(
        &store,
        &bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_MESSAGE_CREATE,
            logbook_id,
            None,
            json!({
                "station_callsign": "KE8YGW",
                "incident_id": incident_id.to_string(),
                "message_number": "KE8YGW-0001",
                "from": "Net Control",
                "to": "Shelter 1",
                "body": "Report current occupancy."
            }),
        ),
    )
    .await
    .expect("message drafts");
    let message_id = message.official_event.entity_id.expect("message id");

    let error = submit_proposal(
        &store,
        &bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_MESSAGE_UPDATE,
            logbook_id,
            Some(message_id),
            json!({ "station_callsign": "KE8YGW", "message_number": "KE8YGW-0002" }),
        ),
    )
    .await
    .expect_err("a message number is assigned once");
    assert!(matches!(error, ProposalValidationError::InvalidSchema(_)));
}

#[tokio::test]
async fn emcomm_offline_message_numbers_are_scoped_to_the_originating_station() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::new(64);
    let logbook_id = Uuid::new_v4();
    let incident_id = open_test_incident(&store, &bus, logbook_id).await;

    for number in ["KE8YGW-0001", "W8ABC-0001", "KE8YGW-0002"] {
        submit_proposal(
            &store,
            &bus,
            &emcomm_context(),
            emcomm_proposal(
                PROPOSAL_EMCOMM_MESSAGE_CREATE,
                logbook_id,
                None,
                json!({
                    "station_callsign": "KE8YGW",
                    "incident_id": incident_id.to_string(),
                    "message_number": number,
                    "from": "Net Control",
                    "to": "Shelter 1",
                    "body": "Traffic."
                }),
            ),
        )
        .await
        .expect("message drafts");
    }

    let mut projection = EmCommProjection::new();
    let events = store.list_events(logbook_id).await.expect("events");
    projection
        .rebuild(events.iter())
        .expect("projection rebuilds");

    assert_eq!(projection.messages_for_incident(incident_id).len(), 3);
    assert_eq!(
        projection
            .next_message_number(incident_id, "KE8YGW")
            .to_string(),
        "KE8YGW-0003"
    );
    assert_eq!(
        projection
            .next_message_number(incident_id, "W8ABC")
            .to_string(),
        "W8ABC-0002"
    );
    assert_eq!(
        projection
            .next_message_number(incident_id, "N0CALL")
            .to_string(),
        "N0CALL-0001"
    );
}

#[tokio::test]
async fn emcomm_messages_sort_by_precedence_and_expose_unacknowledged_traffic() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::new(64);
    let logbook_id = Uuid::new_v4();
    let incident_id = open_test_incident(&store, &bus, logbook_id).await;

    let mut transmitted = Vec::new();
    for (number, precedence) in [
        ("KE8YGW-0001", "routine"),
        ("KE8YGW-0002", "emergency"),
        ("KE8YGW-0003", "priority"),
    ] {
        let outcome = submit_proposal(
            &store,
            &bus,
            &emcomm_context(),
            emcomm_proposal(
                PROPOSAL_EMCOMM_MESSAGE_CREATE,
                logbook_id,
                None,
                json!({
                    "station_callsign": "KE8YGW",
                    "incident_id": incident_id.to_string(),
                    "message_number": number,
                    "precedence": precedence,
                    "from": "Net Control",
                    "to": "Shelter 1",
                    "body": "Traffic."
                }),
            ),
        )
        .await
        .expect("message drafts");
        transmitted.push(outcome.official_event.entity_id.expect("message id"));
    }

    submit_proposal(
        &store,
        &bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_MESSAGE_TRANSMIT,
            logbook_id,
            Some(transmitted[1]),
            json!({ "station_callsign": "KE8YGW", "transmitted_at": "2026-11-24T12:20:00Z" }),
        ),
    )
    .await
    .expect("transmission appends");

    let mut projection = EmCommProjection::new();
    let events = store.list_events(logbook_id).await.expect("events");
    projection
        .rebuild(events.iter())
        .expect("projection rebuilds");

    let ordered = projection.messages_for_incident(incident_id);
    assert_eq!(ordered[0].precedence, MessagePrecedence::Emergency);
    assert_eq!(ordered[1].precedence, MessagePrecedence::Priority);
    assert_eq!(ordered[2].precedence, MessagePrecedence::Routine);

    let waiting = projection.unacknowledged_messages(incident_id);
    assert_eq!(waiting.len(), 1);
    assert_eq!(waiting[0].message_id, transmitted[1]);
}

#[tokio::test]
async fn emcomm_activity_log_links_entries_to_incidents_and_periods() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::new(64);
    let logbook_id = Uuid::new_v4();
    let incident_id = open_test_incident(&store, &bus, logbook_id).await;

    let period = submit_proposal(
        &store,
        &bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_PERIOD_OPEN,
            logbook_id,
            None,
            json!({
                "station_callsign": "KE8YGW",
                "incident_id": incident_id.to_string(),
                "period_number": 1,
                "started_at": "2026-11-24T12:05:00Z"
            }),
        ),
    )
    .await
    .expect("period opens");
    let period_id = period.official_event.entity_id.expect("period id");

    for summary in ["Net opened on 146.940", "Shelter 1 reports 42 occupants"] {
        submit_proposal(
            &store,
            &bus,
            &emcomm_context(),
            emcomm_proposal(
                PROPOSAL_EMCOMM_ACTIVITY_LOG,
                logbook_id,
                None,
                json!({
                    "station_callsign": "KE8YGW",
                    "incident_id": incident_id.to_string(),
                    "period_id": period_id.to_string(),
                    "occurred_at": "2026-11-24T12:30:00Z",
                    "summary": summary
                }),
            ),
        )
        .await
        .expect("activity appends");
    }

    let mut projection = EmCommProjection::new();
    let events = store.list_events(logbook_id).await.expect("events");
    projection
        .rebuild(events.iter())
        .expect("projection rebuilds");

    assert_eq!(projection.activity_for_incident(incident_id).len(), 2);
    assert_eq!(projection.activity_for_period(period_id).len(), 2);
    assert_eq!(
        projection.activity_for_incident(incident_id)[0].payload["summary"],
        json!("Net opened on 146.940")
    );
}

#[tokio::test]
async fn emcomm_incident_package_carries_every_record_and_its_history() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::new(64);
    let logbook_id = Uuid::new_v4();
    let incident_id = open_test_incident(&store, &bus, logbook_id).await;

    let person = submit_proposal(
        &store,
        &bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_PERSON_CHECK_IN,
            logbook_id,
            None,
            json!({
                "station_callsign": "KE8YGW",
                "incident_id": incident_id.to_string(),
                "name": "A. Operator",
                "checked_in_at": "2026-11-24T12:10:00Z"
            }),
        ),
    )
    .await
    .expect("check-in appends");
    let person_id = person.official_event.entity_id.expect("person id");

    submit_proposal(
        &store,
        &bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_PERSON_CHECK_OUT,
            logbook_id,
            Some(person_id),
            json!({ "station_callsign": "KE8YGW", "checked_out_at": "2026-11-24T18:00:00Z" }),
        ),
    )
    .await
    .expect("check-out appends");

    submit_proposal(
        &store,
        &bus,
        &emcomm_context(),
        emcomm_proposal(
            PROPOSAL_EMCOMM_INCIDENT_CLOSE,
            logbook_id,
            Some(incident_id),
            json!({ "station_callsign": "KE8YGW", "closed_at": "2026-11-24T20:00:00Z" }),
        ),
    )
    .await
    .expect("incident closes");

    let mut projection = EmCommProjection::new();
    let events = store.list_events(logbook_id).await.expect("events");
    projection
        .rebuild(events.iter())
        .expect("projection rebuilds");

    assert!(projection.incidents(false).is_empty());
    assert_eq!(projection.incidents(true).len(), 1);

    let package = projection
        .incident_package(incident_id)
        .expect("incident package");
    assert_eq!(package["schema_version"], json!(EMCOMM_SCHEMA_VERSION));
    assert_eq!(package["personnel"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        package["personnel"][0]["history"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(package["incident"]["status"], json!("closed"));
    assert_eq!(
        package["forms"],
        json!(["ICS-211", "ICS-213", "ICS-213RR", "ICS-214"])
    );

    let summary = projection.summary(incident_id);
    assert_eq!(summary["roster"], json!(1));
    assert_eq!(summary["checked_in"], json!(0));
}

#[tokio::test]
async fn emcomm_proposals_require_their_own_capability() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::new(64);
    let context = ProposalContext::local_admin(
        plugin_manifest(vec![PluginCapability::EmCommView]),
        OperatorRole::Admin,
    );

    let error = submit_proposal(
        &store,
        &bus,
        &context,
        emcomm_proposal(
            PROPOSAL_EMCOMM_INCIDENT_OPEN,
            Uuid::new_v4(),
            None,
            json!({
                "station_callsign": "KE8YGW",
                "incident_name": "County Exercise",
                "opened_at": "2026-11-24T12:00:00Z"
            }),
        ),
    )
    .await
    .expect_err("view alone cannot open an incident");
    assert!(matches!(
        error,
        ProposalValidationError::PermissionDenied { .. }
            | ProposalValidationError::MissingPluginCapability(_)
            | ProposalValidationError::PluginPermissionDenied(_)
    ));
}

#[test]
fn emcomm_message_numbers_round_trip_and_reject_bad_input() {
    let number = MessageNumber::new("ke8ygw", 7);
    assert_eq!(number.to_string(), "KE8YGW-0007");
    assert_eq!(
        MessageNumber::parse("KE8YGW-0007").expect("parses"),
        MessageNumber {
            station_prefix: "KE8YGW".to_owned(),
            sequence: 7
        }
    );
    for bad in ["", "7", "KE8YGW-", "KE8YGW-0000", "-0007", "KE8YGW-abc"] {
        assert!(
            MessageNumber::parse(bad).is_err(),
            "`{bad}` must not parse as a message number"
        );
    }
}

#[test]
fn display_settings_default_to_the_first_shipped_layouts() {
    let settings = ApplicationSettings::default();
    assert_eq!(settings.display.appearance, DEFAULT_APPEARANCE_MODE);
    assert_eq!(
        settings.display.desktop_shell_layout,
        DEFAULT_DESKTOP_SHELL_LAYOUT
    );
    assert_eq!(
        settings.display.mobile_dashboard_layout,
        DEFAULT_MOBILE_DASHBOARD_LAYOUT
    );
    assert!(DESKTOP_SHELL_LAYOUTS.contains(&DEFAULT_DESKTOP_SHELL_LAYOUT));
    assert!(MOBILE_DASHBOARD_LAYOUTS.contains(&DEFAULT_MOBILE_DASHBOARD_LAYOUT));
    assert!(APPEARANCE_MODES.contains(&DEFAULT_APPEARANCE_MODE));
}

#[test]
fn normalizing_accepts_known_layouts_and_falls_back_for_unknown_ones() {
    let mut settings = ApplicationSettings::default();
    settings.display.appearance = "  DARK ".to_owned();
    settings.display.desktop_shell_layout = "Tabbed-Workbench".to_owned();
    settings.display.mobile_dashboard_layout = "map-sheet".to_owned();
    let settings = settings.normalized().expect("known choices normalize");
    assert_eq!(settings.display.appearance, "dark");
    assert_eq!(settings.display.desktop_shell_layout, "tabbed-workbench");
    assert_eq!(settings.display.mobile_dashboard_layout, "map-sheet");

    let mut stale = ApplicationSettings::default();
    stale.display.appearance = "solarized".to_owned();
    stale.display.desktop_shell_layout = "holodeck".to_owned();
    stale.display.mobile_dashboard_layout = "carousel".to_owned();
    // A client from a different build must not fail the whole save and lose the
    // operator's other edits just because it named a layout we do not ship.
    let stale = stale.normalized().expect("unknown choices fall back");
    assert_eq!(stale.display.appearance, DEFAULT_APPEARANCE_MODE);
    assert_eq!(
        stale.display.desktop_shell_layout,
        DEFAULT_DESKTOP_SHELL_LAYOUT
    );
    assert_eq!(
        stale.display.mobile_dashboard_layout,
        DEFAULT_MOBILE_DASHBOARD_LAYOUT
    );
}

#[test]
fn settings_saved_before_layout_choice_existed_still_deserialize() {
    let mut value = serde_json::to_value(ApplicationSettings::default()).expect("serializes");
    let display = value
        .get_mut("display")
        .and_then(|display| display.as_object_mut())
        .expect("display object");
    display.remove("desktop_shell_layout");
    display.remove("mobile_dashboard_layout");

    let restored: ApplicationSettings = serde_json::from_value(value).expect("older payload loads");
    assert_eq!(
        restored.display.desktop_shell_layout,
        DEFAULT_DESKTOP_SHELL_LAYOUT
    );
    assert_eq!(
        restored.display.mobile_dashboard_layout,
        DEFAULT_MOBILE_DASHBOARD_LAYOUT
    );
}
