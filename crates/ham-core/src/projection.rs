use std::collections::{HashMap, HashSet};

use ham_plugin_sdk::{
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
