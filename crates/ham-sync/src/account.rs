//! Shared hosted account and session client contract.
//!
//! Every client surface (hosted web, desktop, native iOS, and the CLI) drives
//! the hosted `/api/v1/auth/*`, `/api/v1/devices*`, and account routes through
//! this module. Rust owns request planning, response interpretation, outcome
//! classification, and the durable non-secret account support record. Platform
//! layers only carry bytes over their own transport and store the returned
//! session and refresh secrets in their own secure storage.
//!
//! Secrets are never written to the support record. Rust persists only
//! credential identifiers; the raw session and refresh tokens are handed back
//! exactly once in [`HostedAccountResult`] and are excluded from every
//! serialized form of the result and snapshot.

use std::{
    fmt, fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use thiserror::Error;
use uuid::Uuid;

use crate::offline::{quarantine_file, write_json_atomically};

/// Support-file schema version for the durable hosted account record.
pub const HOSTED_ACCOUNT_FILE_VERSION: u32 = 1;
/// Credential provider identifier used for hosted account secrets.
pub const HOSTED_ACCOUNT_CREDENTIAL_PROVIDER_ID: &str = "hosted-account";
/// Credential label for the hosted session bearer token.
pub const HOSTED_ACCOUNT_SESSION_CREDENTIAL_LABEL: &str = "Hosted session token";
/// Credential label for the hosted refresh token.
pub const HOSTED_ACCOUNT_REFRESH_CREDENTIAL_LABEL: &str = "Hosted refresh token";
/// Default hosted API base URL, matching the `ham-server` development bind.
pub const DEFAULT_HOSTED_ACCOUNT_BASE_URL: &str = "http://127.0.0.1:9750";
/// Default device label sent to the hosted server when a session is created.
pub const DEFAULT_HOSTED_ACCOUNT_DEVICE_NAME: &str = "KE8YGW Logger Device";
/// Default per-request timeout for hosted account calls.
pub const DEFAULT_HOSTED_ACCOUNT_TIMEOUT_SECONDS: u64 = 20;
/// Maximum accepted hosted account response size.
pub const MAX_HOSTED_ACCOUNT_RESPONSE_BYTES: usize = 512 * 1024;

const MAX_BASE_URL_BYTES: usize = 2048;
const MAX_EMAIL_BYTES: usize = 320;
const MAX_DISPLAY_NAME_BYTES: usize = 200;
const MAX_DEVICE_NAME_BYTES: usize = 128;
const MAX_TOKEN_BYTES: usize = 1024;
const MAX_CACHED_DEVICES: usize = 200;
const MAX_CACHED_LOGBOOKS: usize = 200;

/// Stable action names shared by every client surface.
pub const ACCOUNT_ACTION_REGISTER: &str = "account.register";
pub const ACCOUNT_ACTION_VERIFY_EMAIL: &str = "account.verify_email";
pub const ACCOUNT_ACTION_RECOVERY_START: &str = "account.recovery.start";
pub const ACCOUNT_ACTION_RECOVERY_COMPLETE: &str = "account.recovery.complete";
pub const ACCOUNT_ACTION_LOGIN: &str = "account.login";
pub const ACCOUNT_ACTION_SESSION: &str = "account.session";
pub const ACCOUNT_ACTION_SESSION_ROTATE: &str = "account.session.rotate";
pub const ACCOUNT_ACTION_LOGOUT: &str = "account.logout";
pub const ACCOUNT_ACTION_LOGOUT_ALL: &str = "account.logout_all";
pub const ACCOUNT_ACTION_DELETE: &str = "account.delete";
pub const ACCOUNT_ACTION_DEVICE_LIST: &str = "account.devices.list";
pub const ACCOUNT_ACTION_DEVICE_REGISTER: &str = "account.devices.register";
pub const ACCOUNT_ACTION_DEVICE_REVOKE: &str = "account.devices.revoke";
pub const ACCOUNT_ACTION_DEVICE_REVOKE_ALL: &str = "account.devices.revoke_all";

#[derive(Debug, Error)]
pub enum HostedAccountError {
    #[error("hosted account I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("hosted account serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("hosted account file version {0} is not supported")]
    UnsupportedFileVersion(u32),
    #[error("hosted base URL is invalid: {0}")]
    InvalidBaseUrl(String),
    #[error("email address is invalid")]
    InvalidEmail,
    #[error("field `{0}` is required")]
    MissingField(&'static str),
    #[error("field `{field}` is longer than {max} bytes")]
    FieldTooLong { field: &'static str, max: usize },
    #[error("a signed-in hosted session is required")]
    SessionRequired,
    #[error("a stored refresh token is required to rotate the hosted session")]
    RefreshTokenRequired,
    #[error("hosted account secret storage error: {0}")]
    SecretStorage(String),
}

/// Durable, non-secret hosted account configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedAccountConfig {
    pub base_url: String,
    pub device_name: String,
}

impl Default for HostedAccountConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_HOSTED_ACCOUNT_BASE_URL.to_owned(),
            device_name: DEFAULT_HOSTED_ACCOUNT_DEVICE_NAME.to_owned(),
        }
    }
}

