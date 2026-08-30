//! Rust-owned hosted account and session client state.
//!
//! Clients own transport only. Rust plans every hosted account request, the
//! platform executes it over its own network stack, and Rust classifies the
//! response and owns each state transition. Raw session, refresh, invitation,
//! verification, and recovery tokens are never written to this support state;
//! only credential references are stored, and planned requests carry credential
//! IDs that the executing client resolves through its credential backend.

use std::{
    fs,
    io::{self, ErrorKind},
    net::{IpAddr, Ipv4Addr, Ipv6Addr},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value as JsonValue};
use thiserror::Error;
use uuid::Uuid;

use crate::offline::{quarantine_support_file, write_json_atomically};

pub const ACCOUNT_SESSION_FILE_VERSION: u32 = 1;

pub const ACCOUNT_PATH_STATUS: &str = "/api/v1/status";
pub const ACCOUNT_PATH_REGISTER: &str = "/api/v1/auth/register";
pub const ACCOUNT_PATH_VERIFY_EMAIL: &str = "/api/v1/auth/verify-email";
pub const ACCOUNT_PATH_LOGIN: &str = "/api/v1/auth/login";
pub const ACCOUNT_PATH_SESSION: &str = "/api/v1/auth/session";
pub const ACCOUNT_PATH_SESSION_ROTATE: &str = "/api/v1/auth/session/rotate";
pub const ACCOUNT_PATH_LOGOUT: &str = "/api/v1/auth/logout";
pub const ACCOUNT_PATH_LOGOUT_ALL: &str = "/api/v1/auth/logout-all";
pub const ACCOUNT_PATH_RECOVERY_START: &str = "/api/v1/auth/recovery/start";
pub const ACCOUNT_PATH_RECOVERY_COMPLETE: &str = "/api/v1/auth/recovery/complete";
pub const ACCOUNT_PATH_ACCOUNT_DELETE: &str = "/api/v1/auth/account/delete";
pub const ACCOUNT_PATH_DEVICES: &str = "/api/v1/devices";
pub const ACCOUNT_PATH_DEVICES_REVOKE_ALL: &str = "/api/v1/devices/revoke-all";

pub const ACCOUNT_SECRET_FIELD_REFRESH_TOKEN: &str = "refresh_token";

pub const MAX_ACCOUNT_EMAIL_BYTES: usize = 254;
pub const MAX_ACCOUNT_DISPLAY_NAME_BYTES: usize = 120;
pub const MAX_ACCOUNT_DEVICE_NAME_BYTES: usize = 120;
pub const MAX_ACCOUNT_TOKEN_BYTES: usize = 4096;
pub const MAX_ACCOUNT_MESSAGE_BYTES: usize = 512;

const REDACTED_TOKEN_DISPLAY: &str = "<redacted>";

#[derive(Debug, Error)]
pub enum AccountError {
    #[error("account session I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("account session serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("account session file version {0} is not supported")]
    UnsupportedFileVersion(u32),
    #[error("hosted server URL is required")]
    MissingServerUrl,
    #[error("hosted server URL {0:?} must be an absolute http:// or https:// URL")]
    InvalidServerUrl(String),
    #[error("hosted server URL {0:?} must use https:// outside loopback and private networks")]
    InsecureTransport(String),
    #[error("{field} is required")]
    MissingField { field: &'static str },
    #[error("{field} is longer than {max} bytes")]
    FieldTooLong { field: &'static str, max: usize },
    #[error("{0} requires a signed-in hosted session")]
    RequiresSession(&'static str),
    #[error("{0} requires a stored session credential reference")]
    MissingSessionCredential(&'static str),
    #[error("session rotation requires a stored refresh credential reference")]
    MissingRefreshCredential,
    #[error("account deletion requires explicit confirmation")]
    ConfirmationRequired,
}

/// Durable client-side view of the hosted account lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountSessionStatus {
    SignedOut,
    PendingEmailVerification,
    SignedIn,
    SessionExpired,
    DeviceRevoked,
    AccountDeleted,
}

impl AccountSessionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SignedOut => "signed_out",
            Self::PendingEmailVerification => "pending_email_verification",
            Self::SignedIn => "signed_in",
            Self::SessionExpired => "session_expired",
            Self::DeviceRevoked => "device_revoked",
            Self::AccountDeleted => "account_deleted",
        }
    }

    /// True when the client should present signed-in surfaces.
    pub const fn is_signed_in(self) -> bool {
        matches!(self, Self::SignedIn)
    }

    /// True when the operator must act before hosted calls can succeed again.
    pub const fn requires_user_action(self) -> bool {
        matches!(
            self,
            Self::PendingEmailVerification
                | Self::SessionExpired
                | Self::DeviceRevoked
                | Self::AccountDeleted
        )
    }
}

/// Hosted registration policy, read from the unauthenticated status route.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct AccountHostingSnapshot {
    #[serde(default)]
    pub operation_mode: Option<String>,
    #[serde(default)]
    pub registration_mode: Option<String>,
    #[serde(default)]
    pub bootstrap_admin_completed: Option<bool>,
    #[serde(default)]
    pub turnstile_required: bool,
    #[serde(default)]
    pub turnstile_site_key: Option<String>,
    #[serde(default)]
    pub observed_at: Option<DateTime<Utc>>,
}

impl AccountHostingSnapshot {
    /// True when the hosted server accepts self-service registration.
    pub fn allows_open_registration(&self) -> bool {
        self.registration_mode.as_deref() == Some("open")
    }

    /// True when registration requires an invitation token.
    pub fn requires_invitation(&self) -> bool {
        self.registration_mode.as_deref() == Some("invite_only")
    }

    /// True when registration is administratively disabled.
    pub fn registration_disabled(&self) -> bool {
        self.registration_mode.as_deref() == Some("disabled")
    }
}

/// Client mirror of the hosted account record. Token fields are intentionally
/// absent so hosted responses cannot introduce secrets into support state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountProfile {
    pub account_id: Uuid,
    pub user_id: Uuid,
    pub email: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub email_verified_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub deleted_at: Option<DateTime<Utc>>,
}

impl AccountProfile {
    pub fn email_verified(&self) -> bool {
        self.email_verified_at.is_some()
    }
}

/// Client mirror of the hosted login session record without token material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSessionInfo {
    pub session_id: Uuid,
    pub account_id: Uuid,
    pub user_id: Uuid,
    pub device_id: Uuid,
    #[serde(default)]
    pub issued_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub refresh_expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub rotated_at: Option<DateTime<Utc>>,
    #[serde(default = "default_true")]
    pub active: bool,
}

impl AccountSessionInfo {
    pub fn is_expired_at(&self, now: DateTime<Utc>) -> bool {
        self.expires_at.is_some_and(|expires_at| expires_at <= now)
    }
}

/// Client mirror of a hosted device record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountDevice {
    pub device_id: Uuid,
    #[serde(default)]
    pub account_id: Option<Uuid>,
    #[serde(default)]
    pub user_id: Option<Uuid>,
    #[serde(default)]
    pub device_name: String,
    #[serde(default)]
    pub trusted: bool,
    #[serde(default)]
    pub revoked: bool,
    #[serde(default)]
    pub registered_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub revoked_at: Option<DateTime<Utc>>,
}

/// Client mirror of a hosted logbook summary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountLogbookSummary {
    pub logbook_id: Uuid,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub station_callsign: Option<String>,
}

/// Client mirror of a hosted logbook membership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountMembership {
    pub logbook_id: Uuid,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
}

const fn default_true() -> bool {
    true
}

/// Durable, Rust-owned hosted account support state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSessionState {
    pub version: u32,
    #[serde(default)]
    pub server_url: String,
    pub status: AccountSessionStatus,
    #[serde(default)]
    pub hosting: AccountHostingSnapshot,
    #[serde(default)]
    pub account: Option<AccountProfile>,
    #[serde(default)]
    pub session: Option<AccountSessionInfo>,
    #[serde(default)]
    pub device: Option<AccountDevice>,
    #[serde(default)]
    pub logbooks: Vec<AccountLogbookSummary>,
    #[serde(default)]
    pub memberships: Vec<AccountMembership>,
    #[serde(default)]
    pub devices: Vec<AccountDevice>,
    #[serde(default)]
    pub session_token_credential_id: Option<Uuid>,
    #[serde(default)]
    pub refresh_token_credential_id: Option<Uuid>,
    #[serde(default)]
    pub pending_email: Option<String>,
    #[serde(default)]
    pub email_verification_required: bool,
    #[serde(default)]
    pub last_action: Option<AccountActionRecord>,
    pub updated_at: DateTime<Utc>,
}

