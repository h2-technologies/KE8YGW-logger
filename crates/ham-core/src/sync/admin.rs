//! Shared hosted server administration client contract.
//!
//! Every client surface (hosted web, desktop, native iOS, and the CLI) drives
//! the hosted `/api/v1/admin/*` routes through this module. Rust owns request
//! planning, response interpretation, outcome classification, and the durable
//! non-secret administration support record. Platform layers only carry bytes
//! over their own transport.
//!
//! Administration is session-scoped rather than separately configured. The
//! endpoint and the session credential both come from the hosted account
//! record in [`crate::sync::account`], so an operator administers exactly the server
//! they are signed in to and there is no second endpoint setting to drift.
//!
//! Invitation tokens are secrets. They are issued by the hosted server exactly
//! once, returned to the caller exactly once in [`HostedAdminResult`], and are
//! excluded from every serialized form of the result and the snapshot. They are
//! never written to the support record or to platform secure storage: the
//! operator hands the token to the invitee, and the hosted server also emails
//! it.

use std::{
    fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    str::FromStr,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value as JsonValue};
use thiserror::Error;
use uuid::Uuid;

use crate::sync::account::{
    classify_hosted_account_error, normalize_base_url, normalize_email, HostedAccountError,
    HostedAccountOutcome, HostedAccountSecrets, HostedAccountSnapshot,
};
use crate::sync::offline::{quarantine_file, write_json_atomically};

/// Support-file schema version for the durable hosted administration record.
pub const HOSTED_ADMIN_FILE_VERSION: u32 = 1;
/// Default per-request timeout for hosted administration calls.
pub const DEFAULT_HOSTED_ADMIN_TIMEOUT_SECONDS: u64 = 20;
/// Maximum accepted hosted administration response size.
///
/// Audit listings are larger than account responses, so this bound is higher
/// than [`crate::sync::account::MAX_HOSTED_ACCOUNT_RESPONSE_BYTES`].
pub const MAX_HOSTED_ADMIN_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
/// Maximum invitations retained in the durable support record.
pub const MAX_RETAINED_ADMIN_INVITATIONS: usize = 500;
/// Maximum audit records retained in the durable support record.
pub const MAX_RETAINED_ADMIN_AUDITS: usize = 500;
/// Stable action names shared by every client surface.
pub const ADMIN_ACTION_HOSTING_READ: &str = "admin.hosting.read";
pub const ADMIN_ACTION_HOSTING_UPDATE: &str = "admin.hosting.update";
pub const ADMIN_ACTION_INVITATION_LIST: &str = "admin.invitation.list";
pub const ADMIN_ACTION_INVITATION_CREATE: &str = "admin.invitation.create";
pub const ADMIN_ACTION_INVITATION_GET: &str = "admin.invitation.get";
pub const ADMIN_ACTION_INVITATION_RESEND: &str = "admin.invitation.resend";
pub const ADMIN_ACTION_INVITATION_EXPIRE: &str = "admin.invitation.expire";
pub const ADMIN_ACTION_INVITATION_REVOKE: &str = "admin.invitation.revoke";
pub const ADMIN_ACTION_AUDIT_LIST: &str = "admin.audit.list";

#[derive(Debug, Error)]
pub enum HostedAdminError {
    #[error("hosted admin I/O error: {0}")]
    Io(#[from] io::Error),
    #[error("hosted admin serialization error: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("hosted admin file version {0} is not supported")]
    UnsupportedFileVersion(u32),
    #[error("hosted base URL is invalid: {0}")]
    InvalidBaseUrl(String),
    #[error("email address is invalid")]
    InvalidEmail,
    #[error("field `{field}` is longer than {max} bytes")]
    FieldTooLong { field: &'static str, max: usize },
    #[error("a signed-in hosted session is required")]
    SessionRequired,
    #[error("`{value}` is not a recognized {field}")]
    InvalidValue { field: &'static str, value: String },
    #[error("a hosting configuration update must change at least one field")]
    EmptyHostingUpdate,
    #[error("field `{field}` must be a positive number of seconds")]
    NonPositiveSeconds { field: &'static str },
    #[error("hosted admin secret storage error: {0}")]
    SecretStorage(String),
    #[error("{0}")]
    Account(String),
}

impl From<HostedAccountError> for HostedAdminError {
    fn from(error: HostedAccountError) -> Self {
        match error {
            HostedAccountError::InvalidBaseUrl(value) => Self::InvalidBaseUrl(value),
            HostedAccountError::InvalidEmail => Self::InvalidEmail,
            HostedAccountError::FieldTooLong { field, max } => Self::FieldTooLong { field, max },
            HostedAccountError::SessionRequired => Self::SessionRequired,
            HostedAccountError::SecretStorage(message) => Self::SecretStorage(message),
            other => Self::Account(other.to_string()),
        }
    }
}

/// Hosting operation mode reported and set by the hosted server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostedAdminOperationMode {
    PersonalHosted,
    PublicHosted,
    SelfHosted,
}

impl HostedAdminOperationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PersonalHosted => "personal_hosted",
            Self::PublicHosted => "public_hosted",
            Self::SelfHosted => "self_hosted",
        }
    }
}

impl FromStr for HostedAdminOperationMode {
    type Err = HostedAdminError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "personal_hosted" => Ok(Self::PersonalHosted),
            "public_hosted" => Ok(Self::PublicHosted),
            "self_hosted" => Ok(Self::SelfHosted),
            other => Err(HostedAdminError::InvalidValue {
                field: "operation_mode",
                value: other.to_owned(),
            }),
        }
    }
}

/// Registration mode reported and set by the hosted server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostedAdminRegistrationMode {
    InviteOnly,
    Open,
    Disabled,
}

impl HostedAdminRegistrationMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::InviteOnly => "invite_only",
            Self::Open => "open",
            Self::Disabled => "disabled",
        }
    }
}

impl FromStr for HostedAdminRegistrationMode {
    type Err = HostedAdminError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "invite_only" => Ok(Self::InviteOnly),
            "open" => Ok(Self::Open),
            "disabled" => Ok(Self::Disabled),
            other => Err(HostedAdminError::InvalidValue {
                field: "registration_mode",
                value: other.to_owned(),
            }),
        }
    }
}

/// Logbook role granted by an invitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostedAdminLogbookRole {
    Owner,
    Admin,
    Operator,
    Viewer,
}

impl HostedAdminLogbookRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Admin => "admin",
            Self::Operator => "operator",
            Self::Viewer => "viewer",
        }
    }
}

impl FromStr for HostedAdminLogbookRole {
    type Err = HostedAdminError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim() {
            "owner" => Ok(Self::Owner),
            "admin" => Ok(Self::Admin),
            "operator" => Ok(Self::Operator),
            "viewer" => Ok(Self::Viewer),
            other => Err(HostedAdminError::InvalidValue {
                field: "role",
                value: other.to_owned(),
            }),
        }
    }
}

/// Lifecycle state of one invitation, derived rather than stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostedAdminInvitationStatus {
    Pending,
    Accepted,
    Revoked,
    Expired,
}