impl HostedAccountConfig {
    pub fn normalized(&self) -> Result<Self, HostedAccountError> {
        Ok(Self {
            base_url: normalize_base_url(&self.base_url)?,
            device_name: normalize_device_name(&self.device_name)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostedAccountConnectionState {
    SignedOut,
    PendingEmailVerification,
    SignedIn,
    SessionExpired,
    DeviceRevoked,
}

impl HostedAccountConnectionState {
    pub fn is_signed_in(self) -> bool {
        matches!(self, Self::SignedIn)
    }
}

/// Classification of a hosted account call result.
///
/// The classification is derived from the stable hosted error `code` field
/// first and from the HTTP status only as a fallback, so clients never branch
/// on human-readable messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostedAccountOutcome {
    Accepted,
    AuthenticationRequired,
    EmailVerificationRequired,
    RegistrationClosed,
    TokenExpired,
    TokenReplayed,
    HumanVerificationFailed,
    RateLimited,
    ValidationFailed,
    TransientFailure,
    PermanentFailure,
}

impl HostedAccountOutcome {
    pub fn is_accepted(self) -> bool {
        matches!(self, Self::Accepted)
    }

    /// Whether an unattended retry of the same request is allowed.
    pub fn is_retryable(self) -> bool {
        matches!(self, Self::RateLimited | Self::TransientFailure)
    }

    /// Whether the operator has to do something before the call can succeed.
    pub fn requires_user_action(self) -> bool {
        matches!(
            self,
            Self::AuthenticationRequired
                | Self::EmailVerificationRequired
                | Self::RegistrationClosed
                | Self::TokenExpired
                | Self::TokenReplayed
                | Self::HumanVerificationFailed
                | Self::ValidationFailed
        )
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::AuthenticationRequired => "authentication_required",
            Self::EmailVerificationRequired => "email_verification_required",
            Self::RegistrationClosed => "registration_closed",
            Self::TokenExpired => "token_expired",
            Self::TokenReplayed => "token_replayed",
            Self::HumanVerificationFailed => "human_verification_failed",
            Self::RateLimited => "rate_limited",
            Self::ValidationFailed => "validation_failed",
            Self::TransientFailure => "transient_failure",
            Self::PermanentFailure => "permanent_failure",
        }
    }
}

impl fmt::Display for HostedAccountOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedAccountDevice {
    pub device_id: Uuid,
    pub device_name: String,
    pub trusted: bool,
    pub revoked: bool,
    #[serde(default)]
    pub registered_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub revoked_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedAccountLogbook {
    pub logbook_id: Uuid,
    pub name: String,
    #[serde(default)]
    pub role: Option<String>,
}

/// Redacted hosted account state persisted in the support store.
///
/// This record never contains a session token, refresh token, invitation
/// token, verification token, or recovery token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedAccountSnapshot {
    pub schema_version: u32,
    pub base_url: String,
    pub device_name: String,
    pub connection_state: HostedAccountConnectionState,
    #[serde(default)]
    pub account_id: Option<Uuid>,
    #[serde(default)]
    pub user_id: Option<Uuid>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub email_verified: bool,
    #[serde(default)]
    pub session_id: Option<Uuid>,
    #[serde(default)]
    pub session_issued_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub session_expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub refresh_expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub session_token_credential_id: Option<Uuid>,
    #[serde(default)]
    pub refresh_token_credential_id: Option<Uuid>,
    #[serde(default)]
    pub device_id: Option<Uuid>,
    #[serde(default)]
    pub devices: Vec<HostedAccountDevice>,
    #[serde(default)]
    pub logbooks: Vec<HostedAccountLogbook>,
    #[serde(default)]
    pub pending_email_verification_for: Option<String>,
    #[serde(default)]
    pub pending_recovery_for: Option<String>,
    #[serde(default)]
    pub last_action: Option<String>,
    #[serde(default)]
    pub last_outcome: Option<HostedAccountOutcome>,
    #[serde(default)]
    pub last_error_code: Option<String>,
    #[serde(default)]
    pub last_message: Option<String>,
    #[serde(default)]
    pub last_request_id: Option<String>,
    #[serde(default)]
    pub last_updated_at: Option<DateTime<Utc>>,
}

impl HostedAccountSnapshot {
    pub fn new(config: &HostedAccountConfig) -> Self {
        Self {
            schema_version: HOSTED_ACCOUNT_FILE_VERSION,
            base_url: config.base_url.clone(),
            device_name: config.device_name.clone(),
            connection_state: HostedAccountConnectionState::SignedOut,
            account_id: None,
            user_id: None,
            email: None,
            display_name: None,
            email_verified: false,
            session_id: None,
            session_issued_at: None,
            session_expires_at: None,
            refresh_expires_at: None,
            session_token_credential_id: None,
            refresh_token_credential_id: None,
            device_id: None,
            devices: Vec::new(),
            logbooks: Vec::new(),
            pending_email_verification_for: None,
            pending_recovery_for: None,
            last_action: None,
            last_outcome: None,
            last_error_code: None,
            last_message: None,
            last_request_id: None,
            last_updated_at: None,
        }
    }

    pub fn config(&self) -> HostedAccountConfig {
        HostedAccountConfig {
            base_url: self.base_url.clone(),
            device_name: self.device_name.clone(),
        }
    }

    /// Session credential identifiers the caller must be able to resolve.
    pub fn has_session_credentials(&self) -> bool {
        self.session_token_credential_id.is_some()
    }

    /// Stable, human-readable connection-state label for CLI and log output.
    pub fn connection_state_label(&self) -> &'static str {
        match self.connection_state {
            HostedAccountConnectionState::SignedOut => "signed_out",
            HostedAccountConnectionState::PendingEmailVerification => "pending_email_verification",
            HostedAccountConnectionState::SignedIn => "signed_in",
            HostedAccountConnectionState::SessionExpired => "session_expired",
            HostedAccountConnectionState::DeviceRevoked => "device_revoked",
        }
    }

    fn clear_identity(&mut self) {
        self.account_id = None;
        self.user_id = None;
        self.email = None;
        self.display_name = None;
        self.email_verified = false;
        self.devices.clear();
        self.logbooks.clear();
    }

    fn clear_session(&mut self) {
        self.session_id = None;
        self.session_issued_at = None;
        self.session_expires_at = None;
        self.refresh_expires_at = None;
        self.session_token_credential_id = None;
        self.refresh_token_credential_id = None;
        self.device_id = None;
    }
}

/// Hosted account operations available to every client surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum HostedAccountAction {
    Register {
        email: String,
        #[serde(default)]
        display_name: Option<String>,
        #[serde(default)]
        invitation_token: Option<String>,
        #[serde(default)]
        turnstile_token: Option<String>,
    },
    VerifyEmail {
        token: String,
    },
    RecoveryStart {
        email: String,
    },
    RecoveryComplete {
        token: String,
    },
    Login {
        email: String,
        #[serde(default)]
        display_name: Option<String>,
    },
    Session,
    SessionRotate,
    Logout,
    LogoutAll,
    AccountDelete,
    DeviceList,
    DeviceRegister {
        device_name: String,
    },
    DeviceRevoke {
        device_id: Uuid,
    },
    DeviceRevokeAll,
}

impl HostedAccountAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::Register { .. } => ACCOUNT_ACTION_REGISTER,
            Self::VerifyEmail { .. } => ACCOUNT_ACTION_VERIFY_EMAIL,
            Self::RecoveryStart { .. } => ACCOUNT_ACTION_RECOVERY_START,
            Self::RecoveryComplete { .. } => ACCOUNT_ACTION_RECOVERY_COMPLETE,
            Self::Login { .. } => ACCOUNT_ACTION_LOGIN,
            Self::Session => ACCOUNT_ACTION_SESSION,
            Self::SessionRotate => ACCOUNT_ACTION_SESSION_ROTATE,
            Self::Logout => ACCOUNT_ACTION_LOGOUT,
            Self::LogoutAll => ACCOUNT_ACTION_LOGOUT_ALL,
            Self::AccountDelete => ACCOUNT_ACTION_DELETE,
            Self::DeviceList => ACCOUNT_ACTION_DEVICE_LIST,
            Self::DeviceRegister { .. } => ACCOUNT_ACTION_DEVICE_REGISTER,
            Self::DeviceRevoke { .. } => ACCOUNT_ACTION_DEVICE_REVOKE,
            Self::DeviceRevokeAll => ACCOUNT_ACTION_DEVICE_REVOKE_ALL,
        }
    }

    /// Whether the hosted route needs an authenticated bearer session.
    pub fn requires_session(&self) -> bool {
        matches!(
            self,
            Self::Session
                | Self::SessionRotate
                | Self::Logout
                | Self::LogoutAll
                | Self::AccountDelete
                | Self::DeviceList
                | Self::DeviceRegister { .. }
                | Self::DeviceRevoke { .. }
                | Self::DeviceRevokeAll
        )
    }
}

/// A hosted request the platform transport must execute verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedAccountRequestPlan {
    pub action: String,
    pub method: String,
    pub path: String,
    pub url: String,
    #[serde(default)]
    pub body: Option<JsonValue>,
    pub requires_session_token: bool,
    #[serde(default)]
    pub session_token_credential_id: Option<Uuid>,
    pub requires_refresh_token: bool,
    #[serde(default)]
    pub refresh_token_credential_id: Option<Uuid>,
    #[serde(default)]
    pub refresh_token_body_field: Option<String>,
    pub request_id: Uuid,
    pub timeout_seconds: u64,
    pub max_response_bytes: usize,
}

impl HostedAccountRequestPlan {
    /// Body with the caller-supplied refresh token injected.
    ///
    /// Rust never stores or logs the refresh token; the caller reads it from
    /// its own secure storage and passes it only for this single request.
    pub fn body_with_refresh_token(&self, refresh_token: &str) -> JsonValue {
        let Some(field) = self.refresh_token_body_field.as_deref() else {
            return self.body.clone().unwrap_or_else(|| json!({}));
        };
        let mut body = self.body.clone().unwrap_or_else(|| json!({}));
        if let Some(object) = body.as_object_mut() {
            object.insert(
                field.to_owned(),
                JsonValue::String(refresh_token.to_owned()),
            );
        }
        body
    }
}

/// A hosted response as observed by the platform transport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostedAccountHttpResponse {
    pub status: u16,
    #[serde(default)]
    pub body: Option<JsonValue>,
}

impl HostedAccountHttpResponse {
    pub fn new(status: u16, body: Option<JsonValue>) -> Self {
        Self { status, body }
    }
}

/// Outcome of one hosted account operation.
///
/// `session_token` and `refresh_token` are deliberately excluded from every
/// serialized form. Callers must move them straight into platform secure
/// storage under the returned credential identifiers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostedAccountResult {
    pub action: String,
    pub outcome: HostedAccountOutcome,
    pub status: u16,
    pub message: String,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    pub retryable: bool,
    pub user_action_required: bool,
    pub snapshot: HostedAccountSnapshot,
    /// Credential identifiers whose stored secrets must be deleted.
    #[serde(default)]
    pub cleared_credential_ids: Vec<Uuid>,
    #[serde(skip)]
    session_token: Option<String>,
    #[serde(skip)]
    refresh_token: Option<String>,
}

impl HostedAccountResult {
    /// Session bearer token issued by this call, if any.
    pub fn session_token(&self) -> Option<&str> {
        self.session_token.as_deref()
    }

    /// Refresh token issued by this call, if any.
    pub fn refresh_token(&self) -> Option<&str> {
        self.refresh_token.as_deref()
    }

    /// Moves the issued secrets out so callers cannot copy them twice.
    pub fn take_secrets(&mut self) -> HostedAccountIssuedSecrets {
        HostedAccountIssuedSecrets {
            session_token_credential_id: self.snapshot.session_token_credential_id,
            session_token: self.session_token.take(),
            refresh_token_credential_id: self.snapshot.refresh_token_credential_id,
            refresh_token: self.refresh_token.take(),
        }
    }
}

/// Secrets issued by one hosted account call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostedAccountIssuedSecrets {
    pub session_token_credential_id: Option<Uuid>,
    pub session_token: Option<String>,
    pub refresh_token_credential_id: Option<Uuid>,
    pub refresh_token: Option<String>,
}

