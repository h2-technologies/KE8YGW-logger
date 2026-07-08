use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
};

use chrono::{DateTime, Utc};
use ham_core::{
    adif_for_upload_job, default_log_directory, default_service_registry, export_adif, import_adif,
    qso_map_objects, station_markers_from_profiles, submit_proposal, AdifImportOptions, Coordinate,
    CoreEventEnvelope, EquipmentItem, EquipmentStatus, EquipmentType, InMemoryEventBus,
    InMemoryLogbookEventStore, JsonlLogbookEventStore, LogbookEventStore, MapLayerStack,
    NetControlProjection, OperatorRole, Projection, ProposalContext, RegisteredServiceProvider,
    StationProfile,
};
use ham_plugin_sdk::{
    PluginCapability, PluginManifest, ProposalEnvelope, PROPOSAL_ACTIVATION_CREATE,
    PROPOSAL_ACTIVATION_END, PROPOSAL_ACTIVATION_START, PROPOSAL_ACTIVATION_UPDATE,
    PROPOSAL_NET_CHECKIN_CREATE, PROPOSAL_NET_CHECKIN_UPDATE, PROPOSAL_NET_SESSION_END,
    PROPOSAL_NET_SESSION_START, PROPOSAL_NET_TRAFFIC_CREATE, PROPOSAL_QSO_CORRECT,
    PROPOSAL_QSO_CREATE, PROPOSAL_QSO_DELETE, PROPOSAL_QSO_NOTE_ADD, PROPOSAL_QSO_RESTORE,
};
use ham_sync::{
    metadata_for_event, preview_pull_from_events, CloudPullEventsResponse, CloudPushEventsRequest,
    LogbookHeadSummary, PreviewPullRequest, ReplicationStatus,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value as JsonValue};
use sha2::{Digest, Sha256};
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
use tokio::runtime::Runtime;
use tokio::sync::RwLock;
use uuid::Uuid;

