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

    fn recompute_stats(&mut self) {
        for record in self.records.values_mut() {
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
}

impl Projection for ActivationProjection {
    type Error = ProjectionError;

    fn apply(&mut self, event: &CoreEventEnvelope) -> Result<(), Self::Error> {
        self.qso_projection.apply(event)?;
        let Some(entity_id) = event.entity_id else {
            self.recompute_stats();
            return Ok(());
        };

        match event.event_type.as_str() {
            OFFICIAL_LOG_ACTIVATION_CREATED | OFFICIAL_LOG_ACTIVATION_STARTED => {
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
                if let Some(record) = self.records.get_mut(&entity_id) {
                    merge_json_object(&mut record.payload, &event.payload)?;
                    if let Some(status) = event.payload.get("status").and_then(Value::as_str) {
                        record.status = status.to_owned();
                    }
                    record.last_event_hash = event.event_hash.clone();
                }
            }
            OFFICIAL_LOG_ACTIVATION_ENDED => {
                if let Some(record) = self.records.get_mut(&entity_id) {
                    merge_json_object(&mut record.payload, &event.payload)?;
                    record.status = "ended".to_owned();
                    record.last_event_hash = event.event_hash.clone();
                }
            }
            OFFICIAL_LOG_ACTIVATION_CANCELLED => {
                if let Some(record) = self.records.get_mut(&entity_id) {
                    record.status = "cancelled".to_owned();
                    record.last_event_hash = event.event_hash.clone();
                }
            }
            OFFICIAL_LOG_ACTIVATION_NOTE_ADDED => {
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
                    if let Some(record) = self.records.get_mut(&activation_id) {
                        record.linked_qsos.insert(entity_id);
                        record.last_event_hash = event.event_hash.clone();
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
                    if let Some(record) = self.records.get_mut(&activation_id) {
                        record.linked_qsos.remove(&entity_id);
                        record.last_event_hash = event.event_hash.clone();
                    }
                }
            }
            _ => {}
        }
        self.recompute_stats();
        Ok(())
    }

    fn clear(&mut self) {
        self.records.clear();
        self.qso_projection.clear();
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