/// Platform secure storage for hosted account secrets.
pub trait HostedAccountSecrets {
    fn read_secret(&mut self, credential_id: Uuid) -> Result<Option<String>, HostedAccountError>;
    fn write_secret(
        &mut self,
        credential_id: Uuid,
        label: &str,
        secret: &str,
    ) -> Result<(), HostedAccountError>;
    fn clear_secret(&mut self, credential_id: Uuid) -> Result<(), HostedAccountError>;
}

/// Platform transport for hosted account requests.
pub trait HostedAccountTransport {
    fn execute(
        &self,
        plan: &HostedAccountRequestPlan,
        body: Option<&JsonValue>,
        session_token: Option<&str>,
    ) -> Result<HostedAccountHttpResponse, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HostedAccountFile {
    version: u32,
    snapshot: HostedAccountSnapshot,
}

impl HostedAccountFile {
    fn validate(&self) -> Result<(), HostedAccountError> {
        if self.version != HOSTED_ACCOUNT_FILE_VERSION {
            return Err(HostedAccountError::UnsupportedFileVersion(self.version));
        }
        if self.snapshot.schema_version != HOSTED_ACCOUNT_FILE_VERSION {
            return Err(HostedAccountError::UnsupportedFileVersion(
                self.snapshot.schema_version,
            ));
        }
        normalize_base_url(&self.snapshot.base_url)?;
        Ok(())
    }
}

/// Durable JSON support store for the hosted account record.
#[derive(Debug, Clone)]
pub struct JsonHostedAccountStore {
    path: PathBuf,
}

impl JsonHostedAccountStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads the record, creating it when absent and quarantining it when the
    /// stored JSON cannot be read as a supported record.
    pub fn load_or_initialize(
        &self,
        config: &HostedAccountConfig,
        now: DateTime<Utc>,
    ) -> Result<HostedAccountSnapshot, HostedAccountError> {
        match self.load_file() {
            Ok(file) => Ok(file.snapshot),
            Err(HostedAccountError::Io(error)) if error.kind() == ErrorKind::NotFound => {
                let snapshot = HostedAccountSnapshot::new(&config.normalized()?);
                self.save(&snapshot)?;
                Ok(snapshot)
            }
            Err(_) => {
                quarantine_file(&self.path, now)?;
                let snapshot = HostedAccountSnapshot::new(&config.normalized()?);
                self.save(&snapshot)?;
                Ok(snapshot)
            }
        }
    }

    pub fn snapshot(&self) -> Result<HostedAccountSnapshot, HostedAccountError> {
        Ok(self.load_file()?.snapshot)
    }

    pub fn save(&self, snapshot: &HostedAccountSnapshot) -> Result<(), HostedAccountError> {
        let file = HostedAccountFile {
            version: HOSTED_ACCOUNT_FILE_VERSION,
            snapshot: snapshot.clone(),
        };
        write_json_atomically(&self.path, &file)?;
        Ok(())
    }

    fn load_file(&self) -> Result<HostedAccountFile, HostedAccountError> {
        let contents = fs::read_to_string(&self.path)?;
        let file: HostedAccountFile = serde_json::from_str(&contents)?;
        file.validate()?;
        Ok(file)
    }
}

/// Hosted account client shared by desktop, hosted web, and CLI surfaces.
#[derive(Debug, Clone)]
pub struct HostedAccountClient {
    store: JsonHostedAccountStore,
}

impl HostedAccountClient {
    pub fn new(store: JsonHostedAccountStore) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &JsonHostedAccountStore {
        &self.store
    }

    pub fn snapshot(
        &self,
        config: &HostedAccountConfig,
        now: DateTime<Utc>,
    ) -> Result<HostedAccountSnapshot, HostedAccountError> {
        self.store.load_or_initialize(config, now)
    }

    /// Updates the durable base URL and device label without touching session
    /// state.
    pub fn configure(
        &self,
        config: &HostedAccountConfig,
        now: DateTime<Utc>,
    ) -> Result<HostedAccountSnapshot, HostedAccountError> {
        let normalized = config.normalized()?;
        let mut snapshot = self.store.load_or_initialize(&normalized, now)?;
        snapshot.base_url = normalized.base_url;
        snapshot.device_name = normalized.device_name;
        snapshot.last_updated_at = Some(now);
        self.store.save(&snapshot)?;
        Ok(snapshot)
    }

    /// Plans, executes, interprets, and persists one hosted account action.
    pub fn execute<T, S>(
        &self,
        action: &HostedAccountAction,
        config: &HostedAccountConfig,
        transport: &T,
        secrets: &mut S,
        now: DateTime<Utc>,
    ) -> Result<HostedAccountResult, HostedAccountError>
    where
        T: HostedAccountTransport + ?Sized,
        S: HostedAccountSecrets + ?Sized,
    {
        let snapshot = self.store.load_or_initialize(config, now)?;
        let plan = plan_hosted_account_request(action, &snapshot, now)?;

        let session_token = match plan.session_token_credential_id {
            Some(credential_id) => {
                let token = secrets.read_secret(credential_id)?;
                if plan.requires_session_token && token.is_none() {
                    return Err(HostedAccountError::SessionRequired);
                }
                token
            }
            None => None,
        };

        let body = if plan.requires_refresh_token {
            let credential_id = plan
                .refresh_token_credential_id
                .ok_or(HostedAccountError::RefreshTokenRequired)?;
            let refresh_token = secrets
                .read_secret(credential_id)?
                .ok_or(HostedAccountError::RefreshTokenRequired)?;
            Some(plan.body_with_refresh_token(&refresh_token))
        } else {
            plan.body.clone()
        };

        let mut result = match transport.execute(&plan, body.as_ref(), session_token.as_deref()) {
            Ok(response) => apply_hosted_account_response(action, &plan, &response, snapshot, now),
            Err(error) => hosted_account_transport_failure(action, &plan, &error, snapshot, now),
        };

        let issued = result.take_secrets();
        if let (Some(credential_id), Some(token)) = (
            issued.session_token_credential_id,
            issued.session_token.as_deref(),
        ) {
            secrets.write_secret(
                credential_id,
                HOSTED_ACCOUNT_SESSION_CREDENTIAL_LABEL,
                token,
            )?;
        }
        if let (Some(credential_id), Some(token)) = (
            issued.refresh_token_credential_id,
            issued.refresh_token.as_deref(),
        ) {
            secrets.write_secret(
                credential_id,
                HOSTED_ACCOUNT_REFRESH_CREDENTIAL_LABEL,
                token,
            )?;
        }
        for credential_id in &result.cleared_credential_ids {
            secrets.clear_secret(*credential_id)?;
        }

        self.store.save(&result.snapshot)?;
        Ok(result)
    }
}