impl HostedAdminInvitationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Revoked => "revoked",
            Self::Expired => "expired",
        }
    }

    /// Whether resending is possible for this state.
    ///
    /// The hosted server rejects a resend for an accepted or revoked
    /// invitation; an expired one can still be resent with a fresh token.
    pub fn can_resend(self) -> bool {
        matches!(self, Self::Pending | Self::Expired)
    }
}

/// Non-secret email delivery configuration reported by the hosted server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedAdminEmailConfig {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub from_address: Option<String>,
    #[serde(default)]
    pub verification_base_url: Option<String>,
    #[serde(default)]
    pub recovery_base_url: Option<String>,
    #[serde(default)]
    pub webhook_configured: bool,
    #[serde(default)]
    pub credential_reference_configured: bool,
}

/// Non-secret Turnstile configuration reported by the hosted server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedAdminTurnstileConfig {
    #[serde(default)]
    pub enabled_for_open_registration: bool,
    #[serde(default)]
    pub site_key: Option<String>,
    #[serde(default)]
    pub secret_configured: bool,
    #[serde(default)]
    pub siteverify_url: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<i64>,
}

/// Hosting configuration as reported by the hosted server.
///
/// The hosted server redacts the Turnstile secret and the email credential
/// before responding; this record mirrors that redaction and never carries a
/// secret value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedAdminHostingConfig {
    #[serde(default)]
    pub operation_mode: Option<HostedAdminOperationMode>,
    #[serde(default)]
    pub registration_mode: Option<HostedAdminRegistrationMode>,
    #[serde(default)]
    pub bootstrap_admin_completed: bool,
    #[serde(default)]
    pub session_ttl_seconds: Option<i64>,
    #[serde(default)]
    pub refresh_ttl_seconds: Option<i64>,
    #[serde(default)]
    pub invitation_ttl_seconds: Option<i64>,
    #[serde(default)]
    pub verification_ttl_seconds: Option<i64>,
    #[serde(default)]
    pub recovery_ttl_seconds: Option<i64>,
    #[serde(default)]
    pub limits: Option<JsonValue>,
    #[serde(default)]
    pub email: Option<HostedAdminEmailConfig>,
    #[serde(default)]
    pub turnstile: Option<HostedAdminTurnstileConfig>,
    #[serde(default)]
    pub updated_at: Option<DateTime<Utc>>,
}

/// One invitation as reported by the hosted server.
///
/// This mirrors the hosted sanitized invitation, which never includes the
/// invitation token or its hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedAdminInvitation {
    pub invite_id: Uuid,
    #[serde(default)]
    pub account_id: Option<Uuid>,
    #[serde(default)]
    pub logbook_id: Option<Uuid>,
    #[serde(default)]
    pub invited_email: Option<String>,
    #[serde(default)]
    pub role: Option<HostedAdminLogbookRole>,
    #[serde(default)]
    pub created_by_user_id: Option<Uuid>,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub accepted_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub revoked_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_sent_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub resend_count: u32,
}

impl HostedAdminInvitation {
    /// Lifecycle state at `now`.
    ///
    /// Acceptance and revocation are terminal and are reported ahead of
    /// expiry, matching how the hosted server rejects reuse.
    pub fn status(&self, now: DateTime<Utc>) -> HostedAdminInvitationStatus {
        if self.accepted_at.is_some() {
            return HostedAdminInvitationStatus::Accepted;
        }
        if self.revoked_at.is_some() {
            return HostedAdminInvitationStatus::Revoked;
        }
        match self.expires_at {
            Some(expires_at) if expires_at <= now => HostedAdminInvitationStatus::Expired,
            _ => HostedAdminInvitationStatus::Pending,
        }
    }
}

/// One hosted audit record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedAdminAuditRecord {
    pub audit_id: Uuid,
    #[serde(default)]
    pub occurred_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub request_id: Option<String>,
    #[serde(default)]
    pub actor_account_id: Option<Uuid>,
    #[serde(default)]
    pub actor_user_id: Option<Uuid>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub outcome: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub details: Map<String, JsonValue>,
}

/// Redacted hosted administration state persisted in the support store.
///
/// This record never contains an invitation token, a session token, a
/// Turnstile secret, or an email credential.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedAdminSnapshot {
    pub schema_version: u32,
    pub base_url: String,
    /// Whether the signed-in account was accepted as an instance administrator
    /// by the most recent administration call. `None` means no administration
    /// call has been made yet from this record.
    #[serde(default)]
    pub administrator: Option<bool>,
    #[serde(default)]
    pub hosting: Option<HostedAdminHostingConfig>,
    #[serde(default)]
    pub invitations: Vec<HostedAdminInvitation>,
    #[serde(default)]
    pub audits: Vec<HostedAdminAuditRecord>,
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

impl HostedAdminSnapshot {
    pub fn new(base_url: &str) -> Self {
        Self {
            schema_version: HOSTED_ADMIN_FILE_VERSION,
            base_url: base_url.to_owned(),
            administrator: None,
            hosting: None,
            invitations: Vec::new(),
            audits: Vec::new(),
            last_action: None,
            last_outcome: None,
            last_error_code: None,
            last_message: None,
            last_request_id: None,
            last_updated_at: None,
        }
    }

    /// Whether the most recent administration call proved administrator rights.
    pub fn is_administrator(&self) -> bool {
        self.administrator == Some(true)
    }

    /// Invitations in the given lifecycle state at `now`.
    pub fn invitations_with_status(
        &self,
        status: HostedAdminInvitationStatus,
        now: DateTime<Utc>,
    ) -> Vec<&HostedAdminInvitation> {
        self.invitations
            .iter()
            .filter(|invitation| invitation.status(now) == status)
            .collect()
    }

    /// Clears cached server state when the endpoint changes or rights are lost.
    fn clear_server_state(&mut self) {
        self.hosting = None;
        self.invitations.clear();
        self.audits.clear();
    }
}

/// A hosting configuration patch.
///
/// Only the fields an operator actually set are sent, so an update never
/// rewrites a hosting value the operator did not look at.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedAdminHostingUpdate {
    #[serde(default)]
    pub operation_mode: Option<HostedAdminOperationMode>,
    #[serde(default)]
    pub registration_mode: Option<HostedAdminRegistrationMode>,
    #[serde(default)]
    pub session_ttl_seconds: Option<i64>,
    #[serde(default)]
    pub refresh_ttl_seconds: Option<i64>,
    #[serde(default)]
    pub invitation_ttl_seconds: Option<i64>,
    #[serde(default)]
    pub verification_ttl_seconds: Option<i64>,
    #[serde(default)]
    pub recovery_ttl_seconds: Option<i64>,
}

impl HostedAdminHostingUpdate {
    pub fn is_empty(&self) -> bool {
        self.operation_mode.is_none()
            && self.registration_mode.is_none()
            && self.session_ttl_seconds.is_none()
            && self.refresh_ttl_seconds.is_none()
            && self.invitation_ttl_seconds.is_none()
            && self.verification_ttl_seconds.is_none()
            && self.recovery_ttl_seconds.is_none()
    }

