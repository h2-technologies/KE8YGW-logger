//! Unified provider/service framework for plugin-owned integrations.
//!
//! The framework keeps provider registration, selection, cache, permissions, and
//! runtime diagnostics consistent across lookup, uploads, spotting, maps,
//! weather, propagation, and future integrations.

use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use ham_plugin_sdk::{PluginCapability, PluginManifest, ServiceType};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{
    check_plugin_permission, BusEvent, EventBus, EventBusError, OperatorRole, PermissionGrantSet,
    RuntimeEventEnvelope, RuntimeEventSeverity,
};

pub const CAP_LOOKUP_CALLSIGN_BASIC: &str = "lookup.callsign.basic";
pub const CAP_LOOKUP_CALLSIGN_FULL: &str = "lookup.callsign.full";
pub const CAP_LOOKUP_ENTITY_PREFIX: &str = "lookup.entity.prefix";
pub const CAP_LOOKUP_GRID_VALIDATE: &str = "lookup.grid.validate";
pub const CAP_UPLOAD_ADIF: &str = "upload.adif";
pub const CAP_UPLOAD_INCREMENTAL: &str = "upload.incremental";
pub const CAP_UPLOAD_CONFIRMATION_PULL: &str = "upload.confirmation_pull";
pub const CAP_SPOTTING_DX_CLUSTER: &str = "spotting.dx_cluster";
pub const CAP_SPOTTING_POTA: &str = "spotting.pota";
pub const CAP_SPOTTING_SOTA: &str = "spotting.sota";
pub const CAP_SPOTTING_RBN: &str = "spotting.rbn";
pub const CAP_MAP_TILES_ONLINE: &str = "map.tiles.online";
pub const CAP_MAP_TILES_OFFLINE: &str = "map.tiles.offline";
pub const CAP_WEATHER_CURRENT: &str = "weather.current";
pub const CAP_WEATHER_FORECAST: &str = "weather.forecast";
pub const CAP_PROPAGATION_SOLAR_INDICES: &str = "propagation.solar_indices";
pub const CAP_PROPAGATION_GRAYLINE: &str = "propagation.grayline";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderHealthState {
    Healthy,
    Degraded,
    Unavailable,
    MissingConfig,
}