/// Builds the hosted request for one account action.
pub fn plan_hosted_account_request(
    action: &HostedAccountAction,
    snapshot: &HostedAccountSnapshot,
    _now: DateTime<Utc>,
) -> Result<HostedAccountRequestPlan, HostedAccountError> {
    let base_url = normalize_base_url(&snapshot.base_url)?;
    let device_name = normalize_device_name(&snapshot.device_name)?;
    if action.requires_session() && snapshot.session_token_credential_id.is_none() {
        return Err(HostedAccountError::SessionRequired);
    }

    let (method, path, body) = match action {
        HostedAccountAction::Register {
            email,
            display_name,
            invitation_token,
            turnstile_token,
        } => {
            let mut body = json!({
                "email": normalize_email(email)?,
                "device_name": device_name,
            });
            insert_optional_string(
                &mut body,
                "display_name",
                display_name.as_deref(),
                "display_name",
                MAX_DISPLAY_NAME_BYTES,
            )?;
            insert_optional_string(
                &mut body,
                "invitation_token",
                invitation_token.as_deref(),
                "invitation_token",
                MAX_TOKEN_BYTES,
            )?;
            insert_optional_string(
                &mut body,
                "turnstile_token",
                turnstile_token.as_deref(),
                "turnstile_token",
                MAX_TOKEN_BYTES,
            )?;
            ("POST", "/api/v1/auth/register".to_owned(), Some(body))
        }
        HostedAccountAction::VerifyEmail { token } => (
            "POST",
            "/api/v1/auth/verify-email".to_owned(),
            Some(json!({"token": require_token(token, "token")?})),
        ),
        HostedAccountAction::RecoveryStart { email } => (
            "POST",
            "/api/v1/auth/recovery/start".to_owned(),
            Some(json!({"email": normalize_email(email)?})),
        ),
        HostedAccountAction::RecoveryComplete { token } => (
            "POST",
            "/api/v1/auth/recovery/complete".to_owned(),
            Some(json!({
                "token": require_token(token, "token")?,
                "device_name": device_name,
            })),
        ),
        HostedAccountAction::Login {
            email,
            display_name,
        } => {
            let mut body = json!({
                "email": normalize_email(email)?,
                "device_name": device_name,
            });
            insert_optional_string(
                &mut body,
                "display_name",
                display_name.as_deref(),
                "display_name",
                MAX_DISPLAY_NAME_BYTES,
            )?;
            ("POST", "/api/v1/auth/login".to_owned(), Some(body))
        }
        HostedAccountAction::Session => ("GET", "/api/v1/auth/session".to_owned(), None),
        HostedAccountAction::SessionRotate => (
            "POST",
            "/api/v1/auth/session/rotate".to_owned(),
            Some(json!({})),
        ),
        HostedAccountAction::Logout => ("POST", "/api/v1/auth/logout".to_owned(), Some(json!({}))),
        HostedAccountAction::LogoutAll => (
            "POST",
            "/api/v1/auth/logout-all".to_owned(),
            Some(json!({})),
        ),
        HostedAccountAction::AccountDelete => (
            "POST",
            "/api/v1/auth/account/delete".to_owned(),
            Some(json!({"confirm": true})),
        ),
        HostedAccountAction::DeviceList => ("GET", "/api/v1/devices".to_owned(), None),
        HostedAccountAction::DeviceRegister { device_name } => (
            "POST",
            "/api/v1/devices".to_owned(),
            Some(json!({"device_name": normalize_device_name(device_name)?})),
        ),
        HostedAccountAction::DeviceRevoke { device_id } => (
            "POST",
            format!("/api/v1/devices/{device_id}/revoke"),
            Some(json!({})),
        ),
        HostedAccountAction::DeviceRevokeAll => (
            "POST",
            "/api/v1/devices/revoke-all".to_owned(),
            Some(json!({})),
        ),
    };

    let requires_refresh_token = matches!(action, HostedAccountAction::SessionRotate);
    if requires_refresh_token && snapshot.refresh_token_credential_id.is_none() {
        return Err(HostedAccountError::RefreshTokenRequired);
    }

    Ok(HostedAccountRequestPlan {
        action: action.name().to_owned(),
        method: method.to_owned(),
        url: format!("{base_url}{path}"),
        path,
        body,
        requires_session_token: action.requires_session(),
        session_token_credential_id: if action.requires_session() {
            snapshot.session_token_credential_id
        } else {
            None
        },
        requires_refresh_token,
        refresh_token_credential_id: if requires_refresh_token {
            snapshot.refresh_token_credential_id
        } else {
            None
        },
        refresh_token_body_field: requires_refresh_token.then(|| "refresh_token".to_owned()),
        request_id: Uuid::new_v4(),
        timeout_seconds: DEFAULT_HOSTED_ACCOUNT_TIMEOUT_SECONDS,
        max_response_bytes: MAX_HOSTED_ACCOUNT_RESPONSE_BYTES,
    })
}

/// Interprets a hosted response and returns the next durable snapshot.
pub fn apply_hosted_account_response(
    action: &HostedAccountAction,
    plan: &HostedAccountRequestPlan,
    response: &HostedAccountHttpResponse,
    snapshot: HostedAccountSnapshot,
    now: DateTime<Utc>,
) -> HostedAccountResult {
    let body = response.body.clone().unwrap_or(JsonValue::Null);
    if (200..300).contains(&response.status) {
        return accepted_result(action, plan, response.status, &body, snapshot, now);
    }

    let error_code = body
        .get("code")
        .and_then(JsonValue::as_str)
        .map(str::to_owned);
    let request_id = body
        .get("request_id")
        .and_then(JsonValue::as_str)
        .map(str::to_owned);
    let message = body
        .get("error")
        .and_then(JsonValue::as_str)
        .map(str::to_owned)
        .unwrap_or_else(|| {
            format!(
                "hosted account request failed with status {}",
                response.status
            )
        });
    let outcome = classify_hosted_account_error(error_code.as_deref(), response.status);

    let mut snapshot = snapshot;
    let mut cleared = Vec::new();
    match outcome {
        HostedAccountOutcome::AuthenticationRequired => {
            cleared.extend(snapshot.session_token_credential_id);
            cleared.extend(snapshot.refresh_token_credential_id);
            snapshot.clear_session();
            snapshot.connection_state = if error_code.as_deref() == Some("device_revoked") {
                HostedAccountConnectionState::DeviceRevoked
            } else {
                HostedAccountConnectionState::SessionExpired
            };
        }
        HostedAccountOutcome::EmailVerificationRequired => {
            snapshot.email_verified = false;
            snapshot.connection_state = HostedAccountConnectionState::PendingEmailVerification;
        }
        _ => {}
    }

    finish_result(
        action,
        response.status,
        outcome,
        message,
        error_code,
        request_id.or_else(|| Some(plan.request_id.to_string())),
        snapshot,
        cleared,
        None,
        None,
        now,
    )
}

/// Classifies a transport-level failure without contacting the hosted server.
pub fn hosted_account_transport_failure(
    action: &HostedAccountAction,
    plan: &HostedAccountRequestPlan,
    message: &str,
    snapshot: HostedAccountSnapshot,
    now: DateTime<Utc>,
) -> HostedAccountResult {
    finish_result(
        action,
        0,
        HostedAccountOutcome::TransientFailure,
        format!("hosted account transport failure: {message}"),
        Some("transport_failure".to_owned()),
        Some(plan.request_id.to_string()),
        snapshot,
        Vec::new(),
        None,
        None,
        now,
    )
}

/// Maps a stable hosted error code, then the HTTP status, to an outcome.
pub fn classify_hosted_account_error(code: Option<&str>, status: u16) -> HostedAccountOutcome {
    match code {
        Some("missing_token")
        | Some("invalid_token")
        | Some("session_inactive")
        | Some("session_expired")
        | Some("device_revoked") => HostedAccountOutcome::AuthenticationRequired,
        Some("email_unverified") => HostedAccountOutcome::EmailVerificationRequired,
        Some("registration_closed") => HostedAccountOutcome::RegistrationClosed,
        Some("token_expired") => HostedAccountOutcome::TokenExpired,
        Some("token_replayed") => HostedAccountOutcome::TokenReplayed,
        Some("turnstile_failed") => HostedAccountOutcome::HumanVerificationFailed,
        Some("rate_limited") => HostedAccountOutcome::RateLimited,
        Some("bad_request")
        | Some("validation_failed")
        | Some("invalid_json")
        | Some("invalid_uuid")
        | Some("missing_field")
        | Some("unsupported_media_type")
        | Some("payload_too_large")
        | Some("proposal_rejected") => HostedAccountOutcome::ValidationFailed,
        Some("store_unavailable") | Some("internal_error") => {
            HostedAccountOutcome::TransientFailure
        }
        Some("forbidden") | Some("not_found") => HostedAccountOutcome::PermanentFailure,
        _ => match status {
            401 => HostedAccountOutcome::AuthenticationRequired,
            403 | 404 => HostedAccountOutcome::PermanentFailure,
            429 => HostedAccountOutcome::RateLimited,
            status if (500..600).contains(&status) => HostedAccountOutcome::TransientFailure,
            status if (400..500).contains(&status) => HostedAccountOutcome::ValidationFailed,
            _ => HostedAccountOutcome::PermanentFailure,
        },
    }
}

