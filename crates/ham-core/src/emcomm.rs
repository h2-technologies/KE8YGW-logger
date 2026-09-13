//! Append-only incident, operational period, people, assignment, and message
//! model for emergency communications.
//!
//! Every state change is an official event. Nothing in this module edits an
//! earlier record: a correction appends an update event, the projection keeps
//! the full correction history alongside the merged current view, and a
//! message that was transmitted keeps its transmission entry even after it is
//! cancelled. That is what makes an exported incident package auditable.
//!
//! Message numbers are allocated per station without coordination, so two
//! disconnected stations working the same incident cannot collide: a number is
//! the originating station's identity plus its own sequence.

use std::collections::{BTreeMap, HashMap};

use crate::plugin_sdk::{
    OFFICIAL_LOG_EMCOMM_ACTIVITY_LOGGED, OFFICIAL_LOG_EMCOMM_ASSIGNMENT_CREATED,
    OFFICIAL_LOG_EMCOMM_ASSIGNMENT_RELEASED, OFFICIAL_LOG_EMCOMM_ASSIGNMENT_UPDATED,
    OFFICIAL_LOG_EMCOMM_INCIDENT_CLOSED, OFFICIAL_LOG_EMCOMM_INCIDENT_OPENED,
    OFFICIAL_LOG_EMCOMM_INCIDENT_UPDATED, OFFICIAL_LOG_EMCOMM_MESSAGE_ACKNOWLEDGED,
    OFFICIAL_LOG_EMCOMM_MESSAGE_CANCELLED, OFFICIAL_LOG_EMCOMM_MESSAGE_CREATED,
    OFFICIAL_LOG_EMCOMM_MESSAGE_RECEIVED, OFFICIAL_LOG_EMCOMM_MESSAGE_TRANSMITTED,
    OFFICIAL_LOG_EMCOMM_MESSAGE_UPDATED, OFFICIAL_LOG_EMCOMM_PERIOD_CLOSED,
    OFFICIAL_LOG_EMCOMM_PERIOD_OPENED, OFFICIAL_LOG_EMCOMM_PERSON_CHECKED_IN,
    OFFICIAL_LOG_EMCOMM_PERSON_CHECKED_OUT, OFFICIAL_LOG_EMCOMM_PERSON_UPDATED,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use thiserror::Error;
use uuid::Uuid;

use crate::{CoreEventEnvelope, Projection};

/// Schema version of the EmComm record payloads defined here.
pub const EMCOMM_SCHEMA_VERSION: u32 = 1;

/// The ICS forms the v1 record model is designed to produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IcsForm {
    /// Incident check-in list.
    #[serde(rename = "ICS-211")]
    Ics211,
    /// General message.
    #[serde(rename = "ICS-213")]
    Ics213,
    /// Resource request.
    #[serde(rename = "ICS-213RR")]
    Ics213Rr,
    /// Activity log.
    #[serde(rename = "ICS-214")]
    Ics214,
}

