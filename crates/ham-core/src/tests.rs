use ham_plugin_sdk::{
    PluginCapability, PluginManifest, ProposalEnvelope, OFFICIAL_LOG_QSO_CREATED,
    PROPOSAL_QSO_CREATE, PROPOSAL_QSO_DELETE,
};
use serde_json::json;
use uuid::Uuid;

use crate::{
    submit_proposal, BusEvent, EventBus, InMemoryEventBus, InMemoryLogbookEventStore,
    LogbookEventStore, NewLogbookEvent, OperatorRole, Projection, ProposalContext,
    ProposalValidationError, QsoCurrentStateProjection,
};

fn qso_payload() -> serde_json::Value {
    json!({
        "callsign": "K1ABC",
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

#[tokio::test]
async fn events_append_with_correct_previous_hash() {
    let store = InMemoryLogbookEventStore::new();
    let logbook_id = Uuid::new_v4();
    let device_id = Uuid::new_v4();

    let first = store
        .append_event(NewLogbookEvent {
            event_type: OFFICIAL_LOG_QSO_CREATED.to_owned(),
            logbook_id,
            entity_id: Some(Uuid::new_v4()),
            author_operator_id: None,
            author_device_id: device_id,
            source_plugin_id: None,
            schema_version: 1,
            payload: qso_payload(),
        })
        .await
        .unwrap();
    let second = store
        .append_event(NewLogbookEvent {
            event_type: OFFICIAL_LOG_QSO_CREATED.to_owned(),
            logbook_id,
            entity_id: Some(Uuid::new_v4()),
            author_operator_id: None,
            author_device_id: device_id,
            source_plugin_id: None,
            schema_version: 1,
            payload: qso_payload(),
        })
        .await
        .unwrap();

    assert_eq!(first.previous_hash, None);
    assert_eq!(second.previous_hash, Some(first.event_hash));
}

#[tokio::test]
async fn chain_verification_passes_for_valid_chains() {
    let store = InMemoryLogbookEventStore::new();
    let logbook_id = Uuid::new_v4();

    for _ in 0..3 {
        store
            .append_event(NewLogbookEvent {
                event_type: OFFICIAL_LOG_QSO_CREATED.to_owned(),
                logbook_id,
                entity_id: Some(Uuid::new_v4()),
                author_operator_id: None,
                author_device_id: Uuid::new_v4(),
                source_plugin_id: None,
                schema_version: 1,
                payload: qso_payload(),
            })
            .await
            .unwrap();
    }

    store.verify_chain(logbook_id).await.unwrap();
}

#[tokio::test]
async fn tampering_breaks_verification() {
    let store = InMemoryLogbookEventStore::new();
    let logbook_id = Uuid::new_v4();
    let event = store
        .append_event(NewLogbookEvent {
            event_type: OFFICIAL_LOG_QSO_CREATED.to_owned(),
            logbook_id,
            entity_id: Some(Uuid::new_v4()),
            author_operator_id: None,
            author_device_id: Uuid::new_v4(),
            source_plugin_id: None,
            schema_version: 1,
            payload: qso_payload(),
        })
        .await
        .unwrap();

    let mut tampered = event;
    tampered.payload["callsign"] = json!("N0BAD");
    store.replace_event_for_testing(tampered).await;

    assert!(store.verify_chain(logbook_id).await.is_err());
}

#[tokio::test]
async fn qso_deleted_hides_projection_without_removing_event() {
    let store = InMemoryLogbookEventStore::new();
    let logbook_id = Uuid::new_v4();
    let qso_id = Uuid::new_v4();

    store
        .append_event(NewLogbookEvent {
            event_type: OFFICIAL_LOG_QSO_CREATED.to_owned(),
            logbook_id,
            entity_id: Some(qso_id),
            author_operator_id: None,
            author_device_id: Uuid::new_v4(),
            source_plugin_id: None,
            schema_version: 1,
            payload: qso_payload(),
        })
        .await
        .unwrap();
    store
        .append_event(NewLogbookEvent {
            event_type: ham_plugin_sdk::OFFICIAL_LOG_QSO_DELETED.to_owned(),
            logbook_id,
            entity_id: Some(qso_id),
            author_operator_id: None,
            author_device_id: Uuid::new_v4(),
            source_plugin_id: None,
            schema_version: 1,
            payload: json!({"reason": "duplicate"}),
        })
        .await
        .unwrap();

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
    let published = receiver.recv().await.unwrap();

    assert_eq!(outcome.official_event.event_type, OFFICIAL_LOG_QSO_CREATED);
    assert!(matches!(
        published,
        BusEvent::OfficialLogbookEvent(event) if event.event_id == outcome.official_event.event_id
    ));
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
