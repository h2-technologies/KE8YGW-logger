use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// A canonical official logbook event.
///
/// The `event_hash` is SHA-256 over all other event fields using serde_json's
/// deterministic struct field order and sorted JSON object keys.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoreEventEnvelope {
    pub event_id: Uuid,
    pub event_type: String,
    pub logbook_id: Uuid,
    pub entity_id: Option<Uuid>,
    pub previous_hash: Option<String>,
    pub event_hash: String,
    pub timestamp: DateTime<Utc>,
    pub author_operator_id: Option<Uuid>,
    pub station_callsign: String,
    pub operator_callsign: Option<String>,
    pub author_device_id: Uuid,
    pub source_device_id: Uuid,
    pub correlation_id: Uuid,
    pub source_plugin_id: Option<String>,
    pub schema_version: u32,
    pub payload: Value,
}

/// Input for appending a new official logbook event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewLogbookEvent {
    pub event_type: String,
    pub logbook_id: Uuid,
    pub entity_id: Option<Uuid>,
    pub author_operator_id: Option<Uuid>,
    pub station_callsign: String,
    pub operator_callsign: Option<String>,
    pub author_device_id: Uuid,
    pub source_device_id: Uuid,
    pub correlation_id: Uuid,
    pub source_plugin_id: Option<String>,
    pub schema_version: u32,
    pub payload: Value,
}

#[derive(Debug, Serialize)]
struct CanonicalEvent<'a> {
    event_id: Uuid,
    event_type: &'a str,
    logbook_id: Uuid,
    entity_id: Option<Uuid>,
    previous_hash: Option<&'a str>,
    timestamp: DateTime<Utc>,
    author_operator_id: Option<Uuid>,
    station_callsign: &'a str,
    operator_callsign: Option<&'a str>,
    author_device_id: Uuid,
    source_device_id: Uuid,
    correlation_id: Uuid,
    source_plugin_id: Option<&'a str>,
    schema_version: u32,
    payload: &'a Value,
}

impl CoreEventEnvelope {
    pub fn from_new(new_event: NewLogbookEvent, previous_hash: Option<String>) -> Self {
        let mut event = Self {
            event_id: Uuid::new_v4(),
            event_type: new_event.event_type,
            logbook_id: new_event.logbook_id,
            entity_id: new_event.entity_id,
            previous_hash,
            event_hash: String::new(),
            timestamp: Utc::now(),
            author_operator_id: new_event.author_operator_id,
            station_callsign: new_event.station_callsign,
            operator_callsign: new_event.operator_callsign,
            author_device_id: new_event.author_device_id,
            source_device_id: new_event.source_device_id,
            correlation_id: new_event.correlation_id,
            source_plugin_id: new_event.source_plugin_id,
            schema_version: new_event.schema_version,
            payload: new_event.payload,
        };
        event.event_hash = event.calculate_hash();
        event
    }

    pub fn calculate_hash(&self) -> String {
        let canonical = CanonicalEvent {
            event_id: self.event_id,
            event_type: &self.event_type,
            logbook_id: self.logbook_id,
            entity_id: self.entity_id,
            previous_hash: self.previous_hash.as_deref(),
            timestamp: self.timestamp,
            author_operator_id: self.author_operator_id,
            station_callsign: &self.station_callsign,
            operator_callsign: self.operator_callsign.as_deref(),
            author_device_id: self.author_device_id,
            source_device_id: self.source_device_id,
            correlation_id: self.correlation_id,
            source_plugin_id: self.source_plugin_id.as_deref(),
            schema_version: self.schema_version,
            payload: &self.payload,
        };

        let bytes = serde_json::to_vec(&canonical)
            .expect("serializing canonical event hash material should not fail");
        let digest = Sha256::digest(bytes);
        format!("{digest:x}")
    }

    pub fn hash_is_valid(&self) -> bool {
        self.event_hash == self.calculate_hash()
    }
}