    /// Builds the hosted patch body, rejecting non-positive lifetimes before a
    /// request is sent.
    pub fn to_body(&self) -> Result<JsonValue, HostedAdminError> {
        if self.is_empty() {
            return Err(HostedAdminError::EmptyHostingUpdate);
        }
        let mut body = Map::new();
        if let Some(mode) = self.operation_mode {
            body.insert("operation_mode".to_owned(), json!(mode));
        }
        if let Some(mode) = self.registration_mode {
            body.insert("registration_mode".to_owned(), json!(mode));
        }
        for (field, value) in [
            ("session_ttl_seconds", self.session_ttl_seconds),
            ("refresh_ttl_seconds", self.refresh_ttl_seconds),
            ("invitation_ttl_seconds", self.invitation_ttl_seconds),
            ("verification_ttl_seconds", self.verification_ttl_seconds),
            ("recovery_ttl_seconds", self.recovery_ttl_seconds),
        ] {
            let Some(value) = value else {
                continue;
            };
            if value <= 0 {
                return Err(HostedAdminError::NonPositiveSeconds { field });
            }
            body.insert(field.to_owned(), json!(value));
        }
        Ok(JsonValue::Object(body))
    }
}

/// Hosted administration operations available to every client surface.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum HostedAdminAction {
    HostingRead,
    HostingUpdate {
        update: HostedAdminHostingUpdate,
    },
    InvitationList,
    InvitationCreate {
        logbook_id: Uuid,
        email: String,
        role: HostedAdminLogbookRole,
        #[serde(default)]
        expires_at: Option<DateTime<Utc>>,
    },
    InvitationGet {
        invite_id: Uuid,
    },
    InvitationResend {
        invite_id: Uuid,
    },
    InvitationExpire {
        invite_id: Uuid,
        #[serde(default)]
        expires_at: Option<DateTime<Utc>>,
    },
    InvitationRevoke {
        invite_id: Uuid,
    },
    AuditList,
}

impl HostedAdminAction {
    pub fn name(&self) -> &'static str {
        match self {
            Self::HostingRead => ADMIN_ACTION_HOSTING_READ,
            Self::HostingUpdate { .. } => ADMIN_ACTION_HOSTING_UPDATE,
            Self::InvitationList => ADMIN_ACTION_INVITATION_LIST,
            Self::InvitationCreate { .. } => ADMIN_ACTION_INVITATION_CREATE,
            Self::InvitationGet { .. } => ADMIN_ACTION_INVITATION_GET,
            Self::InvitationResend { .. } => ADMIN_ACTION_INVITATION_RESEND,
            Self::InvitationExpire { .. } => ADMIN_ACTION_INVITATION_EXPIRE,
            Self::InvitationRevoke { .. } => ADMIN_ACTION_INVITATION_REVOKE,
            Self::AuditList => ADMIN_ACTION_AUDIT_LIST,
        }
    }

    /// Whether the action changes hosted server state.
    ///
    /// Read-only actions are safe to run on a schedule or a screen refresh;
    /// mutating ones must be operator-initiated.
    pub fn is_mutating(&self) -> bool {
        !matches!(
            self,
            Self::HostingRead | Self::InvitationList | Self::InvitationGet { .. } | Self::AuditList
        )
    }

    /// Whether the hosted response carries a single-use invitation token.
    pub fn issues_invitation_token(&self) -> bool {
        matches!(
            self,
            Self::InvitationCreate { .. } | Self::InvitationResend { .. }
        )
    }
}

/// A hosted request the platform transport must execute verbatim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedAdminRequestPlan {
    pub action: String,
    pub method: String,
    pub path: String,
    pub url: String,
    #[serde(default)]
    pub body: Option<JsonValue>,
    pub session_token_credential_id: Uuid,
    pub request_id: Uuid,
    pub timeout_seconds: u64,
    pub max_response_bytes: usize,
}

/// A hosted response as observed by the platform transport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostedAdminHttpResponse {
    pub status: u16,
    #[serde(default)]
    pub body: Option<JsonValue>,
}

impl HostedAdminHttpResponse {
    pub fn new(status: u16, body: Option<JsonValue>) -> Self {
        Self { status, body }
    }
}

/// Outcome of one hosted administration operation.
///
/// `invitation_token` is deliberately excluded from every serialized form.
/// Callers must show it to the operator once and must not persist it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HostedAdminResult {
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
    pub snapshot: HostedAdminSnapshot,
    /// The invitation this call created or changed, when the action targets one.
    #[serde(default)]
    pub invitation: Option<HostedAdminInvitation>,
    #[serde(skip)]
    invitation_token: Option<String>,
}

impl HostedAdminResult {
    /// Single-use invitation token issued by this call, if any.
    pub fn invitation_token(&self) -> Option<&str> {
        self.invitation_token.as_deref()
    }

    /// Moves the issued invitation token out so callers cannot copy it twice.
    pub fn take_invitation_token(&mut self) -> Option<String> {
        self.invitation_token.take()
    }
}

/// Platform transport for hosted administration requests.
pub trait HostedAdminTransport {
    fn execute(
        &self,
        plan: &HostedAdminRequestPlan,
        body: Option<&JsonValue>,
        session_token: &str,
    ) -> Result<HostedAdminHttpResponse, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HostedAdminFile {
    version: u32,
    snapshot: HostedAdminSnapshot,
}

impl HostedAdminFile {
    fn validate(&self) -> Result<(), HostedAdminError> {
        if self.version != HOSTED_ADMIN_FILE_VERSION {
            return Err(HostedAdminError::UnsupportedFileVersion(self.version));
        }
        if self.snapshot.schema_version != HOSTED_ADMIN_FILE_VERSION {
            return Err(HostedAdminError::UnsupportedFileVersion(
                self.snapshot.schema_version,
            ));
        }
        normalize_base_url(&self.snapshot.base_url)?;
        Ok(())
    }
}

/// Durable JSON support store for the hosted administration record.
#[derive(Debug, Clone)]
pub struct JsonHostedAdminStore {
    path: PathBuf,
}

impl JsonHostedAdminStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Loads the record, creating it when absent and quarantining it when the
    /// stored JSON cannot be read as a supported record.
    ///
    /// A record written against a different hosted endpoint is reset rather
    /// than reused, so cached hosting, invitation, and audit state can never be
    /// shown for the wrong server.
    pub fn load_or_initialize(
        &self,
        base_url: &str,
        now: DateTime<Utc>,
    ) -> Result<HostedAdminSnapshot, HostedAdminError> {
        let base_url = normalize_base_url(base_url)?;
        match self.load_file() {
            Ok(file) => {
                let mut snapshot = file.snapshot;
                if snapshot.base_url != base_url {
                    snapshot.base_url = base_url;
                    snapshot.administrator = None;
                    snapshot.clear_server_state();
                    self.save(&snapshot)?;
                }
                Ok(snapshot)
            }
            Err(HostedAdminError::Io(error)) if error.kind() == ErrorKind::NotFound => {
                let snapshot = HostedAdminSnapshot::new(&base_url);
                self.save(&snapshot)?;
                Ok(snapshot)
            }
            Err(_) => {
                quarantine_file(&self.path, now)?;
                let snapshot = HostedAdminSnapshot::new(&base_url);
                self.save(&snapshot)?;
                Ok(snapshot)
            }
        }
    }