impl Default for AccountSessionState {
    fn default() -> Self {
        Self {
            version: ACCOUNT_SESSION_FILE_VERSION,
            server_url: String::new(),
            status: AccountSessionStatus::SignedOut,
            hosting: AccountHostingSnapshot::default(),
            account: None,
            session: None,
            device: None,
            logbooks: Vec::new(),
            memberships: Vec::new(),
            devices: Vec::new(),
            session_token_credential_id: None,
            refresh_token_credential_id: None,
            pending_email: None,
            email_verification_required: false,
            last_action: None,
            updated_at: DateTime::<Utc>::MIN_UTC,
        }
    }
}

impl AccountSessionState {
    pub fn new(server_url: impl Into<String>, now: DateTime<Utc>) -> Self {
        Self {
            server_url: server_url.into(),
            updated_at: now,
            ..Self::default()
        }
    }

    fn validate(&self) -> Result<(), AccountError> {
        if self.version != ACCOUNT_SESSION_FILE_VERSION {
            return Err(AccountError::UnsupportedFileVersion(self.version));
        }
        Ok(())
    }

    /// Record the credential references produced by a signed-in response. Raw
    /// secrets stay in the caller's credential backend.
    pub fn record_session_credentials(
        &mut self,
        session_token_credential_id: Option<Uuid>,
        refresh_token_credential_id: Option<Uuid>,
        now: DateTime<Utc>,
    ) {
        if session_token_credential_id.is_some() {
            self.session_token_credential_id = session_token_credential_id;
        }
        if refresh_token_credential_id.is_some() {
            self.refresh_token_credential_id = refresh_token_credential_id;
        }
        self.updated_at = now;
    }

    /// Forget every credential reference without deleting the operator's
    /// cached account view.
    pub fn clear_session_credentials(&mut self, now: DateTime<Utc>) {
        self.session_token_credential_id = None;
        self.refresh_token_credential_id = None;
        self.updated_at = now;
    }

    pub fn has_session_credential(&self) -> bool {
        self.session_token_credential_id.is_some()
    }

    /// Normalized `scheme://host[:port]` origin for hosted account requests.
    pub fn normalized_server_url(&self) -> Result<String, AccountError> {
        normalize_server_url(&self.server_url)
    }
}

/// A hosted account operation the operator asked for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum AccountAction {
    HostingStatus,
    Register {
        email: String,
        #[serde(default)]
        display_name: Option<String>,
        #[serde(default)]
        device_name: Option<String>,
        #[serde(default)]
        invitation_token: Option<String>,
        #[serde(default)]
        turnstile_token: Option<String>,
    },
    VerifyEmail {
        token: String,
    },
    Login {
        email: String,
        #[serde(default)]
        display_name: Option<String>,
        #[serde(default)]
        device_name: Option<String>,
    },
    RefreshSession,
    RotateSession,
    Logout,
    LogoutAll,
    RecoveryStart {
        email: String,
    },
    RecoveryComplete {
        token: String,
        #[serde(default)]
        device_name: Option<String>,
    },
    ListDevices,
    RevokeDevice {
        device_id: Uuid,
    },
    RevokeAllDevices,
    DeleteAccount {
        confirm: bool,
    },
}

impl AccountAction {
    pub const fn kind(&self) -> AccountActionKind {
        match self {
            Self::HostingStatus => AccountActionKind::HostingStatus,
            Self::Register { .. } => AccountActionKind::Register,
            Self::VerifyEmail { .. } => AccountActionKind::VerifyEmail,
            Self::Login { .. } => AccountActionKind::Login,
            Self::RefreshSession => AccountActionKind::RefreshSession,
            Self::RotateSession => AccountActionKind::RotateSession,
            Self::Logout => AccountActionKind::Logout,
            Self::LogoutAll => AccountActionKind::LogoutAll,
            Self::RecoveryStart { .. } => AccountActionKind::RecoveryStart,
            Self::RecoveryComplete { .. } => AccountActionKind::RecoveryComplete,
            Self::ListDevices => AccountActionKind::ListDevices,
            Self::RevokeDevice { .. } => AccountActionKind::RevokeDevice,
            Self::RevokeAllDevices => AccountActionKind::RevokeAllDevices,
            Self::DeleteAccount { .. } => AccountActionKind::DeleteAccount,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountActionKind {
    HostingStatus,
    Register,
    VerifyEmail,
    Login,
    RefreshSession,
    RotateSession,
    Logout,
    LogoutAll,
    RecoveryStart,
    RecoveryComplete,
    ListDevices,
    RevokeDevice,
    RevokeAllDevices,
    DeleteAccount,
}

impl AccountActionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HostingStatus => "hosting_status",
            Self::Register => "register",
            Self::VerifyEmail => "verify_email",
            Self::Login => "login",
            Self::RefreshSession => "refresh_session",
            Self::RotateSession => "rotate_session",
            Self::Logout => "logout",
            Self::LogoutAll => "logout_all",
            Self::RecoveryStart => "recovery_start",
            Self::RecoveryComplete => "recovery_complete",
            Self::ListDevices => "list_devices",
            Self::RevokeDevice => "revoke_device",
            Self::RevokeAllDevices => "revoke_all_devices",
            Self::DeleteAccount => "delete_account",
        }
    }

    /// True when the action is sent with the stored bearer session, so an
    /// authorization failure really does mean this client's session is gone.
    pub const fn uses_session(self) -> bool {
        matches!(
            self,
            Self::RefreshSession
                | Self::RotateSession
                | Self::Logout
                | Self::LogoutAll
                | Self::ListDevices
                | Self::RevokeDevice
                | Self::RevokeAllDevices
                | Self::DeleteAccount
        )
    }

    /// Runtime event type published by clients for this action.
    pub const fn runtime_event_type(self) -> &'static str {
        match self {
            Self::HostingStatus => "account.hosting.status",
            Self::Register => "account.register",
            Self::VerifyEmail => "account.email.verify",
            Self::Login => "account.login",
            Self::RefreshSession => "account.session.refresh",
            Self::RotateSession => "account.session.rotate",
            Self::Logout => "account.logout",
            Self::LogoutAll => "account.logout_all",
            Self::RecoveryStart => "account.recovery.start",
            Self::RecoveryComplete => "account.recovery.complete",
            Self::ListDevices => "account.devices.list",
            Self::RevokeDevice => "account.device.revoke",
            Self::RevokeAllDevices => "account.devices.revoke_all",
            Self::DeleteAccount => "account.delete",
        }
    }
}

/// A body field the executing client must fill from its credential backend.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSecretField {
    pub field: String,
    pub credential_id: Uuid,
}

/// A hosted request planned by Rust and executed by the platform transport.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountRequestPlan {
    pub kind: AccountActionKind,
    pub method: String,
    pub path: String,
    pub url: String,
    #[serde(default)]
    pub body: Option<JsonValue>,
    #[serde(default)]
    pub requires_bearer: bool,
    #[serde(default)]
    pub bearer_credential_id: Option<Uuid>,
    #[serde(default)]
    pub body_secret_fields: Vec<AccountSecretField>,
    pub idempotent: bool,
}

/// Raw transport result handed back to Rust for classification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AccountResponseInput {
    #[serde(default)]
    pub status: u16,
    #[serde(default)]
    pub body: Option<JsonValue>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub transport_error: Option<String>,
}

impl AccountResponseInput {
    pub fn success(status: u16, body: JsonValue) -> Self {
        Self {
            status,
            body: Some(body),
            request_id: None,
            transport_error: None,
        }
    }

    pub fn transport_failure(message: impl Into<String>) -> Self {
        Self {
            status: 0,
            body: None,
            request_id: None,
            transport_error: Some(message.into()),
        }
    }
}

/// Rust classification of a hosted account response.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountOutcome {
    Succeeded,
    EmailVerificationRequired,
    InvalidRequest,
    Unauthorized,
    SessionExpired,
    DeviceRevoked,
    RegistrationClosed,
    TokenExpired,
    TokenReplayed,
    TurnstileFailed,
    Forbidden,
    NotFound,
    RateLimited,
    ServerUnavailable,
    NetworkUnavailable,
}

