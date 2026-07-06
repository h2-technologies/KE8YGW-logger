//! Local-first LAN discovery and sync handshake primitives.

use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    sync::Arc,
    time::Duration,
};

use chrono::{DateTime, Utc};
use ham_core::{
    validate_supported_remote_event, CoreEventEnvelope, InMemoryLogbookEventStore,
    LogbookEventStore, StoreError,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;
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

fn cloud_store_error(error: StoreError) -> CloudSyncError {
    CloudSyncError::Store(error.to_string())
}

#[derive(Debug, Error)]
pub enum DiscoveryServiceError {
    #[error("discovery I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("discovery serialization error: {0}")]
    Serde(#[from] serde_json::Error),
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
        socket.set_multicast_loop_v4(false)?;
        socket.send_to(&bytes, ipv4)?;

        let ipv6 = SocketAddr::new(
            IpAddr::V6(self.config.ipv6_multicast_address),
            self.config.discovery_port,
        );
        if let Ok(socket) = UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0)) {
            let _ = socket.send_to(&bytes, ipv6);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ham_core::{CoreEventEnvelope, InMemoryLogbookEventStore, NewLogbookEvent};
    use serde_json::json;

    fn local() -> LocalPeerIdentity {
        LocalPeerIdentity::new("Local", Some(9738))
    }

    fn new_event(logbook_id: Uuid, previous_hash: Option<String>) -> CoreEventEnvelope {
        CoreEventEnvelope::from_new(
            NewLogbookEvent {
                event_type: "official.log.qso.created".to_owned(),
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
        let mut client = CloudSyncClient::in_memory(server);
        client
            .pair(PairDeviceRequest {
                pairing_code: "local-dev-pairing-code".to_owned(),
                account_id: "acct-1".to_owned(),
                user_id: "user-1".to_owned(),
                device_id: Uuid::new_v4(),
                device_name: "Test Device".to_owned(),
                requested_logbooks: vec![logbook_id],
                role_hints: vec!["admin".to_owned()],
            })
            .await
            .unwrap();
        client
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
}