    pub fn snapshot(&self) -> Result<HostedAdminSnapshot, HostedAdminError> {
        Ok(self.load_file()?.snapshot)
    }

    pub fn save(&self, snapshot: &HostedAdminSnapshot) -> Result<(), HostedAdminError> {
        let file = HostedAdminFile {
            version: HOSTED_ADMIN_FILE_VERSION,
            snapshot: snapshot.clone(),
        };
        write_json_atomically(&self.path, &file)?;
        Ok(())
    }

    fn load_file(&self) -> Result<HostedAdminFile, HostedAdminError> {
        let contents = fs::read_to_string(&self.path)?;
        let file: HostedAdminFile = serde_json::from_str(&contents)?;
        file.validate()?;
        Ok(file)
    }
}

/// Hosted administration client shared by desktop, hosted web, and CLI surfaces.
#[derive(Debug, Clone)]
pub struct HostedAdminClient {
    store: JsonHostedAdminStore,
}

impl HostedAdminClient {
    pub fn new(store: JsonHostedAdminStore) -> Self {
        Self { store }
    }

    pub fn store(&self) -> &JsonHostedAdminStore {
        &self.store
    }

    /// Reads the durable record for the server the account is signed in to.
    pub fn snapshot(
        &self,
        account: &HostedAccountSnapshot,
        now: DateTime<Utc>,
    ) -> Result<HostedAdminSnapshot, HostedAdminError> {
        self.store.load_or_initialize(&account.base_url, now)
    }

    /// Plans, executes, interprets, and persists one administration action.
    pub fn execute<T, S>(
        &self,
        action: &HostedAdminAction,
        account: &HostedAccountSnapshot,
        transport: &T,
        secrets: &mut S,
        now: DateTime<Utc>,
    ) -> Result<HostedAdminResult, HostedAdminError>
    where
        T: HostedAdminTransport + ?Sized,
        S: HostedAccountSecrets + ?Sized,
    {
        let snapshot = self.store.load_or_initialize(&account.base_url, now)?;
        let plan = plan_hosted_admin_request(action, account, now)?;
        let session_token = secrets
            .read_secret(plan.session_token_credential_id)?
            .ok_or(HostedAdminError::SessionRequired)?;

        let result = match transport.execute(&plan, plan.body.as_ref(), &session_token) {
            Ok(response) => apply_hosted_admin_response(action, &plan, &response, snapshot, now),
            Err(error) => hosted_admin_transport_failure(action, &plan, &error, snapshot, now),
        };

        self.store.save(&result.snapshot)?;
        Ok(result)
    }
}

/// Builds the hosted request for one administration action.
///
/// The endpoint and the session credential both come from the hosted account
/// record, so administration always targets the signed-in server.
pub fn plan_hosted_admin_request(
    action: &HostedAdminAction,
    account: &HostedAccountSnapshot,
    _now: DateTime<Utc>,
) -> Result<HostedAdminRequestPlan, HostedAdminError> {
    let base_url = normalize_base_url(&account.base_url)?;
    let session_token_credential_id = account
        .session_token_credential_id
        .ok_or(HostedAdminError::SessionRequired)?;

    let (method, path, body) = match action {
        HostedAdminAction::HostingRead => ("GET", "/api/v1/admin/hosting".to_owned(), None),
        HostedAdminAction::HostingUpdate { update } => (
            "PATCH",
            "/api/v1/admin/hosting".to_owned(),
            Some(update.to_body()?),
        ),
        HostedAdminAction::InvitationList => ("GET", "/api/v1/admin/invitations".to_owned(), None),
        HostedAdminAction::InvitationCreate {
            logbook_id,
            email,
            role,
            expires_at,
        } => {
            let email = normalize_email(email)?;
            let mut body = json!({
                "logbook_id": logbook_id,
                "email": email,
                "role": role,
            });
            if let (Some(expires_at), Some(object)) = (expires_at, body.as_object_mut()) {
                object.insert("expires_at".to_owned(), json!(expires_at));
            }
            ("POST", "/api/v1/admin/invitations".to_owned(), Some(body))
        }
        HostedAdminAction::InvitationGet { invite_id } => (
            "GET",
            format!("/api/v1/admin/invitations/{invite_id}"),
            None,
        ),
        HostedAdminAction::InvitationResend { invite_id } => (
            "POST",
            format!("/api/v1/admin/invitations/{invite_id}/resend"),
            Some(json!({})),
        ),
        HostedAdminAction::InvitationExpire {
            invite_id,
            expires_at,
        } => {
            let mut body = json!({});
            if let (Some(expires_at), Some(object)) = (expires_at, body.as_object_mut()) {
                object.insert("expires_at".to_owned(), json!(expires_at));
            }
            (
                "POST",
                format!("/api/v1/admin/invitations/{invite_id}/expire"),
                Some(body),
            )
        }
        HostedAdminAction::InvitationRevoke { invite_id } => (
            "POST",
            format!("/api/v1/admin/invitations/{invite_id}/revoke"),
            Some(json!({})),
        ),
        HostedAdminAction::AuditList => ("GET", "/api/v1/admin/audits".to_owned(), None),
    };

    Ok(HostedAdminRequestPlan {
        action: action.name().to_owned(),
        method: method.to_owned(),
        url: format!("{base_url}{path}"),
        path,
        body,
        session_token_credential_id,
        request_id: Uuid::new_v4(),
        timeout_seconds: DEFAULT_HOSTED_ADMIN_TIMEOUT_SECONDS,
        max_response_bytes: MAX_HOSTED_ADMIN_RESPONSE_BYTES,
    })
}

/// Interprets a hosted response and returns the next durable snapshot.
pub fn apply_hosted_admin_response(
    action: &HostedAdminAction,
    plan: &HostedAdminRequestPlan,
    response: &HostedAdminHttpResponse,
    snapshot: HostedAdminSnapshot,
    now: DateTime<Utc>,
) -> HostedAdminResult {
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
    let outcome = classify_hosted_account_error(error_code.as_deref(), response.status);

    let mut snapshot = snapshot;
    // `forbidden` on an administration route means the signed-in account is
    // authenticated but is not an instance administrator. That is an operator
    // problem rather than a client defect, so the cached administration state
    // is dropped and the record says so plainly.
    let forbidden = error_code.as_deref() == Some("forbidden") || response.status == 403;
    if forbidden {
        snapshot.administrator = Some(false);
        snapshot.clear_server_state();
    }
    if outcome == HostedAccountOutcome::AuthenticationRequired {
        snapshot.administrator = None;
        snapshot.clear_server_state();
    }

    let message = if forbidden {
        "The signed-in account is not a server administrator.".to_owned()
    } else {
        body.get("error")
            .and_then(JsonValue::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| {
                format!(
                    "hosted administration request failed with status {}",
                    response.status
                )
            })
    };

    finish_result(
        action,
        response.status,
        outcome,
        message,
        error_code,
        request_id.or_else(|| Some(plan.request_id.to_string())),
        snapshot,
        None,
        None,
        now,
    )
}