impl AccountOutcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::EmailVerificationRequired => "email_verification_required",
            Self::InvalidRequest => "invalid_request",
            Self::Unauthorized => "unauthorized",
            Self::SessionExpired => "session_expired",
            Self::DeviceRevoked => "device_revoked",
            Self::RegistrationClosed => "registration_closed",
            Self::TokenExpired => "token_expired",
            Self::TokenReplayed => "token_replayed",
            Self::TurnstileFailed => "turnstile_failed",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::RateLimited => "rate_limited",
            Self::ServerUnavailable => "server_unavailable",
            Self::NetworkUnavailable => "network_unavailable",
        }
    }

    pub const fn succeeded(self) -> bool {
        matches!(self, Self::Succeeded)
    }

    /// True when retrying the same request later can plausibly succeed without
    /// operator input.
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::RateLimited | Self::ServerUnavailable | Self::NetworkUnavailable
        )
    }

    /// True when the operator must do something before this action can work.
    pub const fn requires_user_action(self) -> bool {
        matches!(
            self,
            Self::EmailVerificationRequired
                | Self::InvalidRequest
                | Self::Unauthorized
                | Self::SessionExpired
                | Self::DeviceRevoked
                | Self::RegistrationClosed
                | Self::TokenExpired
                | Self::TokenReplayed
                | Self::TurnstileFailed
                | Self::Forbidden
        )
    }
}

/// Redacted, durable record of the most recent hosted account action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountActionRecord {
    pub kind: AccountActionKind,
    pub outcome: AccountOutcome,
    pub message: String,
    #[serde(default)]
    pub error_code: Option<String>,
    #[serde(default)]
    pub request_id: Option<String>,
    pub retryable: bool,
    pub occurred_at: DateTime<Utc>,
}

/// Outcome of applying a hosted response, including any freshly issued tokens
/// the caller must hand to its credential backend.
#[derive(Clone, PartialEq, Eq)]
pub struct AccountActionResult {
    pub record: AccountActionRecord,
    pub state_changed: bool,
    pub issued_session_token: Option<String>,
    pub issued_refresh_token: Option<String>,
    pub clear_session_credentials: bool,
}

impl std::fmt::Debug for AccountActionResult {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AccountActionResult")
            .field("record", &self.record)
            .field("state_changed", &self.state_changed)
            .field(
                "issued_session_token",
                &self
                    .issued_session_token
                    .as_ref()
                    .map(|_| REDACTED_TOKEN_DISPLAY),
            )
            .field(
                "issued_refresh_token",
                &self
                    .issued_refresh_token
                    .as_ref()
                    .map(|_| REDACTED_TOKEN_DISPLAY),
            )
            .field("clear_session_credentials", &self.clear_session_credentials)
            .finish()
    }
}

impl AccountActionResult {
    pub fn issued_tokens(&self) -> bool {
        self.issued_session_token.is_some() || self.issued_refresh_token.is_some()
    }
}

/// Plan a hosted account request without touching secrets or the network.
pub fn plan_account_request(
    state: &AccountSessionState,
    action: &AccountAction,
) -> Result<AccountRequestPlan, AccountError> {
    let origin = normalize_server_url(&state.server_url)?;
    let kind = action.kind();
    let (method, path, body, requires_bearer, idempotent, secret_fields) = match action {
        AccountAction::HostingStatus => (
            "GET",
            ACCOUNT_PATH_STATUS.to_owned(),
            None,
            false,
            true,
            Vec::new(),
        ),
        AccountAction::Register {
            email,
            display_name,
            device_name,
            invitation_token,
            turnstile_token,
        } => {
            let email = require_email(email)?;
            let mut payload = Map::new();
            payload.insert("email".to_owned(), JsonValue::String(email));
            insert_optional_text(
                &mut payload,
                "display_name",
                display_name,
                MAX_ACCOUNT_DISPLAY_NAME_BYTES,
                "display_name",
            )?;
            insert_optional_text(
                &mut payload,
                "device_name",
                device_name,
                MAX_ACCOUNT_DEVICE_NAME_BYTES,
                "device_name",
            )?;
            insert_optional_text(
                &mut payload,
                "invitation_token",
                invitation_token,
                MAX_ACCOUNT_TOKEN_BYTES,
                "invitation_token",
            )?;
            insert_optional_text(
                &mut payload,
                "turnstile_token",
                turnstile_token,
                MAX_ACCOUNT_TOKEN_BYTES,
                "turnstile_token",
            )?;
            (
                "POST",
                ACCOUNT_PATH_REGISTER.to_owned(),
                Some(JsonValue::Object(payload)),
                false,
                false,
                Vec::new(),
            )
        }
        AccountAction::VerifyEmail { token } => {
            let token = require_text(token, MAX_ACCOUNT_TOKEN_BYTES, "token")?;
            (
                "POST",
                ACCOUNT_PATH_VERIFY_EMAIL.to_owned(),
                Some(json!({ "token": token })),
                false,
                false,
                Vec::new(),
            )
        }
        AccountAction::Login {
            email,
            display_name,
            device_name,
        } => {
            let email = require_email(email)?;
            let mut payload = Map::new();
            payload.insert("email".to_owned(), JsonValue::String(email));
            insert_optional_text(
                &mut payload,
                "display_name",
                display_name,
                MAX_ACCOUNT_DISPLAY_NAME_BYTES,
                "display_name",
            )?;
            insert_optional_text(
                &mut payload,
                "device_name",
                device_name,
                MAX_ACCOUNT_DEVICE_NAME_BYTES,
                "device_name",
            )?;
            (
                "POST",
                ACCOUNT_PATH_LOGIN.to_owned(),
                Some(JsonValue::Object(payload)),
                false,
                false,
                Vec::new(),
            )
        }
        AccountAction::RefreshSession => (
            "GET",
            ACCOUNT_PATH_SESSION.to_owned(),
            None,
            true,
            true,
            Vec::new(),
        ),
        AccountAction::RotateSession => {
            let refresh_credential_id = state
                .refresh_token_credential_id
                .ok_or(AccountError::MissingRefreshCredential)?;
            (
                "POST",
                ACCOUNT_PATH_SESSION_ROTATE.to_owned(),
                Some(JsonValue::Object(Map::new())),
                true,
                false,
                vec![AccountSecretField {
                    field: ACCOUNT_SECRET_FIELD_REFRESH_TOKEN.to_owned(),
                    credential_id: refresh_credential_id,
                }],
            )
        }
        AccountAction::Logout => (
            "POST",
            ACCOUNT_PATH_LOGOUT.to_owned(),
            Some(JsonValue::Object(Map::new())),
            true,
            true,
            Vec::new(),
        ),
        AccountAction::LogoutAll => (
            "POST",
            ACCOUNT_PATH_LOGOUT_ALL.to_owned(),
            Some(JsonValue::Object(Map::new())),
            true,
            true,
            Vec::new(),
        ),
        AccountAction::RecoveryStart { email } => {
            let email = require_email(email)?;
            (
                "POST",
                ACCOUNT_PATH_RECOVERY_START.to_owned(),
                Some(json!({ "email": email })),
                false,
                true,
                Vec::new(),
            )
        }
        AccountAction::RecoveryComplete { token, device_name } => {
            let token = require_text(token, MAX_ACCOUNT_TOKEN_BYTES, "token")?;
            let mut payload = Map::new();
            payload.insert("token".to_owned(), JsonValue::String(token));
            insert_optional_text(
                &mut payload,
                "device_name",
                device_name,
                MAX_ACCOUNT_DEVICE_NAME_BYTES,
                "device_name",
            )?;
            (
                "POST",
                ACCOUNT_PATH_RECOVERY_COMPLETE.to_owned(),
                Some(JsonValue::Object(payload)),
                false,
                false,
                Vec::new(),
            )
        }
        AccountAction::ListDevices => (
            "GET",
            ACCOUNT_PATH_DEVICES.to_owned(),
            None,
            true,
            true,
            Vec::new(),
        ),
        AccountAction::RevokeDevice { device_id } => (
            "POST",
            format!("{ACCOUNT_PATH_DEVICES}/{device_id}/revoke"),
            Some(JsonValue::Object(Map::new())),
            true,
            true,
            Vec::new(),
        ),
        AccountAction::RevokeAllDevices => (
            "POST",
            ACCOUNT_PATH_DEVICES_REVOKE_ALL.to_owned(),
            Some(JsonValue::Object(Map::new())),
            true,
            true,
            Vec::new(),
        ),
        AccountAction::DeleteAccount { confirm } => {
            if !confirm {
                return Err(AccountError::ConfirmationRequired);
            }
            (
                "POST",
                ACCOUNT_PATH_ACCOUNT_DELETE.to_owned(),
                Some(json!({ "confirm": true })),
                true,
                false,
                Vec::new(),
            )
        }
    };

    let bearer_credential_id = if requires_bearer {
        Some(
            state
                .session_token_credential_id
                .ok_or(AccountError::MissingSessionCredential(kind.as_str()))?,
        )
    } else {
        None
    };

    let url = format!("{origin}{path}");
    ensure_transport_is_permitted(&url)?;

    Ok(AccountRequestPlan {
        kind,
        method: method.to_owned(),
        url,
        path,
        body,
        requires_bearer,
        bearer_credential_id,
        body_secret_fields: secret_fields,
        idempotent,
    })
}