fn accepted_result(
    action: &HostedAccountAction,
    plan: &HostedAccountRequestPlan,
    status: u16,
    body: &JsonValue,
    snapshot: HostedAccountSnapshot,
    now: DateTime<Utc>,
) -> HostedAccountResult {
    let mut snapshot = snapshot;
    let mut cleared = Vec::new();
    let mut session_token = None;
    let mut refresh_token = None;
    let message;

    match action {
        HostedAccountAction::Register { email, .. } => {
            let normalized = body
                .get("account")
                .and_then(|account| account.get("email"))
                .and_then(JsonValue::as_str)
                .map(str::to_owned)
                .or_else(|| normalize_email(email).ok());
            snapshot.pending_email_verification_for = normalized.clone();
            snapshot.email = normalized;
            snapshot.email_verified = false;
            snapshot.connection_state = HostedAccountConnectionState::PendingEmailVerification;
            message = "Registration accepted. Check the account email for the verification token."
                .to_owned();
        }
        HostedAccountAction::VerifyEmail { .. } => {
            if let Some(account) = body.get("account") {
                apply_account(&mut snapshot, account);
            }
            snapshot.email_verified = true;
            snapshot.pending_email_verification_for = None;
            if !snapshot.connection_state.is_signed_in() {
                snapshot.connection_state = HostedAccountConnectionState::SignedOut;
            }
            message = "Email verified. Sign in to start a hosted session.".to_owned();
        }
        HostedAccountAction::RecoveryStart { email } => {
            snapshot.pending_recovery_for = normalize_email(email).ok();
            message =
                "If the account exists and is verified, a recovery token was sent to its email."
                    .to_owned();
        }
        HostedAccountAction::RecoveryComplete { .. }
        | HostedAccountAction::Login { .. }
        | HostedAccountAction::SessionRotate => {
            let (issued_session, issued_refresh) = apply_session(&mut snapshot, body, now);
            session_token = issued_session;
            refresh_token = issued_refresh;
            snapshot.pending_recovery_for = None;
            message = match action {
                HostedAccountAction::SessionRotate => "Hosted session rotated.".to_owned(),
                _ => "Signed in to the hosted account.".to_owned(),
            };
        }
        HostedAccountAction::Session => {
            let (issued_session, issued_refresh) = apply_session(&mut snapshot, body, now);
            session_token = issued_session;
            refresh_token = issued_refresh;
            message = "Hosted session refreshed.".to_owned();
        }
        HostedAccountAction::Logout | HostedAccountAction::LogoutAll => {
            cleared.extend(snapshot.session_token_credential_id);
            cleared.extend(snapshot.refresh_token_credential_id);
            snapshot.clear_session();
            snapshot.connection_state = HostedAccountConnectionState::SignedOut;
            message = match action {
                HostedAccountAction::LogoutAll => {
                    let count = body
                        .get("revoked_sessions")
                        .and_then(JsonValue::as_u64)
                        .unwrap_or(0);
                    format!("Signed out of {count} hosted sessions.")
                }
                _ => "Signed out of the hosted account.".to_owned(),
            };
        }
        HostedAccountAction::AccountDelete => {
            cleared.extend(snapshot.session_token_credential_id);
            cleared.extend(snapshot.refresh_token_credential_id);
            snapshot.clear_session();
            snapshot.clear_identity();
            snapshot.pending_email_verification_for = None;
            snapshot.pending_recovery_for = None;
            snapshot.connection_state = HostedAccountConnectionState::SignedOut;
            message = "Hosted account deletion accepted.".to_owned();
        }
        HostedAccountAction::DeviceList | HostedAccountAction::DeviceRegister { .. } => {
            apply_devices(&mut snapshot, body);
            message = format!("{} hosted devices known.", snapshot.devices.len());
        }
        HostedAccountAction::DeviceRevoke { device_id } => {
            apply_devices(&mut snapshot, body);
            mark_device_revoked(&mut snapshot, *device_id, now);
            if snapshot.device_id == Some(*device_id) {
                cleared.extend(snapshot.session_token_credential_id);
                cleared.extend(snapshot.refresh_token_credential_id);
                snapshot.clear_session();
                snapshot.connection_state = HostedAccountConnectionState::DeviceRevoked;
                message = "This device was revoked and the local session was cleared.".to_owned();
            } else {
                message = format!("Hosted device {device_id} revoked.");
            }
        }
        HostedAccountAction::DeviceRevokeAll => {
            let count = body
                .get("revoked_devices")
                .and_then(JsonValue::as_u64)
                .unwrap_or(0);
            for device in &mut snapshot.devices {
                device.revoked = true;
                device.revoked_at = Some(now);
                device.current = false;
            }
            cleared.extend(snapshot.session_token_credential_id);
            cleared.extend(snapshot.refresh_token_credential_id);
            snapshot.clear_session();
            snapshot.connection_state = HostedAccountConnectionState::SignedOut;
            message = format!("Revoked {count} hosted devices and signed out.");
        }
    }

    finish_result(
        action,
        status,
        HostedAccountOutcome::Accepted,
        message,
        None,
        body.get("request_id")
            .and_then(JsonValue::as_str)
            .map(str::to_owned)
            .or_else(|| Some(plan.request_id.to_string())),
        snapshot,
        cleared,
        session_token,
        refresh_token,
        now,
    )
}

#[allow(clippy::too_many_arguments)]
fn finish_result(
    action: &HostedAccountAction,
    status: u16,
    outcome: HostedAccountOutcome,
    message: String,
    error_code: Option<String>,
    request_id: Option<String>,
    snapshot: HostedAccountSnapshot,
    cleared_credential_ids: Vec<Uuid>,
    session_token: Option<String>,
    refresh_token: Option<String>,
    now: DateTime<Utc>,
) -> HostedAccountResult {
    let mut snapshot = snapshot;
    snapshot.schema_version = HOSTED_ACCOUNT_FILE_VERSION;
    snapshot.last_action = Some(action.name().to_owned());
    snapshot.last_outcome = Some(outcome);
    snapshot.last_error_code = error_code.clone();
    snapshot.last_message = Some(message.clone());
    snapshot.last_request_id = request_id.clone();
    snapshot.last_updated_at = Some(now);

    HostedAccountResult {
        action: action.name().to_owned(),
        outcome,
        status,
        message,
        error_code,
        request_id,
        retryable: outcome.is_retryable(),
        user_action_required: outcome.requires_user_action(),
        snapshot,
        cleared_credential_ids,
        session_token,
        refresh_token,
    }
}

fn apply_account(snapshot: &mut HostedAccountSnapshot, account: &JsonValue) {
    if let Some(account_id) = account.get("account_id").and_then(parse_uuid_value) {
        snapshot.account_id = Some(account_id);
    }
    if let Some(user_id) = account.get("user_id").and_then(parse_uuid_value) {
        snapshot.user_id = Some(user_id);
    }
    if let Some(email) = account.get("email").and_then(JsonValue::as_str) {
        snapshot.email = Some(email.to_owned());
    }
    if let Some(display_name) = account.get("display_name").and_then(JsonValue::as_str) {
        snapshot.display_name = Some(display_name.to_owned());
    }
    snapshot.email_verified = account
        .get("email_verified_at")
        .map(|value| !value.is_null())
        .unwrap_or(snapshot.email_verified);
}

fn apply_session(
    snapshot: &mut HostedAccountSnapshot,
    body: &JsonValue,
    now: DateTime<Utc>,
) -> (Option<String>, Option<String>) {
    if let Some(account) = body.get("account") {
        apply_account(snapshot, account);
    }

    let mut issued_session = None;
    let mut issued_refresh = None;

    if let Some(session) = body.get("session") {
        if let Some(session_id) = session.get("session_id").and_then(parse_uuid_value) {
            snapshot.session_id = Some(session_id);
        }
        snapshot.session_issued_at = session.get("issued_at").and_then(parse_timestamp_value);
        snapshot.session_expires_at = session.get("expires_at").and_then(parse_timestamp_value);
        snapshot.refresh_expires_at = session
            .get("refresh_expires_at")
            .and_then(parse_timestamp_value);
        if let Some(device_id) = session.get("device_id").and_then(parse_uuid_value) {
            snapshot.device_id = Some(device_id);
        }
        if let Some(token) = non_empty_string(session.get("token")) {
            snapshot.session_token_credential_id = Some(
                snapshot
                    .session_token_credential_id
                    .unwrap_or_else(Uuid::new_v4),
            );
            issued_session = Some(token);
        }
    }

    if let Some(token) = non_empty_string(body.get("refresh_token")) {
        snapshot.refresh_token_credential_id = Some(
            snapshot
                .refresh_token_credential_id
                .unwrap_or_else(Uuid::new_v4),
        );
        issued_refresh = Some(token);
    }

    if let Some(device) = body.get("device") {
        if let Some(device_id) = device.get("device_id").and_then(parse_uuid_value) {
            snapshot.device_id = Some(device_id);
        }
        if let Some(device) = hosted_device_from_value(device) {
            upsert_device(snapshot, device);
        }
    }

    apply_logbooks(snapshot, body);

    snapshot.connection_state = HostedAccountConnectionState::SignedIn;
    snapshot.email_verified = true;
    snapshot.pending_email_verification_for = None;
    if let Some(device_id) = snapshot.device_id {
        for device in &mut snapshot.devices {
            device.current = device.device_id == device_id;
        }
    }
    let _ = now;
    (issued_session, issued_refresh)
}

