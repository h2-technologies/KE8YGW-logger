use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub error: String,
    pub code: String,
    pub request_id: String,
    pub retryable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl ApiErrorBody {
    pub fn new(
        error: impl Into<String>,
        code: ApiErrorCode,
        request_id: impl Into<String>,
        retryable: bool,
    ) -> Self {
        Self {
            error: error.into(),
            code: code.as_str().to_owned(),
            request_id: request_id.into(),
            retryable,
            details: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiErrorCode {
    BadRequest,
    ValidationFailed,
    InvalidJson,
    InvalidUuid,
    MissingField,
    MissingToken,
    InvalidToken,
    SessionInactive,
    SessionExpired,
    DeviceRevoked,
    EmailUnverified,
    RegistrationClosed,
    TokenExpired,
    TokenReplayed,
    TurnstileFailed,
    Forbidden,
    NotFound,
    UnsupportedMediaType,
    PayloadTooLarge,
    RateLimited,
    ProposalRejected,
    StoreUnavailable,
    InternalError,
}

impl ApiErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BadRequest => "bad_request",
            Self::ValidationFailed => "validation_failed",
            Self::InvalidJson => "invalid_json",
            Self::InvalidUuid => "invalid_uuid",
            Self::MissingField => "missing_field",
            Self::MissingToken => "missing_token",
            Self::InvalidToken => "invalid_token",
            Self::SessionInactive => "session_inactive",
            Self::SessionExpired => "session_expired",
            Self::DeviceRevoked => "device_revoked",
            Self::EmailUnverified => "email_unverified",
            Self::RegistrationClosed => "registration_closed",
            Self::TokenExpired => "token_expired",
            Self::TokenReplayed => "token_replayed",
            Self::TurnstileFailed => "turnstile_failed",
            Self::Forbidden => "forbidden",
            Self::NotFound => "not_found",
            Self::UnsupportedMediaType => "unsupported_media_type",
            Self::PayloadTooLarge => "payload_too_large",
            Self::RateLimited => "rate_limited",
            Self::ProposalRejected => "proposal_rejected",
            Self::StoreUnavailable => "store_unavailable",
            Self::InternalError => "internal_error",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentMode {
    Hosted,
    SelfHosted,
    HostedAndSelfHosted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stability {
    Stable,
    Provisional,
    Deprecated,
    CompatibilityOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthRequirement {
    None,
    BearerSession,
    QuerySyncToken,
    BodySyncToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ApiRouteContract {
    pub method: &'static str,
    pub path: &'static str,
    pub deployment: DeploymentMode,
    pub auth: AuthRequirement,
    pub authorization: &'static str,
    pub request_headers: &'static [&'static str],
    pub path_parameters: &'static [&'static str],
    pub query_parameters: &'static [&'static str],
    pub request_body: Option<&'static str>,
    pub success_status: u16,
    pub success_schema: &'static str,
    pub error_statuses: &'static [u16],
    pub error_codes: &'static [&'static str],
    pub pagination: &'static str,
    pub idempotency: &'static str,
    pub content_type: &'static str,
    pub stability: Stability,
}

pub const ERROR_STATUSES_STANDARD: &[u16] = &[400, 401, 403, 404, 415, 429, 500];
pub const ERROR_CODES_STANDARD: &[&str] = &[
    "bad_request",
    "validation_failed",
    "invalid_json",
    "invalid_uuid",
    "missing_field",
    "missing_token",
    "invalid_token",
    "session_inactive",
    "session_expired",
    "device_revoked",
    "email_unverified",
    "registration_closed",
    "token_expired",
    "token_replayed",
    "turnstile_failed",
    "forbidden",
    "not_found",
    "unsupported_media_type",
    "payload_too_large",
    "rate_limited",
    "proposal_rejected",
    "store_unavailable",
    "internal_error",
];

#[rustfmt::skip]
pub const HOSTED_ROUTE_STRINGS: &[&str] = &[
    "GET /health",
    "GET /api/v1/status",
    "GET /api/v1/routes",
    "POST /api/v1/admin/bootstrap",
    "GET /api/v1/admin/hosting",
    "PATCH /api/v1/admin/hosting",
    "GET /api/v1/admin/invitations",
    "POST /api/v1/admin/invitations",
    "GET /api/v1/admin/invitations/:id",
    "POST /api/v1/admin/invitations/:id/resend",
    "POST /api/v1/admin/invitations/:id/expire",
    "POST /api/v1/admin/invitations/:id/revoke",
    "GET /api/v1/admin/audits",
    "POST /api/v1/auth/register",
    "POST /api/v1/auth/verify-email",
    "POST /api/v1/auth/recovery/start",
    "POST /api/v1/auth/recovery/complete",
    "POST /api/v1/auth/login",
    "POST /api/v1/auth/logout",
    "POST /api/v1/auth/logout-all",
    "POST /api/v1/auth/session/rotate",
    "POST /api/v1/auth/account/delete",
    "GET /api/v1/auth/session",
    "GET /api/v1/logbooks",
    "POST /api/v1/logbooks",
    "GET /api/v1/logbooks/:id",
    "PATCH /api/v1/logbooks/:id",
    "GET /api/v1/qsos",
    "POST /api/v1/qsos",
    "GET /api/v1/qsos/:id",
    "PATCH /api/v1/qsos/:id",
    "POST /api/v1/qsos/:id/delete",
    "POST /api/v1/qsos/:id/restore",
    "POST /api/v1/qsos/:id/notes",
    "GET /api/v1/station-profiles",
    "POST /api/v1/station-profiles",
    "GET /api/v1/station-profiles/:id",
    "PATCH /api/v1/station-profiles/:id",
    "POST /api/v1/station-profiles/:id/archive",
    "POST /api/v1/station-profiles/:id/set-default",
    "GET /api/v1/equipment",
    "POST /api/v1/equipment",
    "GET /api/v1/equipment/:id",
    "PATCH /api/v1/equipment/:id",
    "POST /api/v1/equipment/:id/archive",
    "POST /api/v1/adif/import",
    "GET /api/v1/adif/export",
    "GET /api/v1/activations",
    "POST /api/v1/activations",
    "GET /api/v1/activations/:id",
    "PATCH /api/v1/activations/:id",
    "POST /api/v1/activations/:id/end",
    "GET /api/v1/activations/:id/qsos",
    "GET /api/v1/net-control/sessions",
    "POST /api/v1/net-control/sessions",
    "GET /api/v1/net-control/sessions/:id",
    "PATCH /api/v1/net-control/sessions/:id",
    "POST /api/v1/net-control/sessions/:id/start",
    "POST /api/v1/net-control/sessions/:id/end",
    "POST /api/v1/net-control/sessions/:id/checkins",
    "PATCH /api/v1/net-control/sessions/:id/checkins/:id",
    "POST /api/v1/net-control/sessions/:id/traffic",
    "GET /api/v1/maps/qsos",
    "GET /api/v1/maps/stations",
    "GET /api/v1/maps/paths",
    "GET /api/v1/maps/settings",
    "PATCH /api/v1/maps/settings",
    "POST /api/v1/backups/export",
    "GET /api/v1/backups",
    "GET /api/v1/backups/:id",
    "GET /api/v1/backups/:id/download",
    "POST /api/v1/backups/import/dry-run",
    "POST /api/v1/backups/import",
    "GET /api/v1/providers",
    "GET /api/v1/providers/:id",
    "PATCH /api/v1/providers/:id",
    "POST /api/v1/providers/:id/test",
    "POST /api/v1/providers/:id/lookup",
    "GET /api/v1/providers/:id/spots",
    "POST /api/v1/providers/dx-cluster/connect",
    "POST /api/v1/providers/dx-cluster/read",
    "POST /api/v1/providers/dx-cluster/disconnect",
    "GET /api/v1/providers/dx-cluster/status",
    "GET /api/v1/uploads",
    "POST /api/v1/uploads/run",
    "POST /api/v1/uploads/:id/retry",
    "GET /api/v1/sync/status",
    "POST /api/v1/sync/preview",
    "POST /api/v1/sync/push",
    "POST /api/v1/sync/pull",
    "POST /api/v1/sync/divergence/review",
    "GET /api/v1/sync/divergence/:id",
    "POST /api/v1/sync/divergence/:id/export",
    "GET /api/v1/devices",
    "POST /api/v1/devices",
    "POST /api/v1/devices/revoke-all",
    "POST /api/v1/devices/:id/revoke",
];

#[rustfmt::skip]
pub const SELF_HOSTED_ROUTE_STRINGS: &[&str] = &[
    "GET /health",
    "POST /api/v1/auth/pair",
    "GET /api/v1/logbooks",
    "GET /api/v1/logbooks/:logbook_id/head",
    "GET /api/v1/logbooks/:logbook_id/events",
    "POST /api/v1/logbooks/:logbook_id/preview-pull",
    "POST /api/v1/logbooks/:logbook_id/pull",
    "POST /api/v1/logbooks/:logbook_id/push",
    "GET /api/v1/sync/status",
    "POST /api/v1/reports",
    "GET /api/v1/reports/:report_id",
];

pub fn hosted_route_strings() -> Vec<String> {
    HOSTED_ROUTE_STRINGS
        .iter()
        .map(|route| (*route).to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hosted_route_catalog_has_no_duplicates() {
        let mut routes = hosted_route_strings();
        routes.sort();
        routes.dedup();
        assert_eq!(routes.len(), HOSTED_ROUTE_STRINGS.len());
    }

    #[test]
    fn stable_error_codes_are_snake_case() {
        for code in ERROR_CODES_STANDARD {
            assert!(!code.is_empty());
            assert!(code
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch == '_' || ch.is_ascii_digit()));
        }
    }
}