/// Classify a hosted response and apply every resulting state transition.
pub fn apply_account_response(
    state: &mut AccountSessionState,
    action: &AccountAction,
    response: &AccountResponseInput,
    now: DateTime<Utc>,
) -> AccountActionResult {
    let kind = action.kind();
    let body = response.body.as_ref();
    let error_code = body
        .and_then(|body| body.get("code"))
        .and_then(JsonValue::as_str)
        .map(str::to_owned);
    let request_id = response
        .request_id
        .clone()
        .or_else(|| {
            body.and_then(|body| body.get("request_id"))
                .and_then(JsonValue::as_str)
                .map(str::to_owned)
        })
        .filter(|value| !value.trim().is_empty());
    let outcome = classify_outcome(response, error_code.as_deref());
    let message = response_message(response, outcome, error_code.as_deref());

    let mut result = AccountActionResult {
        record: AccountActionRecord {
            kind,
            outcome,
            message,
            error_code,
            request_id,
            retryable: outcome.retryable(),
            occurred_at: now,
        },
        state_changed: false,
        issued_session_token: None,
        issued_refresh_token: None,
        clear_session_credentials: false,
    };

    if outcome.succeeded() {
        apply_success(state, action, body, now, &mut result);
    } else {
        apply_failure(state, kind, outcome, now, &mut result);
    }

    state.last_action = Some(result.record.clone());
    state.updated_at = now;
    result
}

fn apply_success(
    state: &mut AccountSessionState,
    action: &AccountAction,
    body: Option<&JsonValue>,
    now: DateTime<Utc>,
    result: &mut AccountActionResult,
) {
    match action {
        AccountAction::HostingStatus => {
            if let Some(body) = body {
                state.hosting = hosting_snapshot_from_status(body, now);
                result.state_changed = true;
            }
        }
        AccountAction::Register { .. } => {
            if let Some(account) = decode_account(body) {
                state.pending_email = Some(account.email.clone());
                state.account = Some(account);
            }
            state.logbooks = decode_logbooks(body);
            state.email_verification_required = body
                .and_then(|body| body.get("email_verification_required"))
                .and_then(JsonValue::as_bool)
                .unwrap_or(true);
            state.status = if state.email_verification_required {
                AccountSessionStatus::PendingEmailVerification
            } else {
                AccountSessionStatus::SignedOut
            };
            result.state_changed = true;
        }
        AccountAction::VerifyEmail { .. } => {
            if let Some(account) = decode_account(body) {
                state.pending_email = None;
                state.account = Some(account);
            }
            state.email_verification_required = false;
            if !state.status.is_signed_in() {
                state.status = AccountSessionStatus::SignedOut;
            }
            result.state_changed = true;
        }
        AccountAction::Login { .. } | AccountAction::RecoveryComplete { .. } => {
            apply_login_success(state, body, now, result);
        }
        AccountAction::RefreshSession => {
            if let Some(account) = decode_account(body) {
                state.account = Some(account);
            }
            if let Some(session) = decode_session(body) {
                state.session = Some(session);
            }
            if let Some(device) = decode_device(body, "device") {
                state.device = Some(device);
            }
            state.memberships = decode_memberships(body);
            state.email_verification_required = false;
            state.status = AccountSessionStatus::SignedIn;
            result.state_changed = true;
        }
        AccountAction::RotateSession => {
            apply_login_success(state, body, now, result);
        }
        AccountAction::Logout => {
            sign_out(state, AccountSessionStatus::SignedOut, now, result);
        }
        AccountAction::LogoutAll => {
            sign_out(state, AccountSessionStatus::SignedOut, now, result);
        }
        AccountAction::RecoveryStart { .. } => {}
        AccountAction::ListDevices => {
            state.devices = decode_devices(body);
            result.state_changed = true;
        }
        AccountAction::RevokeDevice { device_id } => {
            for device in &mut state.devices {
                if device.device_id == *device_id {
                    device.revoked = true;
                    device.revoked_at = Some(now);
                }
            }
            let revoked_current_device = state
                .device
                .as_ref()
                .is_some_and(|device| device.device_id == *device_id);
            if revoked_current_device {
                sign_out(state, AccountSessionStatus::DeviceRevoked, now, result);
            }
            result.state_changed = true;
        }
        AccountAction::RevokeAllDevices => {
            for device in &mut state.devices {
                device.revoked = true;
                device.revoked_at = Some(now);
            }
            sign_out(state, AccountSessionStatus::DeviceRevoked, now, result);
        }
        AccountAction::DeleteAccount { .. } => {
            sign_out(state, AccountSessionStatus::AccountDeleted, now, result);
            if let Some(account) = state.account.as_mut() {
                account.deleted_at = Some(now);
            }
            state.logbooks.clear();
            state.memberships.clear();
            state.devices.clear();
        }
    }
}

fn apply_login_success(
    state: &mut AccountSessionState,
    body: Option<&JsonValue>,
    now: DateTime<Utc>,
    result: &mut AccountActionResult,
) {
    if let Some(account) = decode_account(body) {
        state.pending_email = None;
        state.account = Some(account);
    }
    if let Some(session) = decode_session(body) {
        state.session = Some(session);
    }
    if let Some(device) = decode_device(body, "device") {
        state.device = Some(device);
    }
    let logbooks = decode_logbooks(body);
    if !logbooks.is_empty() {
        state.logbooks = logbooks;
    }
    result.issued_session_token = body
        .and_then(|body| body.get("session"))
        .and_then(|session| session.get("token"))
        .and_then(JsonValue::as_str)
        .filter(|token| !token.trim().is_empty())
        .map(str::to_owned);
    result.issued_refresh_token = body
        .and_then(|body| body.get("refresh_token"))
        .and_then(JsonValue::as_str)
        .filter(|token| !token.trim().is_empty())
        .map(str::to_owned);
    state.email_verification_required = false;
    state.status = AccountSessionStatus::SignedIn;
    state.updated_at = now;
    result.state_changed = true;
}

fn sign_out(
    state: &mut AccountSessionState,
    status: AccountSessionStatus,
    now: DateTime<Utc>,
    result: &mut AccountActionResult,
) {
    state.status = status;
    state.session = None;
    state.clear_session_credentials(now);
    result.clear_session_credentials = true;
    result.state_changed = true;
}

fn apply_failure(
    state: &mut AccountSessionState,
    kind: AccountActionKind,
    outcome: AccountOutcome,
    now: DateTime<Utc>,
    result: &mut AccountActionResult,
) {
    match outcome {
        AccountOutcome::SessionExpired | AccountOutcome::Unauthorized if kind.uses_session() => {
            if state.has_session_credential() || state.status.is_signed_in() {
                sign_out(state, AccountSessionStatus::SessionExpired, now, result);
            }
        }
        AccountOutcome::DeviceRevoked if kind.uses_session() => {
            sign_out(state, AccountSessionStatus::DeviceRevoked, now, result);
        }
        AccountOutcome::EmailVerificationRequired => {
            state.email_verification_required = true;
            if !state.status.is_signed_in() {
                state.status = AccountSessionStatus::PendingEmailVerification;
            }
            result.state_changed = true;
        }
        _ => {}
    }
}

