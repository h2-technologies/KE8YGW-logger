use std::collections::{HashMap, HashSet};

use ham_plugin_sdk::{
    OFFICIAL_LOG_ACTIVATION_CANCELLED, OFFICIAL_LOG_ACTIVATION_CREATED,
    OFFICIAL_LOG_ACTIVATION_ENDED, OFFICIAL_LOG_ACTIVATION_NOTE_ADDED,
    OFFICIAL_LOG_ACTIVATION_STARTED, OFFICIAL_LOG_ACTIVATION_UPDATED,
    OFFICIAL_LOG_QSO_ACTIVATION_LINKED, OFFICIAL_LOG_QSO_ACTIVATION_UNLINKED,
    OFFICIAL_LOG_QSO_CORRECTED, OFFICIAL_LOG_QSO_CREATED, OFFICIAL_LOG_QSO_DELETED,
    OFFICIAL_LOG_QSO_NOTE_ADDED, OFFICIAL_LOG_QSO_RESTORED,
};
use serde_json::{json, Map, Value};
use uuid::Uuid;

use crate::event::CoreEventEnvelope;

pub trait Projection {
    type Error;

    fn apply(&mut self, event: &CoreEventEnvelope) -> Result<(), Self::Error>;
    fn rebuild<'a>(
        &mut self,
        events: impl IntoIterator<Item = &'a CoreEventEnvelope>,
    ) -> Result<(), Self::Error> {
        self.clear();
        for event in events {
            self.apply(event)?;
        }
        Ok(())
    }
    fn clear(&mut self);
}

#[derive(Debug, Clone, PartialEq)]
pub struct QsoRecord {
    pub qso_id: Uuid,
    pub payload: Value,
    pub note_history: Vec<Value>,
    pub deleted: bool,
    pub last_event_hash: String,
}

#[derive(Debug, Default)]
pub struct QsoCurrentStateProjection {
    records: HashMap<Uuid, QsoRecord>,
    tombstones: HashSet<Uuid>,
}

