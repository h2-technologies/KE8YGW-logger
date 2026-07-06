//! Local-first LAN discovery and sync handshake primitives.

use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket},
    time::Duration,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
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

    fn local() -> LocalPeerIdentity {
        LocalPeerIdentity::new("Local", Some(9738))
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
}