fn classify_outcome(response: &AccountResponseInput, error_code: Option<&str>) -> AccountOutcome {
    if response.transport_error.is_some() {
        return AccountOutcome::NetworkUnavailable;
    }
    if response.status == 0 {
        return AccountOutcome::NetworkUnavailable;
    }
    if (200..300).contains(&response.status) {
        return AccountOutcome::Succeeded;
    }
    if let Some(code) = error_code {
        match code {
            "email_unverified" => return AccountOutcome::EmailVerificationRequired,
            "registration_closed" => return AccountOutcome::RegistrationClosed,
            "token_expired" => return AccountOutcome::TokenExpired,
            "token_replayed" => return AccountOutcome::TokenReplayed,
            "turnstile_failed" => return AccountOutcome::TurnstileFailed,
            "session_expired" => return AccountOutcome::SessionExpired,
            "session_inactive" | "missing_token" | "invalid_token" => {
                return AccountOutcome::Unauthorized
            }
            "device_revoked" => return AccountOutcome::DeviceRevoked,
            "rate_limited" => return AccountOutcome::RateLimited,
            "forbidden" => return AccountOutcome::Forbidden,
            "not_found" => return AccountOutcome::NotFound,
            "store_unavailable" | "internal_error" => return AccountOutcome::ServerUnavailable,
            _ => {}
        }
    }
    match response.status {
        400 | 415 | 422 => AccountOutcome::InvalidRequest,
        401 => AccountOutcome::Unauthorized,
        403 => AccountOutcome::Forbidden,
        404 => AccountOutcome::NotFound,
        429 => AccountOutcome::RateLimited,
        500..=599 => AccountOutcome::ServerUnavailable,
        _ => AccountOutcome::InvalidRequest,
    }
}

fn response_message(
    response: &AccountResponseInput,
    outcome: AccountOutcome,
    error_code: Option<&str>,
) -> String {
    if let Some(transport_error) = response.transport_error.as_deref() {
        return truncate_message(&format!("hosted request failed: {transport_error}"));
    }
    if outcome.succeeded() {
        return "hosted request succeeded".to_owned();
    }
    let server_message = response
        .body
        .as_ref()
        .and_then(|body| body.get("error"))
        .and_then(JsonValue::as_str)
        .filter(|message| !message.trim().is_empty());
    match (server_message, error_code) {
        (Some(message), Some(code)) => truncate_message(&format!("{message} ({code})")),
        (Some(message), None) => truncate_message(message),
        (None, Some(code)) => truncate_message(&format!("hosted request failed: {code}")),
        (None, None) => truncate_message(&format!(
            "hosted request failed with status {}",
            response.status
        )),
    }
}

fn truncate_message(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.len() <= MAX_ACCOUNT_MESSAGE_BYTES {
        return trimmed.to_owned();
    }
    let mut end = MAX_ACCOUNT_MESSAGE_BYTES;
    while end > 0 && !trimmed.is_char_boundary(end) {
        end -= 1;
    }
    trimmed[..end].to_owned()
}

fn hosting_snapshot_from_status(body: &JsonValue, now: DateTime<Utc>) -> AccountHostingSnapshot {
    let turnstile = body.get("turnstile");
    AccountHostingSnapshot {
        operation_mode: body
            .get("operation_mode")
            .and_then(JsonValue::as_str)
            .map(str::to_owned),
        registration_mode: body
            .get("registration_mode")
            .and_then(JsonValue::as_str)
            .map(str::to_owned),
        bootstrap_admin_completed: body
            .get("bootstrap_admin_completed")
            .and_then(JsonValue::as_bool),
        turnstile_required: turnstile
            .and_then(|turnstile| turnstile.get("required"))
            .and_then(JsonValue::as_bool)
            .unwrap_or(false),
        turnstile_site_key: turnstile
            .and_then(|turnstile| turnstile.get("site_key"))
            .and_then(JsonValue::as_str)
            .filter(|site_key| !site_key.trim().is_empty())
            .map(str::to_owned),
        observed_at: Some(now),
    }
}

fn decode_account(body: Option<&JsonValue>) -> Option<AccountProfile> {
    body.and_then(|body| body.get("account"))
        .and_then(|account| serde_json::from_value(account.clone()).ok())
}

fn decode_session(body: Option<&JsonValue>) -> Option<AccountSessionInfo> {
    body.and_then(|body| body.get("session"))
        .and_then(|session| serde_json::from_value(session.clone()).ok())
}

fn decode_device(body: Option<&JsonValue>, key: &str) -> Option<AccountDevice> {
    body.and_then(|body| body.get(key))
        .and_then(|device| serde_json::from_value(device.clone()).ok())
}

fn decode_logbooks(body: Option<&JsonValue>) -> Vec<AccountLogbookSummary> {
    decode_list(body, "logbooks")
}

fn decode_memberships(body: Option<&JsonValue>) -> Vec<AccountMembership> {
    decode_list(body, "memberships")
}

fn decode_devices(body: Option<&JsonValue>) -> Vec<AccountDevice> {
    decode_list(body, "devices")
}

fn decode_list<T>(body: Option<&JsonValue>, key: &str) -> Vec<T>
where
    T: for<'de> Deserialize<'de>,
{
    body.and_then(|body| body.get(key))
        .and_then(JsonValue::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| serde_json::from_value(entry.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

fn require_email(email: &str) -> Result<String, AccountError> {
    let email = require_text(email, MAX_ACCOUNT_EMAIL_BYTES, "email")?;
    let normalized = email.to_lowercase();
    let (local, domain) = normalized
        .split_once('@')
        .ok_or(AccountError::MissingField { field: "email" })?;
    if local.is_empty() || !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.')
    {
        return Err(AccountError::MissingField { field: "email" });
    }
    Ok(normalized)
}

fn require_text(value: &str, max: usize, field: &'static str) -> Result<String, AccountError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AccountError::MissingField { field });
    }
    if trimmed.len() > max {
        return Err(AccountError::FieldTooLong { field, max });
    }
    Ok(trimmed.to_owned())
}

fn insert_optional_text(
    payload: &mut Map<String, JsonValue>,
    key: &str,
    value: &Option<String>,
    max: usize,
    field: &'static str,
) -> Result<(), AccountError> {
    let Some(value) = value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };
    if value.len() > max {
        return Err(AccountError::FieldTooLong { field, max });
    }
    payload.insert(key.to_owned(), JsonValue::String(value.to_owned()));
    Ok(())
}

/// Normalize a hosted server URL down to `scheme://host[:port]`.
pub fn normalize_server_url(server_url: &str) -> Result<String, AccountError> {
    let trimmed = server_url.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err(AccountError::MissingServerUrl);
    }
    let (scheme, remainder) = if let Some(remainder) = trimmed.strip_prefix("https://") {
        ("https", remainder)
    } else if let Some(remainder) = trimmed.strip_prefix("http://") {
        ("http", remainder)
    } else {
        return Err(AccountError::InvalidServerUrl(trimmed.to_owned()));
    };
    let authority = remainder
        .split('/')
        .next()
        .unwrap_or_default()
        .trim_end_matches('.');
    if authority.is_empty() || authority.contains(['@', ' ', '\t']) {
        return Err(AccountError::InvalidServerUrl(trimmed.to_owned()));
    }
    Ok(format!("{scheme}://{authority}"))
}

/// Reject cleartext hosted transport for anything but loopback, private, and
/// link-local addresses used by development and self-hosted deployments.
pub fn ensure_transport_is_permitted(url: &str) -> Result<(), AccountError> {
    let Some(remainder) = url.trim().strip_prefix("http://") else {
        return Ok(());
    };
    let authority = remainder.split('/').next().unwrap_or_default();
    let host = authority_host(authority);
    if host.eq_ignore_ascii_case("localhost") {
        return Ok(());
    }
    match host.parse::<IpAddr>() {
        Ok(IpAddr::V4(address)) if is_local_ipv4(address) => Ok(()),
        Ok(IpAddr::V6(address)) if is_local_ipv6(address) => Ok(()),
        _ => Err(AccountError::InsecureTransport(url.trim().to_owned())),
    }
}

fn authority_host(authority: &str) -> &str {
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or_default();
    }
    authority.split(':').next().unwrap_or_default()
}

fn is_local_ipv4(address: Ipv4Addr) -> bool {
    address.is_loopback() || address.is_private() || address.is_link_local()
}

fn is_local_ipv6(address: Ipv6Addr) -> bool {
    let octets = address.octets();
    address.is_loopback()
        || (octets[0] & 0xfe) == 0xfc
        || (octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80)
}

/// Durable JSON store for hosted account support state.
#[derive(Debug, Clone)]
pub struct JsonAccountSessionStore {
    path: PathBuf,
}

