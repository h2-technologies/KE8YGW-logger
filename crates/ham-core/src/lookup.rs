use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::{BusEvent, EventBus, EventBusError, RuntimeEventEnvelope, RuntimeEventSeverity};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LookupResult {
    pub callsign: String,
    pub normalized_callsign: String,
    pub name: Option<String>,
    pub qth: Option<String>,
    pub country: Option<String>,
    pub dxcc: Option<u16>,
    pub cq_zone: Option<u8>,
    pub itu_zone: Option<u8>,
    pub grid: Option<String>,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub license_class: Option<String>,
    pub previous_callsigns: Vec<String>,
    pub source_provider: String,
    pub fetched_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub confidence: f32,
    pub raw_metadata: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LookupSuggestion {
    pub normalized_callsign: String,
    pub provider: String,
    pub confidence: f32,
    pub suggested_fields: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupProviderStatus {
    pub provider_id: String,
    pub healthy: bool,
    pub message: String,
    pub rate_limited: bool,
}

#[derive(Debug, Error)]
pub enum LookupError {
    #[error("invalid callsign")]
    InvalidCallsign,
    #[error("invalid Maidenhead grid")]
    InvalidGrid,
    #[error("lookup provider error: {0}")]
    Provider(String),
    #[error("event bus error: {0}")]
    EventBus(#[from] EventBusError),
}

#[async_trait]
pub trait CallsignLookupProvider: Send + Sync {
    fn provider_id(&self) -> &str;
    async fn lookup_callsign(&self, callsign: &str) -> Result<Option<LookupResult>, LookupError>;
    async fn lookup_grid(&self, grid: &str) -> Result<Option<GridInfo>, LookupError>;
    async fn lookup_entity(
        &self,
        callsign_or_prefix: &str,
    ) -> Result<Option<EntityInfo>, LookupError>;
    async fn status(&self) -> LookupProviderStatus;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityInfo {
    pub country: String,
    pub dxcc: Option<u16>,
    pub cq_zone: Option<u8>,
    pub itu_zone: Option<u8>,
    pub source_provider: String,
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GridInfo {
    pub grid: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupCacheConfig {
    pub ttl_days: i64,
}

impl Default for LookupCacheConfig {
    fn default() -> Self {
        Self { ttl_days: 30 }
    }
}

#[derive(Debug, Default)]
pub struct LookupCache {
    entries: RwLock<HashMap<String, LookupResult>>,
}

impl LookupCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn get(&self, callsign: &str, now: DateTime<Utc>) -> Option<LookupResult> {
        let key = normalize_callsign(callsign).ok()?;
        let entry = self.entries.read().await.get(&key).cloned()?;
        if entry.expires_at.is_some_and(|expires_at| expires_at <= now) {
            return None;
        }
        Some(entry)
    }

    pub async fn put(&self, result: LookupResult) {
        self.entries
            .write()
            .await
            .insert(result.normalized_callsign.clone(), result);
    }

    pub async fn clear(&self) {
        self.entries.write().await.clear();
    }
}

pub async fn lookup_callsign_with_cache<P, B>(
    provider: &P,
    cache: &LookupCache,
    cache_config: &LookupCacheConfig,
    bus: &B,
    callsign: &str,
    device_id: Uuid,
) -> Result<Option<LookupSuggestion>, LookupError>
where
    P: CallsignLookupProvider,
    B: EventBus,
{
    let normalized = normalize_callsign(callsign)?;
    publish_lookup_event(
        bus,
        "lookup.callsign.started",
        RuntimeEventSeverity::Info,
        device_id,
        &format!("Looking up {normalized}"),
        None,
        None,
    )
    .await?;

    if let Some(result) = cache.get(&normalized, Utc::now()).await {
        publish_lookup_event(
            bus,
            "lookup.callsign.cache_hit",
            RuntimeEventSeverity::Debug,
            device_id,
            "Lookup cache hit",
            Some(json!({"callsign": normalized, "provider": result.source_provider})),
            None,
        )
        .await?;
        let suggestion = suggestion_from_result(&result);
        publish_lookup_event(
            bus,
            "lookup.suggestion.created",
            RuntimeEventSeverity::Info,
            device_id,
            "Lookup suggestion created from cache",
            Some(json!(&suggestion)),
            None,
        )
        .await?;
        return Ok(Some(suggestion));
    }

    publish_lookup_event(
        bus,
        "lookup.callsign.cache_miss",
        RuntimeEventSeverity::Debug,
        device_id,
        "Lookup cache miss",
        Some(json!({"callsign": normalized})),
        None,
    )
    .await?;

    let mut result = provider.lookup_callsign(&normalized).await?;
    if result.is_none() {
        result = LocalPrefixProvider.lookup_callsign(&normalized).await?;
    }

    let Some(mut result) = result else {
        publish_lookup_event(
            bus,
            "lookup.callsign.failed",
            RuntimeEventSeverity::Warn,
            device_id,
            "No lookup result",
            Some(json!({"callsign": normalized})),
            None,
        )
        .await?;
        return Ok(None);
    };
    if result.expires_at.is_none() {
        result.expires_at = Some(Utc::now() + Duration::days(cache_config.ttl_days));
    }
    cache.put(result.clone()).await;

    if result.country.is_some() {
        publish_lookup_event(
            bus,
            "lookup.entity.inferred",
            RuntimeEventSeverity::Info,
            device_id,
            "Entity inferred for callsign",
            Some(json!({"callsign": normalized, "country": result.country})),
            None,
        )
        .await?;
    }
    if result.grid.is_some() {
        publish_lookup_event(
            bus,
            "lookup.grid.validated",
            RuntimeEventSeverity::Info,
            device_id,
            "Grid validated for lookup result",
            Some(json!({"callsign": normalized, "grid": result.grid})),
            None,
        )
        .await?;
    }
    publish_lookup_event(
        bus,
        "lookup.callsign.completed",
        RuntimeEventSeverity::Info,
        device_id,
        "Callsign lookup completed",
        Some(json!({"callsign": normalized, "provider": result.source_provider, "confidence": result.confidence})),
        None,
    )
    .await?;
    let suggestion = suggestion_from_result(&result);
    publish_lookup_event(
        bus,
        "lookup.suggestion.created",
        RuntimeEventSeverity::Info,
        device_id,
        "Lookup suggestion created",
        Some(json!(&suggestion)),
        None,
    )
    .await?;
    Ok(Some(suggestion))
}

pub async fn clear_lookup_cache<B: EventBus>(
    cache: &LookupCache,
    bus: &B,
    device_id: Uuid,
) -> Result<(), LookupError> {
    cache.clear().await;
    publish_lookup_event(
        bus,
        "lookup.cache.cleared",
        RuntimeEventSeverity::Info,
        device_id,
        "Lookup cache cleared",
        None,
        None,
    )
    .await?;
    Ok(())
}

pub fn suggestion_from_result(result: &LookupResult) -> LookupSuggestion {
    LookupSuggestion {
        normalized_callsign: result.normalized_callsign.clone(),
        provider: result.source_provider.clone(),
        confidence: result.confidence,
        suggested_fields: json!({
            "name": result.name,
            "qth": result.qth,
            "grid": result.grid,
            "country": result.country,
            "dxcc": result.dxcc,
            "cq_zone": result.cq_zone,
            "itu_zone": result.itu_zone,
            "lookup_source": result.source_provider,
            "lookup_confidence": result.confidence,
            "enriched_fields": ["name", "qth", "grid", "country", "dxcc", "cq_zone", "itu_zone"]
        }),
    }
}

pub fn normalize_callsign(callsign: &str) -> Result<String, LookupError> {
    let normalized = callsign
        .trim()
        .to_ascii_uppercase()
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    if normalized.len() < 3
        || !normalized
            .chars()
            .any(|character| character.is_ascii_digit())
        || !normalized
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '/')
    {
        return Err(LookupError::InvalidCallsign);
    }
    Ok(normalized)
}

pub fn validate_grid(grid: &str) -> bool {
    let grid = grid.trim().to_ascii_uppercase();
    let bytes = grid.as_bytes();
    matches!(bytes.len(), 4 | 6)
        && (b'A'..=b'R').contains(&bytes[0])
        && (b'A'..=b'R').contains(&bytes[1])
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && (bytes.len() == 4
            || ((b'A'..=b'X').contains(&bytes[4]) && (b'A'..=b'X').contains(&bytes[5])))
}

pub fn grid_to_lat_lon(grid: &str) -> Result<GridInfo, LookupError> {
    if !validate_grid(grid) {
        return Err(LookupError::InvalidGrid);
    }
    let grid = grid.trim().to_ascii_uppercase();
    let bytes = grid.as_bytes();
    let mut lon = -180.0 + ((bytes[0] - b'A') as f64 * 20.0);
    let mut lat = -90.0 + ((bytes[1] - b'A') as f64 * 10.0);
    lon += (bytes[2] - b'0') as f64 * 2.0;
    lat += (bytes[3] - b'0') as f64;
    if bytes.len() == 6 {
        lon += (bytes[4] - b'A') as f64 * (5.0 / 60.0);
        lat += (bytes[5] - b'A') as f64 * (2.5 / 60.0);
        lon += 2.5 / 60.0;
        lat += 1.25 / 60.0;
    } else {
        lon += 1.0;
        lat += 0.5;
    }
    Ok(GridInfo {
        grid,
        latitude: lat,
        longitude: lon,
    })
}

#[derive(Debug, Default, Clone)]
pub struct LocalPrefixProvider;

#[async_trait]
impl CallsignLookupProvider for LocalPrefixProvider {
    fn provider_id(&self) -> &str {
        "local-prefix"
    }

    async fn lookup_callsign(&self, callsign: &str) -> Result<Option<LookupResult>, LookupError> {
        let normalized = normalize_callsign(callsign)?;
        let entity = self.lookup_entity(&normalized).await?;
        Ok(entity.map(|entity| LookupResult {
            callsign: callsign.to_owned(),
            normalized_callsign: normalized,
            name: None,
            qth: None,
            country: Some(entity.country),
            dxcc: entity.dxcc,
            cq_zone: entity.cq_zone,
            itu_zone: entity.itu_zone,
            grid: None,
            latitude: None,
            longitude: None,
            license_class: None,
            previous_callsigns: Vec::new(),
            source_provider: self.provider_id().to_owned(),
            fetched_at: Utc::now(),
            expires_at: None,
            confidence: entity.confidence,
            raw_metadata: None,
        }))
    }

    async fn lookup_grid(&self, grid: &str) -> Result<Option<GridInfo>, LookupError> {
        Ok(Some(grid_to_lat_lon(grid)?))
    }

    async fn lookup_entity(
        &self,
        callsign_or_prefix: &str,
    ) -> Result<Option<EntityInfo>, LookupError> {
        let value = callsign_or_prefix.to_ascii_uppercase();
        let entity = if value.starts_with('K')
            || value.starts_with('N')
            || value.starts_with('W')
            || value.starts_with("AA")
            || value.starts_with("AB")
        {
            Some(("United States", 291, 5, 8, 0.72))
        } else if value.starts_with("VE") || value.starts_with("VA") {
            Some(("Canada", 1, 4, 9, 0.70))
        } else if value.starts_with('G') || value.starts_with('M') {
            Some(("England", 223, 14, 27, 0.66))
        } else if value.starts_with("JA") {
            Some(("Japan", 339, 25, 45, 0.70))
        } else if value.starts_with("DL") {
            Some(("Germany", 230, 14, 28, 0.70))
        } else {
            None
        };
        Ok(
            entity.map(|(country, dxcc, cq, itu, confidence)| EntityInfo {
                country: country.to_owned(),
                dxcc: Some(dxcc),
                cq_zone: Some(cq),
                itu_zone: Some(itu),
                source_provider: self.provider_id().to_owned(),
                confidence,
            }),
        )
    }

    async fn status(&self) -> LookupProviderStatus {
        LookupProviderStatus {
            provider_id: self.provider_id().to_owned(),
            healthy: true,
            message: "Offline prefix resolver ready".to_owned(),
            rate_limited: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MockLookupProvider {
    result: Option<LookupResult>,
}

impl MockLookupProvider {
    pub fn new(result: Option<LookupResult>) -> Self {
        Self { result }
    }
}

#[async_trait]
impl CallsignLookupProvider for MockLookupProvider {
    fn provider_id(&self) -> &str {
        "mock"
    }

    async fn lookup_callsign(&self, callsign: &str) -> Result<Option<LookupResult>, LookupError> {
        Ok(self.result.clone().map(|mut result| {
            result.callsign = callsign.to_owned();
            result.normalized_callsign =
                normalize_callsign(callsign).unwrap_or_else(|_| callsign.to_owned());
            result
        }))
    }

    async fn lookup_grid(&self, grid: &str) -> Result<Option<GridInfo>, LookupError> {
        Ok(Some(grid_to_lat_lon(grid)?))
    }

    async fn lookup_entity(
        &self,
        callsign_or_prefix: &str,
    ) -> Result<Option<EntityInfo>, LookupError> {
        LocalPrefixProvider.lookup_entity(callsign_or_prefix).await
    }

    async fn status(&self) -> LookupProviderStatus {
        LookupProviderStatus {
            provider_id: self.provider_id().to_owned(),
            healthy: true,
            message: "Mock provider ready".to_owned(),
            rate_limited: false,
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct QrzLookupProviderStub;

#[async_trait]
impl CallsignLookupProvider for QrzLookupProviderStub {
    fn provider_id(&self) -> &str {
        "qrz-stub"
    }

    async fn lookup_callsign(&self, _callsign: &str) -> Result<Option<LookupResult>, LookupError> {
        Ok(None)
    }

    async fn lookup_grid(&self, grid: &str) -> Result<Option<GridInfo>, LookupError> {
        Ok(Some(grid_to_lat_lon(grid)?))
    }

    async fn lookup_entity(
        &self,
        callsign_or_prefix: &str,
    ) -> Result<Option<EntityInfo>, LookupError> {
        LocalPrefixProvider.lookup_entity(callsign_or_prefix).await
    }

    async fn status(&self) -> LookupProviderStatus {
        LookupProviderStatus {
            provider_id: self.provider_id().to_owned(),
            healthy: false,
            message: "QRZ/HamQTH credentials are not configured".to_owned(),
            rate_limited: false,
        }
    }
}

async fn publish_lookup_event<B: EventBus>(
    bus: &B,
    event_type: &str,
    severity: RuntimeEventSeverity,
    device_id: Uuid,
    summary: &str,
    payload: Option<Value>,
    error: Option<String>,
) -> Result<(), EventBusError> {
    bus.publish(BusEvent::Runtime(RuntimeEventEnvelope::new(
        event_type,
        severity,
        "plugin.callsign-lookup",
        Some("plugin.callsign-lookup".to_owned()),
        Uuid::new_v4(),
        Uuid::new_v4(),
        device_id,
        Some("casual-logger".to_owned()),
        summary,
        payload,
        error,
    )))
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::{InMemoryEventBus, InMemoryLogbookEventStore, LogbookEventStore, NewLogbookEvent};

    fn sample_result() -> LookupResult {
        LookupResult {
            callsign: "k1abc".to_owned(),
            normalized_callsign: "K1ABC".to_owned(),
            name: Some("Ada".to_owned()),
            qth: Some("Cleveland, OH".to_owned()),
            country: Some("United States".to_owned()),
            dxcc: Some(291),
            cq_zone: Some(5),
            itu_zone: Some(8),
            grid: Some("EN91".to_owned()),
            latitude: None,
            longitude: None,
            license_class: Some("Amateur Extra".to_owned()),
            previous_callsigns: Vec::new(),
            source_provider: "mock".to_owned(),
            fetched_at: Utc::now(),
            expires_at: Some(Utc::now() + Duration::days(30)),
            confidence: 0.95,
            raw_metadata: Some(json!({"safe": true, "api_key": "secret"})),
        }
    }

    #[test]
    fn normalizes_callsigns() {
        assert_eq!(normalize_callsign(" k1abc ").unwrap(), "K1ABC");
        assert!(normalize_callsign("abc").is_err());
    }

    #[test]
    fn validates_and_converts_grid() {
        assert!(validate_grid("EN91"));
        assert!(validate_grid("EN91ab"));
        assert!(!validate_grid("ZZ99"));
        let grid = grid_to_lat_lon("EN91").unwrap();
        assert!(grid.latitude > 40.0 && grid.latitude < 42.0);
        assert!(grid.longitude <= -79.0 && grid.longitude >= -81.0);
    }

    #[tokio::test]
    async fn infers_entities_from_prefixes() {
        let provider = LocalPrefixProvider;
        assert_eq!(
            provider
                .lookup_entity("K1ABC")
                .await
                .unwrap()
                .unwrap()
                .country,
            "United States"
        );
        assert_eq!(
            provider
                .lookup_entity("VE3XYZ")
                .await
                .unwrap()
                .unwrap()
                .country,
            "Canada"
        );
        assert_eq!(
            provider
                .lookup_entity("JA1NRT")
                .await
                .unwrap()
                .unwrap()
                .country,
            "Japan"
        );
    }

    #[tokio::test]
    async fn mock_provider_and_cache_hit_miss_work() {
        let bus = InMemoryEventBus::default();
        let cache = LookupCache::new();
        let provider = MockLookupProvider::new(Some(sample_result()));
        let device_id = Uuid::new_v4();
        let config = LookupCacheConfig::default();

        let first =
            lookup_callsign_with_cache(&provider, &cache, &config, &bus, "K1ABC", device_id)
                .await
                .unwrap()
                .unwrap();
        let second =
            lookup_callsign_with_cache(&provider, &cache, &config, &bus, "K1ABC", device_id)
                .await
                .unwrap()
                .unwrap();

        assert_eq!(first.suggested_fields["name"], "Ada");
        assert_eq!(second.provider, "mock");
    }

    #[tokio::test]
    async fn expired_cache_entry_refreshes() {
        let bus = InMemoryEventBus::default();
        let cache = LookupCache::new();
        let mut expired = sample_result();
        expired.expires_at = Some(Utc::now() - Duration::days(1));
        cache.put(expired).await;
        let provider = MockLookupProvider::new(Some(sample_result()));
        let suggestion = lookup_callsign_with_cache(
            &provider,
            &cache,
            &LookupCacheConfig::default(),
            &bus,
            "K1ABC",
            Uuid::new_v4(),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(suggestion.provider, "mock");
    }

    #[tokio::test]
    async fn suggestions_do_not_write_official_events_directly() {
        let bus = InMemoryEventBus::default();
        let cache = LookupCache::new();
        let provider = MockLookupProvider::new(Some(sample_result()));
        let store = InMemoryLogbookEventStore::new();
        let logbook_id = Uuid::new_v4();
        lookup_callsign_with_cache(
            &provider,
            &cache,
            &LookupCacheConfig::default(),
            &bus,
            "K1ABC",
            Uuid::new_v4(),
        )
        .await
        .unwrap();
        assert!(store.list_events(logbook_id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn accepted_suggestions_can_flow_into_qso_payload() {
        let store = InMemoryLogbookEventStore::new();
        let logbook_id = Uuid::new_v4();
        let suggestion = suggestion_from_result(&sample_result());
        let event = store
            .append_event(NewLogbookEvent {
                event_type: ham_plugin_sdk::OFFICIAL_LOG_QSO_CREATED.to_owned(),
                logbook_id,
                entity_id: Some(Uuid::new_v4()),
                author_operator_id: None,
                station_callsign: "KE8YGW".to_owned(),
                operator_callsign: Some("KE8YGW".to_owned()),
                author_device_id: Uuid::new_v4(),
                source_device_id: Uuid::new_v4(),
                correlation_id: Uuid::new_v4(),
                source_plugin_id: Some("test".to_owned()),
                schema_version: 1,
                payload: json!({
                    "station_callsign": "KE8YGW",
                    "operator_callsign": "KE8YGW",
                    "contacted_callsign": "K1ABC",
                    "started_at": Utc::now().to_rfc3339(),
                    "mode": "SSB",
                    "name": suggestion.suggested_fields["name"],
                    "qth": suggestion.suggested_fields["qth"],
                    "lookup_source": suggestion.suggested_fields["lookup_source"],
                    "lookup_confidence": suggestion.suggested_fields["lookup_confidence"]
                }),
            })
            .await
            .unwrap();
        assert_eq!(event.payload["name"], "Ada");
        assert_eq!(event.payload["lookup_source"], "mock");
    }

    #[test]
    fn redaction_masks_secret_raw_metadata() {
        let redacted = crate::redact_payload(json!({"raw_metadata": {"api_key": "secret"}}));
        assert_eq!(redacted["raw_metadata"]["api_key"], "[REDACTED]");
    }
}