impl QsoCurrentStateProjection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, qso_id: Uuid) -> Option<&QsoRecord> {
        self.records.get(&qso_id).filter(|record| !record.deleted)
    }

    pub fn get_including_deleted(&self, qso_id: Uuid) -> Option<&QsoRecord> {
        self.records.get(&qso_id)
    }

    pub fn current_qsos(&self) -> Vec<&QsoRecord> {
        self.list(false)
    }

    pub fn list(&self, include_deleted: bool) -> Vec<&QsoRecord> {
        self.records
            .values()
            .filter(|record| include_deleted || !record.deleted)
            .collect()
    }

    pub fn upsert_record(&mut self, record: QsoRecord) {
        if record.deleted {
            self.tombstones.insert(record.qso_id);
        } else {
            self.tombstones.remove(&record.qso_id);
        }
        self.records.insert(record.qso_id, record);
    }

    pub fn is_tombstoned(&self, qso_id: Uuid) -> bool {
        self.tombstones.contains(&qso_id)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ActivationRecord {
    pub activation_id: Uuid,
    pub payload: Value,
    pub status: String,
    pub note_history: Vec<Value>,
    pub linked_qsos: HashSet<Uuid>,
    pub qso_count: usize,
    pub unique_callsign_count: usize,
    pub band_summary: HashMap<String, usize>,
    pub mode_summary: HashMap<String, usize>,
    pub last_event_hash: String,
}

#[derive(Debug, Default)]
pub struct ActivationProjection {
    records: HashMap<Uuid, ActivationRecord>,
    qso_projection: QsoCurrentStateProjection,
    activations_by_qso: HashMap<Uuid, HashSet<Uuid>>,
}

impl ActivationProjection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, activation_id: Uuid) -> Option<&ActivationRecord> {
        self.records.get(&activation_id)
    }

    pub fn list(&self, include_ended_cancelled: bool) -> Vec<&ActivationRecord> {
        self.records
            .values()
            .filter(|record| {
                include_ended_cancelled || !matches!(record.status.as_str(), "ended" | "cancelled")
            })
            .collect()
    }

    pub fn active_for_station_operator(
        &self,
        station_callsign: &str,
        operator_callsign: &str,
    ) -> Option<&ActivationRecord> {
        self.records.values().find(|record| {
            record.status == "active"
                && record
                    .payload
                    .get("station_callsign")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.eq_ignore_ascii_case(station_callsign))
                && record
                    .payload
                    .get("operator_callsign")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.eq_ignore_ascii_case(operator_callsign))
        })
    }

    /// Activations that currently link `qso_id`.
    ///
    /// Replay consumers use this to learn which activation rows a QSO event
    /// invalidates without re-deriving link state themselves.
    pub fn activations_for_qso(&self, qso_id: Uuid) -> Vec<Uuid> {
        self.activations_by_qso
            .get(&qso_id)
            .map(|activation_ids| activation_ids.iter().copied().collect())
            .unwrap_or_default()
    }

    /// Recomputes derived counters for the activations an event actually touched.
    ///
    /// Activation counters depend only on `linked_qsos` and the linked QSOs'
    /// current state, so an activation whose links and QSOs are unchanged by an
    /// event keeps identical counters and does not need recomputing.
    fn recompute_stats_for(&mut self, activation_ids: &[Uuid]) {
        for activation_id in activation_ids {
            let Some(record) = self.records.get_mut(activation_id) else {
                continue;
            };
            let mut callsigns = HashSet::new();
            let mut bands = HashMap::new();
            let mut modes = HashMap::new();
            let mut count = 0usize;
            for qso_id in &record.linked_qsos {
                let Some(qso) = self.qso_projection.get(*qso_id) else {
                    continue;
                };
                count += 1;
                if let Some(callsign) = qso
                    .payload
                    .get("contacted_callsign")
                    .and_then(Value::as_str)
                {
                    callsigns.insert(callsign.to_ascii_uppercase());
                }
                if let Some(band) = qso.payload.get("band").and_then(Value::as_str) {
                    *bands.entry(band.to_owned()).or_insert(0) += 1;
                }
                if let Some(mode) = qso.payload.get("mode").and_then(Value::as_str) {
                    *modes.entry(mode.to_owned()).or_insert(0) += 1;
                }
            }
            record.qso_count = count;
            record.unique_callsign_count = callsigns.len();
            record.band_summary = bands;
            record.mode_summary = modes;
        }
    }

    fn link_qso(&mut self, activation_id: Uuid, qso_id: Uuid) {
        self.activations_by_qso
            .entry(qso_id)
            .or_default()
            .insert(activation_id);
    }

    fn unlink_qso(&mut self, activation_id: Uuid, qso_id: Uuid) {
        if let Some(activation_ids) = self.activations_by_qso.get_mut(&qso_id) {
            activation_ids.remove(&activation_id);
            if activation_ids.is_empty() {
                self.activations_by_qso.remove(&qso_id);
            }
        }
    }

    /// Drops the link index entries for an activation record that is being replaced.
    fn forget_links(&mut self, activation_id: Uuid) {
        let Some(record) = self.records.get(&activation_id) else {
            return;
        };
        let linked_qsos = record.linked_qsos.iter().copied().collect::<Vec<_>>();
        for qso_id in linked_qsos {
            self.unlink_qso(activation_id, qso_id);
        }
    }
}

impl Projection for ActivationProjection {
    type Error = ProjectionError;

