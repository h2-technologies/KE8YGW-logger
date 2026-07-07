//! Online service ecosystem models and offline-testable provider helpers.
//!
//! Real network calls stay behind provider implementations. This module defines
//! the shared upload/download, health, spotting, automation, notification, and
//! provider metadata primitives used by online integrations.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};
use ham_plugin_sdk::{PluginCapability, ServiceType, OFFICIAL_LOG_UPLOAD_COMPLETED};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    adif::parse_adif,
    credential::{CredentialMetadata, CredentialStatus},
    event::{CoreEventEnvelope, NewLogbookEvent},
    projection::QsoCurrentStateProjection,
    service::{
        cache_entry_for_value, LogUploadProvider, LogUploadRequest, LogUploadResponse,
        ProviderHealth, ProviderHealthState, ServiceCache, ServiceError, ServiceProviderMetadata,
        Spot, SpotSource, UploadJobStatus, CAP_MAP_REVERSE_GEOCODING, CAP_MAP_TILES_OFFLINE,
        CAP_MAP_TILES_ONLINE, CAP_PROPAGATION_SOLAR_INDICES, CAP_SPOTTING_DX_CLUSTER,
        CAP_SPOTTING_POTA, CAP_SPOTTING_RBN, CAP_SPOTTING_SOTA, CAP_UPLOAD_ADIF,
        CAP_UPLOAD_CONFIRMATION_PULL, CAP_UPLOAD_INCREMENTAL, CAP_WEATHER_CURRENT,
        CAP_WEATHER_FORECAST,
    },
    store::{LogbookEventStore, StoreError},
    upload::{adif_for_upload_job, UploadJob, UploadQueue, UploadQueueError, UploadStatus},
    RuntimeEventSeverity,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnlineServiceProviderKind {
    Lotw,
    Eqsl,
    ClubLog,
    QrzLogbook,
    HrdLog,
    QrzXml,
    HamQth,
    FccUls,
    PrefixFallback,
    DxCluster,
    ReverseBeaconNetwork,
    PotaSpots,
    SotaWatch,
    NoaaSpaceWeather,
    NoaaWeather,
    OpenMeteo,
    OpenStreetMap,
    OfflineTileCache,
    ReverseGeocoder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnlineProviderStatus {
    Healthy,
    MissingCredentials,
    Offline,
    ApiUnavailable,
    RateLimited,
    AuthenticationFailed,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlineProviderHealth {
    pub provider_id: String,
    pub status: OnlineProviderStatus,
    pub message: String,
    pub checked_at: DateTime<Utc>,
    pub retry_after_seconds: Option<u64>,
}

impl OnlineProviderHealth {
    pub fn from_provider_health(health: &ProviderHealth) -> Self {
        let status = match health.state {
            ProviderHealthState::Healthy | ProviderHealthState::Degraded if health.rate_limited => {
                OnlineProviderStatus::RateLimited
            }
            ProviderHealthState::Healthy | ProviderHealthState::Degraded => {
                OnlineProviderStatus::Healthy
            }
            ProviderHealthState::MissingConfig => OnlineProviderStatus::MissingCredentials,
            ProviderHealthState::Unavailable => OnlineProviderStatus::ApiUnavailable,
        };
        Self {
            provider_id: health.provider_id.clone(),
            status,
            message: health.message.clone(),
            checked_at: health.checked_at,
            retry_after_seconds: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlineAccount {
    pub account_id: Uuid,
    pub provider_id: String,
    pub display_name: String,
    pub credential_ids: Vec<Uuid>,
    pub enabled: bool,
    pub last_authenticated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    pub max_attempts: u8,
    pub initial_backoff_seconds: u64,
    pub max_backoff_seconds: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_backoff_seconds: 60,
            max_backoff_seconds: 900,
        }
    }
}

pub fn next_retry_delay(policy: &RetryPolicy, attempt: u8) -> Duration {
    let exponent = attempt.saturating_sub(1).min(10);
    let delay = policy
        .initial_backoff_seconds
        .saturating_mul(2_u64.saturating_pow(exponent.into()))
        .min(policy.max_backoff_seconds);
    Duration::seconds(delay as i64)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadEngineConfig {
    pub automatic_upload_enabled: bool,
    pub scheduled_upload_enabled: bool,
    pub upload_interval_minutes: u32,
    pub confirmation_download_enabled: bool,
    pub confirmation_download_interval_minutes: u32,
    pub retry_policy: RetryPolicy,
}

impl Default for UploadEngineConfig {
    fn default() -> Self {
        Self {
            automatic_upload_enabled: false,
            scheduled_upload_enabled: false,
            upload_interval_minutes: 10,
            confirmation_download_enabled: false,
            confirmation_download_interval_minutes: 60,
            retry_policy: RetryPolicy::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UploadEngineStats {
    pub queued: usize,
    pub running: usize,
    pub completed: usize,
    pub failed: usize,
    pub retryable: usize,
}

pub fn upload_engine_stats(queue: &UploadQueue) -> UploadEngineStats {
    let mut stats = UploadEngineStats {
        queued: 0,
        running: 0,
        completed: 0,
        failed: 0,
        retryable: 0,
    };
    for job in &queue.jobs {
        match job.status {
            UploadStatus::Queued => stats.queued += 1,
            UploadStatus::Running => stats.running += 1,
            UploadStatus::Completed => stats.completed += 1,
            UploadStatus::Failed => {
                stats.failed += 1;
                stats.retryable += 1;
            }
        }
    }
    stats
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UploadExecutionResult {
    pub job_id: Uuid,
    pub provider_id: String,
    pub status: UploadJobStatus,
    pub accepted_count: usize,
    pub rejected_count: usize,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub message: String,
}

#[derive(Debug, Error)]
pub enum OnlineServiceError {
    #[error("missing credentials for provider {0}")]
    MissingCredentials(String),
    #[error("provider error: {0}")]
    Provider(#[from] ServiceError),
    #[error("upload queue error: {0}")]
    UploadQueue(#[from] UploadQueueError),
    #[error("official store error: {0}")]
    Store(#[from] StoreError),
    #[error("confirmation record is invalid: {0}")]
    InvalidConfirmation(String),
}

pub async fn execute_upload_with_provider<P: LogUploadProvider>(
    provider: &P,
    job: &UploadJob,
    projection: &QsoCurrentStateProjection,
    attempt: u8,
    retry_policy: &RetryPolicy,
) -> Result<UploadExecutionResult, OnlineServiceError> {
    let request = LogUploadRequest {
        job_id: job.upload_job_id,
        logbook_id: job.logbook_id,
        provider_id: Some(job.target_id.clone()),
        adif_payload: adif_for_upload_job(projection, &job.qso_ids),
        incremental: true,
    };
    let response = provider.upload_adif(request).await?;
    Ok(upload_execution_from_response(
        response,
        attempt,
        retry_policy,
        Utc::now(),
    ))
}

pub fn upload_execution_from_response(
    response: LogUploadResponse,
    attempt: u8,
    retry_policy: &RetryPolicy,
    now: DateTime<Utc>,
) -> UploadExecutionResult {
    let retryable = matches!(
        response.status,
        UploadJobStatus::Failed | UploadJobStatus::NeedsCredentials
    ) && attempt < retry_policy.max_attempts;
    UploadExecutionResult {
        job_id: response.job_id,
        provider_id: response.provider_id,
        status: response.status,
        accepted_count: response.accepted_count,
        rejected_count: response.rejected_count,
        next_retry_at: retryable.then(|| now + next_retry_delay(retry_policy, attempt)),
        message: response.message,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationDownloadRequest {
    pub provider_id: String,
    pub logbook_id: Uuid,
    pub since: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationRecord {
    pub confirmation_id: Uuid,
    pub provider_id: String,
    pub qso_id: Option<Uuid>,
    pub contacted_callsign: String,
    pub band: Option<String>,
    pub mode: Option<String>,
    pub qso_date: Option<String>,
    pub confirmed_at: DateTime<Utc>,
    pub raw_reference: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfirmationDownloadResponse {
    pub provider_id: String,
    pub fetched_at: DateTime<Utc>,
    pub confirmations: Vec<ConfirmationRecord>,
    pub rejected_count: usize,
}

pub fn confirmations_from_adif(
    provider_id: impl Into<String>,
    adif: &str,
    confirmed_at: DateTime<Utc>,
) -> ConfirmationDownloadResponse {
    let provider_id = provider_id.into();
    let records = parse_adif(adif);
    let mut confirmations = Vec::new();
    let mut rejected_count = 0;
    for record in records {
        let Some(call) = record.get("CALL").filter(|value| !value.trim().is_empty()) else {
            rejected_count += 1;
            continue;
        };
        confirmations.push(ConfirmationRecord {
            confirmation_id: Uuid::new_v4(),
            provider_id: provider_id.clone(),
            qso_id: None,
            contacted_callsign: call.to_ascii_uppercase(),
            band: record.get("BAND").cloned(),
            mode: record.get("MODE").cloned(),
            qso_date: record.get("QSO_DATE").cloned(),
            confirmed_at,
            raw_reference: record.get("APP_LOTW_QSL_RCVD").cloned(),
        });
    }
    ConfirmationDownloadResponse {
        provider_id,
        fetched_at: Utc::now(),
        confirmations,
        rejected_count,
    }
}

pub async fn append_confirmation_events<S: LogbookEventStore>(
    store: &S,
    logbook_id: Uuid,
    response: &ConfirmationDownloadResponse,
    source_device_id: Uuid,
) -> Result<Vec<CoreEventEnvelope>, OnlineServiceError> {
    let mut events = Vec::new();
    for confirmation in &response.confirmations {
        let event = store
            .append_event(NewLogbookEvent {
                event_type: OFFICIAL_LOG_UPLOAD_COMPLETED.to_owned(),
                logbook_id,
                entity_id: confirmation.qso_id.or(Some(confirmation.confirmation_id)),
                author_operator_id: None,
                station_callsign: "SYSTEM".to_owned(),
                operator_callsign: None,
                source_device_id,
                author_device_id: source_device_id,
                correlation_id: Uuid::new_v4(),
                source_plugin_id: Some("plugin.online-services".to_owned()),
                schema_version: 1,
                payload: json!({
                    "provider_id": response.provider_id,
                    "confirmation_id": confirmation.confirmation_id,
                    "qso_id": confirmation.qso_id,
                    "contacted_callsign": confirmation.contacted_callsign,
                    "band": confirmation.band,
                    "mode": confirmation.mode,
                    "confirmed_at": confirmation.confirmed_at
                }),
            })
            .await?;
        events.push(event);
    }
    Ok(events)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxClusterSpot {
    pub spotter_callsign: String,
    pub spotted_callsign: String,
    pub frequency_hz: u64,
    pub comment: Option<String>,
    pub spotted_at: Option<String>,
}

pub fn parse_dx_cluster_line(line: &str) -> Option<DxClusterSpot> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix("DX de ")?;
    let mut parts = rest.split_whitespace().collect::<Vec<_>>();
    if parts.len() < 3 {
        return None;
    }
    let spotter_callsign = parts.remove(0).trim_end_matches(':').to_ascii_uppercase();
    let frequency_khz = parts.remove(0).parse::<f64>().ok()?;
    let spotted_callsign = parts.remove(0).to_ascii_uppercase();
    let spotted_at = parts
        .last()
        .filter(|value| value.ends_with('Z') && value.len() >= 5)
        .map(|value| (*value).to_owned());
    if spotted_at.is_some() {
        parts.pop();
    }
    Some(DxClusterSpot {
        spotter_callsign,
        spotted_callsign,
        frequency_hz: (frequency_khz * 1_000.0).round() as u64,
        comment: (!parts.is_empty()).then(|| parts.join(" ")),
        spotted_at,
    })
}

pub fn dx_cluster_spot_to_spot(parsed: DxClusterSpot, provider_id: &str) -> Spot {
    Spot {
        spotted_callsign: parsed.spotted_callsign,
        spotter_callsign: Some(parsed.spotter_callsign),
        frequency_hz: parsed.frequency_hz,
        band: None,
        mode: None,
        comment: parsed.comment,
        source: SpotSource {
            provider_id: provider_id.to_owned(),
            label: "DX Cluster".to_owned(),
        },
        spotted_at: Utc::now(),
        entity: None,
        grid: None,
        reference: None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PotaSpotRecord {
    pub activator: String,
    pub reference: String,
    pub frequency_hz: u64,
    pub mode: Option<String>,
    pub spotted_at: DateTime<Utc>,
    pub comments: Option<String>,
}

pub fn pota_spot_to_spot(record: PotaSpotRecord) -> Spot {
    Spot {
        spotted_callsign: record.activator,
        spotter_callsign: None,
        frequency_hz: record.frequency_hz,
        band: None,
        mode: record.mode,
        comment: record.comments,
        source: SpotSource {
            provider_id: "pota-spots".to_owned(),
            label: "POTA Spots".to_owned(),
        },
        spotted_at: record.spotted_at,
        entity: None,
        grid: None,
        reference: Some(record.reference),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolarIndexReport {
    pub provider_id: String,
    pub fetched_at: DateTime<Utc>,
    pub sfi: Option<f32>,
    pub a_index: Option<f32>,
    pub k_index: Option<f32>,
    pub xray_class: Option<String>,
    pub aurora: Option<String>,
    pub band_conditions: HashMap<String, String>,
}

pub fn parse_noaa_solar_summary(summary: &str) -> SolarIndexReport {
    let mut report = SolarIndexReport {
        provider_id: "noaa-space-weather".to_owned(),
        fetched_at: Utc::now(),
        sfi: None,
        a_index: None,
        k_index: None,
        xray_class: None,
        aurora: None,
        band_conditions: HashMap::new(),
    };
    for token in summary.split_whitespace() {
        if let Some(value) = token.strip_prefix("SFI=") {
            report.sfi = value.parse().ok();
        } else if let Some(value) = token.strip_prefix("A=") {
            report.a_index = value.parse().ok();
        } else if let Some(value) = token.strip_prefix("K=") {
            report.k_index = value.parse().ok();
        } else if let Some(value) = token.strip_prefix("Xray=") {
            report.xray_class = Some(value.to_owned());
        } else if let Some(value) = token.strip_prefix("Aurora=") {
            report.aurora = Some(value.to_owned());
        }
    }
    report
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnlineAutomationTask {
    pub task_id: Uuid,
    pub name: String,
    pub service_type: ServiceType,
    pub provider_id: Option<String>,
    pub interval_seconds: u64,
    pub enabled: bool,
    pub last_run_at: Option<DateTime<Utc>>,
    pub next_run_at: Option<DateTime<Utc>>,
}

impl OnlineAutomationTask {
    pub fn new(name: impl Into<String>, service_type: ServiceType, interval_seconds: u64) -> Self {
        let now = Utc::now();
        Self {
            task_id: Uuid::new_v4(),
            name: name.into(),
            service_type,
            provider_id: None,
            interval_seconds,
            enabled: false,
            last_run_at: None,
            next_run_at: Some(now + Duration::seconds(interval_seconds as i64)),
        }
    }
}

pub fn default_online_automation_tasks() -> Vec<OnlineAutomationTask> {
    vec![
        OnlineAutomationTask::new("Upload every 10 minutes", ServiceType::LogUpload, 600),
        OnlineAutomationTask::new(
            "Download confirmations hourly",
            ServiceType::LogUpload,
            3_600,
        ),
        OnlineAutomationTask::new("Refresh propagation", ServiceType::Propagation, 1_800),
        OnlineAutomationTask::new("Refresh weather", ServiceType::Weather, 1_800),
        OnlineAutomationTask::new("Refresh DX spots", ServiceType::Spotting, 30),
        OnlineAutomationTask::new("Refresh POTA spots", ServiceType::Spotting, 60),
        OnlineAutomationTask::new("Refresh SOTA spots", ServiceType::Spotting, 60),
    ]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationSeverity {
    Info,
    Success,
    Warning,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnlineNotification {
    pub notification_id: Uuid,
    pub event_type: String,
    pub severity: NotificationSeverity,
    pub title: String,
    pub message: String,
    pub created_at: DateTime<Utc>,
    pub related_provider_id: Option<String>,
    pub related_qso_id: Option<Uuid>,
}

impl OnlineNotification {
    pub fn new(
        event_type: impl Into<String>,
        severity: NotificationSeverity,
        title: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            notification_id: Uuid::new_v4(),
            event_type: event_type.into(),
            severity,
            title: title.into(),
            message: message.into(),
            created_at: Utc::now(),
            related_provider_id: None,
            related_qso_id: None,
        }
    }
}

pub fn notification_for_upload_result(result: &UploadExecutionResult) -> OnlineNotification {
    let severity = match result.status {
        UploadJobStatus::Succeeded => NotificationSeverity::Success,
        UploadJobStatus::Failed | UploadJobStatus::NeedsCredentials => {
            NotificationSeverity::Warning
        }
        UploadJobStatus::Queued | UploadJobStatus::Running => NotificationSeverity::Info,
    };
    let mut notification = OnlineNotification::new(
        "notification.upload.status",
        severity,
        "Upload status changed",
        format!("{}: {}", result.provider_id, result.message),
    );
    notification.related_provider_id = Some(result.provider_id.clone());
    notification
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnlineServicesDashboard {
    pub providers: Vec<ServiceProviderMetadata>,
    pub health: Vec<OnlineProviderHealth>,
    pub credentials: Vec<CredentialMetadata>,
    pub upload_stats: UploadEngineStats,
    pub automation_tasks: Vec<OnlineAutomationTask>,
    pub notifications: Vec<OnlineNotification>,
    pub cache_entries: usize,
}

pub async fn online_services_dashboard(
    providers: Vec<(ServiceProviderMetadata, ProviderHealth)>,
    credentials: Vec<CredentialMetadata>,
    upload_queue: &UploadQueue,
    cache: &ServiceCache,
    notifications: Vec<OnlineNotification>,
) -> OnlineServicesDashboard {
    OnlineServicesDashboard {
        providers: providers
            .iter()
            .map(|(metadata, _)| metadata.clone())
            .collect(),
        health: providers
            .iter()
            .map(|(_, health)| OnlineProviderHealth::from_provider_health(health))
            .collect(),
        credentials,
        upload_stats: upload_engine_stats(upload_queue),
        automation_tasks: default_online_automation_tasks(),
        notifications,
        cache_entries: cache.count().await,
    }
}

pub fn provider_metadata_for_kind(kind: OnlineServiceProviderKind) -> ServiceProviderMetadata {
    match kind {
        OnlineServiceProviderKind::Lotw => logbook_metadata(
            "lotw",
            "ARRL Logbook of The World",
            vec![
                CAP_UPLOAD_ADIF,
                CAP_UPLOAD_INCREMENTAL,
                CAP_UPLOAD_CONFIRMATION_PULL,
            ],
            vec!["lotw.certificate.credential_id"],
        ),
        OnlineServiceProviderKind::Eqsl => logbook_metadata(
            "eqsl",
            "eQSL",
            vec![CAP_UPLOAD_ADIF, CAP_UPLOAD_CONFIRMATION_PULL],
            vec!["eqsl.password.credential_id"],
        ),
        OnlineServiceProviderKind::ClubLog => logbook_metadata(
            "clublog",
            "Club Log",
            vec![CAP_UPLOAD_ADIF, CAP_UPLOAD_INCREMENTAL],
            vec!["clublog.password.credential_id"],
        ),
        OnlineServiceProviderKind::QrzLogbook => logbook_metadata(
            "qrz-logbook",
            "QRZ Logbook",
            vec![CAP_UPLOAD_ADIF, CAP_UPLOAD_CONFIRMATION_PULL],
            vec!["qrz.api_key.credential_id"],
        ),
        OnlineServiceProviderKind::HrdLog => logbook_metadata(
            "hrdlog",
            "HRDLog",
            vec![CAP_UPLOAD_ADIF],
            vec!["hrdlog.upload_code.credential_id"],
        ),
        OnlineServiceProviderKind::QrzXml => lookup_metadata(
            "qrz-xml",
            "QRZ XML API",
            true,
            vec!["qrz.password.credential_id"],
        ),
        OnlineServiceProviderKind::HamQth => lookup_metadata(
            "hamqth",
            "HamQTH",
            true,
            vec!["hamqth.password.credential_id"],
        ),
        OnlineServiceProviderKind::FccUls => lookup_metadata("fcc-uls", "FCC ULS", true, vec![]),
        OnlineServiceProviderKind::PrefixFallback => {
            lookup_metadata("prefix-fallback", "Offline Prefix Fallback", false, vec![])
        }
        OnlineServiceProviderKind::DxCluster => {
            spotting_metadata("dx-cluster", "DX Cluster", CAP_SPOTTING_DX_CLUSTER)
        }
        OnlineServiceProviderKind::ReverseBeaconNetwork => {
            spotting_metadata("rbn", "Reverse Beacon Network", CAP_SPOTTING_RBN)
        }
        OnlineServiceProviderKind::PotaSpots => {
            spotting_metadata("pota-spots", "POTA Spots", CAP_SPOTTING_POTA)
        }
        OnlineServiceProviderKind::SotaWatch => {
            spotting_metadata("sotawatch", "SOTAWatch", CAP_SPOTTING_SOTA)
        }
        OnlineServiceProviderKind::NoaaSpaceWeather => propagation_metadata(),
        OnlineServiceProviderKind::NoaaWeather => weather_metadata("noaa-weather", "NOAA Weather"),
        OnlineServiceProviderKind::OpenMeteo => weather_metadata("open-meteo", "Open-Meteo"),
        OnlineServiceProviderKind::OpenStreetMap => map_metadata(
            "osm-tiles",
            "OpenStreetMap Tiles",
            vec![CAP_MAP_TILES_ONLINE, CAP_MAP_REVERSE_GEOCODING],
            true,
        ),
        OnlineServiceProviderKind::OfflineTileCache => map_metadata(
            "offline-tile-cache",
            "Offline Tile Cache",
            vec![CAP_MAP_TILES_OFFLINE],
            false,
        ),
        OnlineServiceProviderKind::ReverseGeocoder => map_metadata(
            "reverse-geocoder",
            "Reverse Geocoder",
            vec![CAP_MAP_REVERSE_GEOCODING],
            false,
        ),
    }
}

fn logbook_metadata(
    provider_id: &str,
    display_name: &str,
    capabilities: Vec<&str>,
    config_keys: Vec<&str>,
) -> ServiceProviderMetadata {
    let mut metadata = ServiceProviderMetadata::new(
        provider_id,
        ServiceType::LogUpload,
        display_name,
        "0.1.0",
        "plugin.online-services",
        capabilities.into_iter().map(str::to_owned).collect(),
        vec![
            PluginCapability::AdifExport,
            PluginCapability::UploadLog,
            PluginCapability::NetworkExternalUpload,
        ],
        20,
        false,
        true,
    );
    metadata.required_config_keys = config_keys.into_iter().map(str::to_owned).collect();
    metadata
}

fn lookup_metadata(
    provider_id: &str,
    display_name: &str,
    network: bool,
    config_keys: Vec<&str>,
) -> ServiceProviderMetadata {
    let mut permissions = vec![
        PluginCapability::LookupCallsign,
        PluginCapability::LookupEntity,
    ];
    if network {
        permissions.push(PluginCapability::NetworkExternalLookup);
    }
    let mut metadata = ServiceProviderMetadata::new(
        provider_id,
        ServiceType::CallsignLookup,
        display_name,
        "0.1.0",
        "plugin.online-services",
        vec![
            "lookup.callsign.basic".to_owned(),
            "lookup.callsign.full".to_owned(),
            "lookup.entity.prefix".to_owned(),
            "lookup.grid.validate".to_owned(),
        ],
        permissions,
        if network { 30 } else { 100 },
        !network,
        network,
    );
    metadata.required_config_keys = config_keys.into_iter().map(str::to_owned).collect();
    metadata
}

fn spotting_metadata(
    provider_id: &str,
    display_name: &str,
    capability: &str,
) -> ServiceProviderMetadata {
    ServiceProviderMetadata::new(
        provider_id,
        ServiceType::Spotting,
        display_name,
        "0.1.0",
        "plugin.online-services",
        vec![capability.to_owned()],
        vec![
            PluginCapability::SpottingView,
            PluginCapability::NetworkExternalSpotting,
        ],
        30,
        false,
        true,
    )
}

fn propagation_metadata() -> ServiceProviderMetadata {
    ServiceProviderMetadata::new(
        "noaa-space-weather",
        ServiceType::Propagation,
        "NOAA Space Weather",
        "0.1.0",
        "plugin.online-services",
        vec![CAP_PROPAGATION_SOLAR_INDICES.to_owned()],
        vec![
            PluginCapability::PropagationView,
            PluginCapability::NetworkExternalPropagation,
        ],
        30,
        false,
        true,
    )
}

fn weather_metadata(provider_id: &str, display_name: &str) -> ServiceProviderMetadata {
    ServiceProviderMetadata::new(
        provider_id,
        ServiceType::Weather,
        display_name,
        "0.1.0",
        "plugin.online-services",
        vec![
            CAP_WEATHER_CURRENT.to_owned(),
            CAP_WEATHER_FORECAST.to_owned(),
        ],
        vec![
            PluginCapability::WeatherView,
            PluginCapability::NetworkExternalWeather,
        ],
        30,
        false,
        true,
    )
}

fn map_metadata(
    provider_id: &str,
    display_name: &str,
    capabilities: Vec<&str>,
    network: bool,
) -> ServiceProviderMetadata {
    ServiceProviderMetadata::new(
        provider_id,
        ServiceType::MapTiles,
        display_name,
        "0.1.0",
        "plugin.online-services",
        capabilities.into_iter().map(str::to_owned).collect(),
        vec![
            PluginCapability::MapView,
            if network {
                PluginCapability::NetworkExternalMap
            } else {
                PluginCapability::ServiceCacheRead
            },
        ],
        30,
        !network,
        network,
    )
}

pub fn online_provider_metadata() -> Vec<ServiceProviderMetadata> {
    [
        OnlineServiceProviderKind::Lotw,
        OnlineServiceProviderKind::Eqsl,
        OnlineServiceProviderKind::ClubLog,
        OnlineServiceProviderKind::QrzLogbook,
        OnlineServiceProviderKind::HrdLog,
        OnlineServiceProviderKind::QrzXml,
        OnlineServiceProviderKind::HamQth,
        OnlineServiceProviderKind::FccUls,
        OnlineServiceProviderKind::PrefixFallback,
        OnlineServiceProviderKind::DxCluster,
        OnlineServiceProviderKind::ReverseBeaconNetwork,
        OnlineServiceProviderKind::PotaSpots,
        OnlineServiceProviderKind::SotaWatch,
        OnlineServiceProviderKind::NoaaSpaceWeather,
        OnlineServiceProviderKind::NoaaWeather,
        OnlineServiceProviderKind::OpenMeteo,
        OnlineServiceProviderKind::OpenStreetMap,
        OnlineServiceProviderKind::OfflineTileCache,
        OnlineServiceProviderKind::ReverseGeocoder,
    ]
    .into_iter()
    .map(provider_metadata_for_kind)
    .collect()
}

pub fn missing_credential_status(
    provider: &ServiceProviderMetadata,
    credentials: &[CredentialMetadata],
) -> Option<OnlineProviderHealth> {
    if provider.required_config_keys.is_empty() {
        return None;
    }
    let has_active = credentials.iter().any(|credential| {
        credential.provider_id == provider.provider_id
            && credential.status == CredentialStatus::Active
    });
    (!has_active).then(|| OnlineProviderHealth {
        provider_id: provider.provider_id.clone(),
        status: OnlineProviderStatus::MissingCredentials,
        message: "Provider requires credential references before network use".to_owned(),
        checked_at: Utc::now(),
        retry_after_seconds: None,
    })
}

pub async fn cache_provider_value(
    cache: &ServiceCache,
    provider: &ServiceProviderMetadata,
    key: impl Into<String>,
    value: Value,
    ttl: Duration,
) {
    cache
        .put(cache_entry_for_value(
            provider.service_type,
            &provider.provider_id,
            key,
            ttl,
            None,
            value,
        ))
        .await;
}

pub fn online_runtime_event_payload(provider_id: &str, action: &str) -> Value {
    json!({
        "provider_id": provider_id,
        "action": action,
        "credential_values_redacted": true,
    })
}

pub fn runtime_severity_for_provider_status(status: OnlineProviderStatus) -> RuntimeEventSeverity {
    match status {
        OnlineProviderStatus::Healthy => RuntimeEventSeverity::Info,
        OnlineProviderStatus::MissingCredentials | OnlineProviderStatus::RateLimited => {
            RuntimeEventSeverity::Warn
        }
        OnlineProviderStatus::Offline
        | OnlineProviderStatus::ApiUnavailable
        | OnlineProviderStatus::AuthenticationFailed
        | OnlineProviderStatus::Disabled => RuntimeEventSeverity::Error,
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;

    use super::*;
    use crate::{service::ServiceProvider, InMemoryLogbookEventStore};

    #[derive(Debug, Clone)]
    struct MockSuccessfulUploadProvider {
        metadata: ServiceProviderMetadata,
    }

    #[async_trait]
    impl ServiceProvider for MockSuccessfulUploadProvider {
        fn metadata(&self) -> ServiceProviderMetadata {
            self.metadata.clone()
        }

        async fn health(&self) -> ProviderHealth {
            ProviderHealth::healthy(&self.metadata.provider_id, "ok")
        }
    }

    #[async_trait]
    impl LogUploadProvider for MockSuccessfulUploadProvider {
        async fn upload_adif(
            &self,
            request: LogUploadRequest,
        ) -> Result<LogUploadResponse, ServiceError> {
            Ok(LogUploadResponse {
                job_id: request.job_id,
                provider_id: self.metadata.provider_id.clone(),
                status: UploadJobStatus::Succeeded,
                accepted_count: request.adif_payload.matches("<EOR>").count(),
                rejected_count: 0,
                confirmation_reference: Some("mock-confirmation".to_owned()),
                message: "accepted".to_owned(),
            })
        }
    }

    #[test]
    fn provider_metadata_covers_required_online_services() {
        let providers = online_provider_metadata();
        for provider_id in [
            "lotw",
            "eqsl",
            "clublog",
            "qrz-logbook",
            "hrdlog",
            "qrz-xml",
            "hamqth",
            "fcc-uls",
            "dx-cluster",
            "rbn",
            "pota-spots",
            "sotawatch",
            "noaa-space-weather",
            "noaa-weather",
            "open-meteo",
            "osm-tiles",
            "offline-tile-cache",
        ] {
            assert!(providers
                .iter()
                .any(|provider| provider.provider_id == provider_id));
        }
    }

    #[test]
    fn retry_policy_uses_bounded_exponential_backoff() {
        let policy = RetryPolicy {
            max_attempts: 5,
            initial_backoff_seconds: 10,
            max_backoff_seconds: 25,
        };
        assert_eq!(next_retry_delay(&policy, 1), Duration::seconds(10));
        assert_eq!(next_retry_delay(&policy, 2), Duration::seconds(20));
        assert_eq!(next_retry_delay(&policy, 3), Duration::seconds(25));
    }

    #[test]
    fn upload_response_builds_retry_result() {
        let policy = RetryPolicy::default();
        let response = LogUploadResponse {
            job_id: Uuid::new_v4(),
            provider_id: "lotw".to_owned(),
            status: UploadJobStatus::NeedsCredentials,
            accepted_count: 0,
            rejected_count: 0,
            confirmation_reference: None,
            message: "missing cert".to_owned(),
        };
        let result = upload_execution_from_response(response, 1, &policy, Utc::now());
        assert!(result.next_retry_at.is_some());
    }

    #[test]
    fn confirmations_parse_from_adif() {
        let response = confirmations_from_adif(
            "lotw",
            "<CALL:5>K1ABC<BAND:3>20M<MODE:3>FT8<QSO_DATE:8>20260706<EOR>",
            Utc::now(),
        );
        assert_eq!(response.confirmations.len(), 1);
        assert_eq!(response.confirmations[0].contacted_callsign, "K1ABC");
    }

    #[tokio::test]
    async fn confirmation_downloads_append_official_events() {
        let store = InMemoryLogbookEventStore::default();
        let logbook_id = Uuid::new_v4();
        let response = confirmations_from_adif(
            "lotw",
            "<CALL:5>K1ABC<BAND:3>20M<MODE:3>FT8<EOR>",
            Utc::now(),
        );
        let events = append_confirmation_events(&store, logbook_id, &response, Uuid::new_v4())
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].event_type, OFFICIAL_LOG_UPLOAD_COMPLETED);
    }

    #[test]
    fn dx_cluster_parser_handles_standard_spot_lines() {
        let spot = parse_dx_cluster_line("DX de K1ABC: 14074.0 JA1XYZ FT8 loud 1234Z").unwrap();
        assert_eq!(spot.spotter_callsign, "K1ABC");
        assert_eq!(spot.spotted_callsign, "JA1XYZ");
        assert_eq!(spot.frequency_hz, 14_074_000);
        assert_eq!(spot.spotted_at.as_deref(), Some("1234Z"));
    }

    #[test]
    fn pota_spot_maps_to_generic_spot() {
        let spot = pota_spot_to_spot(PotaSpotRecord {
            activator: "K1ABC".to_owned(),
            reference: "US-0001".to_owned(),
            frequency_hz: 14_074_000,
            mode: Some("FT8".to_owned()),
            spotted_at: Utc::now(),
            comments: None,
        });
        assert_eq!(spot.reference.as_deref(), Some("US-0001"));
        assert_eq!(spot.source.provider_id, "pota-spots");
    }

    #[test]
    fn solar_summary_parser_extracts_indices() {
        let report = parse_noaa_solar_summary("SFI=178 A=8 K=2 Xray=C1.2 Aurora=quiet");
        assert_eq!(report.sfi, Some(178.0));
        assert_eq!(report.a_index, Some(8.0));
        assert_eq!(report.k_index, Some(2.0));
        assert_eq!(report.xray_class.as_deref(), Some("C1.2"));
    }

    #[tokio::test]
    async fn dashboard_includes_provider_cache_and_upload_stats() {
        let providers = online_provider_metadata();
        let cache = ServiceCache::new();
        cache_provider_value(
            &cache,
            &providers[0],
            "health",
            json!({"ok": true}),
            Duration::minutes(5),
        )
        .await;
        let mut queue = UploadQueue::new(vec![]);
        queue.jobs.push_back(UploadJob {
            upload_job_id: Uuid::new_v4(),
            target_id: "lotw".to_owned(),
            logbook_id: Uuid::new_v4(),
            qso_ids: vec![],
            items: vec![],
            status: UploadStatus::Failed,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_error: Some("offline".to_owned()),
        });
        let dashboard = online_services_dashboard(
            providers
                .iter()
                .map(|provider| {
                    (
                        provider.clone(),
                        ProviderHealth::healthy(&provider.provider_id, "ok"),
                    )
                })
                .collect(),
            vec![],
            &queue,
            &cache,
            vec![],
        )
        .await;
        assert_eq!(dashboard.cache_entries, 1);
        assert_eq!(dashboard.upload_stats.failed, 1);
    }

    #[tokio::test]
    async fn mock_upload_provider_executes_with_adif() {
        let provider = MockSuccessfulUploadProvider {
            metadata: provider_metadata_for_kind(OnlineServiceProviderKind::Lotw),
        };
        let job = UploadJob {
            upload_job_id: Uuid::new_v4(),
            target_id: "lotw".to_owned(),
            logbook_id: Uuid::new_v4(),
            qso_ids: vec![],
            items: vec![],
            status: UploadStatus::Queued,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_error: None,
        };
        let projection = QsoCurrentStateProjection::new();
        let result =
            execute_upload_with_provider(&provider, &job, &projection, 1, &RetryPolicy::default())
                .await
                .unwrap();
        assert_eq!(result.status, UploadJobStatus::Succeeded);
    }
}