fn apply_logbooks(snapshot: &mut HostedAccountSnapshot, body: &JsonValue) {
    let mut logbooks: Vec<HostedAccountLogbook> = Vec::new();
    if let Some(entries) = body.get("logbooks").and_then(JsonValue::as_array) {
        for entry in entries.iter().take(MAX_CACHED_LOGBOOKS) {
            let Some(logbook_id) = entry.get("logbook_id").and_then(parse_uuid_value) else {
                continue;
            };
            logbooks.push(HostedAccountLogbook {
                logbook_id,
                name: entry
                    .get("name")
                    .and_then(JsonValue::as_str)
                    .unwrap_or("Logbook")
                    .to_owned(),
                role: None,
            });
        }
    }
    if let Some(entries) = body.get("memberships").and_then(JsonValue::as_array) {
        for entry in entries.iter().take(MAX_CACHED_LOGBOOKS) {
            let Some(logbook_id) = entry.get("logbook_id").and_then(parse_uuid_value) else {
                continue;
            };
            let role = entry
                .get("role")
                .and_then(JsonValue::as_str)
                .map(str::to_owned);
            match logbooks
                .iter_mut()
                .find(|logbook| logbook.logbook_id == logbook_id)
            {
                Some(existing) => existing.role = role,
                None => {
                    // Membership records carry no logbook name, so keep the
                    // name this client already cached from a logbook listing.
                    let name = snapshot
                        .logbooks
                        .iter()
                        .find(|logbook| logbook.logbook_id == logbook_id)
                        .map(|logbook| logbook.name.clone())
                        .unwrap_or_else(|| "Logbook".to_owned());
                    logbooks.push(HostedAccountLogbook {
                        logbook_id,
                        name,
                        role,
                    });
                }
            }
        }
    }
    if !logbooks.is_empty() {
        logbooks.sort_by_key(|logbook| logbook.logbook_id);
        snapshot.logbooks = logbooks;
    }
}

fn apply_devices(snapshot: &mut HostedAccountSnapshot, body: &JsonValue) {
    if let Some(entries) = body.get("devices").and_then(JsonValue::as_array) {
        let mut devices = entries
            .iter()
            .take(MAX_CACHED_DEVICES)
            .filter_map(hosted_device_from_value)
            .collect::<Vec<_>>();
        devices.sort_by_key(|device| device.device_id);
        for device in &mut devices {
            device.current = snapshot.device_id == Some(device.device_id);
        }
        snapshot.devices = devices;
        return;
    }
    if let Some(device) = body.get("device").and_then(hosted_device_from_value) {
        upsert_device(snapshot, device);
    }
}

fn upsert_device(snapshot: &mut HostedAccountSnapshot, device: HostedAccountDevice) {
    let mut device = device;
    device.current = snapshot.device_id == Some(device.device_id);
    match snapshot
        .devices
        .iter_mut()
        .find(|existing| existing.device_id == device.device_id)
    {
        Some(existing) => *existing = device,
        None => {
            if snapshot.devices.len() < MAX_CACHED_DEVICES {
                snapshot.devices.push(device);
                snapshot.devices.sort_by_key(|device| device.device_id);
            }
        }
    }
}

fn mark_device_revoked(snapshot: &mut HostedAccountSnapshot, device_id: Uuid, now: DateTime<Utc>) {
    if let Some(device) = snapshot
        .devices
        .iter_mut()
        .find(|device| device.device_id == device_id)
    {
        device.revoked = true;
        device.revoked_at = Some(now);
    }
}

fn hosted_device_from_value(value: &JsonValue) -> Option<HostedAccountDevice> {
    let device_id = value.get("device_id").and_then(parse_uuid_value)?;
    Some(HostedAccountDevice {
        device_id,
        device_name: value
            .get("device_name")
            .and_then(JsonValue::as_str)
            .unwrap_or("Device")
            .to_owned(),
        trusted: value
            .get("trusted")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false),
        revoked: value
            .get("revoked")
            .and_then(JsonValue::as_bool)
            .unwrap_or(false),
        registered_at: value.get("registered_at").and_then(parse_timestamp_value),
        revoked_at: value.get("revoked_at").and_then(parse_timestamp_value),
        current: false,
    })
}

fn parse_uuid_value(value: &JsonValue) -> Option<Uuid> {
    value.as_str().and_then(|value| Uuid::parse_str(value).ok())
}

fn parse_timestamp_value(value: &JsonValue) -> Option<DateTime<Utc>> {
    value
        .as_str()
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&Utc))
}

fn non_empty_string(value: Option<&JsonValue>) -> Option<String> {
    value
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn insert_optional_string(
    body: &mut JsonValue,
    key: &str,
    value: Option<&str>,
    field: &'static str,
    max: usize,
) -> Result<(), HostedAccountError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    if value.len() > max {
        return Err(HostedAccountError::FieldTooLong { field, max });
    }
    if let Some(object) = body.as_object_mut() {
        object.insert(key.to_owned(), JsonValue::String(value.to_owned()));
    }
    Ok(())
}

fn require_token(value: &str, field: &'static str) -> Result<String, HostedAccountError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(HostedAccountError::MissingField(field));
    }
    if trimmed.len() > MAX_TOKEN_BYTES {
        return Err(HostedAccountError::FieldTooLong {
            field,
            max: MAX_TOKEN_BYTES,
        });
    }
    Ok(trimmed.to_owned())
}

/// Normalizes and bounds a hosted API base URL.
pub fn normalize_base_url(base_url: &str) -> Result<String, HostedAccountError> {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(HostedAccountError::InvalidBaseUrl(
            "base URL is required".to_owned(),
        ));
    }
    if trimmed.len() > MAX_BASE_URL_BYTES {
        return Err(HostedAccountError::InvalidBaseUrl(
            "base URL is too long".to_owned(),
        ));
    }
    if !trimmed.starts_with("http://") && !trimmed.starts_with("https://") {
        return Err(HostedAccountError::InvalidBaseUrl(
            "base URL must start with http:// or https://".to_owned(),
        ));
    }
    if trimmed
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(HostedAccountError::InvalidBaseUrl(
            "base URL must not contain whitespace or control characters".to_owned(),
        ));
    }
    let authority = &trimmed[trimmed.find("//").map(|index| index + 2).unwrap_or(0)..];
    if authority.is_empty() || authority.starts_with('/') {
        return Err(HostedAccountError::InvalidBaseUrl(
            "base URL must include a host".to_owned(),
        ));
    }
    Ok(trimmed.to_owned())
}

/// Normalizes and bounds an account email address.
pub fn normalize_email(email: &str) -> Result<String, HostedAccountError> {
    let trimmed = email.trim().to_ascii_lowercase();
    if trimmed.is_empty() || trimmed.len() > MAX_EMAIL_BYTES {
        return Err(HostedAccountError::InvalidEmail);
    }
    if trimmed
        .chars()
        .any(|character| character.is_whitespace() || character.is_control())
    {
        return Err(HostedAccountError::InvalidEmail);
    }
    let mut parts = trimmed.split('@');
    let local = parts.next().unwrap_or_default();
    let domain = parts.next().unwrap_or_default();
    if parts.next().is_some() || local.is_empty() || domain.is_empty() || !domain.contains('.') {
        return Err(HostedAccountError::InvalidEmail);
    }
    Ok(trimmed)
}

