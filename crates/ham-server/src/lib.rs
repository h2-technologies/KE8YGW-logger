use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use chrono::{DateTime, Utc};
use ham_core::{
    default_log_directory, default_service_registry, submit_proposal, InMemoryEventBus,
    InMemoryLogbookEventStore, LogbookEventStore, OperatorRole, ProposalContext,
};
use ham_plugin_sdk::{
    PluginCapability, PluginManifest, ProposalEnvelope, PROPOSAL_QSO_CORRECT, PROPOSAL_QSO_CREATE,
    PROPOSAL_QSO_DELETE, PROPOSAL_QSO_NOTE_ADD, PROPOSAL_QSO_RESTORE,
};
use ham_sync::{
    preview_pull_from_events, CloudPushEventsRequest, LogbookHeadSummary, PreviewPullRequest,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use thiserror::Error;
use tokio::sync::RwLock;
use uuid::Uuid;

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
    #[error("metadata store SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("metadata store serialization error: {0}")]
    Serde(#[from] serde_json::Error),
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
pub struct SqliteHostedMetadataStore {
    path: PathBuf,
}

impl SqliteHostedMetadataStore {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, MetadataStoreError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let store = Self { path };
        store.initialize_schema()?;
        Ok(store)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn connection(&self) -> Result<Connection, MetadataStoreError> {
        Ok(Connection::open(&self.path)?)
    }

    fn initialize_schema(&self) -> Result<(), MetadataStoreError> {
        let connection = self.connection()?;
        connection.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS users (
                account_id TEXT PRIMARY KEY NOT NULL,
                email TEXT NOT NULL UNIQUE,
                payload TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS login_sessions (
                token TEXT PRIMARY KEY NOT NULL,
                account_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                active INTEGER NOT NULL,
                payload TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS devices (
                device_id TEXT PRIMARY KEY NOT NULL,
                account_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                revoked INTEGER NOT NULL,
                payload TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS logbooks (
                logbook_id TEXT PRIMARY KEY NOT NULL,
                account_id TEXT NOT NULL,
                payload TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS logbook_memberships (
                logbook_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                account_id TEXT NOT NULL,
                role TEXT NOT NULL,
                payload TEXT NOT NULL,
                PRIMARY KEY (logbook_id, user_id)
            );
            CREATE TABLE IF NOT EXISTS api_tokens (
                token_id TEXT PRIMARY KEY NOT NULL,
                account_id TEXT NOT NULL,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                revoked INTEGER NOT NULL,
                payload TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS server_invites (
                invite_id TEXT PRIMARY KEY NOT NULL,
                account_id TEXT NOT NULL,
                logbook_id TEXT NOT NULL,
                token TEXT NOT NULL UNIQUE,
                payload TEXT NOT NULL
            );
            INSERT OR IGNORE INTO schema_migrations(version, applied_at)
            VALUES (1, datetime('now'));
            "#,
        )?;
        Ok(())
    }
}

impl HostedMetadataStore for SqliteHostedMetadataStore {
    fn load(&self) -> Result<ServerState, MetadataStoreError> {
        let connection = self.connection()?;
        let mut state = ServerState::default();

        for account in load_payloads::<UserAccount>(&connection, "users")? {
            state
                .users_by_email
                .insert(account.email.clone(), account.account_id);
            state.accounts.insert(account.account_id, account);
        }
        for session in load_payloads::<LoginSession>(&connection, "login_sessions")? {
            state
                .sessions_by_token
                .insert(session.token.clone(), session);
        }
        for device in load_payloads::<DeviceIdentity>(&connection, "devices")? {
            state.devices.insert(device.device_id, device);
        }
        for logbook in load_payloads::<ApiLogbook>(&connection, "logbooks")? {
            state.logbooks.insert(logbook.logbook_id, logbook);
        }
        state.memberships = load_payloads::<LogbookMembership>(&connection, "logbook_memberships")?;
        for token in load_payloads::<ApiToken>(&connection, "api_tokens")? {
            state.api_tokens.insert(token.token_id, token);
        }
        for invite in load_payloads::<ServerInvite>(&connection, "server_invites")? {
            state.invites.insert(invite.invite_id, invite);
        }

        Ok(state)
    }

    fn save(&self, state: &ServerState) -> Result<(), MetadataStoreError> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        for table in [
            "users",
            "login_sessions",
            "devices",
            "logbooks",
            "logbook_memberships",
            "api_tokens",
            "server_invites",
        ] {
            transaction.execute(&format!("DELETE FROM {table}"), [])?;
        }

        for account in state.accounts.values() {
            transaction.execute(
                "INSERT INTO users(account_id, email, payload) VALUES (?1, ?2, ?3)",
                params![
                    account.account_id.to_string(),
                    account.email,
                    serde_json::to_string(account)?
                ],
            )?;
        }
        for session in state.sessions_by_token.values() {
            transaction.execute(
                "INSERT INTO login_sessions(token, account_id, user_id, device_id, active, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    session.token,
                    session.account_id.to_string(),
                    session.user_id.to_string(),
                    session.device_id.to_string(),
                    if session.active { 1_i64 } else { 0_i64 },
                    serde_json::to_string(session)?
                ],
            )?;
        }
        for device in state.devices.values() {
            transaction.execute(
                "INSERT INTO devices(device_id, account_id, user_id, revoked, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    device.device_id.to_string(),
                    device.account_id.to_string(),
                    device.user_id.to_string(),
                    if device.revoked { 1_i64 } else { 0_i64 },
                    serde_json::to_string(device)?
                ],
            )?;
        }
        for logbook in state.logbooks.values() {
            transaction.execute(
                "INSERT INTO logbooks(logbook_id, account_id, payload) VALUES (?1, ?2, ?3)",
                params![
                    logbook.logbook_id.to_string(),
                    logbook.account_id.to_string(),
                    serde_json::to_string(logbook)?
                ],
            )?;
        }
        for membership in &state.memberships {
            transaction.execute(
                "INSERT INTO logbook_memberships(logbook_id, user_id, account_id, role, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    membership.logbook_id.to_string(),
                    membership.user_id.to_string(),
                    membership.account_id.to_string(),
                    serde_json::to_string(&membership.role)?,
                    serde_json::to_string(membership)?
                ],
            )?;
        }
        for token in state.api_tokens.values() {
            transaction.execute(
                "INSERT INTO api_tokens(token_id, account_id, user_id, device_id, revoked, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    token.token_id.to_string(),
                    token.account_id.to_string(),
                    token.user_id.to_string(),
                    token.device_id.to_string(),
                    if token.revoked { 1_i64 } else { 0_i64 },
                    serde_json::to_string(token)?
                ],
            )?;
        }
        for invite in state.invites.values() {
            transaction.execute(
                "INSERT INTO server_invites(invite_id, account_id, logbook_id, token, payload)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    invite.invite_id.to_string(),
                    invite.account_id.to_string(),
                    invite.logbook_id.to_string(),
                    invite.token,
                    serde_json::to_string(invite)?
                ],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    fn is_durable(&self) -> bool {
        true
    }

    fn label(&self) -> String {
        self.path.display().to_string()
    }
}

fn load_payloads<T: for<'de> Deserialize<'de>>(
    connection: &Connection,
    table: &str,
) -> Result<Vec<T>, MetadataStoreError> {
    let mut statement = connection.prepare(&format!("SELECT payload FROM {table}"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut values = Vec::new();
    for row in rows {
        values.push(serde_json::from_str(&row?)?);
    }
    Ok(values)
}

#[derive(Debug, Clone)]
pub struct HostedServer {
    state: Arc<RwLock<ServerState>>,
    metadata_store: Arc<dyn HostedMetadataStore>,
    store: Arc<InMemoryLogbookEventStore>,
    bus: Arc<InMemoryEventBus>,
}

pub fn default_metadata_db_path() -> PathBuf {
    std::env::var("HAM_SERVER_METADATA_DB").map_or_else(
        |_| {
            default_log_directory()
                .join("server")
                .join("ham-server.sqlite3")
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

    pub fn with_sqlite_metadata(path: impl Into<PathBuf>) -> Result<Self, MetadataStoreError> {
        let metadata_store = Arc::new(SqliteHostedMetadataStore::open(path)?);
        Self::with_metadata_store(metadata_store)
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
            ("GET", ["api", "v1", "providers"]) => self.providers(&request).await,
            ("GET", ["api", "v1", "sync", "status"]) => self.sync_status(&request).await,
            ("POST", ["api", "v1", "sync", "preview"]) => self.sync_preview(&request).await,
            ("POST", ["api", "v1", "sync", "push"]) => self.sync_push(&request).await,
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

    async fn providers(&self, request: &ApiRequest) -> Result<Value, ApiError> {
        self.authorize(request).await?;
        let snapshot = default_service_registry().snapshot();
        Ok(
            json!({"providers": snapshot.providers, "preferred_providers": snapshot.preferred_providers}),
        )
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
            "GET /api/v1/providers".to_owned(),
            "GET /api/v1/sync/status".to_owned(),
            "POST /api/v1/sync/preview".to_owned(),
            "POST /api/v1/sync/push".to_owned(),
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

const SCAFFOLDED_ROUTES: &[&str] = &[
    "POST /api/v1/adif/import",
    "GET /api/v1/adif/export",
    "GET /api/v1/station-profiles",
    "POST /api/v1/station-profiles",
    "PATCH /api/v1/station-profiles/:id",
    "POST /api/v1/station-profiles/:id/archive",
    "GET /api/v1/equipment",
    "POST /api/v1/equipment",
    "PATCH /api/v1/equipment/:id",
    "POST /api/v1/equipment/:id/archive",
    "GET /api/v1/activations",
    "POST /api/v1/activations",
    "PATCH /api/v1/activations/:id",
    "GET /api/v1/net-control/sessions",
    "POST /api/v1/net-control/sessions",
    "PATCH /api/v1/net-control/sessions/:id",
    "POST /api/v1/net-control/sessions/:id/checkins",
    "GET /api/v1/maps/qsos",
    "GET /api/v1/maps/settings",
    "PATCH /api/v1/maps/settings",
    "GET /api/v1/providers/:id",
    "PATCH /api/v1/providers/:id",
    "POST /api/v1/providers/:id/test",
    "GET /api/v1/uploads",
    "POST /api/v1/uploads/run",
    "POST /api/v1/uploads/:id/retry",
    "POST /api/v1/sync/pull",
];

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
                            | "providers"
                            | "uploads"
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

    fn sqlite_test_path(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ke8ygw-ham-server-{label}-{}.sqlite3",
            Uuid::new_v4()
        ))
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
            .scaffolded
            .contains(&"POST /api/v1/adif/import".to_owned()));
    }

    #[tokio::test]
    async fn scaffolded_string_id_routes_are_reserved() {
        let server = HostedServer::new();
        let (token, _, _) = login(&server, "owner@example.test").await;
        let response = server
            .handle(ApiRequest::get("/api/v1/providers/lotw").with_bearer(token))
            .await;
        assert_eq!(response.status, 200);
        let payload: Value = response.json();
        assert_eq!(payload["implemented"], false);
    }

    #[tokio::test]
    async fn sqlite_metadata_preserves_user_session_logbook_and_device_after_restart() {
        let path = sqlite_test_path("metadata-restart");
        let server = HostedServer::with_sqlite_metadata(&path).unwrap();
        let (token, logbook_id, device_id) = login(&server, "owner@example.test").await;

        let restarted = HostedServer::with_sqlite_metadata(&path).unwrap();
        let session = restarted
            .handle(ApiRequest::get("/api/v1/auth/session").with_bearer(&token))
            .await;
        assert_eq!(session.status, 200);
        let session: SessionResponse = session.json();
        assert_eq!(session.device.device_id, device_id);
        assert_eq!(session.memberships[0].logbook_id, logbook_id);

        let logbooks = restarted
            .handle(ApiRequest::get("/api/v1/logbooks").with_bearer(token))
            .await;
        assert_eq!(logbooks.status, 200);
        let payload: Value = logbooks.json();
        assert_eq!(payload["logbooks"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn sqlite_metadata_preserves_logout_and_device_revocation_after_restart() {
        let path = sqlite_test_path("logout-revoke");
        let server = HostedServer::with_sqlite_metadata(&path).unwrap();
        let (token, _, device_id) = login(&server, "owner@example.test").await;

        let logout = server
            .handle(ApiRequest::json("POST", "/api/v1/auth/logout", &json!({})).with_bearer(&token))
            .await;
        assert_eq!(logout.status, 200);
        let restarted = HostedServer::with_sqlite_metadata(&path).unwrap();
        let session = restarted
            .handle(ApiRequest::get("/api/v1/auth/session").with_bearer(&token))
            .await;
        assert_eq!(session.status, 401);

        let (new_token, _, _) = login(&restarted, "owner@example.test").await;
        let revoke = restarted
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
        let restarted_again = HostedServer::with_sqlite_metadata(&path).unwrap();
        let sync = restarted_again
            .handle(ApiRequest::get("/api/v1/sync/status").with_bearer(token))
            .await;
        assert_eq!(sync.status, 401);
    }

    #[tokio::test]
    async fn sqlite_metadata_preserves_membership_roles_and_scope_after_restart() {
        let path = sqlite_test_path("roles");
        let server = HostedServer::with_sqlite_metadata(&path).unwrap();
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

        let restarted = HostedServer::with_sqlite_metadata(&path).unwrap();
        let op_created = create_qso(&restarted, &operator_token, logbook_id).await;
        assert!(op_created["event"]["event_hash"].is_string());

        let viewer_write = restarted
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

        let other_read = restarted
            .handle(
                ApiRequest::get(format!("/api/v1/qsos?logbook_id={logbook_id}"))
                    .with_bearer(other_token),
            )
            .await;
        assert_eq!(other_read.status, 403);

        let owner_read = restarted
            .handle(
                ApiRequest::get(format!("/api/v1/qsos?logbook_id={logbook_id}"))
                    .with_bearer(owner_token),
            )
            .await;
        assert_eq!(owner_read.status, 200);
    }
}
