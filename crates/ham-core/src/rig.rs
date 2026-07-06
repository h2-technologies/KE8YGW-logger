use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::sync::{broadcast, RwLock};
use uuid::Uuid;

use crate::{BusEvent, EventBus, EventBusError, RuntimeEventEnvelope, RuntimeEventSeverity};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RigConnectionType {
    Mock,
    Serial,
    Tcp,
    Hamlib,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RigConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigDevice {
    pub rig_id: Uuid,
    pub display_name: String,
    pub provider: String,
    pub connection_type: RigConnectionType,
    pub connection_status: RigConnectionStatus,
    pub model: Option<String>,
    pub serial_port: Option<String>,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub baud_rate: Option<u32>,
    pub civ_address: Option<String>,
    pub capabilities: Vec<String>,
    pub last_seen: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigState {
    pub rig_id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub frequency_hz: Option<u64>,
    pub band: Option<String>,
    pub mode: Option<String>,
    pub submode: Option<String>,
    pub vfo: Option<String>,
    pub split_enabled: Option<bool>,
    pub tx_frequency_hz: Option<u64>,
    pub rx_frequency_hz: Option<u64>,
    pub power_watts: Option<f32>,
    pub ptt: Option<bool>,
    pub s_meter: Option<f32>,
    pub raw_provider_state: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RigProviderStatus {
    pub provider_id: String,
    pub healthy: bool,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigAutofillSuggestion {
    pub source: String,
    pub rig_id: Uuid,
    pub frequency_hz: Option<u64>,
    pub band: Option<String>,
    pub mode: Option<String>,
    pub submode: Option<String>,
}

#[derive(Debug, Error)]
pub enum RigError {
    #[error("rig not connected")]
    NotConnected,
    #[error("unsupported rig command: {0}")]
    UnsupportedCommand(String),
    #[error("rig provider error: {0}")]
    Provider(String),
    #[error("event bus error: {0}")]
    EventBus(#[from] EventBusError),
}

#[async_trait]
pub trait RigProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn list_supported_rigs(&self) -> Vec<RigDevice>;
    async fn connect(&self, rig_id: Uuid) -> Result<RigDevice, RigError>;
    async fn disconnect(&self, rig_id: Uuid) -> Result<RigDevice, RigError>;
    async fn get_state(&self, rig_id: Uuid) -> Result<RigState, RigError>;
    fn subscribe_state(&self) -> broadcast::Receiver<RigState>;
    async fn set_frequency(&self, rig_id: Uuid, frequency_hz: u64) -> Result<RigState, RigError>;
    async fn set_mode(&self, rig_id: Uuid, mode: &str) -> Result<RigState, RigError>;
    async fn set_ptt(&self, rig_id: Uuid, ptt: bool) -> Result<RigState, RigError>;
    async fn provider_status(&self) -> RigProviderStatus;
}

#[derive(Debug)]
struct MockRigInner {
    device: RigDevice,
    state: RigState,
}

#[derive(Debug)]
pub struct MockRigProvider {
    inner: RwLock<MockRigInner>,
    sender: broadcast::Sender<RigState>,
}

impl Default for MockRigProvider {
    fn default() -> Self {
        let rig_id = Uuid::new_v4();
        let (sender, _) = broadcast::channel(16);
        Self {
            inner: RwLock::new(MockRigInner {
                device: RigDevice {
                    rig_id,
                    display_name: "Mock HF Rig".to_owned(),
                    provider: "mock-rig".to_owned(),
                    connection_type: RigConnectionType::Mock,
                    connection_status: RigConnectionStatus::Disconnected,
                    model: Some("Mock-1000".to_owned()),
                    serial_port: None,
                    host: None,
                    port: None,
                    baud_rate: None,
                    civ_address: None,
                    capabilities: vec![
                        "frequency".to_owned(),
                        "mode".to_owned(),
                        "ptt".to_owned(),
                        "split".to_owned(),
                    ],
                    last_seen: None,
                    error: None,
                },
                state: RigState {
                    rig_id,
                    timestamp: Utc::now(),
                    frequency_hz: Some(14_250_000),
                    band: Some("20m".to_owned()),
                    mode: Some("SSB".to_owned()),
                    submode: None,
                    vfo: Some("A".to_owned()),
                    split_enabled: Some(false),
                    tx_frequency_hz: None,
                    rx_frequency_hz: Some(14_250_000),
                    power_watts: Some(100.0),
                    ptt: Some(false),
                    s_meter: Some(0.0),
                    raw_provider_state: Some(json!({"mock": true})),
                },
            }),
            sender,
        }
    }
}

#[async_trait]
impl RigProvider for MockRigProvider {
    fn provider_id(&self) -> &str {
        "mock-rig"
    }

    async fn list_supported_rigs(&self) -> Vec<RigDevice> {
        vec![self.inner.read().await.device.clone()]
    }

    async fn connect(&self, rig_id: Uuid) -> Result<RigDevice, RigError> {
        let mut inner = self.inner.write().await;
        ensure_rig(&inner.device, rig_id)?;
        inner.device.connection_status = RigConnectionStatus::Connected;
        inner.device.last_seen = Some(Utc::now());
        inner.device.error = None;
        inner.state.timestamp = Utc::now();
        let _ = self.sender.send(inner.state.clone());
        Ok(inner.device.clone())
    }

    async fn disconnect(&self, rig_id: Uuid) -> Result<RigDevice, RigError> {
        let mut inner = self.inner.write().await;
        ensure_rig(&inner.device, rig_id)?;
        inner.device.connection_status = RigConnectionStatus::Disconnected;
        inner.device.last_seen = Some(Utc::now());
        Ok(inner.device.clone())
    }

    async fn get_state(&self, rig_id: Uuid) -> Result<RigState, RigError> {
        let inner = self.inner.read().await;
        ensure_rig(&inner.device, rig_id)?;
        if inner.device.connection_status != RigConnectionStatus::Connected {
            return Err(RigError::NotConnected);
        }
        Ok(inner.state.clone())
    }

    fn subscribe_state(&self) -> broadcast::Receiver<RigState> {
        self.sender.subscribe()
    }

    async fn set_frequency(&self, rig_id: Uuid, frequency_hz: u64) -> Result<RigState, RigError> {
        let mut inner = self.inner.write().await;
        ensure_rig(&inner.device, rig_id)?;
        inner.state.frequency_hz = Some(frequency_hz);
        inner.state.rx_frequency_hz = Some(frequency_hz);
        inner.state.band = infer_band(frequency_hz).map(str::to_owned);
        inner.state.timestamp = Utc::now();
        let state = inner.state.clone();
        let _ = self.sender.send(state.clone());
        Ok(state)
    }

    async fn set_mode(&self, rig_id: Uuid, mode: &str) -> Result<RigState, RigError> {
        let mut inner = self.inner.write().await;
        ensure_rig(&inner.device, rig_id)?;
        inner.state.mode = Some(mode.trim().to_ascii_uppercase());
        inner.state.timestamp = Utc::now();
        let state = inner.state.clone();
        let _ = self.sender.send(state.clone());
        Ok(state)
    }

    async fn set_ptt(&self, rig_id: Uuid, ptt: bool) -> Result<RigState, RigError> {
        let mut inner = self.inner.write().await;
        ensure_rig(&inner.device, rig_id)?;
        inner.state.ptt = Some(ptt);
        inner.state.timestamp = Utc::now();
        let state = inner.state.clone();
        let _ = self.sender.send(state.clone());
        Ok(state)
    }

    async fn provider_status(&self) -> RigProviderStatus {
        RigProviderStatus {
            provider_id: self.provider_id().to_owned(),
            healthy: true,
            message: "Mock rig provider ready".to_owned(),
        }
    }
}

#[derive(Debug, Default)]
pub struct HamlibProviderStub;

#[async_trait]
impl RigProvider for HamlibProviderStub {
    fn provider_id(&self) -> &str {
        "hamlib-stub"
    }

    async fn list_supported_rigs(&self) -> Vec<RigDevice> {
        Vec::new()
    }

    async fn connect(&self, _rig_id: Uuid) -> Result<RigDevice, RigError> {
        Err(RigError::Provider(
            "Hamlib is not configured in the MVP build".to_owned(),
        ))
    }

    async fn disconnect(&self, _rig_id: Uuid) -> Result<RigDevice, RigError> {
        Err(RigError::Provider(
            "Hamlib is not configured in the MVP build".to_owned(),
        ))
    }

    async fn get_state(&self, _rig_id: Uuid) -> Result<RigState, RigError> {
        Err(RigError::Provider(
            "Hamlib is not configured in the MVP build".to_owned(),
        ))
    }

    fn subscribe_state(&self) -> broadcast::Receiver<RigState> {
        let (sender, receiver) = broadcast::channel(1);
        drop(sender);
        receiver
    }

    async fn set_frequency(&self, _rig_id: Uuid, _frequency_hz: u64) -> Result<RigState, RigError> {
        Err(RigError::UnsupportedCommand("set_frequency".to_owned()))
    }

    async fn set_mode(&self, _rig_id: Uuid, _mode: &str) -> Result<RigState, RigError> {
        Err(RigError::UnsupportedCommand("set_mode".to_owned()))
    }

    async fn set_ptt(&self, _rig_id: Uuid, _ptt: bool) -> Result<RigState, RigError> {
        Err(RigError::UnsupportedCommand("set_ptt".to_owned()))
    }

    async fn provider_status(&self) -> RigProviderStatus {
        RigProviderStatus {
            provider_id: self.provider_id().to_owned(),
            healthy: false,
            message: "Hamlib support is a clean stub; no Hamlib install is required for CI"
                .to_owned(),
        }
    }
}

pub fn infer_band(frequency_hz: u64) -> Option<&'static str> {
    match frequency_hz {
        1_800_000..=2_000_000 => Some("160m"),
        3_500_000..=4_000_000 => Some("80m"),
        5_330_000..=5_407_000 => Some("60m"),
        7_000_000..=7_300_000 => Some("40m"),
        10_100_000..=10_150_000 => Some("30m"),
        14_000_000..=14_350_000 => Some("20m"),
        18_068_000..=18_168_000 => Some("17m"),
        21_000_000..=21_450_000 => Some("15m"),
        24_890_000..=24_990_000 => Some("12m"),
        28_000_000..=29_700_000 => Some("10m"),
        50_000_000..=54_000_000 => Some("6m"),
        144_000_000..=148_000_000 => Some("2m"),
        222_000_000..=225_000_000 => Some("1.25m"),
        420_000_000..=450_000_000 => Some("70cm"),
        _ => None,
    }
}

pub fn suggestion_from_rig_state(state: &RigState) -> RigAutofillSuggestion {
    RigAutofillSuggestion {
        source: "Rig".to_owned(),
        rig_id: state.rig_id,
        frequency_hz: state.frequency_hz,
        band: state
            .band
            .clone()
            .or_else(|| state.frequency_hz.and_then(infer_band).map(str::to_owned)),
        mode: state.mode.clone(),
        submode: state.submode.clone(),
    }
}

pub fn apply_rig_suggestion_to_form(
    frequency_hz: Option<u64>,
    band: Option<&str>,
    mode: Option<&str>,
    submode: Option<&str>,
    suggestion: &RigAutofillSuggestion,
    explicit_refresh: bool,
) -> (Option<u64>, Option<String>, Option<String>, Option<String>) {
    (
        if explicit_refresh || frequency_hz.is_none() {
            suggestion.frequency_hz
        } else {
            frequency_hz
        },
        if explicit_refresh || band.is_none_or(str::is_empty) {
            suggestion.band.clone()
        } else {
            band.map(str::to_owned)
        },
        if explicit_refresh || mode.is_none_or(str::is_empty) {
            suggestion.mode.clone()
        } else {
            mode.map(str::to_owned)
        },
        if explicit_refresh || submode.is_none_or(str::is_empty) {
            suggestion.submode.clone()
        } else {
            submode.map(str::to_owned)
        },
    )
}

pub async fn publish_rig_runtime_event<B: EventBus>(
    bus: &B,
    device_id: Uuid,
    event_type: impl Into<String>,
    severity: RuntimeEventSeverity,
    summary: impl Into<String>,
    payload: Option<Value>,
    error: Option<String>,
) -> Result<(), EventBusError> {
    bus.publish(BusEvent::Runtime(RuntimeEventEnvelope::new(
        event_type,
        severity,
        "plugin.rig-control",
        Some("plugin.rig-control".to_owned()),
        Uuid::new_v4(),
        Uuid::new_v4(),
        device_id,
        Some("dashboard".to_owned()),
        summary.into(),
        payload,
        error,
    )))
    .await?;
    Ok(())
}

fn ensure_rig(device: &RigDevice, rig_id: Uuid) -> Result<(), RigError> {
    if device.rig_id != rig_id {
        return Err(RigError::Provider(format!("unknown rig_id {rig_id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemoryEventBus, InMemoryLogbookEventStore, LogbookEventStore};

    #[test]
    fn rig_state_serializes() {
        let state = RigState {
            rig_id: Uuid::new_v4(),
            timestamp: Utc::now(),
            frequency_hz: Some(14_250_000),
            band: Some("20m".to_owned()),
            mode: Some("SSB".to_owned()),
            submode: None,
            vfo: Some("A".to_owned()),
            split_enabled: Some(false),
            tx_frequency_hz: None,
            rx_frequency_hz: Some(14_250_000),
            power_watts: Some(100.0),
            ptt: Some(false),
            s_meter: Some(3.0),
            raw_provider_state: Some(json!({"safe": true})),
        };
        let encoded = serde_json::to_string(&state).unwrap();
        let decoded: RigState = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.frequency_hz, Some(14_250_000));
    }

    #[tokio::test]
    async fn mock_provider_connect_disconnect_and_state() {
        let provider = MockRigProvider::default();
        let rig = provider.list_supported_rigs().await.remove(0);
        assert_eq!(rig.connection_status, RigConnectionStatus::Disconnected);
        let connected = provider.connect(rig.rig_id).await.unwrap();
        assert_eq!(connected.connection_status, RigConnectionStatus::Connected);
        let state = provider.get_state(rig.rig_id).await.unwrap();
        assert_eq!(state.band.as_deref(), Some("20m"));
        let disconnected = provider.disconnect(rig.rig_id).await.unwrap();
        assert_eq!(
            disconnected.connection_status,
            RigConnectionStatus::Disconnected
        );
    }

    #[tokio::test]
    async fn mock_provider_emits_state_changes() {
        let provider = MockRigProvider::default();
        let rig = provider.list_supported_rigs().await.remove(0);
        provider.connect(rig.rig_id).await.unwrap();
        let mut rx = provider.subscribe_state();
        provider.set_frequency(rig.rig_id, 7_200_000).await.unwrap();
        let state = rx.recv().await.unwrap();
        assert_eq!(state.band.as_deref(), Some("40m"));
    }

    #[tokio::test]
    async fn rig_runtime_events_publish() {
        let bus = InMemoryEventBus::default();
        let mut rx = bus.subscribe();
        publish_rig_runtime_event(
            &bus,
            Uuid::new_v4(),
            "rig.state.changed",
            RuntimeEventSeverity::Info,
            "state changed",
            Some(json!({"frequency_hz": 14_250_000, "api_key": "secret"})),
            None,
        )
        .await
        .unwrap();
        assert!(
            matches!(rx.recv().await.unwrap(), BusEvent::Runtime(event) if event.event_type == "rig.state.changed")
        );
    }

    #[test]
    fn infers_common_bands() {
        assert_eq!(infer_band(1_900_000), Some("160m"));
        assert_eq!(infer_band(3_800_000), Some("80m"));
        assert_eq!(infer_band(5_357_000), Some("60m"));
        assert_eq!(infer_band(7_200_000), Some("40m"));
        assert_eq!(infer_band(10_120_000), Some("30m"));
        assert_eq!(infer_band(14_250_000), Some("20m"));
        assert_eq!(infer_band(18_100_000), Some("17m"));
        assert_eq!(infer_band(21_300_000), Some("15m"));
        assert_eq!(infer_band(24_950_000), Some("12m"));
        assert_eq!(infer_band(28_500_000), Some("10m"));
        assert_eq!(infer_band(50_125_000), Some("6m"));
        assert_eq!(infer_band(146_520_000), Some("2m"));
        assert_eq!(infer_band(223_500_000), Some("1.25m"));
        assert_eq!(infer_band(432_100_000), Some("70cm"));
    }

    #[test]
    fn logger_autofill_respects_manual_fields_unless_explicit() {
        let suggestion = RigAutofillSuggestion {
            source: "Rig".to_owned(),
            rig_id: Uuid::new_v4(),
            frequency_hz: Some(14_250_000),
            band: Some("20m".to_owned()),
            mode: Some("SSB".to_owned()),
            submode: None,
        };
        let untouched = apply_rig_suggestion_to_form(
            Some(7_200_000),
            Some("40m"),
            Some("CW"),
            None,
            &suggestion,
            false,
        );
        assert_eq!(untouched.0, Some(7_200_000));
        assert_eq!(untouched.2.as_deref(), Some("CW"));
        let refreshed = apply_rig_suggestion_to_form(
            Some(7_200_000),
            Some("40m"),
            Some("CW"),
            None,
            &suggestion,
            true,
        );
        assert_eq!(refreshed.0, Some(14_250_000));
        assert_eq!(refreshed.2.as_deref(), Some("SSB"));
    }

    #[tokio::test]
    async fn rig_plugin_does_not_write_official_events_directly() {
        let store = InMemoryLogbookEventStore::new();
        let provider = MockRigProvider::default();
        let rig = provider.list_supported_rigs().await.remove(0);
        provider.connect(rig.rig_id).await.unwrap();
        provider
            .set_frequency(rig.rig_id, 14_250_000)
            .await
            .unwrap();
        assert!(store.list_events(Uuid::new_v4()).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn hamlib_stub_is_ci_safe() {
        let provider = HamlibProviderStub;
        let status = provider.provider_status().await;
        assert!(!status.healthy);
        assert!(provider.connect(Uuid::new_v4()).await.is_err());
    }
}