type Value = JsonValue;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserAccount {
    pub account_id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    pub display_name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoginSession {
    pub session_id: Uuid,
    pub account_id: Uuid,
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub token: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceIdentity {
    pub device_id: Uuid,
    pub account_id: Uuid,
    pub user_id: Uuid,
    pub device_name: String,
    pub fingerprint: String,
    pub trusted: bool,
    pub revoked: bool,
    pub registered_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiLogbook {
    pub logbook_id: Uuid,
    pub account_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub station_callsign: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LogbookRole {
    Owner,
    Admin,
    Operator,
    Viewer,
}

impl LogbookRole {
    fn can_read(self) -> bool {
        matches!(
            self,
            Self::Owner | Self::Admin | Self::Operator | Self::Viewer
        )
    }

    fn can_log_qso(self) -> bool {
        matches!(self, Self::Owner | Self::Admin | Self::Operator)
    }

    fn can_administer(self) -> bool {
        matches!(self, Self::Owner | Self::Admin)
    }

    fn can_manage_owner_resources(self) -> bool {
        matches!(self, Self::Owner)
    }

    fn proposal_role(self) -> OperatorRole {
        match self {
            Self::Owner | Self::Admin => OperatorRole::Admin,
            Self::Operator => OperatorRole::Logger,
            Self::Viewer => OperatorRole::ReadOnly,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogbookMembership {
    pub account_id: Uuid,
    pub logbook_id: Uuid,
    pub user_id: Uuid,
    pub role: LogbookRole,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerInvite {
    pub invite_id: Uuid,
    pub account_id: Uuid,
    pub logbook_id: Uuid,
    pub invited_email: String,
    pub role: LogbookRole,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiToken {
    pub token_id: Uuid,
    pub account_id: Uuid,
    pub user_id: Uuid,
    pub device_id: Uuid,
    pub token: String,
    pub scopes: Vec<String>,
    pub revoked: bool,
    pub issued_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiRequest {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl ApiRequest {
    pub fn json(method: impl Into<String>, target: impl AsRef<str>, body: &impl Serialize) -> Self {
        let body = serde_json::to_vec(body).expect("test request body should serialize");
        let (path, query) = split_target(target.as_ref());
        Self {
            method: method.into(),
            path: path.to_owned(),
            query: parse_query(query),
            headers: HashMap::new(),
            body,
        }
    }

    pub fn get(target: impl AsRef<str>) -> Self {
        let (path, query) = split_target(target.as_ref());
        Self {
            method: "GET".to_owned(),
            path: path.to_owned(),
            query: parse_query(query),
            headers: HashMap::new(),
            body: Vec::new(),
        }
    }

    pub fn with_bearer(mut self, token: impl Into<String>) -> Self {
        self.headers.insert(
            "authorization".to_owned(),
            format!("Bearer {}", token.into()),
        );
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiResponse {
    pub status: u16,
    pub body: Vec<u8>,
}

impl ApiResponse {
    pub fn json<T: for<'de> Deserialize<'de>>(&self) -> T {
        serde_json::from_slice(&self.body).expect("response body should be valid JSON")
    }
}

#[derive(Debug, Error)]
pub enum ApiError {
    #[error("unauthenticated")]
    Unauthenticated,
    #[error("session is inactive")]
    InactiveSession,
    #[error("device is revoked")]
    RevokedDevice,
    #[error("forbidden")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("proposal rejected: {0}")]
    Proposal(String),
    #[error("store error: {0}")]
    Store(String),
}

#[derive(Debug, Error)]
pub enum MetadataStoreError {
    #[error("metadata store I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("metadata store SurrealDB error: {0}")]
    Surreal(#[from] surrealdb::Error),
    #[error("metadata store serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("official event store error: {0}")]
    OfficialStore(String),
    #[error("metadata store thread failed")]
    Thread,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub display_name: Option<String>,
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginResponse {
    pub account: UserAccount,
    pub session: LoginSession,
    pub device: DeviceIdentity,
    pub logbooks: Vec<ApiLogbook>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionResponse {
    pub account: UserAccount,
    pub session: LoginSession,
    pub device: DeviceIdentity,
    pub memberships: Vec<LogbookMembership>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateLogbookRequest {
    pub name: String,
    pub description: Option<String>,
    pub station_callsign: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateLogbookRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub station_callsign: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QsoWriteRequest {
    pub logbook_id: Uuid,
    pub contacted_callsign: Option<String>,
    pub station_callsign: Option<String>,
    pub operator_callsign: Option<String>,
    pub started_at: Option<String>,
    pub mode: Option<String>,
    pub band: Option<String>,
    pub frequency_hz: Option<u64>,
    pub notes: Option<String>,
    #[serde(default)]
    pub fields: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QsoActionRequest {
    pub logbook_id: Uuid,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StationProfileRequest {
    pub logbook_id: Uuid,
    pub display_name: Option<String>,
    pub station_callsign: Option<String>,
    pub operator_callsign: Option<String>,
    pub default_grid: Option<String>,
    pub default_qth: Option<String>,
    pub default_power_watts: Option<u32>,
    pub notes: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentProfileRequest {
    pub logbook_id: Uuid,
    pub equipment_type: Option<EquipmentType>,
    pub display_name: Option<String>,
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub serial_number: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    pub notes: Option<String>,
    pub status: Option<EquipmentStatus>,
    pub station_profile_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdifImportRequest {
    pub logbook_id: Uuid,
    pub adif: String,
    pub station_callsign: Option<String>,
    pub operator_callsign: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderPatchRequest {
    pub logbook_id: Uuid,
    pub enabled: Option<bool>,
    pub credential_id: Option<String>,
    #[serde(default)]
    pub config: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderTestRequest {
    pub logbook_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadRunRequest {
    pub logbook_id: Uuid,
    pub provider_id: String,
    pub qso_ids: Option<Vec<Uuid>>,
    pub force_fail: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationWriteRequest {
    pub logbook_id: Uuid,
    pub activation_type: String,
    pub station_callsign: Option<String>,
    pub operator_callsign: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub park_id: Option<String>,
    pub summit_id: Option<String>,
    pub reference: Option<String>,
    pub name: Option<String>,
    pub notes: Option<String>,
    #[serde(default)]
    pub fields: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetSessionWriteRequest {
    pub logbook_id: Uuid,
    pub station_callsign: Option<String>,
    pub net_control_operator_id: Option<String>,
    pub net_name: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub frequency_hz: Option<u64>,
    pub band: Option<String>,
    pub mode: Option<String>,
    pub notes: Option<String>,
    #[serde(default)]
    pub fields: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetCheckInWriteRequest {
    pub logbook_id: Uuid,
    pub callsign: Option<String>,
    pub tactical_callsign: Option<String>,
    pub tactical_only: Option<bool>,
    pub checkin_time: Option<String>,
    pub status: Option<String>,
    pub traffic: Option<String>,
    pub notes: Option<String>,
    #[serde(default)]
    pub fields: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetTrafficWriteRequest {
    pub logbook_id: Uuid,
    pub summary: Option<String>,
    pub precedence: Option<String>,
    pub status: Option<String>,
    pub handling_notes: Option<String>,
    #[serde(default)]
    pub fields: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapSettingsPatchRequest {
    pub logbook_id: Uuid,
    pub layer_id: Option<String>,
    pub enabled: Option<bool>,
    pub order: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupExportRequest {
    pub logbook_id: Uuid,
    pub include_runtime_logs: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupDryRunRequest {
    pub logbook_id: Uuid,
    pub backup: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupImportRequest {
    pub logbook_id: Uuid,
    pub backup: Value,
    pub confirm_dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DivergenceReviewRequest {
    pub logbook_id: Uuid,
    pub local_head_hash: Option<String>,
    #[serde(default)]
    pub client_events: Vec<CoreEventEnvelope>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QsoRecordResponse {
    pub qso_id: Uuid,
    pub payload: Value,
    pub note_history: Vec<Value>,
    pub deleted: bool,
    pub last_event_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QsoListResponse {
    pub logbook_id: Uuid,
    pub qsos: Vec<QsoRecordResponse>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostedStationProfile {
    pub account_id: Uuid,
    pub logbook_id: Uuid,
    pub profile: StationProfile,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostedEquipmentProfile {
    pub account_id: Uuid,
    pub logbook_id: Uuid,
    pub equipment: EquipmentItem,
    pub station_profile_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostedProviderSetting {
    pub account_id: Uuid,
    pub logbook_id: Uuid,
    pub provider_id: String,
    pub enabled: bool,
    pub credential_id: Option<String>,
    pub config: Map<String, Value>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostedUploadStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Retryable,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostedUploadJob {
    pub upload_id: Uuid,
    pub account_id: Uuid,
    pub logbook_id: Uuid,
    pub provider_id: String,
    pub status: HostedUploadStatus,
    pub qso_ids: Vec<Uuid>,
    pub generated_adif: String,
    pub retry_count: u32,
    pub failure_reason: Option<String>,
    pub provider_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostedMapSettings {
    pub account_id: Uuid,
    pub logbook_id: Uuid,
    pub layers: MapLayerStack,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostedBackupRecord {
    pub backup_id: Uuid,
    pub account_id: Uuid,
    pub logbook_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub manifest: Value,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostedDivergenceReport {
    pub report_id: Uuid,
    pub account_id: Uuid,
    pub logbook_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub review: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteCatalogResponse {
    pub implemented: Vec<String>,
    pub scaffolded: Vec<String>,
}

#[derive(Debug, Default, Clone)]
struct ServerState {
    users_by_email: HashMap<String, Uuid>,
    accounts: HashMap<Uuid, UserAccount>,
    logbooks: HashMap<Uuid, ApiLogbook>,
    memberships: Vec<LogbookMembership>,
    sessions_by_token: HashMap<String, LoginSession>,
    devices: HashMap<Uuid, DeviceIdentity>,
    invites: HashMap<Uuid, ServerInvite>,
    api_tokens: HashMap<Uuid, ApiToken>,
    station_profiles: HashMap<Uuid, HostedStationProfile>,
    equipment_profiles: HashMap<Uuid, HostedEquipmentProfile>,
    provider_settings: HashMap<String, HostedProviderSetting>,
    upload_jobs: HashMap<Uuid, HostedUploadJob>,
    map_settings: HashMap<Uuid, HostedMapSettings>,
    backups: HashMap<Uuid, HostedBackupRecord>,
    divergence_reports: HashMap<Uuid, HostedDivergenceReport>,
}

trait HostedMetadataStore: Send + Sync + std::fmt::Debug {
    fn load(&self) -> Result<ServerState, MetadataStoreError>;
    fn save(&self, state: &ServerState) -> Result<(), MetadataStoreError>;
    fn is_durable(&self) -> bool;
    fn label(&self) -> String;
}

#[derive(Debug, Default)]
struct InMemoryMetadataStore {
    state: Mutex<ServerState>,
}

impl HostedMetadataStore for InMemoryMetadataStore {
    fn load(&self) -> Result<ServerState, MetadataStoreError> {
        Ok(self
            .state
            .lock()
            .expect("metadata store mutex should not be poisoned")
            .clone())
    }

    fn save(&self, state: &ServerState) -> Result<(), MetadataStoreError> {
        *self
            .state
            .lock()
            .expect("metadata store mutex should not be poisoned") = state.clone();
        Ok(())
    }

    fn is_durable(&self) -> bool {
        false
    }

    fn label(&self) -> String {
        "in-memory".to_owned()
    }
}

#[derive(Debug, Clone)]
pub enum SurrealHostedEndpoint {
    LocalSurrealKv {
        path: PathBuf,
    },
    RemoteWs {
        endpoint: String,
        username: String,
        password: String,
    },
}

#[derive(Debug, Clone)]
pub struct SurrealHostedConfig {
    pub endpoint: SurrealHostedEndpoint,
    pub namespace: String,
    pub database: String,
}

impl SurrealHostedConfig {
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Self {
            endpoint: SurrealHostedEndpoint::LocalSurrealKv { path: path.into() },
            namespace: "ke8ygw".to_owned(),
            database: "ham_server".to_owned(),
        }
    }

    pub fn from_env() -> Self {
        let namespace =
            std::env::var("HAM_SERVER_SURREAL_NAMESPACE").unwrap_or_else(|_| "ke8ygw".to_owned());
        let database = std::env::var("HAM_SERVER_SURREAL_DATABASE")
            .unwrap_or_else(|_| "ham_server".to_owned());
        if let Ok(endpoint) = std::env::var("HAM_SERVER_SURREAL_ENDPOINT") {
            return Self {
                endpoint: SurrealHostedEndpoint::RemoteWs {
                    endpoint,
                    username: std::env::var("HAM_SERVER_SURREAL_USER")
                        .unwrap_or_else(|_| "root".to_owned()),
                    password: std::env::var("HAM_SERVER_SURREAL_PASS")
                        .unwrap_or_else(|_| "root".to_owned()),
                },
                namespace,
                database,
            };
        }
        Self {
            endpoint: SurrealHostedEndpoint::LocalSurrealKv {
                path: default_metadata_store_path(),
            },
            namespace,
            database,
        }
    }

    pub fn label(&self) -> String {
        match &self.endpoint {
            SurrealHostedEndpoint::LocalSurrealKv { path } => {
                format!("surrealdb+surrealkv://{}", path.display())
            }
            SurrealHostedEndpoint::RemoteWs { endpoint, .. } => {
                format!("surrealdb+remote://{endpoint}")
            }
        }
    }
}

#[derive(Clone)]
enum SurrealHostedClient {
    Local(Surreal<Db>),
    Remote(Surreal<Any>),
}

impl std::fmt::Debug for SurrealHostedClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(_) => formatter.write_str("Local(Surreal<Db>)"),
            Self::Remote(_) => formatter.write_str("Remote(Surreal<Any>)"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct SurrealHostedMetadataStore {
    runtime: Arc<Mutex<Option<Runtime>>>,
    client: Arc<Mutex<Option<SurrealHostedClient>>>,
    config: SurrealHostedConfig,
}

#[derive(Debug, Deserialize)]
struct PayloadRow<T> {
    payload: T,
}

impl SurrealHostedMetadataStore {
    pub fn open(config: SurrealHostedConfig) -> Result<Self, MetadataStoreError> {
        let (runtime, client) = thread::spawn({
            let config = config.clone();
            move || {
                let runtime = Runtime::new().map_err(MetadataStoreError::Io)?;
                let client = runtime.block_on(async {
                    let client = connect_hosted_surreal(&config).await?;
                    initialize_hosted_schema(&client).await?;
                    Ok::<_, MetadataStoreError>(client)
                })?;
                Ok::<_, MetadataStoreError>((runtime, client))
            }
        })
        .join()
        .map_err(|_| MetadataStoreError::Thread)??;
        Ok(Self {
            runtime: Arc::new(Mutex::new(Some(runtime))),
            client: Arc::new(Mutex::new(Some(client))),
            config,
        })
    }

    pub fn open_local(path: impl Into<PathBuf>) -> Result<Self, MetadataStoreError> {
        Self::open(SurrealHostedConfig::local(path))
    }

    fn run<T, Fut>(
        &self,
        operation: impl FnOnce(SurrealHostedClient) -> Fut + Send + 'static,
    ) -> Result<T, MetadataStoreError>
    where
        T: Send + 'static,
        Fut: std::future::Future<Output = Result<T, MetadataStoreError>> + Send + 'static,
    {
        let runtime = self.runtime.clone();
        let client = self
            .client
            .lock()
            .expect("SurrealDB client mutex should not be poisoned")
            .as_ref()
            .ok_or(MetadataStoreError::Thread)?
            .clone();
        thread::spawn(move || {
            let guard = runtime
                .lock()
                .expect("SurrealDB runtime mutex should not be poisoned");
            let runtime = guard.as_ref().ok_or(MetadataStoreError::Thread)?;
            runtime.block_on(operation(client))
        })
        .join()
        .map_err(|_| MetadataStoreError::Thread)?
    }
}

impl Drop for SurrealHostedMetadataStore {
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

async fn connect_hosted_surreal(
    config: &SurrealHostedConfig,
) -> Result<SurrealHostedClient, MetadataStoreError> {
    match &config.endpoint {
        SurrealHostedEndpoint::LocalSurrealKv { path } => {
            fs::create_dir_all(path)?;
            let db = Surreal::new::<SurrealKv>(path.display().to_string()).await?;
            db.use_ns(&config.namespace)
                .use_db(&config.database)
                .await?;
            Ok(SurrealHostedClient::Local(db))
        }
        SurrealHostedEndpoint::RemoteWs {
            endpoint,
            username,
            password,
        } => {
            let db = Surreal::<Any>::init();
            db.connect(endpoint.as_str()).await?;
            db.signin(Root {
                username: username.clone(),
                password: password.clone(),
            })
            .await?;
            db.use_ns(&config.namespace)
                .use_db(&config.database)
                .await?;
            Ok(SurrealHostedClient::Remote(db))
        }
    }
}

impl HostedMetadataStore for SurrealHostedMetadataStore {
    fn load(&self) -> Result<ServerState, MetadataStoreError> {
        self.run(|client| async move {
            let mut state = ServerState::default();

            for account in select_payloads::<UserAccount>(&client, "users").await? {
                state
                    .users_by_email
                    .insert(account.email.clone(), account.account_id);
                state.accounts.insert(account.account_id, account);
            }
            for session in select_payloads::<LoginSession>(&client, "login_sessions").await? {
                state
                    .sessions_by_token
                    .insert(session.token.clone(), session);
            }
            for device in select_payloads::<DeviceIdentity>(&client, "devices").await? {
                state.devices.insert(device.device_id, device);
            }
            for logbook in select_payloads::<ApiLogbook>(&client, "logbooks").await? {
                state.logbooks.insert(logbook.logbook_id, logbook);
            }
            state.memberships =
                select_payloads::<LogbookMembership>(&client, "logbook_memberships").await?;
            for token in select_payloads::<ApiToken>(&client, "api_tokens").await? {
                state.api_tokens.insert(token.token_id, token);
            }
            for invite in select_payloads::<ServerInvite>(&client, "server_invites").await? {
                state.invites.insert(invite.invite_id, invite);
            }
            for profile in
                select_payloads::<HostedStationProfile>(&client, "station_profiles").await?
            {
                state
                    .station_profiles
                    .insert(profile.profile.station_profile_id, profile);
            }
            for equipment in
                select_payloads::<HostedEquipmentProfile>(&client, "equipment_profiles").await?
            {
                state
                    .equipment_profiles
                    .insert(equipment.equipment.equipment_id, equipment);
            }
            for setting in
                select_payloads::<HostedProviderSetting>(&client, "provider_settings").await?
            {
                state.provider_settings.insert(
                    provider_setting_key(setting.logbook_id, &setting.provider_id),
                    setting,
                );
            }
            for job in select_payloads::<HostedUploadJob>(&client, "upload_queue_history").await? {
                state.upload_jobs.insert(job.upload_id, job);
            }
            for settings in select_payloads::<HostedMapSettings>(&client, "map_settings").await? {
                state.map_settings.insert(settings.logbook_id, settings);
            }
            for backup in select_payloads::<HostedBackupRecord>(&client, "backup_records").await? {
                state.backups.insert(backup.backup_id, backup);
            }
            for report in
                select_payloads::<HostedDivergenceReport>(&client, "divergence_reports").await?
            {
                state.divergence_reports.insert(report.report_id, report);
            }

            Ok(state)
        })
    }

    fn save(&self, state: &ServerState) -> Result<(), MetadataStoreError> {
        let state = state.clone();
        self.run(move |client| async move {
            for table in [
                "users",
                "login_sessions",
                "devices",
                "logbooks",
                "logbook_memberships",
                "api_tokens",
                "server_invites",
                "station_profiles",
                "equipment_profiles",
                "provider_settings",
                "upload_queue_history",
                "map_settings",
                "backup_records",
                "divergence_reports",
            ] {
                delete_table(&client, table).await?;
            }

            for account in state.accounts.values() {
                create_record(
                    &client,
                    "users",
                    account.account_id.to_string(),
                    json!({
                        "account_id": account.account_id,
                        "user_id": account.user_id,
                        "email": account.email,
                        "payload": account,
                    }),
                )
                .await?;
            }
            for session in state.sessions_by_token.values() {
                create_record(
                    &client,
                    "login_sessions",
                    session_token_hash(&session.token),
                    json!({
                        "account_id": session.account_id,
                        "user_id": session.user_id,
                        "device_id": session.device_id,
                        "active": session.active,
                        "token_hash": session_token_hash(&session.token),
                        "payload": session,
                    }),
                )
                .await?;
            }
            for device in state.devices.values() {
                create_record(
                    &client,
                    "devices",
                    device.device_id.to_string(),
                    json!({
                        "account_id": device.account_id,
                        "user_id": device.user_id,
                        "device_id": device.device_id,
                        "revoked": device.revoked,
                        "payload": device,
                    }),
                )
                .await?;
            }
            for logbook in state.logbooks.values() {
                create_record(
                    &client,
                    "logbooks",
                    logbook.logbook_id.to_string(),
                    json!({
                        "account_id": logbook.account_id,
                        "logbook_id": logbook.logbook_id,
                        "payload": logbook,
                    }),
                )
                .await?;
            }
            for membership in &state.memberships {
                create_record(
                    &client,
                    "logbook_memberships",
                    format!("{}-{}", membership.logbook_id, membership.user_id),
                    json!({
                        "account_id": membership.account_id,
                        "logbook_id": membership.logbook_id,
                        "user_id": membership.user_id,
                        "role": membership.role,
                        "payload": membership,
                    }),
                )
                .await?;
            }
            for token in state.api_tokens.values() {
                create_record(
                    &client,
                    "api_tokens",
                    token.token_id.to_string(),
                    json!({
                        "account_id": token.account_id,
                        "user_id": token.user_id,
                        "device_id": token.device_id,
                        "revoked": token.revoked,
                        "payload": token,
                    }),
                )
                .await?;
            }
            for invite in state.invites.values() {
                create_record(
                    &client,
                    "server_invites",
                    invite.invite_id.to_string(),
                    json!({
                        "account_id": invite.account_id,
                        "logbook_id": invite.logbook_id,
                        "token": invite.token,
                        "payload": invite,
                    }),
                )
                .await?;
            }
            for profile in state.station_profiles.values() {
                create_record(
                    &client,
                    "station_profiles",
                    profile.profile.station_profile_id.to_string(),
                    json!({
                        "account_id": profile.account_id,
                        "logbook_id": profile.logbook_id,
                        "station_profile_id": profile.profile.station_profile_id,
                        "payload": profile,
                    }),
                )
                .await?;
            }
            for equipment in state.equipment_profiles.values() {
                create_record(
                    &client,
                    "equipment_profiles",
                    equipment.equipment.equipment_id.to_string(),
                    json!({
                        "account_id": equipment.account_id,
                        "logbook_id": equipment.logbook_id,
                        "equipment_id": equipment.equipment.equipment_id,
                        "station_profile_id": equipment.station_profile_id,
                        "payload": equipment,
                    }),
                )
                .await?;
            }
            for setting in state.provider_settings.values() {
                create_record(
                    &client,
                    "provider_settings",
                    provider_setting_key(setting.logbook_id, &setting.provider_id),
                    json!({
                        "account_id": setting.account_id,
                        "logbook_id": setting.logbook_id,
                        "provider_id": setting.provider_id,
                        "enabled": setting.enabled,
                        "credential_id": setting.credential_id,
                        "payload": setting,
                    }),
                )
                .await?;
            }
            for job in state.upload_jobs.values() {
                create_record(
                    &client,
                    "upload_queue_history",
                    job.upload_id.to_string(),
                    json!({
                        "account_id": job.account_id,
                        "logbook_id": job.logbook_id,
                        "provider_id": job.provider_id,
                        "upload_id": job.upload_id,
                        "status": job.status,
                        "payload": job,
                    }),
                )
                .await?;
            }
            for settings in state.map_settings.values() {
                create_record(
                    &client,
                    "map_settings",
                    settings.logbook_id.to_string(),
                    json!({
                        "account_id": settings.account_id,
                        "logbook_id": settings.logbook_id,
                        "payload": settings,
                    }),
                )
                .await?;
            }
            for backup in state.backups.values() {
                create_record(
                    &client,
                    "backup_records",
                    backup.backup_id.to_string(),
                    json!({
                        "account_id": backup.account_id,
                        "logbook_id": backup.logbook_id,
                        "backup_id": backup.backup_id,
                        "payload": backup,
                    }),
                )
                .await?;
            }
            for report in state.divergence_reports.values() {
                create_record(
                    &client,
                    "divergence_reports",
                    report.report_id.to_string(),
                    json!({
                        "account_id": report.account_id,
                        "logbook_id": report.logbook_id,
                        "report_id": report.report_id,
                        "payload": report,
                    }),
                )
                .await?;
            }
            Ok(())
        })
    }

    fn is_durable(&self) -> bool {
        true
    }

    fn label(&self) -> String {
        self.config.label()
    }
}

async fn initialize_hosted_schema(client: &SurrealHostedClient) -> Result<(), MetadataStoreError> {
    let schema = r#"
        DEFINE TABLE IF NOT EXISTS schema_migrations SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS users SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS login_sessions SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS devices SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS logbooks SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS logbook_memberships SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS api_tokens SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS server_invites SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS station_profiles SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS equipment_profiles SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS provider_settings SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS upload_queue_history SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS map_settings SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS backup_records SCHEMALESS;
        DEFINE TABLE IF NOT EXISTS divergence_reports SCHEMALESS;
        DEFINE INDEX IF NOT EXISTS users_email_idx ON TABLE users COLUMNS email UNIQUE;
        DEFINE INDEX IF NOT EXISTS users_account_idx ON TABLE users COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS sessions_token_hash_idx ON TABLE login_sessions COLUMNS token_hash UNIQUE;
        DEFINE INDEX IF NOT EXISTS sessions_account_idx ON TABLE login_sessions COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS sessions_device_idx ON TABLE login_sessions COLUMNS device_id;
        DEFINE INDEX IF NOT EXISTS devices_account_idx ON TABLE devices COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS devices_device_idx ON TABLE devices COLUMNS device_id;
        DEFINE INDEX IF NOT EXISTS logbooks_account_idx ON TABLE logbooks COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS memberships_account_idx ON TABLE logbook_memberships COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS memberships_logbook_idx ON TABLE logbook_memberships COLUMNS logbook_id;
        DEFINE INDEX IF NOT EXISTS api_tokens_account_idx ON TABLE api_tokens COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS invites_account_idx ON TABLE server_invites COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS station_profiles_account_idx ON TABLE station_profiles COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS station_profiles_logbook_idx ON TABLE station_profiles COLUMNS logbook_id;
        DEFINE INDEX IF NOT EXISTS equipment_profiles_account_idx ON TABLE equipment_profiles COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS equipment_profiles_logbook_idx ON TABLE equipment_profiles COLUMNS logbook_id;
        DEFINE INDEX IF NOT EXISTS provider_settings_account_idx ON TABLE provider_settings COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS provider_settings_logbook_idx ON TABLE provider_settings COLUMNS logbook_id;
        DEFINE INDEX IF NOT EXISTS provider_settings_provider_idx ON TABLE provider_settings COLUMNS provider_id;
        DEFINE INDEX IF NOT EXISTS upload_queue_account_idx ON TABLE upload_queue_history COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS upload_queue_logbook_idx ON TABLE upload_queue_history COLUMNS logbook_id;
        DEFINE INDEX IF NOT EXISTS upload_queue_provider_idx ON TABLE upload_queue_history COLUMNS provider_id;
        DEFINE INDEX IF NOT EXISTS map_settings_account_idx ON TABLE map_settings COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS map_settings_logbook_idx ON TABLE map_settings COLUMNS logbook_id;
        DEFINE INDEX IF NOT EXISTS backup_records_account_idx ON TABLE backup_records COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS backup_records_logbook_idx ON TABLE backup_records COLUMNS logbook_id;
        DEFINE INDEX IF NOT EXISTS divergence_reports_account_idx ON TABLE divergence_reports COLUMNS account_id;
        DEFINE INDEX IF NOT EXISTS divergence_reports_logbook_idx ON TABLE divergence_reports COLUMNS logbook_id;
        UPSERT schema_migrations:hosted_v3 SET version = 3, component = 'ham-server', applied_at = time::now();
    "#;
    query_checked(client, schema).await
}

async fn query_checked(
    client: &SurrealHostedClient,
    query: &str,
) -> Result<(), MetadataStoreError> {
    match client {
        SurrealHostedClient::Local(db) => {
            db.query(query).await?.check()?;
        }
        SurrealHostedClient::Remote(db) => {
            db.query(query).await?.check()?;
        }
    }
    Ok(())
}

async fn delete_table(
    client: &SurrealHostedClient,
    table: &'static str,
) -> Result<(), MetadataStoreError> {
    match client {
        SurrealHostedClient::Local(db) => {
            let _: Vec<SurrealDbValue> = db.delete(table).await?;
        }
        SurrealHostedClient::Remote(db) => {
            let _: Vec<SurrealDbValue> = db.delete(table).await?;
        }
    }
    Ok(())
}

async fn create_record(
    client: &SurrealHostedClient,
    table: &'static str,
    id: String,
    content: Value,
) -> Result<(), MetadataStoreError> {
    match client {
        SurrealHostedClient::Local(db) => {
            let _: Option<SurrealDbValue> =
                db.upsert((table, id.as_str())).content(content).await?;
        }
        SurrealHostedClient::Remote(db) => {
            let _: Option<SurrealDbValue> =
                db.upsert((table, id.as_str())).content(content).await?;
        }
    }
    Ok(())
}

async fn select_payloads<T: for<'de> Deserialize<'de>>(
    client: &SurrealHostedClient,
    table: &'static str,
) -> Result<Vec<T>, MetadataStoreError> {
    let query = format!("SELECT * FROM {table};");
    let rows: Vec<SurrealDbValue> = match client {
        SurrealHostedClient::Local(db) => {
            let mut response = db.query(query.as_str()).await?;
            response.take(0)?
        }
        SurrealHostedClient::Remote(db) => {
            let mut response = db.query(query.as_str()).await?;
            response.take(0)?
        }
    };
    rows.into_iter()
        .map(|row| {
            serde_json::from_value::<PayloadRow<T>>(row.into_json_value())
                .map(|row| row.payload)
                .map_err(MetadataStoreError::Serde)
        })
        .collect()
}

fn session_token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

#[derive(Clone)]
pub struct HostedServer {
    state: Arc<RwLock<ServerState>>,
    metadata_store: Arc<dyn HostedMetadataStore>,
    store: Arc<dyn LogbookEventStore>,
    bus: Arc<InMemoryEventBus>,
}

pub fn default_metadata_store_path() -> PathBuf {
    std::env::var("HAM_SERVER_SURREAL_PATH").map_or_else(
        |_| default_log_directory().join("server").join("surrealdb"),
        PathBuf::from,
    )
}

pub fn default_server_official_event_log_path() -> PathBuf {
    std::env::var("HAM_SERVER_EVENT_LOG_PATH").map_or_else(
        |_| {
            default_log_directory()
                .join("server")
                .join("official-events.jsonl")
        },
        PathBuf::from,
    )
}

impl Default for HostedServer {
    fn default() -> Self {
        Self::new()
    }
}

impl HostedServer {
    pub fn new() -> Self {
        Self::new_in_memory()
    }

    pub fn new_in_memory() -> Self {
        let metadata_store = Arc::new(InMemoryMetadataStore::default());
        Self::with_metadata_store(metadata_store).expect("in-memory metadata store should load")
    }

    pub fn with_surreal_metadata(path: impl Into<PathBuf>) -> Result<Self, MetadataStoreError> {
        Self::with_surreal_paths(path, default_server_official_event_log_path())
    }

    pub fn with_surreal_paths(
        metadata_path: impl Into<PathBuf>,
        official_event_log_path: impl Into<PathBuf>,
    ) -> Result<Self, MetadataStoreError> {
        let official_store = Arc::new(
            JsonlLogbookEventStore::open(official_event_log_path)
                .map_err(|error| MetadataStoreError::OfficialStore(error.to_string()))?,
        );
        let metadata_store = Arc::new(SurrealHostedMetadataStore::open_local(metadata_path)?);
        Self::with_metadata_and_event_store(metadata_store, official_store)
    }

    pub fn with_surreal_metadata_only(
        path: impl Into<PathBuf>,
    ) -> Result<Self, MetadataStoreError> {
        let metadata_store = Arc::new(SurrealHostedMetadataStore::open_local(path)?);
        Self::with_metadata_store(metadata_store)
    }

    pub fn with_surreal_config(config: SurrealHostedConfig) -> Result<Self, MetadataStoreError> {
        let official_store = Arc::new(
            JsonlLogbookEventStore::open(default_server_official_event_log_path())
                .map_err(|error| MetadataStoreError::OfficialStore(error.to_string()))?,
        );
        let metadata_store = Arc::new(SurrealHostedMetadataStore::open(config)?);
        Self::with_metadata_and_event_store(metadata_store, official_store)
    }

    fn with_metadata_store(
        metadata_store: Arc<dyn HostedMetadataStore>,
    ) -> Result<Self, MetadataStoreError> {
        let state = metadata_store.load()?;
        Ok(Self {
            state: Arc::new(RwLock::new(state)),
            metadata_store,
            store: Arc::new(InMemoryLogbookEventStore::new()),
            bus: Arc::new(InMemoryEventBus::new(256)),
        })
    }

    fn with_metadata_and_event_store(
        metadata_store: Arc<dyn HostedMetadataStore>,
        store: Arc<dyn LogbookEventStore>,
    ) -> Result<Self, MetadataStoreError> {
        let state = metadata_store.load()?;
        Ok(Self {
            state: Arc::new(RwLock::new(state)),
            metadata_store,
            store,
            bus: Arc::new(InMemoryEventBus::new(256)),
        })
    }

    pub async fn handle(&self, request: ApiRequest) -> ApiResponse {
        match self.route(request).await {
            Ok(response) => json_response(200, &response),
            Err(ApiError::BadRequest(message)) => json_error(400, message),
            Err(
                ApiError::Unauthenticated | ApiError::InactiveSession | ApiError::RevokedDevice,
            ) => json_error(401, "unauthenticated"),
            Err(ApiError::Forbidden) => json_error(403, "forbidden"),
            Err(ApiError::NotFound) => json_error(404, "not found"),
            Err(ApiError::Proposal(message) | ApiError::Store(message)) => json_error(400, message),
        }
    }

    async fn route(&self, request: ApiRequest) -> Result<Value, ApiError> {
        let segments = path_segments(&request.path);
        match (request.method.as_str(), segments.as_slice()) {
            ("GET", ["health"]) => Ok(json!({
                "ok": true,
                "service": "ke8ygw-ham-server",
                "version": env!("CARGO_PKG_VERSION")
            })),
            ("GET", ["api", "v1", "status"]) => self.status().await,
            ("GET", ["api", "v1", "routes"]) => Ok(json!(route_catalog())),
            ("POST", ["api", "v1", "auth", "login"]) => self.login(&request.body).await,
            ("POST", ["api", "v1", "auth", "logout"]) => self.logout(&request).await,
            ("GET", ["api", "v1", "auth", "session"]) => self.session(&request).await,
            ("GET", ["api", "v1", "logbooks"]) => self.list_logbooks(&request).await,
            ("POST", ["api", "v1", "logbooks"]) => self.create_logbook(&request).await,
            ("GET", ["api", "v1", "logbooks", logbook_id]) => {
                self.get_logbook(&request, logbook_id).await
            }
            ("PATCH", ["api", "v1", "logbooks", logbook_id]) => {
                self.patch_logbook(&request, logbook_id).await
            }
            ("GET", ["api", "v1", "qsos"]) => self.list_qsos(&request).await,
            ("POST", ["api", "v1", "qsos"]) => self.create_qso(&request).await,
            ("GET", ["api", "v1", "qsos", qso_id]) => self.get_qso(&request, qso_id).await,
            ("PATCH", ["api", "v1", "qsos", qso_id]) => self.patch_qso(&request, qso_id).await,
            ("POST", ["api", "v1", "qsos", qso_id, "delete"]) => {
                self.qso_action(&request, qso_id, PROPOSAL_QSO_DELETE).await
            }
            ("POST", ["api", "v1", "qsos", qso_id, "restore"]) => {
                self.qso_action(&request, qso_id, PROPOSAL_QSO_RESTORE)
                    .await
            }
            ("POST", ["api", "v1", "qsos", qso_id, "notes"]) => {
                self.qso_note(&request, qso_id).await
            }
            ("GET", ["api", "v1", "station-profiles"]) => {
                self.list_station_profiles(&request).await
            }
            ("POST", ["api", "v1", "station-profiles"]) => {
                self.create_station_profile(&request).await
            }
            ("GET", ["api", "v1", "station-profiles", profile_id]) => {
                self.get_station_profile(&request, profile_id).await
            }
            ("PATCH", ["api", "v1", "station-profiles", profile_id]) => {
                self.patch_station_profile(&request, profile_id).await
            }
            ("POST", ["api", "v1", "station-profiles", profile_id, "archive"]) => {
                self.archive_station_profile(&request, profile_id).await
            }
            ("POST", ["api", "v1", "station-profiles", profile_id, "set-default"]) => {
                self.set_default_station_profile(&request, profile_id).await
            }
            ("GET", ["api", "v1", "equipment"]) => self.list_equipment(&request).await,
            ("POST", ["api", "v1", "equipment"]) => self.create_equipment(&request).await,
            ("GET", ["api", "v1", "equipment", equipment_id]) => {
                self.get_equipment(&request, equipment_id).await
            }
            ("PATCH", ["api", "v1", "equipment", equipment_id]) => {
                self.patch_equipment(&request, equipment_id).await
            }
            ("POST", ["api", "v1", "equipment", equipment_id, "archive"]) => {
                self.archive_equipment(&request, equipment_id).await
            }
            ("POST", ["api", "v1", "adif", "import"]) => self.import_adif_route(&request).await,
            ("GET", ["api", "v1", "adif", "export"]) => self.export_adif_route(&request).await,
            ("GET", ["api", "v1", "activations"]) => self.list_activations(&request).await,
            ("POST", ["api", "v1", "activations"]) => self.create_activation(&request).await,
            ("GET", ["api", "v1", "activations", activation_id]) => {
                self.get_activation(&request, activation_id).await
            }
            ("PATCH", ["api", "v1", "activations", activation_id]) => {
                self.patch_activation(&request, activation_id).await
            }
            ("POST", ["api", "v1", "activations", activation_id, "end"]) => {
                self.end_activation(&request, activation_id).await
            }
            ("GET", ["api", "v1", "activations", activation_id, "qsos"]) => {
                self.activation_qsos(&request, activation_id).await
            }
            ("GET", ["api", "v1", "net-control", "sessions"]) => {
                self.list_net_sessions(&request).await
            }
            ("POST", ["api", "v1", "net-control", "sessions"]) => {
                self.create_net_session(&request).await
            }
            ("GET", ["api", "v1", "net-control", "sessions", session_id]) => {
                self.get_net_session(&request, session_id).await
            }
            ("PATCH", ["api", "v1", "net-control", "sessions", session_id]) => {
                self.patch_net_session(&request, session_id).await
            }
            ("POST", ["api", "v1", "net-control", "sessions", session_id, "start"]) => {
                self.create_net_session_with_path(&request, Some(session_id))
                    .await
            }
            ("POST", ["api", "v1", "net-control", "sessions", session_id, "end"]) => {
                self.end_net_session(&request, session_id).await
            }
            ("POST", ["api", "v1", "net-control", "sessions", session_id, "checkins"]) => {
                self.create_net_checkin(&request, session_id).await
            }
            (
                "PATCH",
                ["api", "v1", "net-control", "sessions", session_id, "checkins", checkin_id],
            ) => {
                self.patch_net_checkin(&request, session_id, checkin_id)
                    .await
            }
            ("POST", ["api", "v1", "net-control", "sessions", session_id, "traffic"]) => {
                self.create_net_traffic(&request, session_id).await
            }
            ("GET", ["api", "v1", "maps", "qsos"]) => self.map_qsos(&request).await,
            ("GET", ["api", "v1", "maps", "stations"]) => self.map_stations(&request).await,
            ("GET", ["api", "v1", "maps", "paths"]) => self.map_paths(&request).await,
            ("GET", ["api", "v1", "maps", "settings"]) => self.map_settings(&request).await,
            ("PATCH", ["api", "v1", "maps", "settings"]) => self.patch_map_settings(&request).await,
            ("POST", ["api", "v1", "backups", "export"]) => self.export_backup(&request).await,
            ("GET", ["api", "v1", "backups"]) => self.list_backups(&request).await,
            ("GET", ["api", "v1", "backups", backup_id]) => {
                self.get_backup(&request, backup_id).await
            }
            ("GET", ["api", "v1", "backups", backup_id, "download"]) => {
                self.download_backup(&request, backup_id).await
            }
            ("POST", ["api", "v1", "backups", "import", "dry-run"]) => {
                self.backup_import_dry_run(&request).await
            }
            ("POST", ["api", "v1", "backups", "import"]) => self.backup_import(&request).await,
            ("GET", ["api", "v1", "providers"]) => self.providers(&request).await,
            ("GET", ["api", "v1", "providers", provider_id]) => {
                self.provider_detail(&request, provider_id).await
            }
            ("PATCH", ["api", "v1", "providers", provider_id]) => {
                self.patch_provider(&request, provider_id).await
            }
            ("POST", ["api", "v1", "providers", provider_id, "test"]) => {
                self.test_provider(&request, provider_id).await
            }
            ("GET", ["api", "v1", "uploads"]) => self.list_uploads(&request).await,
            ("POST", ["api", "v1", "uploads", "run"]) => self.run_upload(&request).await,
            ("POST", ["api", "v1", "uploads", upload_id, "retry"]) => {
                self.retry_upload(&request, upload_id).await
            }
            ("GET", ["api", "v1", "sync", "status"]) => self.sync_status(&request).await,
            ("POST", ["api", "v1", "sync", "preview"]) => self.sync_preview(&request).await,
            ("POST", ["api", "v1", "sync", "push"]) => self.sync_push(&request).await,
            ("POST", ["api", "v1", "sync", "pull"]) => self.sync_pull(&request).await,
            ("POST", ["api", "v1", "sync", "divergence", "review"]) => {
                self.divergence_review(&request).await
            }
            ("GET", ["api", "v1", "sync", "divergence", report_id]) => {
                self.get_divergence_report(&request, report_id).await
            }
            ("POST", ["api", "v1", "sync", "divergence", report_id, "export"]) => {
                self.export_divergence_report(&request, report_id).await
            }
            ("GET", ["api", "v1", "devices"]) => self.devices(&request).await,
            ("POST", ["api", "v1", "devices"]) => self.register_device(&request).await,
            ("POST", ["api", "v1", "devices", device_id, "revoke"]) => {
                self.revoke_device(&request, device_id).await
            }
            _ if is_scaffolded_route(&request.method, &segments) => self.scaffolded(&request).await,
            _ => Err(ApiError::NotFound),
        }
    }

    async fn status(&self) -> Result<Value, ApiError> {
        let state = self.state.read().await;
        Ok(json!({
            "ok": true,
            "api_version": "v1",
            "mode": "hosted_beta",
            "accounts": state.accounts.len(),
            "logbooks": state.logbooks.len(),
            "sessions": state.sessions_by_token.values().filter(|session| session.active).count(),
            "invites": state.invites.len(),
            "api_tokens": state.api_tokens.len(),
            "durable_server_storage": self.metadata_store.is_durable(),
            "metadata_store": self.metadata_store.label(),
            "ios_release_target": "v1.1_native_swiftui"
        }))
    }

    async fn login(&self, body: &[u8]) -> Result<Value, ApiError> {
        let request: LoginRequest = parse_json(body)?;
        let email = request.email.trim().to_ascii_lowercase();
        if email.is_empty() {
            return Err(ApiError::BadRequest("email is required".to_owned()));
        }

        let mut state = self.state.write().await;
        let now = Utc::now();
        let account = if let Some(account_id) = state.users_by_email.get(&email).copied() {
            state
                .accounts
                .get(&account_id)
                .cloned()
                .ok_or(ApiError::NotFound)?
        } else {
            let account = UserAccount {
                account_id: Uuid::new_v4(),
                user_id: Uuid::new_v4(),
                email: email.clone(),
                display_name: request
                    .display_name
                    .clone()
                    .unwrap_or_else(|| email.clone()),
                created_at: now,
            };
            let logbook = ApiLogbook {
                logbook_id: Uuid::new_v4(),
                account_id: account.account_id,
                name: format!("{} Logbook", account.display_name),
                description: Some("Hosted beta default logbook".to_owned()),
                station_callsign: None,
                created_at: now,
                updated_at: now,
            };
            state
                .users_by_email
                .insert(email.clone(), account.account_id);
            state.logbooks.insert(logbook.logbook_id, logbook.clone());
            state.memberships.push(LogbookMembership {
                account_id: account.account_id,
                logbook_id: logbook.logbook_id,
                user_id: account.user_id,
                role: LogbookRole::Owner,
                created_at: now,
            });
            state.accounts.insert(account.account_id, account.clone());
            account
        };

        let device = DeviceIdentity {
            device_id: Uuid::new_v4(),
            account_id: account.account_id,
            user_id: account.user_id,
            device_name: request
                .device_name
                .unwrap_or_else(|| "Hosted web session".to_owned()),
            fingerprint: format!("dev-{}", Uuid::new_v4()),
            trusted: true,
            revoked: false,
            registered_at: now,
            revoked_at: None,
        };
        let token = format!("api-{}-{}", account.account_id, Uuid::new_v4());
        let session = LoginSession {
            session_id: Uuid::new_v4(),
            account_id: account.account_id,
            user_id: account.user_id,
            device_id: device.device_id,
            token,
            issued_at: now,
            expires_at: None,
            active: true,
        };
        state.devices.insert(device.device_id, device.clone());
        state
            .sessions_by_token
            .insert(session.token.clone(), session.clone());
        let logbooks = visible_logbooks(&state, account.user_id);
        self.persist_metadata(&state)?;
        Ok(json!(LoginResponse {
            account,
            session,
            device,
            logbooks
        }))
    }

    async fn logout(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let token = bearer_token(request).ok_or(ApiError::Unauthenticated)?;
        let mut state = self.state.write().await;
        let session = state
            .sessions_by_token
            .get_mut(&token)
            .ok_or(ApiError::Unauthenticated)?;
        session.active = false;
        self.persist_metadata(&state)?;
        Ok(json!({"ok": true}))
    }

    async fn session(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let auth = self.authorize(request).await?;
        let state = self.state.read().await;
        let account = state
            .accounts
            .get(&auth.session.account_id)
            .cloned()
            .ok_or(ApiError::Unauthenticated)?;
        let device = state
            .devices
            .get(&auth.session.device_id)
            .cloned()
            .ok_or(ApiError::Unauthenticated)?;
        let memberships = state
            .memberships
            .iter()
            .filter(|membership| membership.user_id == auth.session.user_id)
            .cloned()
            .collect();
        Ok(json!(SessionResponse {
            account,
            session: auth.session,
            device,
            memberships
        }))
    }

    async fn list_logbooks(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let auth = self.authorize(request).await?;
        let state = self.state.read().await;
        Ok(json!({
            "logbooks": visible_logbooks(&state, auth.session.user_id)
        }))
    }

    async fn create_logbook(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let auth = self.authorize(request).await?;
        let input: CreateLogbookRequest = parse_json(&request.body)?;
        if input.name.trim().is_empty() {
            return Err(ApiError::BadRequest("name is required".to_owned()));
        }
        let mut state = self.state.write().await;
        let now = Utc::now();
        let logbook = ApiLogbook {
            logbook_id: Uuid::new_v4(),
            account_id: auth.session.account_id,
            name: input.name,
            description: input.description,
            station_callsign: input.station_callsign,
            created_at: now,
            updated_at: now,
        };
        state.logbooks.insert(logbook.logbook_id, logbook.clone());
        state.memberships.push(LogbookMembership {
            account_id: auth.session.account_id,
            logbook_id: logbook.logbook_id,
            user_id: auth.session.user_id,
            role: LogbookRole::Owner,
            created_at: now,
        });
        self.persist_metadata(&state)?;
        Ok(json!({"logbook": logbook}))
    }

    async fn get_logbook(&self, request: &ApiRequest, logbook_id: &str) -> Result<Value, ApiError> {
        let logbook_id = parse_uuid(logbook_id)?;
        let auth = self
            .require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let state = self.state.read().await;
        let logbook = state
            .logbooks
            .get(&auth.logbook_id)
            .cloned()
            .ok_or(ApiError::NotFound)?;
        Ok(json!({"logbook": logbook, "role": auth.role}))
    }

    async fn patch_logbook(
        &self,
        request: &ApiRequest,
        logbook_id: &str,
    ) -> Result<Value, ApiError> {
        let logbook_id = parse_uuid(logbook_id)?;
        self.require_logbook_role(request, logbook_id, LogbookAccess::Admin)
            .await?;
        let input: UpdateLogbookRequest = parse_json(&request.body)?;
        let mut state = self.state.write().await;
        let logbook = state
            .logbooks
            .get_mut(&logbook_id)
            .ok_or(ApiError::NotFound)?;
        if let Some(name) = input.name {
            if name.trim().is_empty() {
                return Err(ApiError::BadRequest("name must not be empty".to_owned()));
            }
            logbook.name = name;
        }
        if input.description.is_some() {
            logbook.description = input.description;
        }
        if input.station_callsign.is_some() {
            logbook.station_callsign = input.station_callsign;
        }
        logbook.updated_at = Utc::now();
        let logbook = logbook.clone();
        self.persist_metadata(&state)?;
        Ok(json!({"logbook": logbook}))
    }

    async fn list_qsos(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let logbook_id = logbook_id_from_query(request)?;
        self.require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let include_deleted = request
            .query
            .get("include_deleted")
            .is_some_and(|value| value == "true");
        let projection = self
            .store
            .rebuild_projections(logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let qsos = projection
            .list(include_deleted)
            .into_iter()
            .map(qso_response)
            .collect::<Vec<_>>();
        Ok(json!(QsoListResponse { logbook_id, qsos }))
    }

    async fn create_qso(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let input: QsoWriteRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::LogQso)
            .await?;
        let payload = qso_create_payload(input)?;
        self.submit_qso_proposal(auth, PROPOSAL_QSO_CREATE, None, payload)
            .await
    }

    async fn get_qso(&self, request: &ApiRequest, qso_id: &str) -> Result<Value, ApiError> {
        let qso_id = parse_uuid(qso_id)?;
        let logbook_id = logbook_id_from_query(request)?;
        self.require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let projection = self
            .store
            .rebuild_projections(logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let qso = projection
            .get_including_deleted(qso_id)
            .map(qso_response)
            .ok_or(ApiError::NotFound)?;
        Ok(json!({"qso": qso}))
    }

    async fn patch_qso(&self, request: &ApiRequest, qso_id: &str) -> Result<Value, ApiError> {
        let qso_id = parse_uuid(qso_id)?;
        let input: QsoWriteRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::LogQso)
            .await?;
        let payload = qso_patch_payload(input)?;
        self.submit_qso_proposal(auth, PROPOSAL_QSO_CORRECT, Some(qso_id), payload)
            .await
    }

    async fn qso_action(
        &self,
        request: &ApiRequest,
        qso_id: &str,
        proposal_type: &str,
    ) -> Result<Value, ApiError> {
        let qso_id = parse_uuid(qso_id)?;
        let input: QsoActionRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::LogQso)
            .await?;
        self.submit_qso_proposal(auth, proposal_type, Some(qso_id), json!({}))
            .await
    }

    async fn qso_note(&self, request: &ApiRequest, qso_id: &str) -> Result<Value, ApiError> {
        let qso_id = parse_uuid(qso_id)?;
        let input: QsoActionRequest = parse_json(&request.body)?;
        let note = input
            .note
            .filter(|note| !note.trim().is_empty())
            .ok_or_else(|| ApiError::BadRequest("note is required".to_owned()))?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::LogQso)
            .await?;
        self.submit_qso_proposal(
            auth,
            PROPOSAL_QSO_NOTE_ADD,
            Some(qso_id),
            json!({ "note": note }),
        )
        .await
    }

    async fn list_station_profiles(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let logbook_id = logbook_id_from_query(request)?;
        let auth = self
            .require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let state = self.state.read().await;
        let mut profiles = state
            .station_profiles
            .values()
            .filter(|profile| {
                profile.account_id == auth.session.account_id && profile.logbook_id == logbook_id
            })
            .cloned()
            .collect::<Vec<_>>();
        profiles.sort_by(|left, right| {
            left.profile
                .display_name
                .cmp(&right.profile.display_name)
                .then_with(|| {
                    left.profile
                        .station_profile_id
                        .cmp(&right.profile.station_profile_id)
                })
        });
        Ok(json!({"station_profiles": profiles}))
    }

    async fn create_station_profile(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let input: StationProfileRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let mut profile = station_profile_from_request(&input)?;
        profile.account_id = Some(auth.session.account_id.to_string());
        let now = Utc::now();
        profile.created_at = now;
        profile.updated_at = now;
        let mut hosted = HostedStationProfile {
            account_id: auth.session.account_id,
            logbook_id: input.logbook_id,
            is_default: input.active.unwrap_or(false),
            profile,
        };
        let mut state = self.state.write().await;
        if state
            .station_profiles
            .values()
            .all(|existing| existing.logbook_id != input.logbook_id)
            || hosted.is_default
        {
            for existing in state
                .station_profiles
                .values_mut()
                .filter(|existing| existing.logbook_id == input.logbook_id)
            {
                existing.is_default = false;
                existing.profile.active = false;
            }
            hosted.is_default = true;
            hosted.profile.active = true;
        }
        state
            .station_profiles
            .insert(hosted.profile.station_profile_id, hosted.clone());
        self.persist_metadata(&state)?;
        Ok(json!({"station_profile": hosted}))
    }

    async fn get_station_profile(
        &self,
        request: &ApiRequest,
        profile_id: &str,
    ) -> Result<Value, ApiError> {
        let profile_id = parse_uuid(profile_id)?;
        let logbook_id = logbook_id_from_query(request)?;
        let auth = self
            .require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let state = self.state.read().await;
        let profile = state
            .station_profiles
            .get(&profile_id)
            .filter(|profile| {
                profile.account_id == auth.session.account_id && profile.logbook_id == logbook_id
            })
            .cloned()
            .ok_or(ApiError::NotFound)?;
        Ok(json!({"station_profile": profile}))
    }

    async fn patch_station_profile(
        &self,
        request: &ApiRequest,
        profile_id: &str,
    ) -> Result<Value, ApiError> {
        let profile_id = parse_uuid(profile_id)?;
        let input: StationProfileRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let mut state = self.state.write().await;
        {
            let existing = state
                .station_profiles
                .get(&profile_id)
                .filter(|profile| {
                    profile.account_id == auth.session.account_id
                        && profile.logbook_id == input.logbook_id
                })
                .ok_or(ApiError::NotFound)?;
            validate_station_patch(&input, existing)?;
        }
        if input.active == Some(true) {
            for other in state
                .station_profiles
                .values_mut()
                .filter(|profile| profile.logbook_id == input.logbook_id)
            {
                other.is_default = false;
                other.profile.active = false;
            }
        }
        let existing = state
            .station_profiles
            .get_mut(&profile_id)
            .ok_or(ApiError::NotFound)?;
        apply_station_patch(existing, input);
        let profile = existing.clone();
        self.persist_metadata(&state)?;
        Ok(json!({"station_profile": profile}))
    }

    async fn archive_station_profile(
        &self,
        request: &ApiRequest,
        profile_id: &str,
    ) -> Result<Value, ApiError> {
        let profile_id = parse_uuid(profile_id)?;
        let input: QsoActionRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let mut state = self.state.write().await;
        let profile = state
            .station_profiles
            .get_mut(&profile_id)
            .filter(|profile| {
                profile.account_id == auth.session.account_id
                    && profile.logbook_id == input.logbook_id
            })
            .ok_or(ApiError::NotFound)?;
        profile.is_default = false;
        profile.profile.active = false;
        profile.profile.updated_at = Utc::now();
        let profile = profile.clone();
        self.persist_metadata(&state)?;
        Ok(json!({"station_profile": profile}))
    }

    async fn set_default_station_profile(
        &self,
        request: &ApiRequest,
        profile_id: &str,
    ) -> Result<Value, ApiError> {
        let profile_id = parse_uuid(profile_id)?;
        let input: QsoActionRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let mut state = self.state.write().await;
        if !state.station_profiles.values().any(|profile| {
            profile.profile.station_profile_id == profile_id
                && profile.account_id == auth.session.account_id
                && profile.logbook_id == input.logbook_id
        }) {
            return Err(ApiError::NotFound);
        }
        for profile in state
            .station_profiles
            .values_mut()
            .filter(|profile| profile.logbook_id == input.logbook_id)
        {
            profile.is_default = profile.profile.station_profile_id == profile_id;
            profile.profile.active = profile.is_default;
            profile.profile.updated_at = Utc::now();
        }
        let profile = state
            .station_profiles
            .get(&profile_id)
            .cloned()
            .ok_or(ApiError::NotFound)?;
        self.persist_metadata(&state)?;
        Ok(json!({"station_profile": profile}))
    }

    async fn list_equipment(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let logbook_id = logbook_id_from_query(request)?;
        let auth = self
            .require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let state = self.state.read().await;
        let mut equipment = state
            .equipment_profiles
            .values()
            .filter(|equipment| {
                equipment.account_id == auth.session.account_id
                    && equipment.logbook_id == logbook_id
            })
            .cloned()
            .collect::<Vec<_>>();
        equipment.sort_by(|left, right| {
            left.equipment
                .display_name
                .cmp(&right.equipment.display_name)
                .then_with(|| {
                    left.equipment
                        .equipment_id
                        .cmp(&right.equipment.equipment_id)
                })
        });
        Ok(json!({"equipment": equipment}))
    }

    async fn create_equipment(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let input: EquipmentProfileRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let mut equipment = equipment_from_request(&input)?;
        equipment.account_id = Some(auth.session.account_id.to_string());
        let now = Utc::now();
        equipment.created_at = now;
        equipment.updated_at = now;
        let hosted = HostedEquipmentProfile {
            account_id: auth.session.account_id,
            logbook_id: input.logbook_id,
            equipment,
            station_profile_id: input.station_profile_id,
        };
        let mut state = self.state.write().await;
        validate_station_assignment(&state, &hosted)?;
        state
            .equipment_profiles
            .insert(hosted.equipment.equipment_id, hosted.clone());
        self.persist_metadata(&state)?;
        Ok(json!({"equipment": hosted}))
    }

    async fn get_equipment(
        &self,
        request: &ApiRequest,
        equipment_id: &str,
    ) -> Result<Value, ApiError> {
        let equipment_id = parse_uuid(equipment_id)?;
        let logbook_id = logbook_id_from_query(request)?;
        let auth = self
            .require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let state = self.state.read().await;
        let equipment = state
            .equipment_profiles
            .get(&equipment_id)
            .filter(|equipment| {
                equipment.account_id == auth.session.account_id
                    && equipment.logbook_id == logbook_id
            })
            .cloned()
            .ok_or(ApiError::NotFound)?;
        Ok(json!({"equipment": equipment}))
    }

    async fn patch_equipment(
        &self,
        request: &ApiRequest,
        equipment_id: &str,
    ) -> Result<Value, ApiError> {
        let equipment_id = parse_uuid(equipment_id)?;
        let input: EquipmentProfileRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let mut state = self.state.write().await;
        let updated = {
            let equipment = state
                .equipment_profiles
                .get(&equipment_id)
                .filter(|equipment| {
                    equipment.account_id == auth.session.account_id
                        && equipment.logbook_id == input.logbook_id
                })
                .cloned()
                .ok_or(ApiError::NotFound)?;
            let mut updated = equipment;
            apply_equipment_patch(&mut updated, input);
            validate_station_assignment(&state, &updated)?;
            updated
        };
        state
            .equipment_profiles
            .insert(equipment_id, updated.clone());
        self.persist_metadata(&state)?;
        Ok(json!({"equipment": updated}))
    }

    async fn archive_equipment(
        &self,
        request: &ApiRequest,
        equipment_id: &str,
    ) -> Result<Value, ApiError> {
        let equipment_id = parse_uuid(equipment_id)?;
        let input: QsoActionRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let mut state = self.state.write().await;
        let equipment = state
            .equipment_profiles
            .get_mut(&equipment_id)
            .filter(|equipment| {
                equipment.account_id == auth.session.account_id
                    && equipment.logbook_id == input.logbook_id
            })
            .ok_or(ApiError::NotFound)?;
        equipment.equipment.status = EquipmentStatus::Retired;
        equipment.equipment.updated_at = Utc::now();
        let equipment = equipment.clone();
        self.persist_metadata(&state)?;
        Ok(json!({"equipment": equipment}))
    }

    async fn import_adif_route(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let input: AdifImportRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::LogQso)
            .await?;
        let station_callsign = input
            .station_callsign
            .or_else(|| {
                self.state
                    .try_read()
                    .ok()
                    .and_then(|state| state.logbooks.get(&input.logbook_id).cloned())
                    .and_then(|logbook| logbook.station_callsign)
            })
            .unwrap_or_else(|| "KE8YGW".to_owned());
        let mut options =
            AdifImportOptions::mvp_default(station_callsign, "core.gui", auth.session.device_id);
        options.operator_callsign = input.operator_callsign;
        let context = ProposalContext::local_admin(core_gui_manifest(), auth.role.proposal_role());
        let summary = import_adif(
            self.store.as_ref(),
            self.bus.as_ref(),
            &context,
            input.logbook_id,
            &input.adif,
            &options,
        )
        .await;
        let head = self.logbook_head(input.logbook_id).await?;
        Ok(json!({
            "summary": summary,
            "head": head
        }))
    }

    async fn export_adif_route(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let logbook_id = logbook_id_from_query(request)?;
        self.require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let include_deleted = request
            .query
            .get("include_deleted")
            .is_some_and(|value| value == "true");
        let projection = self
            .store
            .rebuild_projections(logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let qso_count = projection.list(include_deleted).len();
        let adif = export_adif(&projection, include_deleted);
        let head = self.logbook_head(logbook_id).await?;
        Ok(json!({
            "file_name": format!("ke8ygw-{logbook_id}.adi"),
            "content_type": "application/x-adif",
            "qso_count": qso_count,
            "head": head,
            "adif": adif
        }))
    }

    async fn list_activations(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let logbook_id = logbook_id_from_query(request)?;
        self.require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let include_ended = request
            .query
            .get("include_ended")
            .is_some_and(|value| value == "true");
        let projection = self
            .store
            .rebuild_activation_projections(logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let activations = projection
            .list(include_ended)
            .into_iter()
            .map(activation_response)
            .collect::<Vec<_>>();
        Ok(json!({"logbook_id": logbook_id, "activations": activations}))
    }

    async fn create_activation(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let input: ActivationWriteRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let started = input.started_at.is_some();
        let payload = activation_payload(input, started)?;
        self.submit_workflow_proposal(
            auth,
            if started {
                PROPOSAL_ACTIVATION_START
            } else {
                PROPOSAL_ACTIVATION_CREATE
            },
            None,
            payload,
        )
        .await
    }

    async fn get_activation(
        &self,
        request: &ApiRequest,
        activation_id: &str,
    ) -> Result<Value, ApiError> {
        let activation_id = parse_uuid(activation_id)?;
        let logbook_id = logbook_id_from_query(request)?;
        self.require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let projection = self
            .store
            .rebuild_activation_projections(logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let activation = projection
            .get(activation_id)
            .map(activation_response)
            .ok_or(ApiError::NotFound)?;
        Ok(json!({"activation": activation}))
    }

    async fn patch_activation(
        &self,
        request: &ApiRequest,
        activation_id: &str,
    ) -> Result<Value, ApiError> {
        let activation_id = parse_uuid(activation_id)?;
        let input: ActivationWriteRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let payload = activation_patch_payload(input)?;
        self.submit_workflow_proposal(
            auth,
            PROPOSAL_ACTIVATION_UPDATE,
            Some(activation_id),
            payload,
        )
        .await
    }

    async fn end_activation(
        &self,
        request: &ApiRequest,
        activation_id: &str,
    ) -> Result<Value, ApiError> {
        let activation_id = parse_uuid(activation_id)?;
        let input: ActivationWriteRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let ended_at = input.ended_at.unwrap_or_else(|| Utc::now().to_rfc3339());
        self.submit_workflow_proposal(
            auth,
            PROPOSAL_ACTIVATION_END,
            Some(activation_id),
            json!({"ended_at": ended_at}),
        )
        .await
    }

    async fn activation_qsos(
        &self,
        request: &ApiRequest,
        activation_id: &str,
    ) -> Result<Value, ApiError> {
        let activation_id = parse_uuid(activation_id)?;
        let logbook_id = logbook_id_from_query(request)?;
        self.require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let activation_projection = self
            .store
            .rebuild_activation_projections(logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let qso_projection = self
            .store
            .rebuild_projections(logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let activation = activation_projection
            .get(activation_id)
            .ok_or(ApiError::NotFound)?;
        let qsos = activation
            .linked_qsos
            .iter()
            .filter_map(|qso_id| qso_projection.get(*qso_id).map(qso_response))
            .collect::<Vec<_>>();
        Ok(json!({"activation_id": activation_id, "qsos": qsos}))
    }

    async fn list_net_sessions(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let logbook_id = logbook_id_from_query(request)?;
        self.require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let include_ended = request
            .query
            .get("include_ended")
            .is_some_and(|value| value == "true");
        let projection = self.net_projection(logbook_id).await?;
        let sessions = projection
            .sessions(include_ended)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        Ok(json!({"logbook_id": logbook_id, "sessions": sessions}))
    }

    async fn create_net_session(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        self.create_net_session_with_path(request, None).await
    }

    async fn create_net_session_with_path(
        &self,
        request: &ApiRequest,
        path_session_id: Option<&str>,
    ) -> Result<Value, ApiError> {
        let input: NetSessionWriteRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let payload = net_session_start_payload(input)?;
        let path_session_id = path_session_id.map(parse_uuid).transpose()?;
        self.submit_workflow_proposal(auth, PROPOSAL_NET_SESSION_START, path_session_id, payload)
            .await
    }

    async fn get_net_session(
        &self,
        request: &ApiRequest,
        session_id: &str,
    ) -> Result<Value, ApiError> {
        let session_id = parse_uuid(session_id)?;
        let logbook_id = logbook_id_from_query(request)?;
        self.require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let projection = self.net_projection(logbook_id).await?;
        let session = projection
            .get_session(session_id)
            .cloned()
            .ok_or(ApiError::NotFound)?;
        let checkins = projection
            .checkins_for_session(session_id, true)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        let traffic = projection
            .traffic_for_session(session_id)
            .into_iter()
            .cloned()
            .collect::<Vec<_>>();
        Ok(json!({"session": session, "checkins": checkins, "traffic": traffic}))
    }

    async fn patch_net_session(
        &self,
        request: &ApiRequest,
        session_id: &str,
    ) -> Result<Value, ApiError> {
        let session_id = parse_uuid(session_id)?;
        let input: NetSessionWriteRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        if input.ended_at.is_some() {
            return self
                .submit_workflow_proposal(
                    auth,
                    PROPOSAL_NET_SESSION_END,
                    Some(session_id),
                    net_session_end_payload(input),
                )
                .await;
        }
        Err(ApiError::BadRequest(
            "net session patch currently supports ended_at; mutable session metadata remains append-only event work"
                .to_owned(),
        ))
    }

    async fn end_net_session(
        &self,
        request: &ApiRequest,
        session_id: &str,
    ) -> Result<Value, ApiError> {
        let session_id = parse_uuid(session_id)?;
        let input: NetSessionWriteRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        self.submit_workflow_proposal(
            auth,
            PROPOSAL_NET_SESSION_END,
            Some(session_id),
            net_session_end_payload(input),
        )
        .await
    }

    async fn create_net_checkin(
        &self,
        request: &ApiRequest,
        session_id: &str,
    ) -> Result<Value, ApiError> {
        let session_id = parse_uuid(session_id)?;
        let input: NetCheckInWriteRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        self.submit_workflow_proposal(
            auth,
            PROPOSAL_NET_CHECKIN_CREATE,
            None,
            net_checkin_payload(session_id, input)?,
        )
        .await
    }

    async fn patch_net_checkin(
        &self,
        request: &ApiRequest,
        session_id: &str,
        checkin_id: &str,
    ) -> Result<Value, ApiError> {
        let session_id = parse_uuid(session_id)?;
        let checkin_id = parse_uuid(checkin_id)?;
        let input: NetCheckInWriteRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        self.submit_workflow_proposal(
            auth,
            PROPOSAL_NET_CHECKIN_UPDATE,
            Some(checkin_id),
            net_checkin_patch_payload(session_id, input)?,
        )
        .await
    }

    async fn create_net_traffic(
        &self,
        request: &ApiRequest,
        session_id: &str,
    ) -> Result<Value, ApiError> {
        let session_id = parse_uuid(session_id)?;
        let input: NetTrafficWriteRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        self.submit_workflow_proposal(
            auth,
            PROPOSAL_NET_TRAFFIC_CREATE,
            None,
            net_traffic_payload(session_id, input)?,
        )
        .await
    }

    async fn map_qsos(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let logbook_id = logbook_id_from_query(request)?;
        let auth = self
            .require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let projection = self
            .store
            .rebuild_projections(logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let station_coordinate = self
            .default_station_coordinate(auth.session.account_id, logbook_id)
            .await;
        let objects = qso_map_objects(&projection, station_coordinate, None);
        Ok(json!({"logbook_id": logbook_id, "markers": objects}))
    }

    async fn map_stations(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let logbook_id = logbook_id_from_query(request)?;
        let auth = self
            .require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let state = self.state.read().await;
        let profiles = state
            .station_profiles
            .values()
            .filter(|profile| {
                profile.account_id == auth.session.account_id && profile.logbook_id == logbook_id
            })
            .filter_map(|profile| serde_json::to_value(&profile.profile).ok())
            .collect::<Vec<_>>();
        let markers = station_markers_from_profiles(&profiles);
        Ok(json!({"logbook_id": logbook_id, "markers": markers}))
    }

    async fn map_paths(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let logbook_id = logbook_id_from_query(request)?;
        let auth = self
            .require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let projection = self
            .store
            .rebuild_projections(logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let station_coordinate = self
            .default_station_coordinate(auth.session.account_id, logbook_id)
            .await;
        let paths = qso_map_objects(&projection, station_coordinate, None)
            .into_iter()
            .filter_map(|object| {
                object.path.map(|path| {
                    json!({
                        "qso_id": object.marker.metadata.get("qso_id").cloned(),
                        "marker_id": object.marker.marker_id,
                        "path": path,
                        "distance": object.distance,
                        "bearing": object.bearing
                    })
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({"logbook_id": logbook_id, "paths": paths}))
    }

    async fn map_settings(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let logbook_id = logbook_id_from_query(request)?;
        let auth = self
            .require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let state = self.state.read().await;
        let settings = state
            .map_settings
            .get(&logbook_id)
            .filter(|settings| settings.account_id == auth.session.account_id)
            .cloned()
            .unwrap_or_else(|| HostedMapSettings {
                account_id: auth.session.account_id,
                logbook_id,
                layers: MapLayerStack::default_layers(),
                updated_at: Utc::now(),
            });
        Ok(json!({"map_settings": settings}))
    }

    async fn patch_map_settings(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let input: MapSettingsPatchRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let mut state = self.state.write().await;
        let settings = state
            .map_settings
            .entry(input.logbook_id)
            .or_insert_with(|| HostedMapSettings {
                account_id: auth.session.account_id,
                logbook_id: input.logbook_id,
                layers: MapLayerStack::default_layers(),
                updated_at: Utc::now(),
            });
        settings.account_id = auth.session.account_id;
        if let Some(layer_id) = input.layer_id {
            if let Some(enabled) = input.enabled {
                settings
                    .layers
                    .set_enabled(&layer_id, enabled)
                    .map_err(|error| ApiError::BadRequest(error.to_string()))?;
            }
            if let Some(order) = input.order {
                settings
                    .layers
                    .set_order(&layer_id, order)
                    .map_err(|error| ApiError::BadRequest(error.to_string()))?;
            }
        }
        settings.updated_at = Utc::now();
        let settings = settings.clone();
        self.persist_metadata(&state)?;
        Ok(json!({"map_settings": settings}))
    }

    async fn export_backup(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let input: BackupExportRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Read)
            .await?;
        self.store
            .verify_chain(input.logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let events = self
            .store
            .list_events(input.logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let state = self.state.read().await;
        let backup =
            build_backup_record(&state, auth.session.account_id, input.logbook_id, events)?;
        drop(state);
        let mut state = self.state.write().await;
        state.backups.insert(backup.backup_id, backup.clone());
        self.persist_metadata(&state)?;
        Ok(json!({"backup": backup}))
    }

    async fn list_backups(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let logbook_id = logbook_id_from_query(request)?;
        let auth = self
            .require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let state = self.state.read().await;
        let mut backups = state
            .backups
            .values()
            .filter(|backup| {
                backup.account_id == auth.session.account_id && backup.logbook_id == logbook_id
            })
            .cloned()
            .collect::<Vec<_>>();
        backups.sort_by_key(|backup| std::cmp::Reverse(backup.created_at));
        Ok(json!({"backups": backups}))
    }

    async fn get_backup(&self, request: &ApiRequest, backup_id: &str) -> Result<Value, ApiError> {
        let backup_id = parse_uuid(backup_id)?;
        let logbook_id = logbook_id_from_query(request)?;
        let auth = self
            .require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let state = self.state.read().await;
        let backup = state
            .backups
            .get(&backup_id)
            .filter(|backup| {
                backup.account_id == auth.session.account_id && backup.logbook_id == logbook_id
            })
            .cloned()
            .ok_or(ApiError::NotFound)?;
        Ok(json!({"backup": backup}))
    }

    async fn download_backup(
        &self,
        request: &ApiRequest,
        backup_id: &str,
    ) -> Result<Value, ApiError> {
        self.get_backup(request, backup_id).await
    }

    async fn backup_import_dry_run(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let input: BackupDryRunRequest = parse_json(&request.body)?;
        self.require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let plan = validate_backup_plan(input.logbook_id, &input.backup);
        Ok(plan.to_dry_run_response())
    }

    async fn backup_import(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let input: BackupImportRequest = parse_json(&request.body)?;
        if !input.confirm_dry_run {
            return Err(ApiError::BadRequest(
                "confirm_dry_run must be true after reviewing /api/v1/backups/import/dry-run"
                    .to_owned(),
            ));
        }
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let plan = validate_backup_plan(input.logbook_id, &input.backup);
        if !plan.ok {
            return Err(ApiError::BadRequest(
                plan.errors
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "backup import validation failed".to_owned()),
            ));
        }

        let existing_events = self
            .store
            .list_events(input.logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let mut errors = Vec::new();
        if existing_events.len() > plan.events.len() {
            errors.push("target logbook is ahead of the backup; use divergence review".to_owned());
        } else {
            for (index, existing) in existing_events.iter().enumerate() {
                if plan.events.get(index) != Some(existing) {
                    errors.push(format!(
                        "target logbook diverges before backup event {}",
                        existing.event_id
                    ));
                    break;
                }
            }
        }
        if !errors.is_empty() {
            return Err(ApiError::BadRequest(errors.join("; ")));
        }

        let skipped_duplicate_count = existing_events.len();
        let mut imported_official_events_count = 0usize;
        for event in plan.events.iter().skip(skipped_duplicate_count).cloned() {
            self.store
                .append_verified_remote_event(event)
                .await
                .map_err(|error| ApiError::Store(error.to_string()))?;
            imported_official_events_count += 1;
        }
        self.store
            .verify_chain(input.logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let projection = self
            .store
            .rebuild_projections(input.logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let final_chain_head = self
            .store
            .get_head(input.logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;

        let mut state = self.state.write().await;
        let restored_support_sections = restore_backup_support_metadata(
            &mut state,
            auth.session.account_id,
            input.logbook_id,
            &input.backup,
        )?;
        self.persist_metadata(&state)?;
        Ok(json!({
            "ok": true,
            "imported_official_events_count": imported_official_events_count,
            "skipped_duplicate_count": skipped_duplicate_count,
            "restored_support_sections": restored_support_sections,
            "missing_credential_references": plan.missing_credential_references,
            "warnings": plan.warnings,
            "final_chain_head": final_chain_head,
            "projection_rebuild": {
                "ok": true,
                "qso_count": projection.list(false).len()
            },
            "manual_review_needed": false
        }))
    }

    async fn divergence_review(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let input: DivergenceReviewRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Read)
            .await?;
        let events = self
            .store
            .list_events(input.logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let preview = preview_pull_from_events(
            PreviewPullRequest {
                peer_id: "hosted-server".to_owned(),
                logbook_id: input.logbook_id,
                local_head_hash: input.local_head_hash.clone(),
            },
            &events,
        );
        let server_head = events.last().map(|event| event.event_hash.clone());
        let client_events = input.client_events;
        let can_push = client_events
            .first()
            .is_some_and(|event| event.previous_hash == server_head)
            || (client_events.is_empty() && input.local_head_hash == server_head);
        let divergent = preview.status == ReplicationStatus::Diverged
            || client_events
                .first()
                .is_some_and(|event| event.previous_hash != server_head);
        let report_id = Uuid::new_v4();
        let review = json!({
            "report_id": report_id,
            "logbook_id": input.logbook_id,
            "local_head_hash": input.local_head_hash,
            "remote_head_hash": server_head,
            "common_ancestor": if divergent { Value::Null } else { json!(preview.local_head_hash) },
            "missing_local_events": client_events.iter().map(metadata_for_event).collect::<Vec<_>>(),
            "missing_remote_events": preview.events,
            "can_safely_pull": preview.status == ReplicationStatus::RemoteAhead || preview.status == ReplicationStatus::InSync,
            "can_safely_push": can_push && !divergent,
            "divergence_detected": divergent,
            "recommended_action": if divergent { "export divergence report; do not auto-merge" } else if can_push { "safe to push" } else if preview.status == ReplicationStatus::RemoteAhead { "safe to pull" } else { "in sync" }
        });
        let report = HostedDivergenceReport {
            report_id,
            account_id: auth.session.account_id,
            logbook_id: input.logbook_id,
            created_at: Utc::now(),
            review,
        };
        let mut state = self.state.write().await;
        state.divergence_reports.insert(report_id, report.clone());
        self.persist_metadata(&state)?;
        Ok(json!({"review": report.review, "report_id": report_id}))
    }

    async fn get_divergence_report(
        &self,
        request: &ApiRequest,
        report_id: &str,
    ) -> Result<Value, ApiError> {
        let report_id = parse_uuid(report_id)?;
        let logbook_id = logbook_id_from_query(request)?;
        let auth = self
            .require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let state = self.state.read().await;
        let report = state
            .divergence_reports
            .get(&report_id)
            .filter(|report| {
                report.account_id == auth.session.account_id && report.logbook_id == logbook_id
            })
            .cloned()
            .ok_or(ApiError::NotFound)?;
        Ok(json!({"divergence_report": report}))
    }

    async fn export_divergence_report(
        &self,
        request: &ApiRequest,
        report_id: &str,
    ) -> Result<Value, ApiError> {
        self.get_divergence_report(request, report_id).await
    }

    async fn providers(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let auth = self.authorize(request).await?;
        let logbook_id = request
            .query
            .get("logbook_id")
            .and_then(|value| Uuid::parse_str(value).ok());
        if let Some(logbook_id) = logbook_id {
            self.require_logbook_role(request, logbook_id, LogbookAccess::Read)
                .await?;
        }
        let snapshot = default_service_registry().snapshot();
        let state = self.state.read().await;
        let providers = snapshot
            .providers
            .into_iter()
            .map(|provider| {
                let setting = logbook_id.and_then(|logbook_id| {
                    state
                        .provider_settings
                        .get(&provider_setting_key(
                            logbook_id,
                            &provider.metadata.provider_id,
                        ))
                        .filter(|setting| setting.account_id == auth.session.account_id)
                        .cloned()
                });
                json!({
                    "provider": provider,
                    "setting": setting,
                })
            })
            .collect::<Vec<_>>();
        Ok(json!({"providers": providers, "preferred_providers": snapshot.preferred_providers}))
    }

    async fn provider_detail(
        &self,
        request: &ApiRequest,
        provider_id: &str,
    ) -> Result<Value, ApiError> {
        let logbook_id = logbook_id_from_query(request)?;
        let auth = self
            .require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let provider = provider_metadata(provider_id).ok_or(ApiError::NotFound)?;
        let state = self.state.read().await;
        let setting = state
            .provider_settings
            .get(&provider_setting_key(logbook_id, provider_id))
            .filter(|setting| setting.account_id == auth.session.account_id)
            .cloned();
        Ok(json!({"provider": provider, "setting": setting}))
    }

    async fn patch_provider(
        &self,
        request: &ApiRequest,
        provider_id: &str,
    ) -> Result<Value, ApiError> {
        let input: ProviderPatchRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        provider_metadata(provider_id).ok_or(ApiError::NotFound)?;
        validate_secret_free_config(&input.config)?;
        let key = provider_setting_key(input.logbook_id, provider_id);
        let mut state = self.state.write().await;
        let mut setting = state
            .provider_settings
            .get(&key)
            .cloned()
            .unwrap_or_else(|| HostedProviderSetting {
                account_id: auth.session.account_id,
                logbook_id: input.logbook_id,
                provider_id: provider_id.to_owned(),
                enabled: false,
                credential_id: None,
                config: Map::new(),
                updated_at: Utc::now(),
            });
        setting.account_id = auth.session.account_id;
        setting.enabled = input.enabled.unwrap_or(setting.enabled);
        if input.credential_id.is_some() {
            setting.credential_id = input.credential_id.filter(|value| !value.trim().is_empty());
        }
        for (key, value) in input.config {
            setting.config.insert(key, value);
        }
        setting.updated_at = Utc::now();
        state.provider_settings.insert(key, setting.clone());
        self.persist_metadata(&state)?;
        Ok(json!({"provider_setting": setting}))
    }

    async fn test_provider(
        &self,
        request: &ApiRequest,
        provider_id: &str,
    ) -> Result<Value, ApiError> {
        let input: ProviderTestRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let provider = provider_metadata(provider_id).ok_or(ApiError::NotFound)?;
        let state = self.state.read().await;
        let setting = state
            .provider_settings
            .get(&provider_setting_key(input.logbook_id, provider_id))
            .filter(|setting| setting.account_id == auth.session.account_id)
            .cloned();
        let credential_present = setting
            .as_ref()
            .and_then(|setting| setting.credential_id.as_ref())
            .is_some();
        let mock_mode = setting
            .as_ref()
            .is_some_and(|setting| config_bool(&setting.config, "mock_mode"));
        let enabled = setting.as_ref().is_some_and(|setting| setting.enabled);
        let requires_credential = !provider.metadata.required_credentials.is_empty()
            || !provider.metadata.required_config_keys.is_empty();
        let credential_reference_status = if mock_mode {
            "mock_bypassed"
        } else if requires_credential && !credential_present {
            "missing"
        } else if credential_present {
            "reference_present"
        } else {
            "not_required"
        };
        let credential_reference_resolves =
            mock_mode || credential_reference_status == "reference_present";
        let (status, diagnostic_message) = if mock_mode {
            ("ok", "fake provider test succeeded")
        } else if requires_credential && !credential_present {
            ("missing_credential", "credential reference is required")
        } else {
            ("ok", "provider credential reference is structurally valid")
        };
        Ok(json!({
            "provider_id": provider_id,
            "enabled": enabled,
            "credential_reference_present": credential_present,
            "credential_reference_status": credential_reference_status,
            "credential_reference_resolves": credential_reference_resolves,
            "test_status": status,
            "diagnostic_message": diagnostic_message,
            "redacted_error": Value::Null
        }))
    }

    async fn list_uploads(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let logbook_id = logbook_id_from_query(request)?;
        let auth = self
            .require_logbook_role(request, logbook_id, LogbookAccess::Read)
            .await?;
        let state = self.state.read().await;
        let mut uploads = state
            .upload_jobs
            .values()
            .filter(|job| job.account_id == auth.session.account_id && job.logbook_id == logbook_id)
            .cloned()
            .collect::<Vec<_>>();
        uploads.sort_by_key(|job| std::cmp::Reverse(job.created_at));
        Ok(json!({"uploads": uploads, "summary": upload_summary(&uploads)}))
    }

    async fn run_upload(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let input: UploadRunRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        provider_metadata(&input.provider_id).ok_or(ApiError::NotFound)?;
        let projection = self
            .store
            .rebuild_projections(input.logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let qso_ids = input.qso_ids.unwrap_or_else(|| {
            projection
                .list(false)
                .iter()
                .map(|qso| qso.qso_id)
                .collect()
        });
        let adif = adif_for_upload_job(&projection, &qso_ids);
        let mut state = self.state.write().await;
        if let Some(existing) = state.upload_jobs.values().find(|job| {
            job.account_id == auth.session.account_id
                && job.logbook_id == input.logbook_id
                && job.provider_id == input.provider_id
                && job.qso_ids == qso_ids
                && matches!(
                    job.status,
                    HostedUploadStatus::Queued
                        | HostedUploadStatus::Running
                        | HostedUploadStatus::Succeeded
                )
        }) {
            return Ok(json!({"upload": existing, "deduplicated": true}));
        }
        let now = Utc::now();
        let mut job = HostedUploadJob {
            upload_id: Uuid::new_v4(),
            account_id: auth.session.account_id,
            logbook_id: input.logbook_id,
            provider_id: input.provider_id,
            status: HostedUploadStatus::Queued,
            qso_ids,
            generated_adif: adif,
            retry_count: 0,
            failure_reason: None,
            provider_error: None,
            created_at: now,
            updated_at: now,
        };
        execute_hosted_upload_job(&mut job, &state, input.force_fail.unwrap_or(false));
        state.upload_jobs.insert(job.upload_id, job.clone());
        self.persist_metadata(&state)?;
        Ok(json!({"upload": job, "deduplicated": false}))
    }

    async fn retry_upload(&self, request: &ApiRequest, upload_id: &str) -> Result<Value, ApiError> {
        let upload_id = parse_uuid(upload_id)?;
        let input: QsoActionRequest = parse_json(&request.body)?;
        let auth = self
            .require_logbook_role(request, input.logbook_id, LogbookAccess::Admin)
            .await?;
        let mut state = self.state.write().await;
        let snapshot = state.clone();
        let job = state
            .upload_jobs
            .get_mut(&upload_id)
            .filter(|job| {
                job.account_id == auth.session.account_id && job.logbook_id == input.logbook_id
            })
            .ok_or(ApiError::NotFound)?;
        if job.status == HostedUploadStatus::Succeeded {
            return Ok(json!({"upload": job, "deduplicated": true}));
        }
        job.retry_count += 1;
        execute_hosted_upload_job(job, &snapshot, false);
        let job = job.clone();
        self.persist_metadata(&state)?;
        Ok(json!({"upload": job, "deduplicated": false}))
    }

    async fn sync_status(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let auth = self.authorize(request).await?;
        let state = self.state.read().await;
        let mut logbooks = Vec::new();
        for membership in state
            .memberships
            .iter()
            .filter(|membership| membership.user_id == auth.session.user_id)
        {
            if membership.role.can_read() {
                logbooks.push(self.logbook_head(membership.logbook_id).await?);
            }
        }
        Ok(json!({
            "connection_state": "connected",
            "account_id": auth.session.account_id,
            "device_id": auth.session.device_id,
            "accessible_logbooks": logbooks
        }))
    }

    async fn sync_preview(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let input: PreviewSyncRequest = parse_json(&request.body)?;
        self.require_logbook_role(request, input.logbook_id, LogbookAccess::Read)
            .await?;
        let events = self
            .store
            .list_events(input.logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let preview = preview_pull_from_events(
            PreviewPullRequest {
                peer_id: "hosted-server".to_owned(),
                logbook_id: input.logbook_id,
                local_head_hash: input.local_head_hash,
            },
            &events,
        );
        Ok(json!(preview))
    }

    async fn sync_push(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let input: CloudPushEventsRequest = parse_json(&request.body)?;
        self.require_logbook_role(request, input.logbook_id, LogbookAccess::LogQso)
            .await?;
        let mut accepted_count = 0usize;
        let mut ignored_duplicate_count = 0usize;
        let mut errors = Vec::new();
        for event in input.events {
            let event_id = event.event_id;
            match self.store.get_event(event_id).await {
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
            .get_head(input.logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        Ok(json!({
            "status": if errors.is_empty() { "pulled" } else { "rejected" },
            "accepted_count": accepted_count,
            "ignored_duplicate_count": ignored_duplicate_count,
            "rejected_count": errors.len(),
            "server_head_hash": server_head_hash,
            "errors": errors
        }))
    }

    async fn sync_pull(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let input: PreviewSyncRequest = parse_json(&request.body)?;
        self.require_logbook_role(request, input.logbook_id, LogbookAccess::Read)
            .await?;
        let events = self
            .store
            .list_events(input.logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let preview = preview_pull_from_events(
            PreviewPullRequest {
                peer_id: "hosted-server".to_owned(),
                logbook_id: input.logbook_id,
                local_head_hash: input.local_head_hash,
            },
            &events,
        );
        let missing_hashes = preview
            .events
            .iter()
            .map(|event| event.event_hash.as_str())
            .collect::<std::collections::HashSet<_>>();
        let events = events
            .into_iter()
            .filter(|event| missing_hashes.contains(event.event_hash.as_str()))
            .collect::<Vec<_>>();
        Ok(json!(CloudPullEventsResponse { preview, events }))
    }

    async fn devices(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let auth = self.authorize(request).await?;
        let state = self.state.read().await;
        let devices = state
            .devices
            .values()
            .filter(|device| device.account_id == auth.session.account_id)
            .cloned()
            .collect::<Vec<_>>();
        Ok(json!({"devices": devices}))
    }

    async fn register_device(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        let auth = self.authorize(request).await?;
        let input: RegisterDeviceRequest = parse_json(&request.body)?;
        let now = Utc::now();
        let device = DeviceIdentity {
            device_id: Uuid::new_v4(),
            account_id: auth.session.account_id,
            user_id: auth.session.user_id,
            device_name: input.device_name,
            fingerprint: input
                .fingerprint
                .unwrap_or_else(|| format!("dev-{}", Uuid::new_v4())),
            trusted: false,
            revoked: false,
            registered_at: now,
            revoked_at: None,
        };
        self.state
            .write()
            .await
            .devices
            .insert(device.device_id, device.clone());
        let state = self.state.read().await;
        self.persist_metadata(&state)?;
        Ok(json!({"device": device}))
    }

    async fn revoke_device(
        &self,
        request: &ApiRequest,
        device_id: &str,
    ) -> Result<Value, ApiError> {
        let device_id = parse_uuid(device_id)?;
        let auth = self.authorize(request).await?;
        let mut state = self.state.write().await;
        let device = state.devices.get(&device_id).ok_or(ApiError::NotFound)?;
        if device.account_id != auth.session.account_id {
            return Err(ApiError::Forbidden);
        }
        let has_owner_role = state
            .memberships
            .iter()
            .filter(|membership| membership.user_id == auth.session.user_id)
            .any(|membership| membership.role.can_manage_owner_resources());
        if !has_owner_role {
            return Err(ApiError::Forbidden);
        }
        let device = state
            .devices
            .get_mut(&device_id)
            .ok_or(ApiError::NotFound)?;
        device.revoked = true;
        device.revoked_at = Some(Utc::now());
        for session in state.sessions_by_token.values_mut() {
            if session.device_id == device_id {
                session.active = false;
            }
        }
        self.persist_metadata(&state)?;
        Ok(json!({"ok": true}))
    }

    async fn scaffolded(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        self.authorize(request).await?;
        Ok(json!({
            "ok": true,
            "implemented": false,
            "path": request.path,
            "method": request.method,
            "message": "v0.2 hosted API route is reserved; domain implementation remains a tracked v0.2 gap"
        }))
    }

    async fn authorize(&self, request: &ApiRequest) -> Result<AuthorizedSession, ApiError> {
        let token = bearer_token(request).ok_or(ApiError::Unauthenticated)?;
        let state = self.state.read().await;
        let session = state
            .sessions_by_token
            .get(&token)
            .cloned()
            .ok_or(ApiError::Unauthenticated)?;
        if !session.active {
            return Err(ApiError::InactiveSession);
        }
        let device = state
            .devices
            .get(&session.device_id)
            .ok_or(ApiError::Unauthenticated)?;
        if device.revoked {
            return Err(ApiError::RevokedDevice);
        }
        Ok(AuthorizedSession { session })
    }

    async fn require_logbook_role(
        &self,
        request: &ApiRequest,
        logbook_id: Uuid,
        access: LogbookAccess,
    ) -> Result<AuthorizedLogbook, ApiError> {
        let auth = self.authorize(request).await?;
        let state = self.state.read().await;
        let membership = state
            .memberships
            .iter()
            .find(|membership| {
                membership.user_id == auth.session.user_id && membership.logbook_id == logbook_id
            })
            .ok_or(ApiError::Forbidden)?;
        let allowed = match access {
            LogbookAccess::Read => membership.role.can_read(),
            LogbookAccess::LogQso => membership.role.can_log_qso(),
            LogbookAccess::Admin => membership.role.can_administer(),
        };
        if !allowed {
            return Err(ApiError::Forbidden);
        }
        Ok(AuthorizedLogbook {
            session: auth.session,
            logbook_id,
            role: membership.role,
        })
    }

    async fn submit_qso_proposal(
        &self,
        auth: AuthorizedLogbook,
        proposal_type: &str,
        qso_id: Option<Uuid>,
        payload: Value,
    ) -> Result<Value, ApiError> {
        let proposal = ProposalEnvelope::new(
            proposal_type,
            auth.logbook_id,
            qso_id,
            Some(auth.session.user_id),
            auth.session.device_id,
            "core.gui",
            1,
            payload,
        );
        let context = ProposalContext::local_admin(core_gui_manifest(), auth.role.proposal_role());
        let outcome = submit_proposal(self.store.as_ref(), self.bus.as_ref(), &context, proposal)
            .await
            .map_err(|error| ApiError::Proposal(error.to_string()))?;
        let projection = self
            .store
            .rebuild_projections(auth.logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let qsos = projection
            .list(true)
            .into_iter()
            .map(qso_response)
            .collect::<Vec<_>>();
        Ok(json!({
            "ok": true,
            "event": outcome.official_event,
            "projection": QsoListResponse { logbook_id: auth.logbook_id, qsos }
        }))
    }

    async fn submit_workflow_proposal(
        &self,
        auth: AuthorizedLogbook,
        proposal_type: &str,
        entity_id: Option<Uuid>,
        payload: Value,
    ) -> Result<Value, ApiError> {
        let proposal = ProposalEnvelope::new(
            proposal_type,
            auth.logbook_id,
            entity_id,
            Some(auth.session.user_id),
            auth.session.device_id,
            "core.gui",
            1,
            payload,
        );
        let context = ProposalContext::local_admin(core_gui_manifest(), auth.role.proposal_role());
        let outcome = submit_proposal(self.store.as_ref(), self.bus.as_ref(), &context, proposal)
            .await
            .map_err(|error| ApiError::Proposal(error.to_string()))?;
        let head = self.logbook_head(auth.logbook_id).await?;
        Ok(json!({
            "ok": true,
            "event": outcome.official_event,
            "head": head
        }))
    }

    async fn net_projection(&self, logbook_id: Uuid) -> Result<NetControlProjection, ApiError> {
        let events = self
            .store
            .list_events(logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        let mut projection = NetControlProjection::new();
        projection
            .rebuild(events.iter())
            .map_err(|error| ApiError::Store(error.to_string()))?;
        Ok(projection)
    }

    async fn default_station_coordinate(
        &self,
        account_id: Uuid,
        logbook_id: Uuid,
    ) -> Option<Coordinate> {
        let state = self.state.read().await;
        state
            .station_profiles
            .values()
            .find(|profile| {
                profile.account_id == account_id
                    && profile.logbook_id == logbook_id
                    && profile.is_default
            })
            .or_else(|| {
                state.station_profiles.values().find(|profile| {
                    profile.account_id == account_id && profile.logbook_id == logbook_id
                })
            })
            .and_then(|profile| profile.profile.default_grid.as_deref())
            .and_then(|grid| ham_core::maidenhead_to_coordinate(grid).ok())
    }

    async fn logbook_head(&self, logbook_id: Uuid) -> Result<LogbookHeadSummary, ApiError> {
        let events = self
            .store
            .list_events(logbook_id)
            .await
            .map_err(|error| ApiError::Store(error.to_string()))?;
        Ok(LogbookHeadSummary {
            logbook_id,
            head_hash: events.last().map(|event| event.event_hash.clone()),
            event_count: Some(events.len() as u64),
        })
    }

    fn persist_metadata(&self, state: &ServerState) -> Result<(), ApiError> {
        self.metadata_store
            .save(state)
            .map_err(|error| ApiError::Store(error.to_string()))
    }

    #[cfg(test)]
    async fn reload_metadata_from_store(&self) -> Result<(), MetadataStoreError> {
        let state = self.metadata_store.load()?;
        *self.state.write().await = state;
        Ok(())
    }

    #[cfg(test)]
    async fn add_membership_for_email(
        &self,
        email: &str,
        logbook_id: Uuid,
        role: LogbookRole,
    ) -> Result<(), ApiError> {
        let mut state = self.state.write().await;
        let account_id = state
            .users_by_email
            .get(&email.to_ascii_lowercase())
            .copied()
            .ok_or(ApiError::NotFound)?;
        let user = state
            .accounts
            .get(&account_id)
            .cloned()
            .ok_or(ApiError::NotFound)?;
        let logbook = state
            .logbooks
            .get(&logbook_id)
            .cloned()
            .ok_or(ApiError::NotFound)?;
        state.memberships.push(LogbookMembership {
            account_id: logbook.account_id,
            logbook_id,
            user_id: user.user_id,
            role,
            created_at: Utc::now(),
        });
        self.persist_metadata(&state)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
struct AuthorizedSession {
    session: LoginSession,
}

#[derive(Debug, Clone)]
struct AuthorizedLogbook {
    session: LoginSession,
    logbook_id: Uuid,
    role: LogbookRole,
}

#[derive(Debug, Clone, Copy)]
enum LogbookAccess {
    Read,
    LogQso,
    Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PreviewSyncRequest {
    logbook_id: Uuid,
    local_head_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegisterDeviceRequest {
    device_name: String,
    fingerprint: Option<String>,
}

fn qso_create_payload(input: QsoWriteRequest) -> Result<Value, ApiError> {
    let contacted_callsign = input
        .contacted_callsign
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::BadRequest("contacted_callsign is required".to_owned()))?;
    let mode = input
        .mode
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::BadRequest("mode is required".to_owned()))?;
    let mut payload = json!({
        "station_callsign": input.station_callsign.clone().unwrap_or_else(|| "KE8YGW".to_owned()),
        "operator_callsign": input.operator_callsign.clone().unwrap_or_else(|| "KE8YGW".to_owned()),
        "contacted_callsign": contacted_callsign.trim().to_ascii_uppercase(),
        "started_at": input.started_at.clone().unwrap_or_else(|| Utc::now().to_rfc3339()),
        "mode": mode.trim().to_ascii_uppercase(),
        "source": "hosted-api"
    });
    merge_optional_qso_fields(&mut payload, &input);
    merge_extra_fields(&mut payload, input.fields);
    Ok(payload)
}

fn qso_patch_payload(input: QsoWriteRequest) -> Result<Value, ApiError> {
    let mut payload = Value::Object(Map::new());
    if let Some(value) = input
        .contacted_callsign
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload["contacted_callsign"] = json!(value.trim().to_ascii_uppercase());
    }
    if let Some(value) = input
        .station_callsign
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload["station_callsign"] = json!(value);
    }
    if let Some(value) = input
        .operator_callsign
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload["operator_callsign"] = json!(value);
    }
    if let Some(value) = input
        .started_at
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload["started_at"] = json!(value);
    }
    if let Some(value) = input
        .mode
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload["mode"] = json!(value.trim().to_ascii_uppercase());
    }
    merge_optional_qso_fields(&mut payload, &input);
    merge_extra_fields(&mut payload, input.fields);
    if payload.as_object().is_some_and(Map::is_empty) {
        return Err(ApiError::BadRequest(
            "qso patch payload must not be empty".to_owned(),
        ));
    }
    Ok(payload)
}

fn merge_optional_qso_fields(payload: &mut Value, input: &QsoWriteRequest) {
    if let Some(value) = input
        .band
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload["band"] = json!(value);
    }
    if let Some(value) = input.frequency_hz {
        payload["frequency_hz"] = json!(value);
    }
    if let Some(value) = input
        .notes
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        payload["notes"] = json!(value);
    }
}

fn merge_extra_fields(payload: &mut Value, fields: Map<String, Value>) {
    for (key, value) in fields {
        if !matches!(
            key.as_str(),
            "qso_id" | "event_hash" | "previous_hash" | "source_device_id"
        ) {
            payload[key] = value;
        }
    }
}

fn qso_response(record: &ham_core::QsoRecord) -> QsoRecordResponse {
    QsoRecordResponse {
        qso_id: record.qso_id,
        payload: record.payload.clone(),
        note_history: record.note_history.clone(),
        deleted: record.deleted,
        last_event_hash: record.last_event_hash.clone(),
    }
}

fn activation_response(record: &ham_core::ActivationRecord) -> Value {
    json!({
        "activation_id": record.activation_id,
        "payload": record.payload,
        "status": record.status,
        "note_history": record.note_history,
        "linked_qsos": record.linked_qsos,
        "qso_count": record.qso_count,
        "unique_callsign_count": record.unique_callsign_count,
        "band_summary": record.band_summary,
        "mode_summary": record.mode_summary,
        "last_event_hash": record.last_event_hash,
    })
}

fn core_gui_manifest() -> PluginManifest {
    PluginManifest::new(
        "core.gui",
        "Hosted API",
        env!("CARGO_PKG_VERSION"),
        vec![
            PluginCapability::QsoView,
            PluginCapability::QsoCreate,
            PluginCapability::QsoCorrect,
            PluginCapability::QsoDelete,
            PluginCapability::QsoRestore,
            PluginCapability::QsoNoteAdd,
            PluginCapability::ActivationCreate,
            PluginCapability::ActivationUpdate,
            PluginCapability::ActivationEnd,
            PluginCapability::NetSessionStart,
            PluginCapability::NetSessionEnd,
            PluginCapability::NetCheckinCreate,
            PluginCapability::NetCheckinUpdate,
            PluginCapability::NetTrafficManage,
        ],
    )
}

fn visible_logbooks(state: &ServerState, user_id: Uuid) -> Vec<ApiLogbook> {
    let mut logbooks = state
        .memberships
        .iter()
        .filter(|membership| membership.user_id == user_id && membership.role.can_read())
        .filter_map(|membership| state.logbooks.get(&membership.logbook_id))
        .cloned()
        .collect::<Vec<_>>();
    logbooks.sort_by(|left, right| left.name.cmp(&right.name));
    logbooks
}

fn station_profile_from_request(input: &StationProfileRequest) -> Result<StationProfile, ApiError> {
    let display_name = input
        .display_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::BadRequest("display_name is required".to_owned()))?;
    let station_callsign = input
        .station_callsign
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::BadRequest("station_callsign is required".to_owned()))?;
    let mut profile = StationProfile::new(display_name, station_callsign);
    profile.operator_callsign = input.operator_callsign.clone();
    profile.default_grid = input.default_grid.clone();
    profile.default_qth = input.default_qth.clone();
    profile.default_power_watts = input.default_power_watts;
    profile.notes = input.notes.clone();
    profile.tags = input.tags.clone();
    profile.active = input.active.unwrap_or(false);
    Ok(profile)
}

fn validate_station_patch(
    input: &StationProfileRequest,
    existing: &HostedStationProfile,
) -> Result<(), ApiError> {
    if input.display_name.is_none()
        && input.station_callsign.is_none()
        && input.operator_callsign.is_none()
        && input.default_grid.is_none()
        && input.default_qth.is_none()
        && input.default_power_watts.is_none()
        && input.notes.is_none()
        && input.tags.is_empty()
        && input.active.is_none()
    {
        return Err(ApiError::BadRequest(
            "station profile patch must not be empty".to_owned(),
        ));
    }
    if input
        .display_name
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(ApiError::BadRequest(
            "display_name must not be empty".to_owned(),
        ));
    }
    if input
        .station_callsign
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(ApiError::BadRequest(
            "station_callsign must not be empty".to_owned(),
        ));
    }
    if existing.logbook_id != input.logbook_id {
        return Err(ApiError::NotFound);
    }
    Ok(())
}

fn apply_station_patch(profile: &mut HostedStationProfile, input: StationProfileRequest) {
    if let Some(display_name) = input.display_name {
        profile.profile.display_name = display_name;
    }
    if let Some(station_callsign) = input.station_callsign {
        profile.profile.station_callsign = station_callsign.trim().to_ascii_uppercase();
    }
    if input.operator_callsign.is_some() {
        profile.profile.operator_callsign = input.operator_callsign;
    }
    if input.default_grid.is_some() {
        profile.profile.default_grid = input.default_grid;
    }
    if input.default_qth.is_some() {
        profile.profile.default_qth = input.default_qth;
    }
    if input.default_power_watts.is_some() {
        profile.profile.default_power_watts = input.default_power_watts;
    }
    if input.notes.is_some() {
        profile.profile.notes = input.notes;
    }
    if !input.tags.is_empty() {
        profile.profile.tags = input.tags;
    }
    if let Some(active) = input.active {
        profile.is_default = active;
        profile.profile.active = active;
    }
    profile.profile.updated_at = Utc::now();
}

fn equipment_from_request(input: &EquipmentProfileRequest) -> Result<EquipmentItem, ApiError> {
    let equipment_type = input
        .equipment_type
        .clone()
        .ok_or_else(|| ApiError::BadRequest("equipment_type is required".to_owned()))?;
    let display_name = input
        .display_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::BadRequest("display_name is required".to_owned()))?;
    let mut equipment = EquipmentItem::new(equipment_type, display_name);
    equipment.manufacturer = input.manufacturer.clone();
    equipment.model = input.model.clone();
    equipment.serial_number = input.serial_number.clone();
    equipment.capabilities = input.capabilities.clone();
    equipment.notes = input.notes.clone();
    equipment.status = input.status.clone().unwrap_or(EquipmentStatus::Active);
    Ok(equipment)
}

fn apply_equipment_patch(equipment: &mut HostedEquipmentProfile, input: EquipmentProfileRequest) {
    if let Some(equipment_type) = input.equipment_type {
        equipment.equipment.equipment_type = equipment_type;
    }
    if let Some(display_name) = input.display_name {
        equipment.equipment.display_name = display_name;
    }
    if input.manufacturer.is_some() {
        equipment.equipment.manufacturer = input.manufacturer;
    }
    if input.model.is_some() {
        equipment.equipment.model = input.model;
    }
    if input.serial_number.is_some() {
        equipment.equipment.serial_number = input.serial_number;
    }
    if !input.capabilities.is_empty() {
        equipment.equipment.capabilities = input.capabilities;
    }
    if input.notes.is_some() {
        equipment.equipment.notes = input.notes;
    }
    if let Some(status) = input.status {
        equipment.equipment.status = status;
    }
    if input.station_profile_id.is_some() {
        equipment.station_profile_id = input.station_profile_id;
    }
    equipment.equipment.updated_at = Utc::now();
}

fn validate_station_assignment(
    state: &ServerState,
    equipment: &HostedEquipmentProfile,
) -> Result<(), ApiError> {
    if let Some(station_profile_id) = equipment.station_profile_id {
        let exists = state.station_profiles.values().any(|profile| {
            profile.account_id == equipment.account_id
                && profile.logbook_id == equipment.logbook_id
                && profile.profile.station_profile_id == station_profile_id
        });
        if !exists {
            return Err(ApiError::BadRequest(
                "station_profile_id is not in this logbook".to_owned(),
            ));
        }
    }
    Ok(())
}

fn provider_metadata(provider_id: &str) -> Option<RegisteredServiceProvider> {
    default_service_registry().provider(provider_id).cloned()
}

fn provider_setting_key(logbook_id: Uuid, provider_id: &str) -> String {
    format!("{logbook_id}-{provider_id}")
}

fn validate_secret_free_config(config: &Map<String, Value>) -> Result<(), ApiError> {
    for (key, value) in config {
        if secret_like_key(key) {
            return Err(ApiError::BadRequest(format!(
                "provider config field {key} looks like a secret; store a credential_id reference instead"
            )));
        }
        if let Value::Object(map) = value {
            validate_secret_free_config(map)?;
        }
    }
    Ok(())
}

fn secret_like_key(key: &str) -> bool {
    let lowered = key.to_ascii_lowercase();
    !lowered.ends_with("credential_id")
        && (lowered.contains("secret")
            || lowered.contains("password")
            || lowered.contains("token")
            || lowered.contains("api_key")
            || lowered.contains("apikey"))
}

fn config_bool(config: &Map<String, Value>, key: &str) -> bool {
    config.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn execute_hosted_upload_job(job: &mut HostedUploadJob, state: &ServerState, force_fail: bool) {
    job.status = HostedUploadStatus::Running;
    job.updated_at = Utc::now();
    let setting = state
        .provider_settings
        .get(&provider_setting_key(job.logbook_id, &job.provider_id));
    let mock_mode = setting.is_some_and(|setting| config_bool(&setting.config, "mock_mode"));
    let credential_present = setting
        .and_then(|setting| setting.credential_id.as_ref())
        .is_some();
    let provider_requires_credentials =
        provider_metadata(&job.provider_id).is_some_and(|provider| {
            !provider.metadata.required_credentials.is_empty()
                || !provider.metadata.required_config_keys.is_empty()
        });
    if force_fail {
        job.status = HostedUploadStatus::Retryable;
        job.failure_reason = Some("forced fake provider failure".to_owned());
        job.provider_error = Some("redacted fake provider failure".to_owned());
    } else if !mock_mode && provider_requires_credentials && !credential_present {
        job.status = HostedUploadStatus::Retryable;
        job.failure_reason = Some("missing credential reference".to_owned());
        job.provider_error = Some("credential reference is required".to_owned());
    } else {
        job.status = HostedUploadStatus::Succeeded;
        job.failure_reason = None;
        job.provider_error = None;
    }
    job.updated_at = Utc::now();
}

fn upload_summary(uploads: &[HostedUploadJob]) -> Value {
    let pending_count = uploads
        .iter()
        .filter(|job| {
            matches!(
                job.status,
                HostedUploadStatus::Queued | HostedUploadStatus::Running
            )
        })
        .count();
    let failed_qso_count = uploads
        .iter()
        .filter(|job| {
            matches!(
                job.status,
                HostedUploadStatus::Failed | HostedUploadStatus::Retryable
            )
        })
        .map(|job| job.qso_ids.len())
        .sum::<usize>();
    json!({
        "pending_count": pending_count,
        "last_upload": uploads.first().map(|job| job.updated_at),
        "last_success": uploads.iter().find(|job| job.status == HostedUploadStatus::Succeeded).map(|job| job.updated_at),
        "last_failure": uploads.iter().find(|job| matches!(job.status, HostedUploadStatus::Failed | HostedUploadStatus::Retryable)).map(|job| job.updated_at),
        "failed_qso_count": failed_qso_count
    })
}

fn activation_payload(
    input: ActivationWriteRequest,
    require_started: bool,
) -> Result<Value, ApiError> {
    let station_callsign = input
        .station_callsign
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "KE8YGW".to_owned());
    let operator_callsign = input
        .operator_callsign
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| station_callsign.clone());
    let mut payload = json!({
        "activation_type": input.activation_type.trim().to_ascii_lowercase(),
        "station_callsign": station_callsign,
        "operator_callsign": operator_callsign,
    });
    if require_started {
        payload["started_at"] = json!(input
            .started_at
            .clone()
            .unwrap_or_else(|| Utc::now().to_rfc3339()));
    } else if let Some(started_at) = input.started_at.clone() {
        payload["started_at"] = json!(started_at);
    }
    merge_activation_optional_fields(&mut payload, &input);
    merge_extra_fields(&mut payload, input.fields);
    Ok(payload)
}

fn activation_patch_payload(input: ActivationWriteRequest) -> Result<Value, ApiError> {
    let mut payload = Value::Object(Map::new());
    if !input.activation_type.trim().is_empty() {
        payload["activation_type"] = json!(input.activation_type.trim().to_ascii_lowercase());
    }
    if let Some(value) = input
        .station_callsign
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        payload["station_callsign"] = json!(value);
    }
    if let Some(value) = input
        .operator_callsign
        .clone()
        .filter(|value| !value.trim().is_empty())
    {
        payload["operator_callsign"] = json!(value);
    }
    if let Some(value) = input.started_at.clone() {
        payload["started_at"] = json!(value);
    }
    if let Some(value) = input.ended_at.clone() {
        payload["ended_at"] = json!(value);
    }
    merge_activation_optional_fields(&mut payload, &input);
    merge_extra_fields(&mut payload, input.fields);
    if payload.as_object().is_some_and(Map::is_empty) {
        return Err(ApiError::BadRequest(
            "activation patch payload must not be empty".to_owned(),
        ));
    }
    Ok(payload)
}

fn merge_activation_optional_fields(payload: &mut Value, input: &ActivationWriteRequest) {
    if let Some(value) = &input.park_id {
        payload["park_id"] = json!(value);
    }
    if let Some(value) = &input.summit_id {
        payload["summit_id"] = json!(value);
    }
    if let Some(value) = &input.reference {
        payload["reference"] = json!(value);
    }
    if let Some(value) = &input.name {
        payload["name"] = json!(value);
    }
    if let Some(value) = &input.notes {
        payload["notes"] = json!(value);
    }
}

fn net_session_start_payload(input: NetSessionWriteRequest) -> Result<Value, ApiError> {
    let station_callsign = input
        .station_callsign
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "KE8YGW".to_owned());
    let net_control_operator_id = input
        .net_control_operator_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let net_name = input
        .net_name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::BadRequest("net_name is required".to_owned()))?;
    let mut payload = json!({
        "station_callsign": station_callsign,
        "net_control_operator_id": net_control_operator_id,
        "net_name": net_name,
        "started_at": input.started_at.clone().unwrap_or_else(|| Utc::now().to_rfc3339()),
    });
    merge_net_session_optional_fields(&mut payload, &input);
    merge_extra_fields(&mut payload, input.fields);
    Ok(payload)
}

fn net_session_end_payload(input: NetSessionWriteRequest) -> Value {
    let mut payload = json!({
        "ended_at": input.ended_at.unwrap_or_else(|| Utc::now().to_rfc3339()),
    });
    if let Some(notes) = input.notes {
        payload["notes"] = json!(notes);
    }
    merge_extra_fields(&mut payload, input.fields);
    payload
}

fn merge_net_session_optional_fields(payload: &mut Value, input: &NetSessionWriteRequest) {
    if let Some(value) = input.frequency_hz {
        payload["frequency_hz"] = json!(value);
    }
    if let Some(value) = &input.band {
        payload["band"] = json!(value);
    }
    if let Some(value) = &input.mode {
        payload["mode"] = json!(value);
    }
    if let Some(value) = &input.notes {
        payload["notes"] = json!(value);
    }
}

fn net_checkin_payload(session_id: Uuid, input: NetCheckInWriteRequest) -> Result<Value, ApiError> {
    let tactical_only = input.tactical_only.unwrap_or(false);
    let mut payload = json!({
        "net_session_id": session_id,
        "checkin_time": input.checkin_time.unwrap_or_else(|| Utc::now().to_rfc3339()),
        "tactical_only": tactical_only,
    });
    if !tactical_only {
        let callsign = input
            .callsign
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| ApiError::BadRequest("callsign is required".to_owned()))?;
        payload["callsign"] = json!(callsign.trim().to_ascii_uppercase());
    }
    if let Some(value) = input.tactical_callsign {
        payload["tactical_callsign"] = json!(value);
    }
    if let Some(value) = input.status {
        payload["status"] = json!(value);
    }
    if let Some(value) = input.traffic {
        payload["traffic"] = json!(value);
    }
    if let Some(value) = input.notes {
        payload["notes"] = json!(value);
    }
    merge_extra_fields(&mut payload, input.fields);
    Ok(payload)
}

fn net_checkin_patch_payload(
    session_id: Uuid,
    input: NetCheckInWriteRequest,
) -> Result<Value, ApiError> {
    let mut payload = json!({"net_session_id": session_id});
    if let Some(value) = input.callsign.filter(|value| !value.trim().is_empty()) {
        payload["callsign"] = json!(value.trim().to_ascii_uppercase());
    }
    if let Some(value) = input.tactical_callsign {
        payload["tactical_callsign"] = json!(value);
    }
    if let Some(value) = input.tactical_only {
        payload["tactical_only"] = json!(value);
    }
    if let Some(value) = input.checkin_time {
        payload["checkin_time"] = json!(value);
    }
    if let Some(value) = input.status {
        payload["status"] = json!(value);
    }
    if let Some(value) = input.traffic {
        payload["traffic"] = json!(value);
    }
    if let Some(value) = input.notes {
        payload["notes"] = json!(value);
    }
    merge_extra_fields(&mut payload, input.fields);
    Ok(payload)
}

fn net_traffic_payload(session_id: Uuid, input: NetTrafficWriteRequest) -> Result<Value, ApiError> {
    let summary = input
        .summary
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ApiError::BadRequest("summary is required".to_owned()))?;
    let mut payload = json!({
        "net_session_id": session_id,
        "summary": summary,
    });
    if let Some(value) = input.precedence {
        payload["precedence"] = json!(value);
    }
    if let Some(value) = input.status {
        payload["status"] = json!(value);
    }
    if let Some(value) = input.handling_notes {
        payload["handling_notes"] = json!(value);
    }
    merge_extra_fields(&mut payload, input.fields);
    Ok(payload)
}

fn build_backup_record(
    state: &ServerState,
    account_id: Uuid,
    logbook_id: Uuid,
    events: Vec<CoreEventEnvelope>,
) -> Result<HostedBackupRecord, ApiError> {
    let station_profiles = state
        .station_profiles
        .values()
        .filter(|profile| profile.account_id == account_id && profile.logbook_id == logbook_id)
        .cloned()
        .collect::<Vec<_>>();
    let equipment = state
        .equipment_profiles
        .values()
        .filter(|equipment| {
            equipment.account_id == account_id && equipment.logbook_id == logbook_id
        })
        .cloned()
        .collect::<Vec<_>>();
    let provider_settings = state
        .provider_settings
        .values()
        .filter(|setting| setting.account_id == account_id && setting.logbook_id == logbook_id)
        .cloned()
        .collect::<Vec<_>>();
    let uploads = state
        .upload_jobs
        .values()
        .filter(|job| job.account_id == account_id && job.logbook_id == logbook_id)
        .cloned()
        .collect::<Vec<_>>();
    let map_settings = state.map_settings.get(&logbook_id).cloned();
    let head_hash = events.last().map(|event| event.event_hash.clone());
    let manifest = json!({
        "format_version": 1,
        "created_at": Utc::now(),
        "app_version": env!("CARGO_PKG_VERSION"),
        "account_id": account_id,
        "logbook_id": logbook_id,
        "head_hash": head_hash,
        "event_count": events.len(),
        "included_sections": [
            "official_events",
            "station_profiles",
            "equipment_profiles",
            "provider_settings_without_secrets",
            "upload_queue_history",
            "map_preferences"
        ],
        "excluded_sections": ["credential_secret_values", "raw_session_tokens", "device_tokens", "runtime_logs"]
    });
    let payload = json!({
        "manifest": manifest,
        "official_events": events,
        "station_profiles": station_profiles,
        "equipment_profiles": equipment,
        "provider_settings": provider_settings,
        "upload_queue_history": uploads,
        "map_settings": map_settings,
    });
    if payload
        .to_string()
        .to_ascii_lowercase()
        .contains("test-secret")
    {
        return Err(ApiError::Store(
            "backup payload contains a test secret".to_owned(),
        ));
    }
    Ok(HostedBackupRecord {
        backup_id: Uuid::new_v4(),
        account_id,
        logbook_id,
        created_at: Utc::now(),
        manifest,
        payload,
    })
}

#[derive(Debug, Clone)]
struct BackupValidationPlan {
    ok: bool,
    errors: Vec<String>,
    warnings: Vec<String>,
    events: Vec<CoreEventEnvelope>,
    head_hash: Option<String>,
    missing_credential_references: Vec<String>,
    support_sections: Vec<String>,
}

impl BackupValidationPlan {
    fn to_dry_run_response(&self) -> Value {
        json!({
            "ok": self.ok,
            "errors": self.errors,
            "warnings": self.warnings,
            "event_count": self.events.len(),
            "head_hash": self.head_hash,
            "support_sections": self.support_sections,
            "missing_credential_references": self.missing_credential_references,
            "would_import": self.ok,
            "requires_manual_review": !self.ok
        })
    }
}

fn validate_backup_plan(logbook_id: Uuid, backup: &Value) -> BackupValidationPlan {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let Some(manifest) = backup.get("manifest") else {
        return BackupValidationPlan {
            ok: false,
            errors: vec!["backup manifest is required".to_owned()],
            warnings,
            events: Vec::new(),
            head_hash: None,
            missing_credential_references: Vec::new(),
            support_sections: Vec::new(),
        };
    };
    if manifest.get("format_version").and_then(Value::as_u64) != Some(1) {
        errors.push("unsupported backup format_version".to_owned());
    }
    let manifest_logbook = manifest
        .get("logbook_id")
        .and_then(Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());
    if manifest_logbook != Some(logbook_id) {
        errors.push("backup logbook_id does not match target".to_owned());
    }
    let events = backup
        .get("official_events")
        .cloned()
        .unwrap_or_else(|| json!([]));
    let parsed = serde_json::from_value::<Vec<CoreEventEnvelope>>(events);
    let events = match parsed {
        Ok(events) => events,
        Err(_) => {
            return BackupValidationPlan {
                ok: false,
                errors: vec!["official_events could not deserialize".to_owned()],
                warnings,
                events: Vec::new(),
                head_hash: None,
                missing_credential_references: Vec::new(),
                support_sections: backup_support_sections(backup),
            }
        }
    };
    let mut previous_hash = None;
    let mut seen_event_ids = HashSet::new();
    for event in &events {
        if !seen_event_ids.insert(event.event_id) {
            errors.push(format!("event {} appears more than once", event.event_id));
        }
        if event.logbook_id != logbook_id {
            errors.push(format!(
                "event {} belongs to another logbook",
                event.event_id
            ));
        }
        if !event.hash_is_valid() {
            errors.push(format!("event {} has an invalid hash", event.event_id));
        }
        if event.previous_hash != previous_hash {
            errors.push(format!(
                "event {} breaks previous_hash continuity",
                event.event_id
            ));
        }
        previous_hash = Some(event.event_hash.clone());
    }

    let missing_credential_references = backup
        .get("provider_settings")
        .and_then(Value::as_array)
        .map(|settings| {
            settings
                .iter()
                .filter_map(|setting| {
                    let provider_id = setting
                        .get("provider_id")
                        .and_then(Value::as_str)
                        .unwrap_or("unknown-provider");
                    if setting
                        .get("credential_id")
                        .is_none_or(|value| value.is_null())
                    {
                        Some(provider_id.to_owned())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !missing_credential_references.is_empty() {
        warnings.push(format!(
            "{} provider settings will require credentials after restore",
            missing_credential_references.len()
        ));
    }

    BackupValidationPlan {
        ok: errors.is_empty(),
        errors,
        warnings,
        events,
        head_hash: previous_hash,
        missing_credential_references,
        support_sections: backup_support_sections(backup),
    }
}

fn backup_support_sections(backup: &Value) -> Vec<String> {
    [
        "station_profiles",
        "equipment_profiles",
        "provider_settings",
        "upload_queue_history",
        "map_settings",
    ]
    .into_iter()
    .filter(|section| !backup.get(section).unwrap_or(&Value::Null).is_null())
    .map(str::to_owned)
    .collect()
}

fn restore_backup_support_metadata(
    state: &mut ServerState,
    account_id: Uuid,
    logbook_id: Uuid,
    backup: &Value,
) -> Result<Vec<String>, ApiError> {
    let mut restored = Vec::new();

    if let Some(station_profiles) = backup.get("station_profiles").and_then(Value::as_array) {
        for value in station_profiles {
            let mut profile: HostedStationProfile =
                serde_json::from_value(value.clone()).map_err(|error| {
                    ApiError::BadRequest(format!("invalid station profile backup record: {error}"))
                })?;
            profile.account_id = account_id;
            profile.logbook_id = logbook_id;
            state
                .station_profiles
                .insert(profile.profile.station_profile_id, profile);
        }
        restored.push("station_profiles".to_owned());
    }

    if let Some(equipment_profiles) = backup.get("equipment_profiles").and_then(Value::as_array) {
        for value in equipment_profiles {
            let mut equipment: HostedEquipmentProfile = serde_json::from_value(value.clone())
                .map_err(|error| {
                    ApiError::BadRequest(format!("invalid equipment backup record: {error}"))
                })?;
            equipment.account_id = account_id;
            equipment.logbook_id = logbook_id;
            state
                .equipment_profiles
                .insert(equipment.equipment.equipment_id, equipment);
        }
        restored.push("equipment_profiles".to_owned());
    }

    if let Some(provider_settings) = backup.get("provider_settings").and_then(Value::as_array) {
        for value in provider_settings {
            let mut setting: HostedProviderSetting = serde_json::from_value(value.clone())
                .map_err(|error| {
                    ApiError::BadRequest(format!("invalid provider setting backup record: {error}"))
                })?;
            validate_secret_free_config(&setting.config)?;
            setting.account_id = account_id;
            setting.logbook_id = logbook_id;
            setting.credential_id = None;
            setting.updated_at = Utc::now();
            state.provider_settings.insert(
                provider_setting_key(logbook_id, &setting.provider_id),
                setting,
            );
        }
        restored.push("provider_settings_without_secrets".to_owned());
    }

    if let Some(upload_jobs) = backup.get("upload_queue_history").and_then(Value::as_array) {
        for value in upload_jobs {
            let mut job: HostedUploadJob =
                serde_json::from_value(value.clone()).map_err(|error| {
                    ApiError::BadRequest(format!("invalid upload history backup record: {error}"))
                })?;
            job.account_id = account_id;
            job.logbook_id = logbook_id;
            if job.status == HostedUploadStatus::Running {
                job.status = HostedUploadStatus::Retryable;
                job.failure_reason =
                    Some("restored from backup while job state was running".to_owned());
            }
            state.upload_jobs.insert(job.upload_id, job);
        }
        restored.push("upload_queue_history".to_owned());
    }

    if let Some(value) = backup.get("map_settings").filter(|value| !value.is_null()) {
        let mut settings: HostedMapSettings =
            serde_json::from_value(value.clone()).map_err(|error| {
                ApiError::BadRequest(format!("invalid map settings backup record: {error}"))
            })?;
        settings.account_id = account_id;
        settings.logbook_id = logbook_id;
        settings.updated_at = Utc::now();
        state.map_settings.insert(logbook_id, settings);
        restored.push("map_preferences".to_owned());
    }

    Ok(restored)
}

fn route_catalog() -> RouteCatalogResponse {
    RouteCatalogResponse {
        implemented: vec![
            "GET /health".to_owned(),
            "GET /api/v1/status".to_owned(),
            "POST /api/v1/auth/login".to_owned(),
            "POST /api/v1/auth/logout".to_owned(),
            "GET /api/v1/auth/session".to_owned(),
            "GET /api/v1/logbooks".to_owned(),
            "POST /api/v1/logbooks".to_owned(),
            "GET /api/v1/logbooks/:id".to_owned(),
            "PATCH /api/v1/logbooks/:id".to_owned(),
            "GET /api/v1/qsos".to_owned(),
            "POST /api/v1/qsos".to_owned(),
            "GET /api/v1/qsos/:id".to_owned(),
            "PATCH /api/v1/qsos/:id".to_owned(),
            "POST /api/v1/qsos/:id/delete".to_owned(),
            "POST /api/v1/qsos/:id/restore".to_owned(),
            "POST /api/v1/qsos/:id/notes".to_owned(),
            "GET /api/v1/station-profiles".to_owned(),
            "POST /api/v1/station-profiles".to_owned(),
            "GET /api/v1/station-profiles/:id".to_owned(),
            "PATCH /api/v1/station-profiles/:id".to_owned(),
            "POST /api/v1/station-profiles/:id/archive".to_owned(),
            "POST /api/v1/station-profiles/:id/set-default".to_owned(),
            "GET /api/v1/equipment".to_owned(),
            "POST /api/v1/equipment".to_owned(),
            "GET /api/v1/equipment/:id".to_owned(),
            "PATCH /api/v1/equipment/:id".to_owned(),
            "POST /api/v1/equipment/:id/archive".to_owned(),
            "POST /api/v1/adif/import".to_owned(),
            "GET /api/v1/adif/export".to_owned(),
            "GET /api/v1/activations".to_owned(),
            "POST /api/v1/activations".to_owned(),
            "GET /api/v1/activations/:id".to_owned(),
            "PATCH /api/v1/activations/:id".to_owned(),
            "POST /api/v1/activations/:id/end".to_owned(),
            "GET /api/v1/activations/:id/qsos".to_owned(),
            "GET /api/v1/net-control/sessions".to_owned(),
            "POST /api/v1/net-control/sessions".to_owned(),
            "GET /api/v1/net-control/sessions/:id".to_owned(),
            "PATCH /api/v1/net-control/sessions/:id".to_owned(),
            "POST /api/v1/net-control/sessions/:id/start".to_owned(),
            "POST /api/v1/net-control/sessions/:id/end".to_owned(),
            "POST /api/v1/net-control/sessions/:id/checkins".to_owned(),
            "PATCH /api/v1/net-control/sessions/:id/checkins/:id".to_owned(),
            "POST /api/v1/net-control/sessions/:id/traffic".to_owned(),
            "GET /api/v1/maps/qsos".to_owned(),
            "GET /api/v1/maps/stations".to_owned(),
            "GET /api/v1/maps/paths".to_owned(),
            "GET /api/v1/maps/settings".to_owned(),
            "PATCH /api/v1/maps/settings".to_owned(),
            "POST /api/v1/backups/export".to_owned(),
            "GET /api/v1/backups".to_owned(),
            "GET /api/v1/backups/:id".to_owned(),
            "GET /api/v1/backups/:id/download".to_owned(),
            "POST /api/v1/backups/import/dry-run".to_owned(),
            "POST /api/v1/backups/import".to_owned(),
            "GET /api/v1/providers".to_owned(),
            "GET /api/v1/providers/:id".to_owned(),
            "PATCH /api/v1/providers/:id".to_owned(),
            "POST /api/v1/providers/:id/test".to_owned(),
            "GET /api/v1/uploads".to_owned(),
            "POST /api/v1/uploads/run".to_owned(),
            "POST /api/v1/uploads/:id/retry".to_owned(),
            "GET /api/v1/sync/status".to_owned(),
            "POST /api/v1/sync/preview".to_owned(),
            "POST /api/v1/sync/push".to_owned(),
            "POST /api/v1/sync/pull".to_owned(),
            "POST /api/v1/sync/divergence/review".to_owned(),
            "GET /api/v1/sync/divergence/:id".to_owned(),
            "POST /api/v1/sync/divergence/:id/export".to_owned(),
            "GET /api/v1/devices".to_owned(),
            "POST /api/v1/devices".to_owned(),
            "POST /api/v1/devices/:id/revoke".to_owned(),
        ],
        scaffolded: SCAFFOLDED_ROUTES
            .iter()
            .map(|route| (*route).to_owned())
            .collect(),
    }
}

const SCAFFOLDED_ROUTES: &[&str] = &[];

fn is_scaffolded_route(method: &str, segments: &[&str]) -> bool {
    let route = format!("{method} {}", route_pattern(segments));
    SCAFFOLDED_ROUTES.contains(&route.as_str())
}

fn route_pattern(segments: &[&str]) -> String {
    let mut normalized = Vec::new();
    for (index, segment) in segments.iter().enumerate() {
        if index > 2
            && (Uuid::parse_str(segment).is_ok()
                || segments.get(index.wrapping_sub(1)).is_some_and(|previous| {
                    matches!(
                        *previous,
                        "station-profiles"
                            | "equipment"
                            | "activations"
                            | "sessions"
                            | "checkins"
                            | "providers"
                            | "uploads"
                            | "backups"
                            | "divergence"
                            | "devices"
                    )
                }))
        {
            normalized.push(":id");
        } else {
            normalized.push(segment);
        }
    }
    format!("/{}", normalized.join("/"))
}

fn logbook_id_from_query(request: &ApiRequest) -> Result<Uuid, ApiError> {
    request
        .query
        .get("logbook_id")
        .ok_or_else(|| ApiError::BadRequest("logbook_id query parameter is required".to_owned()))
        .and_then(|value| parse_uuid(value))
}

fn parse_json<T: for<'de> Deserialize<'de>>(body: &[u8]) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|error| ApiError::BadRequest(error.to_string()))
}

fn parse_uuid(value: &str) -> Result<Uuid, ApiError> {
    Uuid::parse_str(value).map_err(|_| ApiError::BadRequest(format!("invalid uuid: {value}")))
}

fn bearer_token(request: &ApiRequest) -> Option<String> {
    request
        .headers
        .get("authorization")
        .or_else(|| request.headers.get("Authorization"))
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::to_owned)
}

fn json_response<T: Serialize>(status: u16, payload: &T) -> ApiResponse {
    ApiResponse {
        status,
        body: serde_json::to_vec(payload).expect("API payload should serialize"),
    }
}

fn json_error(status: u16, message: impl Into<String>) -> ApiResponse {
    json_response(
        status,
        &json!({
            "error": message.into(),
            "retryable": false
        }),
    )
}

pub fn split_target(target: &str) -> (&str, &str) {
    target
        .split_once('?')
        .map_or((target, ""), |(path, query)| (path, query))
}

pub fn parse_query(query: &str) -> HashMap<String, String> {
    query
        .split('&')
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (key, value) = part.split_once('=').unwrap_or((part, ""));
            (key.to_owned(), value.replace('+', " "))
        })
        .collect()
}

fn path_segments(path: &str) -> Vec<&str> {
    path.trim_matches('/')
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn surreal_test_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!("ke8ygw-ham-server-{label}-{}", Uuid::new_v4()))
    }

    fn open_surreal_test_server(path: &PathBuf) -> HostedServer {
        let mut last_error = None;
        for _ in 0..20 {
            let event_path = path.with_extension("events.jsonl");
            match HostedServer::with_surreal_paths(path, event_path) {
                Ok(server) => return server,
                Err(error) => {
                    last_error = Some(error);
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
            }
        }
        panic!(
            "failed to open SurrealDB test server: {}",
            last_error.unwrap()
        );
    }

    async fn login(server: &HostedServer, email: &str) -> (String, Uuid, Uuid) {
        let response = server
            .handle(ApiRequest::json(
                "POST",
                "/api/v1/auth/login",
                &LoginRequest {
                    email: email.to_owned(),
                    display_name: None,
                    device_name: Some(format!("{email} device")),
                },
            ))
            .await;
        assert_eq!(response.status, 200);
        let login: LoginResponse = response.json();
        (
            login.session.token,
            login.logbooks[0].logbook_id,
            login.device.device_id,
        )
    }

    async fn create_qso(server: &HostedServer, token: &str, logbook_id: Uuid) -> Value {
        let response = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/qsos",
                    &QsoWriteRequest {
                        logbook_id,
                        contacted_callsign: Some("k1abc".to_owned()),
                        station_callsign: Some("KE8YGW".to_owned()),
                        operator_callsign: Some("KE8YGW".to_owned()),
                        started_at: Some("2026-07-08T00:00:00Z".to_owned()),
                        mode: Some("ssb".to_owned()),
                        band: Some("20m".to_owned()),
                        frequency_hz: Some(14_250_000),
                        notes: Some("first contact".to_owned()),
                        fields: Map::new(),
                    },
                )
                .with_bearer(token),
            )
            .await;
        assert_eq!(response.status, 200);
        response.json()
    }

    #[tokio::test]
    async fn qso_lifecycle_uses_api_boundary_and_proposals() {
        let server = HostedServer::new();
        let (token, logbook_id, _) = login(&server, "owner@example.test").await;

        let created = create_qso(&server, &token, logbook_id).await;
        let qso_id = created["event"]["entity_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap();

        let list = server
            .handle(
                ApiRequest::get(format!("/api/v1/qsos?logbook_id={logbook_id}"))
                    .with_bearer(&token),
            )
            .await;
        assert_eq!(list.status, 200);
        let list: QsoListResponse = list.json();
        assert_eq!(list.qsos.len(), 1);
        assert_eq!(list.qsos[0].payload["contacted_callsign"], "K1ABC");

        let edited = server
            .handle(
                ApiRequest::json(
                    "PATCH",
                    format!("/api/v1/qsos/{qso_id}"),
                    &QsoWriteRequest {
                        logbook_id,
                        contacted_callsign: None,
                        station_callsign: None,
                        operator_callsign: None,
                        started_at: None,
                        mode: Some("cw".to_owned()),
                        band: None,
                        frequency_hz: None,
                        notes: None,
                        fields: Map::new(),
                    },
                )
                .with_bearer(&token),
            )
            .await;
        assert_eq!(edited.status, 200);

        let note = server
            .handle(
                ApiRequest::json(
                    "POST",
                    format!("/api/v1/qsos/{qso_id}/notes"),
                    &QsoActionRequest {
                        logbook_id,
                        note: Some("confirmed by email".to_owned()),
                    },
                )
                .with_bearer(&token),
            )
            .await;
        assert_eq!(note.status, 200);

        let deleted = server
            .handle(
                ApiRequest::json(
                    "POST",
                    format!("/api/v1/qsos/{qso_id}/delete"),
                    &QsoActionRequest {
                        logbook_id,
                        note: None,
                    },
                )
                .with_bearer(&token),
            )
            .await;
        assert_eq!(deleted.status, 200);

        let restored = server
            .handle(
                ApiRequest::json(
                    "POST",
                    format!("/api/v1/qsos/{qso_id}/restore"),
                    &QsoActionRequest {
                        logbook_id,
                        note: None,
                    },
                )
                .with_bearer(&token),
            )
            .await;
        assert_eq!(restored.status, 200);

        let detail = server
            .handle(
                ApiRequest::get(format!("/api/v1/qsos/{qso_id}?logbook_id={logbook_id}"))
                    .with_bearer(&token),
            )
            .await;
        assert_eq!(detail.status, 200);
        let detail: Value = detail.json();
        assert_eq!(detail["qso"]["payload"]["mode"], "CW");
        assert_eq!(detail["qso"]["deleted"], false);
        assert_eq!(detail["qso"]["note_history"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn user_cannot_read_or_mutate_another_users_logbook() {
        let server = HostedServer::new();
        let (_owner_token, owner_logbook, _) = login(&server, "a@example.test").await;
        let (other_token, _, _) = login(&server, "b@example.test").await;

        let read = server
            .handle(
                ApiRequest::get(format!("/api/v1/qsos?logbook_id={owner_logbook}"))
                    .with_bearer(&other_token),
            )
            .await;
        assert_eq!(read.status, 403);

        let write = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/qsos",
                    &QsoWriteRequest {
                        logbook_id: owner_logbook,
                        contacted_callsign: Some("K1ABC".to_owned()),
                        station_callsign: None,
                        operator_callsign: None,
                        started_at: Some("2026-07-08T00:00:00Z".to_owned()),
                        mode: Some("SSB".to_owned()),
                        band: None,
                        frequency_hz: None,
                        notes: None,
                        fields: Map::new(),
                    },
                )
                .with_bearer(&other_token),
            )
            .await;
        assert_eq!(write.status, 403);
    }

    #[tokio::test]
    async fn operator_can_log_and_viewer_cannot_mutate_qsos() {
        let server = HostedServer::new();
        let (owner_token, logbook_id, _) = login(&server, "owner@example.test").await;
        let (operator_token, _, _) = login(&server, "op@example.test").await;
        let (viewer_token, _, _) = login(&server, "viewer@example.test").await;
        server
            .add_membership_for_email("op@example.test", logbook_id, LogbookRole::Operator)
            .await
            .unwrap();
        server
            .add_membership_for_email("viewer@example.test", logbook_id, LogbookRole::Viewer)
            .await
            .unwrap();

        let op_created = create_qso(&server, &operator_token, logbook_id).await;
        assert!(op_created["event"]["event_hash"].is_string());

        let viewer_write = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/qsos",
                    &QsoWriteRequest {
                        logbook_id,
                        contacted_callsign: Some("N0CALL".to_owned()),
                        station_callsign: None,
                        operator_callsign: None,
                        started_at: Some("2026-07-08T00:00:00Z".to_owned()),
                        mode: Some("SSB".to_owned()),
                        band: None,
                        frequency_hz: None,
                        notes: None,
                        fields: Map::new(),
                    },
                )
                .with_bearer(&viewer_token),
            )
            .await;
        assert_eq!(viewer_write.status, 403);

        let owner_read = server
            .handle(
                ApiRequest::get(format!("/api/v1/qsos?logbook_id={logbook_id}"))
                    .with_bearer(owner_token),
            )
            .await;
        assert_eq!(owner_read.status, 200);
    }

    #[tokio::test]
    async fn logout_invalidates_session() {
        let server = HostedServer::new();
        let (token, _, _) = login(&server, "owner@example.test").await;
        let logout = server
            .handle(ApiRequest::json("POST", "/api/v1/auth/logout", &json!({})).with_bearer(&token))
            .await;
        assert_eq!(logout.status, 200);

        let session = server
            .handle(ApiRequest::get("/api/v1/auth/session").with_bearer(token))
            .await;
        assert_eq!(session.status, 401);
    }

    #[tokio::test]
    async fn revoked_device_token_cannot_sync() {
        let server = HostedServer::new();
        let (token, _, device_id) = login(&server, "owner@example.test").await;

        let revoke = server
            .handle(
                ApiRequest::json(
                    "POST",
                    format!("/api/v1/devices/{device_id}/revoke"),
                    &json!({}),
                )
                .with_bearer(&token),
            )
            .await;
        assert_eq!(revoke.status, 200);

        let sync = server
            .handle(ApiRequest::get("/api/v1/sync/status").with_bearer(token))
            .await;
        assert_eq!(sync.status, 401);
    }

    #[tokio::test]
    async fn route_catalog_lists_scaffolded_v0_2_api_surface() {
        let server = HostedServer::new();
        let response = server.handle(ApiRequest::get("/api/v1/routes")).await;
        assert_eq!(response.status, 200);
        let catalog: RouteCatalogResponse = response.json();
        assert!(catalog
            .implemented
            .contains(&"POST /api/v1/qsos".to_owned()));
        assert!(catalog
            .implemented
            .contains(&"POST /api/v1/adif/import".to_owned()));
        assert!(catalog
            .implemented
            .contains(&"GET /api/v1/activations".to_owned()));
        assert!(catalog
            .implemented
            .contains(&"POST /api/v1/sync/divergence/review".to_owned()));
        assert!(catalog.scaffolded.is_empty());
    }

    #[tokio::test]
    async fn unknown_routes_return_not_found() {
        let server = HostedServer::new();
        let response = server.handle(ApiRequest::get("/api/v1/not-a-route")).await;
        assert_eq!(response.status, 404);
    }

    #[tokio::test]
    async fn station_and_equipment_routes_are_scoped_role_checked_and_durable() {
        let path = surreal_test_path("station-equipment");
        let server = open_surreal_test_server(&path);
        let (owner_token, logbook_id, _) = login(&server, "owner@example.test").await;
        let (viewer_token, _, _) = login(&server, "viewer@example.test").await;
        let (other_token, other_logbook, _) = login(&server, "other@example.test").await;
        server
            .add_membership_for_email("viewer@example.test", logbook_id, LogbookRole::Viewer)
            .await
            .unwrap();

        let created = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/station-profiles",
                    &StationProfileRequest {
                        logbook_id,
                        display_name: Some("Home HF".to_owned()),
                        station_callsign: Some("ke8ygw".to_owned()),
                        operator_callsign: Some("KE8YGW".to_owned()),
                        default_grid: Some("EN80".to_owned()),
                        default_qth: None,
                        default_power_watts: Some(100),
                        notes: None,
                        tags: vec!["home".to_owned()],
                        active: Some(true),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(created.status, 200);
        let created: Value = created.json();
        let profile_id = created["station_profile"]["profile"]["station_profile_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap();
        assert_eq!(
            created["station_profile"]["profile"]["station_callsign"],
            "KE8YGW"
        );

        let viewer_write = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/station-profiles",
                    &StationProfileRequest {
                        logbook_id,
                        display_name: Some("Blocked".to_owned()),
                        station_callsign: Some("N0CALL".to_owned()),
                        operator_callsign: None,
                        default_grid: None,
                        default_qth: None,
                        default_power_watts: None,
                        notes: None,
                        tags: vec![],
                        active: None,
                    },
                )
                .with_bearer(&viewer_token),
            )
            .await;
        assert_eq!(viewer_write.status, 403);

        let patched = server
            .handle(
                ApiRequest::json(
                    "PATCH",
                    format!("/api/v1/station-profiles/{profile_id}"),
                    &StationProfileRequest {
                        logbook_id,
                        display_name: Some("Home HF Updated".to_owned()),
                        station_callsign: None,
                        operator_callsign: None,
                        default_grid: None,
                        default_qth: None,
                        default_power_watts: Some(50),
                        notes: None,
                        tags: vec![],
                        active: None,
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(patched.status, 200);

        let equipment = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/equipment",
                    &EquipmentProfileRequest {
                        logbook_id,
                        equipment_type: Some(EquipmentType::Radio),
                        display_name: Some("IC-7300".to_owned()),
                        manufacturer: Some("Icom".to_owned()),
                        model: Some("IC-7300".to_owned()),
                        serial_number: None,
                        capabilities: vec!["hf".to_owned()],
                        notes: None,
                        status: None,
                        station_profile_id: Some(profile_id),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(equipment.status, 200);
        let equipment: Value = equipment.json();
        let equipment_id = equipment["equipment"]["equipment"]["equipment_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap();

        let cross_logbook = server
            .handle(
                ApiRequest::get(format!(
                    "/api/v1/station-profiles/{profile_id}?logbook_id={other_logbook}"
                ))
                .with_bearer(&other_token),
            )
            .await;
        assert_eq!(cross_logbook.status, 404);

        let archived = server
            .handle(
                ApiRequest::json(
                    "POST",
                    format!("/api/v1/equipment/{equipment_id}/archive"),
                    &QsoActionRequest {
                        logbook_id,
                        note: None,
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(archived.status, 200);
        assert_eq!(
            archived.json::<Value>()["equipment"]["equipment"]["status"],
            "retired"
        );

        server.reload_metadata_from_store().await.unwrap();
        let after_reload = server
            .handle(
                ApiRequest::get(format!(
                    "/api/v1/station-profiles/{profile_id}?logbook_id={logbook_id}"
                ))
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(after_reload.status, 200);
        let equipment_after_reload = server
            .handle(
                ApiRequest::get(format!(
                    "/api/v1/equipment/{equipment_id}?logbook_id={logbook_id}"
                ))
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(equipment_after_reload.status, 200);
    }

    #[tokio::test]
    async fn activation_routes_use_official_proposals_and_roles() {
        let server = HostedServer::new();
        let (owner_token, logbook_id, _) = login(&server, "owner@example.test").await;
        let (operator_token, _, _) = login(&server, "operator@example.test").await;
        let (viewer_token, _, _) = login(&server, "viewer@example.test").await;
        let (other_token, other_logbook, _) = login(&server, "other@example.test").await;
        server
            .add_membership_for_email("operator@example.test", logbook_id, LogbookRole::Operator)
            .await
            .unwrap();
        server
            .add_membership_for_email("viewer@example.test", logbook_id, LogbookRole::Viewer)
            .await
            .unwrap();

        let operator_write = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/activations",
                    &ActivationWriteRequest {
                        logbook_id,
                        activation_type: "pota".to_owned(),
                        station_callsign: Some("KE8YGW".to_owned()),
                        operator_callsign: Some("KE8YGW".to_owned()),
                        started_at: Some("2026-07-08T12:00:00Z".to_owned()),
                        ended_at: None,
                        park_id: Some("US-1234".to_owned()),
                        summit_id: None,
                        reference: None,
                        name: Some("Park".to_owned()),
                        notes: None,
                        fields: Map::new(),
                    },
                )
                .with_bearer(&operator_token),
            )
            .await;
        assert_eq!(operator_write.status, 403);

        let created = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/activations",
                    &ActivationWriteRequest {
                        logbook_id,
                        activation_type: "pota".to_owned(),
                        station_callsign: Some("KE8YGW".to_owned()),
                        operator_callsign: Some("KE8YGW".to_owned()),
                        started_at: Some("2026-07-08T12:00:00Z".to_owned()),
                        ended_at: None,
                        park_id: Some("US-1234".to_owned()),
                        summit_id: None,
                        reference: None,
                        name: Some("Park".to_owned()),
                        notes: None,
                        fields: Map::new(),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(created.status, 200);
        let created: Value = created.json();
        let activation_id = created["event"]["entity_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap();
        assert_eq!(
            created["event"]["event_type"],
            "official.log.activation.started"
        );

        let viewer_write = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/activations",
                    &ActivationWriteRequest {
                        logbook_id,
                        activation_type: "pota".to_owned(),
                        station_callsign: Some("KE8YGW".to_owned()),
                        operator_callsign: Some("KE8YGW".to_owned()),
                        started_at: Some("2026-07-08T12:00:00Z".to_owned()),
                        ended_at: None,
                        park_id: Some("US-0001".to_owned()),
                        summit_id: None,
                        reference: None,
                        name: None,
                        notes: None,
                        fields: Map::new(),
                    },
                )
                .with_bearer(&viewer_token),
            )
            .await;
        assert_eq!(viewer_write.status, 403);

        let list = server
            .handle(
                ApiRequest::get(format!("/api/v1/activations?logbook_id={logbook_id}"))
                    .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(list.status, 200);
        assert_eq!(
            list.json::<Value>()["activations"][0]["status"],
            Value::String("active".to_owned())
        );

        let end = server
            .handle(
                ApiRequest::json(
                    "POST",
                    format!("/api/v1/activations/{activation_id}/end"),
                    &ActivationWriteRequest {
                        logbook_id,
                        activation_type: "pota".to_owned(),
                        station_callsign: None,
                        operator_callsign: None,
                        started_at: None,
                        ended_at: Some("2026-07-08T13:00:00Z".to_owned()),
                        park_id: None,
                        summit_id: None,
                        reference: None,
                        name: None,
                        notes: None,
                        fields: Map::new(),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(end.status, 200);
        assert_eq!(
            end.json::<Value>()["event"]["event_type"],
            "official.log.activation.ended"
        );

        let cross = server
            .handle(
                ApiRequest::get(format!(
                    "/api/v1/activations/{activation_id}?logbook_id={other_logbook}"
                ))
                .with_bearer(&other_token),
            )
            .await;
        assert_eq!(cross.status, 404);
    }

    #[tokio::test]
    async fn net_control_routes_use_official_proposals_and_roles() {
        let server = HostedServer::new();
        let (owner_token, logbook_id, _) = login(&server, "owner@example.test").await;
        let (viewer_token, _, _) = login(&server, "viewer@example.test").await;
        server
            .add_membership_for_email("viewer@example.test", logbook_id, LogbookRole::Viewer)
            .await
            .unwrap();

        let created = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/net-control/sessions",
                    &NetSessionWriteRequest {
                        logbook_id,
                        station_callsign: Some("KE8YGW".to_owned()),
                        net_control_operator_id: Some(Uuid::new_v4().to_string()),
                        net_name: Some("Weekly Net".to_owned()),
                        started_at: Some("2026-07-08T00:00:00Z".to_owned()),
                        ended_at: None,
                        frequency_hz: Some(146_520_000),
                        band: Some("2m".to_owned()),
                        mode: Some("FM".to_owned()),
                        notes: None,
                        fields: Map::new(),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(created.status, 200);
        let session_id = created.json::<Value>()["event"]["entity_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap();

        let checkin = server
            .handle(
                ApiRequest::json(
                    "POST",
                    format!("/api/v1/net-control/sessions/{session_id}/checkins"),
                    &NetCheckInWriteRequest {
                        logbook_id,
                        callsign: Some("K1ABC".to_owned()),
                        tactical_callsign: None,
                        tactical_only: None,
                        checkin_time: Some("2026-07-08T00:01:00Z".to_owned()),
                        status: None,
                        traffic: Some("listed".to_owned()),
                        notes: None,
                        fields: Map::new(),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(checkin.status, 200);
        let checkin_id = checkin.json::<Value>()["event"]["entity_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap();

        let updated = server
            .handle(
                ApiRequest::json(
                    "PATCH",
                    format!("/api/v1/net-control/sessions/{session_id}/checkins/{checkin_id}"),
                    &NetCheckInWriteRequest {
                        logbook_id,
                        callsign: Some("K1ABC".to_owned()),
                        tactical_callsign: Some("Alpha".to_owned()),
                        tactical_only: None,
                        checkin_time: None,
                        status: Some("late".to_owned()),
                        traffic: None,
                        notes: None,
                        fields: Map::new(),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(updated.status, 200);

        let traffic = server
            .handle(
                ApiRequest::json(
                    "POST",
                    format!("/api/v1/net-control/sessions/{session_id}/traffic"),
                    &NetTrafficWriteRequest {
                        logbook_id,
                        summary: Some("Need relay".to_owned()),
                        precedence: Some("routine".to_owned()),
                        status: None,
                        handling_notes: None,
                        fields: Map::new(),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(traffic.status, 200);

        let viewer_end = server
            .handle(
                ApiRequest::json(
                    "POST",
                    format!("/api/v1/net-control/sessions/{session_id}/end"),
                    &NetSessionWriteRequest {
                        logbook_id,
                        station_callsign: None,
                        net_control_operator_id: None,
                        net_name: None,
                        started_at: None,
                        ended_at: Some("2026-07-08T01:00:00Z".to_owned()),
                        frequency_hz: None,
                        band: None,
                        mode: None,
                        notes: None,
                        fields: Map::new(),
                    },
                )
                .with_bearer(&viewer_token),
            )
            .await;
        assert_eq!(viewer_end.status, 403);

        let detail = server
            .handle(
                ApiRequest::get(format!(
                    "/api/v1/net-control/sessions/{session_id}?logbook_id={logbook_id}"
                ))
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(detail.status, 200);
        let detail: Value = detail.json();
        assert_eq!(detail["session"]["checkin_count"], 1);
        assert_eq!(detail["traffic"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn adif_import_export_uses_official_events_and_enforces_roles() {
        let server = HostedServer::new();
        let (owner_token, logbook_id, _) = login(&server, "owner@example.test").await;
        let (viewer_token, _, _) = login(&server, "viewer@example.test").await;
        let (other_token, other_logbook, _) = login(&server, "other@example.test").await;
        server
            .add_membership_for_email("viewer@example.test", logbook_id, LogbookRole::Viewer)
            .await
            .unwrap();

        let invalid = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/adif/import",
                    &AdifImportRequest {
                        logbook_id,
                        adif: "<CALL:5>K1ABC<QSO_DATE:8>20260708<TIME_ON:6>120000<EOR>".to_owned(),
                        station_callsign: Some("KE8YGW".to_owned()),
                        operator_callsign: None,
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(invalid.status, 200);
        let invalid: Value = invalid.json();
        assert_eq!(invalid["summary"]["rejected_count"], 1);

        let viewer_import = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/adif/import",
                    &AdifImportRequest {
                        logbook_id,
                        adif: String::new(),
                        station_callsign: None,
                        operator_callsign: None,
                    },
                )
                .with_bearer(&viewer_token),
            )
            .await;
        assert_eq!(viewer_import.status, 403);

        let valid_adif =
            "<CALL:5>K1ABC<QSO_DATE:8>20260705<TIME_ON:6>120000<BAND:3>20m<MODE:3>SSB<EOR>";
        let imported = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/adif/import",
                    &AdifImportRequest {
                        logbook_id,
                        adif: valid_adif.to_owned(),
                        station_callsign: Some("KE8YGW".to_owned()),
                        operator_callsign: Some("KE8YGW".to_owned()),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(imported.status, 200);
        let imported: Value = imported.json();
        assert_eq!(imported["summary"]["imported_count"], 1);
        assert_eq!(imported["head"]["event_count"], 1);

        let exported = server
            .handle(
                ApiRequest::get(format!("/api/v1/adif/export?logbook_id={logbook_id}"))
                    .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(exported.status, 200);
        let exported: Value = exported.json();
        assert!(exported["adif"].as_str().unwrap().contains("K1ABC"));

        let other_export = server
            .handle(
                ApiRequest::get(format!("/api/v1/adif/export?logbook_id={other_logbook}"))
                    .with_bearer(&other_token),
            )
            .await;
        assert_eq!(other_export.status, 200);
        let other_export: Value = other_export.json();
        assert!(!other_export["adif"].as_str().unwrap().contains("K1ABC"));
    }

    #[tokio::test]
    async fn provider_settings_and_upload_queue_are_redacted_scoped_and_durable() {
        const TEST_SECRET: &str = "TEST_SECRET_SHOULD_NOT_APPEAR";
        let path = surreal_test_path("providers-upload");
        let server = open_surreal_test_server(&path);
        let (owner_token, logbook_id, _) = login(&server, "owner@example.test").await;
        let (viewer_token, _, _) = login(&server, "viewer@example.test").await;
        server
            .add_membership_for_email("viewer@example.test", logbook_id, LogbookRole::Viewer)
            .await
            .unwrap();
        create_qso(&server, &owner_token, logbook_id).await;

        let raw_secret = server
            .handle(
                ApiRequest::json(
                    "PATCH",
                    "/api/v1/providers/lotw-stub",
                    &ProviderPatchRequest {
                        logbook_id,
                        enabled: Some(true),
                        credential_id: None,
                        config: Map::from_iter([(
                            "token".to_owned(),
                            Value::String(TEST_SECRET.to_owned()),
                        )]),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(raw_secret.status, 400);

        let viewer_patch = server
            .handle(
                ApiRequest::json(
                    "PATCH",
                    "/api/v1/providers/lotw-stub",
                    &ProviderPatchRequest {
                        logbook_id,
                        enabled: Some(true),
                        credential_id: Some("cred-viewer".to_owned()),
                        config: Map::new(),
                    },
                )
                .with_bearer(&viewer_token),
            )
            .await;
        assert_eq!(viewer_patch.status, 403);

        let patched = server
            .handle(
                ApiRequest::json(
                    "PATCH",
                    "/api/v1/providers/lotw-stub",
                    &ProviderPatchRequest {
                        logbook_id,
                        enabled: Some(true),
                        credential_id: Some("cred-lotw".to_owned()),
                        config: Map::from_iter([("mock_mode".to_owned(), Value::Bool(true))]),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(patched.status, 200);
        let patched: Value = patched.json();
        assert!(!patched.to_string().contains(TEST_SECRET));

        let test = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/providers/lotw-stub/test",
                    &ProviderTestRequest { logbook_id },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(test.status, 200);
        let test = test.json::<Value>();
        assert_eq!(test["test_status"], "ok");
        assert_eq!(test["credential_reference_status"], "mock_bypassed");
        assert_eq!(test["credential_reference_resolves"], true);
        assert!(!test.to_string().contains(TEST_SECRET));

        let failed = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/uploads/run",
                    &UploadRunRequest {
                        logbook_id,
                        provider_id: "lotw-stub".to_owned(),
                        qso_ids: None,
                        force_fail: Some(true),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(failed.status, 200);
        let failed: Value = failed.json();
        assert_eq!(failed["upload"]["status"], "retryable");
        let upload_id = failed["upload"]["upload_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap();

        let retried = server
            .handle(
                ApiRequest::json(
                    "POST",
                    format!("/api/v1/uploads/{upload_id}/retry"),
                    &QsoActionRequest {
                        logbook_id,
                        note: None,
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(retried.status, 200);
        let retried: Value = retried.json();
        assert_eq!(retried["upload"]["status"], "succeeded");
        assert!(!retried.to_string().contains(TEST_SECRET));

        server.reload_metadata_from_store().await.unwrap();
        let uploads = server
            .handle(
                ApiRequest::get(format!("/api/v1/uploads?logbook_id={logbook_id}"))
                    .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(uploads.status, 200);
        let uploads: Value = uploads.json();
        assert_eq!(uploads["uploads"].as_array().unwrap().len(), 1);
        assert_eq!(uploads["uploads"][0]["status"], "succeeded");
        assert!(!uploads.to_string().contains(TEST_SECRET));
    }

    #[tokio::test]
    async fn map_routes_derive_from_projection_and_persist_settings() {
        let path = surreal_test_path("maps");
        let server = open_surreal_test_server(&path);
        let (owner_token, logbook_id, _) = login(&server, "owner@example.test").await;
        let (viewer_token, _, _) = login(&server, "viewer@example.test").await;
        let (other_token, other_logbook, _) = login(&server, "other@example.test").await;
        server
            .add_membership_for_email("viewer@example.test", logbook_id, LogbookRole::Viewer)
            .await
            .unwrap();

        let station = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/station-profiles",
                    &StationProfileRequest {
                        logbook_id,
                        display_name: Some("Home".to_owned()),
                        station_callsign: Some("KE8YGW".to_owned()),
                        operator_callsign: None,
                        default_grid: Some("EN80".to_owned()),
                        default_qth: None,
                        default_power_watts: None,
                        notes: None,
                        tags: vec![],
                        active: Some(true),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(station.status, 200);

        let mut fields = Map::new();
        fields.insert("grid".to_owned(), Value::String("FN31".to_owned()));
        create_qso(&server, &owner_token, logbook_id).await;
        let mapped_qso = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/qsos",
                    &QsoWriteRequest {
                        logbook_id,
                        contacted_callsign: Some("W1AW".to_owned()),
                        station_callsign: Some("KE8YGW".to_owned()),
                        operator_callsign: Some("KE8YGW".to_owned()),
                        started_at: Some("2026-07-08T00:10:00Z".to_owned()),
                        mode: Some("SSB".to_owned()),
                        band: Some("20m".to_owned()),
                        frequency_hz: None,
                        notes: None,
                        fields,
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(mapped_qso.status, 200);

        let markers = server
            .handle(
                ApiRequest::get(format!("/api/v1/maps/qsos?logbook_id={logbook_id}"))
                    .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(markers.status, 200);
        let markers: Value = markers.json();
        assert_eq!(markers["markers"].as_array().unwrap().len(), 1);
        assert_eq!(markers["markers"][0]["marker"]["title"], "W1AW");

        let other_markers = server
            .handle(
                ApiRequest::get(format!("/api/v1/maps/qsos?logbook_id={other_logbook}"))
                    .with_bearer(&other_token),
            )
            .await;
        assert_eq!(other_markers.status, 200);
        assert!(other_markers.json::<Value>()["markers"]
            .as_array()
            .unwrap()
            .is_empty());

        let viewer_patch = server
            .handle(
                ApiRequest::json(
                    "PATCH",
                    "/api/v1/maps/settings",
                    &MapSettingsPatchRequest {
                        logbook_id,
                        layer_id: Some("grid".to_owned()),
                        enabled: Some(false),
                        order: None,
                    },
                )
                .with_bearer(&viewer_token),
            )
            .await;
        assert_eq!(viewer_patch.status, 403);

        let settings = server
            .handle(
                ApiRequest::json(
                    "PATCH",
                    "/api/v1/maps/settings",
                    &MapSettingsPatchRequest {
                        logbook_id,
                        layer_id: Some("grid".to_owned()),
                        enabled: Some(false),
                        order: Some(5),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(settings.status, 200);
        server.reload_metadata_from_store().await.unwrap();
        let settings = server
            .handle(
                ApiRequest::get(format!("/api/v1/maps/settings?logbook_id={logbook_id}"))
                    .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(settings.status, 200);
        let settings: Value = settings.json();
        let grid = settings["map_settings"]["layers"]["layers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|layer| layer["layer_id"] == "grid")
            .unwrap();
        assert_eq!(grid["enabled"], false);
        assert_eq!(grid["order"], 5);
    }

    #[tokio::test]
    async fn backup_export_and_dry_run_are_scoped_and_redacted() {
        const TEST_SECRET: &str = "TEST_SECRET_SHOULD_NOT_APPEAR";
        let path = surreal_test_path("backups");
        let server = open_surreal_test_server(&path);
        let (owner_token, logbook_id, _) = login(&server, "owner@example.test").await;
        let (other_token, other_logbook, _) = login(&server, "other@example.test").await;
        create_qso(&server, &owner_token, logbook_id).await;
        let patched = server
            .handle(
                ApiRequest::json(
                    "PATCH",
                    "/api/v1/providers/lotw-stub",
                    &ProviderPatchRequest {
                        logbook_id,
                        enabled: Some(true),
                        credential_id: Some("cred-lotw".to_owned()),
                        config: Map::from_iter([("mock_mode".to_owned(), Value::Bool(true))]),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(patched.status, 200);
        let raw_secret = server
            .handle(
                ApiRequest::json(
                    "PATCH",
                    "/api/v1/providers/lotw-stub",
                    &ProviderPatchRequest {
                        logbook_id,
                        enabled: None,
                        credential_id: None,
                        config: Map::from_iter([(
                            "client_secret".to_owned(),
                            Value::String(TEST_SECRET.to_owned()),
                        )]),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(raw_secret.status, 400);

        let backup = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/backups/export",
                    &BackupExportRequest {
                        logbook_id,
                        include_runtime_logs: None,
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(backup.status, 200);
        let backup: Value = backup.json();
        assert_eq!(
            backup["backup"]["manifest"]["format_version"],
            Value::Number(1.into())
        );
        assert_eq!(
            backup["backup"]["payload"]["official_events"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(!backup.to_string().contains(TEST_SECRET));

        let dry_run = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/backups/import/dry-run",
                    &BackupDryRunRequest {
                        logbook_id,
                        backup: backup["backup"]["payload"].clone(),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(dry_run.status, 200);
        assert_eq!(dry_run.json::<Value>()["ok"], true);

        let invalid = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/backups/import/dry-run",
                    &BackupDryRunRequest {
                        logbook_id,
                        backup: json!({"manifest": {"format_version": 99, "logbook_id": logbook_id}}),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(invalid.status, 200);
        assert_eq!(invalid.json::<Value>()["ok"], false);

        let cross = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/backups/export",
                    &BackupExportRequest {
                        logbook_id,
                        include_runtime_logs: None,
                    },
                )
                .with_bearer(&other_token),
            )
            .await;
        assert_eq!(cross.status, 403);

        let other_backup = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/backups/export",
                    &BackupExportRequest {
                        logbook_id: other_logbook,
                        include_runtime_logs: None,
                    },
                )
                .with_bearer(&other_token),
            )
            .await;
        assert_eq!(other_backup.status, 200);
        assert!(
            other_backup.json::<Value>()["backup"]["payload"]["official_events"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[tokio::test]
    async fn backup_import_restores_events_support_metadata_and_blocks_unsafe_cases() {
        let path = surreal_test_path("backup-import");
        let server = open_surreal_test_server(&path);
        let (owner_token, logbook_id, _) = login(&server, "owner@example.test").await;
        let (viewer_token, _, _) = login(&server, "viewer@example.test").await;
        let (other_token, _, _) = login(&server, "other@example.test").await;
        server
            .add_membership_for_email("viewer@example.test", logbook_id, LogbookRole::Viewer)
            .await
            .unwrap();

        create_qso(&server, &owner_token, logbook_id).await;
        let station = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/station-profiles",
                    &StationProfileRequest {
                        logbook_id,
                        display_name: Some("Restore Station".to_owned()),
                        station_callsign: Some("KE8YGW".to_owned()),
                        operator_callsign: None,
                        default_grid: Some("EN80".to_owned()),
                        default_qth: None,
                        default_power_watts: Some(100),
                        notes: None,
                        tags: vec![],
                        active: Some(true),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(station.status, 200);
        let provider = server
            .handle(
                ApiRequest::json(
                    "PATCH",
                    "/api/v1/providers/lotw-stub",
                    &ProviderPatchRequest {
                        logbook_id,
                        enabled: Some(true),
                        credential_id: Some("cred-lotw".to_owned()),
                        config: Map::from_iter([("mock_mode".to_owned(), Value::Bool(true))]),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(provider.status, 200);

        let backup = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/backups/export",
                    &BackupExportRequest {
                        logbook_id,
                        include_runtime_logs: None,
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(backup.status, 200);
        let payload = backup.json::<Value>()["backup"]["payload"].clone();

        let viewer_import = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/backups/import",
                    &BackupImportRequest {
                        logbook_id,
                        backup: payload.clone(),
                        confirm_dry_run: true,
                    },
                )
                .with_bearer(&viewer_token),
            )
            .await;
        assert_eq!(viewer_import.status, 403);

        let cross_scope = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/backups/import",
                    &BackupImportRequest {
                        logbook_id,
                        backup: payload.clone(),
                        confirm_dry_run: true,
                    },
                )
                .with_bearer(&other_token),
            )
            .await;
        assert_eq!(cross_scope.status, 403);

        let import = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/backups/import",
                    &BackupImportRequest {
                        logbook_id,
                        backup: payload.clone(),
                        confirm_dry_run: true,
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(import.status, 200);
        let import: Value = import.json();
        assert_eq!(import["imported_official_events_count"], 0);
        assert_eq!(import["skipped_duplicate_count"], 1);
        assert_eq!(import["projection_rebuild"]["qso_count"], 1);
        assert!(import["restored_support_sections"]
            .as_array()
            .unwrap()
            .iter()
            .any(|section| section == "provider_settings_without_secrets"));

        let provider_after_restore = server
            .handle(
                ApiRequest::get(format!(
                    "/api/v1/providers/lotw-stub?logbook_id={logbook_id}"
                ))
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(provider_after_restore.status, 200);
        assert!(provider_after_restore.json::<Value>()["setting"]["credential_id"].is_null());

        server.reload_metadata_from_store().await.unwrap();
        let qsos = server
            .handle(
                ApiRequest::get(format!("/api/v1/qsos?logbook_id={logbook_id}"))
                    .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(qsos.status, 200);
        assert_eq!(qsos.json::<Value>()["qsos"].as_array().unwrap().len(), 1);
        let profiles = server
            .handle(
                ApiRequest::get(format!("/api/v1/station-profiles?logbook_id={logbook_id}"))
                    .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(profiles.status, 200);
        assert_eq!(
            profiles.json::<Value>()["station_profiles"]
                .as_array()
                .unwrap()
                .len(),
            1
        );

        let mut invalid_payload = payload.clone();
        invalid_payload["official_events"][0]["event_hash"] = json!("bad-hash");
        let invalid = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/backups/import",
                    &BackupImportRequest {
                        logbook_id,
                        backup: invalid_payload,
                        confirm_dry_run: true,
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(invalid.status, 400);

        create_qso(&server, &owner_token, logbook_id).await;
        let divergent = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/backups/import",
                    &BackupImportRequest {
                        logbook_id,
                        backup: payload,
                        confirm_dry_run: true,
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(divergent.status, 400);
    }

    #[tokio::test]
    async fn divergence_review_reports_safe_and_divergent_states() {
        let path = surreal_test_path("divergence");
        let server = open_surreal_test_server(&path);
        let (owner_token, logbook_id, device_id) = login(&server, "owner@example.test").await;
        create_qso(&server, &owner_token, logbook_id).await;

        let review = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/sync/divergence/review",
                    &DivergenceReviewRequest {
                        logbook_id,
                        local_head_hash: None,
                        client_events: vec![],
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(review.status, 200);
        let review: Value = review.json();
        assert_eq!(review["review"]["can_safely_pull"], true);
        assert_eq!(review["review"]["divergence_detected"], false);
        let report_id = review["report_id"]
            .as_str()
            .and_then(|value| Uuid::parse_str(value).ok())
            .unwrap();

        let divergent = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/sync/divergence/review",
                    &DivergenceReviewRequest {
                        logbook_id,
                        local_head_hash: Some("not-on-server".to_owned()),
                        client_events: vec![],
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(divergent.status, 200);
        assert_eq!(
            divergent.json::<Value>()["review"]["divergence_detected"],
            true
        );

        server.reload_metadata_from_store().await.unwrap();
        let report = server
            .handle(
                ApiRequest::get(format!(
                    "/api/v1/sync/divergence/{report_id}?logbook_id={logbook_id}"
                ))
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(report.status, 200);

        let revoke = server
            .handle(
                ApiRequest::json(
                    "POST",
                    format!("/api/v1/devices/{device_id}/revoke"),
                    &json!({}),
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(revoke.status, 200);
        let denied = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/sync/divergence/review",
                    &DivergenceReviewRequest {
                        logbook_id,
                        local_head_hash: None,
                        client_events: vec![],
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(denied.status, 401);
    }

    #[tokio::test]
    async fn sync_pull_returns_allowed_missing_events_and_respects_revocation() {
        let server = HostedServer::new();
        let (owner_token, logbook_id, _) = login(&server, "owner@example.test").await;
        create_qso(&server, &owner_token, logbook_id).await;

        let pull = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/sync/pull",
                    &PreviewSyncRequest {
                        logbook_id,
                        local_head_hash: None,
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(pull.status, 200);
        let pull: CloudPullEventsResponse = pull.json();
        assert_eq!(pull.events.len(), 1);
        let head = pull.events[0].event_hash.clone();

        let duplicate_push = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/sync/push",
                    &CloudPushEventsRequest {
                        auth: ham_sync::CloudAuth {
                            sync_token: "unused-by-hosted-bearer".to_owned(),
                        },
                        logbook_id,
                        events: pull.events.clone(),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(duplicate_push.status, 200);
        let duplicate_push: Value = duplicate_push.json();
        assert_eq!(duplicate_push["ignored_duplicate_count"], 1);

        let in_sync = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/sync/pull",
                    &PreviewSyncRequest {
                        logbook_id,
                        local_head_hash: Some(head),
                    },
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(in_sync.status, 200);
        let in_sync: CloudPullEventsResponse = in_sync.json();
        assert!(in_sync.events.is_empty());

        let (second_token, _, second_device) = login(&server, "owner@example.test").await;
        let revoke = server
            .handle(
                ApiRequest::json(
                    "POST",
                    format!("/api/v1/devices/{second_device}/revoke"),
                    &json!({}),
                )
                .with_bearer(&owner_token),
            )
            .await;
        assert_eq!(revoke.status, 200);
        let revoked_pull = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/sync/pull",
                    &PreviewSyncRequest {
                        logbook_id,
                        local_head_hash: None,
                    },
                )
                .with_bearer(&second_token),
            )
            .await;
        assert_eq!(revoked_pull.status, 401);
    }

    #[tokio::test]
    async fn surreal_metadata_preserves_user_session_logbook_and_device_after_store_reload() {
        let path = surreal_test_path("metadata-restart");
        let server = open_surreal_test_server(&path);
        let (token, logbook_id, device_id) = login(&server, "owner@example.test").await;
        server.reload_metadata_from_store().await.unwrap();
        let session = server
            .handle(ApiRequest::get("/api/v1/auth/session").with_bearer(&token))
            .await;
        assert_eq!(session.status, 200);
        let session: SessionResponse = session.json();
        assert_eq!(session.device.device_id, device_id);
        assert_eq!(session.memberships[0].logbook_id, logbook_id);

        let logbooks = server
            .handle(ApiRequest::get("/api/v1/logbooks").with_bearer(token))
            .await;
        assert_eq!(logbooks.status, 200);
        let payload: Value = logbooks.json();
        assert_eq!(payload["logbooks"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn surreal_metadata_preserves_logout_and_device_revocation_after_store_reload() {
        let path = surreal_test_path("logout-revoke");
        let server = open_surreal_test_server(&path);
        let (token, _, device_id) = login(&server, "owner@example.test").await;

        let logout = server
            .handle(ApiRequest::json("POST", "/api/v1/auth/logout", &json!({})).with_bearer(&token))
            .await;
        assert_eq!(logout.status, 200);
        server.reload_metadata_from_store().await.unwrap();
        let session = server
            .handle(ApiRequest::get("/api/v1/auth/session").with_bearer(&token))
            .await;
        assert_eq!(session.status, 401);

        let (new_token, _, _) = login(&server, "owner@example.test").await;
        let revoke = server
            .handle(
                ApiRequest::json(
                    "POST",
                    format!("/api/v1/devices/{device_id}/revoke"),
                    &json!({}),
                )
                .with_bearer(&new_token),
            )
            .await;
        assert_eq!(revoke.status, 200);
        server.reload_metadata_from_store().await.unwrap();
        let sync = server
            .handle(ApiRequest::get("/api/v1/sync/status").with_bearer(token))
            .await;
        assert_eq!(sync.status, 401);
    }

    #[tokio::test]
    async fn surreal_metadata_preserves_membership_roles_and_scope_after_store_reload() {
        let path = surreal_test_path("roles");
        let server = open_surreal_test_server(&path);
        let (owner_token, logbook_id, _) = login(&server, "owner@example.test").await;
        let (operator_token, _, _) = login(&server, "operator@example.test").await;
        let (viewer_token, _, _) = login(&server, "viewer@example.test").await;
        let (other_token, _, _) = login(&server, "other@example.test").await;
        server
            .add_membership_for_email("operator@example.test", logbook_id, LogbookRole::Operator)
            .await
            .unwrap();
        server
            .add_membership_for_email("viewer@example.test", logbook_id, LogbookRole::Viewer)
            .await
            .unwrap();
        server.reload_metadata_from_store().await.unwrap();
        let op_created = create_qso(&server, &operator_token, logbook_id).await;
        assert!(op_created["event"]["event_hash"].is_string());

        let viewer_write = server
            .handle(
                ApiRequest::json(
                    "POST",
                    "/api/v1/qsos",
                    &QsoWriteRequest {
                        logbook_id,
                        contacted_callsign: Some("N0CALL".to_owned()),
                        station_callsign: None,
                        operator_callsign: None,
                        started_at: Some("2026-07-08T00:00:00Z".to_owned()),
                        mode: Some("SSB".to_owned()),
                        band: None,
                        frequency_hz: None,
                        notes: None,
                        fields: Map::new(),
                    },
                )
                .with_bearer(&viewer_token),
            )
            .await;
        assert_eq!(viewer_write.status, 403);

        let other_read = server
            .handle(
                ApiRequest::get(format!("/api/v1/qsos?logbook_id={logbook_id}"))
                    .with_bearer(other_token),
            )
            .await;
        assert_eq!(other_read.status, 403);

        let owner_read = server
            .handle(
                ApiRequest::get(format!("/api/v1/qsos?logbook_id={logbook_id}"))
                    .with_bearer(owner_token),
            )
            .await;
        assert_eq!(owner_read.status, 200);
    }
}