/// Classifies a transport-level failure without contacting the hosted server.
pub fn hosted_admin_transport_failure(
    action: &HostedAdminAction,
    plan: &HostedAdminRequestPlan,
    message: &str,
    snapshot: HostedAdminSnapshot,
    now: DateTime<Utc>,
) -> HostedAdminResult {
    finish_result(
        action,
        0,
        HostedAccountOutcome::TransientFailure,
        format!("hosted administration transport failure: {message}"),
        Some("transport_failure".to_owned()),
        Some(plan.request_id.to_string()),
        snapshot,
        None,
        None,
        now,
    )
}

fn accepted_result(
    action: &HostedAdminAction,
    plan: &HostedAdminRequestPlan,
    status: u16,
    body: &JsonValue,
    snapshot: HostedAdminSnapshot,
    now: DateTime<Utc>,
) -> HostedAdminResult {
    let mut snapshot = snapshot;
    // Reaching an accepted administration response proves instance-admin
    // rights, because every one of these routes is admin-gated server-side.
    snapshot.administrator = Some(true);
    let mut invitation = None;
    let mut invitation_token = None;
    let message;

    match action {
        HostedAdminAction::HostingRead | HostedAdminAction::HostingUpdate { .. } => {
            if let Some(hosting) = parse_hosting(body) {
                snapshot.hosting = Some(hosting);
            }
            message = match action {
                HostedAdminAction::HostingUpdate { .. } => {
                    "Hosting configuration updated.".to_owned()
                }
                _ => "Hosting configuration loaded.".to_owned(),
            };
        }
        HostedAdminAction::InvitationList => {
            let mut invitations = parse_invitations(body);
            invitations.sort_by_key(|invitation| std::cmp::Reverse(invitation.created_at));
            invitations.truncate(MAX_RETAINED_ADMIN_INVITATIONS);
            message = format!("Loaded {} invitations.", invitations.len());
            snapshot.invitations = invitations;
        }
        HostedAdminAction::InvitationCreate { .. }
        | HostedAdminAction::InvitationGet { .. }
        | HostedAdminAction::InvitationResend { .. }
        | HostedAdminAction::InvitationExpire { .. }
        | HostedAdminAction::InvitationRevoke { .. } => {
            invitation = body.get("invitation").and_then(|value| {
                serde_json::from_value::<HostedAdminInvitation>(value.clone()).ok()
            });
            if let Some(invitation) = invitation.clone() {
                upsert_invitation(&mut snapshot.invitations, invitation);
            }
            if action.issues_invitation_token() {
                invitation_token = body
                    .get("invitation_token")
                    .and_then(JsonValue::as_str)
                    .map(str::to_owned);
            }
            message = match action {
                HostedAdminAction::InvitationCreate { .. } => {
                    "Invitation created. The single-use token is shown once.".to_owned()
                }
                HostedAdminAction::InvitationResend { .. } => {
                    "Invitation resent with a new single-use token.".to_owned()
                }
                HostedAdminAction::InvitationExpire { .. } => "Invitation expired.".to_owned(),
                HostedAdminAction::InvitationRevoke { .. } => "Invitation revoked.".to_owned(),
                _ => "Invitation loaded.".to_owned(),
            };
        }
        HostedAdminAction::AuditList => {
            let mut audits = parse_audits(body);
            audits.sort_by_key(|audit| std::cmp::Reverse(audit.occurred_at));
            audits.truncate(MAX_RETAINED_ADMIN_AUDITS);
            message = format!("Loaded {} audit records.", audits.len());
            snapshot.audits = audits;
        }
    }

    let request_id = body
        .get("request_id")
        .and_then(JsonValue::as_str)
        .map(str::to_owned)
        .or_else(|| Some(plan.request_id.to_string()));

    finish_result(
        action,
        status,
        HostedAccountOutcome::Accepted,
        message,
        None,
        request_id,
        snapshot,
        invitation,
        invitation_token,
        now,
    )
}

#[allow(clippy::too_many_arguments)]
fn finish_result(
    action: &HostedAdminAction,
    status: u16,
    outcome: HostedAccountOutcome,
    message: String,
    error_code: Option<String>,
    request_id: Option<String>,
    snapshot: HostedAdminSnapshot,
    invitation: Option<HostedAdminInvitation>,
    invitation_token: Option<String>,
    now: DateTime<Utc>,
) -> HostedAdminResult {
    let mut snapshot = snapshot;
    snapshot.schema_version = HOSTED_ADMIN_FILE_VERSION;
    snapshot.last_action = Some(action.name().to_owned());
    snapshot.last_outcome = Some(outcome);
    snapshot.last_error_code = error_code.clone();
    snapshot.last_message = Some(message.clone());
    snapshot.last_request_id = request_id.clone();
    snapshot.last_updated_at = Some(now);

    HostedAdminResult {
        action: action.name().to_owned(),
        outcome,
        status,
        message,
        error_code,
        request_id,
        retryable: outcome.is_retryable(),
        user_action_required: outcome.requires_user_action(),
        snapshot,
        invitation,
        invitation_token,
    }
}

fn parse_hosting(body: &JsonValue) -> Option<HostedAdminHostingConfig> {
    let hosting = body.get("hosting")?;
    serde_json::from_value::<HostedAdminHostingConfig>(hosting.clone()).ok()
}

