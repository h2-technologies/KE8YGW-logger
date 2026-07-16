//! Online service ecosystem models and offline-testable provider helpers.
//!
//! Real network calls stay behind provider implementations. This module defines
//! the shared upload/download, health, spotting, automation, notification, and
//! provider metadata primitives used by online integrations.

use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration as StdDuration,
};

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
    lookup::{normalize_callsign, LookupResult},
    projection::QsoCurrentStateProjection,
    service::{
        cache_entry_for_value, LogUploadProvider, LogUploadRequest, LogUploadResponse,
        ProviderHealth, ProviderHealthState, ServiceCache, ServiceError, ServiceProviderMetadata,
        Spot, SpotSource, UploadJobStatus, CAP_LOOKUP_CALLSIGN_BASIC, CAP_MAP_REVERSE_GEOCODING,
        CAP_MAP_TILES_OFFLINE, CAP_MAP_TILES_ONLINE, CAP_PROPAGATION_SOLAR_INDICES,
        CAP_SPOTTING_DX_CLUSTER, CAP_SPOTTING_POTA, CAP_SPOTTING_RBN, CAP_SPOTTING_SOTA,
        CAP_UPLOAD_ADIF, CAP_UPLOAD_CONFIRMATION_PULL, CAP_UPLOAD_INCREMENTAL, CAP_WEATHER_CURRENT,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRuntimeHealthState {
    Unknown,
    Healthy,
    Degraded,
    RateLimited,
    Unavailable,
    Misconfigured,
    AuthenticationFailed,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRuntimeHealth {
    pub provider_id: String,
    pub state: ProviderRuntimeHealthState,
    pub last_success: Option<DateTime<Utc>>,
    pub last_failure: Option<DateTime<Utc>>,
    pub failure_category: Option<ProviderOutcomeKind>,
    pub consecutive_failures: u32,
    pub next_retry: Option<DateTime<Utc>>,
    pub queue_depth: usize,
    pub rate_limit_state: Option<ProviderRateLimitSnapshot>,
    pub credential_reference_status: CredentialStatus,
    pub credential_validation_status: Option<String>,
    pub data_freshness_seconds: Option<u64>,
    pub circuit_state: CircuitBreakerState,
}

impl ProviderRuntimeHealth {
    pub fn unknown(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            state: ProviderRuntimeHealthState::Unknown,
            last_success: None,
            last_failure: None,
            failure_category: None,
            consecutive_failures: 0,
            next_retry: None,
            queue_depth: 0,
            rate_limit_state: None,
            credential_reference_status: CredentialStatus::Missing,
            credential_validation_status: None,
            data_freshness_seconds: None,
            circuit_state: CircuitBreakerState::Closed,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAdapterMode {
    Fake,
    Live,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAdapterTestInput {
    pub provider_id: String,
    pub capability: Option<String>,
    pub enabled: bool,
    pub credential_reference_present: bool,
    pub credential_resolved: bool,
    pub mode: ProviderAdapterMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAdapterTestResult {
    pub provider_id: String,
    pub capability_tested: Option<String>,
    pub credential_required: bool,
    pub credential_reference_present: bool,
    pub credential_resolved: bool,
    pub test_status: String,
    pub provider_health_state: ProviderHealthState,
    pub redacted_diagnostics: Vec<String>,
    pub next_recommended_action: String,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderUploadInput {
    pub provider_id: String,
    pub job_id: Uuid,
    pub adif_payload: String,
    pub qso_count: usize,
    pub enabled: bool,
    pub credential_reference_present: bool,
    pub credential_resolved: bool,
    #[serde(skip_serializing, skip_deserializing)]
    pub credential_secret: Option<String>,
    pub mode: ProviderAdapterMode,
    pub force_fake_failure: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderUploadExecution {
    pub provider_id: String,
    pub status: UploadJobStatus,
    pub accepted_count: usize,
    pub rejected_count: usize,
    pub provider_correlation_id: Option<String>,
    pub result_summary: String,
    pub failure_reason: Option<String>,
    pub redacted_error: Option<String>,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRuntimeStatus {
    Succeeded,
    NotFound,
    Failed,
    NeedsCredentials,
    Disabled,
    LiveModeNotConfigured,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderLookupInput {
    pub provider_id: String,
    pub callsign: String,
    pub enabled: bool,
    pub credential_reference_present: bool,
    pub credential_resolved: bool,
    #[serde(skip_serializing, skip_deserializing)]
    pub credential_secret: Option<String>,
    pub mode: ProviderAdapterMode,
    pub fake_response: Option<String>,
    pub force_fake_not_found: bool,
    pub force_fake_auth_failure: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderLookupExecution {
    pub provider_id: String,
    pub status: ProviderRuntimeStatus,
    pub result: Option<LookupResult>,
    pub result_summary: String,
    pub failure_reason: Option<String>,
    pub error_code: Option<String>,
    pub redacted_error: Option<String>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderSpotInput {
    pub provider_id: String,
    pub enabled: bool,
    pub mode: ProviderAdapterMode,
    pub fake_response: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderSpotExecution {
    pub provider_id: String,
    pub status: ProviderRuntimeStatus,
    pub spots: Vec<Spot>,
    pub result_summary: String,
    pub failure_reason: Option<String>,
    pub error_code: Option<String>,
    pub redacted_error: Option<String>,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderDxClusterInput {
    pub enabled: bool,
    pub mode: ProviderAdapterMode,
    pub config: DxClusterClientConfig,
    pub fake_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderOutcomeKind {
    Success,
    PartialSuccess,
    RetryableFailure,
    PermanentFailure,
    AuthenticationRequired,
    AuthenticationRejected,
    AuthorizationDenied,
    RateLimited,
    ProviderUnavailable,
    MalformedProviderResponse,
    InvalidLocalConfiguration,
    Timeout,
    TransportFailure,
    Cancelled,
    UncertainResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderRetryClass {
    None,
    Retryable,
    RetryAfter,
    UserActionRequired,
    UnknownSafety,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderOutcome {
    pub provider_id: String,
    pub kind: ProviderOutcomeKind,
    pub code: String,
    pub user_message: String,
    pub retry_class: ProviderRetryClass,
    pub retry_after_seconds: Option<u64>,
    pub correlation_id: Uuid,
    pub request_id: Option<String>,
    pub queue_item_id: Option<Uuid>,
    pub redacted_diagnostics: Vec<String>,
}

impl ProviderOutcome {
    pub fn new(
        provider_id: impl Into<String>,
        kind: ProviderOutcomeKind,
        code: impl Into<String>,
        user_message: impl Into<String>,
    ) -> Self {
        Self {
            provider_id: provider_id.into(),
            kind,
            code: code.into(),
            user_message: user_message.into(),
            retry_class: retry_class_for_outcome(kind),
            retry_after_seconds: None,
            correlation_id: Uuid::new_v4(),
            request_id: None,
            queue_item_id: None,
            redacted_diagnostics: Vec::new(),
        }
    }
}

pub fn retry_class_for_outcome(kind: ProviderOutcomeKind) -> ProviderRetryClass {
    match kind {
        ProviderOutcomeKind::RetryableFailure
        | ProviderOutcomeKind::ProviderUnavailable
        | ProviderOutcomeKind::Timeout
        | ProviderOutcomeKind::TransportFailure => ProviderRetryClass::Retryable,
        ProviderOutcomeKind::RateLimited => ProviderRetryClass::RetryAfter,
        ProviderOutcomeKind::AuthenticationRequired
        | ProviderOutcomeKind::AuthenticationRejected
        | ProviderOutcomeKind::AuthorizationDenied
        | ProviderOutcomeKind::InvalidLocalConfiguration => ProviderRetryClass::UserActionRequired,
        ProviderOutcomeKind::UncertainResult => ProviderRetryClass::UnknownSafety,
        ProviderOutcomeKind::Success
        | ProviderOutcomeKind::PartialSuccess
        | ProviderOutcomeKind::PermanentFailure
        | ProviderOutcomeKind::MalformedProviderResponse
        | ProviderOutcomeKind::Cancelled => ProviderRetryClass::None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRuntimeEvent {
    pub provider_id: String,
    pub capability: String,
    pub operation: String,
    pub correlation_id: Uuid,
    pub duration_ms: u64,
    pub outcome: ProviderOutcomeKind,
    pub attempt: u32,
    pub http_status: Option<u16>,
    pub queue_item_id: Option<Uuid>,
    pub retry_at: Option<DateTime<Utc>>,
    pub rate_limit_state: Option<ProviderRateLimitSnapshot>,
    pub circuit_state: Option<CircuitBreakerState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHttpRuntimeConfig {
    pub connect_timeout_seconds: u64,
    pub request_timeout_seconds: u64,
    pub total_deadline_seconds: u64,
    pub max_response_body_bytes: usize,
    pub user_agent: String,
    pub tls_verify: bool,
    pub max_redirects: usize,
    pub accept_compression: bool,
    pub correlation_id: Uuid,
}

impl Default for ProviderHttpRuntimeConfig {
    fn default() -> Self {
        Self {
            connect_timeout_seconds: 5,
            request_timeout_seconds: DEFAULT_PROVIDER_TIMEOUT_SECONDS,
            total_deadline_seconds: DEFAULT_PROVIDER_TIMEOUT_SECONDS + 5,
            max_response_body_bytes: 512 * 1024,
            user_agent: PROVIDER_USER_AGENT.to_owned(),
            tls_verify: true,
            max_redirects: 5,
            accept_compression: true,
            correlation_id: Uuid::new_v4(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHttpTiming {
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHttpRuntimeResult {
    pub response: ProviderHttpResponse,
    pub timing: ProviderHttpTiming,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRateLimitPolicy {
    pub provider_id: String,
    pub max_concurrent_global: u32,
    pub max_concurrent_per_account: u32,
    pub burst_limit: u32,
    pub refill_interval_seconds: u64,
    pub queue_limit: usize,
}

impl ProviderRateLimitPolicy {
    pub fn for_provider(provider_id: impl Into<String>) -> Self {
        Self {
            provider_id: provider_id.into(),
            max_concurrent_global: 4,
            max_concurrent_per_account: 1,
            burst_limit: 10,
            refill_interval_seconds: 60,
            queue_limit: 1000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRateLimitSnapshot {
    pub provider_id: String,
    pub account_scope: Option<String>,
    pub available_burst: u32,
    pub running_global: u32,
    pub running_for_account: u32,
    pub next_allowed_at: Option<DateTime<Utc>>,
    pub queue_depth: usize,
    pub overflowed: bool,
    pub instance_local: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderRateLimiter {
    pub policy: ProviderRateLimitPolicy,
    pub available_burst: u32,
    pub running_global: u32,
    pub running_by_account: HashMap<String, u32>,
    pub next_refill_at: DateTime<Utc>,
    pub queue_depth: usize,
}

impl ProviderRateLimiter {
    pub fn new(policy: ProviderRateLimitPolicy, now: DateTime<Utc>) -> Self {
        Self {
            available_burst: policy.burst_limit,
            next_refill_at: now + Duration::seconds(policy.refill_interval_seconds as i64),
            policy,
            running_global: 0,
            running_by_account: HashMap::new(),
            queue_depth: 0,
        }
    }

    pub fn try_acquire(
        &mut self,
        account_scope: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ProviderRateLimitSnapshot, ProviderRateLimitSnapshot> {
        self.refill(now);
        let account_key = account_scope.unwrap_or("__anonymous__").to_owned();
        let running_for_account = *self.running_by_account.get(&account_key).unwrap_or(&0);
        let limited = self.running_global >= self.policy.max_concurrent_global
            || running_for_account >= self.policy.max_concurrent_per_account
            || self.available_burst == 0;
        let overflowed = self.queue_depth >= self.policy.queue_limit;
        if limited || overflowed {
            return Err(self.snapshot(account_scope.map(str::to_owned), overflowed));
        }
        self.running_global += 1;
        *self.running_by_account.entry(account_key).or_default() += 1;
        self.available_burst = self.available_burst.saturating_sub(1);
        Ok(self.snapshot(account_scope.map(str::to_owned), false))
    }

    pub fn release(&mut self, account_scope: Option<&str>) {
        self.running_global = self.running_global.saturating_sub(1);
        let account_key = account_scope.unwrap_or("__anonymous__").to_owned();
        if let Some(running) = self.running_by_account.get_mut(&account_key) {
            *running = running.saturating_sub(1);
        }
    }

    pub fn snapshot(
        &self,
        account_scope: Option<String>,
        overflowed: bool,
    ) -> ProviderRateLimitSnapshot {
        let running_for_account = account_scope
            .as_deref()
            .and_then(|account_key| self.running_by_account.get(account_key))
            .copied()
            .unwrap_or_else(|| *self.running_by_account.get("__anonymous__").unwrap_or(&0));
        ProviderRateLimitSnapshot {
            provider_id: self.policy.provider_id.clone(),
            account_scope,
            available_burst: self.available_burst,
            running_global: self.running_global,
            running_for_account,
            next_allowed_at: (self.available_burst == 0).then_some(self.next_refill_at),
            queue_depth: self.queue_depth,
            overflowed,
            instance_local: true,
        }
    }

    fn refill(&mut self, now: DateTime<Utc>) {
        if now >= self.next_refill_at {
            self.available_burst = self.policy.burst_limit;
            self.next_refill_at =
                now + Duration::seconds(self.policy.refill_interval_seconds as i64);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCircuitBreaker {
    pub state: CircuitBreakerState,
    pub failure_threshold: u32,
    pub consecutive_failures: u32,
    pub cooldown_seconds: u64,
    pub opened_at: Option<DateTime<Utc>>,
    pub half_open_probe_limit: u32,
    pub half_open_in_flight: u32,
}

impl ProviderCircuitBreaker {
    pub fn new(failure_threshold: u32, cooldown_seconds: u64, half_open_probe_limit: u32) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_threshold,
            consecutive_failures: 0,
            cooldown_seconds,
            opened_at: None,
            half_open_probe_limit,
            half_open_in_flight: 0,
        }
    }

    pub fn allow_request(&mut self, now: DateTime<Utc>) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                let ready = self.opened_at.is_some_and(|opened| {
                    now >= opened + Duration::seconds(self.cooldown_seconds as i64)
                });
                if ready {
                    self.state = CircuitBreakerState::HalfOpen;
                    self.half_open_in_flight = 0;
                    self.allow_request(now)
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => {
                if self.half_open_in_flight < self.half_open_probe_limit {
                    self.half_open_in_flight += 1;
                    true
                } else {
                    false
                }
            }
        }
    }

    pub fn record_success(&mut self) {
        self.state = CircuitBreakerState::Closed;
        self.consecutive_failures = 0;
        self.opened_at = None;
        self.half_open_in_flight = 0;
    }

    pub fn record_outcome(&mut self, kind: ProviderOutcomeKind, now: DateTime<Utc>) {
        if !opens_circuit(kind) {
            if matches!(
                kind,
                ProviderOutcomeKind::Success | ProviderOutcomeKind::PartialSuccess
            ) {
                self.record_success();
            }
            return;
        }
        self.consecutive_failures += 1;
        if self.state == CircuitBreakerState::HalfOpen
            || self.consecutive_failures >= self.failure_threshold
        {
            self.state = CircuitBreakerState::Open;
            self.opened_at = Some(now);
            self.half_open_in_flight = 0;
        }
    }
}

impl Default for ProviderCircuitBreaker {
    fn default() -> Self {
        Self::new(3, 300, 1)
    }
}

fn opens_circuit(kind: ProviderOutcomeKind) -> bool {
    matches!(
        kind,
        ProviderOutcomeKind::RetryableFailure
            | ProviderOutcomeKind::ProviderUnavailable
            | ProviderOutcomeKind::Timeout
            | ProviderOutcomeKind::TransportFailure
            | ProviderOutcomeKind::MalformedProviderResponse
    )
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProviderAdapterError {
    #[error("provider {0} is not registered")]
    UnknownProvider(String),
    #[error("provider {provider_id} does not support capability {capability}")]
    UnsupportedCapability {
        provider_id: String,
        capability: String,
    },
}

const PROVIDER_USER_AGENT: &str = "KE8YGW-logger/0.2 provider-transport";
const DEFAULT_PROVIDER_TIMEOUT_SECONDS: u64 = 20;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHttpRequest {
    pub method: String,
    pub url: String,
    pub content_type: Option<String>,
    pub body: Option<String>,
    pub timeout_seconds: u64,
    pub user_agent: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderHttpResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum ProviderHttpError {
    #[error("provider HTTP request timed out")]
    Timeout,
    #[error("provider HTTP status {status}: {body}")]
    HttpStatus { status: u16, body: String },
    #[error("provider HTTP response exceeded {limit_bytes} bytes")]
    ResponseTooLarge { limit_bytes: usize },
    #[error("provider HTTP transport failed: {0}")]
    Transport(String),
    #[error("provider operation is unsupported: {0}")]
    Unsupported(String),
}

pub fn provider_http_config_for_request(
    request: &ProviderHttpRequest,
) -> ProviderHttpRuntimeConfig {
    ProviderHttpRuntimeConfig {
        request_timeout_seconds: request.timeout_seconds,
        total_deadline_seconds: request.timeout_seconds.saturating_add(5),
        user_agent: request.user_agent.clone(),
        ..ProviderHttpRuntimeConfig::default()
    }
}

pub fn retryable_http_status(status: u16) -> bool {
    matches!(status, 408 | 429 | 502 | 503 | 504)
}

pub fn classify_provider_http_error(error: &ProviderHttpError) -> ProviderOutcomeKind {
    match error {
        ProviderHttpError::Timeout => ProviderOutcomeKind::Timeout,
        ProviderHttpError::ResponseTooLarge { .. } => {
            ProviderOutcomeKind::MalformedProviderResponse
        }
        ProviderHttpError::Transport(message) => {
            let lower = message.to_ascii_lowercase();
            if lower.contains("dns")
                || lower.contains("reset")
                || lower.contains("temporar")
                || lower.contains("connection refused")
            {
                ProviderOutcomeKind::TransportFailure
            } else {
                ProviderOutcomeKind::PermanentFailure
            }
        }
        ProviderHttpError::HttpStatus { status, .. } if *status == 429 => {
            ProviderOutcomeKind::RateLimited
        }
        ProviderHttpError::HttpStatus { status, .. } if retryable_http_status(*status) => {
            ProviderOutcomeKind::ProviderUnavailable
        }
        ProviderHttpError::HttpStatus {
            status: 401 | 403, ..
        } => ProviderOutcomeKind::AuthenticationRejected,
        ProviderHttpError::HttpStatus {
            status: 400..=499, ..
        } => ProviderOutcomeKind::PermanentFailure,
        ProviderHttpError::HttpStatus { .. } => ProviderOutcomeKind::RetryableFailure,
        ProviderHttpError::Unsupported(_) => ProviderOutcomeKind::InvalidLocalConfiguration,
    }
}

pub fn provider_retry_after_seconds(value: &str, max_seconds: u64) -> Option<u64> {
    let seconds = value.trim().parse::<u64>().ok()?;
    Some(seconds.min(max_seconds))
}

pub fn next_retry_delay_with_jitter(
    policy: &RetryPolicy,
    attempt: u8,
    jitter_ratio: f32,
    jitter_seed: u64,
) -> Duration {
    let base = next_retry_delay(policy, attempt);
    if jitter_ratio <= 0.0 {
        return base;
    }
    let base_seconds = base.num_seconds().max(0) as u64;
    let jitter_cap = ((base_seconds as f32) * jitter_ratio).round() as u64;
    if jitter_cap == 0 {
        return base;
    }
    let jitter = jitter_seed % (jitter_cap + 1);
    Duration::seconds(base_seconds.saturating_add(jitter) as i64)
}

pub fn redact_provider_text(value: &str, secrets: &[String]) -> String {
    let mut redacted = value.to_owned();
    for secret in secrets {
        if !secret.is_empty() {
            redacted = redacted.replace(secret, "[REDACTED]");
        }
    }
    redacted
}

pub fn provider_form_body(fields: &[(&str, &str)]) -> String {
    fields
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                urlencoding::encode(key),
                urlencoding::encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

pub fn fake_provider_http_response(status: u16, body: impl Into<String>) -> ProviderHttpResponse {
    ProviderHttpResponse {
        status,
        body: body.into(),
    }
}

pub fn map_provider_http_status(
    response: ProviderHttpResponse,
) -> Result<ProviderHttpResponse, ProviderHttpError> {
    if (200..300).contains(&response.status) {
        Ok(response)
    } else {
        Err(ProviderHttpError::HttpStatus {
            status: response.status,
            body: response.body,
        })
    }
}

pub fn provider_http_error_from_transport(message: &str) -> ProviderHttpError {
    let lower = message.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        ProviderHttpError::Timeout
    } else {
        ProviderHttpError::Transport(message.to_owned())
    }
}

pub fn send_provider_http_request(
    request: &ProviderHttpRequest,
) -> Result<ProviderHttpResponse, ProviderHttpError> {
    send_provider_http_request_with_config(request, &provider_http_config_for_request(request))
        .map(|result| result.response)
}

pub fn send_provider_http_request_with_config(
    request: &ProviderHttpRequest,
    config: &ProviderHttpRuntimeConfig,
) -> Result<ProviderHttpRuntimeResult, ProviderHttpError> {
    if !config.tls_verify {
        return Err(ProviderHttpError::Unsupported(
            "disabling TLS verification is not supported in this build".to_owned(),
        ));
    }
    let started_at = Utc::now();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(StdDuration::from_secs(config.connect_timeout_seconds))
        .timeout_read(StdDuration::from_secs(config.request_timeout_seconds))
        .timeout_write(StdDuration::from_secs(config.request_timeout_seconds))
        .redirects(config.max_redirects.min(u32::MAX as usize) as u32)
        .user_agent(&config.user_agent)
        .build();
    let result = match request.method.as_str() {
        "GET" => agent
            .get(&request.url)
            .set("X-Correlation-ID", &config.correlation_id.to_string())
            .call(),
        "POST" => {
            let mut req = agent
                .post(&request.url)
                .set("X-Correlation-ID", &config.correlation_id.to_string());
            if let Some(content_type) = &request.content_type {
                req = req.set("Content-Type", content_type);
            }
            req.send_string(request.body.as_deref().unwrap_or_default())
        }
        other => {
            return Err(ProviderHttpError::Unsupported(format!(
                "HTTP method {other} is not supported"
            )))
        }
    };
    let finish = |response: ProviderHttpResponse| {
        let completed_at = Utc::now();
        let duration_ms = completed_at
            .signed_duration_since(started_at)
            .num_milliseconds()
            .max(0) as u64;
        ProviderHttpRuntimeResult {
            response,
            timing: ProviderHttpTiming {
                started_at,
                completed_at,
                duration_ms,
            },
            correlation_id: config.correlation_id,
        }
    };
    match result {
        Ok(response) => {
            let status = response.status();
            let body = read_bounded_provider_body(response, config.max_response_body_bytes)?;
            Ok(finish(ProviderHttpResponse { status, body }))
        }
        Err(ureq::Error::Status(status, response)) => {
            let body = read_bounded_provider_body(response, config.max_response_body_bytes)?;
            Err(ProviderHttpError::HttpStatus { status, body })
        }
        Err(ureq::Error::Transport(error)) => {
            Err(provider_http_error_from_transport(&error.to_string()))
        }
    }
}

fn read_bounded_provider_body(
    response: ureq::Response,
    limit_bytes: usize,
) -> Result<String, ProviderHttpError> {
    let mut reader = response.into_reader().take(limit_bytes as u64 + 1);
    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .map_err(|error| ProviderHttpError::Transport(error.to_string()))?;
    if bytes.len() > limit_bytes {
        return Err(ProviderHttpError::ResponseTooLarge { limit_bytes });
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn credential_fields(secret: Option<&str>) -> HashMap<String, String> {
    let Some(secret) = secret else {
        return HashMap::new();
    };
    if let Ok(value) = serde_json::from_str::<HashMap<String, String>>(secret) {
        return value
            .into_iter()
            .map(|(key, value)| (key.to_ascii_lowercase(), value))
            .collect();
    }
    secret
        .split(['\n', ';'])
        .filter_map(|part| {
            let (key, value) = part.split_once('=')?;
            Some((key.trim().to_ascii_lowercase(), value.trim().to_owned()))
        })
        .collect()
}

fn required_field(fields: &HashMap<String, String>, names: &[&str]) -> Result<String, String> {
    names
        .iter()
        .find_map(|name| fields.get(&name.to_ascii_lowercase()).cloned())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("credential secret is missing {}", names.join("/")))
}

pub fn tier_one_provider_metadata(provider_id: &str) -> Option<ServiceProviderMetadata> {
    online_provider_metadata()
        .into_iter()
        .find(|provider| provider.provider_id == provider_id)
}

pub fn tier_one_provider_supports_capability(
    provider_id: &str,
    capability: &str,
) -> Result<bool, ProviderAdapterError> {
    let provider = tier_one_provider_metadata(provider_id)
        .ok_or_else(|| ProviderAdapterError::UnknownProvider(provider_id.to_owned()))?;
    Ok(provider
        .capabilities
        .iter()
        .any(|value| value == capability))
}

pub fn test_tier_one_provider(
    input: ProviderAdapterTestInput,
) -> Result<ProviderAdapterTestResult, ProviderAdapterError> {
    let provider = tier_one_provider_metadata(&input.provider_id)
        .ok_or_else(|| ProviderAdapterError::UnknownProvider(input.provider_id.clone()))?;
    if let Some(capability) = input.capability.clone() {
        if !provider
            .capabilities
            .iter()
            .any(|value| value == &capability)
        {
            return Ok(provider_test_result(
                input,
                provider_requires_credentials(&provider),
                "unsupported_capability",
                ProviderHealthState::Unavailable,
                vec![format!("provider does not support capability {capability}")],
                "select a provider that supports the requested capability",
            ));
        }
    }
    if !input.enabled {
        return Ok(provider_test_result(
            input,
            provider_requires_credentials(&provider),
            "disabled",
            ProviderHealthState::Unavailable,
            vec!["provider is disabled for this logbook".to_owned()],
            "enable the provider before running live operations",
        ));
    }
    let credential_required = provider_requires_credentials(&provider);
    if input.mode == ProviderAdapterMode::Live && credential_required {
        if !input.credential_reference_present {
            return Ok(provider_test_result(
                input,
                credential_required,
                "missing_credential",
                ProviderHealthState::MissingConfig,
                vec!["credential reference is required".to_owned()],
                "store a credential in the credential backend and attach its credential_id",
            ));
        }
        if !input.credential_resolved {
            return Ok(provider_test_result(
                input,
                credential_required,
                "invalid_credential_reference",
                ProviderHealthState::MissingConfig,
                vec![
                    "credential reference did not resolve through the configured backend"
                        .to_owned(),
                ],
                "replace the provider credential_id with an active credential reference",
            ));
        }
    }
    if input.mode == ProviderAdapterMode::Fake {
        return Ok(provider_test_result(
            input,
            credential_required,
            "ok",
            ProviderHealthState::Healthy,
            vec!["fake provider test succeeded without network access".to_owned()],
            "safe for CI; set live_test=true only for release-runner validation",
        ));
    }
    if matches!(
        provider.provider_id.as_str(),
        "clublog" | "qrz-logbook" | "eqsl" | "qrz-xml" | "hamqth" | "pota-spots" | "dx-cluster"
    ) {
        let diagnostics = match provider.provider_id.as_str() {
            "clublog" => vec![
                "live transport modeled from Club Log real-time upload API".to_owned(),
                "network execution is gated by live_test and credential resolution".to_owned(),
            ],
            "qrz-logbook" => vec![
                "live transport modeled from QRZ Logbook API ACTION=INSERT".to_owned(),
                "network execution is gated by live_test and credential resolution".to_owned(),
            ],
            "eqsl" => vec![
                "live transport modeled from eQSL logger ADIF upload interface".to_owned(),
                "network execution is gated by live_test and credential resolution".to_owned(),
            ],
            "qrz-xml" => vec![
                "QRZ XML session and callsign response parsers are available".to_owned(),
                "hosted lookup execution is wired and remains fake by default unless live_test=true"
                    .to_owned(),
            ],
            "hamqth" => vec![
                "HamQTH session and callsign response parsers are available".to_owned(),
                "hosted lookup execution is wired and remains fake by default unless live_test=true"
                    .to_owned(),
            ],
            "pota-spots" => vec![
                "POTA spot JSON parser and live endpoint request builder are available".to_owned(),
                "hosted spot fetch execution is wired and remains fake by default unless live_test=true".to_owned(),
            ],
            "dx-cluster" => vec![
                "DX Cluster telnet read-once client foundation is available".to_owned(),
                "no always-on daemon is started by provider tests".to_owned(),
            ],
            _ => vec![],
        };
        return Ok(provider_test_result(
            input,
            credential_required,
            "live_transport_ready",
            ProviderHealthState::Healthy,
            diagnostics,
            "run explicit gated live validation with provider credentials before production use",
        ));
    }
    Ok(provider_test_result(
        input,
        credential_required,
        "live_transport_not_configured",
        ProviderHealthState::Unavailable,
        vec![live_limitation_for_provider(&provider.provider_id)],
        "run fake tests by default; complete provider-specific live transport before release use",
    ))
}

pub fn execute_tier_one_upload(
    input: ProviderUploadInput,
) -> Result<ProviderUploadExecution, ProviderAdapterError> {
    let provider = tier_one_provider_metadata(&input.provider_id)
        .ok_or_else(|| ProviderAdapterError::UnknownProvider(input.provider_id.clone()))?;
    if provider.service_type != ServiceType::LogUpload
        || !provider
            .capabilities
            .iter()
            .any(|capability| capability == CAP_UPLOAD_ADIF)
    {
        return Err(ProviderAdapterError::UnsupportedCapability {
            provider_id: input.provider_id,
            capability: CAP_UPLOAD_ADIF.to_owned(),
        });
    }
    if !input.enabled {
        return Ok(ProviderUploadExecution {
            provider_id: provider.provider_id,
            status: UploadJobStatus::Failed,
            accepted_count: 0,
            rejected_count: 0,
            provider_correlation_id: None,
            result_summary: "provider is disabled".to_owned(),
            failure_reason: Some("provider disabled".to_owned()),
            redacted_error: Some("provider is disabled for this logbook".to_owned()),
            retryable: false,
        });
    }
    let credential_required = provider_requires_credentials(&provider);
    if input.force_fake_failure {
        return Ok(ProviderUploadExecution {
            provider_id: provider.provider_id,
            status: UploadJobStatus::Failed,
            accepted_count: 0,
            rejected_count: input.qso_count,
            provider_correlation_id: Some(format!("fake-{}", input.job_id)),
            result_summary: "forced fake provider failure".to_owned(),
            failure_reason: Some("forced fake provider failure".to_owned()),
            redacted_error: Some("redacted fake provider failure".to_owned()),
            retryable: true,
        });
    }
    if input.mode == ProviderAdapterMode::Fake {
        return Ok(ProviderUploadExecution {
            provider_id: provider.provider_id,
            status: UploadJobStatus::Succeeded,
            accepted_count: input.qso_count,
            rejected_count: 0,
            provider_correlation_id: Some(format!("fake-{}", input.job_id)),
            result_summary: format!("fake upload accepted {} QSO(s)", input.qso_count),
            failure_reason: None,
            redacted_error: None,
            retryable: false,
        });
    }
    if credential_required && !input.credential_reference_present {
        return Ok(ProviderUploadExecution {
            provider_id: provider.provider_id,
            status: UploadJobStatus::NeedsCredentials,
            accepted_count: 0,
            rejected_count: input.qso_count,
            provider_correlation_id: None,
            result_summary: "credential reference is required".to_owned(),
            failure_reason: Some("missing credential reference".to_owned()),
            redacted_error: Some("credential reference is required".to_owned()),
            retryable: true,
        });
    }
    if credential_required && !input.credential_resolved {
        return Ok(ProviderUploadExecution {
            provider_id: provider.provider_id,
            status: UploadJobStatus::NeedsCredentials,
            accepted_count: 0,
            rejected_count: input.qso_count,
            provider_correlation_id: None,
            result_summary: "credential reference did not resolve".to_owned(),
            failure_reason: Some("invalid credential reference".to_owned()),
            redacted_error: Some(
                "credential reference did not resolve through the configured backend".to_owned(),
            ),
            retryable: true,
        });
    }
    if input.mode == ProviderAdapterMode::Live {
        let provider_id = provider.provider_id.clone();
        let execution = match provider_id.as_str() {
            "clublog" => execute_clublog_upload(&input),
            "qrz-logbook" => execute_qrz_logbook_upload(&input),
            "eqsl" => execute_eqsl_upload(&input),
            "lotw" => Ok(ProviderUploadExecution {
                provider_id: provider_id.clone(),
                status: UploadJobStatus::Failed,
                accepted_count: 0,
                rejected_count: input.qso_count,
                provider_correlation_id: None,
                result_summary: live_limitation_for_provider(&provider_id),
                failure_reason: Some("tqsl signing flow not modeled".to_owned()),
                redacted_error: Some(live_limitation_for_provider(&provider_id)),
                retryable: false,
            }),
            _ => Err(ProviderHttpError::Unsupported(
                live_limitation_for_provider(&provider_id),
            )),
        };
        return Ok(match execution {
            Ok(execution) => execution,
            Err(error) => ProviderUploadExecution {
                provider_id,
                status: UploadJobStatus::Failed,
                accepted_count: 0,
                rejected_count: input.qso_count,
                provider_correlation_id: None,
                result_summary: "provider live upload failed".to_owned(),
                failure_reason: Some("provider live upload failed".to_owned()),
                redacted_error: Some(error.to_string()),
                retryable: matches!(
                    error,
                    ProviderHttpError::Timeout
                        | ProviderHttpError::Transport(_)
                        | ProviderHttpError::HttpStatus {
                            status: 500..=599,
                            ..
                        }
                ),
            },
        });
    }
    Ok(ProviderUploadExecution {
        provider_id: provider.provider_id.clone(),
        status: UploadJobStatus::Failed,
        accepted_count: 0,
        rejected_count: input.qso_count,
        provider_correlation_id: None,
        result_summary: live_limitation_for_provider(&provider.provider_id),
        failure_reason: Some("live provider transport not configured".to_owned()),
        redacted_error: Some(live_limitation_for_provider(&provider.provider_id)),
        retryable: false,
    })
}

fn execute_clublog_upload(
    input: &ProviderUploadInput,
) -> Result<ProviderUploadExecution, ProviderHttpError> {
    let fields = credential_fields(input.credential_secret.as_deref());
    let email = required_field(&fields, &["email"]).map_err(ProviderHttpError::Unsupported)?;
    let password = required_field(&fields, &["password", "app_password"])
        .map_err(ProviderHttpError::Unsupported)?;
    let callsign =
        required_field(&fields, &["callsign"]).map_err(ProviderHttpError::Unsupported)?;
    let api =
        required_field(&fields, &["api", "api_key"]).map_err(ProviderHttpError::Unsupported)?;
    let body = provider_form_body(&[
        ("email", &email),
        ("password", &password),
        ("callsign", &callsign),
        ("api", &api),
        ("adif", &input.adif_payload),
    ]);
    let response = send_provider_http_request(&ProviderHttpRequest {
        method: "POST".to_owned(),
        url: "https://clublog.org/realtime.php".to_owned(),
        content_type: Some("application/x-www-form-urlencoded".to_owned()),
        body: Some(body),
        timeout_seconds: DEFAULT_PROVIDER_TIMEOUT_SECONDS,
        user_agent: PROVIDER_USER_AGENT.to_owned(),
    });
    parse_clublog_upload_response(
        input,
        response.map_err(|error| redact_http_error(error, &[email, password, api]))?,
    )
}

fn execute_qrz_logbook_upload(
    input: &ProviderUploadInput,
) -> Result<ProviderUploadExecution, ProviderHttpError> {
    let fields = credential_fields(input.credential_secret.as_deref());
    let key =
        required_field(&fields, &["key", "api_key"]).map_err(ProviderHttpError::Unsupported)?;
    let body = provider_form_body(&[
        ("KEY", &key),
        ("ACTION", "INSERT"),
        ("ADIF", &input.adif_payload),
    ]);
    let response = send_provider_http_request(&ProviderHttpRequest {
        method: "POST".to_owned(),
        url: "https://logbook.qrz.com/api".to_owned(),
        content_type: Some("application/x-www-form-urlencoded".to_owned()),
        body: Some(body),
        timeout_seconds: DEFAULT_PROVIDER_TIMEOUT_SECONDS,
        user_agent: PROVIDER_USER_AGENT.to_owned(),
    });
    parse_qrz_logbook_upload_response(
        input,
        response.map_err(|error| redact_http_error(error, &[key]))?,
    )
}

fn execute_eqsl_upload(
    input: &ProviderUploadInput,
) -> Result<ProviderUploadExecution, ProviderHttpError> {
    let fields = credential_fields(input.credential_secret.as_deref());
    let username = required_field(&fields, &["username", "callsign"])
        .map_err(ProviderHttpError::Unsupported)?;
    let password =
        required_field(&fields, &["password"]).map_err(ProviderHttpError::Unsupported)?;
    let mut pairs = vec![
        ("UserName", username.as_str()),
        ("Password", password.as_str()),
        ("ADIFData", input.adif_payload.as_str()),
    ];
    if let Some(qth) = fields
        .get("qthnickname")
        .or_else(|| fields.get("qth_nickname"))
    {
        pairs.push(("QTHNickname", qth.as_str()));
    }
    let body = provider_form_body(&pairs);
    let response = send_provider_http_request(&ProviderHttpRequest {
        method: "POST".to_owned(),
        url: "https://www.eqsl.cc/qslcard/ImportADIF.cfm".to_owned(),
        content_type: Some("application/x-www-form-urlencoded".to_owned()),
        body: Some(body),
        timeout_seconds: DEFAULT_PROVIDER_TIMEOUT_SECONDS,
        user_agent: PROVIDER_USER_AGENT.to_owned(),
    });
    parse_eqsl_upload_response(
        input,
        response.map_err(|error| redact_http_error(error, &[username, password]))?,
    )
}

fn redact_http_error(error: ProviderHttpError, secrets: &[String]) -> ProviderHttpError {
    match error {
        ProviderHttpError::HttpStatus { status, body } => ProviderHttpError::HttpStatus {
            status,
            body: redact_provider_text(&body, secrets),
        },
        ProviderHttpError::Transport(message) => {
            ProviderHttpError::Transport(redact_provider_text(&message, secrets))
        }
        other => other,
    }
}

pub fn parse_clublog_upload_response(
    input: &ProviderUploadInput,
    response: ProviderHttpResponse,
) -> Result<ProviderUploadExecution, ProviderHttpError> {
    let body = response.body.trim();
    if response.status == 403 {
        return Ok(provider_upload_failure(
            &input.provider_id,
            input.qso_count,
            "Club Log authentication failed",
            body,
            false,
        ));
    }
    if response.status == 400 {
        return Ok(provider_upload_failure(
            &input.provider_id,
            input.qso_count,
            "Club Log rejected the QSO",
            body,
            false,
        ));
    }
    if response.status >= 500 {
        return Ok(provider_upload_failure(
            &input.provider_id,
            input.qso_count,
            "Club Log temporary server failure",
            body,
            true,
        ));
    }
    let lower = body.to_ascii_lowercase();
    if response.status == 200
        && (lower.contains("qso ok")
            || lower.contains("qso duplicate")
            || lower.contains("qso modified")
            || lower.contains("accepted"))
    {
        return Ok(provider_upload_success(
            &input.provider_id,
            input.qso_count,
            body,
        ));
    }
    Ok(provider_upload_failure(
        &input.provider_id,
        input.qso_count,
        "Club Log returned an unrecognized response",
        body,
        false,
    ))
}

pub fn parse_qrz_logbook_upload_response(
    input: &ProviderUploadInput,
    response: ProviderHttpResponse,
) -> Result<ProviderUploadExecution, ProviderHttpError> {
    let body = response.body.trim();
    let lower = body.to_ascii_lowercase();
    if response.status >= 500 {
        return Ok(provider_upload_failure(
            &input.provider_id,
            input.qso_count,
            "QRZ Logbook temporary server failure",
            body,
            true,
        ));
    }
    if response.status == 401 || lower.contains("auth") || lower.contains("not authorized") {
        return Ok(provider_upload_failure(
            &input.provider_id,
            input.qso_count,
            "QRZ Logbook authentication failed",
            body,
            false,
        ));
    }
    if response.status == 200 && (lower.contains("result=ok") || lower.contains("<result>ok")) {
        return Ok(provider_upload_success(
            &input.provider_id,
            input.qso_count,
            body,
        ));
    }
    Ok(provider_upload_failure(
        &input.provider_id,
        input.qso_count,
        "QRZ Logbook rejected the upload",
        body,
        false,
    ))
}

pub fn parse_eqsl_upload_response(
    input: &ProviderUploadInput,
    response: ProviderHttpResponse,
) -> Result<ProviderUploadExecution, ProviderHttpError> {
    let body = response.body.trim();
    let lower = body.to_ascii_lowercase();
    if response.status >= 500 {
        return Ok(provider_upload_failure(
            &input.provider_id,
            input.qso_count,
            "eQSL temporary server failure",
            body,
            true,
        ));
    }
    if lower.contains("password") && (lower.contains("invalid") || lower.contains("incorrect")) {
        return Ok(provider_upload_failure(
            &input.provider_id,
            input.qso_count,
            "eQSL authentication failed",
            body,
            false,
        ));
    }
    if response.status == 200
        && (lower.contains("records added")
            || lower.contains("record added")
            || lower.contains("success")
            || lower.contains("accepted"))
    {
        return Ok(provider_upload_success(
            &input.provider_id,
            input.qso_count,
            body,
        ));
    }
    Ok(provider_upload_failure(
        &input.provider_id,
        input.qso_count,
        "eQSL returned an unrecognized response",
        body,
        false,
    ))
}

fn provider_upload_success(
    provider_id: &str,
    qso_count: usize,
    summary: &str,
) -> ProviderUploadExecution {
    ProviderUploadExecution {
        provider_id: provider_id.to_owned(),
        status: UploadJobStatus::Succeeded,
        accepted_count: qso_count,
        rejected_count: 0,
        provider_correlation_id: Some(format!("live-{}", Uuid::new_v4())),
        result_summary: summary.to_owned(),
        failure_reason: None,
        redacted_error: None,
        retryable: false,
    }
}

fn provider_upload_failure(
    provider_id: &str,
    qso_count: usize,
    summary: &str,
    error: &str,
    retryable: bool,
) -> ProviderUploadExecution {
    ProviderUploadExecution {
        provider_id: provider_id.to_owned(),
        status: UploadJobStatus::Failed,
        accepted_count: 0,
        rejected_count: qso_count,
        provider_correlation_id: None,
        result_summary: summary.to_owned(),
        failure_reason: Some(summary.to_owned()),
        redacted_error: Some(error.to_owned()),
        retryable,
    }
}

fn provider_requires_credentials(provider: &ServiceProviderMetadata) -> bool {
    !provider.required_credentials.is_empty() || !provider.required_config_keys.is_empty()
}

fn provider_test_result(
    input: ProviderAdapterTestInput,
    credential_required: bool,
    test_status: &str,
    provider_health_state: ProviderHealthState,
    redacted_diagnostics: Vec<String>,
    next_recommended_action: &str,
) -> ProviderAdapterTestResult {
    ProviderAdapterTestResult {
        provider_id: input.provider_id,
        capability_tested: input.capability,
        credential_required,
        credential_reference_present: input.credential_reference_present,
        credential_resolved: input.credential_resolved,
        test_status: test_status.to_owned(),
        provider_health_state,
        redacted_diagnostics,
        next_recommended_action: next_recommended_action.to_owned(),
        checked_at: Utc::now(),
    }
}

fn live_limitation_for_provider(provider_id: &str) -> String {
    match provider_id {
        "lotw" => "LoTW upload requires a modeled TQSL/certificate signing flow before live upload"
            .to_owned(),
        "dx-cluster" => {
            "DX Cluster live client has parser/scaffold support; persistent telnet runtime is pending"
                .to_owned()
        }
        "pota-spots" | "sotawatch" => {
            "spot feed adapter has fake/scaffold support; POTA live parser is modeled, SOTA live access is deferred pending API approval".to_owned()
        }
        "qrz-xml" | "hamqth" => {
            "callsign lookup adapter has credential/test scaffolding, live response parsers, and hosted fake-default execution"
                .to_owned()
        }
        "clublog" | "qrz-logbook" | "eqsl" => {
            "ADIF upload adapter has fake/test execution and gated live HTTP transport".to_owned()
        }
        _ => "live provider transport is not configured for this build".to_owned(),
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SotaSpotRecord {
    pub activator: String,
    pub summit_reference: String,
    pub frequency_hz: u64,
    pub mode: Option<String>,
    pub spotted_at: DateTime<Utc>,
    pub comments: Option<String>,
}

pub fn sota_spot_to_spot(record: SotaSpotRecord) -> Spot {
    Spot {
        spotted_callsign: record.activator,
        spotter_callsign: None,
        frequency_hz: record.frequency_hz,
        band: None,
        mode: record.mode,
        comment: record.comments,
        source: SpotSource {
            provider_id: "sotawatch".to_owned(),
            label: "SOTAWatch".to_owned(),
        },
        spotted_at: record.spotted_at,
        entity: None,
        grid: None,
        reference: Some(record.summit_reference),
    }
}

pub fn pota_spots_request() -> ProviderHttpRequest {
    ProviderHttpRequest {
        method: "GET".to_owned(),
        url: "https://api.pota.app/spot/activator".to_owned(),
        content_type: None,
        body: None,
        timeout_seconds: DEFAULT_PROVIDER_TIMEOUT_SECONDS,
        user_agent: PROVIDER_USER_AGENT.to_owned(),
    }
}

pub fn sota_spots_request() -> Result<ProviderHttpRequest, ProviderHttpError> {
    Err(ProviderHttpError::Unsupported(
        "SOTA API terms require explicit approval/terms handling before live spot access"
            .to_owned(),
    ))
}

pub fn parse_pota_spots_json(payload: &str) -> Result<Vec<PotaSpotRecord>, String> {
    let value = serde_json::from_str::<Value>(payload).map_err(|error| error.to_string())?;
    let rows = value
        .as_array()
        .ok_or_else(|| "POTA spot payload is not an array".to_owned())?;
    rows.iter()
        .map(|row| {
            let activator = string_field(row, &["activator", "activatorCallsign", "callsign"])
                .ok_or_else(|| "POTA spot missing activator".to_owned())?;
            let reference = string_field(row, &["reference", "referenceId", "park", "parkId"])
                .ok_or_else(|| "POTA spot missing reference".to_owned())?;
            let frequency_hz = frequency_hz_from_value(row, &["frequency", "frequencyKHz", "freq"])
                .ok_or_else(|| "POTA spot missing frequency".to_owned())?;
            let spotted_at = datetime_field(row, &["spotTime", "spottedAt", "timestamp", "time"])
                .unwrap_or_else(Utc::now);
            Ok(PotaSpotRecord {
                activator: activator.to_ascii_uppercase(),
                reference,
                frequency_hz,
                mode: string_field(row, &["mode"]),
                spotted_at,
                comments: string_field(row, &["comments", "comment", "remarks"]),
            })
        })
        .collect()
}

pub fn parse_sota_spots_json(payload: &str) -> Result<Vec<SotaSpotRecord>, String> {
    let value = serde_json::from_str::<Value>(payload).map_err(|error| error.to_string())?;
    let rows = value
        .as_array()
        .ok_or_else(|| "SOTA spot payload is not an array".to_owned())?;
    rows.iter()
        .map(|row| {
            let activator = string_field(row, &["activatorCallsign", "activator", "callsign"])
                .ok_or_else(|| "SOTA spot missing activator".to_owned())?;
            let summit_reference =
                string_field(row, &["summitCode", "summitReference", "reference"])
                    .ok_or_else(|| "SOTA spot missing summit reference".to_owned())?;
            let frequency_hz = frequency_hz_from_value(row, &["frequency", "frequencyMHz", "freq"])
                .ok_or_else(|| "SOTA spot missing frequency".to_owned())?;
            let spotted_at = datetime_field(row, &["timeStamp", "spottedAt", "timestamp", "time"])
                .unwrap_or_else(Utc::now);
            Ok(SotaSpotRecord {
                activator: activator.to_ascii_uppercase(),
                summit_reference,
                frequency_hz,
                mode: string_field(row, &["mode"]),
                spotted_at,
                comments: string_field(row, &["comments", "comment"]),
            })
        })
        .collect()
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    })
}

fn datetime_field(value: &Value, keys: &[&str]) -> Option<DateTime<Utc>> {
    string_field(value, keys)
        .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
        .map(|value| value.with_timezone(&Utc))
}

fn frequency_hz_from_value(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| {
        let raw = value.get(*key)?;
        let number = raw.as_f64().or_else(|| {
            raw.as_str()
                .and_then(|value| value.trim().parse::<f64>().ok())
        })?;
        if number > 1_000_000.0 {
            Some(number.round() as u64)
        } else if number > 1_000.0 {
            Some((number * 1_000.0).round() as u64)
        } else {
            Some((number * 1_000_000.0).round() as u64)
        }
    })
}

pub fn parse_qrz_xml_lookup_response(payload: &str) -> Result<Option<LookupResult>, String> {
    if tag_text(payload, "Error").is_some() {
        return Ok(None);
    }
    let Some(callsign) = tag_text(payload, "call") else {
        return Ok(None);
    };
    let normalized_callsign = normalize_callsign(&callsign).map_err(|error| error.to_string())?;
    let name = [tag_text(payload, "fname"), tag_text(payload, "name")]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_owned();
    Ok(Some(LookupResult {
        callsign,
        normalized_callsign,
        name: (!name.is_empty()).then_some(name),
        qth: tag_text(payload, "addr2"),
        country: tag_text(payload, "country"),
        dxcc: tag_text(payload, "dxcc").and_then(|value| value.parse().ok()),
        cq_zone: tag_text(payload, "cqzone").and_then(|value| value.parse().ok()),
        itu_zone: tag_text(payload, "ituzone").and_then(|value| value.parse().ok()),
        grid: tag_text(payload, "grid"),
        latitude: tag_text(payload, "lat").and_then(|value| value.parse().ok()),
        longitude: tag_text(payload, "lon").and_then(|value| value.parse().ok()),
        license_class: tag_text(payload, "class"),
        previous_callsigns: Vec::new(),
        source_provider: "qrz-xml".to_owned(),
        fetched_at: Utc::now(),
        expires_at: None,
        confidence: 0.95,
        raw_metadata: None,
    }))
}

pub fn parse_hamqth_lookup_response(payload: &str) -> Result<Option<LookupResult>, String> {
    if tag_text(payload, "error").is_some() {
        return Ok(None);
    }
    let Some(callsign) = tag_text(payload, "callsign") else {
        return Ok(None);
    };
    let normalized_callsign = normalize_callsign(&callsign).map_err(|error| error.to_string())?;
    Ok(Some(LookupResult {
        callsign,
        normalized_callsign,
        name: tag_text(payload, "nick").or_else(|| tag_text(payload, "adr_name")),
        qth: tag_text(payload, "qth"),
        country: tag_text(payload, "country"),
        dxcc: tag_text(payload, "dxcc").and_then(|value| value.parse().ok()),
        cq_zone: tag_text(payload, "cq").and_then(|value| value.parse().ok()),
        itu_zone: tag_text(payload, "itu").and_then(|value| value.parse().ok()),
        grid: tag_text(payload, "grid"),
        latitude: tag_text(payload, "latitude").and_then(|value| value.parse().ok()),
        longitude: tag_text(payload, "longitude").and_then(|value| value.parse().ok()),
        license_class: None,
        previous_callsigns: Vec::new(),
        source_provider: "hamqth".to_owned(),
        fetched_at: Utc::now(),
        expires_at: None,
        confidence: 0.9,
        raw_metadata: None,
    }))
}

pub fn execute_tier_one_lookup(
    input: ProviderLookupInput,
) -> Result<ProviderLookupExecution, ProviderAdapterError> {
    let provider = tier_one_provider_metadata(&input.provider_id)
        .ok_or_else(|| ProviderAdapterError::UnknownProvider(input.provider_id.clone()))?;
    if !provider
        .capabilities
        .iter()
        .any(|capability| capability == CAP_LOOKUP_CALLSIGN_BASIC)
    {
        return Err(ProviderAdapterError::UnsupportedCapability {
            provider_id: input.provider_id.clone(),
            capability: CAP_LOOKUP_CALLSIGN_BASIC.to_owned(),
        });
    }
    if !input.enabled {
        return Ok(provider_lookup_failure(
            &input.provider_id,
            ProviderRuntimeStatus::Disabled,
            "provider disabled",
            "enable the provider before running lookup operations",
        ));
    }
    if input.mode == ProviderAdapterMode::Live {
        if !input.credential_reference_present {
            return Ok(provider_lookup_failure(
                &input.provider_id,
                ProviderRuntimeStatus::NeedsCredentials,
                "missing credential reference",
                "store a credential reference before live lookup",
            ));
        }
        if !input.credential_resolved {
            return Ok(provider_lookup_failure(
                &input.provider_id,
                ProviderRuntimeStatus::NeedsCredentials,
                "invalid credential reference",
                "credential reference did not resolve through CredentialStore",
            ));
        }
    }
    match input.provider_id.as_str() {
        "qrz-xml" => execute_qrz_xml_lookup(&input),
        "hamqth" => execute_hamqth_lookup(&input),
        _ => Ok(provider_lookup_failure(
            &input.provider_id,
            ProviderRuntimeStatus::LiveModeNotConfigured,
            "lookup live mode not configured",
            &live_limitation_for_provider(&input.provider_id),
        )),
    }
}

pub fn fetch_tier_one_spots(
    input: ProviderSpotInput,
) -> Result<ProviderSpotExecution, ProviderAdapterError> {
    let provider = tier_one_provider_metadata(&input.provider_id)
        .ok_or_else(|| ProviderAdapterError::UnknownProvider(input.provider_id.clone()))?;
    if !provider
        .capabilities
        .iter()
        .any(|capability| capability == CAP_SPOTTING_POTA)
    {
        return Err(ProviderAdapterError::UnsupportedCapability {
            provider_id: input.provider_id.clone(),
            capability: CAP_SPOTTING_POTA.to_owned(),
        });
    }
    if !input.enabled {
        return Ok(provider_spot_failure(
            &input.provider_id,
            ProviderRuntimeStatus::Disabled,
            "provider disabled",
            "enable the provider before fetching spots",
        ));
    }
    match input.provider_id.as_str() {
        "pota-spots" => execute_pota_spots(input),
        _ => Ok(provider_spot_failure(
            &input.provider_id,
            ProviderRuntimeStatus::LiveModeNotConfigured,
            "spot live mode not configured",
            &live_limitation_for_provider(&input.provider_id),
        )),
    }
}

pub fn execute_dx_cluster_read_once(input: ProviderDxClusterInput) -> ProviderSpotExecution {
    if !input.enabled {
        return provider_spot_failure(
            "dx-cluster",
            ProviderRuntimeStatus::Disabled,
            "provider disabled",
            "enable DX Cluster before connecting",
        );
    }
    if input.mode == ProviderAdapterMode::Fake {
        let spots = input
            .fake_lines
            .iter()
            .filter_map(|line| parse_dx_cluster_line(line))
            .map(|spot| dx_cluster_spot_to_spot(spot, "dx-cluster"))
            .collect::<Vec<_>>();
        return ProviderSpotExecution {
            provider_id: "dx-cluster".to_owned(),
            status: ProviderRuntimeStatus::Succeeded,
            result_summary: format!("fake DX Cluster read returned {} parsed spots", spots.len()),
            spots,
            failure_reason: None,
            error_code: None,
            redacted_error: None,
            checked_at: Utc::now(),
        };
    }
    match read_dx_cluster_spots_once(&input.config) {
        Ok(spots) => ProviderSpotExecution {
            provider_id: "dx-cluster".to_owned(),
            status: ProviderRuntimeStatus::Succeeded,
            result_summary: format!("DX Cluster read returned {} parsed spots", spots.len()),
            spots,
            failure_reason: None,
            error_code: None,
            redacted_error: None,
            checked_at: Utc::now(),
        },
        Err(error) => provider_spot_failure(
            "dx-cluster",
            ProviderRuntimeStatus::Failed,
            "DX Cluster read failed",
            &error.to_string(),
        ),
    }
}

fn execute_qrz_xml_lookup(
    input: &ProviderLookupInput,
) -> Result<ProviderLookupExecution, ProviderAdapterError> {
    if input.mode == ProviderAdapterMode::Fake {
        return execute_fake_lookup(input, "qrz-xml");
    }
    let fields = credential_fields(input.credential_secret.as_deref());
    let username = match required_field(&fields, &["username", "callsign"]) {
        Ok(value) => value,
        Err(error) => {
            return Ok(provider_lookup_failure(
                &input.provider_id,
                ProviderRuntimeStatus::NeedsCredentials,
                "missing QRZ XML credential field",
                &error,
            ))
        }
    };
    let password = match required_field(&fields, &["password"]) {
        Ok(value) => value,
        Err(error) => {
            return Ok(provider_lookup_failure(
                &input.provider_id,
                ProviderRuntimeStatus::NeedsCredentials,
                "missing QRZ XML credential field",
                &error,
            ))
        }
    };
    let session_url = format!(
        "https://xmldata.qrz.com/xml/current/?username={}&password={}",
        urlencoding::encode(&username),
        urlencoding::encode(&password)
    );
    let session = match send_provider_http_request(&ProviderHttpRequest {
        method: "GET".to_owned(),
        url: session_url,
        content_type: None,
        body: None,
        timeout_seconds: DEFAULT_PROVIDER_TIMEOUT_SECONDS,
        user_agent: PROVIDER_USER_AGENT.to_owned(),
    }) {
        Ok(response) => response.body,
        Err(error) => {
            let redacted = redact_http_error(error, &[username, password]);
            return Ok(provider_lookup_failure(
                &input.provider_id,
                ProviderRuntimeStatus::Failed,
                "QRZ XML session request failed",
                &redacted.to_string(),
            ));
        }
    };
    let Some(key) = tag_text(&session, "Key") else {
        let message =
            tag_text(&session, "Error").unwrap_or_else(|| "QRZ XML session key missing".to_owned());
        return Ok(provider_lookup_failure(
            &input.provider_id,
            ProviderRuntimeStatus::NeedsCredentials,
            "QRZ XML authentication failed",
            &redact_provider_text(&message, &[username, password]),
        ));
    };
    let lookup_url = format!(
        "https://xmldata.qrz.com/xml/current/?s={}&callsign={}",
        urlencoding::encode(&key),
        urlencoding::encode(&input.callsign)
    );
    let response = match send_provider_http_request(&ProviderHttpRequest {
        method: "GET".to_owned(),
        url: lookup_url,
        content_type: None,
        body: None,
        timeout_seconds: DEFAULT_PROVIDER_TIMEOUT_SECONDS,
        user_agent: PROVIDER_USER_AGENT.to_owned(),
    }) {
        Ok(response) => response,
        Err(error) => {
            let redacted = redact_http_error(error, &[key]);
            return Ok(provider_lookup_failure(
                &input.provider_id,
                ProviderRuntimeStatus::Failed,
                "QRZ XML lookup request failed",
                &redacted.to_string(),
            ));
        }
    };
    parse_lookup_execution(input, response.body, "QRZ XML")
}

fn execute_hamqth_lookup(
    input: &ProviderLookupInput,
) -> Result<ProviderLookupExecution, ProviderAdapterError> {
    if input.mode == ProviderAdapterMode::Fake {
        return execute_fake_lookup(input, "hamqth");
    }
    let fields = credential_fields(input.credential_secret.as_deref());
    let username = match required_field(&fields, &["username", "callsign"]) {
        Ok(value) => value,
        Err(error) => {
            return Ok(provider_lookup_failure(
                &input.provider_id,
                ProviderRuntimeStatus::NeedsCredentials,
                "missing HamQTH credential field",
                &error,
            ))
        }
    };
    let password = match required_field(&fields, &["password"]) {
        Ok(value) => value,
        Err(error) => {
            return Ok(provider_lookup_failure(
                &input.provider_id,
                ProviderRuntimeStatus::NeedsCredentials,
                "missing HamQTH credential field",
                &error,
            ))
        }
    };
    let session_url = format!(
        "https://www.hamqth.com/xml.php?u={}&p={}",
        urlencoding::encode(&username),
        urlencoding::encode(&password)
    );
    let session = match send_provider_http_request(&ProviderHttpRequest {
        method: "GET".to_owned(),
        url: session_url,
        content_type: None,
        body: None,
        timeout_seconds: DEFAULT_PROVIDER_TIMEOUT_SECONDS,
        user_agent: PROVIDER_USER_AGENT.to_owned(),
    }) {
        Ok(response) => response.body,
        Err(error) => {
            let redacted = redact_http_error(error, &[username, password]);
            return Ok(provider_lookup_failure(
                &input.provider_id,
                ProviderRuntimeStatus::Failed,
                "HamQTH session request failed",
                &redacted.to_string(),
            ));
        }
    };
    let Some(session_id) = tag_text(&session, "session_id") else {
        let message =
            tag_text(&session, "error").unwrap_or_else(|| "HamQTH session id missing".to_owned());
        return Ok(provider_lookup_failure(
            &input.provider_id,
            ProviderRuntimeStatus::NeedsCredentials,
            "HamQTH authentication failed",
            &redact_provider_text(&message, &[username, password]),
        ));
    };
    let lookup_url = format!(
        "https://www.hamqth.com/xml.php?id={}&callsign={}&prg=KE8YGW-logger",
        urlencoding::encode(&session_id),
        urlencoding::encode(&input.callsign)
    );
    let response = match send_provider_http_request(&ProviderHttpRequest {
        method: "GET".to_owned(),
        url: lookup_url,
        content_type: None,
        body: None,
        timeout_seconds: DEFAULT_PROVIDER_TIMEOUT_SECONDS,
        user_agent: PROVIDER_USER_AGENT.to_owned(),
    }) {
        Ok(response) => response,
        Err(error) => {
            let redacted = redact_http_error(error, &[session_id]);
            return Ok(provider_lookup_failure(
                &input.provider_id,
                ProviderRuntimeStatus::Failed,
                "HamQTH lookup request failed",
                &redacted.to_string(),
            ));
        }
    };
    parse_lookup_execution(input, response.body, "HamQTH")
}

fn execute_fake_lookup(
    input: &ProviderLookupInput,
    provider_id: &str,
) -> Result<ProviderLookupExecution, ProviderAdapterError> {
    if input.force_fake_auth_failure {
        return Ok(provider_lookup_failure(
            provider_id,
            ProviderRuntimeStatus::NeedsCredentials,
            "fake authentication failed",
            "fake provider reported authentication failure",
        ));
    }
    if input.force_fake_not_found {
        return Ok(provider_lookup_failure(
            provider_id,
            ProviderRuntimeStatus::NotFound,
            "callsign not found",
            "fake provider did not find the callsign",
        ));
    }
    let payload = input.fake_response.clone().unwrap_or_else(|| {
        if provider_id == "qrz-xml" {
            format!(
                "<QRZDatabase><Callsign><call>{}</call><fname>Ada</fname><name>Lovelace</name><addr2>Cleveland</addr2><country>United States</country><grid>EN91</grid><dxcc>291</dxcc></Callsign></QRZDatabase>",
                input.callsign
            )
        } else {
            format!(
                "<HamQTH><search><callsign>{}</callsign><nick>Ada</nick><qth>Cleveland</qth><country>United States</country><grid>EN91</grid><dxcc>291</dxcc></search></HamQTH>",
                input.callsign
            )
        }
    });
    parse_lookup_execution(
        input,
        payload,
        if provider_id == "qrz-xml" {
            "QRZ XML"
        } else {
            "HamQTH"
        },
    )
}

fn parse_lookup_execution(
    input: &ProviderLookupInput,
    payload: String,
    display_name: &str,
) -> Result<ProviderLookupExecution, ProviderAdapterError> {
    let parsed = match input.provider_id.as_str() {
        "qrz-xml" => parse_qrz_xml_lookup_response(&payload),
        "hamqth" => parse_hamqth_lookup_response(&payload),
        _ => Ok(None),
    };
    match parsed {
        Ok(Some(result)) => Ok(ProviderLookupExecution {
            provider_id: input.provider_id.clone(),
            status: ProviderRuntimeStatus::Succeeded,
            result_summary: format!("{display_name} lookup succeeded"),
            result: Some(result),
            failure_reason: None,
            error_code: None,
            redacted_error: None,
            checked_at: Utc::now(),
        }),
        Ok(None) => Ok(provider_lookup_failure(
            &input.provider_id,
            lookup_failure_status(&payload),
            lookup_failure_reason(&payload, display_name),
            &lookup_failure_message(&payload, display_name),
        )),
        Err(error) => Ok(provider_lookup_failure(
            &input.provider_id,
            ProviderRuntimeStatus::Failed,
            "malformed provider response",
            &error,
        )),
    }
}

fn execute_pota_spots(
    input: ProviderSpotInput,
) -> Result<ProviderSpotExecution, ProviderAdapterError> {
    if input.mode == ProviderAdapterMode::Fake {
        let payload = input.fake_response.unwrap_or_else(|| {
            r#"[{"activator":"K1ABC","reference":"US-0001","frequency":14.074,"mode":"FT8","spotTime":"2026-07-08T18:00:00Z","comments":"fake POTA spot"}]"#.to_owned()
        });
        return parse_pota_spot_execution(&input.provider_id, &payload);
    }
    let response = match send_provider_http_request(&pota_spots_request()) {
        Ok(response) => response,
        Err(error) => {
            return Ok(provider_spot_failure(
                &input.provider_id,
                ProviderRuntimeStatus::Failed,
                "POTA spot request failed",
                &error.to_string(),
            ))
        }
    };
    parse_pota_spot_execution(&input.provider_id, &response.body)
}

fn parse_pota_spot_execution(
    provider_id: &str,
    payload: &str,
) -> Result<ProviderSpotExecution, ProviderAdapterError> {
    match parse_pota_spots_json(payload) {
        Ok(records) => {
            let spots = records
                .into_iter()
                .map(pota_spot_to_spot)
                .collect::<Vec<_>>();
            Ok(ProviderSpotExecution {
                provider_id: provider_id.to_owned(),
                status: ProviderRuntimeStatus::Succeeded,
                result_summary: format!("POTA spot fetch returned {} spots", spots.len()),
                spots,
                failure_reason: None,
                error_code: None,
                redacted_error: None,
                checked_at: Utc::now(),
            })
        }
        Err(error) => Ok(provider_spot_failure(
            provider_id,
            ProviderRuntimeStatus::Failed,
            "malformed POTA spot response",
            &error,
        )),
    }
}

fn provider_lookup_failure(
    provider_id: &str,
    status: ProviderRuntimeStatus,
    reason: &str,
    error: &str,
) -> ProviderLookupExecution {
    let error_code = provider_runtime_error_code(&status, reason, error);
    ProviderLookupExecution {
        provider_id: provider_id.to_owned(),
        status,
        result: None,
        result_summary: reason.to_owned(),
        failure_reason: Some(reason.to_owned()),
        error_code: Some(error_code),
        redacted_error: Some(error.to_owned()),
        checked_at: Utc::now(),
    }
}

fn provider_spot_failure(
    provider_id: &str,
    status: ProviderRuntimeStatus,
    reason: &str,
    error: &str,
) -> ProviderSpotExecution {
    let error_code = provider_runtime_error_code(&status, reason, error);
    ProviderSpotExecution {
        provider_id: provider_id.to_owned(),
        status,
        spots: Vec::new(),
        result_summary: reason.to_owned(),
        failure_reason: Some(reason.to_owned()),
        error_code: Some(error_code),
        redacted_error: Some(error.to_owned()),
        checked_at: Utc::now(),
    }
}

fn lookup_failure_status(payload: &str) -> ProviderRuntimeStatus {
    let code = provider_lookup_error_code(payload);
    if code == "callsign_not_found" {
        ProviderRuntimeStatus::NotFound
    } else if matches!(code.as_str(), "auth_failure" | "session_failure") {
        ProviderRuntimeStatus::NeedsCredentials
    } else {
        ProviderRuntimeStatus::Failed
    }
}

fn lookup_failure_reason(payload: &str, display_name: &str) -> &'static str {
    match provider_lookup_error_code(payload).as_str() {
        "callsign_not_found" => "callsign not found",
        "auth_failure" => "provider authentication failed",
        "session_failure" => "provider session failed",
        "rate_limited" | "permission_issue" => "provider permission or rate issue",
        _ => {
            if payload.trim().is_empty() {
                "empty provider response"
            } else if payload.contains('<') {
                "malformed provider response"
            } else {
                let _ = display_name;
                "malformed provider response"
            }
        }
    }
}

fn lookup_failure_message(payload: &str, display_name: &str) -> String {
    let code = provider_lookup_error_code(payload);
    if code == "malformed_response" {
        format!("{display_name} returned no parseable callsign record")
    } else {
        format!("{display_name} returned provider error category {code}")
    }
}

fn provider_lookup_error_code(payload: &str) -> String {
    let error = tag_text(payload, "Error")
        .or_else(|| tag_text(payload, "error"))
        .unwrap_or_default();
    provider_runtime_error_code(&ProviderRuntimeStatus::Failed, "", &error)
}

fn provider_runtime_error_code(
    status: &ProviderRuntimeStatus,
    reason: &str,
    error: &str,
) -> String {
    let text = format!("{reason} {error}").to_ascii_lowercase();
    if matches!(status, ProviderRuntimeStatus::Disabled) || text.contains("disabled") {
        "provider_disabled".to_owned()
    } else if matches!(status, ProviderRuntimeStatus::LiveModeNotConfigured)
        || text.contains("live provider transport not configured")
    {
        "live_mode_not_configured".to_owned()
    } else if text.contains("missing credential") || text.contains("not_present") {
        "missing_credential".to_owned()
    } else if text.contains("invalid credential")
        || text.contains("unresolved")
        || text.contains("did not resolve")
    {
        "invalid_credential_reference".to_owned()
    } else if text.contains("auth")
        || text.contains("password")
        || text.contains("username")
        || text.contains("login")
    {
        "auth_failure".to_owned()
    } else if text.contains("session")
        || text.contains("key missing")
        || text.contains("id missing")
    {
        "session_failure".to_owned()
    } else if text.contains("not found")
        || text.contains("no match")
        || text.contains("unknown call")
    {
        "callsign_not_found".to_owned()
    } else if text.contains("rate") || text.contains("too many") || text.contains("429") {
        "rate_limited".to_owned()
    } else if text.contains("permission")
        || text.contains("subscription")
        || text.contains("not authorized")
        || text.contains("terms")
        || text.contains("approval")
    {
        "permission_issue".to_owned()
    } else if text.contains("timed out") || text.contains("timeout") {
        "network_timeout".to_owned()
    } else if text.contains("refused") || text.contains("did not resolve") {
        "connection_failed".to_owned()
    } else if text.contains("tls") || text.contains("certificate") {
        "transport_failure".to_owned()
    } else if text.contains("malformed") || text.contains("parse") || text.contains("not an array")
    {
        "malformed_response".to_owned()
    } else if text.contains("rejected") {
        "provider_rejection".to_owned()
    } else {
        match status {
            ProviderRuntimeStatus::NotFound => "callsign_not_found".to_owned(),
            ProviderRuntimeStatus::NeedsCredentials => "missing_credential".to_owned(),
            ProviderRuntimeStatus::Failed => "provider_error".to_owned(),
            ProviderRuntimeStatus::Succeeded => "ok".to_owned(),
            ProviderRuntimeStatus::Disabled => "provider_disabled".to_owned(),
            ProviderRuntimeStatus::LiveModeNotConfigured => "live_mode_not_configured".to_owned(),
        }
    }
}

fn tag_text(payload: &str, tag: &str) -> Option<String> {
    let start = format!("<{tag}>");
    let end = format!("</{tag}>");
    let (_, after_start) = payload.split_once(&start)?;
    let (value, _) = after_start.split_once(&end)?;
    Some(value.trim().to_owned()).filter(|value| !value.is_empty())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DxClusterClientConfig {
    pub host: String,
    pub port: u16,
    pub callsign: String,
    pub read_lines: usize,
    pub timeout_seconds: u64,
}

pub fn read_dx_cluster_spots_once(
    config: &DxClusterClientConfig,
) -> Result<Vec<Spot>, ProviderHttpError> {
    let address = (config.host.as_str(), config.port)
        .to_socket_addrs()
        .map_err(|error| ProviderHttpError::Transport(error.to_string()))?
        .next()
        .ok_or_else(|| {
            ProviderHttpError::Transport("DX Cluster host did not resolve".to_owned())
        })?;
    let timeout = StdDuration::from_secs(config.timeout_seconds);
    let mut stream = TcpStream::connect_timeout(&address, timeout).map_err(|error| {
        provider_http_error_from_transport(&format!("DX Cluster connect failed: {error}"))
    })?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|error| ProviderHttpError::Transport(error.to_string()))?;
    stream
        .write_all(format!("{}\r\n", config.callsign).as_bytes())
        .map_err(|error| ProviderHttpError::Transport(error.to_string()))?;
    let mut reader = BufReader::new(stream);
    let mut spots = Vec::new();
    for _ in 0..config.read_lines {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if let Some(parsed) = parse_dx_cluster_line(&line) {
                    spots.push(dx_cluster_spot_to_spot(parsed, "dx-cluster"));
                }
            }
            Err(error) => {
                return Err(provider_http_error_from_transport(&format!(
                    "DX Cluster read failed: {error}"
                )))
            }
        }
    }
    Ok(spots)
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

    fn one_shot_http_fixture(status: u16, body: &'static str) -> String {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                while reader.read_line(&mut line).unwrap_or(0) > 0 {
                    if line == "\r\n" {
                        break;
                    }
                    line.clear();
                }
                let response = format!(
                    "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes());
            }
        });
        format!("http://{address}/fixture")
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
    fn tier_one_registry_reports_unsupported_capabilities() {
        assert!(tier_one_provider_supports_capability("clublog", CAP_UPLOAD_ADIF).unwrap());
        assert!(!tier_one_provider_supports_capability("qrz-xml", CAP_UPLOAD_ADIF).unwrap());
        assert!(matches!(
            tier_one_provider_supports_capability("missing-provider", CAP_UPLOAD_ADIF),
            Err(ProviderAdapterError::UnknownProvider(_))
        ));
    }

    #[test]
    fn fake_provider_test_is_ci_safe_without_credentials() {
        let result = test_tier_one_provider(ProviderAdapterTestInput {
            provider_id: "qrz-xml".to_owned(),
            capability: Some("lookup.callsign.basic".to_owned()),
            enabled: true,
            credential_reference_present: false,
            credential_resolved: false,
            mode: ProviderAdapterMode::Fake,
        })
        .unwrap();
        assert_eq!(result.test_status, "ok");
        assert!(result.credential_required);
        assert_eq!(result.provider_health_state, ProviderHealthState::Healthy);
    }

    #[test]
    fn live_provider_test_requires_resolved_credentials() {
        let result = test_tier_one_provider(ProviderAdapterTestInput {
            provider_id: "clublog".to_owned(),
            capability: Some(CAP_UPLOAD_ADIF.to_owned()),
            enabled: true,
            credential_reference_present: true,
            credential_resolved: false,
            mode: ProviderAdapterMode::Live,
        })
        .unwrap();
        assert_eq!(result.test_status, "invalid_credential_reference");
        assert_eq!(
            result.provider_health_state,
            ProviderHealthState::MissingConfig
        );
    }

    #[test]
    fn fake_upload_succeeds_and_live_missing_credential_is_retryable() {
        let fake = execute_tier_one_upload(ProviderUploadInput {
            provider_id: "clublog".to_owned(),
            job_id: Uuid::new_v4(),
            adif_payload: "<CALL:5>K1ABC<EOR>".to_owned(),
            qso_count: 1,
            enabled: true,
            credential_reference_present: false,
            credential_resolved: false,
            credential_secret: None,
            mode: ProviderAdapterMode::Fake,
            force_fake_failure: false,
        })
        .unwrap();
        assert_eq!(fake.status, UploadJobStatus::Succeeded);
        assert_eq!(fake.accepted_count, 1);

        let missing = execute_tier_one_upload(ProviderUploadInput {
            provider_id: "clublog".to_owned(),
            job_id: Uuid::new_v4(),
            adif_payload: "<CALL:5>K1ABC<EOR>".to_owned(),
            qso_count: 1,
            enabled: true,
            credential_reference_present: false,
            credential_resolved: false,
            credential_secret: None,
            mode: ProviderAdapterMode::Live,
            force_fake_failure: false,
        })
        .unwrap();
        assert_eq!(missing.status, UploadJobStatus::NeedsCredentials);
        assert!(missing.retryable);
    }

    fn upload_input(provider_id: &str) -> ProviderUploadInput {
        ProviderUploadInput {
            provider_id: provider_id.to_owned(),
            job_id: Uuid::new_v4(),
            adif_payload: "<CALL:5>K1ABC<EOR>".to_owned(),
            qso_count: 1,
            enabled: true,
            credential_reference_present: true,
            credential_resolved: true,
            credential_secret: None,
            mode: ProviderAdapterMode::Live,
            force_fake_failure: false,
        }
    }

    fn live_provider_tests_enabled() -> bool {
        std::env::var("HAM_LIVE_PROVIDER_TESTS").ok().as_deref() == Some("1")
    }

    fn live_upload_tests_enabled() -> bool {
        live_provider_tests_enabled()
            && std::env::var("HAM_LIVE_PROVIDER_ALLOW_UPLOAD")
                .ok()
                .as_deref()
                == Some("1")
    }

    fn live_env(name: &str) -> Option<String> {
        std::env::var(name)
            .ok()
            .filter(|value| !value.trim().is_empty())
    }

    fn skip_live(provider_id: &str, reason: &str) {
        eprintln!("provider={provider_id} mode=live result=skip reason={reason}");
    }

    fn assert_execution_redacted(summary: &str, error: Option<&str>, secrets: &[&str]) {
        for secret in secrets {
            assert!(!secret.is_empty());
            assert!(!summary.contains(secret));
            if let Some(error) = error {
                assert!(!error.contains(secret));
            }
        }
    }

    fn live_upload_input(provider_id: &str, credential_secret: String) -> ProviderUploadInput {
        ProviderUploadInput {
            provider_id: provider_id.to_owned(),
            job_id: Uuid::new_v4(),
            adif_payload:
                "<CALL:5>N0CALL<BAND:3>20M<MODE:3>FT8<QSO_DATE:8>20260708<TIME_ON:6>120000<EOR>"
                    .to_owned(),
            qso_count: 1,
            enabled: true,
            credential_reference_present: true,
            credential_resolved: true,
            credential_secret: Some(credential_secret),
            mode: ProviderAdapterMode::Live,
            force_fake_failure: false,
        }
    }

    #[test]
    #[ignore = "release-runner live validation; requires HAM_LIVE_PROVIDER_TESTS=1 and real Club Log test credentials"]
    fn live_clublog_upload_validation_is_env_gated() {
        if !live_upload_tests_enabled() {
            skip_live(
                "clublog",
                "HAM_LIVE_PROVIDER_TESTS/HAM_LIVE_PROVIDER_ALLOW_UPLOAD not set",
            );
            return;
        }
        let (Some(email), Some(callsign), Some(password), Some(api)) = (
            live_env("HAM_CLUBLOG_TEST_EMAIL"),
            live_env("HAM_CLUBLOG_TEST_CALLSIGN"),
            live_env("HAM_CLUBLOG_TEST_PASSWORD"),
            live_env("HAM_CLUBLOG_TEST_API_KEY"),
        ) else {
            skip_live("clublog", "provider-specific credentials missing");
            return;
        };
        eprintln!(
            "provider=clublog mode=live action=upload warning=may_create_provider_side_record"
        );
        let secret = serde_json::json!({
            "email": email.clone(),
            "callsign": callsign.clone(),
            "password": password.clone(),
            "api": api.clone(),
        })
        .to_string();
        let result = execute_tier_one_upload(live_upload_input("clublog", secret)).unwrap();
        assert!(matches!(
            result.status,
            UploadJobStatus::Succeeded | UploadJobStatus::Failed
        ));
        eprintln!(
            "provider=clublog mode=live status={:?} retryable={}",
            result.status, result.retryable
        );
        assert_execution_redacted(
            &result.result_summary,
            result.redacted_error.as_deref(),
            &[&password, &api],
        );
    }

    #[test]
    #[ignore = "release-runner live validation; requires HAM_LIVE_PROVIDER_TESTS=1 and real QRZ Logbook test key"]
    fn live_qrz_logbook_upload_validation_is_env_gated() {
        if !live_upload_tests_enabled() {
            skip_live(
                "qrz-logbook",
                "HAM_LIVE_PROVIDER_TESTS/HAM_LIVE_PROVIDER_ALLOW_UPLOAD not set",
            );
            return;
        }
        let Some(key) = live_env("HAM_QRZ_LOGBOOK_TEST_KEY") else {
            skip_live("qrz-logbook", "HAM_QRZ_LOGBOOK_TEST_KEY missing");
            return;
        };
        eprintln!(
            "provider=qrz-logbook mode=live action=upload warning=may_create_provider_side_record"
        );
        let secret = serde_json::json!({ "key": key.clone() }).to_string();
        let result = execute_tier_one_upload(live_upload_input("qrz-logbook", secret)).unwrap();
        assert!(matches!(
            result.status,
            UploadJobStatus::Succeeded | UploadJobStatus::Failed
        ));
        eprintln!(
            "provider=qrz-logbook mode=live status={:?} retryable={}",
            result.status, result.retryable
        );
        assert_execution_redacted(
            &result.result_summary,
            result.redacted_error.as_deref(),
            &[&key],
        );
    }

    #[test]
    #[ignore = "release-runner live validation; requires HAM_LIVE_PROVIDER_TESTS=1 and real eQSL test credentials"]
    fn live_eqsl_upload_validation_is_env_gated() {
        if !live_upload_tests_enabled() {
            skip_live(
                "eqsl",
                "HAM_LIVE_PROVIDER_TESTS/HAM_LIVE_PROVIDER_ALLOW_UPLOAD not set",
            );
            return;
        }
        let (Some(username), Some(password)) = (
            live_env("HAM_EQSL_TEST_USERNAME"),
            live_env("HAM_EQSL_TEST_PASSWORD"),
        ) else {
            skip_live("eqsl", "provider-specific credentials missing");
            return;
        };
        eprintln!("provider=eqsl mode=live action=upload warning=may_create_provider_side_record");
        let secret = serde_json::json!({
            "username": username.clone(),
            "password": password.clone(),
        })
        .to_string();
        let result = execute_tier_one_upload(live_upload_input("eqsl", secret)).unwrap();
        assert!(matches!(
            result.status,
            UploadJobStatus::Succeeded | UploadJobStatus::Failed
        ));
        eprintln!(
            "provider=eqsl mode=live status={:?} retryable={}",
            result.status, result.retryable
        );
        assert_execution_redacted(
            &result.result_summary,
            result.redacted_error.as_deref(),
            &[&password],
        );
    }

    #[test]
    #[ignore = "release-runner live validation; requires HAM_LIVE_PROVIDER_TESTS=1 and QRZ XML credentials"]
    fn live_qrz_xml_lookup_validation_is_env_gated() {
        if !live_provider_tests_enabled() {
            skip_live("qrz-xml", "HAM_LIVE_PROVIDER_TESTS not set");
            return;
        }
        let (Some(username), Some(password), Some(callsign)) = (
            live_env("HAM_QRZ_XML_TEST_USERNAME"),
            live_env("HAM_QRZ_XML_TEST_PASSWORD"),
            live_env("HAM_QRZ_XML_TEST_CALLSIGN"),
        ) else {
            skip_live("qrz-xml", "provider-specific credentials/callsign missing");
            return;
        };
        let secret = serde_json::json!({
            "username": username.clone(),
            "password": password.clone(),
        })
        .to_string();
        let result = execute_tier_one_lookup(ProviderLookupInput {
            provider_id: "qrz-xml".to_owned(),
            callsign,
            enabled: true,
            credential_reference_present: true,
            credential_resolved: true,
            credential_secret: Some(secret),
            mode: ProviderAdapterMode::Live,
            fake_response: None,
            force_fake_not_found: false,
            force_fake_auth_failure: false,
        })
        .unwrap();
        eprintln!(
            "provider=qrz-xml mode=live status={:?} code={}",
            result.status,
            result.error_code.as_deref().unwrap_or("ok")
        );
        assert_execution_redacted(
            &result.result_summary,
            result.redacted_error.as_deref(),
            &[&password],
        );
        if result.status == ProviderRuntimeStatus::Succeeded {
            assert!(result.result.is_some());
        } else {
            assert!(result.error_code.is_some());
        }
    }

    #[test]
    #[ignore = "release-runner live validation; requires HAM_LIVE_PROVIDER_TESTS=1 and HamQTH credentials"]
    fn live_hamqth_lookup_validation_is_env_gated() {
        if !live_provider_tests_enabled() {
            skip_live("hamqth", "HAM_LIVE_PROVIDER_TESTS not set");
            return;
        }
        let (Some(username), Some(password), Some(callsign)) = (
            live_env("HAM_HAMQTH_TEST_USERNAME"),
            live_env("HAM_HAMQTH_TEST_PASSWORD"),
            live_env("HAM_HAMQTH_TEST_CALLSIGN"),
        ) else {
            skip_live("hamqth", "provider-specific credentials/callsign missing");
            return;
        };
        let secret = serde_json::json!({
            "username": username.clone(),
            "password": password.clone(),
        })
        .to_string();
        let result = execute_tier_one_lookup(ProviderLookupInput {
            provider_id: "hamqth".to_owned(),
            callsign,
            enabled: true,
            credential_reference_present: true,
            credential_resolved: true,
            credential_secret: Some(secret),
            mode: ProviderAdapterMode::Live,
            fake_response: None,
            force_fake_not_found: false,
            force_fake_auth_failure: false,
        })
        .unwrap();
        eprintln!(
            "provider=hamqth mode=live status={:?} code={}",
            result.status,
            result.error_code.as_deref().unwrap_or("ok")
        );
        assert_execution_redacted(
            &result.result_summary,
            result.redacted_error.as_deref(),
            &[&password],
        );
        if result.status == ProviderRuntimeStatus::Succeeded {
            assert!(result.result.is_some());
        } else {
            assert!(result.error_code.is_some());
        }
    }

    #[test]
    #[ignore = "release-runner live validation; requires HAM_LIVE_PROVIDER_TESTS=1 for read-only POTA fetch"]
    fn live_pota_spot_fetch_validation_is_env_gated() {
        if !live_provider_tests_enabled() {
            skip_live("pota-spots", "HAM_LIVE_PROVIDER_TESTS not set");
            return;
        }
        let result = fetch_tier_one_spots(ProviderSpotInput {
            provider_id: "pota-spots".to_owned(),
            enabled: true,
            mode: ProviderAdapterMode::Live,
            fake_response: None,
        })
        .unwrap();
        eprintln!(
            "provider=pota-spots mode=live status={:?} count={} code={}",
            result.status,
            result.spots.len(),
            result.error_code.as_deref().unwrap_or("ok")
        );
        if result.status == ProviderRuntimeStatus::Succeeded {
            assert!(result
                .spots
                .iter()
                .all(|spot| spot.source.provider_id == "pota-spots"));
        } else {
            assert!(result.error_code.is_some());
        }
    }

    #[test]
    #[ignore = "release-runner live validation; requires HAM_LIVE_PROVIDER_TESTS=1 and DX Cluster host/callsign"]
    fn live_dx_cluster_read_once_validation_is_env_gated() {
        if !live_provider_tests_enabled() {
            skip_live("dx-cluster", "HAM_LIVE_PROVIDER_TESTS not set");
            return;
        }
        let (Some(host), Some(callsign)) = (
            live_env("HAM_DX_CLUSTER_TEST_HOST"),
            live_env("HAM_DX_CLUSTER_TEST_CALLSIGN"),
        ) else {
            skip_live("dx-cluster", "host/callsign missing");
            return;
        };
        let port = live_env("HAM_DX_CLUSTER_TEST_PORT")
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(7300);
        let timeout_seconds = live_env("HAM_DX_CLUSTER_TEST_TIMEOUT_SECONDS")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(5)
            .min(30);
        let result = execute_dx_cluster_read_once(ProviderDxClusterInput {
            enabled: true,
            mode: ProviderAdapterMode::Live,
            config: DxClusterClientConfig {
                host,
                port,
                callsign,
                read_lines: 20,
                timeout_seconds,
            },
            fake_lines: Vec::new(),
        });
        eprintln!(
            "provider=dx-cluster mode=live status={:?} count={} code={}",
            result.status,
            result.spots.len(),
            result.error_code.as_deref().unwrap_or("ok")
        );
        if result.status != ProviderRuntimeStatus::Succeeded {
            assert!(result.error_code.is_some());
        }
    }

    #[test]
    fn provider_redaction_masks_secret_values() {
        let redacted = redact_provider_text(
            "POST password=swordfish api=abc123",
            &["swordfish".to_owned(), "abc123".to_owned()],
        );
        assert!(!redacted.contains("swordfish"));
        assert!(!redacted.contains("abc123"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn provider_http_error_mapping_handles_status_and_timeout() {
        assert!(matches!(
            map_provider_http_status(fake_provider_http_response(500, "down")),
            Err(ProviderHttpError::HttpStatus { status: 500, .. })
        ));
        assert!(matches!(
            provider_http_error_from_transport("operation timed out"),
            ProviderHttpError::Timeout
        ));
    }

    #[test]
    fn provider_runtime_error_codes_classify_common_failures() {
        assert_eq!(
            provider_runtime_error_code(
                &ProviderRuntimeStatus::NeedsCredentials,
                "missing credential reference",
                "",
            ),
            "missing_credential"
        );
        assert_eq!(
            provider_runtime_error_code(
                &ProviderRuntimeStatus::Failed,
                "QRZ XML authentication failed",
                "password incorrect",
            ),
            "auth_failure"
        );
        assert_eq!(
            provider_runtime_error_code(
                &ProviderRuntimeStatus::Failed,
                "provider request failed",
                "operation timed out",
            ),
            "network_timeout"
        );
        assert_eq!(
            provider_runtime_error_code(
                &ProviderRuntimeStatus::Failed,
                "malformed provider response",
                "POTA spot payload is not an array",
            ),
            "malformed_response"
        );

        let lookup = execute_tier_one_lookup(ProviderLookupInput {
            provider_id: "qrz-xml".to_owned(),
            callsign: "K1ABC".to_owned(),
            enabled: true,
            credential_reference_present: false,
            credential_resolved: false,
            credential_secret: None,
            mode: ProviderAdapterMode::Fake,
            fake_response: Some(
                "<QRZDatabase><Session><Error>Not found: K1ABC</Error></Session></QRZDatabase>"
                    .to_owned(),
            ),
            force_fake_not_found: false,
            force_fake_auth_failure: false,
        })
        .unwrap();
        assert_eq!(lookup.status, ProviderRuntimeStatus::NotFound);
        assert_eq!(lookup.error_code.as_deref(), Some("callsign_not_found"));

        let spots = fetch_tier_one_spots(ProviderSpotInput {
            provider_id: "pota-spots".to_owned(),
            enabled: true,
            mode: ProviderAdapterMode::Fake,
            fake_response: Some("{}".to_owned()),
        })
        .unwrap();
        assert_eq!(spots.status, ProviderRuntimeStatus::Failed);
        assert_eq!(spots.error_code.as_deref(), Some("malformed_response"));
    }

    #[test]
    fn live_upload_response_parsers_classify_success_auth_and_retry() {
        let input = upload_input("clublog");
        let ok = parse_clublog_upload_response(&input, fake_provider_http_response(200, "QSO OK"))
            .unwrap();
        assert_eq!(ok.status, UploadJobStatus::Succeeded);

        let auth =
            parse_clublog_upload_response(&input, fake_provider_http_response(403, "bad password"))
                .unwrap();
        assert_eq!(auth.status, UploadJobStatus::Failed);
        assert!(!auth.retryable);

        let retry =
            parse_clublog_upload_response(&input, fake_provider_http_response(500, "temporary"))
                .unwrap();
        assert!(retry.retryable);

        let qrz = parse_qrz_logbook_upload_response(
            &upload_input("qrz-logbook"),
            fake_provider_http_response(200, "RESULT=OK&LOGID=123"),
        )
        .unwrap();
        assert_eq!(qrz.status, UploadJobStatus::Succeeded);

        let eqsl = parse_eqsl_upload_response(
            &upload_input("eqsl"),
            fake_provider_http_response(200, "1 out of 1 records added"),
        )
        .unwrap();
        assert_eq!(eqsl.accepted_count, 1);
    }

    #[test]
    fn lookup_xml_parsers_extract_safe_fields() {
        let qrz = parse_qrz_xml_lookup_response(
            "<QRZDatabase><Callsign><call>K1ABC</call><fname>Ada</fname><name>Lovelace</name><addr2>Cleveland</addr2><country>United States</country><grid>EN91</grid><dxcc>291</dxcc></Callsign></QRZDatabase>",
        )
        .unwrap()
        .unwrap();
        assert_eq!(qrz.normalized_callsign, "K1ABC");
        assert_eq!(qrz.grid.as_deref(), Some("EN91"));
        assert_eq!(qrz.source_provider, "qrz-xml");

        let hamqth = parse_hamqth_lookup_response(
            "<HamQTH><search><callsign>K1ABC</callsign><nick>Ada</nick><qth>Cleveland</qth><grid>EN91</grid><dxcc>291</dxcc></search></HamQTH>",
        )
        .unwrap()
        .unwrap();
        assert_eq!(hamqth.source_provider, "hamqth");
        assert_eq!(hamqth.name.as_deref(), Some("Ada"));
    }

    #[test]
    fn spot_fixture_parsers_normalize_pota_and_sota() {
        let pota = parse_pota_spots_json(
            r#"[{"activator":"k1abc","reference":"US-0001","frequency":14.074,"mode":"FT8","spotTime":"2026-07-08T18:00:00Z","comments":"test"}]"#,
        )
        .unwrap();
        assert_eq!(pota[0].frequency_hz, 14_074_000);
        assert_eq!(
            pota_spot_to_spot(pota[0].clone()).reference.as_deref(),
            Some("US-0001")
        );

        let sota = parse_sota_spots_json(
            r#"[{"activatorCallsign":"k1abc","summitCode":"W8O/NE-001","frequency":14.285,"mode":"SSB","timeStamp":"2026-07-08T18:00:00Z","comments":"cq"}]"#,
        )
        .unwrap();
        assert_eq!(
            sota_spot_to_spot(sota[0].clone()).reference.as_deref(),
            Some("W8O/NE-001")
        );
        assert!(parse_pota_spots_json("{}").is_err());
        assert!(sota_spots_request().is_err());
    }

    #[test]
    fn dx_cluster_client_config_and_malformed_lines_are_safe() {
        let config = DxClusterClientConfig {
            host: "cluster.example.test".to_owned(),
            port: 7300,
            callsign: "K1ABC".to_owned(),
            read_lines: 10,
            timeout_seconds: 3,
        };
        assert_eq!(config.port, 7300);
        assert!(parse_dx_cluster_line("not a spot").is_none());
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
        assert_eq!(
            next_retry_delay_with_jitter(&policy, 2, 0.25, 3),
            Duration::seconds(23)
        );
        assert!(retryable_http_status(429));
        assert!(!retryable_http_status(401));
        assert_eq!(
            provider_retry_after_seconds("999", 120),
            Some(120),
            "Retry-After values are capped"
        );
        assert_eq!(
            classify_provider_http_error(&ProviderHttpError::HttpStatus {
                status: 429,
                body: "slow down".to_owned()
            }),
            ProviderOutcomeKind::RateLimited
        );
    }

    #[test]
    fn circuit_breaker_transitions_deterministically() {
        let now = Utc::now();
        let mut circuit = ProviderCircuitBreaker::new(2, 30, 1);
        assert!(circuit.allow_request(now));
        circuit.record_outcome(ProviderOutcomeKind::AuthenticationRejected, now);
        assert_eq!(circuit.state, CircuitBreakerState::Closed);
        circuit.record_outcome(ProviderOutcomeKind::Timeout, now);
        assert_eq!(circuit.state, CircuitBreakerState::Closed);
        circuit.record_outcome(ProviderOutcomeKind::ProviderUnavailable, now);
        assert_eq!(circuit.state, CircuitBreakerState::Open);
        assert!(!circuit.allow_request(now + Duration::seconds(10)));
        assert!(circuit.allow_request(now + Duration::seconds(31)));
        assert_eq!(circuit.state, CircuitBreakerState::HalfOpen);
        assert!(!circuit.allow_request(now + Duration::seconds(32)));
        circuit.record_success();
        assert_eq!(circuit.state, CircuitBreakerState::Closed);
    }

    #[test]
    fn rate_limiter_tracks_provider_account_and_overflow() {
        let now = Utc::now();
        let mut policy = ProviderRateLimitPolicy::for_provider("clublog");
        policy.max_concurrent_global = 1;
        policy.max_concurrent_per_account = 1;
        policy.burst_limit = 1;
        policy.queue_limit = 1;
        let mut limiter = ProviderRateLimiter::new(policy, now);
        let first = limiter.try_acquire(Some("acct-1"), now).unwrap();
        assert_eq!(first.running_global, 1);
        let blocked = limiter.try_acquire(Some("acct-1"), now).unwrap_err();
        assert_eq!(blocked.next_allowed_at, Some(limiter.next_refill_at));
        limiter.release(Some("acct-1"));
        limiter.queue_depth = 1;
        let overflow = limiter
            .try_acquire(Some("acct-2"), now + Duration::seconds(61))
            .unwrap_err();
        assert!(overflow.overflowed);
        assert!(overflow.instance_local);
    }

    #[test]
    fn http_runtime_rejects_oversized_response_and_redacts_echoed_secret() {
        let secret = "TEST_SECRET_SHOULD_NOT_APPEAR";
        let request = ProviderHttpRequest {
            method: "GET".to_owned(),
            url: one_shot_http_fixture(500, secret),
            content_type: None,
            body: None,
            timeout_seconds: 2,
            user_agent: PROVIDER_USER_AGENT.to_owned(),
        };
        let error = send_provider_http_request_with_config(
            &request,
            &ProviderHttpRuntimeConfig {
                max_response_body_bytes: 8,
                ..provider_http_config_for_request(&request)
            },
        )
        .unwrap_err();
        assert!(matches!(error, ProviderHttpError::ResponseTooLarge { .. }));

        let redacted = redact_http_error(
            ProviderHttpError::HttpStatus {
                status: 500,
                body: format!("provider echoed password {secret}"),
            },
            &[secret.to_owned()],
        );
        assert!(!redacted.to_string().contains(secret));
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
            provider_id: "lotw".to_owned(),
            account_scope: None,
            logbook_id: Uuid::new_v4(),
            operation_type: "upload.adif".to_owned(),
            idempotency_key: Uuid::new_v4().to_string(),
            qso_ids: vec![],
            items: vec![],
            status: UploadStatus::Failed,
            queue_state: crate::upload::UploadQueueState::DeadLetter,
            attempt_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_attempt_at: None,
            next_attempt_at: None,
            last_error: Some("offline".to_owned()),
            safe_failure_code: Some("transport_failure".to_owned()),
            credential_reference: None,
            provider_side_identifier: None,
            uncertain_outcome: false,
            claim_token: None,
            lease_expires_at: None,
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
            provider_id: "lotw".to_owned(),
            account_scope: None,
            logbook_id: Uuid::new_v4(),
            operation_type: "upload.adif".to_owned(),
            idempotency_key: Uuid::new_v4().to_string(),
            qso_ids: vec![],
            items: vec![],
            status: UploadStatus::Queued,
            queue_state: crate::upload::UploadQueueState::Pending,
            attempt_count: 0,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            last_attempt_at: None,
            next_attempt_at: None,
            last_error: None,
            safe_failure_code: None,
            credential_reference: None,
            provider_side_identifier: None,
            uncertain_outcome: false,
            claim_token: None,
            lease_expires_at: None,
        };
        let projection = QsoCurrentStateProjection::new();
        let result =
            execute_upload_with_provider(&provider, &job, &projection, 1, &RetryPolicy::default())
                .await
                .unwrap();
        assert_eq!(result.status, UploadJobStatus::Succeeded);
    }
}