impl IcsForm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ics211 => "ICS-211",
            Self::Ics213 => "ICS-213",
            Self::Ics213Rr => "ICS-213RR",
            Self::Ics214 => "ICS-214",
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EmCommProjectionError {
    #[error("event payload must be a JSON object")]
    PayloadNotObject,
    #[error("event payload is missing required field `{0}`")]
    MissingField(&'static str),
    #[error("event payload field `{field}` is not a valid uuid")]
    InvalidUuid { field: &'static str },
    #[error("message number `{0}` is not a valid station-scoped message number")]
    InvalidMessageNumber(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentStatus {
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationalPeriodStatus {
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonStatus {
    CheckedIn,
    CheckedOut,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentStatus {
    Active,
    Released,
}

/// Where a message currently stands. A message never leaves a terminal state
/// by mutation; a later correction appends a new state entry instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageStatus {
    Draft,
    Transmitted,
    Received,
    Acknowledged,
    Cancelled,
}

/// ICS message precedence, most urgent first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessagePrecedence {
    Emergency,
    Priority,
    Immediate,
    Routine,
}

impl MessagePrecedence {
    fn from_payload(payload: &Map<String, Value>) -> Self {
        match payload
            .get("precedence")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "emergency" => Self::Emergency,
            "priority" => Self::Priority,
            "immediate" => Self::Immediate,
            _ => Self::Routine,
        }
    }
}

/// A station-scoped message number that is safe to allocate offline.
///
/// The station prefix is the originating station's own identifier, so two
/// stations that never meet cannot mint the same number.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct MessageNumber {
    pub station_prefix: String,
    pub sequence: u32,
}

impl MessageNumber {
    /// Allocate the number for `sequence` at the station identified by
    /// `station_prefix`, which is normally the operator's tactical or station
    /// callsign.
    pub fn new(station_prefix: impl AsRef<str>, sequence: u32) -> Self {
        Self {
            station_prefix: station_prefix
                .as_ref()
                .trim()
                .to_ascii_uppercase()
                .replace('-', ""),
            sequence,
        }
    }

    pub fn parse(value: &str) -> Result<Self, EmCommProjectionError> {
        let invalid = || EmCommProjectionError::InvalidMessageNumber(value.to_owned());
        let (prefix, sequence) = value.rsplit_once('-').ok_or_else(invalid)?;
        if prefix.trim().is_empty() {
            return Err(invalid());
        }
        let sequence: u32 = sequence.parse().map_err(|_| invalid())?;
        if sequence == 0 {
            return Err(invalid());
        }
        Ok(Self {
            station_prefix: prefix.to_ascii_uppercase(),
            sequence,
        })
    }
}

impl std::fmt::Display for MessageNumber {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}-{:04}", self.station_prefix, self.sequence)
    }
}

/// One entry in a record's audit trail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecordChange {
    /// The official event type that produced this entry.
    pub event_type: String,
    pub event_hash: String,
    pub recorded_at: DateTime<Utc>,
    /// The payload exactly as it was appended, before merging.
    pub payload: Value,
}

/// Common shape for every projected EmComm record.
///
/// `payload` is the merged current view. `history` is every event that built
/// it, in order, so a correction never erases what it corrected.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmCommRecord<S> {
    pub payload: Value,
    pub status: S,
    pub history: Vec<RecordChange>,
    pub last_event_hash: String,
}

impl<S> EmCommRecord<S> {
    fn new(status: S, event: &CoreEventEnvelope, payload: Value) -> Self {
        Self {
            payload,
            status,
            history: vec![change_from(event)],
            last_event_hash: event.event_hash.clone(),
        }
    }

    fn append(&mut self, event: &CoreEventEnvelope, status: S) -> Result<(), EmCommProjectionError>
    where
        S: Copy,
    {
        merge_payload(&mut self.payload, &event.payload)?;
        self.history.push(change_from(event));
        self.status = status;
        self.last_event_hash = event.event_hash.clone();
        Ok(())
    }

    /// Every event that produced this record, oldest first.
    pub fn history(&self) -> &[RecordChange] {
        &self.history
    }

    pub fn field(&self, key: &str) -> Option<&Value> {
        self.payload.get(key)
    }