fn parse_invitations(body: &JsonValue) -> Vec<HostedAdminInvitation> {
    body.get("invitations")
        .and_then(JsonValue::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    serde_json::from_value::<HostedAdminInvitation>(value.clone()).ok()
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_audits(body: &JsonValue) -> Vec<HostedAdminAuditRecord> {
    body.get("audits")
        .and_then(JsonValue::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    serde_json::from_value::<HostedAdminAuditRecord>(value.clone()).ok()
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Replaces the cached copy of one invitation, or inserts it when new.
fn upsert_invitation(
    invitations: &mut Vec<HostedAdminInvitation>,
    invitation: HostedAdminInvitation,
) {
    match invitations
        .iter_mut()
        .find(|existing| existing.invite_id == invitation.invite_id)
    {
        Some(existing) => *existing = invitation,
        None => {
            invitations.insert(0, invitation);
            invitations.truncate(MAX_RETAINED_ADMIN_INVITATIONS);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use chrono::TimeZone;

    use super::*;
    use crate::sync::account::{HostedAccountConfig, HostedAccountSnapshot};

    struct MemorySecrets {
        secrets: HashMap<Uuid, String>,
    }

    impl MemorySecrets {
        fn with_session(credential_id: Uuid, token: &str) -> Self {
            let mut secrets = HashMap::new();
            secrets.insert(credential_id, token.to_owned());
            Self { secrets }
        }

        fn empty() -> Self {
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
        bearer: String,
        body: Option<JsonValue>,
    }

    struct ScriptedTransport {
        responses: std::cell::RefCell<Vec<Result<HostedAdminHttpResponse, String>>>,
        seen: std::cell::RefCell<Vec<ObservedRequest>>,
    }

    impl ScriptedTransport {
        fn new(responses: Vec<Result<HostedAdminHttpResponse, String>>) -> Self {
            Self {
                responses: std::cell::RefCell::new(responses),
                seen: std::cell::RefCell::new(Vec::new()),
            }
        }
    }

    impl HostedAdminTransport for ScriptedTransport {
        fn execute(
            &self,
            plan: &HostedAdminRequestPlan,
            body: Option<&JsonValue>,
            session_token: &str,
        ) -> Result<HostedAdminHttpResponse, String> {
            self.seen.borrow_mut().push(ObservedRequest {
                method: plan.method.clone(),
                path: plan.path.clone(),
                bearer: session_token.to_owned(),
                body: body.cloned(),
            });
            if self.responses.borrow().is_empty() {
                return Err("no scripted response".to_owned());
            }
            self.responses.borrow_mut().remove(0)
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 1, 12, 0, 0).unwrap()
    }

    fn client() -> (HostedAdminClient, PathBuf) {
        let dir = std::env::temp_dir().join(format!("ham-sync-admin-{}", Uuid::new_v4()));
        (
            HostedAdminClient::new(JsonHostedAdminStore::new(dir.join("hosted-admin.json"))),
            dir,
        )
    }

    fn signed_in_account(credential_id: Uuid) -> HostedAccountSnapshot {
        let mut account = HostedAccountSnapshot::new(&HostedAccountConfig {
            base_url: "https://logger.example".to_owned(),
            device_name: "Shack Desktop".to_owned(),
        });
        account.session_token_credential_id = Some(credential_id);
        account
    }

    fn hosting_body() -> JsonValue {
        json!({
            "hosting": {
                "operation_mode": "self_hosted",
                "registration_mode": "invite_only",
                "bootstrap_admin_completed": true,
                "session_ttl_seconds": 3600,
                "refresh_ttl_seconds": 86400,
                "invitation_ttl_seconds": 604800,
                "verification_ttl_seconds": 3600,
                "recovery_ttl_seconds": 3600,
                "limits": {"window_seconds": 60},
                "email": {
                    "mode": "test",
                    "from_address": "noreply@logger.example",
                    "verification_base_url": "https://logger.example/verify",
                    "recovery_base_url": "https://logger.example/recover",
                    "webhook_configured": false,
                    "credential_reference_configured": false
                },
                "turnstile": {
                    "enabled_for_open_registration": false,
                    "site_key": null,
                    "secret_configured": false,
                    "siteverify_url": "https://challenges.cloudflare.com/turnstile/v0/siteverify",
                    "timeout_seconds": 10
                },
                "updated_at": "2026-09-01T00:00:00Z"
            }
        })
    }

    fn invitation_body(invite_id: Uuid, with_token: bool) -> JsonValue {
        let mut body = json!({
            "invitation": {
                "invite_id": invite_id,
                "account_id": Uuid::new_v4(),
                "logbook_id": Uuid::new_v4(),
                "invited_email": "operator@example.test",
                "role": "operator",
                "created_by_user_id": Uuid::new_v4(),
                "created_at": "2026-09-01T00:00:00Z",
                "expires_at": "2026-09-08T00:00:00Z",
                "accepted_at": null,
                "revoked_at": null,
                "last_sent_at": "2026-09-01T00:00:00Z",
                "resend_count": 0
            }
        });
        if with_token {
            body.as_object_mut().unwrap().insert(
                "invitation_token".to_owned(),
                JsonValue::String("invite-secret-token".to_owned()),
            );
        }
        body
    }

    #[test]
    fn planning_requires_a_signed_in_session() {
        let account = HostedAccountSnapshot::new(&HostedAccountConfig::default());
        let error = plan_hosted_admin_request(&HostedAdminAction::HostingRead, &account, now())
            .expect_err("planning must fail without a session credential");
        assert!(matches!(error, HostedAdminError::SessionRequired));
    }

    #[test]
    fn planning_builds_admin_routes_from_the_account_endpoint() {
        let credential_id = Uuid::new_v4();
        let account = signed_in_account(credential_id);
        let invite_id = Uuid::new_v4();

        let plan = plan_hosted_admin_request(&HostedAdminAction::HostingRead, &account, now())
            .expect("hosting read plans");
        assert_eq!(plan.method, "GET");
        assert_eq!(plan.path, "/api/v1/admin/hosting");
        assert_eq!(plan.url, "https://logger.example/api/v1/admin/hosting");
        assert_eq!(plan.session_token_credential_id, credential_id);
        assert!(plan.body.is_none());

        let plan = plan_hosted_admin_request(
            &HostedAdminAction::InvitationResend { invite_id },
            &account,
            now(),
        )
        .expect("resend plans");
        assert_eq!(plan.method, "POST");
        assert_eq!(
            plan.path,
            format!("/api/v1/admin/invitations/{invite_id}/resend")
        );

        let plan = plan_hosted_admin_request(&HostedAdminAction::AuditList, &account, now())
            .expect("audit list plans");
        assert_eq!(plan.path, "/api/v1/admin/audits");
    }

    #[test]
    fn hosting_updates_send_only_the_fields_the_operator_set() {
        let update = HostedAdminHostingUpdate {
            registration_mode: Some(HostedAdminRegistrationMode::Open),
            ..Default::default()
        };
        let body = update.to_body().expect("partial update builds a body");
        let object = body.as_object().expect("body is an object");
        assert_eq!(object.len(), 1);
        assert_eq!(object.get("registration_mode").unwrap(), "open");
    }

    #[test]
    fn empty_and_non_positive_hosting_updates_are_rejected_before_sending() {
        let empty = HostedAdminHostingUpdate::default();
        assert!(matches!(
            empty.to_body().expect_err("empty updates are rejected"),
            HostedAdminError::EmptyHostingUpdate
        ));

        let negative = HostedAdminHostingUpdate {
            session_ttl_seconds: Some(0),
            ..Default::default()
        };
        assert!(matches!(
            negative
                .to_body()
                .expect_err("non-positive lifetimes are rejected"),
            HostedAdminError::NonPositiveSeconds {
                field: "session_ttl_seconds"
            }
        ));
    }

    #[test]
    fn mode_and_role_parsing_rejects_unknown_values() {
        assert_eq!(
            "self_hosted".parse::<HostedAdminOperationMode>().unwrap(),
            HostedAdminOperationMode::SelfHosted
        );
        assert_eq!(
            "invite_only"
                .parse::<HostedAdminRegistrationMode>()
                .unwrap(),
            HostedAdminRegistrationMode::InviteOnly
        );
        assert_eq!(
            "viewer".parse::<HostedAdminLogbookRole>().unwrap(),
            HostedAdminLogbookRole::Viewer
        );
        assert!(matches!(
            "wide_open"
                .parse::<HostedAdminRegistrationMode>()
                .expect_err("unknown modes are rejected"),
            HostedAdminError::InvalidValue {
                field: "registration_mode",
                ..
            }
        ));
    }

    #[test]
    fn accepted_hosting_read_records_administrator_rights_and_config() {
        let credential_id = Uuid::new_v4();
        let account = signed_in_account(credential_id);
        let (client, dir) = client();
        let transport = ScriptedTransport::new(vec![Ok(HostedAdminHttpResponse::new(
            200,
            Some(hosting_body()),
        ))]);
        let mut secrets = MemorySecrets::with_session(credential_id, "session-token");

        let result = client
            .execute(
                &HostedAdminAction::HostingRead,
                &account,
                &transport,
                &mut secrets,
                now(),
            )
            .expect("hosting read executes");

        assert!(result.outcome.is_accepted());
        assert!(result.snapshot.is_administrator());
        let hosting = result.snapshot.hosting.expect("hosting config is recorded");
        assert_eq!(
            hosting.registration_mode,
            Some(HostedAdminRegistrationMode::InviteOnly)
        );
        assert!(hosting.bootstrap_admin_completed);
        assert!(!hosting.turnstile.unwrap().secret_configured);

        let seen = transport.seen.borrow();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].bearer, "session-token");
        assert_eq!(seen[0].method, "GET");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn created_invitation_token_is_returned_once_and_never_persisted() {
        let credential_id = Uuid::new_v4();
        let account = signed_in_account(credential_id);
        let invite_id = Uuid::new_v4();
        let (client, dir) = client();
        let transport = ScriptedTransport::new(vec![Ok(HostedAdminHttpResponse::new(
            200,
            Some(invitation_body(invite_id, true)),
        ))]);
        let mut secrets = MemorySecrets::with_session(credential_id, "session-token");

        let mut result = client
            .execute(
                &HostedAdminAction::InvitationCreate {
                    logbook_id: Uuid::new_v4(),
                    email: "Operator@Example.test ".to_owned(),
                    role: HostedAdminLogbookRole::Operator,
                    expires_at: None,
                },
                &account,
                &transport,
                &mut secrets,
                now(),
            )
            .expect("invitation create executes");

        assert!(result.outcome.is_accepted());
        assert_eq!(result.invitation_token(), Some("invite-secret-token"));
        assert_eq!(
            result.take_invitation_token().as_deref(),
            Some("invite-secret-token")
        );
        assert_eq!(result.take_invitation_token(), None);

        // The token must not survive serialization or reach the support file.
        let serialized = serde_json::to_string(&result).expect("result serializes");
        assert!(!serialized.contains("invite-secret-token"));
        let stored = fs::read_to_string(client.store().path()).expect("support record is written");
        assert!(!stored.contains("invite-secret-token"));

        // The email is normalized by Rust before the request is sent.
        let seen = transport.seen.borrow();
        assert_eq!(seen[0].path, "/api/v1/admin/invitations");
        let body = seen[0].body.as_ref().expect("create sends a body");
        assert_eq!(body.get("email").unwrap(), "operator@example.test");

        // The invitation itself is cached for the list surface.
        assert_eq!(result.snapshot.invitations.len(), 1);
        assert_eq!(result.snapshot.invitations[0].invite_id, invite_id);

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn invitation_status_reports_terminal_states_before_expiry() {
        let base = HostedAdminInvitation {
            invite_id: Uuid::new_v4(),
            account_id: None,
            logbook_id: None,
            invited_email: None,
            role: None,
            created_by_user_id: None,
            created_at: None,
            expires_at: Some(now() - chrono::Duration::hours(1)),
            accepted_at: None,
            revoked_at: None,
            last_sent_at: None,
            resend_count: 0,
        };
        assert_eq!(base.status(now()), HostedAdminInvitationStatus::Expired);
        assert!(base.status(now()).can_resend());

        let accepted = HostedAdminInvitation {
            accepted_at: Some(now()),
            ..base.clone()
        };
        assert_eq!(
            accepted.status(now()),
            HostedAdminInvitationStatus::Accepted
        );
        assert!(!accepted.status(now()).can_resend());

        let revoked = HostedAdminInvitation {
            revoked_at: Some(now()),
            ..base.clone()
        };
        assert_eq!(revoked.status(now()), HostedAdminInvitationStatus::Revoked);
        assert!(!revoked.status(now()).can_resend());

        let pending = HostedAdminInvitation {
            expires_at: Some(now() + chrono::Duration::hours(1)),
            ..base
        };
        assert_eq!(pending.status(now()), HostedAdminInvitationStatus::Pending);
    }

    #[test]
    fn revoking_an_invitation_replaces_the_cached_copy_without_duplicating_it() {
        let credential_id = Uuid::new_v4();
        let account = signed_in_account(credential_id);
        let invite_id = Uuid::new_v4();
        let (client, dir) = client();

        let mut revoked = invitation_body(invite_id, false);
        revoked["invitation"]["revoked_at"] = json!("2026-09-02T00:00:00Z");
        let transport = ScriptedTransport::new(vec![
            Ok(HostedAdminHttpResponse::new(
                200,
                Some(invitation_body(invite_id, true)),
            )),
            Ok(HostedAdminHttpResponse::new(200, Some(revoked))),
        ]);
        let mut secrets = MemorySecrets::with_session(credential_id, "session-token");

        client
            .execute(
                &HostedAdminAction::InvitationCreate {
                    logbook_id: Uuid::new_v4(),
                    email: "operator@example.test".to_owned(),
                    role: HostedAdminLogbookRole::Operator,
                    expires_at: None,
                },
                &account,
                &transport,
                &mut secrets,
                now(),
            )
            .expect("create executes");
        let result = client
            .execute(
                &HostedAdminAction::InvitationRevoke { invite_id },
                &account,
                &transport,
                &mut secrets,
                now(),
            )
            .expect("revoke executes");

        assert_eq!(result.snapshot.invitations.len(), 1);
        assert_eq!(
            result.snapshot.invitations[0].status(now()),
            HostedAdminInvitationStatus::Revoked
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn forbidden_marks_the_account_as_not_an_administrator_and_drops_cached_state() {
        let credential_id = Uuid::new_v4();
        let account = signed_in_account(credential_id);
        let (client, dir) = client();
        let transport = ScriptedTransport::new(vec![
            Ok(HostedAdminHttpResponse::new(200, Some(hosting_body()))),
            Ok(HostedAdminHttpResponse::new(
                403,
                Some(json!({"code": "forbidden", "error": "forbidden"})),
            )),
        ]);
        let mut secrets = MemorySecrets::with_session(credential_id, "session-token");

        client
            .execute(
                &HostedAdminAction::HostingRead,
                &account,
                &transport,
                &mut secrets,
                now(),
            )
            .expect("first read executes");
        let result = client
            .execute(
                &HostedAdminAction::HostingRead,
                &account,
                &transport,
                &mut secrets,
                now(),
            )
            .expect("second read executes");

        assert!(!result.outcome.is_accepted());
        assert_eq!(result.snapshot.administrator, Some(false));
        assert!(result.snapshot.hosting.is_none());
        assert_eq!(
            result.message,
            "The signed-in account is not a server administrator."
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn revoked_session_clears_administrator_state_and_requires_user_action() {
        let credential_id = Uuid::new_v4();
        let account = signed_in_account(credential_id);
        let (client, dir) = client();
        let transport = ScriptedTransport::new(vec![
            Ok(HostedAdminHttpResponse::new(200, Some(hosting_body()))),
            Ok(HostedAdminHttpResponse::new(
                401,
                Some(json!({"code": "session_expired", "error": "session expired"})),
            )),
        ]);
        let mut secrets = MemorySecrets::with_session(credential_id, "session-token");

        client
            .execute(
                &HostedAdminAction::HostingRead,
                &account,
                &transport,
                &mut secrets,
                now(),
            )
            .expect("first read executes");
        let result = client
            .execute(
                &HostedAdminAction::AuditList,
                &account,
                &transport,
                &mut secrets,
                now(),
            )
            .expect("audit list executes");

        assert_eq!(result.outcome, HostedAccountOutcome::AuthenticationRequired);
        assert!(result.user_action_required);
        assert!(!result.retryable);
        assert_eq!(result.snapshot.administrator, None);
        assert!(result.snapshot.hosting.is_none());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_missing_stored_session_secret_is_rejected_before_a_request_is_sent() {
        let credential_id = Uuid::new_v4();
        let account = signed_in_account(credential_id);
        let (client, dir) = client();
        let transport = ScriptedTransport::new(Vec::new());
        let mut secrets = MemorySecrets::empty();

        let error = client
            .execute(
                &HostedAdminAction::HostingRead,
                &account,
                &transport,
                &mut secrets,
                now(),
            )
            .expect_err("a missing secret is rejected");

        assert!(matches!(error, HostedAdminError::SessionRequired));
        assert!(transport.seen.borrow().is_empty());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn transport_failures_are_retryable_and_keep_cached_state() {
        let credential_id = Uuid::new_v4();
        let account = signed_in_account(credential_id);
        let (client, dir) = client();
        let transport = ScriptedTransport::new(vec![
            Ok(HostedAdminHttpResponse::new(200, Some(hosting_body()))),
            Err("connection refused".to_owned()),
        ]);
        let mut secrets = MemorySecrets::with_session(credential_id, "session-token");

        client
            .execute(
                &HostedAdminAction::HostingRead,
                &account,
                &transport,
                &mut secrets,
                now(),
            )
            .expect("first read executes");
        let result = client
            .execute(
                &HostedAdminAction::HostingRead,
                &account,
                &transport,
                &mut secrets,
                now(),
            )
            .expect("failed read still returns a result");

        assert_eq!(result.outcome, HostedAccountOutcome::TransientFailure);
        assert!(result.retryable);
        assert!(!result.user_action_required);
        // A network failure must not discard what the operator already loaded.
        assert!(result.snapshot.hosting.is_some());
        assert!(result.snapshot.is_administrator());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn audit_listings_are_newest_first_and_bounded() {
        let credential_id = Uuid::new_v4();
        let account = signed_in_account(credential_id);
        let (client, dir) = client();
        let audits = (0..(MAX_RETAINED_ADMIN_AUDITS + 25))
            .map(|index| {
                json!({
                    "audit_id": Uuid::new_v4(),
                    "occurred_at": Utc
                        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
                        .unwrap()
                        .checked_add_signed(chrono::Duration::seconds(index as i64))
                        .unwrap(),
                    "request_id": format!("request-{index}"),
                    "action": "admin.hosting.update",
                    "outcome": "succeeded",
                    "details": {}
                })
            })
            .collect::<Vec<_>>();
        let transport = ScriptedTransport::new(vec![Ok(HostedAdminHttpResponse::new(
            200,
            Some(json!({"audits": audits})),
        ))]);
        let mut secrets = MemorySecrets::with_session(credential_id, "session-token");

        let result = client
            .execute(
                &HostedAdminAction::AuditList,
                &account,
                &transport,
                &mut secrets,
                now(),
            )
            .expect("audit list executes");

        assert_eq!(result.snapshot.audits.len(), MAX_RETAINED_ADMIN_AUDITS);
        assert_eq!(
            result.snapshot.audits[0].request_id.as_deref(),
            Some(format!("request-{}", MAX_RETAINED_ADMIN_AUDITS + 24).as_str())
        );

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn changing_the_hosted_endpoint_resets_cached_administration_state() {
        let credential_id = Uuid::new_v4();
        let account = signed_in_account(credential_id);
        let (client, dir) = client();
        let transport = ScriptedTransport::new(vec![Ok(HostedAdminHttpResponse::new(
            200,
            Some(hosting_body()),
        ))]);
        let mut secrets = MemorySecrets::with_session(credential_id, "session-token");

        client
            .execute(
                &HostedAdminAction::HostingRead,
                &account,
                &transport,
                &mut secrets,
                now(),
            )
            .expect("read executes");

        let mut moved = account.clone();
        moved.base_url = "https://other.example".to_owned();
        let snapshot = client.snapshot(&moved, now()).expect("record reloads");

        assert_eq!(snapshot.base_url, "https://other.example");
        assert_eq!(snapshot.administrator, None);
        assert!(snapshot.hosting.is_none());
        assert!(snapshot.invitations.is_empty());

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_corrupt_support_record_is_quarantined_and_reinitialized() {
        let credential_id = Uuid::new_v4();
        let account = signed_in_account(credential_id);
        let (client, dir) = client();
        fs::create_dir_all(&dir).expect("support directory is created");
        fs::write(client.store().path(), b"{ not json").expect("corrupt record is written");

        let snapshot = client
            .snapshot(&account, now())
            .expect("a corrupt record is recovered");

        assert_eq!(snapshot.base_url, "https://logger.example");
        assert!(snapshot.hosting.is_none());
        let quarantined = fs::read_dir(&dir)
            .expect("support directory is readable")
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().contains(".corrupt-"));
        assert!(quarantined, "the corrupt record must be quarantined");

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn unsupported_record_versions_are_not_read_as_current() {
        let (client, dir) = client();
        fs::create_dir_all(&dir).expect("support directory is created");
        fs::write(
            client.store().path(),
            serde_json::to_vec(&json!({
                "version": HOSTED_ADMIN_FILE_VERSION + 1,
                "snapshot": HostedAdminSnapshot::new("https://logger.example")
            }))
            .expect("record serializes"),
        )
        .expect("record is written");

        let error = client
            .store()
            .snapshot()
            .expect_err("a newer record version is rejected");
        assert!(matches!(
            error,
            HostedAdminError::UnsupportedFileVersion(version) if version == HOSTED_ADMIN_FILE_VERSION + 1
        ));

        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn read_only_actions_are_separated_from_mutating_ones() {
        assert!(!HostedAdminAction::HostingRead.is_mutating());
        assert!(!HostedAdminAction::InvitationList.is_mutating());
        assert!(!HostedAdminAction::AuditList.is_mutating());
        assert!(HostedAdminAction::InvitationRevoke {
            invite_id: Uuid::new_v4()
        }
        .is_mutating());
        assert!(HostedAdminAction::HostingUpdate {
            update: HostedAdminHostingUpdate::default()
        }
        .is_mutating());
    }
}
