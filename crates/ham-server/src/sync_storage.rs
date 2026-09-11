//! Durable SurrealDB-backed storage for self-hosted sync and report metadata.
//!
//! The wire models and protocol rules live in `ham_core::sync`; only the
//! server-side persistence lives here, so client and iOS builds never link
//! SurrealDB.

use std::{collections::HashSet, fs, path::PathBuf, sync::Arc, thread};

use chrono::Utc;
use ham_core::sync::*;
use ham_core::{
    default_log_directory, validate_supported_remote_event, JsonlLogbookEventStore,
    LogbookEventStore,
};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
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
use tokio::runtime::Runtime;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DurableCloudSyncServer {
    config: CloudServerConfig,
    store: Arc<JsonlLogbookEventStore>,
    metadata: Arc<SurrealCloudMetadataStore>,
    reports_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DurableCloudSyncPaths {
    pub metadata_store_path: PathBuf,
    pub official_event_log_path: PathBuf,
    pub report_dir: PathBuf,
}

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

#[derive(Debug, Clone)]
struct StoredReportRef {
    metadata: DiagnosticReportMetadata,
    bundle_path: PathBuf,
}

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

#[derive(Debug, Clone)]
pub struct SurrealCloudConfig {
    pub endpoint: SurrealCloudEndpoint,
    pub namespace: String,
    pub database: String,
}

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

#[derive(Clone)]
enum SurrealCloudClient {
    Local(Surreal<Db>),
    Remote(Surreal<Any>),
}

impl std::fmt::Debug for SurrealCloudClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Local(_) => formatter.write_str("Local(Surreal<Db>)"),
            Self::Remote(_) => formatter.write_str("Remote(Surreal<Any>)"),
        }
    }
}

#[derive(Debug, Clone)]
struct SurrealCloudMetadataStore {
    runtime: Arc<std::sync::Mutex<Option<Runtime>>>,
    client: Arc<std::sync::Mutex<Option<SurrealCloudClient>>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CloudPayloadRow<T> {
    payload: T,
}

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
            if cloud_session_is_expired(&session, Utc::now()) {
                return Err(CloudSyncError::Unauthenticated);
            }
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

fn sync_token_hash(token: &str) -> String {
    format!("{:x}", Sha256::digest(token.as_bytes()))
}

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
        let issued_at = Utc::now();
        let session = CloudSession {
            account_id: request.account_id,
            user_id: request.user_id,
            device_id: request.device_id,
            device_name: request.device_name,
            sync_token: token,
            authorized_logbooks: request.requested_logbooks,
            issued_at,
            expires_at: cloud_session_expiry(issued_at, self.config.sync_session_ttl_seconds),
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

#[cfg(test)]
mod tests {
    use super::*;
    use ham_core::{CoreEventEnvelope, NewLogbookEvent};
    use serde_json::json;

    const EVENT_QSO_CREATED: &str = "official.log.qso.created";
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

    fn durable_paths(label: &str) -> DurableCloudSyncPaths {
        let root = std::env::temp_dir().join(format!("ke8ygw-ham-sync-{label}-{}", Uuid::new_v4()));
        DurableCloudSyncPaths {
            metadata_store_path: root.join("surrealdb"),
            official_event_log_path: root.join("official-events.jsonl"),
            report_dir: root.join("reports"),
        }
    }

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