/// Normalizes and bounds the device label reported to the hosted server.
pub fn normalize_device_name(device_name: &str) -> Result<String, HostedAccountError> {
    let trimmed = device_name.trim();
    if trimmed.is_empty() {
        return Ok(DEFAULT_HOSTED_ACCOUNT_DEVICE_NAME.to_owned());
    }
    if trimmed.len() > MAX_DEVICE_NAME_BYTES {
        return Err(HostedAccountError::FieldTooLong {
            field: "device_name",
            max: MAX_DEVICE_NAME_BYTES,
        });
    }
    if trimmed.chars().any(char::is_control) {
        return Err(HostedAccountError::FieldTooLong {
            field: "device_name",
            max: MAX_DEVICE_NAME_BYTES,
        });
    }
    Ok(trimmed.to_owned())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    struct MemorySecrets {
        secrets: HashMap<Uuid, String>,
    }

    impl MemorySecrets {
        fn new() -> Self {
            Self {
                secrets: HashMap::new(),
            }
        }
    }

    impl HostedAccountSecrets for MemorySecrets {
        fn read_secret(
            &mut self,
            credential_id: Uuid,
        ) -> Result<Option<String>, HostedAccountError> {
            Ok(self.secrets.get(&credential_id).cloned())
        }

        fn write_secret(
            &mut self,
            credential_id: Uuid,
            _label: &str,
            secret: &str,
        ) -> Result<(), HostedAccountError> {
            self.secrets.insert(credential_id, secret.to_owned());
            Ok(())
        }

        fn clear_secret(&mut self, credential_id: Uuid) -> Result<(), HostedAccountError> {
            self.secrets.remove(&credential_id);
            Ok(())
        }
    }

    struct ObservedRequest {
        method: String,
        path: String,
        bearer: Option<String>,
        body: Option<JsonValue>,
    }

    struct ScriptedTransport {
        responses: std::cell::RefCell<Vec<Result<HostedAccountHttpResponse, String>>>,
        seen: std::cell::RefCell<Vec<ObservedRequest>>,
    }

    impl ScriptedTransport {
        fn new(responses: Vec<Result<HostedAccountHttpResponse, String>>) -> Self {
            Self {
                responses: std::cell::RefCell::new(responses),
                seen: std::cell::RefCell::new(Vec::new()),
            }
        }
    }

    impl HostedAccountTransport for ScriptedTransport {
        fn execute(
            &self,
            plan: &HostedAccountRequestPlan,
            body: Option<&JsonValue>,
            session_token: Option<&str>,
        ) -> Result<HostedAccountHttpResponse, String> {
            self.seen.borrow_mut().push(ObservedRequest {
                method: plan.method.clone(),
                path: plan.path.clone(),
                bearer: session_token.map(str::to_owned),
                body: body.cloned(),
            });
            if self.responses.borrow().is_empty() {
                return Err("no scripted response".to_owned());
            }
            self.responses.borrow_mut().remove(0)
        }
    }

    fn client() -> (HostedAccountClient, PathBuf) {
        let dir = std::env::temp_dir().join(format!("ham-sync-account-{}", Uuid::new_v4()));
        (
            HostedAccountClient::new(JsonHostedAccountStore::new(dir.join("hosted-account.json"))),
            dir,
        )
    }

    fn config() -> HostedAccountConfig {
        HostedAccountConfig {
            base_url: "https://logger.example/".to_owned(),
            device_name: "Shack Desktop".to_owned(),
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-31T12:00:00Z")
            .expect("fixed timestamp parses")
            .with_timezone(&Utc)
    }

    fn login_body(session_token: &str, refresh_token: &str, device_id: Uuid) -> JsonValue {
        json!({
            "account": {
                "account_id": "11111111-1111-4111-8111-111111111111",
                "user_id": "22222222-2222-4222-8222-222222222222",
                "email": "operator@example.com",
                "display_name": "Operator",
                "created_at": "2026-08-01T00:00:00Z",
                "email_verified_at": "2026-08-02T00:00:00Z"
            },
            "session": {
                "session_id": "33333333-3333-4333-8333-333333333333",
                "account_id": "11111111-1111-4111-8111-111111111111",
                "user_id": "22222222-2222-4222-8222-222222222222",
                "device_id": device_id,
                "token": session_token,
                "issued_at": "2026-08-31T12:00:00Z",
                "expires_at": "2026-09-30T12:00:00Z",
                "refresh_expires_at": "2026-11-29T12:00:00Z",
                "active": true
            },
            "device": {
                "device_id": device_id,
                "account_id": "11111111-1111-4111-8111-111111111111",
                "user_id": "22222222-2222-4222-8222-222222222222",
                "device_name": "Shack Desktop",
                "fingerprint": "dev-fingerprint",
                "trusted": true,
                "revoked": false,
                "registered_at": "2026-08-31T12:00:00Z"
            },
            "logbooks": [{
                "logbook_id": "44444444-4444-4444-8444-444444444444",
                "account_id": "11111111-1111-4111-8111-111111111111",
                "name": "Home Station",
                "created_at": "2026-08-01T00:00:00Z",
                "updated_at": "2026-08-01T00:00:00Z"
            }],
            "refresh_token": refresh_token,
            "session_cookie": "ke8ygw_session=redacted; HttpOnly"
        })
    }

    fn sign_in(
        client: &HostedAccountClient,
        secrets: &mut MemorySecrets,
        device_id: Uuid,
    ) -> HostedAccountResult {
        let transport = ScriptedTransport::new(vec![Ok(HostedAccountHttpResponse::new(
            200,
            Some(login_body("session-secret", "refresh-secret", device_id)),
        ))]);
        client
            .execute(
                &HostedAccountAction::Login {
                    email: "Operator@Example.com".to_owned(),
                    display_name: None,
                },
                &config(),
                &transport,
                secrets,
                now(),
            )
            .expect("login executes")
    }

    #[test]
    fn base_url_normalization_rejects_unsupported_values() {
        assert_eq!(
            normalize_base_url("https://logger.example/").expect("https base URL is accepted"),
            "https://logger.example"
        );
        assert!(matches!(
            normalize_base_url("ftp://logger.example"),
            Err(HostedAccountError::InvalidBaseUrl(_))
        ));
        assert!(matches!(
            normalize_base_url("https://logger.example/path\nHost: evil"),
            Err(HostedAccountError::InvalidBaseUrl(_))
        ));
        assert!(matches!(
            normalize_base_url("   "),
            Err(HostedAccountError::InvalidBaseUrl(_))
        ));
    }

    #[test]
    fn email_normalization_lowercases_and_rejects_malformed_addresses() {
        assert_eq!(
            normalize_email("  Operator@Example.COM ").expect("valid email"),
            "operator@example.com"
        );
        assert!(matches!(
            normalize_email("operator@example"),
            Err(HostedAccountError::InvalidEmail)
        ));
        assert!(matches!(
            normalize_email("operator@@example.com"),
            Err(HostedAccountError::InvalidEmail)
        ));
    }

    #[test]
    fn session_routes_require_stored_session_credentials() {
        let snapshot = HostedAccountSnapshot::new(&config().normalized().expect("config"));
        for action in [
            HostedAccountAction::Session,
            HostedAccountAction::Logout,
            HostedAccountAction::DeviceList,
            HostedAccountAction::DeviceRevokeAll,
        ] {
            assert!(matches!(
                plan_hosted_account_request(&action, &snapshot, now()),
                Err(HostedAccountError::SessionRequired)
            ));
        }
    }

    #[test]
    fn login_plan_targets_the_frozen_hosted_route() {
        let snapshot = HostedAccountSnapshot::new(&config().normalized().expect("config"));
        let plan = plan_hosted_account_request(
            &HostedAccountAction::Login {
                email: "Operator@Example.com".to_owned(),
                display_name: Some("  Operator  ".to_owned()),
            },
            &snapshot,
            now(),
        )
        .expect("login plans");
        assert_eq!(plan.method, "POST");
        assert_eq!(plan.path, "/api/v1/auth/login");
        assert_eq!(plan.url, "https://logger.example/api/v1/auth/login");
        assert!(!plan.requires_session_token);
        let body = plan.body.expect("login body");
        assert_eq!(body["email"], json!("operator@example.com"));
        assert_eq!(body["display_name"], json!("Operator"));
        assert_eq!(body["device_name"], json!("Shack Desktop"));
    }

    #[test]
    fn accepted_login_stores_only_credential_ids_and_never_persists_tokens() {
        let (client, dir) = client();
        let mut secrets = MemorySecrets::new();
        let device_id = Uuid::new_v4();
        let result = sign_in(&client, &mut secrets, device_id);

        assert!(result.outcome.is_accepted());
        assert!(!result.user_action_required);
        assert_eq!(
            result.snapshot.connection_state,
            HostedAccountConnectionState::SignedIn
        );
        assert_eq!(
            result.snapshot.email.as_deref(),
            Some("operator@example.com")
        );
        assert_eq!(result.snapshot.device_id, Some(device_id));
        assert_eq!(result.snapshot.logbooks.len(), 1);

        let session_credential = result
            .snapshot
            .session_token_credential_id
            .expect("session credential id");
        let refresh_credential = result
            .snapshot
            .refresh_token_credential_id
            .expect("refresh credential id");
        assert_eq!(
            secrets.read_secret(session_credential).expect("read"),
            Some("session-secret".to_owned())
        );
        assert_eq!(
            secrets.read_secret(refresh_credential).expect("read"),
            Some("refresh-secret".to_owned())
        );

        let stored = std::fs::read_to_string(client.store().path()).expect("stored record");
        assert!(!stored.contains("session-secret"));
        assert!(!stored.contains("refresh-secret"));
        let serialized_result = serde_json::to_string(&result).expect("result serializes");
        assert!(!serialized_result.contains("session-secret"));
        assert!(!serialized_result.contains("refresh-secret"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn session_rotate_sends_the_stored_refresh_token_without_persisting_it() {
        let (client, dir) = client();
        let mut secrets = MemorySecrets::new();
        let device_id = Uuid::new_v4();
        sign_in(&client, &mut secrets, device_id);

        let transport = ScriptedTransport::new(vec![Ok(HostedAccountHttpResponse::new(
            200,
            Some(login_body("rotated-session", "rotated-refresh", device_id)),
        ))]);
        let result = client
            .execute(
                &HostedAccountAction::SessionRotate,
                &config(),
                &transport,
                &mut secrets,
                now(),
            )
            .expect("rotate executes");

        assert!(result.outcome.is_accepted());
        let seen = transport.seen.borrow();
        let observed = seen.first().expect("one request");
        assert_eq!(observed.method, "POST");
        assert_eq!(observed.path, "/api/v1/auth/session/rotate");
        assert_eq!(observed.bearer.as_deref(), Some("session-secret"));
        assert_eq!(
            observed.body.as_ref().expect("rotate body")["refresh_token"],
            json!("refresh-secret")
        );

        let credential = result
            .snapshot
            .session_token_credential_id
            .expect("session credential id");
        assert_eq!(
            secrets.read_secret(credential).expect("read"),
            Some("rotated-session".to_owned())
        );
        let stored = std::fs::read_to_string(client.store().path()).expect("stored record");
        assert!(!stored.contains("rotated-refresh"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn expired_session_clears_stored_secrets_and_requires_user_action() {
        let (client, dir) = client();
        let mut secrets = MemorySecrets::new();
        let device_id = Uuid::new_v4();
        let signed_in = sign_in(&client, &mut secrets, device_id);
        let session_credential = signed_in
            .snapshot
            .session_token_credential_id
            .expect("session credential id");

        let transport = ScriptedTransport::new(vec![Ok(HostedAccountHttpResponse::new(
            401,
            Some(json!({
                "error": "session expired",
                "code": "session_expired",
                "request_id": "req-1",
                "retryable": false
            })),
        ))]);
        let result = client
            .execute(
                &HostedAccountAction::Session,
                &config(),
                &transport,
                &mut secrets,
                now(),
            )
            .expect("session executes");

        assert_eq!(result.outcome, HostedAccountOutcome::AuthenticationRequired);
        assert!(result.user_action_required);
        assert!(!result.retryable);
        assert_eq!(result.request_id.as_deref(), Some("req-1"));
        assert_eq!(
            result.snapshot.connection_state,
            HostedAccountConnectionState::SessionExpired
        );
        assert!(result.snapshot.session_token_credential_id.is_none());
        assert!(secrets
            .read_secret(session_credential)
            .expect("read")
            .is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn revoked_device_marks_the_local_record_and_clears_the_session() {
        let (client, dir) = client();
        let mut secrets = MemorySecrets::new();
        let device_id = Uuid::new_v4();
        sign_in(&client, &mut secrets, device_id);

        let transport = ScriptedTransport::new(vec![Ok(HostedAccountHttpResponse::new(
            200,
            Some(json!({"ok": true})),
        ))]);
        let result = client
            .execute(
                &HostedAccountAction::DeviceRevoke { device_id },
                &config(),
                &transport,
                &mut secrets,
                now(),
            )
            .expect("device revoke executes");

        assert!(result.outcome.is_accepted());
        assert_eq!(
            result.snapshot.connection_state,
            HostedAccountConnectionState::DeviceRevoked
        );
        assert!(result.snapshot.session_token_credential_id.is_none());
        assert!(result
            .snapshot
            .devices
            .iter()
            .any(|device| device.device_id == device_id && device.revoked));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn transport_failures_stay_retryable_without_clearing_the_session() {
        let (client, dir) = client();
        let mut secrets = MemorySecrets::new();
        let device_id = Uuid::new_v4();
        sign_in(&client, &mut secrets, device_id);

        let transport = ScriptedTransport::new(vec![Err("connection refused".to_owned())]);
        let result = client
            .execute(
                &HostedAccountAction::Session,
                &config(),
                &transport,
                &mut secrets,
                now(),
            )
            .expect("session executes");

        assert_eq!(result.outcome, HostedAccountOutcome::TransientFailure);
        assert!(result.retryable);
        assert!(!result.user_action_required);
        assert_eq!(
            result.snapshot.connection_state,
            HostedAccountConnectionState::SignedIn
        );
        assert!(result.snapshot.session_token_credential_id.is_some());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn error_codes_map_to_stable_outcomes_before_status_codes() {
        assert_eq!(
            classify_hosted_account_error(Some("registration_closed"), 200),
            HostedAccountOutcome::RegistrationClosed
        );
        assert_eq!(
            classify_hosted_account_error(Some("email_unverified"), 400),
            HostedAccountOutcome::EmailVerificationRequired
        );
        assert_eq!(
            classify_hosted_account_error(Some("turnstile_failed"), 400),
            HostedAccountOutcome::HumanVerificationFailed
        );
        assert_eq!(
            classify_hosted_account_error(Some("rate_limited"), 429),
            HostedAccountOutcome::RateLimited
        );
        assert_eq!(
            classify_hosted_account_error(None, 503),
            HostedAccountOutcome::TransientFailure
        );
        assert_eq!(
            classify_hosted_account_error(None, 404),
            HostedAccountOutcome::PermanentFailure
        );
    }

    #[test]
    fn registration_records_a_pending_verification_without_a_session() {
        let (client, dir) = client();
        let mut secrets = MemorySecrets::new();
        let transport = ScriptedTransport::new(vec![Ok(HostedAccountHttpResponse::new(
            200,
            Some(json!({
                "ok": true,
                "account": {
                    "account_id": "11111111-1111-4111-8111-111111111111",
                    "user_id": "22222222-2222-4222-8222-222222222222",
                    "email": "operator@example.com",
                    "display_name": "Operator",
                    "created_at": "2026-08-01T00:00:00Z",
                    "email_verified_at": null
                },
                "email_verification_required": true
            })),
        ))]);
        let result = client
            .execute(
                &HostedAccountAction::Register {
                    email: "operator@example.com".to_owned(),
                    display_name: Some("Operator".to_owned()),
                    invitation_token: Some("invite-token".to_owned()),
                    turnstile_token: None,
                },
                &config(),
                &transport,
                &mut secrets,
                now(),
            )
            .expect("register executes");

        assert!(result.outcome.is_accepted());
        assert_eq!(
            result.snapshot.connection_state,
            HostedAccountConnectionState::PendingEmailVerification
        );
        assert_eq!(
            result.snapshot.pending_email_verification_for.as_deref(),
            Some("operator@example.com")
        );
        assert!(result.snapshot.session_token_credential_id.is_none());
        let stored = std::fs::read_to_string(client.store().path()).expect("stored record");
        assert!(!stored.contains("invite-token"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn logout_clears_session_credentials_and_signs_out() {
        let (client, dir) = client();
        let mut secrets = MemorySecrets::new();
        let device_id = Uuid::new_v4();
        let signed_in = sign_in(&client, &mut secrets, device_id);
        let session_credential = signed_in
            .snapshot
            .session_token_credential_id
            .expect("session credential id");
        let refresh_credential = signed_in
            .snapshot
            .refresh_token_credential_id
            .expect("refresh credential id");

        let transport = ScriptedTransport::new(vec![Ok(HostedAccountHttpResponse::new(
            200,
            Some(json!({"ok": true})),
        ))]);
        let result = client
            .execute(
                &HostedAccountAction::Logout,
                &config(),
                &transport,
                &mut secrets,
                now(),
            )
            .expect("logout executes");

        assert!(result.outcome.is_accepted());
        assert_eq!(
            result.snapshot.connection_state,
            HostedAccountConnectionState::SignedOut
        );
        assert!(secrets
            .read_secret(session_credential)
            .expect("read")
            .is_none());
        assert!(secrets
            .read_secret(refresh_credential)
            .expect("read")
            .is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_support_records_are_quarantined_and_reinitialized() {
        let dir = std::env::temp_dir().join(format!("ham-sync-account-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("hosted-account.json");
        std::fs::write(&path, b"{ not json").expect("write corrupt record");
        let store = JsonHostedAccountStore::new(&path);

        let snapshot = store
            .load_or_initialize(&config(), now())
            .expect("corrupt record recovers");
        assert_eq!(
            snapshot.connection_state,
            HostedAccountConnectionState::SignedOut
        );
        assert_eq!(snapshot.base_url, "https://logger.example");
        let quarantined = std::fs::read_dir(&dir)
            .expect("dir listing")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains("corrupt-"));
        assert!(quarantined);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn unsupported_record_versions_are_rejected_before_use() {
        let dir = std::env::temp_dir().join(format!("ham-sync-account-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("hosted-account.json");
        std::fs::write(
            &path,
            serde_json::to_vec(&json!({
                "version": 99,
                "snapshot": HostedAccountSnapshot::new(&config().normalized().expect("config"))
            }))
            .expect("serialize"),
        )
        .expect("write record");
        let store = JsonHostedAccountStore::new(&path);
        assert!(matches!(
            store.snapshot(),
            Err(HostedAccountError::UnsupportedFileVersion(99))
        ));
        let _ = std::fs::remove_dir_all(dir);
    }
}
