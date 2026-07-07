//! Net Control MVP models, projection, and report export.
//!
//! Net Control is modeled as append-only official events. This module contains
//! the rebuildable current-state projection used by the GUI and tests.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Utc};
use ham_plugin_sdk::{
    OFFICIAL_LOG_NET_CHECKIN_CREATED, OFFICIAL_LOG_NET_CHECKIN_DELETED,
    OFFICIAL_LOG_NET_CHECKIN_UPDATED, OFFICIAL_LOG_NET_REPORT_EXPORTED,
    OFFICIAL_LOG_NET_SESSION_CANCELLED, OFFICIAL_LOG_NET_SESSION_ENDED,
    OFFICIAL_LOG_NET_SESSION_STARTED, OFFICIAL_LOG_NET_TEMPLATE_CREATED,
    OFFICIAL_LOG_NET_TEMPLATE_UPDATED, OFFICIAL_LOG_NET_TRAFFIC_CREATED,
    OFFICIAL_LOG_NET_TRAFFIC_UPDATED,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use thiserror::Error;
use uuid::Uuid;

use crate::{CoreEventEnvelope, Projection};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetSessionStatus {
    Planned,
    Active,
    Ended,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetCheckInStatus {
    CheckedIn,
    Late,
    Excused,
    Left,
    Duplicate,
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetTrafficLevel {
    None,
    Listed,
    Priority,
    Emergency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetTrafficPrecedence {
    Routine,
    Priority,
    Emergency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NetTrafficStatus {
    Listed,
    Passed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetTemplate {
    pub net_template_id: Uuid,
    pub account_id: String,
    pub name: String,
    pub description: Option<String>,
    pub default_frequency_hz: Option<u64>,
    pub default_band: Option<String>,
    pub default_mode: Option<String>,
    pub default_schedule: Option<String>,
    pub default_script: Option<String>,
    pub default_roster: Vec<String>,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetSessionRecord {
    pub net_session_id: Uuid,
    pub payload: Value,
    pub status: NetSessionStatus,
    pub checkin_ids: HashSet<Uuid>,
    pub traffic_ids: HashSet<Uuid>,
    pub checkin_count: usize,
    pub late_checkin_count: usize,
    pub traffic_count: usize,
    pub emergency_traffic_count: usize,
    pub duplicate_warnings: Vec<String>,
    pub last_event_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetCheckInRecord {
    pub checkin_id: Uuid,
    pub net_session_id: Uuid,
    pub payload: Value,
    pub status: NetCheckInStatus,
    pub traffic: NetTrafficLevel,
    pub deleted: bool,
    pub last_event_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetTrafficRecord {
    pub traffic_id: Uuid,
    pub net_session_id: Uuid,
    pub payload: Value,
    pub precedence: NetTrafficPrecedence,
    pub status: NetTrafficStatus,
    pub last_event_hash: String,
}

#[derive(Debug, Default)]
pub struct NetControlProjection {
    templates: HashMap<Uuid, Value>,
    sessions: HashMap<Uuid, NetSessionRecord>,
    checkins: HashMap<Uuid, NetCheckInRecord>,
    traffic: HashMap<Uuid, NetTrafficRecord>,
    exported_reports: Vec<Value>,
}

impl NetControlProjection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn templates(&self) -> Vec<&Value> {
        self.templates.values().collect()
    }

    pub fn sessions(&self, include_ended_cancelled: bool) -> Vec<&NetSessionRecord> {
        self.sessions
            .values()
            .filter(|session| {
                include_ended_cancelled
                    || matches!(
                        session.status,
                        NetSessionStatus::Planned | NetSessionStatus::Active
                    )
            })
            .collect()
    }

    pub fn get_session(&self, session_id: Uuid) -> Option<&NetSessionRecord> {
        self.sessions.get(&session_id)
    }

    pub fn active_session(&self) -> Option<&NetSessionRecord> {
        self.sessions
            .values()
            .find(|session| session.status == NetSessionStatus::Active)
    }

    pub fn checkins_for_session(
        &self,
        net_session_id: Uuid,
        include_deleted: bool,
    ) -> Vec<&NetCheckInRecord> {
        let mut checkins = self
            .checkins
            .values()
            .filter(|checkin| {
                checkin.net_session_id == net_session_id && (include_deleted || !checkin.deleted)
            })
            .collect::<Vec<_>>();
        checkins.sort_by(|a, b| {
            a.payload
                .get("checkin_time")
                .and_then(Value::as_str)
                .cmp(&b.payload.get("checkin_time").and_then(Value::as_str))
        });
        checkins
    }

    pub fn traffic_for_session(&self, net_session_id: Uuid) -> Vec<&NetTrafficRecord> {
        self.traffic
            .values()
            .filter(|traffic| traffic.net_session_id == net_session_id)
            .collect()
    }

    pub fn checkin_exists_in_session(&self, net_session_id: Uuid, callsign: &str) -> bool {
        self.checkins.values().any(|checkin| {
            checkin.net_session_id == net_session_id
                && !checkin.deleted
                && checkin
                    .payload
                    .get("callsign")
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.eq_ignore_ascii_case(callsign))
        })
    }

    pub fn exported_reports(&self) -> &[Value] {
        &self.exported_reports
    }

    fn recompute_stats(&mut self) {
        for session in self.sessions.values_mut() {
            session.checkin_ids.clear();
            session.traffic_ids.clear();
            session.checkin_count = 0;
            session.late_checkin_count = 0;
            session.traffic_count = 0;
            session.emergency_traffic_count = 0;
            session.duplicate_warnings.clear();

            let mut seen_callsigns = HashSet::new();
            for checkin in self.checkins.values() {
                if checkin.net_session_id != session.net_session_id || checkin.deleted {
                    continue;
                }
                session.checkin_ids.insert(checkin.checkin_id);
                session.checkin_count += 1;
                if checkin.status == NetCheckInStatus::Late {
                    session.late_checkin_count += 1;
                }
                if let Some(callsign) = checkin.payload.get("callsign").and_then(Value::as_str) {
                    let normalized = callsign.to_ascii_uppercase();
                    if !seen_callsigns.insert(normalized.clone()) {
                        session
                            .duplicate_warnings
                            .push(format!("Duplicate check-in: {normalized}"));
                    }
                }
            }

            for traffic in self.traffic.values() {
                if traffic.net_session_id != session.net_session_id {
                    continue;
                }
                session.traffic_ids.insert(traffic.traffic_id);
                session.traffic_count += 1;
                if traffic.precedence == NetTrafficPrecedence::Emergency {
                    session.emergency_traffic_count += 1;
                }
            }
        }
    }
}

impl Projection for NetControlProjection {
    type Error = NetProjectionError;

    fn apply(&mut self, event: &CoreEventEnvelope) -> Result<(), Self::Error> {
        let Some(entity_id) = event.entity_id else {
            return Ok(());
        };

        match event.event_type.as_str() {
            OFFICIAL_LOG_NET_TEMPLATE_CREATED => {
                let mut payload = event.payload.clone();
                payload["net_template_id"] = json!(entity_id);
                self.templates.insert(entity_id, payload);
            }
            OFFICIAL_LOG_NET_TEMPLATE_UPDATED => {
                if let Some(template) = self.templates.get_mut(&entity_id) {
                    merge_json_object(template, &event.payload)?;
                }
            }
            OFFICIAL_LOG_NET_SESSION_STARTED => {
                let mut payload = event.payload.clone();
                payload["net_session_id"] = json!(entity_id);
                self.sessions.insert(
                    entity_id,
                    NetSessionRecord {
                        net_session_id: entity_id,
                        payload,
                        status: NetSessionStatus::Active,
                        checkin_ids: HashSet::new(),
                        traffic_ids: HashSet::new(),
                        checkin_count: 0,
                        late_checkin_count: 0,
                        traffic_count: 0,
                        emergency_traffic_count: 0,
                        duplicate_warnings: Vec::new(),
                        last_event_hash: event.event_hash.clone(),
                    },
                );
            }
            OFFICIAL_LOG_NET_SESSION_ENDED => {
                if let Some(session) = self.sessions.get_mut(&entity_id) {
                    merge_json_object(&mut session.payload, &event.payload)?;
                    session.status = NetSessionStatus::Ended;
                    session.last_event_hash = event.event_hash.clone();
                }
            }
            OFFICIAL_LOG_NET_SESSION_CANCELLED => {
                if let Some(session) = self.sessions.get_mut(&entity_id) {
                    merge_json_object(&mut session.payload, &event.payload)?;
                    session.status = NetSessionStatus::Cancelled;
                    session.last_event_hash = event.event_hash.clone();
                }
            }
            OFFICIAL_LOG_NET_CHECKIN_CREATED => {
                let net_session_id = uuid_from_payload(&event.payload, "net_session_id")?;
                let status = checkin_status_from_payload(&event.payload);
                let traffic = traffic_level_from_payload(&event.payload);
                let mut payload = event.payload.clone();
                payload["checkin_id"] = json!(entity_id);
                self.checkins.insert(
                    entity_id,
                    NetCheckInRecord {
                        checkin_id: entity_id,
                        net_session_id,
                        payload,
                        status,
                        traffic,
                        deleted: false,
                        last_event_hash: event.event_hash.clone(),
                    },
                );
            }
            OFFICIAL_LOG_NET_CHECKIN_UPDATED => {
                if let Some(checkin) = self.checkins.get_mut(&entity_id) {
                    merge_json_object(&mut checkin.payload, &event.payload)?;
                    checkin.status = checkin_status_from_payload(&checkin.payload);
                    checkin.traffic = traffic_level_from_payload(&checkin.payload);
                    checkin.last_event_hash = event.event_hash.clone();
                }
            }
            OFFICIAL_LOG_NET_CHECKIN_DELETED => {
                if let Some(checkin) = self.checkins.get_mut(&entity_id) {
                    checkin.deleted = true;
                    checkin.status = NetCheckInStatus::Deleted;
                    checkin.last_event_hash = event.event_hash.clone();
                }
            }
            OFFICIAL_LOG_NET_TRAFFIC_CREATED => {
                let net_session_id = uuid_from_payload(&event.payload, "net_session_id")?;
                let precedence = traffic_precedence_from_payload(&event.payload);
                let status = traffic_status_from_payload(&event.payload);
                let mut payload = event.payload.clone();
                payload["traffic_id"] = json!(entity_id);
                self.traffic.insert(
                    entity_id,
                    NetTrafficRecord {
                        traffic_id: entity_id,
                        net_session_id,
                        payload,
                        precedence,
                        status,
                        last_event_hash: event.event_hash.clone(),
                    },
                );
            }
            OFFICIAL_LOG_NET_TRAFFIC_UPDATED => {
                if let Some(traffic) = self.traffic.get_mut(&entity_id) {
                    merge_json_object(&mut traffic.payload, &event.payload)?;
                    traffic.precedence = traffic_precedence_from_payload(&traffic.payload);
                    traffic.status = traffic_status_from_payload(&traffic.payload);
                    traffic.last_event_hash = event.event_hash.clone();
                }
            }
            OFFICIAL_LOG_NET_REPORT_EXPORTED => {
                self.exported_reports.push(event.payload.clone());
            }
            _ => {}
        }
        self.recompute_stats();
        Ok(())
    }

    fn clear(&mut self) {
        self.templates.clear();
        self.sessions.clear();
        self.checkins.clear();
        self.traffic.clear();
        self.exported_reports.clear();
    }
}

#[derive(Debug, Error)]
pub enum NetProjectionError {
    #[error("net event payload must be an object")]
    PayloadMustBeObject,
    #[error("net payload field {0} must be a uuid string")]
    InvalidUuid(String),
}

pub fn export_net_report_markdown(
    projection: &NetControlProjection,
    net_session_id: Uuid,
) -> Result<String, NetProjectionError> {
    let session = projection
        .get_session(net_session_id)
        .ok_or_else(|| NetProjectionError::InvalidUuid("net_session_id".to_owned()))?;
    let net_name = session
        .payload
        .get("net_name")
        .and_then(Value::as_str)
        .unwrap_or("Net Session");
    let mut report = format!(
        "# {net_name}\n\nStatus: {:?}\nCheck-ins: {}\nLate: {}\nTraffic: {}\nEmergency traffic: {}\n\n",
        session.status,
        session.checkin_count,
        session.late_checkin_count,
        session.traffic_count,
        session.emergency_traffic_count
    );
    report.push_str("## Check-ins\n\n");
    for checkin in projection.checkins_for_session(net_session_id, false) {
        let callsign = checkin
            .payload
            .get("callsign")
            .and_then(Value::as_str)
            .unwrap_or("(tactical only)");
        let tactical = checkin
            .payload
            .get("tactical_callsign")
            .and_then(Value::as_str)
            .unwrap_or("");
        report.push_str(&format!(
            "- {callsign} {tactical} - {:?} / {:?}\n",
            checkin.status, checkin.traffic
        ));
    }
    Ok(report)
}

fn merge_json_object(target: &mut Value, patch: &Value) -> Result<(), NetProjectionError> {
    let Some(patch_object) = patch.as_object() else {
        return Err(NetProjectionError::PayloadMustBeObject);
    };
    if !target.is_object() {
        *target = Value::Object(Map::new());
    }
    let target_object = target.as_object_mut().expect("target is an object");
    for (key, value) in patch_object {
        target_object.insert(key.clone(), value.clone());
    }
    Ok(())
}

fn uuid_from_payload(payload: &Value, key: &str) -> Result<Uuid, NetProjectionError> {
    payload
        .get(key)
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok())
        .ok_or_else(|| NetProjectionError::InvalidUuid(key.to_owned()))
}

fn checkin_status_from_payload(payload: &Value) -> NetCheckInStatus {
    match payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("checked_in")
    {
        "late" => NetCheckInStatus::Late,
        "excused" => NetCheckInStatus::Excused,
        "left" => NetCheckInStatus::Left,
        "duplicate" => NetCheckInStatus::Duplicate,
        _ => NetCheckInStatus::CheckedIn,
    }
}

fn traffic_level_from_payload(payload: &Value) -> NetTrafficLevel {
    match payload
        .get("traffic")
        .and_then(Value::as_str)
        .unwrap_or("none")
    {
        "listed" => NetTrafficLevel::Listed,
        "priority" => NetTrafficLevel::Priority,
        "emergency" => NetTrafficLevel::Emergency,
        _ => NetTrafficLevel::None,
    }
}

fn traffic_precedence_from_payload(payload: &Value) -> NetTrafficPrecedence {
    match payload
        .get("precedence")
        .and_then(Value::as_str)
        .unwrap_or("routine")
    {
        "priority" => NetTrafficPrecedence::Priority,
        "emergency" => NetTrafficPrecedence::Emergency,
        _ => NetTrafficPrecedence::Routine,
    }
}

fn traffic_status_from_payload(payload: &Value) -> NetTrafficStatus {
    match payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("listed")
    {
        "passed" => NetTrafficStatus::Passed,
        "cancelled" => NetTrafficStatus::Cancelled,
        _ => NetTrafficStatus::Listed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NewLogbookEvent, Projection};

    fn event(
        event_type: &str,
        logbook_id: Uuid,
        entity_id: Uuid,
        payload: Value,
    ) -> CoreEventEnvelope {
        let device_id = Uuid::new_v4();
        CoreEventEnvelope::from_new(
            NewLogbookEvent {
                event_type: event_type.to_owned(),
                logbook_id,
                entity_id: Some(entity_id),
                author_operator_id: None,
                station_callsign: "KE8YGW".to_owned(),
                operator_callsign: Some("KE8YGW".to_owned()),
                author_device_id: device_id,
                source_device_id: device_id,
                correlation_id: Uuid::new_v4(),
                source_plugin_id: Some("plugin.net-control".to_owned()),
                schema_version: 1,
                payload,
            },
            None,
        )
    }

    #[test]
    fn projection_counts_checkins_and_emergency_traffic() {
        let logbook_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();
        let checkin_id = Uuid::new_v4();
        let traffic_id = Uuid::new_v4();
        let mut projection = NetControlProjection::new();
        projection
            .apply(&event(
                OFFICIAL_LOG_NET_SESSION_STARTED,
                logbook_id,
                session_id,
                json!({"station_callsign": "KE8YGW", "net_control_operator_id": "op", "net_name": "ARES Net", "started_at": "2026-07-06T00:00:00Z"}),
            ))
            .unwrap();
        projection
            .apply(&event(
                OFFICIAL_LOG_NET_CHECKIN_CREATED,
                logbook_id,
                checkin_id,
                json!({"net_session_id": session_id, "callsign": "K1ABC", "checkin_time": "2026-07-06T00:01:00Z", "status": "late", "traffic": "listed"}),
            ))
            .unwrap();
        projection
            .apply(&event(
                OFFICIAL_LOG_NET_TRAFFIC_CREATED,
                logbook_id,
                traffic_id,
                json!({"net_session_id": session_id, "precedence": "emergency", "summary": "test", "status": "listed"}),
            ))
            .unwrap();
        let session = projection.get_session(session_id).unwrap();
        assert_eq!(session.checkin_count, 1);
        assert_eq!(session.late_checkin_count, 1);
        assert_eq!(session.emergency_traffic_count, 1);
    }

    #[test]
    fn deleted_checkin_hidden_by_default() {
        let logbook_id = Uuid::new_v4();
        let session_id = Uuid::new_v4();
        let checkin_id = Uuid::new_v4();
        let mut projection = NetControlProjection::new();
        for event in [
            event(
                OFFICIAL_LOG_NET_SESSION_STARTED,
                logbook_id,
                session_id,
                json!({"station_callsign": "KE8YGW", "net_control_operator_id": "op", "net_name": "ARES Net", "started_at": "2026-07-06T00:00:00Z"}),
            ),
            event(
                OFFICIAL_LOG_NET_CHECKIN_CREATED,
                logbook_id,
                checkin_id,
                json!({"net_session_id": session_id, "callsign": "K1ABC", "checkin_time": "2026-07-06T00:01:00Z"}),
            ),
            event(
                OFFICIAL_LOG_NET_CHECKIN_DELETED,
                logbook_id,
                checkin_id,
                json!({"net_session_id": session_id, "reason": "duplicate"}),
            ),
        ] {
            projection.apply(&event).unwrap();
        }
        assert!(projection
            .checkins_for_session(session_id, false)
            .is_empty());
        assert_eq!(projection.checkins_for_session(session_id, true).len(), 1);
    }
}