    fn apply(&mut self, event: &CoreEventEnvelope) -> Result<(), Self::Error> {
        self.qso_projection.apply(event)?;
        let Some(entity_id) = event.entity_id else {
            // Nothing changed: the QSO projection also ignores entity-less events.
            return Ok(());
        };

        // A QSO event changes the counters of every activation linking that QSO.
        let mut affected = self.activations_for_qso(entity_id);

        match event.event_type.as_str() {
            OFFICIAL_LOG_ACTIVATION_CREATED | OFFICIAL_LOG_ACTIVATION_STARTED => {
                self.forget_links(entity_id);
                affected.push(entity_id);
                let mut payload = event.payload.clone();
                payload["activation_id"] = json!(entity_id);
                let status = if event.event_type == OFFICIAL_LOG_ACTIVATION_STARTED {
                    "active".to_owned()
                } else {
                    payload
                        .get("status")
                        .and_then(Value::as_str)
                        .unwrap_or("planned")
                        .to_owned()
                };
                self.records.insert(
                    entity_id,
                    ActivationRecord {
                        activation_id: entity_id,
                        payload,
                        status,
                        note_history: Vec::new(),
                        linked_qsos: HashSet::new(),
                        qso_count: 0,
                        unique_callsign_count: 0,
                        band_summary: HashMap::new(),
                        mode_summary: HashMap::new(),
                        last_event_hash: event.event_hash.clone(),
                    },
                );
            }
            OFFICIAL_LOG_ACTIVATION_UPDATED => {
                affected.push(entity_id);
                if let Some(record) = self.records.get_mut(&entity_id) {
                    merge_json_object(&mut record.payload, &event.payload)?;
                    if let Some(status) = event.payload.get("status").and_then(Value::as_str) {
                        record.status = status.to_owned();
                    }
                    record.last_event_hash = event.event_hash.clone();
                }
            }
            OFFICIAL_LOG_ACTIVATION_ENDED => {
                affected.push(entity_id);
                if let Some(record) = self.records.get_mut(&entity_id) {
                    merge_json_object(&mut record.payload, &event.payload)?;
                    record.status = "ended".to_owned();
                    record.last_event_hash = event.event_hash.clone();
                }
            }
            OFFICIAL_LOG_ACTIVATION_CANCELLED => {
                affected.push(entity_id);
                if let Some(record) = self.records.get_mut(&entity_id) {
                    record.status = "cancelled".to_owned();
                    record.last_event_hash = event.event_hash.clone();
                }
            }
            OFFICIAL_LOG_ACTIVATION_NOTE_ADDED => {
                affected.push(entity_id);
                if let Some(record) = self.records.get_mut(&entity_id) {
                    record.note_history.push(event.payload.clone());
                    record.last_event_hash = event.event_hash.clone();
                }
            }
            OFFICIAL_LOG_QSO_ACTIVATION_LINKED => {
                if let Some(activation_id) = event
                    .payload
                    .get("activation_id")
                    .and_then(Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                {
                    affected.push(activation_id);
                    if let Some(record) = self.records.get_mut(&activation_id) {
                        record.linked_qsos.insert(entity_id);
                        record.last_event_hash = event.event_hash.clone();
                        self.link_qso(activation_id, entity_id);
                    }
                }
            }
            OFFICIAL_LOG_QSO_ACTIVATION_UNLINKED => {
                if let Some(activation_id) = event
                    .payload
                    .get("activation_id")
                    .and_then(Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok())
                {
                    affected.push(activation_id);
                    if let Some(record) = self.records.get_mut(&activation_id) {
                        record.linked_qsos.remove(&entity_id);
                        record.last_event_hash = event.event_hash.clone();
                        self.unlink_qso(activation_id, entity_id);
                    }
                }
            }
            _ => {}
        }
        self.recompute_stats_for(&affected);
        Ok(())
    }

    fn clear(&mut self) {
        self.records.clear();
        self.qso_projection.clear();
        self.activations_by_qso.clear();
    }
}

impl Projection for QsoCurrentStateProjection {
    type Error = ProjectionError;

    fn apply(&mut self, event: &CoreEventEnvelope) -> Result<(), Self::Error> {
        let Some(qso_id) = event.entity_id else {
            return Ok(());
        };

        match event.event_type.as_str() {
            OFFICIAL_LOG_QSO_CREATED => {
                let mut payload = event.payload.clone();
                payload["qso_id"] = json!(qso_id);
                self.records.insert(
                    qso_id,
                    QsoRecord {
                        qso_id,
                        payload,
                        note_history: Vec::new(),
                        deleted: false,
                        last_event_hash: event.event_hash.clone(),
                    },
                );
            }
            OFFICIAL_LOG_QSO_CORRECTED => {
                if let Some(record) = self.records.get_mut(&qso_id) {
                    merge_json_object(&mut record.payload, &event.payload)?;
                    record.last_event_hash = event.event_hash.clone();
                }
            }
            OFFICIAL_LOG_QSO_DELETED => {
                if let Some(record) = self.records.get_mut(&qso_id) {
                    record.deleted = true;
                    record.last_event_hash = event.event_hash.clone();
                }
                self.tombstones.insert(qso_id);
            }
            OFFICIAL_LOG_QSO_RESTORED => {
                if let Some(record) = self.records.get_mut(&qso_id) {
                    record.deleted = false;
                    record.last_event_hash = event.event_hash.clone();
                }
                self.tombstones.remove(&qso_id);
            }
            OFFICIAL_LOG_QSO_NOTE_ADDED => {
                if let Some(record) = self.records.get_mut(&qso_id) {
                    record.note_history.push(event.payload.clone());
                    record.last_event_hash = event.event_hash.clone();
                }
            }
            _ => {}
        }

        Ok(())
    }

