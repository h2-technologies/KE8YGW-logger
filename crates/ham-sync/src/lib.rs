//! Local-first LAN discovery and sync handshake primitives.

pub mod offline;
pub use offline::*;

use std::{
    collections::{HashMap, HashSet},
    io::ErrorKind,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    sync::Arc,
    time::{Duration, Instant},
};
#[cfg(feature = "surreal-storage")]
use std::{fs, path::PathBuf, thread};

use chrono::{DateTime, Utc};
#[cfg(feature = "surreal-storage")]
use ham_core::{default_log_directory, JsonlLogbookEventStore};
use ham_core::{
    validate_supported_remote_event, CoreEventEnvelope, InMemoryLogbookEventStore,
    LogbookEventStore, StoreError,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
#[cfg(feature = "surreal-storage")]
use sha2::{Digest, Sha256};
use socket2::{Domain, Protocol, SockAddr, Socket, Type};
#[cfg(feature = "surreal-storage")]
use surrealdb::{
    engine::{
        any::Any,
        local::{Db, SurrealKv},
    },
    opt::auth::Root,
    types::Value as SurrealDbValue,
    Surreal,
};
use thiserror::Error;
#[cfg(feature = "surreal-storage")]
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use uuid::Uuid;

pub const PROTOCOL_NAME: &str = "ke8ygw-logger-sync";
pub const PROTOCOL_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncConfig {
    pub enable_lan_discovery: bool,
    pub ipv4_multicast_address: Ipv4Addr,
    pub ipv6_multicast_address: Ipv6Addr,
    pub discovery_port: u16,
    pub local_sync_port: u16,
    pub peer_timeout_seconds: u64,
    pub discovery_interval_seconds: u64,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            enable_lan_discovery: true,
            ipv4_multicast_address: Ipv4Addr::new(239, 73, 89, 71),
            ipv6_multicast_address: Ipv6Addr::new(0xff12, 0, 0, 0, 0, 0, 0x73, 0x5947),
            discovery_port: 9737,
            local_sync_port: 9738,
            peer_timeout_seconds: 45,
            discovery_interval_seconds: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalPeerIdentity {
    pub device_id: Uuid,
    pub session_id: Uuid,
    pub user_hash: Option<String>,
    pub display_name: String,
    pub capabilities: Vec<String>,
    pub local_api_port: Option<u16>,
}

impl LocalPeerIdentity {
    pub fn new(display_name: impl Into<String>, local_api_port: Option<u16>) -> Self {
        Self {
            device_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            user_hash: None,
            display_name: display_name.into(),
            capabilities: vec![
                "discovery.v1".to_owned(),
                "handshake.v1".to_owned(),
                "head-compare.v1".to_owned(),
            ],
            local_api_port,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoveryPacket {
    pub protocol_name: String,
    pub protocol_version: u16,
    pub device_id: Uuid,
    pub session_id: Uuid,
    pub user_hash: Option<String>,
    pub display_name: String,
    pub capabilities: Vec<String>,
    pub local_api_port: Option<u16>,
    pub timestamp: DateTime<Utc>,
}

impl DiscoveryPacket {
    pub fn from_identity(identity: &LocalPeerIdentity) -> Self {
        Self {
            protocol_name: PROTOCOL_NAME.to_owned(),
            protocol_version: PROTOCOL_VERSION,
            device_id: identity.device_id,
            session_id: identity.session_id,
            user_hash: identity.user_hash.clone(),
            display_name: identity.display_name.clone(),
            capabilities: identity.capabilities.clone(),
            local_api_port: identity.local_api_port,
            timestamp: Utc::now(),
        }
    }

    pub fn peer_id(&self) -> String {
        format!("{}:{}", self.device_id, self.session_id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerConnectionState {
    Discovered,
    Handshaking,
    Connected,
    Unreachable,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerSyncState {
    Unknown,
    InSync,
    Diverged,
    LocalAhead,
    RemoteAhead,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRecord {
    pub peer_id: String,
    pub device_id: Uuid,
    pub session_id: Uuid,
    pub display_name: String,
    pub addresses: Vec<SocketAddr>,
    pub protocol_version: u16,
    pub capabilities: Vec<String>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub connection_state: PeerConnectionState,
    pub sync_state: PeerSyncState,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct PeerRegistry {
    peers: HashMap<String, PeerRecord>,
}

impl PeerRegistry {
    pub fn observe(
        &mut self,
        local: &LocalPeerIdentity,
        packet: DiscoveryPacket,
        address: SocketAddr,
    ) -> PeerObservation {
        if packet.protocol_name != PROTOCOL_NAME || packet.protocol_version != PROTOCOL_VERSION {
            return PeerObservation::IgnoredIncompatible;
        }
        if packet.device_id == local.device_id && packet.session_id == local.session_id {
            return PeerObservation::IgnoredSelf;
        }

        let address = packet
            .local_api_port
            .map(|port| SocketAddr::new(address.ip(), port))
            .unwrap_or(address);
        let peer_id = packet.peer_id();
        let now = Utc::now();
        match self.peers.get_mut(&peer_id) {
            Some(peer) => {
                if !peer.addresses.contains(&address) {
                    peer.addresses.push(address);
                }
                peer.display_name = packet.display_name;
                peer.capabilities = packet.capabilities;
                peer.last_seen = now;
                peer.connection_state = PeerConnectionState::Discovered;
                PeerObservation::Updated(peer_id)
            }
            None => {
                self.peers.insert(
                    peer_id.clone(),
                    PeerRecord {
                        peer_id: peer_id.clone(),
                        device_id: packet.device_id,
                        session_id: packet.session_id,
                        display_name: packet.display_name,
                        addresses: vec![address],
                        protocol_version: packet.protocol_version,
                        capabilities: packet.capabilities,
                        first_seen: now,
                        last_seen: now,
                        connection_state: PeerConnectionState::Discovered,
                        sync_state: PeerSyncState::Unknown,
                    },
                );
                PeerObservation::Discovered(peer_id)
            }
        }
    }

    pub fn expire_stale(&mut self, now: DateTime<Utc>, timeout: Duration) -> Vec<String> {
        let timeout =
            chrono::Duration::from_std(timeout).unwrap_or_else(|_| chrono::Duration::seconds(45));
        let mut expired = Vec::new();
        for peer in self.peers.values_mut() {
            if now - peer.last_seen > timeout
                && peer.connection_state != PeerConnectionState::Expired
            {
                peer.connection_state = PeerConnectionState::Expired;
                peer.sync_state = PeerSyncState::Unknown;
                expired.push(peer.peer_id.clone());
            }
        }
        expired
    }

    pub fn list(&self) -> Vec<PeerRecord> {
        let mut peers = self.peers.values().cloned().collect::<Vec<_>>();
        peers.sort_by(|a, b| a.display_name.cmp(&b.display_name));
        peers
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeerObservation {
    Discovered(String),
    Updated(String),
    IgnoredSelf,
    IgnoredIncompatible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogbookHeadSummary {
    pub logbook_id: Uuid,
    pub head_hash: Option<String>,
    pub event_count: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandshakeRequest {
    pub protocol_version: u16,
    pub device_id: Uuid,
    pub session_id: Uuid,
    pub supported_capabilities: Vec<String>,
    pub logbooks: Vec<LogbookHeadSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandshakeResponse {
    pub accepted: bool,
    pub reason: Option<String>,
    pub peer_device_id: Uuid,
    pub protocol_version: u16,
    pub supported_capabilities: Vec<String>,
    pub matching_logbooks: Vec<LogbookHeadComparison>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogbookHeadComparison {
    pub logbook_id: Uuid,
    pub local_head_hash: Option<String>,
    pub remote_head_hash: Option<String>,
    pub local_event_count: Option<u64>,
    pub remote_event_count: Option<u64>,
    pub status: HeadComparisonStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HeadComparisonStatus {
    Unknown,
    Match,
    LocalAhead,
    RemoteAhead,
    Diverged,
}

pub fn compare_heads(
    local: &LogbookHeadSummary,
    remote: &LogbookHeadSummary,
) -> HeadComparisonStatus {
    match (&local.head_hash, &remote.head_hash) {
        (None, None) => HeadComparisonStatus::Match,
        (Some(left), Some(right)) if left == right => HeadComparisonStatus::Match,
        (Some(_), None) => HeadComparisonStatus::LocalAhead,
        (None, Some(_)) => HeadComparisonStatus::RemoteAhead,
        (Some(_), Some(_)) => match (local.event_count, remote.event_count) {
            (Some(local_count), Some(remote_count)) if local_count > remote_count => {
                HeadComparisonStatus::LocalAhead
            }
            (Some(local_count), Some(remote_count)) if remote_count > local_count => {
                HeadComparisonStatus::RemoteAhead
            }
            (Some(_), Some(_)) => HeadComparisonStatus::Diverged,
            _ => HeadComparisonStatus::Unknown,
        },
    }
}

pub fn build_handshake_response(
    local_identity: &LocalPeerIdentity,
    local_heads: &[LogbookHeadSummary],
    request: &HandshakeRequest,
) -> HandshakeResponse {
    if request.protocol_version != PROTOCOL_VERSION {
        return HandshakeResponse {
            accepted: false,
            reason: Some("incompatible protocol version".to_owned()),
            peer_device_id: local_identity.device_id,
            protocol_version: PROTOCOL_VERSION,
            supported_capabilities: local_identity.capabilities.clone(),
            matching_logbooks: Vec::new(),
        };
    }

    let mut comparisons = Vec::new();
    for local in local_heads {
        if let Some(remote) = request
            .logbooks
            .iter()
            .find(|remote| remote.logbook_id == local.logbook_id)
        {
            comparisons.push(LogbookHeadComparison {
                logbook_id: local.logbook_id,
                local_head_hash: local.head_hash.clone(),
                remote_head_hash: remote.head_hash.clone(),
                local_event_count: local.event_count,
                remote_event_count: remote.event_count,
                status: compare_heads(local, remote),
            });
        }
    }

    HandshakeResponse {
        accepted: true,
        reason: None,
        peer_device_id: local_identity.device_id,
        protocol_version: PROTOCOL_VERSION,
        supported_capabilities: local_identity.capabilities.clone(),
        matching_logbooks: comparisons,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListLogbooksRequest {
    pub protocol_version: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListLogbooksResponse {
    pub logbooks: Vec<LogbookHeadSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetHeadRequest {
    pub logbook_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetEventRangeRequest {
    pub logbook_id: Uuid,
    pub after_hash: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMetadata {
    pub event_id: Uuid,
    pub logbook_id: Uuid,
    pub entity_id: Option<Uuid>,
    pub previous_hash: Option<String>,
    pub event_hash: String,
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub schema_version: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetEventRangeResponse {
    pub logbook_id: Uuid,
    pub events: Vec<CoreEventEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetEventMetadataRequest {
    pub logbook_id: Uuid,
    pub after_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GetEventMetadataResponse {
    pub logbook_id: Uuid,
    pub events: Vec<EventMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewPullRequest {
    pub peer_id: String,
    pub logbook_id: Uuid,
    pub local_head_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PreviewPullResponse {
    pub peer_id: String,
    pub logbook_id: Uuid,
    pub status: ReplicationStatus,
    pub local_head_hash: Option<String>,
    pub remote_head_hash: Option<String>,
    pub missing_event_count: usize,
    pub remote_event_count: usize,
    pub events: Vec<EventMetadata>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullEventsRequest {
    pub peer_id: String,
    pub logbook_id: Uuid,
    pub local_head_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullEventsResponse {
    pub peer_id: String,
    pub logbook_id: Uuid,
    pub status: ReplicationStatus,
    pub accepted_count: usize,
    pub ignored_duplicate_count: usize,
    pub rejected_count: usize,
    pub local_head_hash: Option<String>,
    pub remote_head_hash: Option<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplicationStatus {
    InSync,
    RemoteAhead,
    Pulled,
    Diverged,
    Rejected,
}

#[derive(Debug, Error)]
pub enum ReplicationError {
    #[error("remote event {event_id} is for logbook {actual_logbook_id}, expected {expected_logbook_id}")]
    LogbookMismatch {
        event_id: Uuid,
        expected_logbook_id: Uuid,
        actual_logbook_id: Uuid,
    },
    #[error(
        "remote event {event_id} previous hash {actual:?} does not match expected {expected:?}"
    )]
    PreviousHashMismatch {
        event_id: Uuid,
        expected: Option<String>,
        actual: Option<String>,
    },
    #[error("remote chain diverged from local head {local_head_hash:?}")]
    Diverged { local_head_hash: Option<String> },
    #[error("store error: {0}")]
    Store(#[from] StoreError),
}

pub fn metadata_for_event(event: &CoreEventEnvelope) -> EventMetadata {
    EventMetadata {
        event_id: event.event_id,
        logbook_id: event.logbook_id,
        entity_id: event.entity_id,
        previous_hash: event.previous_hash.clone(),
        event_hash: event.event_hash.clone(),
        timestamp: event.timestamp,
        event_type: event.event_type.clone(),
        schema_version: event.schema_version,
    }
}

pub fn preview_pull_from_events(
    request: PreviewPullRequest,
    remote_events: &[CoreEventEnvelope],
) -> PreviewPullResponse {
    let remote_events = events_for_logbook(remote_events, request.logbook_id);
    let remote_head_hash = remote_events.last().map(|event| event.event_hash.clone());

    if request.local_head_hash == remote_head_hash {
        return PreviewPullResponse {
            peer_id: request.peer_id,
            logbook_id: request.logbook_id,
            status: ReplicationStatus::InSync,
            local_head_hash: request.local_head_hash,
            remote_head_hash,
            missing_event_count: 0,
            remote_event_count: remote_events.len(),
            events: Vec::new(),
            message: "Local and remote heads match".to_owned(),
        };
    }

    let missing = match missing_events_after(&remote_events, request.local_head_hash.as_deref()) {
        MissingEvents::Events(events) => events,
        MissingEvents::Diverged => {
            return PreviewPullResponse {
                peer_id: request.peer_id,
                logbook_id: request.logbook_id,
                status: ReplicationStatus::Diverged,
                local_head_hash: request.local_head_hash,
                remote_head_hash,
                missing_event_count: 0,
                remote_event_count: remote_events.len(),
                events: Vec::new(),
                message: "Remote chain does not contain the local head".to_owned(),
            };
        }
    };

    PreviewPullResponse {
        peer_id: request.peer_id,
        logbook_id: request.logbook_id,
        status: ReplicationStatus::RemoteAhead,
        local_head_hash: request.local_head_hash,
        remote_head_hash,
        missing_event_count: missing.len(),
        remote_event_count: remote_events.len(),
        events: missing.iter().map(metadata_for_event).collect(),
        message: format!("{} remote events are available to pull", missing.len()),
    }
}

pub async fn pull_missing_events<S>(
    store: &S,
    request: PullEventsRequest,
    remote_events: Vec<CoreEventEnvelope>,
) -> PullEventsResponse
where
    S: LogbookEventStore,
{
    let local_head_hash = match store.get_head(request.logbook_id).await {
        Ok(head) => head,
        Err(error) => {
            return PullEventsResponse::rejected(request, None, None, vec![error.to_string()]);
        }
    };
    let remote_events = events_for_logbook(&remote_events, request.logbook_id);
    let remote_head_hash = remote_events.last().map(|event| event.event_hash.clone());

    if local_head_hash == remote_head_hash {
        return PullEventsResponse {
            peer_id: request.peer_id,
            logbook_id: request.logbook_id,
            status: ReplicationStatus::InSync,
            accepted_count: 0,
            ignored_duplicate_count: 0,
            rejected_count: 0,
            local_head_hash,
            remote_head_hash,
            errors: Vec::new(),
        };
    }

    let missing = match missing_events_after(&remote_events, local_head_hash.as_deref()) {
        MissingEvents::Events(events) => events,
        MissingEvents::Diverged
            if remote_events_are_missing_tail(&remote_events, local_head_hash.as_deref()) =>
        {
            remote_events.clone()
        }
        MissingEvents::Diverged => {
            return PullEventsResponse {
                peer_id: request.peer_id,
                logbook_id: request.logbook_id,
                status: ReplicationStatus::Diverged,
                accepted_count: 0,
                ignored_duplicate_count: 0,
                rejected_count: remote_events.len(),
                local_head_hash,
                remote_head_hash,
                errors: vec!["remote chain does not contain the local head".to_owned()],
            };
        }
    };

    if let Err(error) = verify_incoming_chain(request.logbook_id, local_head_hash.clone(), &missing)
    {
        return PullEventsResponse {
            peer_id: request.peer_id,
            logbook_id: request.logbook_id,
            status: match error {
                ReplicationError::Diverged { .. }
                | ReplicationError::PreviousHashMismatch { .. } => ReplicationStatus::Diverged,
                _ => ReplicationStatus::Rejected,
            },
            accepted_count: 0,
            ignored_duplicate_count: 0,
            rejected_count: missing.len(),
            local_head_hash,
            remote_head_hash,
            errors: vec![error.to_string()],
        };
    }

    let mut accepted_count = 0usize;
    let mut ignored_duplicate_count = 0usize;
    let mut errors = Vec::new();
    for event in missing {
        let event_id = event.event_id;
        match store.get_event(event_id).await {
            Ok(Some(existing)) if existing == event => {
                ignored_duplicate_count += 1;
                continue;
            }
            Ok(Some(_)) => {
                errors.push(format!(
                    "event id {event_id} already exists with different content"
                ));
                break;
            }
            Ok(None) => {}
            Err(error) => {
                errors.push(error.to_string());
                break;
            }
        }

        match store.append_verified_remote_event(event).await {
            Ok(_) => accepted_count += 1,
            Err(error) => {
                errors.push(error.to_string());
                break;
            }
        }
    }

    let final_head = store.get_head(request.logbook_id).await.unwrap_or(None);
    let status = if errors.is_empty() {
        ReplicationStatus::Pulled
    } else {
        ReplicationStatus::Rejected
    };
    PullEventsResponse {
        peer_id: request.peer_id,
        logbook_id: request.logbook_id,
        status,
        accepted_count,
        ignored_duplicate_count,
        rejected_count: errors.len(),
        local_head_hash: final_head,
        remote_head_hash,
        errors,
    }
}

fn events_for_logbook(events: &[CoreEventEnvelope], logbook_id: Uuid) -> Vec<CoreEventEnvelope> {
    events
        .iter()
        .filter(|event| event.logbook_id == logbook_id)
        .cloned()
        .collect()
}

enum MissingEvents {
    Events(Vec<CoreEventEnvelope>),
    Diverged,
}

fn missing_events_after(
    remote_events: &[CoreEventEnvelope],
    local_head_hash: Option<&str>,
) -> MissingEvents {
    match local_head_hash {
        None => MissingEvents::Events(remote_events.to_vec()),
        Some(hash) => remote_events
            .iter()
            .position(|event| event.event_hash == hash)
            .map(|index| MissingEvents::Events(remote_events[index + 1..].to_vec()))
            .unwrap_or(MissingEvents::Diverged),
    }
}

fn remote_events_are_missing_tail(
    remote_events: &[CoreEventEnvelope],
    local_head_hash: Option<&str>,
) -> bool {
    match (local_head_hash, remote_events.first()) {
        (Some(hash), Some(first)) => first.previous_hash.as_deref() == Some(hash),
        _ => false,
    }
}

pub fn verify_incoming_chain(
    logbook_id: Uuid,
    local_head_hash: Option<String>,
    events: &[CoreEventEnvelope],
) -> Result<(), ReplicationError> {
    let mut expected_previous_hash = local_head_hash;
    for event in events {
        if event.logbook_id != logbook_id {
            return Err(ReplicationError::LogbookMismatch {
                event_id: event.event_id,
                expected_logbook_id: logbook_id,
                actual_logbook_id: event.logbook_id,
            });
        }
        validate_supported_remote_event(event)?;
        if event.previous_hash != expected_previous_hash {
            return Err(ReplicationError::PreviousHashMismatch {
                event_id: event.event_id,
                expected: expected_previous_hash,
                actual: event.previous_hash.clone(),
            });
        }
        expected_previous_hash = Some(event.event_hash.clone());
    }
    Ok(())
}

impl PullEventsResponse {
    fn rejected(
        request: PullEventsRequest,
        local_head_hash: Option<String>,
        remote_head_hash: Option<String>,
        errors: Vec<String>,
    ) -> Self {
        Self {
            peer_id: request.peer_id,
            logbook_id: request.logbook_id,
            status: ReplicationStatus::Rejected,
            accepted_count: 0,
            ignored_duplicate_count: 0,
            rejected_count: errors.len(),
            local_head_hash,
            remote_head_hash,
            errors,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudSyncConfig {
    pub enable_cloud_sync: bool,
    pub sync_server_url: String,
    pub account_login_mode: CloudLoginMode,
    pub device_name: String,
    pub prefer_lan_sync: bool,
    pub auto_push_enabled: bool,
    pub auto_pull_enabled: bool,
    pub sync_interval_seconds: u64,
}

impl Default for CloudSyncConfig {
    fn default() -> Self {
        Self {
            enable_cloud_sync: false,
            sync_server_url: "http://127.0.0.1:9740".to_owned(),
            account_login_mode: CloudLoginMode::PairingCode,
            device_name: "KE8YGW Logger Device".to_owned(),
            prefer_lan_sync: true,
            auto_push_enabled: false,
            auto_pull_enabled: false,
            sync_interval_seconds: 300,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudLoginMode {
    PairingCode,
    SyncToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudConnectionState {
    Disconnected,
    Connected,
    Unauthorized,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudHealthResponse {
    pub ok: bool,
    pub service: String,
    pub version: String,
    pub mode: CloudServiceMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CloudServiceMode {
    Hosted,
    SelfHosted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairDeviceRequest {
    pub pairing_code: String,
    pub account_id: String,
    pub user_id: String,
    pub device_id: Uuid,
    pub device_name: String,
    pub requested_logbooks: Vec<Uuid>,
    pub role_hints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairDeviceResponse {
    pub accepted: bool,
    pub reason: Option<String>,
    pub session: Option<CloudSession>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudSession {
    pub account_id: String,
    pub user_id: String,
    pub device_id: Uuid,
    pub device_name: String,
    pub sync_token: String,
    pub authorized_logbooks: Vec<Uuid>,
    pub issued_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudAuth {
    pub sync_token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudPreviewPullRequest {
    pub auth: CloudAuth,
    pub logbook_id: Uuid,
    pub local_head_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudPullEventsRequest {
    pub auth: CloudAuth,
    pub logbook_id: Uuid,
    pub local_head_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudPullEventsResponse {
    pub preview: PreviewPullResponse,
    pub events: Vec<CoreEventEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CloudPushEventsRequest {
    pub auth: CloudAuth,
    pub logbook_id: Uuid,
    pub events: Vec<CoreEventEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudPushEventsResponse {
    pub status: ReplicationStatus,
    pub accepted_count: usize,
    pub ignored_duplicate_count: usize,
    pub rejected_count: usize,
    pub server_head_hash: Option<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticReportUploadType {
    Basic,
    Sync,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticReportStatus {
    Submitted,
    Triaged,
    Investigating,
    WaitingOnUser,
    Fixed,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticReportUploadRequest {
    pub auth: CloudAuth,
    pub report_type: DiagnosticReportUploadType,
    pub app_version: String,
    pub core_version: String,
    pub platform: String,
    pub plugin_list: Vec<String>,
    pub sync_state_summary: Option<String>,
    pub short_description: String,
    pub bundle_hash: String,
    pub bundle_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticReportUploadResponse {
    pub report_id: String,
    pub status: DiagnosticReportStatus,
    pub received_at: DateTime<Utc>,
    pub bundle_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticReportMetadata {
    pub report_id: String,
    pub user_id: String,
    pub account_id: String,
    pub app_version: String,
    pub core_version: String,
    pub platform: String,
    pub created_at: DateTime<Utc>,
    pub report_type: DiagnosticReportUploadType,
    pub plugin_list: Vec<String>,
    pub sync_state_summary: Option<String>,
    pub short_description: String,
    pub bundle_hash: String,
    pub status: DiagnosticReportStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudSyncStatusResponse {
    pub connection_state: CloudConnectionState,
    pub account_id: Option<String>,
    pub device_id: Option<Uuid>,
    pub server_url: String,
    pub accessible_logbooks: Vec<LogbookHeadSummary>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CloudSyncError {
    #[error("unauthenticated request")]
    Unauthenticated,
    #[error("unauthorized logbook access: {0}")]
    UnauthorizedLogbook(Uuid),
    #[error("pairing rejected: {0}")]
    PairingRejected(String),
    #[error("cloud event validation failed: {0}")]
    Validation(String),
    #[error("cloud store error: {0}")]
    Store(String),
}

#[derive(Debug, Clone)]
pub struct CloudServerConfig {
    pub mode: CloudServiceMode,
    pub public_url: String,
    pub pairing_code: String,
}

impl Default for CloudServerConfig {
    fn default() -> Self {
        Self {
            mode: CloudServiceMode::SelfHosted,
            public_url: "http://127.0.0.1:9740".to_owned(),
            pairing_code: "local-dev-pairing-code".to_owned(),
        }
    }
}

#[derive(Debug, Default)]
struct CloudAuthState {
    sessions_by_token: HashMap<String, CloudSession>,
    account_logbooks: HashMap<String, HashSet<Uuid>>,
    reports: HashMap<String, StoredDiagnosticReport>,
}

#[derive(Debug, Clone)]
struct StoredDiagnosticReport {
    metadata: DiagnosticReportMetadata,
    bundle_bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct InMemoryCloudSyncServer {
    config: CloudServerConfig,
    store: Arc<InMemoryLogbookEventStore>,
    auth: Arc<RwLock<CloudAuthState>>,
}

#[cfg(feature = "surreal-storage")]
#[derive(Debug, Clone)]
pub struct DurableCloudSyncServer {
    config: CloudServerConfig,
    store: Arc<JsonlLogbookEventStore>,
    metadata: Arc<SurrealCloudMetadataStore>,
    reports_dir: PathBuf,
}

#[cfg(feature = "surreal-storage")]
#[derive(Debug, Clone)]
pub struct DurableCloudSyncPaths {
    pub metadata_store_path: PathBuf,
    pub official_event_log_path: PathBuf,
    pub report_dir: PathBuf,
}

#[cfg(feature = "surreal-storage")]
impl DurableCloudSyncPaths {
    pub fn from_env() -> Self {
        Self {
            metadata_store_path: std::env::var("HAM_SYNC_SURREAL_PATH").map_or_else(
                |_| {
                    default_log_directory()
                        .join("sync-server")
                        .join("surrealdb")
                },
                PathBuf::from,
            ),
            official_event_log_path: std::env::var("HAM_SYNC_EVENT_LOG").map_or_else(
                |_| {
                    default_log_directory()
                        .join("sync-server")
                        .join("official-events.jsonl")
                },
                PathBuf::from,
            ),
            report_dir: std::env::var("HAM_SYNC_REPORT_DIR").map_or_else(
                |_| default_log_directory().join("sync-server").join("reports"),
                PathBuf::from,
            ),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderSettingMetadata {
    pub account_id: String,
    pub logbook_id: Option<Uuid>,
    pub provider_id: String,
    pub enabled: bool,
    pub credential_id: Option<String>,
    pub settings: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UploadQueueMetadata {
    pub account_id: String,
    pub logbook_id: Uuid,
    pub upload_id: String,
    pub provider_id: String,
    pub status: String,
    pub qso_count: usize,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(feature = "surreal-storage")]
#[derive(Debug, Clone)]
struct StoredReportRef {
    metadata: DiagnosticReportMetadata,
    bundle_path: PathBuf,
}

#[cfg(feature = "surreal-storage")]
#[derive(Debug, Clone)]
pub enum SurrealCloudEndpoint {
    LocalSurrealKv {
        path: PathBuf,
    },
    RemoteWs {
        endpoint: String,
        username: String,
        password: String,
    },
}

#[cfg(feature = "surreal-storage")]
#[derive(Debug, Clone)]
pub struct SurrealCloudConfig {
    pub endpoint: SurrealCloudEndpoint,
    pub namespace: String,
    pub database: String,
}

#[cfg(feature = "surreal-storage")]
impl SurrealCloudConfig {
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self {
            endpoint: SurrealCloudEndpoint::LocalSurrealKv { path: path.into() },
            namespace: "ke8ygw".to_owned(),
            database: "ham_sync".to_owned(),
        }
    }

    pub fn from_env_path(path: PathBuf) -> Self {
        let namespace =
            std::env::var("HAM_SYNC_SURREAL_NAMESPACE").unwrap_or_else(|_| "ke8ygw".to_owned());
        let database =
            std::env::var("HAM_SYNC_SURREAL_DATABASE").unwrap_or_else(|_| "ham_sync".to_owned());
        if let Ok(endpoint) = std::env::var("HAM_SYNC_SURREAL_ENDPOINT") {
            return Self {
                endpoint: SurrealCloudEndpoint::RemoteWs {
                    endpoint,
                    username: std::env::var("HAM_SYNC_SURREAL_USER")
                        .unwrap_or_else(|_| "root".to_owned()),
                    password: std::env::var("HAM_SYNC_SURREAL_PASS")
                        .unwrap_or_else(|_| "root".to_owned()),
                },
                namespace,
                database,
            };
        }
        Self {
            endpoint: SurrealCloudEndpoint::LocalSurrealKv { path },
            namespace,
            database,
        }
    }
}

#[cfg(feature = "surreal-storage")]
#[derive(Clone)]
enum SurrealCloudClient {
    Local(Surreal<Db>),
    Remote(Surreal<Any>),
}

#[cfg(feature = "surreal-storage")]
impl std::fmt::Debug for SurrealCloudClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(_) => formatter.write_str("Local(Surreal<Db>)"),
            Self::Remote(_) => formatter.write_str("Remote(Surreal<Any>)"),
        }
    }
}

#[cfg(feature = "surreal-storage")]
#[derive(Debug, Clone)]
struct SurrealCloudMetadataStore {
    runtime: Arc<std::sync::Mutex<Option<Runtime>>>,
    client: Arc<std::sync::Mutex<Option<SurrealCloudClient>>>,
}

#[cfg(feature = "surreal-storage")]
#[derive(Debug, Serialize, Deserialize)]
struct CloudPayloadRow<T> {
    payload: T,
}

#[cfg(feature = "surreal-storage")]
impl SurrealCloudMetadataStore {
    fn open(config: SurrealCloudConfig) -> Result<Self, CloudSyncError> {
        let (runtime, client) = thread::spawn({
            let config = config.clone();
            move || {
                let runtime = Runtime::new().map_err(cloud_store_error)?;
                let client = runtime.block_on(async {
                    let client = connect_cloud_surreal(&config).await?;
                    initialize_cloud_schema(&client).await?;
                    Ok::<_, CloudSyncError>(client)
                })?;
                Ok::<_, CloudSyncError>((runtime, client))
            }
        })
        .join()
        .map_err(|_| CloudSyncError::Store("SurrealDB storage thread failed".to_owned()))??;
        Ok(Self {
            runtime: Arc::new(std::sync::Mutex::new(Some(runtime))),
            client: Arc::new(std::sync::Mutex::new(Some(client))),
        })
    }

    fn run<T, Fut>(
        &self,
        operation: impl FnOnce(SurrealCloudClient) -> Fut + Send + 'static,
    ) -> Result<T, CloudSyncError>
    where
        T: Send + 'static,
        Fut: std::future::Future<Output = Result<T, CloudSyncError>> + Send + 'static,
    {
        let runtime = self.runtime.clone();
        let client = self
            .client
            .lock()
            .expect("SurrealDB client mutex should not be poisoned")
            .as_ref()
            .ok_or_else(|| CloudSyncError::Store("SurrealDB client closed".to_owned()))?
            .clone();
        thread::spawn(move || {
            let guard = runtime
                .lock()
                .expect("SurrealDB runtime mutex should not be poisoned");
            let runtime = guard
                .as_ref()
                .ok_or_else(|| CloudSyncError::Store("SurrealDB runtime closed".to_owned()))?;
            runtime.block_on(operation(client))
        })
        .join()
        .map_err(|_| CloudSyncError::Store("SurrealDB storage thread failed".to_owned()))?
    }

    fn save_session(&self, session: &CloudSession) -> Result<(), CloudSyncError> {
        let session = session.clone();
        self.run(move |client| async move {
            create_cloud_record(
                &client,
                "sync_sessions",
                sync_token_hash(&session.sync_token),
                serde_json::json!({
                    "account_id": session.account_id,
                    "user_id": session.user_id,
                    "device_id": session.device_id,
                    "token_hash": sync_token_hash(&session.sync_token),
                    "revoked": false,
                    "payload": session,
                }),
            )
            .await?;
            create_cloud_record(
                &client,
                "sync_devices",
                session.device_id.to_string(),
                serde_json::json!({
                    "account_id": session.account_id,
                    "user_id": session.user_id,
                    "device_id": session.device_id,
                    "device_name": session.device_name,
                    "revoked": false,
                    "payload": session,
                }),
            )
            .await?;
            for logbook_id in &session.authorized_logbooks {
                create_cloud_record(
                    &client,
                    "sync_logbook_access",
                    format!("{}-{}", session.account_id, logbook_id),
                    serde_json::json!({
                        "account_id": session.account_id,
                        "logbook_id": logbook_id,
                        "payload": {
                            "account_id": session.account_id,
                            "logbook_id": logbook_id,
                        },
                    }),
                )
                .await?;
            }
            Ok(())
        })
    }

    fn session(&self, auth: &CloudAuth) -> Result<CloudSession, CloudSyncError> {
        let token_hash = sync_token_hash(&auth.sync_token);
        self.run(move |client| async move {
            let sessions = select_cloud_payloads::<CloudSession>(&client, "sync_sessions").await?;
            let Some(session) = sessions
                .into_iter()
                .find(|session| sync_token_hash(&session.sync_token) == token_hash)
            else {
                return Err(CloudSyncError::Unauthenticated);
            };
            let devices = select_cloud_payloads::<CloudSession>(&client, "sync_devices").await?;
            let Some(device) = devices
                .into_iter()
                .find(|device| device.device_id == session.device_id)
            else {
                return Err(CloudSyncError::Unauthenticated);
            };
            let revoked = select_cloud_rows(&client, "sync_devices")
                .await?
                .into_iter()
                .find(|row| {
                    row.get("device_id")
                        .and_then(JsonValue::as_str)
                        .is_some_and(|id| id == session.device_id.to_string())
                })
                .and_then(|row| row.get("revoked").and_then(JsonValue::as_bool))
                .unwrap_or(false);
            if revoked || device.device_id != session.device_id {
                return Err(CloudSyncError::Unauthenticated);
            }
            Ok(session)
        })
    }

    fn revoke_device(&self, device_id: Uuid) -> Result<(), CloudSyncError> {
        self.run(move |client| async move {
            merge_cloud_record(
                &client,
                "sync_devices",
                device_id.to_string(),
                serde_json::json!({ "revoked": true }),
            )
            .await?;
            Ok(())
        })
    }

    fn account_logbooks(&self, account_id: &str) -> Result<HashSet<Uuid>, CloudSyncError> {
        let account_id = account_id.to_owned();
        self.run(move |client| async move {
            let rows = select_cloud_rows(&client, "sync_logbook_access").await?;
            let mut logbooks = HashSet::new();
            for row in rows {
                if row.get("account_id").and_then(JsonValue::as_str) != Some(account_id.as_str()) {
                    continue;
                }
                if let Some(value) = row.get("logbook_id").and_then(JsonValue::as_str) {
                    logbooks.insert(
                        Uuid::parse_str(value)
                            .map_err(|error| CloudSyncError::Store(error.to_string()))?,
                    );
                }
            }
            Ok(logbooks)
        })
    }

    fn update_sync_state(
        &self,
        logbook_id: Uuid,
        head_hash: Option<String>,
        event_count: usize,
    ) -> Result<(), CloudSyncError> {
        self.run(move |client| async move {
            create_cloud_record(
                &client,
                "sync_heads",
                logbook_id.to_string(),
                serde_json::json!({
                    "logbook_id": logbook_id,
                    "head_hash": head_hash,
                    "event_count": event_count,
                    "updated_at": Utc::now(),
                    "payload": {
                        "logbook_id": logbook_id,
                        "head_hash": head_hash,
                        "event_count": event_count,
                    },
                }),
            )
            .await?;
            Ok(())
        })
    }

    fn save_report(&self, report: &StoredReportRef) -> Result<(), CloudSyncError> {
        let report = report.clone();
        self.run(move |client| async move {
            create_cloud_record(
                &client,
                "diagnostic_reports",
                report.metadata.report_id.clone(),
                serde_json::json!({
                    "account_id": report.metadata.account_id,
                    "user_id": report.metadata.user_id,
                    "report_id": report.metadata.report_id,
                    "bundle_path": report.bundle_path.display().to_string(),
                    "payload": report.metadata,
                }),
            )
            .await?;
            Ok(())
        })
    }

    fn report(&self, report_id: &str) -> Result<StoredReportRef, CloudSyncError> {
        let report_id = report_id.to_owned();
        self.run(move |client| async move {
            let rows = select_cloud_rows(&client, "diagnostic_reports").await?;
            let Some(row) = rows.into_iter().find(|row| {
                row.get("report_id")
                    .and_then(JsonValue::as_str)
                    .is_some_and(|id| id == report_id)
            }) else {
                return Err(CloudSyncError::Validation("report not found".to_owned()));
            };
            let metadata: DiagnosticReportMetadata =
                serde_json::from_value(row.get("payload").cloned().unwrap_or(JsonValue::Null))
                    .map_err(cloud_store_error)?;
            let bundle_path = row
                .get("bundle_path")
                .and_then(JsonValue::as_str)
                .ok_or_else(|| CloudSyncError::Store("report bundle path missing".to_owned()))?;
            Ok(StoredReportRef {
                metadata,
                bundle_path: PathBuf::from(bundle_path),
            })
        })
    }

    fn save_provider_setting(
        &self,
        setting: ProviderSettingMetadata,
    ) -> Result<(), CloudSyncError> {
        self.run(move |client| async move {
            create_cloud_record(
                &client,
                "provider_settings",
                format!(
                    "{}-{}-{}",
                    setting.account_id,
                    setting
                        .logbook_id
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| "account".to_owned()),
                    setting.provider_id
                ),
                serde_json::json!({
                    "account_id": setting.account_id,
                    "logbook_id": setting.logbook_id,
                    "provider_id": setting.provider_id,
                    "enabled": setting.enabled,
                    "credential_id": setting.credential_id,
                    "settings": setting.settings,
                    "payload": setting,
                }),
            )
            .await
        })
    }

    fn provider_setting(
        &self,
        account_id: &str,
        provider_id: &str,
    ) -> Result<Option<ProviderSettingMetadata>, CloudSyncError> {
        let account_id = account_id.to_owned();
        let provider_id = provider_id.to_owned();
        self.run(move |client| async move {
            let rows =
                select_cloud_payloads::<ProviderSettingMetadata>(&client, "provider_settings")
                    .await?;
            Ok(rows
                .into_iter()
                .find(|row| row.account_id == account_id && row.provider_id == provider_id))
        })
    }

    fn save_upload_queue_item(&self, item: UploadQueueMetadata) -> Result<(), CloudSyncError> {
        self.run(move |client| async move {
            create_cloud_record(
                &client,
                "upload_queue_history",
                item.upload_id.clone(),
                serde_json::json!({
                    "account_id": item.account_id,
                    "logbook_id": item.logbook_id,
                    "provider_id": item.provider_id,
                    "status": item.status,
                    "payload": item,
                }),
            )
            .await
        })
    }

    fn upload_queue_item(
        &self,
        account_id: &str,
        upload_id: &str,
    ) -> Result<Option<UploadQueueMetadata>, CloudSyncError> {
        let account_id = account_id.to_owned();
        let upload_id = upload_id.to_owned();
        self.run(move |client| async move {
            let rows =
                select_cloud_payloads::<UploadQueueMetadata>(&client, "upload_queue_history")
                    .await?;
            Ok(rows
                .into_iter()
                .find(|row| row.account_id == account_id && row.upload_id == upload_id))
        })
    }
}

#[cfg(feature = "surreal-storage")]
impl Drop for SurrealCloudMetadataStore {
    fn drop(&mut self) {
        let client = self
            .client
            .lock()
            .expect("SurrealDB client mutex should not be poisoned")
            .take();
        let runtime = self
            .runtime
            .lock()
            .expect("SurrealDB runtime mutex should not be poisoned")
            .take();
        if client.is_some() || runtime.is_some() {
            let _ = thread::spawn(move || {
                drop(client);
                drop(runtime);
            })
            .join();
        }
    }
}

#[cfg(feature = "surreal-storage")]
async fn connect_cloud_surreal(
    config: &SurrealCloudConfig,
) -> Result<SurrealCloudClient, CloudSyncError> {
    match &config.endpoint {
        SurrealCloudEndpoint::LocalSurrealKv { path } => {
            fs::create_dir_all(path).map_err(cloud_store_error)?;
            let db = Surreal::new::<SurrealKv>(path.display().to_string())
                .await
                .map_err(cloud_store_error)?;
            db.use_ns(&config.namespace)
                .use_db(&config.database)
                .await
                .map_err(cloud_store_error)?;
            Ok(SurrealCloudClient::Local(db))
        }
        SurrealCloudEndpoint::RemoteWs {
            endpoint,
            username,
            password,
        } => {
            let db = Surreal::<Any>::init();
            db.connect(endpoint.as_str())
                .await
                .map_err(cloud_store_error)?;
            db.signin(Root {
                username: username.clone(),
                password: password.clone(),
            })
            .await
            .map_err(cloud_store_error)?;
            db.use_ns(&config.namespace)
                .use_db(&config.database)
                .await
                .map_err(cloud_store_error)?;
            Ok(SurrealCloudClient::Remote(db))
        }
    }
}

#[cfg(feature = "surreal-storage")]
async fn initialize_cloud_schema(client: &SurrealCloudClient) -> Result<(), CloudSyncError> {
    let schema = r#"
        DEFINE TABLE IF NOT EXISTS schema_migrations SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS sync_sessions SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS sync_devices SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS sync_logbook_access SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS pairing_tokens SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS sync_heads SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS sync_event_refs SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS diagnostic_reports SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS provider_settings SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS upload_queue_history SCHEMALESS;
        DEFINE INDEX IF NOT EXISTS sync_sessions_token_hash_idx ON TABLE sync_sessions COLUMNS token_hash UNIQUE;
        DEFINE INDEX IF NOT EXISTS sync_sessions_account_idx ON TABLE sync_sessions COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS sync_sessions_device_idx ON TABLE sync_sessions COLUMNS device_id;
        DEFINE INDEX IF NOT EXISTS sync_devices_account_idx ON TABLE sync_devices COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS sync_devices_device_idx ON TABLE sync_devices COLUMNS device_id;
        DEFINE INDEX IF NOT EXISTS sync_logbook_access_account_idx ON TABLE sync_logbook_access COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS sync_logbook_access_logbook_idx ON TABLE sync_logbook_access COLUMNS logbook_id;
        DEFINE INDEX IF NOT EXISTS sync_heads_logbook_idx ON TABLE sync_heads COLUMNS logbook_id;
        DEFINE INDEX IF NOT EXISTS sync_event_refs_logbook_idx ON TABLE sync_event_refs COLUMNS logbook_id;
        DEFINE INDEX IF NOT EXISTS diagnostic_reports_account_idx ON TABLE diagnostic_reports COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS provider_settings_account_idx ON TABLE provider_settings COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS provider_settings_logbook_idx ON TABLE provider_settings COLUMNS logbook_id;
        DEFINE INDEX IF NOT EXISTS provider_settings_provider_idx ON TABLE provider_settings COLUMNS provider_id;
        DEFINE INDEX IF NOT EXISTS upload_queue_account_idx ON TABLE upload_queue_history COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS upload_queue_logbook_idx ON TABLE upload_queue_history COLUMNS logbook_id;
        DEFINE INDEX IF NOT EXISTS upload_queue_provider_idx ON TABLE upload_queue_history COLUMNS provider_id;
        UPSERT schema_migrations:sync_v1 SET version = 1, component = 'ham-sync', applied_at = time::now();
    "#;
    query_cloud_checked(client, schema).await
}

#[cfg(feature = "surreal-storage")]
async fn query_cloud_checked(
    client: &SurrealCloudClient,
    query: &str,
) -> Result<(), CloudSyncError> {
    match client {
        SurrealCloudClient::Local(db) => {
            db.query(query)
                .await
                .map_err(cloud_store_error)?
                .check()
                .map_err(cloud_store_error)?;
        }
        SurrealCloudClient::Remote(db) => {
            db.query(query)
                .await
                .map_err(cloud_store_error)?
                .check()
                .map_err(cloud_store_error)?;
        }
    }
    Ok(())
}

#[cfg(feature = "surreal-storage")]
async fn create_cloud_record(
    client: &SurrealCloudClient,
    table: &'static str,
    id: String,
    content: JsonValue,
) -> Result<(), CloudSyncError> {
    match client {
        SurrealCloudClient::Local(db) => {
            let _: Option<SurrealDbValue> = db
                .upsert((table, id.as_str()))
                .content(content)
                .await
                .map_err(cloud_store_error)?;
        }
        SurrealCloudClient::Remote(db) => {
            let _: Option<SurrealDbValue> = db
                .upsert((table, id.as_str()))
                .content(content)
                .await
                .map_err(cloud_store_error)?;
        }
    }
    Ok(())
}

#[cfg(feature = "surreal-storage")]
async fn merge_cloud_record(
    client: &SurrealCloudClient,
    table: &'static str,
    id: String,
    content: JsonValue,
) -> Result<(), CloudSyncError> {
    match client {
        SurrealCloudClient::Local(db) => {
            let _: Option<SurrealDbValue> = db
                .update((table, id.as_str()))
                .merge(content)
                .await
                .map_err(cloud_store_error)?;
        }
        SurrealCloudClient::Remote(db) => {
            let _: Option<SurrealDbValue> = db
                .update((table, id.as_str()))
                .merge(content)
                .await
                .map_err(cloud_store_error)?;
        }
    }
    Ok(())
}

#[cfg(feature = "surreal-storage")]
async fn select_cloud_rows(
    client: &SurrealCloudClient,
    table: &'static str,
) -> Result<Vec<JsonValue>, CloudSyncError> {
    let query = format!("SELECT * FROM {table};");
    let rows: Vec<SurrealDbValue> = match client {
        SurrealCloudClient::Local(db) => {
            let mut response = db.query(query.as_str()).await.map_err(cloud_store_error)?;
            response.take(0).map_err(cloud_store_error)
        }
        SurrealCloudClient::Remote(db) => {
            let mut response = db.query(query.as_str()).await.map_err(cloud_store_error)?;
            response.take(0).map_err(cloud_store_error)
        }
    }?;
    rows.into_iter()
        .map(|row| Ok(row.into_json_value()))
        .collect()
}

#[cfg(feature = "surreal-storage")]
async fn select_cloud_payloads<T: for<'de> Deserialize<'de>>(
    client: &SurrealCloudClient,
    table: &'static str,
) -> Result<Vec<T>, CloudSyncError> {
    let rows = select_cloud_rows(client, table).await?;
    rows.into_iter()
        .map(|row| {
            serde_json::from_value::<CloudPayloadRow<T>>(row)
                .map(|row| row.payload)
                .map_err(cloud_store_error)
        })
        .collect()
}

#[cfg(feature = "surreal-storage")]
fn sync_token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

impl InMemoryCloudSyncServer {
    pub fn new(config: CloudServerConfig) -> Self {
        Self {
            config,
            store: Arc::new(InMemoryLogbookEventStore::new()),
            auth: Arc::new(RwLock::new(CloudAuthState::default())),
        }
    }

    pub fn health(&self) -> CloudHealthResponse {
        CloudHealthResponse {
            ok: true,
            service: "ke8ygw-sync-server".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            mode: self.config.mode,
        }
    }

    pub async fn pair_device(&self, request: PairDeviceRequest) -> PairDeviceResponse {
        if request.pairing_code != self.config.pairing_code {
            return PairDeviceResponse {
                accepted: false,
                reason: Some("invalid pairing code".to_owned()),
                session: None,
            };
        }

        let token = format!("sync-{}-{}", request.account_id, Uuid::new_v4());
        let session = CloudSession {
            account_id: request.account_id,
            user_id: request.user_id,
            device_id: request.device_id,
            device_name: request.device_name,
            sync_token: token.clone(),
            authorized_logbooks: request.requested_logbooks,
            issued_at: Utc::now(),
        };
        let mut auth = self.auth.write().await;
        auth.account_logbooks
            .entry(session.account_id.clone())
            .or_default()
            .extend(session.authorized_logbooks.iter().copied());
        auth.sessions_by_token.insert(token, session.clone());

        PairDeviceResponse {
            accepted: true,
            reason: None,
            session: Some(session),
        }
    }

    pub async fn list_logbooks(
        &self,
        auth: &CloudAuth,
    ) -> Result<ListLogbooksResponse, CloudSyncError> {
        let session = self.authorize(auth).await?;
        let mut logbooks = Vec::new();
        for logbook_id in session.authorized_logbooks {
            logbooks.push(LogbookHeadSummary {
                logbook_id,
                head_hash: self
                    .store
                    .get_head(logbook_id)
                    .await
                    .map_err(cloud_store_error)?,
                event_count: Some(
                    self.store
                        .list_events(logbook_id)
                        .await
                        .map_err(cloud_store_error)?
                        .len() as u64,
                ),
            });
        }
        Ok(ListLogbooksResponse { logbooks })
    }

    pub async fn get_head(
        &self,
        auth: &CloudAuth,
        logbook_id: Uuid,
    ) -> Result<LogbookHeadSummary, CloudSyncError> {
        self.authorize_logbook(auth, logbook_id).await?;
        Ok(LogbookHeadSummary {
            logbook_id,
            head_hash: self
                .store
                .get_head(logbook_id)
                .await
                .map_err(cloud_store_error)?,
            event_count: Some(
                self.store
                    .list_events(logbook_id)
                    .await
                    .map_err(cloud_store_error)?
                    .len() as u64,
            ),
        })
    }

    pub async fn event_metadata(
        &self,
        auth: &CloudAuth,
        logbook_id: Uuid,
        after_hash: Option<String>,
    ) -> Result<GetEventMetadataResponse, CloudSyncError> {
        self.authorize_logbook(auth, logbook_id).await?;
        let events = self
            .store
            .list_events_after(logbook_id, after_hash)
            .await
            .map_err(cloud_store_error)?;
        Ok(GetEventMetadataResponse {
            logbook_id,
            events: events.iter().map(metadata_for_event).collect(),
        })
    }

    pub async fn preview_pull(
        &self,
        request: CloudPreviewPullRequest,
    ) -> Result<PreviewPullResponse, CloudSyncError> {
        self.authorize_logbook(&request.auth, request.logbook_id)
            .await?;
        let events = self
            .store
            .list_events(request.logbook_id)
            .await
            .map_err(cloud_store_error)?;
        Ok(preview_pull_from_events(
            PreviewPullRequest {
                peer_id: "cloud".to_owned(),
                logbook_id: request.logbook_id,
                local_head_hash: request.local_head_hash,
            },
            &events,
        ))
    }

    pub async fn pull_events(
        &self,
        request: CloudPullEventsRequest,
    ) -> Result<CloudPullEventsResponse, CloudSyncError> {
        self.authorize_logbook(&request.auth, request.logbook_id)
            .await?;
        let events = self
            .store
            .list_events(request.logbook_id)
            .await
            .map_err(cloud_store_error)?;
        let preview = preview_pull_from_events(
            PreviewPullRequest {
                peer_id: "cloud".to_owned(),
                logbook_id: request.logbook_id,
                local_head_hash: request.local_head_hash,
            },
            &events,
        );
        let event_hashes = preview
            .events
            .iter()
            .map(|event| event.event_hash.as_str())
            .collect::<HashSet<_>>();
        let events = events
            .into_iter()
            .filter(|event| event_hashes.contains(event.event_hash.as_str()))
            .collect();
        Ok(CloudPullEventsResponse { preview, events })
    }

    pub async fn push_events(
        &self,
        request: CloudPushEventsRequest,
    ) -> Result<CloudPushEventsResponse, CloudSyncError> {
        self.authorize_logbook(&request.auth, request.logbook_id)
            .await?;
        if request
            .events
            .iter()
            .any(|event| event.logbook_id != request.logbook_id)
        {
            return Err(CloudSyncError::UnauthorizedLogbook(request.logbook_id));
        }

        let mut accepted_count = 0usize;
        let mut ignored_duplicate_count = 0usize;
        let mut errors = Vec::new();
        for event in request.events {
            if let Err(error) = validate_supported_remote_event(&event) {
                errors.push(error.to_string());
                break;
            }
            match self.store.get_event(event.event_id).await {
                Ok(Some(existing)) if existing == event => {
                    ignored_duplicate_count += 1;
                    continue;
                }
                Ok(Some(_)) => {
                    errors.push(format!(
                        "event id {} already exists with different content",
                        event.event_id
                    ));
                    break;
                }
                Ok(None) => {}
                Err(error) => {
                    errors.push(error.to_string());
                    break;
                }
            }
            match self.store.append_verified_remote_event(event).await {
                Ok(_) => accepted_count += 1,
                Err(error) => {
                    errors.push(error.to_string());
                    break;
                }
            }
        }

        let server_head_hash = self
            .store
            .get_head(request.logbook_id)
            .await
            .map_err(cloud_store_error)?;
        let status = if errors.is_empty() {
            ReplicationStatus::Pulled
        } else if errors
            .iter()
            .any(|error| error.contains("does not connect") || error.contains("previous hash"))
        {
            ReplicationStatus::Diverged
        } else {
            ReplicationStatus::Rejected
        };
        Ok(CloudPushEventsResponse {
            status,
            accepted_count,
            ignored_duplicate_count,
            rejected_count: errors.len(),
            server_head_hash,
            errors,
        })
    }

    pub async fn upload_report(
        &self,
        request: DiagnosticReportUploadRequest,
    ) -> Result<DiagnosticReportUploadResponse, CloudSyncError> {
        let session = self.authorize(&request.auth).await?;
        if request.bundle_hash.trim().is_empty() || request.bundle_bytes.is_empty() {
            return Err(CloudSyncError::Validation(
                "diagnostic report bundle is empty".to_owned(),
            ));
        }
        let report_id = format!("rpt-{}", Uuid::new_v4());
        let received_at = Utc::now();
        let metadata = DiagnosticReportMetadata {
            report_id: report_id.clone(),
            user_id: session.user_id,
            account_id: session.account_id,
            app_version: request.app_version,
            core_version: request.core_version,
            platform: request.platform,
            created_at: received_at,
            report_type: request.report_type,
            plugin_list: request.plugin_list,
            sync_state_summary: request.sync_state_summary,
            short_description: request.short_description,
            bundle_hash: request.bundle_hash.clone(),
            status: DiagnosticReportStatus::Submitted,
        };
        self.auth.write().await.reports.insert(
            report_id.clone(),
            StoredDiagnosticReport {
                metadata,
                bundle_bytes: request.bundle_bytes,
            },
        );
        Ok(DiagnosticReportUploadResponse {
            report_id,
            status: DiagnosticReportStatus::Submitted,
            received_at,
            bundle_hash: request.bundle_hash,
        })
    }

    pub async fn report_metadata(
        &self,
        auth: &CloudAuth,
        report_id: &str,
    ) -> Result<DiagnosticReportMetadata, CloudSyncError> {
        let session = self.authorize(auth).await?;
        let auth = self.auth.read().await;
        let report = auth
            .reports
            .get(report_id)
            .ok_or_else(|| CloudSyncError::Validation("report not found".to_owned()))?;
        if report.metadata.account_id != session.account_id {
            return Err(CloudSyncError::Unauthenticated);
        }
        let _retained_size = report.bundle_bytes.len();
        Ok(report.metadata.clone())
    }

    pub async fn status(
        &self,
        auth: Option<&CloudAuth>,
    ) -> Result<CloudSyncStatusResponse, CloudSyncError> {
        let Some(auth) = auth else {
            return Ok(CloudSyncStatusResponse {
                connection_state: CloudConnectionState::Disconnected,
                account_id: None,
                device_id: None,
                server_url: self.config.public_url.clone(),
                accessible_logbooks: Vec::new(),
            });
        };
        let session = self.authorize(auth).await?;
        let logbooks = self.list_logbooks(auth).await?.logbooks;
        Ok(CloudSyncStatusResponse {
            connection_state: CloudConnectionState::Connected,
            account_id: Some(session.account_id),
            device_id: Some(session.device_id),
            server_url: self.config.public_url.clone(),
            accessible_logbooks: logbooks,
        })
    }

    async fn authorize(&self, auth: &CloudAuth) -> Result<CloudSession, CloudSyncError> {
        self.auth
            .read()
            .await
            .sessions_by_token
            .get(&auth.sync_token)
            .cloned()
            .ok_or(CloudSyncError::Unauthenticated)
    }

    async fn authorize_logbook(
        &self,
        auth: &CloudAuth,
        logbook_id: Uuid,
    ) -> Result<CloudSession, CloudSyncError> {
        let session = self.authorize(auth).await?;
        if !session.authorized_logbooks.contains(&logbook_id) {
            return Err(CloudSyncError::UnauthorizedLogbook(logbook_id));
        }
        Ok(session)
    }
}

#[cfg(feature = "surreal-storage")]
impl DurableCloudSyncServer {
    pub fn open(
        config: CloudServerConfig,
        paths: DurableCloudSyncPaths,
    ) -> Result<Self, CloudSyncError> {
        if let Some(parent) = paths.official_event_log_path.parent() {
            fs::create_dir_all(parent).map_err(cloud_store_error)?;
        }
        fs::create_dir_all(&paths.report_dir).map_err(cloud_store_error)?;
        Ok(Self {
            config,
            store: Arc::new(
                JsonlLogbookEventStore::open(paths.official_event_log_path)
                    .map_err(cloud_store_error)?,
            ),
            metadata: Arc::new(SurrealCloudMetadataStore::open(
                SurrealCloudConfig::from_env_path(paths.metadata_store_path),
            )?),
            reports_dir: paths.report_dir,
        })
    }

    pub fn health(&self) -> CloudHealthResponse {
        CloudHealthResponse {
            ok: true,
            service: "ke8ygw-sync-server".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            mode: self.config.mode,
        }
    }

    pub async fn pair_device(&self, request: PairDeviceRequest) -> PairDeviceResponse {
        if request.pairing_code != self.config.pairing_code {
            return PairDeviceResponse {
                accepted: false,
                reason: Some("invalid pairing code".to_owned()),
                session: None,
            };
        }

        let token = format!("sync-{}-{}", request.account_id, Uuid::new_v4());
        let session = CloudSession {
            account_id: request.account_id,
            user_id: request.user_id,
            device_id: request.device_id,
            device_name: request.device_name,
            sync_token: token,
            authorized_logbooks: request.requested_logbooks,
            issued_at: Utc::now(),
        };
        if let Err(error) = self.metadata.save_session(&session) {
            return PairDeviceResponse {
                accepted: false,
                reason: Some(error.to_string()),
                session: None,
            };
        }
        PairDeviceResponse {
            accepted: true,
            reason: None,
            session: Some(session),
        }
    }

    pub async fn list_logbooks(
        &self,
        auth: &CloudAuth,
    ) -> Result<ListLogbooksResponse, CloudSyncError> {
        let session = self.authorize(auth).await?;
        let mut logbooks = Vec::new();
        for logbook_id in self.metadata.account_logbooks(&session.account_id)? {
            logbooks.push(self.get_head(auth, logbook_id).await?);
        }
        logbooks.sort_by_key(|logbook| logbook.logbook_id);
        Ok(ListLogbooksResponse { logbooks })
    }

    pub async fn get_head(
        &self,
        auth: &CloudAuth,
        logbook_id: Uuid,
    ) -> Result<LogbookHeadSummary, CloudSyncError> {
        self.authorize_logbook(auth, logbook_id).await?;
        let events = self
            .store
            .list_events(logbook_id)
            .await
            .map_err(cloud_store_error)?;
        Ok(LogbookHeadSummary {
            logbook_id,
            head_hash: events.last().map(|event| event.event_hash.clone()),
            event_count: Some(events.len() as u64),
        })
    }

    pub async fn event_metadata(
        &self,
        auth: &CloudAuth,
        logbook_id: Uuid,
        after_hash: Option<String>,
    ) -> Result<GetEventMetadataResponse, CloudSyncError> {
        self.authorize_logbook(auth, logbook_id).await?;
        let events = self
            .store
            .list_events_after(logbook_id, after_hash)
            .await
            .map_err(cloud_store_error)?;
        Ok(GetEventMetadataResponse {
            logbook_id,
            events: events.iter().map(metadata_for_event).collect(),
        })
    }

    pub async fn preview_pull(
        &self,
        request: CloudPreviewPullRequest,
    ) -> Result<PreviewPullResponse, CloudSyncError> {
        self.authorize_logbook(&request.auth, request.logbook_id)
            .await?;
        let events = self
            .store
            .list_events(request.logbook_id)
            .await
            .map_err(cloud_store_error)?;
        Ok(preview_pull_from_events(
            PreviewPullRequest {
                peer_id: "cloud".to_owned(),
                logbook_id: request.logbook_id,
                local_head_hash: request.local_head_hash,
            },
            &events,
        ))
    }

    pub async fn pull_events(
        &self,
        request: CloudPullEventsRequest,
    ) -> Result<CloudPullEventsResponse, CloudSyncError> {
        self.authorize_logbook(&request.auth, request.logbook_id)
            .await?;
        let events = self
            .store
            .list_events(request.logbook_id)
            .await
            .map_err(cloud_store_error)?;
        let preview = preview_pull_from_events(
            PreviewPullRequest {
                peer_id: "cloud".to_owned(),
                logbook_id: request.logbook_id,
                local_head_hash: request.local_head_hash,
            },
            &events,
        );
        let event_hashes = preview
            .events
            .iter()
            .map(|event| event.event_hash.as_str())
            .collect::<HashSet<_>>();
        let events = events
            .into_iter()
            .filter(|event| event_hashes.contains(event.event_hash.as_str()))
            .collect();
        Ok(CloudPullEventsResponse { preview, events })
    }

    pub async fn push_events(
        &self,
        request: CloudPushEventsRequest,
    ) -> Result<CloudPushEventsResponse, CloudSyncError> {
        self.authorize_logbook(&request.auth, request.logbook_id)
            .await?;
        if request
            .events
            .iter()
            .any(|event| event.logbook_id != request.logbook_id)
        {
            return Err(CloudSyncError::UnauthorizedLogbook(request.logbook_id));
        }

        let mut accepted_count = 0usize;
        let mut ignored_duplicate_count = 0usize;
        let mut errors = Vec::new();
        for event in request.events {
            if let Err(error) = validate_supported_remote_event(&event) {
                errors.push(error.to_string());
                break;
            }
            match self.store.get_event(event.event_id).await {
                Ok(Some(existing)) if existing == event => {
                    ignored_duplicate_count += 1;
                    continue;
                }
                Ok(Some(_)) => {
                    errors.push(format!(
                        "event id {} already exists with different content",
                        event.event_id
                    ));
                    break;
                }
                Ok(None) => {}
                Err(error) => {
                    errors.push(error.to_string());
                    break;
                }
            }
            match self.store.append_verified_remote_event(event).await {
                Ok(_) => accepted_count += 1,
                Err(error) => {
                    errors.push(error.to_string());
                    break;
                }
            }
        }

        let server_head_hash = self
            .store
            .get_head(request.logbook_id)
            .await
            .map_err(cloud_store_error)?;
        let event_count = self
            .store
            .list_events(request.logbook_id)
            .await
            .map_err(cloud_store_error)?
            .len();
        self.metadata.update_sync_state(
            request.logbook_id,
            server_head_hash.clone(),
            event_count,
        )?;
        let status = if errors.is_empty() {
            ReplicationStatus::Pulled
        } else if errors
            .iter()
            .any(|error| error.contains("does not connect") || error.contains("previous hash"))
        {
            ReplicationStatus::Diverged
        } else {
            ReplicationStatus::Rejected
        };
        Ok(CloudPushEventsResponse {
            status,
            accepted_count,
            ignored_duplicate_count,
            rejected_count: errors.len(),
            server_head_hash,
            errors,
        })
    }

    pub async fn upload_report(
        &self,
        request: DiagnosticReportUploadRequest,
    ) -> Result<DiagnosticReportUploadResponse, CloudSyncError> {
        let session = self.authorize(&request.auth).await?;
        if request.bundle_hash.trim().is_empty() || request.bundle_bytes.is_empty() {
            return Err(CloudSyncError::Validation(
                "diagnostic report bundle is empty".to_owned(),
            ));
        }
        fs::create_dir_all(&self.reports_dir).map_err(cloud_store_error)?;
        let report_id = format!("rpt-{}", Uuid::new_v4());
        let received_at = Utc::now();
        let bundle_path = self.reports_dir.join(format!("{report_id}.bin"));
        fs::write(&bundle_path, &request.bundle_bytes).map_err(cloud_store_error)?;
        let metadata = DiagnosticReportMetadata {
            report_id: report_id.clone(),
            user_id: session.user_id,
            account_id: session.account_id,
            app_version: request.app_version,
            core_version: request.core_version,
            platform: request.platform,
            created_at: received_at,
            report_type: request.report_type,
            plugin_list: request.plugin_list,
            sync_state_summary: request.sync_state_summary,
            short_description: request.short_description,
            bundle_hash: request.bundle_hash.clone(),
            status: DiagnosticReportStatus::Submitted,
        };
        self.metadata.save_report(&StoredReportRef {
            metadata,
            bundle_path,
        })?;
        Ok(DiagnosticReportUploadResponse {
            report_id,
            status: DiagnosticReportStatus::Submitted,
            received_at,
            bundle_hash: request.bundle_hash,
        })
    }

    pub async fn report_metadata(
        &self,
        auth: &CloudAuth,
        report_id: &str,
    ) -> Result<DiagnosticReportMetadata, CloudSyncError> {
        let session = self.authorize(auth).await?;
        let report = self.metadata.report(report_id)?;
        if report.metadata.account_id != session.account_id {
            return Err(CloudSyncError::Unauthenticated);
        }
        Ok(report.metadata)
    }

    pub fn report_bundle_bytes(&self, report_id: &str) -> Result<Vec<u8>, CloudSyncError> {
        let report = self.metadata.report(report_id)?;
        fs::read(report.bundle_path).map_err(cloud_store_error)
    }

    pub fn revoke_device(&self, device_id: Uuid) -> Result<(), CloudSyncError> {
        self.metadata.revoke_device(device_id)
    }

    pub fn save_provider_setting(
        &self,
        setting: ProviderSettingMetadata,
    ) -> Result<(), CloudSyncError> {
        self.metadata.save_provider_setting(setting)
    }

    pub fn provider_setting(
        &self,
        account_id: &str,
        provider_id: &str,
    ) -> Result<Option<ProviderSettingMetadata>, CloudSyncError> {
        self.metadata.provider_setting(account_id, provider_id)
    }

    pub fn save_upload_queue_item(&self, item: UploadQueueMetadata) -> Result<(), CloudSyncError> {
        self.metadata.save_upload_queue_item(item)
    }

    pub fn upload_queue_item(
        &self,
        account_id: &str,
        upload_id: &str,
    ) -> Result<Option<UploadQueueMetadata>, CloudSyncError> {
        self.metadata.upload_queue_item(account_id, upload_id)
    }

    pub async fn status(
        &self,
        auth: Option<&CloudAuth>,
    ) -> Result<CloudSyncStatusResponse, CloudSyncError> {
        let Some(auth) = auth else {
            return Ok(CloudSyncStatusResponse {
                connection_state: CloudConnectionState::Disconnected,
                account_id: None,
                device_id: None,
                server_url: self.config.public_url.clone(),
                accessible_logbooks: Vec::new(),
            });
        };
        let session = self.authorize(auth).await?;
        let logbooks = self.list_logbooks(auth).await?.logbooks;
        Ok(CloudSyncStatusResponse {
            connection_state: CloudConnectionState::Connected,
            account_id: Some(session.account_id),
            device_id: Some(session.device_id),
            server_url: self.config.public_url.clone(),
            accessible_logbooks: logbooks,
        })
    }

    async fn authorize(&self, auth: &CloudAuth) -> Result<CloudSession, CloudSyncError> {
        self.metadata.session(auth)
    }

    async fn authorize_logbook(
        &self,
        auth: &CloudAuth,
        logbook_id: Uuid,
    ) -> Result<CloudSession, CloudSyncError> {
        let session = self.authorize(auth).await?;
        if !session.authorized_logbooks.contains(&logbook_id) {
            return Err(CloudSyncError::UnauthorizedLogbook(logbook_id));
        }
        Ok(session)
    }
}

#[derive(Debug, Clone)]
pub struct CloudSyncClient {
    server: InMemoryCloudSyncServer,
    auth: Option<CloudAuth>,
}

impl CloudSyncClient {
    pub fn in_memory(server: InMemoryCloudSyncServer) -> Self {
        Self { server, auth: None }
    }

    pub fn auth(&self) -> Option<&CloudAuth> {
        self.auth.as_ref()
    }

    pub async fn pair(
        &mut self,
        request: PairDeviceRequest,
    ) -> Result<CloudSession, CloudSyncError> {
        let response = self.server.pair_device(request).await;
        let Some(session) = response.session else {
            return Err(CloudSyncError::PairingRejected(
                response
                    .reason
                    .unwrap_or_else(|| "pairing rejected".to_owned()),
            ));
        };
        self.auth = Some(CloudAuth {
            sync_token: session.sync_token.clone(),
        });
        Ok(session)
    }

    pub async fn preview_pull(
        &self,
        logbook_id: Uuid,
        local_head_hash: Option<String>,
    ) -> Result<PreviewPullResponse, CloudSyncError> {
        self.server
            .preview_pull(CloudPreviewPullRequest {
                auth: self.required_auth()?,
                logbook_id,
                local_head_hash,
            })
            .await
    }

    pub async fn pull_events(
        &self,
        logbook_id: Uuid,
        local_head_hash: Option<String>,
    ) -> Result<CloudPullEventsResponse, CloudSyncError> {
        self.server
            .pull_events(CloudPullEventsRequest {
                auth: self.required_auth()?,
                logbook_id,
                local_head_hash,
            })
            .await
    }

    pub async fn push_events(
        &self,
        logbook_id: Uuid,
        events: Vec<CoreEventEnvelope>,
    ) -> Result<CloudPushEventsResponse, CloudSyncError> {
        self.server
            .push_events(CloudPushEventsRequest {
                auth: self.required_auth()?,
                logbook_id,
                events,
            })
            .await
    }

    pub async fn upload_report(
        &self,
        mut request: DiagnosticReportUploadRequest,
    ) -> Result<DiagnosticReportUploadResponse, CloudSyncError> {
        request.auth = self.required_auth()?;
        self.server.upload_report(request).await
    }

    fn required_auth(&self) -> Result<CloudAuth, CloudSyncError> {
        self.auth.clone().ok_or(CloudSyncError::Unauthenticated)
    }
}

fn cloud_store_error(error: impl std::fmt::Display) -> CloudSyncError {
    CloudSyncError::Store(error.to_string())
}

#[derive(Debug, Error)]
pub enum DiscoveryServiceError {
    #[error("discovery I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("discovery serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveryObservation {
    pub packet: DiscoveryPacket,
    pub source: SocketAddr,
}

#[derive(Debug)]
pub struct LanDiscoveryService {
    pub config: SyncConfig,
    pub identity: LocalPeerIdentity,
}

impl LanDiscoveryService {
    pub fn discovery_packet(&self) -> DiscoveryPacket {
        DiscoveryPacket::from_identity(&self.identity)
    }

    pub fn send_once(&self) -> Result<(), DiscoveryServiceError> {
        let bytes = serde_json::to_vec(&self.discovery_packet())?;
        let ipv4 = SocketAddr::new(
            IpAddr::V4(self.config.ipv4_multicast_address),
            self.config.discovery_port,
        );
        let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
        socket.set_multicast_loop_v4(true)?;
        socket.send_to(&bytes, ipv4)?;

        let ipv6 = SocketAddr::new(
            IpAddr::V6(self.config.ipv6_multicast_address),
            self.config.discovery_port,
        );
        if let Ok(socket) = UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0)) {
            let _ = socket.set_multicast_loop_v6(true);
            let _ = socket.send_to(&bytes, ipv6);
        }
        Ok(())
    }

    pub fn discover_once(
        &self,
        listen_for: Duration,
    ) -> Result<Vec<DiscoveryObservation>, DiscoveryServiceError> {
        if !self.config.enable_lan_discovery {
            return Ok(Vec::new());
        }
        let receivers = discovery_receiver_sockets(&self.config)?;
        self.send_once()?;
        receive_discovery_packets(&receivers, listen_for)
    }
}

fn discovery_receiver_sockets(
    config: &SyncConfig,
) -> Result<Vec<UdpSocket>, DiscoveryServiceError> {
    let mut sockets = Vec::new();
    let mut last_error = None;

    match bind_reusable_udp_socket(SocketAddr::new(
        IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        config.discovery_port,
    )) {
        Ok(socket) => match socket
            .join_multicast_v4(&config.ipv4_multicast_address, &Ipv4Addr::UNSPECIFIED)
            .and_then(|()| socket.set_nonblocking(true))
        {
            Ok(()) => sockets.push(socket),
            Err(error) => last_error = Some(error),
        },
        Err(error) => last_error = Some(error),
    }

    match bind_reusable_udp_socket(SocketAddr::new(
        IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        config.discovery_port,
    )) {
        Ok(socket) => match socket
            .join_multicast_v6(&config.ipv6_multicast_address, 0)
            .and_then(|()| socket.set_nonblocking(true))
        {
            Ok(()) => sockets.push(socket),
            Err(error) => last_error = Some(error),
        },
        Err(error) => last_error = Some(error),
    }

    if sockets.is_empty() {
        return Err(DiscoveryServiceError::Io(last_error.unwrap_or_else(|| {
            std::io::Error::new(
                ErrorKind::AddrNotAvailable,
                "no usable LAN discovery sockets",
            )
        })));
    }

    Ok(sockets)
}

fn bind_reusable_udp_socket(address: SocketAddr) -> std::io::Result<UdpSocket> {
    let domain = if address.is_ipv4() {
        Domain::IPV4
    } else {
        Domain::IPV6
    };
    let socket = Socket::new(domain, Type::DGRAM, Some(Protocol::UDP))?;
    socket.set_reuse_address(true)?;
    #[cfg(unix)]
    socket.set_reuse_port(true)?;
    if address.is_ipv6() {
        socket.set_only_v6(true)?;
    }
    socket.bind(&SockAddr::from(address))?;
    Ok(socket.into())
}

fn receive_discovery_packets(
    sockets: &[UdpSocket],
    listen_for: Duration,
) -> Result<Vec<DiscoveryObservation>, DiscoveryServiceError> {
    let deadline = Instant::now() + listen_for;
    let mut observations = Vec::new();
    let mut buf = [0_u8; 4096];

    loop {
        let mut made_progress = false;
        for socket in sockets {
            loop {
                match socket.recv_from(&mut buf) {
                    Ok((size, source)) => {
                        made_progress = true;
                        if let Some(observation) =
                            discovery_observation_from_datagram(&buf[..size], source)
                        {
                            observations.push(observation);
                        }
                    }
                    Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                    Err(error) if error.kind() == ErrorKind::TimedOut => break,
                    Err(error) => return Err(DiscoveryServiceError::Io(error)),
                }
            }
        }
        if Instant::now() >= deadline {
            break;
        }
        if !made_progress {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    Ok(observations)
}

fn discovery_observation_from_datagram(
    bytes: &[u8],
    source: SocketAddr,
) -> Option<DiscoveryObservation> {
    serde_json::from_slice::<DiscoveryPacket>(bytes)
        .ok()
        .map(|packet| DiscoveryObservation { packet, source })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ham_core::{CoreEventEnvelope, InMemoryLogbookEventStore, NewLogbookEvent};
    use serde_json::json;

    const EVENT_QSO_CREATED: &str = "official.log.qso.created";
    const EVENT_QSO_CORRECTED: &str = "official.log.qso.corrected";
    const EVENT_QSO_DELETED: &str = "official.log.qso.deleted";
    const EVENT_QSO_RESTORED: &str = "official.log.qso.restored";

    fn local() -> LocalPeerIdentity {
        LocalPeerIdentity::new("Local", Some(9738))
    }

    fn new_event(logbook_id: Uuid, previous_hash: Option<String>) -> CoreEventEnvelope {
        CoreEventEnvelope::from_new(
            NewLogbookEvent {
                event_type: EVENT_QSO_CREATED.to_owned(),
                logbook_id,
                entity_id: Some(Uuid::new_v4()),
                author_operator_id: None,
                station_callsign: "KE8YGW".to_owned(),
                operator_callsign: Some("KE8YGW".to_owned()),
                author_device_id: Uuid::new_v4(),
                source_device_id: Uuid::new_v4(),
                correlation_id: Uuid::new_v4(),
                source_plugin_id: Some("sync-test".to_owned()),
                schema_version: 1,
                payload: json!({
                    "qso_id": Uuid::new_v4(),
                    "station_callsign": "KE8YGW",
                    "operator_callsign": "KE8YGW",
                    "contacted_callsign": "K1ABC",
                    "started_at": "2026-07-05T12:00:00Z",
                    "mode": "SSB"
                }),
            },
            previous_hash,
        )
    }

    fn fixed_time(offset_seconds: i64) -> DateTime<Utc> {
        chrono::DateTime::parse_from_rfc3339("2026-07-05T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
            + chrono::Duration::seconds(offset_seconds)
    }

    fn qso_event(
        logbook_id: Uuid,
        entity_id: Uuid,
        previous_hash: Option<String>,
        event_type: &str,
        source_device_id: Uuid,
        timestamp: DateTime<Utc>,
        payload: JsonValue,
    ) -> CoreEventEnvelope {
        let mut event = CoreEventEnvelope::from_new(
            NewLogbookEvent {
                event_type: event_type.to_owned(),
                logbook_id,
                entity_id: Some(entity_id),
                author_operator_id: None,
                station_callsign: "KE8YGW".to_owned(),
                operator_callsign: Some("KE8YGW".to_owned()),
                author_device_id: source_device_id,
                source_device_id,
                correlation_id: Uuid::new_v4(),
                source_plugin_id: Some("sync-golden-test".to_owned()),
                schema_version: 1,
                payload,
            },
            previous_hash,
        );
        event.timestamp = timestamp;
        event.event_hash = event.calculate_hash();
        event
    }

    fn qso_create_event(
        logbook_id: Uuid,
        entity_id: Uuid,
        previous_hash: Option<String>,
        source_device_id: Uuid,
        timestamp: DateTime<Utc>,
        contacted_callsign: &str,
    ) -> CoreEventEnvelope {
        qso_event(
            logbook_id,
            entity_id,
            previous_hash,
            EVENT_QSO_CREATED,
            source_device_id,
            timestamp,
            json!({
                "qso_id": entity_id,
                "station_callsign": "KE8YGW",
                "operator_callsign": "KE8YGW",
                "contacted_callsign": contacted_callsign,
                "started_at": "2026-07-05T12:00:00Z",
                "band": "20m",
                "mode": "SSB"
            }),
        )
    }

    fn qso_correction_event(
        logbook_id: Uuid,
        entity_id: Uuid,
        previous_hash: Option<String>,
        source_device_id: Uuid,
        timestamp: DateTime<Utc>,
        mode: &str,
    ) -> CoreEventEnvelope {
        qso_event(
            logbook_id,
            entity_id,
            previous_hash,
            EVENT_QSO_CORRECTED,
            source_device_id,
            timestamp,
            json!({ "mode": mode }),
        )
    }

    fn qso_tombstone_event(
        logbook_id: Uuid,
        entity_id: Uuid,
        previous_hash: Option<String>,
        event_type: &str,
        source_device_id: Uuid,
        timestamp: DateTime<Utc>,
    ) -> CoreEventEnvelope {
        qso_event(
            logbook_id,
            entity_id,
            previous_hash,
            event_type,
            source_device_id,
            timestamp,
            json!({ "reason": "sync golden scenario" }),
        )
    }

    fn remote_chain(logbook_id: Uuid, count: usize) -> Vec<CoreEventEnvelope> {
        let mut events = Vec::new();
        let mut previous_hash = None;
        for _ in 0..count {
            let event = new_event(logbook_id, previous_hash);
            previous_hash = Some(event.event_hash.clone());
            events.push(event);
        }
        events
    }

    fn cloud_server() -> InMemoryCloudSyncServer {
        InMemoryCloudSyncServer::new(CloudServerConfig::default())
    }

    async fn paired_client(server: InMemoryCloudSyncServer, logbook_id: Uuid) -> CloudSyncClient {
        paired_client_for_device(server, logbook_id, Uuid::new_v4(), "Test Device".to_owned()).await
    }

    async fn paired_client_for_device(
        server: InMemoryCloudSyncServer,
        logbook_id: Uuid,
        device_id: Uuid,
        device_name: String,
    ) -> CloudSyncClient {
        let mut client = CloudSyncClient::in_memory(server);
        client
            .pair(PairDeviceRequest {
                pairing_code: "local-dev-pairing-code".to_owned(),
                account_id: "acct-1".to_owned(),
                user_id: "user-1".to_owned(),
                device_id,
                device_name,
                requested_logbooks: vec![logbook_id],
                role_hints: vec!["admin".to_owned()],
            })
            .await
            .unwrap();
        client
    }

    fn enqueue_event_for_sync(
        queue: &JsonOfflineMutationQueue,
        event: &CoreEventEnvelope,
        operation_type: &str,
        operation_id: Uuid,
        now: DateTime<Utc>,
    ) -> OfflineMutationEnvelope {
        let mutation = queue
            .enqueue_input(
                OfflineMutationInput::new(
                    event.logbook_id,
                    event.source_device_id,
                    event.source_device_id,
                    operation_type,
                    event.payload.clone(),
                )
                .with_operation_id(operation_id)
                .with_correlation_id(event.correlation_id)
                .with_entity_id(event.entity_id)
                .with_idempotency_key(format!("{operation_type}-{operation_id}")),
                now,
            )
            .unwrap();
        queue
            .record_local_event(mutation.operation_id, event, now)
            .unwrap()
    }

    #[cfg(feature = "surreal-storage")]
    fn durable_paths(label: &str) -> DurableCloudSyncPaths {
        let root = std::env::temp_dir().join(format!("ke8ygw-ham-sync-{label}-{}", Uuid::new_v4()));
        DurableCloudSyncPaths {
            metadata_store_path: root.join("surrealdb"),
            official_event_log_path: root.join("official-events.jsonl"),
            report_dir: root.join("reports"),
        }
    }

    #[cfg(feature = "surreal-storage")]
    fn durable_server(paths: &DurableCloudSyncPaths) -> DurableCloudSyncServer {
        let mut last_error = None;
        for _ in 0..20 {
            match DurableCloudSyncServer::open(CloudServerConfig::default(), paths.clone()) {
                Ok(server) => return server,
                Err(error) => {
                    last_error = Some(error);
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        panic!(
            "failed to open SurrealDB sync test server: {}",
            last_error.unwrap()
        );
    }

    #[cfg(feature = "surreal-storage")]
    fn pair_request(logbook_id: Uuid, device_id: Uuid) -> PairDeviceRequest {
        PairDeviceRequest {
            pairing_code: "local-dev-pairing-code".to_owned(),
            account_id: "acct-1".to_owned(),
            user_id: "user-1".to_owned(),
            device_id,
            device_name: "Test Device".to_owned(),
            requested_logbooks: vec![logbook_id],
            role_hints: vec!["admin".to_owned()],
        }
    }

    fn sample_report_request(token: &str) -> DiagnosticReportUploadRequest {
        DiagnosticReportUploadRequest {
            auth: CloudAuth {
                sync_token: token.to_owned(),
            },
            report_type: DiagnosticReportUploadType::Basic,
            app_version: "0.1.0".to_owned(),
            core_version: "0.1.0".to_owned(),
            platform: "test".to_owned(),
            plugin_list: vec!["core.gui".to_owned()],
            sync_state_summary: None,
            short_description: "problem".to_owned(),
            bundle_hash: "hash".to_owned(),
            bundle_bytes: b"PK report".to_vec(),
        }
    }

    #[test]
    fn config_defaults_are_safe() {
        let config = SyncConfig::default();
        assert!(config.enable_lan_discovery);
        assert_eq!(config.discovery_port, 9737);
        assert_eq!(config.peer_timeout_seconds, 45);
    }

    #[test]
    fn discovery_packet_serializes() {
        let packet = DiscoveryPacket::from_identity(&local());
        let encoded = serde_json::to_string(&packet).unwrap();
        let decoded: DiscoveryPacket = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.protocol_name, PROTOCOL_NAME);
        assert_eq!(decoded.protocol_version, PROTOCOL_VERSION);
    }

    #[test]
    fn discovery_datagram_decodes_valid_packets_and_ignores_noise() {
        let source = "192.168.1.10:9737".parse().unwrap();
        let packet = DiscoveryPacket::from_identity(&local());
        let bytes = serde_json::to_vec(&packet).unwrap();
        let observation = discovery_observation_from_datagram(&bytes, source).unwrap();
        assert_eq!(observation.packet.device_id, packet.device_id);
        assert_eq!(observation.source, source);
        assert!(discovery_observation_from_datagram(b"not-json", source).is_none());
    }

    #[test]
    fn disabled_lan_discovery_returns_no_observations() {
        let config = SyncConfig {
            enable_lan_discovery: false,
            ..SyncConfig::default()
        };
        let service = LanDiscoveryService {
            config,
            identity: local(),
        };
        let observations = service.discover_once(Duration::from_millis(1)).unwrap();
        assert!(observations.is_empty());
    }

    #[test]
    fn discovery_receiver_sockets_allow_same_port_reuse() {
        let temporary = UdpSocket::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = temporary.local_addr().unwrap().port();
        drop(temporary);

        let config = SyncConfig {
            discovery_port: port,
            ..SyncConfig::default()
        };
        let first = discovery_receiver_sockets(&config).unwrap();
        let second = discovery_receiver_sockets(&config).unwrap();
        assert!(!first.is_empty());
        assert!(!second.is_empty());
    }

    #[test]
    fn self_and_incompatible_discovery_are_ignored() {
        let local = local();
        let mut registry = PeerRegistry::default();
        let address = "127.0.0.1:9738".parse().unwrap();
        assert_eq!(
            registry.observe(&local, DiscoveryPacket::from_identity(&local), address),
            PeerObservation::IgnoredSelf
        );
        let mut packet =
            DiscoveryPacket::from_identity(&LocalPeerIdentity::new("Peer", Some(9738)));
        packet.protocol_version = 999;
        assert_eq!(
            registry.observe(&local, packet, address),
            PeerObservation::IgnoredIncompatible
        );
    }

    #[test]
    fn peer_registry_adds_updates_and_expires() {
        let local = local();
        let mut registry = PeerRegistry::default();
        let packet = DiscoveryPacket::from_identity(&LocalPeerIdentity::new("Peer", Some(9738)));
        let address = "192.0.2.10:9738".parse().unwrap();
        assert!(matches!(
            registry.observe(&local, packet.clone(), address),
            PeerObservation::Discovered(_)
        ));
        assert!(matches!(
            registry.observe(&local, packet, address),
            PeerObservation::Updated(_)
        ));
        let expired = registry.expire_stale(
            Utc::now() + chrono::Duration::seconds(60),
            Duration::from_secs(1),
        );
        assert_eq!(expired.len(), 1);
    }

    #[test]
    fn peer_registry_uses_advertised_api_port() {
        let local = local();
        let mut registry = PeerRegistry::default();
        let packet = DiscoveryPacket::from_identity(&LocalPeerIdentity::new("Peer", Some(9468)));
        let source_address = "192.0.2.10:58124".parse().unwrap();
        registry.observe(&local, packet, source_address);
        let peers = registry.list();
        assert_eq!(peers.len(), 1);
        assert_eq!(peers[0].addresses, vec!["192.0.2.10:9468".parse().unwrap()]);
    }

    #[test]
    fn handshake_serializes_and_compares_heads() {
        let logbook_id = Uuid::new_v4();
        let request = HandshakeRequest {
            protocol_version: PROTOCOL_VERSION,
            device_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            supported_capabilities: vec!["handshake.v1".to_owned()],
            logbooks: vec![LogbookHeadSummary {
                logbook_id,
                head_hash: Some("abc".to_owned()),
                event_count: Some(2),
            }],
        };
        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: HandshakeRequest = serde_json::from_str(&encoded).unwrap();
        let local = LogbookHeadSummary {
            logbook_id,
            head_hash: Some("abc".to_owned()),
            event_count: Some(2),
        };
        assert_eq!(
            compare_heads(&local, &decoded.logbooks[0]),
            HeadComparisonStatus::Match
        );
    }

    #[test]
    fn head_comparison_statuses() {
        let id = Uuid::new_v4();
        let local = |head: Option<&str>, count| LogbookHeadSummary {
            logbook_id: id,
            head_hash: head.map(str::to_owned),
            event_count: count,
        };
        assert_eq!(
            compare_heads(&local(Some("a"), Some(1)), &local(Some("a"), Some(1))),
            HeadComparisonStatus::Match
        );
        assert_eq!(
            compare_heads(&local(Some("a"), Some(2)), &local(Some("b"), Some(1))),
            HeadComparisonStatus::LocalAhead
        );
        assert_eq!(
            compare_heads(&local(Some("a"), Some(1)), &local(Some("b"), Some(2))),
            HeadComparisonStatus::RemoteAhead
        );
        assert_eq!(
            compare_heads(&local(Some("a"), Some(1)), &local(Some("b"), Some(1))),
            HeadComparisonStatus::Diverged
        );
        assert_eq!(
            compare_heads(&local(Some("a"), None), &local(Some("b"), None)),
            HeadComparisonStatus::Unknown
        );
    }

    #[test]
    fn event_range_request_response_serializes() {
        let logbook_id = Uuid::new_v4();
        let events = remote_chain(logbook_id, 1);
        let request = GetEventRangeRequest {
            logbook_id,
            after_hash: None,
            limit: Some(100),
        };
        let response = GetEventRangeResponse { logbook_id, events };

        let encoded_request = serde_json::to_string(&request).unwrap();
        let encoded_response = serde_json::to_string(&response).unwrap();

        assert_eq!(
            serde_json::from_str::<GetEventRangeRequest>(&encoded_request)
                .unwrap()
                .logbook_id,
            logbook_id
        );
        assert_eq!(
            serde_json::from_str::<GetEventRangeResponse>(&encoded_response)
                .unwrap()
                .events
                .len(),
            1
        );
    }

    #[test]
    fn cloud_api_models_serialize() {
        let logbook_id = Uuid::new_v4();
        let request = PairDeviceRequest {
            pairing_code: "pair".to_owned(),
            account_id: "account".to_owned(),
            user_id: "user".to_owned(),
            device_id: Uuid::new_v4(),
            device_name: "Radio Desk".to_owned(),
            requested_logbooks: vec![logbook_id],
            role_hints: vec!["logger".to_owned()],
        };
        let encoded = serde_json::to_string(&request).unwrap();
        let decoded: PairDeviceRequest = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.requested_logbooks, vec![logbook_id]);
    }

    #[test]
    fn preview_pull_with_no_missing_events_is_in_sync() {
        let logbook_id = Uuid::new_v4();
        let remote = remote_chain(logbook_id, 2);
        let response = preview_pull_from_events(
            PreviewPullRequest {
                peer_id: "peer".to_owned(),
                logbook_id,
                local_head_hash: remote.last().map(|event| event.event_hash.clone()),
            },
            &remote,
        );

        assert_eq!(response.status, ReplicationStatus::InSync);
        assert_eq!(response.missing_event_count, 0);
    }

    #[test]
    fn preview_pull_with_remote_ahead_counts_missing_events() {
        let logbook_id = Uuid::new_v4();
        let remote = remote_chain(logbook_id, 3);
        let response = preview_pull_from_events(
            PreviewPullRequest {
                peer_id: "peer".to_owned(),
                logbook_id,
                local_head_hash: Some(remote[0].event_hash.clone()),
            },
            &remote,
        );

        assert_eq!(response.status, ReplicationStatus::RemoteAhead);
        assert_eq!(response.missing_event_count, 2);
    }

    #[tokio::test]
    async fn successful_pull_appends_valid_remote_events_and_updates_projection() {
        let logbook_id = Uuid::new_v4();
        let remote = remote_chain(logbook_id, 2);
        let store = InMemoryLogbookEventStore::new();

        let response = pull_missing_events(
            &store,
            PullEventsRequest {
                peer_id: "peer".to_owned(),
                logbook_id,
                local_head_hash: None,
            },
            remote.clone(),
        )
        .await;

        assert_eq!(response.status, ReplicationStatus::Pulled);
        assert_eq!(response.accepted_count, 2);
        assert_eq!(
            store.get_head(logbook_id).await.unwrap(),
            remote.last().map(|event| event.event_hash.clone())
        );
        let projection = store.rebuild_projections(logbook_id).await.unwrap();
        assert_eq!(projection.list(false).len(), 2);
    }

    #[tokio::test]
    async fn successful_pull_accepts_verified_missing_tail() {
        let logbook_id = Uuid::new_v4();
        let remote = remote_chain(logbook_id, 3);
        let store = InMemoryLogbookEventStore::new();
        store
            .append_verified_remote_event(remote[0].clone())
            .await
            .unwrap();
        let local_head = store.get_head(logbook_id).await.unwrap();

        let response = pull_missing_events(
            &store,
            PullEventsRequest {
                peer_id: "peer".to_owned(),
                logbook_id,
                local_head_hash: local_head,
            },
            remote[1..].to_vec(),
        )
        .await;

        assert_eq!(response.status, ReplicationStatus::Pulled);
        assert_eq!(response.accepted_count, 2);
        assert_eq!(
            store.get_head(logbook_id).await.unwrap(),
            remote.last().map(|event| event.event_hash.clone())
        );
    }

    #[tokio::test]
    async fn invalid_event_hash_is_rejected() {
        let logbook_id = Uuid::new_v4();
        let mut remote = remote_chain(logbook_id, 1);
        remote[0].payload["mode"] = json!("CW");
        let store = InMemoryLogbookEventStore::new();

        let response = pull_missing_events(
            &store,
            PullEventsRequest {
                peer_id: "peer".to_owned(),
                logbook_id,
                local_head_hash: None,
            },
            remote,
        )
        .await;

        assert_eq!(response.status, ReplicationStatus::Rejected);
        assert_eq!(response.accepted_count, 0);
    }

    #[tokio::test]
    async fn broken_previous_hash_chain_is_rejected() {
        let logbook_id = Uuid::new_v4();
        let mut remote = remote_chain(logbook_id, 2);
        remote[1].previous_hash = Some("not-the-first-hash".to_owned());
        remote[1].event_hash = remote[1].calculate_hash();
        let store = InMemoryLogbookEventStore::new();

        let response = pull_missing_events(
            &store,
            PullEventsRequest {
                peer_id: "peer".to_owned(),
                logbook_id,
                local_head_hash: None,
            },
            remote,
        )
        .await;

        assert_eq!(response.status, ReplicationStatus::Diverged);
        assert_eq!(response.accepted_count, 0);
    }

    #[tokio::test]
    async fn duplicate_identical_event_is_ignored_safely() {
        let logbook_id = Uuid::new_v4();
        let remote = remote_chain(logbook_id, 1);
        let store = InMemoryLogbookEventStore::new();
        store
            .append_verified_remote_event(remote[0].clone())
            .await
            .unwrap();

        let response = pull_missing_events(
            &store,
            PullEventsRequest {
                peer_id: "peer".to_owned(),
                logbook_id,
                local_head_hash: None,
            },
            remote,
        )
        .await;

        assert_eq!(response.status, ReplicationStatus::InSync);
        assert_eq!(store.list_events(logbook_id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn duplicate_event_id_with_different_hash_is_rejected() {
        let logbook_id = Uuid::new_v4();
        let local = remote_chain(logbook_id, 1);
        let mut duplicate = new_event(logbook_id, Some(local[0].event_hash.clone()));
        duplicate.event_id = local[0].event_id;
        duplicate.event_hash = duplicate.calculate_hash();
        let remote = vec![local[0].clone(), duplicate];
        let store = InMemoryLogbookEventStore::new();
        store
            .append_verified_remote_event(local[0].clone())
            .await
            .unwrap();

        let response = pull_missing_events(
            &store,
            PullEventsRequest {
                peer_id: "peer".to_owned(),
                logbook_id,
                local_head_hash: None,
            },
            remote,
        )
        .await;

        assert_eq!(response.status, ReplicationStatus::Rejected);
        assert_eq!(store.list_events(logbook_id).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn unsupported_schema_version_is_rejected() {
        let logbook_id = Uuid::new_v4();
        let mut remote = remote_chain(logbook_id, 1);
        remote[0].schema_version = 99;
        remote[0].event_hash = remote[0].calculate_hash();
        let store = InMemoryLogbookEventStore::new();

        let response = pull_missing_events(
            &store,
            PullEventsRequest {
                peer_id: "peer".to_owned(),
                logbook_id,
                local_head_hash: None,
            },
            remote,
        )
        .await;

        assert_eq!(response.status, ReplicationStatus::Rejected);
    }

    #[test]
    fn divergence_is_detected_when_remote_lacks_local_head() {
        let logbook_id = Uuid::new_v4();
        let remote = remote_chain(logbook_id, 1);
        let response = preview_pull_from_events(
            PreviewPullRequest {
                peer_id: "peer".to_owned(),
                logbook_id,
                local_head_hash: Some("local-only-head".to_owned()),
            },
            &remote,
        );

        assert_eq!(response.status, ReplicationStatus::Diverged);
    }

    #[tokio::test]
    async fn cloud_server_rejects_unauthenticated_requests() {
        let server = cloud_server();
        let error = server
            .list_logbooks(&CloudAuth {
                sync_token: "missing".to_owned(),
            })
            .await
            .unwrap_err();

        assert_eq!(error, CloudSyncError::Unauthenticated);
    }

    #[tokio::test]
    async fn cloud_pairing_token_validation() {
        let server = cloud_server();
        let rejected = server
            .pair_device(PairDeviceRequest {
                pairing_code: "wrong".to_owned(),
                account_id: "acct".to_owned(),
                user_id: "user".to_owned(),
                device_id: Uuid::new_v4(),
                device_name: "Bad Device".to_owned(),
                requested_logbooks: vec![Uuid::new_v4()],
                role_hints: Vec::new(),
            })
            .await;

        assert!(!rejected.accepted);
    }

    #[tokio::test]
    async fn report_upload_rejects_unauthenticated_request() {
        let server = cloud_server();
        let result = server.upload_report(sample_report_request("missing")).await;
        assert_eq!(result.unwrap_err(), CloudSyncError::Unauthenticated);
    }

    #[tokio::test]
    async fn report_upload_accepts_authenticated_request_and_returns_report_id() {
        let logbook_id = Uuid::new_v4();
        let client = paired_client(cloud_server(), logbook_id).await;
        let auth = client.auth().unwrap().sync_token.clone();
        let response = client
            .upload_report(sample_report_request(&auth))
            .await
            .unwrap();

        assert!(response.report_id.starts_with("rpt-"));
        assert_eq!(response.status, DiagnosticReportStatus::Submitted);
        assert_eq!(response.bundle_hash, "hash");
    }

    #[tokio::test]
    async fn report_metadata_status_starts_submitted() {
        let server = cloud_server();
        let logbook_id = Uuid::new_v4();
        let client = paired_client(server.clone(), logbook_id).await;
        let auth = client.auth().unwrap().sync_token.clone();
        let response = client
            .upload_report(sample_report_request(&auth))
            .await
            .unwrap();
        let metadata = server
            .report_metadata(client.auth().unwrap(), response.report_id.as_str())
            .await
            .unwrap();

        assert_eq!(metadata.status, DiagnosticReportStatus::Submitted);
        assert_eq!(metadata.account_id, "acct-1");
    }

    #[tokio::test]
    async fn cloud_push_valid_events_and_preview_pull() {
        let logbook_id = Uuid::new_v4();
        let server = cloud_server();
        let client = paired_client(server, logbook_id).await;
        let events = remote_chain(logbook_id, 2);

        let push = client
            .push_events(logbook_id, events.clone())
            .await
            .unwrap();
        assert_eq!(push.accepted_count, 2);

        let preview = client.preview_pull(logbook_id, None).await.unwrap();
        assert_eq!(preview.status, ReplicationStatus::RemoteAhead);
        assert_eq!(preview.missing_event_count, 2);
    }

    #[tokio::test]
    async fn cloud_push_invalid_hash_is_rejected() {
        let logbook_id = Uuid::new_v4();
        let server = cloud_server();
        let client = paired_client(server, logbook_id).await;
        let mut events = remote_chain(logbook_id, 1);
        events[0].payload["mode"] = json!("CW");

        let push = client.push_events(logbook_id, events).await.unwrap();
        assert_eq!(push.status, ReplicationStatus::Rejected);
        assert_eq!(push.accepted_count, 0);
    }

    #[tokio::test]
    async fn cloud_push_duplicate_identical_is_ignored() {
        let logbook_id = Uuid::new_v4();
        let server = cloud_server();
        let client = paired_client(server, logbook_id).await;
        let events = remote_chain(logbook_id, 1);

        client
            .push_events(logbook_id, events.clone())
            .await
            .unwrap();
        let push = client.push_events(logbook_id, events).await.unwrap();

        assert_eq!(push.ignored_duplicate_count, 1);
    }

    #[tokio::test]
    async fn cloud_push_duplicate_id_with_different_hash_is_rejected() {
        let logbook_id = Uuid::new_v4();
        let server = cloud_server();
        let client = paired_client(server, logbook_id).await;
        let local = remote_chain(logbook_id, 1);
        client.push_events(logbook_id, local.clone()).await.unwrap();
        let mut duplicate = new_event(logbook_id, Some(local[0].event_hash.clone()));
        duplicate.event_id = local[0].event_id;
        duplicate.event_hash = duplicate.calculate_hash();

        let push = client
            .push_events(logbook_id, vec![duplicate])
            .await
            .unwrap();

        assert_eq!(push.status, ReplicationStatus::Rejected);
    }

    #[tokio::test]
    async fn desktop_queue_recovers_restart_and_drains_to_cloud_without_duplicates() {
        let logbook_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let root =
            std::env::temp_dir().join(format!("ke8ygw-ham-sync-desktop-drain-{}", Uuid::new_v4()));
        let queue_path = root.join("offline-mutations.json");
        let queue = JsonOfflineMutationQueue::new(&queue_path);
        let local_store = InMemoryLogbookEventStore::new();
        let local_events = remote_chain(logbook_id, 2);
        let now = Utc::now();
        let mut operation_ids = Vec::new();

        for (index, event) in local_events.iter().enumerate() {
            local_store
                .append_verified_remote_event(event.clone())
                .await
                .unwrap();
            let operation_id = Uuid::new_v4();
            let mutation = queue
                .enqueue_input(
                    OfflineMutationInput::new(
                        logbook_id,
                        device_id,
                        device_id,
                        OFFLINE_OP_QSO_CREATE,
                        json!({
                            "contacted_callsign": format!("K1DRAIN{index}"),
                            "mode": "SSB"
                        }),
                    )
                    .with_operation_id(operation_id)
                    .with_correlation_id(operation_id)
                    .with_idempotency_key(format!("desktop-qso-create-{index}")),
                    now + chrono::Duration::seconds(index as i64),
                )
                .unwrap();
            queue
                .record_local_event(mutation.operation_id, event, now)
                .unwrap();
            operation_ids.push(mutation.operation_id);
        }

        queue
            .mark_sending(operation_ids[0], now + chrono::Duration::seconds(10))
            .unwrap();

        let restarted_queue = JsonOfflineMutationQueue::new(&queue_path);
        let restart_time = now + chrono::Duration::seconds(20);
        assert_eq!(
            restarted_queue
                .recover_interrupted_writes(restart_time)
                .unwrap(),
            1
        );
        let recovered = restarted_queue.load_snapshot(restart_time).unwrap();
        assert_eq!(recovered.health.retrying, 1);
        assert_eq!(recovered.health.pending, 1);
        assert_eq!(recovered.health.ready_to_send, 2);

        let client = paired_client(cloud_server(), logbook_id).await;
        let listed_local_events = local_store.list_events(logbook_id).await.unwrap();
        let batch = restarted_queue
            .ready_event_batch(logbook_id, &listed_local_events, restart_time)
            .unwrap();
        assert_eq!(batch.operation_ids, operation_ids);
        assert_eq!(batch.events, local_events);
        assert!(batch.missing_local_event_operation_ids.is_empty());

        for operation_id in &batch.operation_ids {
            restarted_queue
                .mark_sending(*operation_id, restart_time)
                .unwrap();
        }
        let push = client
            .push_events(logbook_id, batch.events.clone())
            .await
            .unwrap();
        assert_eq!(push.status, ReplicationStatus::Pulled);
        assert_eq!(push.accepted_count, 2);
        assert_eq!(push.ignored_duplicate_count, 0);

        let accepted_hashes = batch
            .events
            .iter()
            .map(|event| event.event_hash.clone())
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(
            restarted_queue
                .mark_accepted_by_event_hashes(&accepted_hashes, restart_time)
                .unwrap(),
            2
        );

        let after_drain = JsonOfflineMutationQueue::new(&queue_path)
            .load_snapshot(restart_time)
            .unwrap();
        assert_eq!(after_drain.health.accepted, 2);
        assert_eq!(after_drain.health.ready_to_send, 0);
        assert_eq!(
            local_store.list_events(logbook_id).await.unwrap().len(),
            2,
            "desktop local official log must not gain duplicate events while draining"
        );

        let duplicate_push = client
            .push_events(logbook_id, batch.events.clone())
            .await
            .unwrap();
        assert_eq!(duplicate_push.accepted_count, 0);
        assert_eq!(duplicate_push.ignored_duplicate_count, 2);
        let cloud_pull = client.pull_events(logbook_id, None).await.unwrap();
        assert_eq!(cloud_pull.events.len(), 2);

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn cross_client_golden_retry_duplicate_reorder_and_restore_path() {
        let logbook_id = Uuid::new_v4();
        let qso_id = Uuid::new_v4();
        let desktop_device = Uuid::new_v4();
        let ios_device = Uuid::new_v4();
        let root = std::env::temp_dir().join(format!("ke8ygw-ham-sync-golden-{}", Uuid::new_v4()));
        let queue_path = root.join("offline-mutations.json");
        let queue = JsonOfflineMutationQueue::new(&queue_path);
        let local_store = InMemoryLogbookEventStore::new();
        let server = cloud_server();
        let desktop_client = paired_client_for_device(
            server.clone(),
            logbook_id,
            desktop_device,
            "Desktop".to_owned(),
        )
        .await;
        let ios_client =
            paired_client_for_device(server, logbook_id, ios_device, "iOS".to_owned()).await;
        let now = fixed_time(10_000);

        let created = qso_create_event(
            logbook_id,
            qso_id,
            None,
            desktop_device,
            fixed_time(-3_600),
            "K1GOLD",
        );
        let deleted = qso_tombstone_event(
            logbook_id,
            qso_id,
            Some(created.event_hash.clone()),
            EVENT_QSO_DELETED,
            desktop_device,
            fixed_time(30),
        );
        let restored = qso_tombstone_event(
            logbook_id,
            qso_id,
            Some(deleted.event_hash.clone()),
            EVENT_QSO_RESTORED,
            desktop_device,
            fixed_time(-1_800),
        );
        let local_events = vec![created.clone(), deleted.clone(), restored.clone()];
        for event in &local_events {
            local_store
                .append_verified_remote_event(event.clone())
                .await
                .unwrap();
        }
        local_store.verify_chain(logbook_id).await.unwrap();

        let create_operation = Uuid::new_v4();
        let delete_operation = Uuid::new_v4();
        let restore_operation = Uuid::new_v4();
        enqueue_event_for_sync(
            &queue,
            &created,
            OFFLINE_OP_QSO_CREATE,
            create_operation,
            now,
        );
        enqueue_event_for_sync(
            &queue,
            &deleted,
            OFFLINE_OP_QSO_DELETE,
            delete_operation,
            now + chrono::Duration::seconds(1),
        );
        enqueue_event_for_sync(
            &queue,
            &restored,
            OFFLINE_OP_QSO_RESTORE,
            restore_operation,
            now + chrono::Duration::seconds(2),
        );

        queue
            .mark_sending(create_operation, now + chrono::Duration::seconds(3))
            .unwrap();
        let restarted_queue = JsonOfflineMutationQueue::new(&queue_path);
        let recovered_at = now + chrono::Duration::seconds(4);
        assert_eq!(
            restarted_queue
                .recover_interrupted_writes(recovered_at)
                .unwrap(),
            1
        );
        restarted_queue
            .record_transient_failure(
                create_operation,
                "network unavailable",
                Some("network_unavailable".to_owned()),
                recovered_at,
            )
            .unwrap();
        let retry_at = recovered_at + chrono::Duration::seconds(6);
        let recovered = restarted_queue.load_snapshot(retry_at).unwrap();
        assert_eq!(recovered.health.retrying, 1);
        assert_eq!(recovered.health.pending, 2);
        assert_eq!(recovered.health.ready_to_send, 3);

        let listed_local_events = local_store.list_events(logbook_id).await.unwrap();
        let batch = restarted_queue
            .ready_event_batch(logbook_id, &listed_local_events, retry_at)
            .unwrap();
        assert_eq!(
            batch.operation_ids,
            vec![create_operation, delete_operation, restore_operation]
        );
        assert_eq!(batch.events, local_events);
        assert!(batch.missing_local_event_operation_ids.is_empty());

        for operation_id in &batch.operation_ids {
            restarted_queue
                .mark_sending(*operation_id, retry_at)
                .unwrap();
        }
        let push = desktop_client
            .push_events(logbook_id, batch.events.clone())
            .await
            .unwrap();
        assert_eq!(push.status, ReplicationStatus::Pulled);
        assert_eq!(push.accepted_count, 3);
        assert_eq!(push.ignored_duplicate_count, 0);

        let accepted_hashes = batch
            .events
            .iter()
            .map(|event| event.event_hash.clone())
            .collect::<HashSet<_>>();
        assert_eq!(
            restarted_queue
                .mark_accepted_by_event_hashes(&accepted_hashes, retry_at)
                .unwrap(),
            3
        );
        let drained = restarted_queue.load_snapshot(retry_at).unwrap();
        assert_eq!(drained.health.accepted, 3);
        assert_eq!(drained.health.ready_to_send, 0);

        let duplicate = desktop_client
            .push_events(logbook_id, batch.events.clone())
            .await
            .unwrap();
        assert_eq!(duplicate.accepted_count, 0);
        assert_eq!(duplicate.ignored_duplicate_count, 3);

        let ios_store = InMemoryLogbookEventStore::new();
        let pulled = ios_client.pull_events(logbook_id, None).await.unwrap();
        assert_eq!(pulled.preview.status, ReplicationStatus::RemoteAhead);
        assert_eq!(pulled.events.len(), 3);
        let applied = pull_missing_events(
            &ios_store,
            PullEventsRequest {
                peer_id: "cloud".to_owned(),
                logbook_id,
                local_head_hash: None,
            },
            pulled.events,
        )
        .await;
        assert_eq!(applied.status, ReplicationStatus::Pulled);
        assert_eq!(applied.accepted_count, 3);
        ios_store.verify_chain(logbook_id).await.unwrap();
        let projection = ios_store.rebuild_projections(logbook_id).await.unwrap();
        assert!(projection.get(qso_id).is_some());
        assert!(!projection.is_tombstoned(qso_id));

        let current_head = desktop_client
            .preview_pull(logbook_id, Some(restored.event_hash.clone()))
            .await
            .unwrap()
            .remote_head_hash
            .unwrap();
        let next_qso_id = Uuid::new_v4();
        let next_first = qso_create_event(
            logbook_id,
            next_qso_id,
            Some(current_head),
            desktop_device,
            fixed_time(120),
            "K1ORDER",
        );
        let next_second = qso_correction_event(
            logbook_id,
            next_qso_id,
            Some(next_first.event_hash.clone()),
            desktop_device,
            fixed_time(121),
            "CW",
        );
        let reordered = desktop_client
            .push_events(logbook_id, vec![next_second.clone(), next_first.clone()])
            .await
            .unwrap();
        assert_eq!(reordered.status, ReplicationStatus::Diverged);
        assert_eq!(reordered.accepted_count, 0);

        let ordered = desktop_client
            .push_events(logbook_id, vec![next_first, next_second])
            .await
            .unwrap();
        assert_eq!(ordered.status, ReplicationStatus::Pulled);
        assert_eq!(ordered.accepted_count, 2);

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn cross_client_golden_divergence_revocation_and_upgrade_review_path() {
        let logbook_id = Uuid::new_v4();
        let qso_id = Uuid::new_v4();
        let desktop_device = Uuid::new_v4();
        let ios_device = Uuid::new_v4();
        let server = cloud_server();
        let desktop_client = paired_client_for_device(
            server.clone(),
            logbook_id,
            desktop_device,
            "Desktop".to_owned(),
        )
        .await;
        let ios_client =
            paired_client_for_device(server.clone(), logbook_id, ios_device, "iOS".to_owned())
                .await;
        let base = qso_create_event(
            logbook_id,
            qso_id,
            None,
            desktop_device,
            fixed_time(0),
            "K1REVIEW",
        );
        desktop_client
            .push_events(logbook_id, vec![base.clone()])
            .await
            .unwrap();

        let remote_correction = qso_correction_event(
            logbook_id,
            qso_id,
            Some(base.event_hash.clone()),
            desktop_device,
            fixed_time(30),
            "FT8",
        );
        let remote_delete = qso_tombstone_event(
            logbook_id,
            qso_id,
            Some(remote_correction.event_hash.clone()),
            EVENT_QSO_DELETED,
            desktop_device,
            fixed_time(60),
        );
        desktop_client
            .push_events(
                logbook_id,
                vec![remote_correction.clone(), remote_delete.clone()],
            )
            .await
            .unwrap();

        let root = std::env::temp_dir().join(format!("ke8ygw-ham-sync-review-{}", Uuid::new_v4()));
        let queue_path = root.join("offline-mutations.json");
        let review_path = root.join("conflict-reviews.json");
        let queue = JsonOfflineMutationQueue::new(&queue_path);
        let local_correction_operation = Uuid::new_v4();
        let local_correction = queue
            .enqueue_input(
                OfflineMutationInput::new(
                    logbook_id,
                    ios_device,
                    ios_device,
                    OFFLINE_OP_QSO_CORRECT,
                    json!({ "mode": "CW", "qso_id": qso_id }),
                )
                .with_operation_id(local_correction_operation)
                .with_entity_id(Some(qso_id))
                .with_idempotency_key("ios-local-correction"),
                fixed_time(90),
            )
            .unwrap();
        let preview = ios_client
            .preview_pull(logbook_id, Some(base.event_hash.clone()))
            .await
            .unwrap();
        assert_eq!(preview.status, ReplicationStatus::RemoteAhead);
        let report = conflict_report_from_preview(
            &preview,
            std::slice::from_ref(&local_correction),
            fixed_time(91),
        );
        assert!(report.conflicts.iter().any(|conflict| {
            conflict.kind == SyncConflictKind::ConcurrentCorrection
                && conflict.requires_user_action
                && !conflict.safe_auto_merge
        }));
        assert!(report.conflicts.iter().any(|conflict| {
            conflict.kind == SyncConflictKind::TombstoneRestore
                && conflict.requires_user_action
                && !conflict.safe_auto_merge
        }));

        let report_json = serde_json::to_value(&report).unwrap();
        let round_tripped_report: SyncConflictReport =
            serde_json::from_value(report_json.clone()).unwrap();
        assert_eq!(
            round_tripped_report, report,
            "client-ready conflict reports must survive exact JSON round trips"
        );

        let desktop_review_store = JsonConflictReviewStore::new(&review_path);
        let desktop_review = desktop_review_store
            .create_review(report.clone(), fixed_time(92))
            .unwrap();
        let unsafe_desktop_pull = desktop_review_store.resolve_review(
            desktop_review.review_id,
            ManualConflictResolution::new(ManualConflictResolutionChoice::PullRemoteAfterReview)
                .with_note("operator tried to pull a conflicting branch")
                .with_resolved_by_device_id(desktop_device),
            fixed_time(93),
        );
        assert!(matches!(
            unsafe_desktop_pull,
            Err(ConflictReviewError::UnsafeResolution {
                resolution: ManualConflictResolutionChoice::PullRemoteAfterReview,
                status: ReplicationStatus::RemoteAhead
            })
        ));
        let desktop_resolved = desktop_review_store
            .resolve_review(
                desktop_review.review_id,
                ManualConflictResolution::new(
                    ManualConflictResolutionChoice::MarkUserActionRequired,
                )
                .with_note("Desktop kept local pending work visible for manual review")
                .with_resolved_by_device_id(desktop_device),
                fixed_time(94),
            )
            .unwrap();
        assert_eq!(desktop_resolved.status, ConflictReviewStatus::Resolved);
        let marked_local_correction = queue
            .mark_user_action_required(
                local_correction_operation,
                "manual conflict review required",
                Some("manual_conflict_review".to_owned()),
                fixed_time(95),
            )
            .unwrap();
        assert_eq!(
            marked_local_correction.status,
            OfflineMutationStatus::UserActionRequired
        );
        assert_eq!(
            queue
                .load_snapshot(fixed_time(95))
                .unwrap()
                .health
                .user_action_required,
            1
        );

        let ios_review_store = JsonConflictReviewStore::new(root.join("ios-conflict-reviews.json"));
        let ios_report: SyncConflictReport = serde_json::from_value(report_json).unwrap();
        let ios_review = ios_review_store
            .create_review(ios_report.clone(), fixed_time(96))
            .unwrap();
        assert_eq!(ios_review.report, report);
        let ios_resolved = ios_review_store
            .resolve_review(
                ios_review.review_id,
                ManualConflictResolution::new(
                    ManualConflictResolutionChoice::CreateCorrectiveEvents,
                )
                .with_corrective_event_hashes(vec![
                    "ios-operator-reviewed-corrective-hash".to_owned()
                ])
                .with_resolved_by_device_id(ios_device)
                .with_note("iOS resolved with corrective proposal hashes"),
                fixed_time(97),
            )
            .unwrap();
        assert_eq!(ios_resolved.status, ConflictReviewStatus::Resolved);
        assert_eq!(ios_review_store.load_snapshot().unwrap().health.resolved, 1);

        let ios_store = InMemoryLogbookEventStore::new();
        ios_store
            .append_verified_remote_event(base.clone())
            .await
            .unwrap();
        let ios_local_correction = qso_correction_event(
            logbook_id,
            qso_id,
            Some(base.event_hash.clone()),
            ios_device,
            fixed_time(120),
            "RTTY",
        );
        ios_store
            .append_verified_remote_event(ios_local_correction.clone())
            .await
            .unwrap();
        let divergent_preview = ios_client
            .preview_pull(logbook_id, Some(ios_local_correction.event_hash.clone()))
            .await
            .unwrap();
        assert_eq!(divergent_preview.status, ReplicationStatus::Diverged);
        let divergent_report = conflict_report_from_preview(
            &divergent_preview,
            std::slice::from_ref(&marked_local_correction),
            fixed_time(121),
        );
        assert!(divergent_report
            .conflicts
            .iter()
            .any(|conflict| conflict.kind == SyncConflictKind::DivergentHeads));
        let before_divergent_pull_head = ios_store.get_head(logbook_id).await.unwrap();
        let before_divergent_pull_events = ios_store.list_events(logbook_id).await.unwrap();
        let divergent_pull = pull_missing_events(
            &ios_store,
            PullEventsRequest {
                peer_id: "cloud".to_owned(),
                logbook_id,
                local_head_hash: before_divergent_pull_head.clone(),
            },
            vec![
                base.clone(),
                remote_correction.clone(),
                remote_delete.clone(),
            ],
        )
        .await;
        assert_eq!(divergent_pull.status, ReplicationStatus::Diverged);
        assert_eq!(divergent_pull.accepted_count, 0);
        assert_eq!(
            ios_store.get_head(logbook_id).await.unwrap(),
            before_divergent_pull_head
        );
        assert_eq!(
            ios_store.list_events(logbook_id).await.unwrap(),
            before_divergent_pull_events,
            "divergent branch review must not silently append remote events"
        );
        ios_store.verify_chain(logbook_id).await.unwrap();

        let legacy_queue = JsonOfflineMutationQueue::new(root.join("legacy-offline.json"));
        let legacy_operation = Uuid::new_v4();
        let legacy = json!({
            "version": OFFLINE_QUEUE_LEGACY_V0_2_FILE_VERSION,
            "pending_operations": [{
                "operation_id": legacy_operation,
                "device_id": ios_device,
                "logbook_id": logbook_id,
                "operation_type": OFFLINE_OP_QSO_DELETE,
                "payload": { "qso_id": qso_id },
                "local_event_hash": remote_delete.event_hash
            }]
        });
        std::fs::write(
            legacy_queue.path(),
            serde_json::to_vec_pretty(&legacy).unwrap(),
        )
        .unwrap();
        let migration = legacy_queue.recover_or_initialize(fixed_time(130)).unwrap();
        assert!(migration.migrated_v0_2_file);
        assert_eq!(migration.migrated_legacy_mutations, 1);
        let migrated_snapshot = legacy_queue.load_snapshot(fixed_time(130)).unwrap();
        assert_eq!(migrated_snapshot.health.retrying, 1);
        assert_eq!(
            migrated_snapshot.mutations[0].operation_id,
            legacy_operation
        );

        let trust_store = JsonLanTrustStore::new(root.join("lan-trust.json"));
        let pairing = trust_store
            .issue_pairing_token(desktop_device, logbook_id, "ios", true, fixed_time(140))
            .unwrap();
        trust_store
            .accept_pairing_token(
                LanPairingAcceptance {
                    token_id: pairing.token_id,
                    pairing_code: pairing.pairing_code,
                    peer_device_id: ios_device,
                    peer_display_name: "iOS".to_owned(),
                    requested_logbooks: vec![logbook_id],
                    public_key_fingerprint: Some("ios-fingerprint".to_owned()),
                    auth_credential_id: Some(Uuid::new_v4()),
                },
                fixed_time(141),
            )
            .unwrap();
        trust_store
            .authorize_peer(ios_device, logbook_id, "nonce-1", fixed_time(142))
            .unwrap();
        trust_store
            .revoke_device(ios_device, fixed_time(143))
            .unwrap();
        assert!(matches!(
            trust_store.authorize_peer(ios_device, logbook_id, "nonce-2", fixed_time(144)),
            Err(LanTrustError::DeviceRevoked(device)) if device == ios_device
        ));

        let _ = std::fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn cloud_pull_missing_events() {
        let logbook_id = Uuid::new_v4();
        let server = cloud_server();
        let client = paired_client(server, logbook_id).await;
        let events = remote_chain(logbook_id, 2);
        client
            .push_events(logbook_id, events.clone())
            .await
            .unwrap();

        let pull = client.pull_events(logbook_id, None).await.unwrap();

        assert_eq!(pull.events.len(), 2);
        assert_eq!(pull.preview.missing_event_count, 2);
    }

    #[tokio::test]
    async fn cloud_unauthorized_logbook_access_is_rejected() {
        let logbook_id = Uuid::new_v4();
        let other_logbook = Uuid::new_v4();
        let server = cloud_server();
        let client = paired_client(server, logbook_id).await;

        let error = client.preview_pull(other_logbook, None).await.unwrap_err();

        assert_eq!(error, CloudSyncError::UnauthorizedLogbook(other_logbook));
    }

    #[tokio::test]
    async fn cloud_push_divergence_response() {
        let logbook_id = Uuid::new_v4();
        let server = cloud_server();
        let client = paired_client(server, logbook_id).await;
        let server_chain = remote_chain(logbook_id, 1);
        client.push_events(logbook_id, server_chain).await.unwrap();
        let divergent = remote_chain(logbook_id, 1);

        let push = client.push_events(logbook_id, divergent).await.unwrap();

        assert_eq!(push.status, ReplicationStatus::Diverged);
    }

    #[cfg(feature = "surreal-storage")]
    #[tokio::test]
    async fn durable_sync_state_survives_store_reload() {
        let paths = durable_paths("state-survives-restart");
        let logbook_id = Uuid::new_v4();
        let server = durable_server(&paths);
        let session = server
            .pair_device(pair_request(logbook_id, Uuid::new_v4()))
            .await
            .session
            .unwrap();
        let auth = CloudAuth {
            sync_token: session.sync_token.clone(),
        };
        let events = remote_chain(logbook_id, 2);

        let push = server
            .push_events(CloudPushEventsRequest {
                auth: auth.clone(),
                logbook_id,
                events: events.clone(),
            })
            .await
            .unwrap();
        assert_eq!(push.accepted_count, 2);
        let preview = server
            .preview_pull(CloudPreviewPullRequest {
                auth: auth.clone(),
                logbook_id,
                local_head_hash: None,
            })
            .await
            .unwrap();
        assert_eq!(preview.status, ReplicationStatus::RemoteAhead);
        assert_eq!(preview.missing_event_count, 2);

        let duplicate = server
            .push_events(CloudPushEventsRequest {
                auth,
                logbook_id,
                events,
            })
            .await
            .unwrap();
        assert_eq!(duplicate.accepted_count, 0);
        assert_eq!(duplicate.ignored_duplicate_count, 2);
    }

    #[cfg(feature = "surreal-storage")]
    #[tokio::test]
    async fn durable_sync_rejects_revoked_device_after_store_reload() {
        let paths = durable_paths("revoked-device");
        let logbook_id = Uuid::new_v4();
        let device_id = Uuid::new_v4();
        let server = durable_server(&paths);
        let session = server
            .pair_device(pair_request(logbook_id, device_id))
            .await
            .session
            .unwrap();
        let auth = CloudAuth {
            sync_token: session.sync_token,
        };

        server.revoke_device(device_id).unwrap();
        let error = server.status(Some(&auth)).await.unwrap_err();

        assert_eq!(error, CloudSyncError::Unauthenticated);
    }

    #[cfg(feature = "surreal-storage")]
    #[tokio::test]
    async fn durable_sync_rejects_invalid_chain_after_store_reload() {
        let paths = durable_paths("invalid-chain");
        let logbook_id = Uuid::new_v4();
        let server = durable_server(&paths);
        let session = server
            .pair_device(pair_request(logbook_id, Uuid::new_v4()))
            .await
            .session
            .unwrap();
        let auth = CloudAuth {
            sync_token: session.sync_token.clone(),
        };
        let first = remote_chain(logbook_id, 1);
        server
            .push_events(CloudPushEventsRequest {
                auth: auth.clone(),
                logbook_id,
                events: first,
            })
            .await
            .unwrap();
        let mut broken = new_event(logbook_id, Some("not-the-server-head".to_owned()));
        broken.event_hash = broken.calculate_hash();
        let push = server
            .push_events(CloudPushEventsRequest {
                auth,
                logbook_id,
                events: vec![broken],
            })
            .await
            .unwrap();

        assert_eq!(push.status, ReplicationStatus::Diverged);
        assert_eq!(push.accepted_count, 0);
    }

    #[cfg(feature = "surreal-storage")]
    #[tokio::test]
    async fn durable_report_metadata_and_payload_survive_store_reload() {
        let paths = durable_paths("reports");
        let logbook_id = Uuid::new_v4();
        let server = durable_server(&paths);
        let session = server
            .pair_device(pair_request(logbook_id, Uuid::new_v4()))
            .await
            .session
            .unwrap();
        let auth = CloudAuth {
            sync_token: session.sync_token.clone(),
        };
        let mut request = sample_report_request(&auth.sync_token);
        request.bundle_bytes = b"redacted diagnostic payload".to_vec();
        request.bundle_hash = "redacted-hash".to_owned();

        let upload = server.upload_report(request).await.unwrap();
        let metadata = server
            .report_metadata(&auth, upload.report_id.as_str())
            .await
            .unwrap();
        let payload = server
            .report_bundle_bytes(upload.report_id.as_str())
            .unwrap();

        assert_eq!(metadata.status, DiagnosticReportStatus::Submitted);
        assert_eq!(metadata.bundle_hash, "redacted-hash");
        assert_eq!(payload, b"redacted diagnostic payload");
        assert!(!String::from_utf8_lossy(&payload).contains("super-secret"));
    }

    #[cfg(feature = "surreal-storage")]
    #[test]
    fn durable_provider_setting_survives_store_reload_without_secrets() {
        let paths = durable_paths("provider-setting");
        let logbook_id = Uuid::new_v4();
        let server = durable_server(&paths);
        server
            .save_provider_setting(ProviderSettingMetadata {
                account_id: "acct-1".to_owned(),
                logbook_id: Some(logbook_id),
                provider_id: "lotw".to_owned(),
                enabled: true,
                credential_id: Some("cred-lotw-1".to_owned()),
                settings: serde_json::json!({
                    "station_location": "Home",
                    "credential_ref": "cred-lotw-1"
                }),
            })
            .unwrap();
        let setting = server.provider_setting("acct-1", "lotw").unwrap().unwrap();
        let serialized = serde_json::to_string(&setting).unwrap();

        assert_eq!(setting.credential_id.as_deref(), Some("cred-lotw-1"));
        assert!(setting.enabled);
        assert!(!serialized.contains("super-secret"));
        assert!(!serialized.contains("password"));
    }

    #[cfg(feature = "surreal-storage")]
    #[test]
    fn durable_upload_queue_history_survives_store_reload() {
        let paths = durable_paths("upload-history");
        let logbook_id = Uuid::new_v4();
        let server = durable_server(&paths);
        let item = UploadQueueMetadata {
            account_id: "acct-1".to_owned(),
            logbook_id,
            upload_id: "upload-1".to_owned(),
            provider_id: "clublog".to_owned(),
            status: "failed".to_owned(),
            qso_count: 3,
            last_error: Some("provider rejected record".to_owned()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        server.save_upload_queue_item(item.clone()).unwrap();

        let restored = server
            .upload_queue_item("acct-1", "upload-1")
            .unwrap()
            .unwrap();

        assert_eq!(restored.upload_id, item.upload_id);
        assert_eq!(restored.provider_id, "clublog");
        assert_eq!(restored.qso_count, 3);
        assert_eq!(
            restored.last_error.as_deref(),
            Some("provider rejected record")
        );
    }
}