impl JsonAccountSessionStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load durable state, creating a default record when none exists. A
    /// corrupt or unsupported file is quarantined rather than deleted.
    pub fn load_or_create(
        &self,
        default_server_url: &str,
        now: DateTime<Utc>,
    ) -> Result<AccountSessionState, AccountError> {
        match self.load() {
            Ok(state) => Ok(state),
            Err(AccountError::Io(error)) if error.kind() == ErrorKind::NotFound => {
                let state = AccountSessionState::new(default_server_url, now);
                self.save(&state)?;
                Ok(state)
            }
            Err(AccountError::Io(error)) => Err(AccountError::Io(error)),
            Err(_) => {
                quarantine_support_file(&self.path, now)?;
                let state = AccountSessionState::new(default_server_url, now);
                self.save(&state)?;
                Ok(state)
            }
        }
    }

    pub fn load(&self) -> Result<AccountSessionState, AccountError> {
        let contents = fs::read_to_string(&self.path)?;
        let state: AccountSessionState = serde_json::from_str(&contents)?;
        state.validate()?;
        Ok(state)
    }

    pub fn save(&self, state: &AccountSessionState) -> Result<(), AccountError> {
        write_json_atomically(&self.path, state)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_store() -> (JsonAccountSessionStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("ham-sync-account-{}", Uuid::new_v4()));
        (
            JsonAccountSessionStore::new(dir.join("account-session.json")),
            dir,
        )
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-30T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn signed_in_state() -> AccountSessionState {
        let mut state = AccountSessionState::new("https://logger.example", now());
        state.status = AccountSessionStatus::SignedIn;
        state.session_token_credential_id = Some(Uuid::new_v4());
        state.refresh_token_credential_id = Some(Uuid::new_v4());
        state
    }

    fn login_body(session_token: &str, refresh_token: &str) -> JsonValue {
        json!({
            "account": {
                "account_id": Uuid::new_v4(),
                "user_id": Uuid::new_v4(),
                "email": "operator@example.com",
                "display_name": "Operator",
                "created_at": "2026-08-01T00:00:00Z",
                "email_verified_at": "2026-08-01T00:05:00Z"
            },
            "session": {
                "session_id": Uuid::new_v4(),
                "account_id": Uuid::new_v4(),
                "user_id": Uuid::new_v4(),
                "device_id": Uuid::new_v4(),
                "token": session_token,
                "token_hash": "hash",
                "issued_at": "2026-08-30T11:59:00Z",
                "expires_at": "2026-09-29T11:59:00Z",
                "active": true
            },
            "device": {
                "device_id": Uuid::new_v4(),
                "account_id": Uuid::new_v4(),
                "user_id": Uuid::new_v4(),
                "device_name": "Shack Desktop",
                "fingerprint": "dev-1",
                "trusted": true,
                "revoked": false,
                "registered_at": "2026-08-30T11:59:00Z",
                "revoked_at": null
            },
            "logbooks": [{
                "logbook_id": Uuid::new_v4(),
                "account_id": Uuid::new_v4(),
                "name": "Home Station",
                "description": null,
                "station_callsign": "KE8YGW",
                "created_at": "2026-08-01T00:00:00Z",
                "updated_at": "2026-08-01T00:00:00Z"
            }],
            "refresh_token": refresh_token,
            "session_cookie": "ham_session=..."
        })
    }

    #[test]
    fn normalize_server_url_accepts_http_and_https_origins() {
        assert_eq!(
            normalize_server_url("https://logger.example/api/v1/").unwrap(),
            "https://logger.example"
        );
        assert_eq!(
            normalize_server_url(" http://127.0.0.1:9750 ").unwrap(),
            "http://127.0.0.1:9750"
        );
    }

    #[test]
    fn normalize_server_url_rejects_unsupported_input() {
        assert!(matches!(
            normalize_server_url(""),
            Err(AccountError::MissingServerUrl)
        ));
        assert!(matches!(
            normalize_server_url("logger.example"),
            Err(AccountError::InvalidServerUrl(_))
        ));
        assert!(matches!(
            normalize_server_url("ftp://logger.example"),
            Err(AccountError::InvalidServerUrl(_))
        ));
        assert!(matches!(
            normalize_server_url("https://user@logger.example"),
            Err(AccountError::InvalidServerUrl(_))
        ));
    }

    #[test]
    fn login_plan_targets_the_hosted_login_route_without_bearer() {
        let state = AccountSessionState::new("https://logger.example", now());
        let plan = plan_account_request(
            &state,
            &AccountAction::Login {
                email: "  Operator@Example.com ".to_owned(),
                display_name: None,
                device_name: Some("Shack Desktop".to_owned()),
            },
        )
        .unwrap();
        assert_eq!(plan.method, "POST");
        assert_eq!(plan.path, ACCOUNT_PATH_LOGIN);
        assert_eq!(plan.url, "https://logger.example/api/v1/auth/login");
        assert!(!plan.requires_bearer);
        assert!(plan.bearer_credential_id.is_none());
        assert_eq!(plan.body.as_ref().unwrap()["email"], "operator@example.com");
        assert_eq!(plan.body.as_ref().unwrap()["device_name"], "Shack Desktop");
        assert!(plan.body.as_ref().unwrap().get("display_name").is_none());
    }

    #[test]
    fn cleartext_hosted_transport_is_limited_to_local_networks() {
        assert!(ensure_transport_is_permitted("http://127.0.0.1:9750/api/v1/status").is_ok());
        assert!(ensure_transport_is_permitted("http://localhost:9750/api/v1/status").is_ok());
        assert!(ensure_transport_is_permitted("http://192.168.1.20:9750/api/v1/status").is_ok());
        assert!(ensure_transport_is_permitted("http://[::1]:9750/api/v1/status").is_ok());
        assert!(ensure_transport_is_permitted("https://logger.example/api/v1/status").is_ok());
        assert!(matches!(
            ensure_transport_is_permitted("http://logger.example/api/v1/status"),
            Err(AccountError::InsecureTransport(_))
        ));
        assert!(matches!(
            ensure_transport_is_permitted("http://203.0.113.10/api/v1/status"),
            Err(AccountError::InsecureTransport(_))
        ));
    }

    #[test]
    fn plans_refuse_cleartext_public_hosted_servers() {
        let state = AccountSessionState::new("http://logger.example", now());
        assert!(matches!(
            plan_account_request(&state, &AccountAction::HostingStatus),
            Err(AccountError::InsecureTransport(_))
        ));
        let loopback = AccountSessionState::new("http://127.0.0.1:9750", now());
        assert!(plan_account_request(&loopback, &AccountAction::HostingStatus).is_ok());
    }

    #[test]
    fn login_plan_rejects_malformed_email() {
        let state = AccountSessionState::new("https://logger.example", now());
        let error = plan_account_request(
            &state,
            &AccountAction::Login {
                email: "operator".to_owned(),
                display_name: None,
                device_name: None,
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            AccountError::MissingField { field: "email" }
        ));
    }

    #[test]
    fn bearer_actions_require_a_stored_session_credential() {
        let state = AccountSessionState::new("https://logger.example", now());
        let error = plan_account_request(&state, &AccountAction::ListDevices).unwrap_err();
        assert!(matches!(
            error,
            AccountError::MissingSessionCredential("list_devices")
        ));

        let signed_in = signed_in_state();
        let plan = plan_account_request(&signed_in, &AccountAction::ListDevices).unwrap();
        assert!(plan.requires_bearer);
        assert_eq!(
            plan.bearer_credential_id,
            signed_in.session_token_credential_id
        );
    }

    #[test]
    fn rotation_plan_references_the_refresh_credential_without_the_secret() {
        let state = signed_in_state();
        let plan = plan_account_request(&state, &AccountAction::RotateSession).unwrap();
        assert_eq!(plan.path, ACCOUNT_PATH_SESSION_ROTATE);
        assert_eq!(plan.body_secret_fields.len(), 1);
        assert_eq!(
            plan.body_secret_fields[0].field,
            ACCOUNT_SECRET_FIELD_REFRESH_TOKEN
        );
        assert_eq!(
            plan.body_secret_fields[0].credential_id,
            state.refresh_token_credential_id.unwrap()
        );
        let serialized = serde_json::to_string(&plan).unwrap();
        assert!(!serialized.contains("refresh-secret"));
    }

    #[test]
    fn rotation_plan_requires_a_refresh_credential() {
        let mut state = signed_in_state();
        state.refresh_token_credential_id = None;
        assert!(matches!(
            plan_account_request(&state, &AccountAction::RotateSession),
            Err(AccountError::MissingRefreshCredential)
        ));
    }

    #[test]
    fn account_delete_plan_requires_confirmation() {
        let state = signed_in_state();
        assert!(matches!(
            plan_account_request(&state, &AccountAction::DeleteAccount { confirm: false }),
            Err(AccountError::ConfirmationRequired)
        ));
        let plan =
            plan_account_request(&state, &AccountAction::DeleteAccount { confirm: true }).unwrap();
        assert_eq!(plan.body.unwrap()["confirm"], true);
    }

    #[test]
    fn revoke_device_plan_uses_the_device_scoped_path() {
        let state = signed_in_state();
        let device_id = Uuid::new_v4();
        let plan =
            plan_account_request(&state, &AccountAction::RevokeDevice { device_id }).unwrap();
        assert_eq!(plan.path, format!("/api/v1/devices/{device_id}/revoke"));
        assert!(plan.requires_bearer);
    }

    #[test]
    fn register_success_moves_state_to_pending_verification() {
        let mut state = AccountSessionState::new("https://logger.example", now());
        let action = AccountAction::Register {
            email: "operator@example.com".to_owned(),
            display_name: None,
            device_name: None,
            invitation_token: Some("invite".to_owned()),
            turnstile_token: None,
        };
        let body = json!({
            "ok": true,
            "account": {
                "account_id": Uuid::new_v4(),
                "user_id": Uuid::new_v4(),
                "email": "operator@example.com",
                "display_name": "operator",
                "created_at": "2026-08-30T11:00:00Z"
            },
            "logbooks": [],
            "email_verification_required": true
        });
        let result = apply_account_response(
            &mut state,
            &action,
            &AccountResponseInput::success(200, body),
            now(),
        );
        assert!(result.record.outcome.succeeded());
        assert_eq!(state.status, AccountSessionStatus::PendingEmailVerification);
        assert!(state.email_verification_required);
        assert_eq!(state.pending_email.as_deref(), Some("operator@example.com"));
        assert!(!result.issued_tokens());
    }

    #[test]
    fn login_success_signs_in_and_returns_tokens_without_persisting_them() {
        let mut state = AccountSessionState::new("https://logger.example", now());
        let action = AccountAction::Login {
            email: "operator@example.com".to_owned(),
            display_name: None,
            device_name: None,
        };
        let body = login_body("session-secret", "refresh-secret");
        let result = apply_account_response(
            &mut state,
            &action,
            &AccountResponseInput::success(200, body),
            now(),
        );
        assert_eq!(state.status, AccountSessionStatus::SignedIn);
        assert_eq!(
            result.issued_session_token.as_deref(),
            Some("session-secret")
        );
        assert_eq!(
            result.issued_refresh_token.as_deref(),
            Some("refresh-secret")
        );
        assert_eq!(state.logbooks.len(), 1);
        assert!(state.session.is_some());

        let serialized = serde_json::to_string(&state).unwrap();
        assert!(!serialized.contains("session-secret"));
        assert!(!serialized.contains("refresh-secret"));
        assert!(!serialized.contains("token_hash"));

        let debug = format!("{result:?}");
        assert!(!debug.contains("session-secret"));
        assert!(!debug.contains("refresh-secret"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn login_email_unverified_moves_state_to_pending_verification() {
        let mut state = AccountSessionState::new("https://logger.example", now());
        let action = AccountAction::Login {
            email: "operator@example.com".to_owned(),
            display_name: None,
            device_name: None,
        };
        let response = AccountResponseInput {
            status: 403,
            body: Some(json!({
                "error": "email is not verified",
                "code": "email_unverified",
                "request_id": "req-1",
                "retryable": false
            })),
            request_id: None,
            transport_error: None,
        };
        let result = apply_account_response(&mut state, &action, &response, now());
        assert_eq!(
            result.record.outcome,
            AccountOutcome::EmailVerificationRequired
        );
        assert_eq!(state.status, AccountSessionStatus::PendingEmailVerification);
        assert_eq!(result.record.request_id.as_deref(), Some("req-1"));
        assert!(result.record.message.contains("email_unverified"));
    }

    #[test]
    fn expired_session_response_clears_credential_references() {
        let mut state = signed_in_state();
        let response = AccountResponseInput {
            status: 401,
            body: Some(json!({
                "error": "session expired",
                "code": "session_expired",
                "request_id": "req-2",
                "retryable": false
            })),
            request_id: None,
            transport_error: None,
        };
        let result =
            apply_account_response(&mut state, &AccountAction::RefreshSession, &response, now());
        assert_eq!(result.record.outcome, AccountOutcome::SessionExpired);
        assert_eq!(state.status, AccountSessionStatus::SessionExpired);
        assert!(result.clear_session_credentials);
        assert!(!state.has_session_credential());
        assert!(state.refresh_token_credential_id.is_none());
    }

    #[test]
    fn revoked_device_response_marks_the_device_revoked_state() {
        let mut state = signed_in_state();
        let response = AccountResponseInput {
            status: 401,
            body: Some(json!({
                "error": "device is revoked",
                "code": "device_revoked",
                "request_id": "req-3",
                "retryable": false
            })),
            request_id: None,
            transport_error: None,
        };
        let result =
            apply_account_response(&mut state, &AccountAction::ListDevices, &response, now());
        assert_eq!(result.record.outcome, AccountOutcome::DeviceRevoked);
        assert_eq!(state.status, AccountSessionStatus::DeviceRevoked);
        assert!(!state.has_session_credential());
    }

    #[test]
    fn a_failed_sign_in_does_not_expire_an_existing_session() {
        let mut state = signed_in_state();
        let response = AccountResponseInput {
            status: 401,
            body: Some(json!({"error": "unauthenticated", "code": "missing_token"})),
            request_id: None,
            transport_error: None,
        };
        let result = apply_account_response(
            &mut state,
            &AccountAction::Login {
                email: "someone-else@example.com".to_owned(),
                display_name: None,
                device_name: None,
            },
            &response,
            now(),
        );
        assert_eq!(result.record.outcome, AccountOutcome::Unauthorized);
        assert_eq!(state.status, AccountSessionStatus::SignedIn);
        assert!(state.has_session_credential());
        assert!(!result.clear_session_credentials);
    }

    #[test]
    fn rate_limited_response_is_retryable_and_keeps_the_session() {
        let mut state = signed_in_state();
        let response = AccountResponseInput {
            status: 429,
            body: Some(json!({
                "error": "rate limit exceeded",
                "code": "rate_limited",
                "request_id": "req-4",
                "retryable": true
            })),
            request_id: None,
            transport_error: None,
        };
        let result = apply_account_response(
            &mut state,
            &AccountAction::RecoveryStart {
                email: "operator@example.com".to_owned(),
            },
            &response,
            now(),
        );
        assert_eq!(result.record.outcome, AccountOutcome::RateLimited);
        assert!(result.record.retryable);
        assert_eq!(state.status, AccountSessionStatus::SignedIn);
        assert!(state.has_session_credential());
    }

    #[test]
    fn transport_failure_is_classified_as_network_unavailable() {
        let mut state = signed_in_state();
        let result = apply_account_response(
            &mut state,
            &AccountAction::RefreshSession,
            &AccountResponseInput::transport_failure("connection refused"),
            now(),
        );
        assert_eq!(result.record.outcome, AccountOutcome::NetworkUnavailable);
        assert!(result.record.retryable);
        assert_eq!(state.status, AccountSessionStatus::SignedIn);
        assert!(state.has_session_credential());
    }

    #[test]
    fn logout_clears_the_session_and_credential_references() {
        let mut state = signed_in_state();
        let result = apply_account_response(
            &mut state,
            &AccountAction::Logout,
            &AccountResponseInput::success(200, json!({"ok": true})),
            now(),
        );
        assert_eq!(state.status, AccountSessionStatus::SignedOut);
        assert!(result.clear_session_credentials);
        assert!(state.session.is_none());
        assert!(!state.has_session_credential());
    }

    #[test]
    fn revoke_all_devices_marks_devices_revoked_and_signs_out() {
        let mut state = signed_in_state();
        state.devices = vec![AccountDevice {
            device_id: Uuid::new_v4(),
            account_id: None,
            user_id: None,
            device_name: "Handheld".to_owned(),
            trusted: true,
            revoked: false,
            registered_at: None,
            revoked_at: None,
        }];
        let result = apply_account_response(
            &mut state,
            &AccountAction::RevokeAllDevices,
            &AccountResponseInput::success(
                200,
                json!({"ok": true, "revoked_devices": 1, "revoked_sessions": 1}),
            ),
            now(),
        );
        assert!(state.devices.iter().all(|device| device.revoked));
        assert_eq!(state.status, AccountSessionStatus::DeviceRevoked);
        assert!(result.clear_session_credentials);
    }

    #[test]
    fn revoking_the_current_device_signs_the_client_out() {
        let mut state = signed_in_state();
        let device_id = Uuid::new_v4();
        state.device = Some(AccountDevice {
            device_id,
            account_id: None,
            user_id: None,
            device_name: "Shack Desktop".to_owned(),
            trusted: true,
            revoked: false,
            registered_at: None,
            revoked_at: None,
        });
        state.devices = vec![state.device.clone().unwrap()];
        apply_account_response(
            &mut state,
            &AccountAction::RevokeDevice { device_id },
            &AccountResponseInput::success(200, json!({"ok": true})),
            now(),
        );
        assert_eq!(state.status, AccountSessionStatus::DeviceRevoked);
        assert!(state.devices[0].revoked);
        assert!(!state.has_session_credential());
    }

    #[test]
    fn account_delete_marks_the_account_deleted_and_clears_cached_scope() {
        let mut state = signed_in_state();
        state.account = Some(AccountProfile {
            account_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            email: "operator@example.com".to_owned(),
            display_name: "Operator".to_owned(),
            created_at: None,
            email_verified_at: None,
            deleted_at: None,
        });
        state.logbooks = vec![AccountLogbookSummary {
            logbook_id: Uuid::new_v4(),
            name: "Home".to_owned(),
            description: None,
            station_callsign: None,
        }];
        apply_account_response(
            &mut state,
            &AccountAction::DeleteAccount { confirm: true },
            &AccountResponseInput::success(200, json!({"ok": true})),
            now(),
        );
        assert_eq!(state.status, AccountSessionStatus::AccountDeleted);
        assert!(state.logbooks.is_empty());
        assert!(state.account.as_ref().unwrap().deleted_at.is_some());
        assert!(!state.has_session_credential());
    }

    #[test]
    fn hosting_status_records_registration_policy_for_the_client() {
        let mut state = AccountSessionState::new("https://logger.example", now());
        let body = json!({
            "ok": true,
            "operation_mode": "public_hosted",
            "registration_mode": "open",
            "bootstrap_admin_completed": true,
            "turnstile": {"required": true, "site_key": "0x-site-key"}
        });
        apply_account_response(
            &mut state,
            &AccountAction::HostingStatus,
            &AccountResponseInput::success(200, body),
            now(),
        );
        assert!(state.hosting.allows_open_registration());
        assert!(!state.hosting.requires_invitation());
        assert!(state.hosting.turnstile_required);
        assert_eq!(
            state.hosting.turnstile_site_key.as_deref(),
            Some("0x-site-key")
        );
        assert_eq!(state.hosting.observed_at, Some(now()));
    }

    #[test]
    fn session_refresh_success_records_memberships_and_device() {
        let mut state = signed_in_state();
        let body = json!({
            "account": {
                "account_id": Uuid::new_v4(),
                "user_id": Uuid::new_v4(),
                "email": "operator@example.com",
                "display_name": "Operator",
                "created_at": "2026-08-01T00:00:00Z",
                "email_verified_at": "2026-08-01T00:05:00Z"
            },
            "session": {
                "session_id": Uuid::new_v4(),
                "account_id": Uuid::new_v4(),
                "user_id": Uuid::new_v4(),
                "device_id": Uuid::new_v4(),
                "issued_at": "2026-08-30T11:59:00Z",
                "expires_at": "2026-09-29T11:59:00Z",
                "active": true
            },
            "device": {
                "device_id": Uuid::new_v4(),
                "device_name": "Shack Desktop",
                "trusted": true,
                "revoked": false
            },
            "memberships": [{
                "account_id": Uuid::new_v4(),
                "logbook_id": Uuid::new_v4(),
                "user_id": Uuid::new_v4(),
                "role": "owner",
                "created_at": "2026-08-01T00:00:00Z"
            }]
        });
        apply_account_response(
            &mut state,
            &AccountAction::RefreshSession,
            &AccountResponseInput::success(200, body),
            now(),
        );
        assert_eq!(state.status, AccountSessionStatus::SignedIn);
        assert_eq!(state.memberships.len(), 1);
        assert_eq!(state.memberships[0].role.as_deref(), Some("owner"));
        assert_eq!(state.device.as_ref().unwrap().device_name, "Shack Desktop");
    }

    #[test]
    fn additive_hosted_response_fields_are_tolerated() {
        let mut state = AccountSessionState::new("https://logger.example", now());
        let mut body = login_body("session-secret", "refresh-secret");
        body["account"]["future_field"] = json!("ignored");
        body["session"]["future_flag"] = json!(true);
        body["future_top_level"] = json!({"nested": 1});
        let result = apply_account_response(
            &mut state,
            &AccountAction::Login {
                email: "operator@example.com".to_owned(),
                display_name: None,
                device_name: None,
            },
            &AccountResponseInput::success(200, body),
            now(),
        );
        assert!(result.record.outcome.succeeded());
        assert_eq!(state.status, AccountSessionStatus::SignedIn);
    }

    #[test]
    fn state_round_trips_through_the_durable_store() {
        let (store, dir) = temp_store();
        let mut state = store
            .load_or_create("https://logger.example", now())
            .unwrap();
        assert_eq!(state.status, AccountSessionStatus::SignedOut);
        state.status = AccountSessionStatus::SignedIn;
        state.record_session_credentials(Some(Uuid::new_v4()), Some(Uuid::new_v4()), now());
        store.save(&state).unwrap();

        let reloaded = store.load().unwrap();
        assert_eq!(reloaded, state);
        let contents = fs::read_to_string(store.path()).unwrap();
        assert!(contents.contains("\"version\": 1"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unsupported_file_versions_are_rejected_and_quarantined() {
        let (store, dir) = temp_store();
        let state = store
            .load_or_create("https://logger.example", now())
            .unwrap();
        let mut raw: JsonValue =
            serde_json::from_str(&fs::read_to_string(store.path()).unwrap()).unwrap();
        raw["version"] = json!(99);
        fs::write(store.path(), serde_json::to_string_pretty(&raw).unwrap()).unwrap();

        assert!(matches!(
            store.load(),
            Err(AccountError::UnsupportedFileVersion(99))
        ));

        let recovered = store
            .load_or_create("https://logger.example", now())
            .unwrap();
        assert_eq!(recovered.status, state.status);
        let quarantined = fs::read_dir(store.path().parent().unwrap())
            .unwrap()
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains("corrupt-"));
        assert!(quarantined);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn corrupt_state_files_are_quarantined_instead_of_deleted() {
        let (store, dir) = temp_store();
        store
            .load_or_create("https://logger.example", now())
            .unwrap();
        fs::write(store.path(), b"{ not json").unwrap();
        let recovered = store
            .load_or_create("https://logger.example", now())
            .unwrap();
        assert_eq!(recovered.status, AccountSessionStatus::SignedOut);
        assert_eq!(recovered.server_url, "https://logger.example");
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn action_kinds_expose_stable_runtime_event_types() {
        assert_eq!(
            AccountActionKind::Login.runtime_event_type(),
            "account.login"
        );
        assert_eq!(
            AccountActionKind::RevokeAllDevices.runtime_event_type(),
            "account.devices.revoke_all"
        );
        assert_eq!(AccountActionKind::Login.as_str(), "login");
    }

    #[test]
    fn long_server_messages_are_truncated_for_support_state() {
        let mut state = AccountSessionState::new("https://logger.example", now());
        let long_message = "x".repeat(MAX_ACCOUNT_MESSAGE_BYTES * 2);
        let response = AccountResponseInput {
            status: 400,
            body: Some(json!({"error": long_message, "code": "bad_request"})),
            request_id: None,
            transport_error: None,
        };
        let result = apply_account_response(
            &mut state,
            &AccountAction::VerifyEmail {
                token: "token".to_owned(),
            },
            &response,
            now(),
        );
        assert!(result.record.message.len() <= MAX_ACCOUNT_MESSAGE_BYTES);
        assert_eq!(result.record.outcome, AccountOutcome::InvalidRequest);
    }
}