impl ProviderHealthState {
    fn is_usable(self) -> bool {
        matches!(self, Self::Healthy | Self::Degraded)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHealth {
    pub provider_id: String,
    pub state: ProviderHealthState,
    pub message: String,
    pub checked_at: DateTime<Utc>,
    pub rate_limited: bool,
}

impl ProviderHealth {
    pub fn healthy(provider_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            state: ProviderHealthState::Healthy,
            message: message.into(),
            checked_at: Utc::now(),
            rate_limited: false,
        }
    }

    pub fn missing_config(provider_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            state: ProviderHealthState::MissingConfig,
            message: message.into(),
            checked_at: Utc::now(),
            rate_limited: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceProviderMetadata {
    pub provider_id: String,
    pub service_type: ServiceType,
    pub display_name: String,
    pub version: String,
    pub source_plugin_id: String,
    pub capabilities: Vec<String>,
    pub required_permissions: Vec<PluginCapability>,
    pub required_config_keys: Vec<String>,
    pub optional_config_keys: Vec<String>,
    pub priority: i32,
    pub supports_offline: bool,
    pub requires_network_access: bool,
}

impl ServiceProviderMetadata {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        provider_id: impl Into<String>,
        service_type: ServiceType,
        display_name: impl Into<String>,
        version: impl Into<String>,
        source_plugin_id: impl Into<String>,
        capabilities: Vec<String>,
        required_permissions: Vec<PluginCapability>,
        priority: i32,
        supports_offline: bool,
        requires_network_access: bool,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            service_type,
            display_name: display_name.into(),
            version: version.into(),
            source_plugin_id: source_plugin_id.into(),
            capabilities,
            required_permissions,
            required_config_keys: Vec::new(),
            optional_config_keys: Vec::new(),
            priority,
            supports_offline,
            requires_network_access,
        }
    }

    pub fn requires_capability(&self, capability: Option<&str>) -> bool {
        capability.is_none_or(|capability| self.capabilities.iter().any(|held| held == capability))
    }
}

#[async_trait]
pub trait ServiceProvider: Send + Sync {
    fn metadata(&self) -> ServiceProviderMetadata;
    async fn health(&self) -> ProviderHealth;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredServiceProvider {
    pub metadata: ServiceProviderMetadata,
    pub enabled: bool,
    pub health: ProviderHealth,
    pub last_success_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

impl RegisteredServiceProvider {
    pub fn new(metadata: ServiceProviderMetadata) -> Self {
        let health = ProviderHealth::healthy(metadata.provider_id.clone(), "Provider registered");
        Self {
            metadata,
            enabled: true,
            health,
            last_success_at: None,
            last_error: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderSelectionCriteria {
    pub service_type: ServiceType,
    pub required_capability: Option<String>,
    pub allow_network: bool,
    pub require_offline: bool,
}

impl ProviderSelectionCriteria {
    pub fn new(service_type: ServiceType) -> Self {
        Self {
            service_type,
            required_capability: None,
            allow_network: true,
            require_offline: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceRegistry {
    providers: HashMap<String, RegisteredServiceProvider>,
    preferred_providers: HashMap<ServiceType, String>,
}

impl Default for ServiceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ServiceRegistry {
    pub fn new() -> Self {
        Self {
            providers: HashMap::new(),
            preferred_providers: HashMap::new(),
        }
    }

    pub fn register_provider(
        &mut self,
        metadata: ServiceProviderMetadata,
    ) -> Result<(), ServiceError> {
        if self.providers.contains_key(&metadata.provider_id) {
            return Err(ServiceError::DuplicateProviderId(metadata.provider_id));
        }
        self.providers.insert(
            metadata.provider_id.clone(),
            RegisteredServiceProvider::new(metadata),
        );
        Ok(())
    }

    pub fn set_enabled(&mut self, provider_id: &str, enabled: bool) -> Result<(), ServiceError> {
        let provider = self
            .providers
            .get_mut(provider_id)
            .ok_or_else(|| ServiceError::ProviderNotFound(provider_id.to_owned()))?;
        provider.enabled = enabled;
        Ok(())
    }

    pub fn set_priority(&mut self, provider_id: &str, priority: i32) -> Result<(), ServiceError> {
        let provider = self
            .providers
            .get_mut(provider_id)
            .ok_or_else(|| ServiceError::ProviderNotFound(provider_id.to_owned()))?;
        provider.metadata.priority = priority;
        Ok(())
    }

    pub fn set_health(
        &mut self,
        provider_id: &str,
        health: ProviderHealth,
    ) -> Result<(), ServiceError> {
        let provider = self
            .providers
            .get_mut(provider_id)
            .ok_or_else(|| ServiceError::ProviderNotFound(provider_id.to_owned()))?;
        provider.health = health;
        Ok(())
    }

    pub fn record_success(&mut self, provider_id: &str) -> Result<(), ServiceError> {
        let provider = self
            .providers
            .get_mut(provider_id)
            .ok_or_else(|| ServiceError::ProviderNotFound(provider_id.to_owned()))?;
        provider.last_success_at = Some(Utc::now());
        provider.last_error = None;
        Ok(())
    }

    pub fn record_error(
        &mut self,
        provider_id: &str,
        error: impl Into<String>,
    ) -> Result<(), ServiceError> {
        let provider = self
            .providers
            .get_mut(provider_id)
            .ok_or_else(|| ServiceError::ProviderNotFound(provider_id.to_owned()))?;
        provider.last_error = Some(error.into());
        Ok(())
    }

    pub fn set_preferred_provider(
        &mut self,
        service_type: ServiceType,
        provider_id: impl Into<String>,
    ) -> Result<(), ServiceError> {
        let provider_id = provider_id.into();
        let provider = self
            .providers
            .get(&provider_id)
            .ok_or_else(|| ServiceError::ProviderNotFound(provider_id.clone()))?;
        if provider.metadata.service_type != service_type {
            return Err(ServiceError::WrongServiceType(provider_id));
        }
        self.preferred_providers.insert(service_type, provider_id);
        Ok(())
    }

    pub fn provider(&self, provider_id: &str) -> Option<&RegisteredServiceProvider> {
        self.providers.get(provider_id)
    }

    pub fn providers_for(&self, service_type: ServiceType) -> Vec<&RegisteredServiceProvider> {
        let mut providers = self
            .providers
            .values()
            .filter(|provider| provider.metadata.service_type == service_type)
            .collect::<Vec<_>>();
        providers.sort_by(|a, b| {
            b.metadata
                .priority
                .cmp(&a.metadata.priority)
                .then_with(|| a.metadata.display_name.cmp(&b.metadata.display_name))
        });
        providers
    }

    pub fn select_provider(
        &self,
        criteria: &ProviderSelectionCriteria,
    ) -> Result<RegisteredServiceProvider, ServiceError> {
        let mut candidates = self
            .providers_for(criteria.service_type)
            .into_iter()
            .filter(|provider| {
                provider.enabled
                    && provider.health.state.is_usable()
                    && provider
                        .metadata
                        .requires_capability(criteria.required_capability.as_deref())
                    && (criteria.allow_network || !provider.metadata.requires_network_access)
                    && (!criteria.require_offline || provider.metadata.supports_offline)
            })
            .cloned()
            .collect::<Vec<_>>();

        if candidates.is_empty() {
            return Err(ServiceError::NoUsableProvider(criteria.service_type));
        }

        if let Some(preferred_id) = self.preferred_providers.get(&criteria.service_type) {
            if let Some(index) = candidates
                .iter()
                .position(|provider| &provider.metadata.provider_id == preferred_id)
            {
                return Ok(candidates.remove(index));
            }
        }

        candidates.sort_by(|a, b| {
            b.metadata
                .priority
                .cmp(&a.metadata.priority)
                .then_with(|| a.metadata.display_name.cmp(&b.metadata.display_name))
        });
        Ok(candidates.remove(0))
    }

    pub fn snapshot(&self) -> ServiceRegistrySnapshot {
        ServiceRegistrySnapshot {
            providers: self.providers.values().cloned().collect(),
            preferred_providers: self.preferred_providers.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceRegistrySnapshot {
    pub providers: Vec<RegisteredServiceProvider>,
    pub preferred_providers: HashMap<ServiceType, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ServiceCacheEntry {
    pub service_type: ServiceType,
    pub provider_id: String,
    pub cache_key: String,
    pub fetched_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub confidence: Option<f32>,
    pub value: Value,
    pub safe_metadata: Option<Value>,
}

impl ServiceCacheEntry {
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|expires_at| expires_at <= now)
    }
}

#[derive(Debug, Default)]
pub struct ServiceCache {
    entries: RwLock<HashMap<String, ServiceCacheEntry>>,
}

impl ServiceCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get(
        &self,
        service_type: ServiceType,
        provider_id: &str,
        cache_key: &str,
        now: DateTime<Utc>,
    ) -> Option<ServiceCacheEntry> {
        let key = service_cache_key(service_type, provider_id, cache_key);
        let entry = self.entries.read().await.get(&key).cloned()?;
        if entry.is_expired(now) {
            return None;
        }
        Some(entry)
    }

    pub async fn put(&self, entry: ServiceCacheEntry) {
        let key = service_cache_key(entry.service_type, &entry.provider_id, &entry.cache_key);
        self.entries.write().await.insert(key, entry);
    }

    pub async fn clear_service(&self, service_type: ServiceType) -> usize {
        let mut entries = self.entries.write().await;
        let before = entries.len();
        entries.retain(|_, entry| entry.service_type != service_type);
        before - entries.len()
    }

    pub async fn clear_all(&self) -> usize {
        let mut entries = self.entries.write().await;
        let count = entries.len();
        entries.clear();
        count
    }

    pub async fn count(&self) -> usize {
        self.entries.read().await.len()
    }
}

fn service_cache_key(service_type: ServiceType, provider_id: &str, cache_key: &str) -> String {
    format!("{}:{provider_id}:{cache_key}", service_type.as_str())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UploadJobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    NeedsCredentials,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallsignLookupRequest {
    pub callsign: String,
    pub requested_fields: Vec<String>,
    pub allow_network: bool,
    pub preferred_provider_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallsignLookupResponse {
    pub provider_id: String,
    pub normalized_callsign: String,
    pub confidence: f32,
    pub suggested_fields: Value,
    pub raw_safe_metadata: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogUploadRequest {
    pub job_id: Uuid,
    pub logbook_id: Uuid,
    pub provider_id: Option<String>,
    pub adif_payload: String,
    pub incremental: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogUploadResponse {
    pub job_id: Uuid,
    pub provider_id: String,
    pub status: UploadJobStatus,
    pub accepted_count: usize,
    pub rejected_count: usize,
    pub confirmation_reference: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadJob {
    pub job_id: Uuid,
    pub provider_id: String,
    pub logbook_id: Uuid,
    pub status: UploadJobStatus,
    pub created_at: DateTime<Utc>,
    pub last_attempt_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
}

#[async_trait]
pub trait LogUploadProvider: ServiceProvider {
    async fn upload_adif(
        &self,
        request: LogUploadRequest,
    ) -> Result<LogUploadResponse, ServiceError>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotSource {
    pub provider_id: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spot {
    pub spotted_callsign: String,
    pub spotter_callsign: Option<String>,
    pub frequency_hz: u64,
    pub band: Option<String>,
    pub mode: Option<String>,
    pub comment: Option<String>,
    pub source: SpotSource,
    pub spotted_at: DateTime<Utc>,
    pub entity: Option<String>,
    pub grid: Option<String>,
    pub reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotFilter {
    pub bands: Vec<String>,
    pub modes: Vec<String>,
    pub sources: Vec<String>,
    pub callsign_contains: Option<String>,
    pub reference_contains: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotAlertRule {
    pub rule_id: Uuid,
    pub name: String,
    pub filter: SpotFilter,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpotQueryRequest {
    pub filter: Option<SpotFilter>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpotQueryResponse {
    pub provider_id: String,
    pub spots: Vec<Spot>,
    pub fetched_at: DateTime<Utc>,
}

#[async_trait]
pub trait SpottingProvider: ServiceProvider {
    async fn query_spots(
        &self,
        request: SpotQueryRequest,
    ) -> Result<SpotQueryResponse, ServiceError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapTileRequest {
    pub zoom: u8,
    pub x: u32,
    pub y: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapTileResponse {
    pub provider_id: String,
    pub mime_type: String,
    pub tile_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherRequest {
    pub latitude: f64,
    pub longitude: f64,
    pub forecast_hours: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherResponse {
    pub provider_id: String,
    pub fetched_at: DateTime<Utc>,
    pub current_summary: String,
    pub forecast_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropagationRequest {
    pub band: Option<String>,
    pub mode: Option<String>,
    pub grid: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PropagationResponse {
    pub provider_id: String,
    pub fetched_at: DateTime<Utc>,
    pub solar_flux_index: Option<f32>,
    pub k_index: Option<f32>,
    pub a_index: Option<f32>,
    pub summary: String,
}

#[derive(Debug, Error)]
pub enum ServiceError {
    #[error("duplicate provider id {0}")]
    DuplicateProviderId(String),
    #[error("provider {0} not found")]
    ProviderNotFound(String),
    #[error("provider {0} is registered for a different service type")]
    WrongServiceType(String),
    #[error("no usable provider for {0:?}")]
    NoUsableProvider(ServiceType),
    #[error("service permission denied: {0}")]
    PermissionDenied(String),
    #[error("operator role {role:?} cannot use service permission {permission}")]
    OperatorPermissionDenied {
        role: OperatorRole,
        permission: String,
    },
    #[error("missing provider configuration: {0}")]
    MissingConfig(String),
    #[error("provider error: {0}")]
    Provider(String),
    #[error("event bus error: {0}")]
    EventBus(#[from] EventBusError),
}

pub fn authorize_service_request(
    manifest: &PluginManifest,
    grants: &PermissionGrantSet,
    operator_role: OperatorRole,
    provider: &ServiceProviderMetadata,
    required_permissions: &[PluginCapability],
) -> Result<(), ServiceError> {
    for permission in required_permissions
        .iter()
        .chain(provider.required_permissions.iter())
    {
        check_plugin_permission(manifest, grants, permission)
            .map_err(|error| ServiceError::PermissionDenied(error.to_string()))?;
        if !operator_role_allows_service_permission(operator_role, permission) {
            return Err(ServiceError::OperatorPermissionDenied {
                role: operator_role,
                permission: permission.as_str().to_owned(),
            });
        }
    }
    Ok(())
}

pub fn operator_role_allows_service_permission(
    role: OperatorRole,
    permission: &PluginCapability,
) -> bool {
    match role {
        OperatorRole::ReadOnly => matches!(
            permission,
            PluginCapability::ServiceCacheRead
                | PluginCapability::LookupCallsign
                | PluginCapability::LookupEntity
                | PluginCapability::LookupGrid
                | PluginCapability::SpottingView
                | PluginCapability::MapView
                | PluginCapability::WeatherView
                | PluginCapability::PropagationView
        ),
        OperatorRole::Logger => matches!(
            permission,
            PluginCapability::LookupCallsign
                | PluginCapability::LookupEntity
                | PluginCapability::LookupGrid
                | PluginCapability::LookupCacheRead
                | PluginCapability::LookupCacheWrite
                | PluginCapability::ServiceCacheRead
                | PluginCapability::ServiceCacheWrite
                | PluginCapability::QsoSuggestFields
                | PluginCapability::SpottingView
                | PluginCapability::MapView
                | PluginCapability::WeatherView
                | PluginCapability::PropagationView
        ),
        OperatorRole::Admin => true,
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn publish_service_runtime_event<B: EventBus>(
    bus: &B,
    event_type: &str,
    severity: RuntimeEventSeverity,
    source_plugin_id: Option<String>,
    device_id: Uuid,
    summary: impl Into<String>,
    payload: Option<Value>,
    error: Option<String>,
) -> Result<(), EventBusError> {
    bus.publish(BusEvent::Runtime(RuntimeEventEnvelope::new(
        event_type,
        severity,
        "ham-core.services",
        source_plugin_id,
        Uuid::new_v4(),
        Uuid::new_v4(),
        device_id,
        None,
        summary,
        payload,
        error,
    )))
    .await?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct StubLogUploadProvider {
    metadata: ServiceProviderMetadata,
    health: ProviderHealth,
}

impl StubLogUploadProvider {
    pub fn lotw() -> Self {
        upload_stub("lotw-stub", "LoTW Upload Stub", "lotw.certificate_path")
    }

    pub fn eqsl() -> Self {
        upload_stub("eqsl-stub", "eQSL Upload Stub", "eqsl.username")
    }

    pub fn club_log() -> Self {
        upload_stub("clublog-stub", "Club Log Upload Stub", "clublog.email")
    }

    pub fn qrz_logbook() -> Self {
        upload_stub(
            "qrz-logbook-stub",
            "QRZ Logbook Upload Stub",
            "qrz.username",
        )
    }
}

fn upload_stub(
    provider_id: &str,
    display_name: &str,
    required_config_key: &str,
) -> StubLogUploadProvider {
    let mut metadata = ServiceProviderMetadata::new(
        provider_id,
        ServiceType::LogUpload,
        display_name,
        "0.1.0",
        "plugin.log-upload",
        vec![CAP_UPLOAD_ADIF.to_owned()],
        vec![PluginCapability::AdifExport, PluginCapability::UploadLog],
        10,
        false,
        true,
    );
    metadata.required_config_keys = vec![required_config_key.to_owned()];
    StubLogUploadProvider {
        health: ProviderHealth::missing_config(provider_id, "Credentials are not configured"),
        metadata,
    }
}

#[async_trait]
impl ServiceProvider for StubLogUploadProvider {
    fn metadata(&self) -> ServiceProviderMetadata {
        self.metadata.clone()
    }

    async fn health(&self) -> ProviderHealth {
        self.health.clone()
    }
}

#[async_trait]
impl LogUploadProvider for StubLogUploadProvider {
    async fn upload_adif(
        &self,
        request: LogUploadRequest,
    ) -> Result<LogUploadResponse, ServiceError> {
        Ok(LogUploadResponse {
            job_id: request.job_id,
            provider_id: self.metadata.provider_id.clone(),
            status: UploadJobStatus::NeedsCredentials,
            accepted_count: 0,
            rejected_count: 0,
            confirmation_reference: None,
            message: "Upload provider stub requires credential storage integration".to_owned(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct MockSpottingProvider {
    provider_id: String,
    spots: Vec<Spot>,
}

impl Default for MockSpottingProvider {
    fn default() -> Self {
        Self {
            provider_id: "mock-spots".to_owned(),
            spots: vec![Spot {
                spotted_callsign: "K1ABC".to_owned(),
                spotter_callsign: Some("KE8YGW".to_owned()),
                frequency_hz: 14_074_000,
                band: Some("20m".to_owned()),
                mode: Some("FT8".to_owned()),
                comment: Some("Mock POTA spot".to_owned()),
                source: SpotSource {
                    provider_id: "mock-spots".to_owned(),
                    label: "Mock Spots".to_owned(),
                },
                spotted_at: Utc::now(),
                entity: Some("United States".to_owned()),
                grid: Some("EN91".to_owned()),
                reference: Some("US-0001".to_owned()),
            }],
        }
    }
}

#[async_trait]
impl ServiceProvider for MockSpottingProvider {
    fn metadata(&self) -> ServiceProviderMetadata {
        ServiceProviderMetadata::new(
            self.provider_id.clone(),
            ServiceType::Spotting,
            "Mock Spotting Provider",
            "0.1.0",
            "plugin.spotting",
            vec![
                CAP_SPOTTING_DX_CLUSTER.to_owned(),
                CAP_SPOTTING_POTA.to_owned(),
                CAP_SPOTTING_SOTA.to_owned(),
            ],
            vec![PluginCapability::SpottingView],
            100,
            true,
            false,
        )
    }

    async fn health(&self) -> ProviderHealth {
        ProviderHealth::healthy(&self.provider_id, "Mock spots ready")
    }
}

#[async_trait]
impl SpottingProvider for MockSpottingProvider {
    async fn query_spots(
        &self,
        request: SpotQueryRequest,
    ) -> Result<SpotQueryResponse, ServiceError> {
        let mut spots = self.spots.clone();
        if let Some(filter) = request.filter {
            spots.retain(|spot| spot_matches_filter(spot, &filter));
        }
        spots.truncate(request.limit);
        Ok(SpotQueryResponse {
            provider_id: self.provider_id.clone(),
            spots,
            fetched_at: Utc::now(),
        })
    }
}

fn spot_matches_filter(spot: &Spot, filter: &SpotFilter) -> bool {
    (filter.bands.is_empty()
        || spot
            .band
            .as_ref()
            .is_some_and(|band| filter.bands.contains(band)))
        && (filter.modes.is_empty()
            || spot
                .mode
                .as_ref()
                .is_some_and(|mode| filter.modes.contains(mode)))
        && (filter.sources.is_empty() || filter.sources.contains(&spot.source.provider_id))
        && filter.callsign_contains.as_ref().is_none_or(|needle| {
            spot.spotted_callsign
                .to_ascii_uppercase()
                .contains(&needle.to_ascii_uppercase())
        })
        && filter.reference_contains.as_ref().is_none_or(|needle| {
            spot.reference
                .as_ref()
                .is_some_and(|reference| reference.contains(needle))
        })
}

pub fn default_service_registry() -> ServiceRegistry {
    let mut registry = ServiceRegistry::new();
    for metadata in [
        local_prefix_provider_metadata(),
        mock_lookup_provider_metadata(),
        qrz_lookup_provider_metadata(),
        StubLogUploadProvider::lotw().metadata(),
        StubLogUploadProvider::eqsl().metadata(),
        StubLogUploadProvider::club_log().metadata(),
        StubLogUploadProvider::qrz_logbook().metadata(),
        MockSpottingProvider::default().metadata(),
        map_provider_metadata("local-map-stub", "Local Map Stub", true, false),
        map_provider_metadata("osm-stub", "OpenStreetMap Stub", false, true),
        weather_provider_metadata("manual-weather-stub", "Manual Weather Stub", true, false),
        propagation_provider_metadata("mock-propagation", "Mock Propagation Provider"),
    ] {
        let _ = registry.register_provider(metadata);
    }
    registry
}

pub fn local_prefix_provider_metadata() -> ServiceProviderMetadata {
    ServiceProviderMetadata::new(
        "local-prefix",
        ServiceType::CallsignLookup,
        "Local Prefix Resolver",
        "0.1.0",
        "plugin.callsign-lookup",
        vec![
            CAP_LOOKUP_CALLSIGN_BASIC.to_owned(),
            CAP_LOOKUP_ENTITY_PREFIX.to_owned(),
            CAP_LOOKUP_GRID_VALIDATE.to_owned(),
        ],
        vec![
            PluginCapability::LookupCallsign,
            PluginCapability::LookupEntity,
            PluginCapability::LookupGrid,
            PluginCapability::ServiceCacheRead,
            PluginCapability::ServiceCacheWrite,
        ],
        100,
        true,
        false,
    )
}

pub fn mock_lookup_provider_metadata() -> ServiceProviderMetadata {
    ServiceProviderMetadata::new(
        "mock",
        ServiceType::CallsignLookup,
        "Mock Lookup Provider",
        "0.1.0",
        "plugin.callsign-lookup",
        vec![CAP_LOOKUP_CALLSIGN_FULL.to_owned()],
        vec![PluginCapability::LookupCallsign],
        50,
        true,
        false,
    )
}

pub fn qrz_lookup_provider_metadata() -> ServiceProviderMetadata {
    let mut metadata = ServiceProviderMetadata::new(
        "qrz-stub",
        ServiceType::CallsignLookup,
        "QRZ/HamQTH Lookup Stub",
        "0.1.0",
        "plugin.callsign-lookup",
        vec![CAP_LOOKUP_CALLSIGN_FULL.to_owned()],
        vec![
            PluginCapability::LookupCallsign,
            PluginCapability::NetworkExternalLookup,
        ],
        75,
        false,
        true,
    );
    metadata.required_config_keys = vec!["qrz.username".to_owned(), "qrz.token".to_owned()];
    metadata
}

fn map_provider_metadata(
    provider_id: &str,
    display_name: &str,
    offline: bool,
    network: bool,
) -> ServiceProviderMetadata {
    ServiceProviderMetadata::new(
        provider_id,
        ServiceType::MapTiles,
        display_name,
        "0.1.0",
        "plugin.maps",
        vec![if offline {
            CAP_MAP_TILES_OFFLINE.to_owned()
        } else {
            CAP_MAP_TILES_ONLINE.to_owned()
        }],
        vec![PluginCapability::MapView],
        if offline { 50 } else { 40 },
        offline,
        network,
    )
}

fn weather_provider_metadata(
    provider_id: &str,
    display_name: &str,
    offline: bool,
    network: bool,
) -> ServiceProviderMetadata {
    ServiceProviderMetadata::new(
        provider_id,
        ServiceType::Weather,
        display_name,
        "0.1.0",
        "plugin.weather",
        vec![
            CAP_WEATHER_CURRENT.to_owned(),
            CAP_WEATHER_FORECAST.to_owned(),
        ],
        vec![PluginCapability::WeatherView],
        50,
        offline,
        network,
    )
}

fn propagation_provider_metadata(provider_id: &str, display_name: &str) -> ServiceProviderMetadata {
    ServiceProviderMetadata::new(
        provider_id,
        ServiceType::Propagation,
        display_name,
        "0.1.0",
        "plugin.propagation",
        vec![
            CAP_PROPAGATION_SOLAR_INDICES.to_owned(),
            CAP_PROPAGATION_GRAYLINE.to_owned(),
        ],
        vec![PluginCapability::PropagationView],
        50,
        true,
        false,
    )
}

pub fn cache_entry_for_value(
    service_type: ServiceType,
    provider_id: impl Into<String>,
    cache_key: impl Into<String>,
    ttl: Duration,
    confidence: Option<f32>,
    value: Value,
) -> ServiceCacheEntry {
    let fetched_at = Utc::now();
    ServiceCacheEntry {
        service_type,
        provider_id: provider_id.into(),
        cache_key: cache_key.into(),
        fetched_at,
        expires_at: Some(fetched_at + ttl),
        confidence,
        value,
        safe_metadata: Some(json!({"cache": "service"})),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemoryEventBus, RuntimeEventFilter};

    fn lookup_manifest_with(permissions: Vec<PluginCapability>) -> PluginManifest {
        PluginManifest::new("plugin.callsign-lookup", "Lookup", "0.1.0", permissions)
    }

    #[test]
    fn service_registry_registers_and_rejects_duplicates() {
        let mut registry = ServiceRegistry::new();
        let metadata = local_prefix_provider_metadata();
        registry.register_provider(metadata.clone()).unwrap();
        assert!(registry.register_provider(metadata).is_err());
    }

    #[test]
    fn provider_enable_disable_and_priority_ordering_work() {
        let mut registry = default_service_registry();
        registry.set_enabled("local-prefix", false).unwrap();
        let selected = registry
            .select_provider(&ProviderSelectionCriteria::new(ServiceType::CallsignLookup))
            .unwrap();
        assert_ne!(selected.metadata.provider_id, "local-prefix");
        registry.set_enabled("local-prefix", true).unwrap();
        registry.set_priority("mock", 200).unwrap();
        let selected = registry
            .select_provider(&ProviderSelectionCriteria::new(ServiceType::CallsignLookup))
            .unwrap();
        assert_eq!(selected.metadata.provider_id, "mock");
    }

    #[test]
    fn preferred_provider_and_fallback_selection_work() {
        let mut registry = default_service_registry();
        registry
            .set_preferred_provider(ServiceType::CallsignLookup, "mock")
            .unwrap();
        let selected = registry
            .select_provider(&ProviderSelectionCriteria::new(ServiceType::CallsignLookup))
            .unwrap();
        assert_eq!(selected.metadata.provider_id, "mock");
        registry.set_enabled("mock", false).unwrap();
        let fallback = registry
            .select_provider(&ProviderSelectionCriteria::new(ServiceType::CallsignLookup))
            .unwrap();
        assert_eq!(fallback.metadata.provider_id, "local-prefix");
    }

    #[test]
    fn external_provider_requires_network_permission() {
        let manifest = lookup_manifest_with(vec![PluginCapability::LookupCallsign]);
        let grants = PermissionGrantSet::grants_for_manifest(&manifest);
        let provider = qrz_lookup_provider_metadata();
        let error = authorize_service_request(
            &manifest,
            &grants,
            OperatorRole::Admin,
            &provider,
            &[PluginCapability::LookupCallsign],
        )
        .unwrap_err();
        assert!(matches!(error, ServiceError::PermissionDenied(_)));
    }

    #[test]
    fn service_request_denied_when_operator_role_lacks_permission() {
        let manifest = lookup_manifest_with(vec![
            PluginCapability::AdifExport,
            PluginCapability::UploadLog,
        ]);
        let grants = PermissionGrantSet::grants_for_manifest(&manifest);
        let provider = StubLogUploadProvider::lotw().metadata();
        let error = authorize_service_request(
            &manifest,
            &grants,
            OperatorRole::Logger,
            &provider,
            &[PluginCapability::UploadLog],
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ServiceError::OperatorPermissionDenied { .. }
        ));
    }

    #[test]
    fn service_request_allowed_when_plugin_and_role_allow() {
        let manifest = lookup_manifest_with(vec![
            PluginCapability::LookupCallsign,
            PluginCapability::LookupEntity,
            PluginCapability::LookupGrid,
            PluginCapability::ServiceCacheRead,
            PluginCapability::ServiceCacheWrite,
        ]);
        let grants = PermissionGrantSet::grants_for_manifest(&manifest);
        authorize_service_request(
            &manifest,
            &grants,
            OperatorRole::Logger,
            &local_prefix_provider_metadata(),
            &[PluginCapability::LookupCallsign],
        )
        .unwrap();
    }

    #[test]
    fn cache_clear_requires_cache_clear_permission() {
        let manifest = lookup_manifest_with(vec![PluginCapability::ServiceCacheRead]);
        let grants = PermissionGrantSet::grants_for_manifest(&manifest);
        let provider = local_prefix_provider_metadata();
        let error = authorize_service_request(
            &manifest,
            &grants,
            OperatorRole::Admin,
            &provider,
            &[PluginCapability::ServiceCacheClear],
        )
        .unwrap_err();
        assert!(matches!(error, ServiceError::PermissionDenied(_)));
    }

    #[tokio::test]
    async fn service_cache_hit_miss_expiration_and_clear_work() {
        let cache = ServiceCache::new();
        assert!(cache
            .get(ServiceType::CallsignLookup, "mock", "K1ABC", Utc::now())
            .await
            .is_none());
        cache
            .put(cache_entry_for_value(
                ServiceType::CallsignLookup,
                "mock",
                "K1ABC",
                Duration::days(1),
                Some(0.9),
                json!({"callsign": "K1ABC"}),
            ))
            .await;
        assert!(cache
            .get(ServiceType::CallsignLookup, "mock", "K1ABC", Utc::now())
            .await
            .is_some());
        assert!(cache
            .get(
                ServiceType::CallsignLookup,
                "local-prefix",
                "K1ABC",
                Utc::now()
            )
            .await
            .is_none());
        cache
            .put(ServiceCacheEntry {
                service_type: ServiceType::Weather,
                provider_id: "manual".to_owned(),
                cache_key: "EN91".to_owned(),
                fetched_at: Utc::now() - Duration::days(2),
                expires_at: Some(Utc::now() - Duration::days(1)),
                confidence: None,
                value: json!({}),
                safe_metadata: None,
            })
            .await;
        assert!(cache
            .get(ServiceType::Weather, "manual", "EN91", Utc::now())
            .await
            .is_none());
        assert_eq!(cache.clear_service(ServiceType::CallsignLookup).await, 1);
        assert_eq!(cache.clear_all().await, 1);
    }

    #[tokio::test]
    async fn service_runtime_events_are_published() {
        let bus = InMemoryEventBus::default();
        publish_service_runtime_event(
            &bus,
            "service.request.started",
            RuntimeEventSeverity::Info,
            Some("plugin.test".to_owned()),
            Uuid::new_v4(),
            "Service request started",
            Some(json!({"service_type": "callsign_lookup"})),
            None,
        )
        .await
        .unwrap();
        let events = bus
            .replay_runtime_events(RuntimeEventFilter::default(), 10)
            .await;
        assert_eq!(events[0].event_type, "service.request.started");
    }

    #[tokio::test]
    async fn mock_spotting_provider_returns_test_spots() {
        let provider = MockSpottingProvider::default();
        let response = provider
            .query_spots(SpotQueryRequest {
                filter: Some(SpotFilter {
                    bands: vec!["20m".to_owned()],
                    modes: Vec::new(),
                    sources: Vec::new(),
                    callsign_contains: Some("K1".to_owned()),
                    reference_contains: Some("US-".to_owned()),
                }),
                limit: 10,
            })
            .await
            .unwrap();
        assert_eq!(response.spots.len(), 1);
    }

    #[test]
    fn upload_and_spot_models_serialize() {
        let upload = LogUploadRequest {
            job_id: Uuid::new_v4(),
            logbook_id: Uuid::new_v4(),
            provider_id: Some("lotw-stub".to_owned()),
            adif_payload: "<EOH>".to_owned(),
            incremental: true,
        };
        serde_json::to_string(&upload).unwrap();
        let spot = MockSpottingProvider::default().spots[0].clone();
        serde_json::to_string(&spot).unwrap();
    }
}