    fn clear(&mut self) {
        self.records.clear();
        self.tombstones.clear();
    }
}

/// Which projected entities an official event touches.
///
/// Replay consumers (including out-of-process projectors) use this instead of
/// re-deriving the event vocabulary, so the projections in this module stay the
/// single place that decides what each official event type affects. Net Control
/// events have their own projection in [`crate::net`] and are reported here as
/// unconsumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ProjectionTouch {
    /// The QSO whose projected state this event changes.
    pub qso_id: Option<Uuid>,
    /// The activation whose projected state this event changes.
    pub activation_id: Option<Uuid>,
    /// True for tombstone events, which remove an entity from current state
    /// without removing anything from official history.
    pub tombstone: bool,
    /// False when neither projection in this module consumes the event.
    pub consumed: bool,
}

/// Classifies which QSO and activation projection entities `event` touches.
pub fn projection_touch(event: &CoreEventEnvelope) -> ProjectionTouch {
    let Some(entity_id) = event.entity_id else {
        return ProjectionTouch::default();
    };

    match event.event_type.as_str() {
        OFFICIAL_LOG_QSO_CREATED
        | OFFICIAL_LOG_QSO_CORRECTED
        | OFFICIAL_LOG_QSO_RESTORED
        | OFFICIAL_LOG_QSO_NOTE_ADDED => ProjectionTouch {
            qso_id: Some(entity_id),
            consumed: true,
            ..ProjectionTouch::default()
        },
        OFFICIAL_LOG_QSO_DELETED => ProjectionTouch {
            qso_id: Some(entity_id),
            tombstone: true,
            consumed: true,
            ..ProjectionTouch::default()
        },
        OFFICIAL_LOG_ACTIVATION_CREATED
        | OFFICIAL_LOG_ACTIVATION_UPDATED
        | OFFICIAL_LOG_ACTIVATION_STARTED
        | OFFICIAL_LOG_ACTIVATION_ENDED
        | OFFICIAL_LOG_ACTIVATION_CANCELLED
        | OFFICIAL_LOG_ACTIVATION_NOTE_ADDED => ProjectionTouch {
            activation_id: Some(entity_id),
            consumed: true,
            ..ProjectionTouch::default()
        },
        OFFICIAL_LOG_QSO_ACTIVATION_LINKED | OFFICIAL_LOG_QSO_ACTIVATION_UNLINKED => {
            ProjectionTouch {
                qso_id: Some(entity_id),
                activation_id: event
                    .payload
                    .get("activation_id")
                    .and_then(Value::as_str)
                    .and_then(|value| Uuid::parse_str(value).ok()),
                consumed: true,
                ..ProjectionTouch::default()
            }
        }
        _ => ProjectionTouch::default(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    #[error("qso correction payload must be a JSON object")]
    CorrectionPayloadMustBeObject,
}

fn merge_json_object(target: &mut Value, patch: &Value) -> Result<(), ProjectionError> {
    let Some(patch_object) = patch.as_object() else {
        return Err(ProjectionError::CorrectionPayloadMustBeObject);
    };

    if !target.is_object() {
        *target = Value::Object(Map::new());
    }

    let target_object = target
        .as_object_mut()
        .expect("target was converted to object before merge");
    for (key, value) in patch_object {
        target_object.insert(key.clone(), value.clone());
    }

    Ok(())
}
