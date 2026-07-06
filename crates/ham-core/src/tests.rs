use chrono::Utc;
use ham_plugin_sdk::{
    PluginCapability, PluginManifest, ProposalEnvelope, OFFICIAL_LOG_QSO_CORRECTED,
    OFFICIAL_LOG_QSO_CREATED, OFFICIAL_LOG_QSO_DELETED, OFFICIAL_LOG_QSO_NOTE_ADDED,
    OFFICIAL_LOG_QSO_RESTORED, PROPOSAL_QSO_CREATE, PROPOSAL_QSO_DELETE,
};
use serde_json::json;
use std::{fs, path::PathBuf};
use uuid::Uuid;

use crate::{
    submit_proposal, BusEvent, EventBus, InMemoryEventBus, InMemoryLogbookEventStore,
    LogbookEventStore, NewLogbookEvent, OperatorRole, Projection, ProposalContext,
    ProposalValidationError, QsoCurrentStateProjection,
};

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
    PluginManifest {
        plugin_id: "test-plugin".to_owned(),
        name: "Test Plugin".to_owned(),
        version: "0.1.0".to_owned(),
        capabilities,
    }
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
    let context = ProposalContext {
        plugin_manifest: plugin_manifest(vec![PluginCapability::QsoCreate]),
        operator_role: OperatorRole::Logger,
    };

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
    let context = ProposalContext {
        plugin_manifest: plugin_manifest(vec![PluginCapability::QsoCreate]),
        operator_role: OperatorRole::Logger,
    };
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
        ham_plugin_sdk::OFFICIAL_LOG_QSO_DELETED,
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
    let context = ProposalContext {
        plugin_manifest: plugin_manifest(vec![]),
        operator_role: OperatorRole::Logger,
    };

    let err = submit_proposal(&store, &bus, &context, proposal(PROPOSAL_QSO_CREATE, None))
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        ProposalValidationError::MissingPluginCapability(PluginCapability::QsoCreate)
    ));
}

#[tokio::test]
async fn accepted_proposals_publish_an_event_on_the_event_bus() {
    let store = InMemoryLogbookEventStore::new();
    let bus = InMemoryEventBus::default();
    let mut receiver = bus.subscribe();
    let context = ProposalContext {
        plugin_manifest: plugin_manifest(vec![PluginCapability::QsoCreate]),
        operator_role: OperatorRole::Logger,
    };

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
    let context = ProposalContext {
        plugin_manifest: plugin_manifest(vec![PluginCapability::QsoDelete]),
        operator_role: OperatorRole::Logger,
    };

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