    pub fn text(&self, key: &str) -> Option<&str> {
        self.payload.get(key).and_then(Value::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IncidentRecord {
    pub incident_id: Uuid,
    #[serde(flatten)]
    pub record: EmCommRecord<IncidentStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OperationalPeriodRecord {
    pub period_id: Uuid,
    pub incident_id: Uuid,
    #[serde(flatten)]
    pub record: EmCommRecord<OperationalPeriodStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonRecord {
    pub person_id: Uuid,
    pub incident_id: Uuid,
    pub period_id: Option<Uuid>,
    #[serde(flatten)]
    pub record: EmCommRecord<PersonStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AssignmentRecord {
    pub assignment_id: Uuid,
    pub incident_id: Uuid,
    pub person_id: Uuid,
    #[serde(flatten)]
    pub record: EmCommRecord<AssignmentStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageRecord {
    pub message_id: Uuid,
    pub incident_id: Uuid,
    pub message_number: MessageNumber,
    pub precedence: MessagePrecedence,
    pub form: IcsForm,
    #[serde(flatten)]
    pub record: EmCommRecord<MessageStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityLogEntry {
    pub activity_id: Uuid,
    pub incident_id: Uuid,
    pub period_id: Option<Uuid>,
    pub occurred_at: Option<DateTime<Utc>>,
    pub payload: Value,
    pub event_hash: String,
}

/// Rebuildable current state for one or more incidents.
///
/// The projection is derived entirely from the official event chain, so
/// replaying the chain on another client produces the same records.
#[derive(Debug, Default)]
pub struct EmCommProjection {
    incidents: HashMap<Uuid, IncidentRecord>,
    periods: HashMap<Uuid, OperationalPeriodRecord>,
    people: HashMap<Uuid, PersonRecord>,
    assignments: HashMap<Uuid, AssignmentRecord>,
    messages: HashMap<Uuid, MessageRecord>,
    activity: Vec<ActivityLogEntry>,
}

impl EmCommProjection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn incidents(&self, include_closed: bool) -> Vec<&IncidentRecord> {
        let mut incidents = self
            .incidents
            .values()
            .filter(|incident| include_closed || incident.record.status == IncidentStatus::Open)
            .collect::<Vec<_>>();
        incidents.sort_by_key(|incident| incident.incident_id);
        incidents
    }

    pub fn incident(&self, incident_id: Uuid) -> Option<&IncidentRecord> {
        self.incidents.get(&incident_id)
    }

    pub fn periods_for_incident(&self, incident_id: Uuid) -> Vec<&OperationalPeriodRecord> {
        let mut periods = self
            .periods
            .values()
            .filter(|period| period.incident_id == incident_id)
            .collect::<Vec<_>>();
        periods.sort_by_key(|period| period.period_id);
        periods
    }

    pub fn open_period(&self, incident_id: Uuid) -> Option<&OperationalPeriodRecord> {
        self.periods.values().find(|period| {
            period.incident_id == incident_id
                && period.record.status == OperationalPeriodStatus::Open
        })
    }

    /// People on the ICS 211 check-in list for an incident.
    pub fn people_for_incident(
        &self,
        incident_id: Uuid,
        include_checked_out: bool,
    ) -> Vec<&PersonRecord> {
        let mut people = self
            .people
            .values()
            .filter(|person| {
                person.incident_id == incident_id
                    && (include_checked_out || person.record.status == PersonStatus::CheckedIn)
            })
            .collect::<Vec<_>>();
        people.sort_by_key(|person| person.person_id);
        people
    }

    pub fn person(&self, person_id: Uuid) -> Option<&PersonRecord> {
        self.people.get(&person_id)
    }

    pub fn assignments_for_person(&self, person_id: Uuid) -> Vec<&AssignmentRecord> {
        let mut assignments = self
            .assignments
            .values()
            .filter(|assignment| assignment.person_id == person_id)
            .collect::<Vec<_>>();
        assignments.sort_by_key(|assignment| assignment.assignment_id);
        assignments
    }

    pub fn assignment(&self, assignment_id: Uuid) -> Option<&AssignmentRecord> {
        self.assignments.get(&assignment_id)
    }

    /// Messages for an incident, most urgent first and then by number.
    pub fn messages_for_incident(&self, incident_id: Uuid) -> Vec<&MessageRecord> {
        let mut messages = self
            .messages
            .values()
            .filter(|message| message.incident_id == incident_id)
            .collect::<Vec<_>>();
        messages.sort_by(|left, right| {
            left.precedence
                .cmp(&right.precedence)
                .then_with(|| left.message_number.cmp(&right.message_number))
        });
        messages
    }

    pub fn message(&self, message_id: Uuid) -> Option<&MessageRecord> {
        self.messages.get(&message_id)
    }

    /// Messages that have been sent but not yet acknowledged.
    pub fn unacknowledged_messages(&self, incident_id: Uuid) -> Vec<&MessageRecord> {
        self.messages_for_incident(incident_id)
            .into_iter()
            .filter(|message| message.record.status == MessageStatus::Transmitted)
            .collect()
    }

    /// The ICS 214 activity log for an incident, in the order it was recorded.
    pub fn activity_for_incident(&self, incident_id: Uuid) -> Vec<&ActivityLogEntry> {
        self.activity
            .iter()
            .filter(|entry| entry.incident_id == incident_id)
            .collect()
    }

    /// The activity log for one operational period.
    pub fn activity_for_period(&self, period_id: Uuid) -> Vec<&ActivityLogEntry> {
        self.activity
            .iter()
            .filter(|entry| entry.period_id == Some(period_id))
            .collect()
    }

    /// The next message number this station should use for an incident.
    ///
    /// Only numbers minted by `station_prefix` are considered, so the sequence
    /// stays correct while disconnected from other stations.
    pub fn next_message_number(
        &self,
        incident_id: Uuid,
        station_prefix: impl AsRef<str>,
    ) -> MessageNumber {
        let prefix = MessageNumber::new(station_prefix, 1).station_prefix;
        let highest = self
            .messages
            .values()
            .filter(|message| {
                message.incident_id == incident_id
                    && message.message_number.station_prefix == prefix
            })
            .map(|message| message.message_number.sequence)
            .max()
            .unwrap_or(0);
        MessageNumber {
            station_prefix: prefix,
            sequence: highest.saturating_add(1),
        }
    }

    /// A complete, ordered audit package for one incident.
    ///
    /// Every record carries its own event history, so the export preserves how
    /// the incident actually unfolded rather than only how it ended.
    pub fn incident_package(&self, incident_id: Uuid) -> Option<Value> {
        let incident = self.incidents.get(&incident_id)?;
        Some(json!({
            "schema_version": EMCOMM_SCHEMA_VERSION,
            "incident": incident,
            "operational_periods": self.periods_for_incident(incident_id),
            "personnel": self.people_for_incident(incident_id, true),
            "assignments": self
                .assignments
                .values()
                .filter(|assignment| assignment.incident_id == incident_id)
                .collect::<Vec<_>>(),
            "messages": self.messages_for_incident(incident_id),
            "activity_log": self.activity_for_incident(incident_id),
            "forms": [
                IcsForm::Ics211,
                IcsForm::Ics213,
                IcsForm::Ics213Rr,
                IcsForm::Ics214,
            ],
        }))
    }

    /// Counts suitable for a dashboard or a runtime event.
    pub fn summary(&self, incident_id: Uuid) -> Value {
        let messages = self.messages_for_incident(incident_id);
        let mut by_status: BTreeMap<&str, usize> = BTreeMap::new();
        for message in &messages {
            let key = match message.record.status {
                MessageStatus::Draft => "draft",
                MessageStatus::Transmitted => "transmitted",
                MessageStatus::Received => "received",
                MessageStatus::Acknowledged => "acknowledged",
                MessageStatus::Cancelled => "cancelled",
            };
            *by_status.entry(key).or_default() += 1;
        }
        json!({
            "incident_id": incident_id,
            "operational_periods": self.periods_for_incident(incident_id).len(),
            "checked_in": self.people_for_incident(incident_id, false).len(),
            "roster": self.people_for_incident(incident_id, true).len(),
            "messages": messages.len(),
            "messages_by_status": by_status,
            "activity_entries": self.activity_for_incident(incident_id).len(),
        })
    }
}

impl Projection for EmCommProjection {
    type Error = EmCommProjectionError;

    fn clear(&mut self) {
        *self = Self::default();
    }

    fn apply(&mut self, event: &CoreEventEnvelope) -> Result<(), Self::Error> {
        let Some(entity_id) = event.entity_id else {
            return Ok(());
        };
        if !event.event_type.starts_with("official.log.emcomm.") {
            return Ok(());
        }
        let payload = event
            .payload
            .as_object()
            .ok_or(EmCommProjectionError::PayloadNotObject)?;

        match event.event_type.as_str() {
            OFFICIAL_LOG_EMCOMM_INCIDENT_OPENED => {
                let mut stored = event.payload.clone();
                stored["incident_id"] = json!(entity_id);
                self.incidents.insert(
                    entity_id,
                    IncidentRecord {
                        incident_id: entity_id,
                        record: EmCommRecord::new(IncidentStatus::Open, event, stored),
                    },
                );
            }
            OFFICIAL_LOG_EMCOMM_INCIDENT_UPDATED => {
                if let Some(incident) = self.incidents.get_mut(&entity_id) {
                    let status = incident.record.status;
                    incident.record.append(event, status)?;
                }
            }
            OFFICIAL_LOG_EMCOMM_INCIDENT_CLOSED => {
                if let Some(incident) = self.incidents.get_mut(&entity_id) {
                    incident.record.append(event, IncidentStatus::Closed)?;
                }
            }
            OFFICIAL_LOG_EMCOMM_PERIOD_OPENED => {
                let incident_id = required_uuid(payload, "incident_id")?;
                let mut stored = event.payload.clone();
                stored["period_id"] = json!(entity_id);
                self.periods.insert(
                    entity_id,
                    OperationalPeriodRecord {
                        period_id: entity_id,
                        incident_id,
                        record: EmCommRecord::new(OperationalPeriodStatus::Open, event, stored),
                    },
                );
            }
            OFFICIAL_LOG_EMCOMM_PERIOD_CLOSED => {
                if let Some(period) = self.periods.get_mut(&entity_id) {
                    period
                        .record
                        .append(event, OperationalPeriodStatus::Closed)?;
                }
            }
            OFFICIAL_LOG_EMCOMM_PERSON_CHECKED_IN => {
                let incident_id = required_uuid(payload, "incident_id")?;
                let period_id = optional_uuid(payload, "period_id")?;
                let mut stored = event.payload.clone();
                stored["person_id"] = json!(entity_id);
                self.people.insert(
                    entity_id,
                    PersonRecord {
                        person_id: entity_id,
                        incident_id,
                        period_id,
                        record: EmCommRecord::new(PersonStatus::CheckedIn, event, stored),
                    },
                );
            }
            OFFICIAL_LOG_EMCOMM_PERSON_UPDATED => {
                if let Some(person) = self.people.get_mut(&entity_id) {
                    let status = person.record.status;
                    person.record.append(event, status)?;
                    if let Some(period_id) = optional_uuid(payload, "period_id")? {
                        person.period_id = Some(period_id);
                    }
                }
            }
            OFFICIAL_LOG_EMCOMM_PERSON_CHECKED_OUT => {
                if let Some(person) = self.people.get_mut(&entity_id) {
                    person.record.append(event, PersonStatus::CheckedOut)?;
                }
            }
            OFFICIAL_LOG_EMCOMM_ASSIGNMENT_CREATED => {
                let incident_id = required_uuid(payload, "incident_id")?;
                let person_id = required_uuid(payload, "person_id")?;
                let mut stored = event.payload.clone();
                stored["assignment_id"] = json!(entity_id);
                self.assignments.insert(
                    entity_id,
                    AssignmentRecord {
                        assignment_id: entity_id,
                        incident_id,
                        person_id,
                        record: EmCommRecord::new(AssignmentStatus::Active, event, stored),
                    },
                );
            }
            OFFICIAL_LOG_EMCOMM_ASSIGNMENT_UPDATED => {
                if let Some(assignment) = self.assignments.get_mut(&entity_id) {
                    let status = assignment.record.status;
                    assignment.record.append(event, status)?;
                }
            }
            OFFICIAL_LOG_EMCOMM_ASSIGNMENT_RELEASED => {
                if let Some(assignment) = self.assignments.get_mut(&entity_id) {
                    assignment
                        .record
                        .append(event, AssignmentStatus::Released)?;
                }
            }
            OFFICIAL_LOG_EMCOMM_MESSAGE_CREATED => {
                let incident_id = required_uuid(payload, "incident_id")?;
                let message_number = MessageNumber::parse(
                    payload
                        .get("message_number")
                        .and_then(Value::as_str)
                        .ok_or(EmCommProjectionError::MissingField("message_number"))?,
                )?;
                let mut stored = event.payload.clone();
                stored["message_id"] = json!(entity_id);
                self.messages.insert(
                    entity_id,
                    MessageRecord {
                        message_id: entity_id,
                        incident_id,
                        message_number,
                        precedence: MessagePrecedence::from_payload(payload),
                        form: form_from_payload(payload),
                        record: EmCommRecord::new(MessageStatus::Draft, event, stored),
                    },
                );
            }
            OFFICIAL_LOG_EMCOMM_MESSAGE_UPDATED => {
                if let Some(message) = self.messages.get_mut(&entity_id) {
                    let status = message.record.status;
                    message.record.append(event, status)?;
                    if payload.contains_key("precedence") {
                        message.precedence = MessagePrecedence::from_payload(payload);
                    }
                }
            }
            OFFICIAL_LOG_EMCOMM_MESSAGE_TRANSMITTED => {
                self.advance_message(entity_id, event, MessageStatus::Transmitted)?;
            }
            OFFICIAL_LOG_EMCOMM_MESSAGE_RECEIVED => {
                self.advance_message(entity_id, event, MessageStatus::Received)?;
            }
            OFFICIAL_LOG_EMCOMM_MESSAGE_ACKNOWLEDGED => {
                self.advance_message(entity_id, event, MessageStatus::Acknowledged)?;
            }
            OFFICIAL_LOG_EMCOMM_MESSAGE_CANCELLED => {
                self.advance_message(entity_id, event, MessageStatus::Cancelled)?;
            }
            OFFICIAL_LOG_EMCOMM_ACTIVITY_LOGGED => {
                let incident_id = required_uuid(payload, "incident_id")?;
                let mut stored = event.payload.clone();
                stored["activity_id"] = json!(entity_id);
                self.activity.push(ActivityLogEntry {
                    activity_id: entity_id,
                    incident_id,
                    period_id: optional_uuid(payload, "period_id")?,
                    occurred_at: payload
                        .get("occurred_at")
                        .and_then(Value::as_str)
                        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
                        .map(|value| value.with_timezone(&Utc)),
                    payload: stored,
                    event_hash: event.event_hash.clone(),
                });
            }
            _ => {}
        }

        Ok(())
    }
}

impl EmCommProjection {
    fn advance_message(
        &mut self,
        message_id: Uuid,
        event: &CoreEventEnvelope,
        status: MessageStatus,
    ) -> Result<(), EmCommProjectionError> {
        if let Some(message) = self.messages.get_mut(&message_id) {
            message.record.append(event, status)?;
        }
        Ok(())
    }
}

fn change_from(event: &CoreEventEnvelope) -> RecordChange {
    RecordChange {
        event_type: event.event_type.clone(),
        event_hash: event.event_hash.clone(),
        recorded_at: event.timestamp,
        payload: event.payload.clone(),
    }
}

fn merge_payload(target: &mut Value, patch: &Value) -> Result<(), EmCommProjectionError> {
    let patch = patch
        .as_object()
        .ok_or(EmCommProjectionError::PayloadNotObject)?;
    let target = target
        .as_object_mut()
        .ok_or(EmCommProjectionError::PayloadNotObject)?;
    for (key, value) in patch {
        target.insert(key.clone(), value.clone());
    }
    Ok(())
}

fn required_uuid(
    payload: &Map<String, Value>,
    field: &'static str,
) -> Result<Uuid, EmCommProjectionError> {
    let raw = payload
        .get(field)
        .and_then(Value::as_str)
        .ok_or(EmCommProjectionError::MissingField(field))?;
    Uuid::parse_str(raw).map_err(|_| EmCommProjectionError::InvalidUuid { field })
}

fn optional_uuid(
    payload: &Map<String, Value>,
    field: &'static str,
) -> Result<Option<Uuid>, EmCommProjectionError> {
    match payload.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let raw = value
                .as_str()
                .ok_or(EmCommProjectionError::InvalidUuid { field })?;
            Uuid::parse_str(raw)
                .map(Some)
                .map_err(|_| EmCommProjectionError::InvalidUuid { field })
        }
    }
}

fn form_from_payload(payload: &Map<String, Value>) -> IcsForm {
    match payload
        .get("form")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_ascii_uppercase()
        .as_str()
    {
        "ICS-213RR" | "ICS213RR" | "213RR" => IcsForm::Ics213Rr,
        "ICS-211" | "ICS211" | "211" => IcsForm::Ics211,
        "ICS-214" | "ICS214" | "214" => IcsForm::Ics214,
        _ => IcsForm::Ics213,
    }
}
