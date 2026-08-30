//! Desktop hosted-account client: transport and credential orchestration.
//!
//! Every domain decision stays in `ham_sync::account`. This module only resolves
//! credential references, executes the Rust-planned request over the desktop
//! network stack, hands the classified response back to Rust, and persists the
//! resulting credential references. Raw session and refresh tokens never reach
//! support state, runtime events, or the browser UI.

use std::{collections::BTreeMap, io::Read, time::Duration};

use chrono::{DateTime, Utc};
use ham_core::{CredentialMetadata, CredentialStore};
use ham_plugin_sdk::ServiceType;
use ham_sync::account::{
    apply_account_response, plan_account_request, AccountAction, AccountActionRecord, AccountError,
    AccountRequestPlan, AccountResponseInput, AccountSessionState, JsonAccountSessionStore,
};
use serde_json::{json, Value as JsonValue};
use uuid::Uuid;

const ACCOUNT_CREDENTIAL_PROVIDER_ID: &str = "hosted-account";
const SESSION_CREDENTIAL_PURPOSE: &str = "hosted_account_session_token";
const REFRESH_CREDENTIAL_PURPOSE: &str = "hosted_account_refresh_token";
const REDACTED: &str = "<redacted>";

/// Largest hosted account response body the desktop transport will read.
pub const MAX_ACCOUNT_RESPONSE_BYTES: usize = 256 * 1024;
/// Default per-request timeout for hosted account calls.
pub const DEFAULT_ACCOUNT_TIMEOUT_SECONDS: u64 = 15;

/// A fully resolved hosted request. Secrets live here and nowhere else.
#[derive(Clone)]
pub struct AccountHttpRequest {
    pub method: String,
    pub url: String,
    pub bearer_token: Option<String>,
    pub body: Option<JsonValue>,
    pub request_id: String,
}

impl std::fmt::Debug for AccountHttpRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AccountHttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| REDACTED),
            )
            .field("body", &self.body.as_ref().map(|_| REDACTED))
            .field("request_id", &self.request_id)
            .finish()
    }
}

/// Platform transport boundary. Implementations perform I/O only.
pub trait AccountTransport: Send + Sync {
    fn execute(&self, request: &AccountHttpRequest) -> AccountResponseInput;
}

#[derive(Debug, thiserror::Error)]
pub enum AccountClientError {
    #[error("{0}")]
    Plan(#[from] AccountError),
    #[error("hosted account credential could not be read: {0}")]
    CredentialUnavailable(String),
    #[error("hosted account credential could not be stored: {0}")]
    CredentialWriteFailed(String),
    #[error("hosted account state could not be saved: {0}")]
    StateWriteFailed(String),
}

/// Run one hosted account action end to end: plan, resolve credentials,
/// transport, classify, persist.
pub fn run_account_action(
    state: &mut AccountSessionState,
    store: &JsonAccountSessionStore,
    credentials: &mut dyn CredentialStore,
    transport: &dyn AccountTransport,
    action: &AccountAction,
    now: DateTime<Utc>,
) -> Result<AccountActionRecord, AccountClientError> {
    let plan = plan_account_request(state, action)?;
    let request = resolve_request(&plan, credentials)?;
    let previous_credentials = [
        state.session_token_credential_id,
        state.refresh_token_credential_id,
    ];
    let response = transport.execute(&request);
    let result = apply_account_response(state, action, &response, now);

    if let Some(session_token) = result.issued_session_token.as_deref() {
        let credential_id = upsert_credential(
            credentials,
            state.session_token_credential_id,
            session_token,
            SESSION_CREDENTIAL_PURPOSE,
            "Hosted account session token",
            state,
        )?;
        state.record_session_credentials(Some(credential_id), None, now);
    }
    if let Some(refresh_token) = result.issued_refresh_token.as_deref() {
        let credential_id = upsert_credential(
            credentials,
            state.refresh_token_credential_id,
            refresh_token,
            REFRESH_CREDENTIAL_PURPOSE,
            "Hosted account refresh token",
            state,
        )?;
        state.record_session_credentials(None, Some(credential_id), now);
    }
    if result.clear_session_credentials {
        // Signing out must destroy both stored tokens, not only the one the
        // planned request happened to reference.
        for credential_id in previous_credentials.into_iter().flatten() {
            let _ = credentials.delete_credential(credential_id);
        }
    }

    store
        .save(state)
        .map_err(|error| AccountClientError::StateWriteFailed(error.to_string()))?;
    Ok(result.record)
}

fn resolve_request(
    plan: &AccountRequestPlan,
    credentials: &mut dyn CredentialStore,
) -> Result<AccountHttpRequest, AccountClientError> {
    let bearer_token = match plan.bearer_credential_id {
        Some(credential_id) => Some(
            credentials
                .retrieve_secret(credential_id)
                .map_err(|error| AccountClientError::CredentialUnavailable(error.to_string()))?,
        ),
        None => None,
    };
    let mut secrets = BTreeMap::new();
    for field in &plan.body_secret_fields {
        let secret = credentials
            .retrieve_secret(field.credential_id)
            .map_err(|error| AccountClientError::CredentialUnavailable(error.to_string()))?;
        secrets.insert(field.field.clone(), secret);
    }
    let body = plan.body.clone().map(|mut body| {
        if let Some(object) = body.as_object_mut() {
            for (field, secret) in secrets {
                object.insert(field, JsonValue::String(secret));
            }
        }
        body
    });
    Ok(AccountHttpRequest {
        method: plan.method.clone(),
        url: plan.url.clone(),
        bearer_token,
        body,
        request_id: Uuid::new_v4().to_string(),
    })
}

fn upsert_credential(
    credentials: &mut dyn CredentialStore,
    existing: Option<Uuid>,
    secret: &str,
    purpose: &str,
    label: &str,
    state: &AccountSessionState,
) -> Result<Uuid, AccountClientError> {
    if let Some(credential_id) = existing {
        if credentials.credential_exists(credential_id) {
            credentials
                .update_credential(credential_id, secret, None)
                .map_err(|error| AccountClientError::CredentialWriteFailed(error.to_string()))?;
            return Ok(credential_id);
        }
    }
    let account_reference = state
        .account
        .as_ref()
        .map(|account| account.account_id.to_string())
        .unwrap_or_else(|| "hosted-account".to_owned());
    let mut metadata = CredentialMetadata::new(
        ACCOUNT_CREDENTIAL_PROVIDER_ID,
        account_reference,
        ServiceType::Authentication,
        label,
    );
    metadata.metadata = json!({ "purpose": purpose });
    let stored = credentials
        .store_credential(metadata, secret)
        .map_err(|error| AccountClientError::CredentialWriteFailed(error.to_string()))?;
    Ok(stored.credential_id)
}

/// Desktop transport backed by `ureq`.
#[derive(Debug, Clone)]
pub struct UreqAccountTransport {
    timeout: Duration,
}

impl Default for UreqAccountTransport {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(DEFAULT_ACCOUNT_TIMEOUT_SECONDS),
        }
    }
}

impl UreqAccountTransport {
    pub fn with_timeout_seconds(timeout_seconds: u64) -> Self {
        Self {
            timeout: Duration::from_secs(timeout_seconds.max(1)),
        }
    }
}

impl AccountTransport for UreqAccountTransport {
    fn execute(&self, request: &AccountHttpRequest) -> AccountResponseInput {
        let mut builder = ureq::request(&request.method, &request.url)
            .timeout(self.timeout)
            .set("Accept", "application/json")
            .set("X-Request-ID", &request.request_id);
        if let Some(bearer_token) = request.bearer_token.as_deref() {
            builder = builder.set("Authorization", &format!("Bearer {bearer_token}"));
        }
        let outcome = match request.body.as_ref() {
            Some(body) => match serde_json::to_vec(body) {
                Ok(payload) => builder
                    .set("Content-Type", "application/json")
                    .send_bytes(&payload),
                Err(error) => return AccountResponseInput::transport_failure(error.to_string()),
            },
            None => builder.call(),
        };
        match outcome {
            Ok(response) => read_response(response.status(), response),
            Err(ureq::Error::Status(status, response)) => read_response(status, response),
            Err(error) => AccountResponseInput::transport_failure(error.to_string()),
        }
    }
}

fn read_response(status: u16, response: ureq::Response) -> AccountResponseInput {
    let request_id = response
        .header("x-request-id")
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned);
    let mut buffer = Vec::new();
    if let Err(error) = response
        .into_reader()
        .take(MAX_ACCOUNT_RESPONSE_BYTES as u64 + 1)
        .read_to_end(&mut buffer)
    {
        return AccountResponseInput::transport_failure(error.to_string());
    }
    if buffer.len() > MAX_ACCOUNT_RESPONSE_BYTES {
        return AccountResponseInput::transport_failure(format!(
            "hosted account response exceeded {MAX_ACCOUNT_RESPONSE_BYTES} bytes"
        ));
    }
    AccountResponseInput {
        status,
        body: serde_json::from_slice(&buffer).ok(),
        request_id,
        transport_error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ham_core::InsecureDevCredentialStore;
    use ham_sync::account::{AccountOutcome, AccountSessionStatus};
    use std::{
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    #[derive(Default)]
    struct ScriptedTransport {
        responses: Mutex<Vec<AccountResponseInput>>,
        seen: Arc<Mutex<Vec<AccountHttpRequest>>>,
    }

    impl ScriptedTransport {
        fn new(
            responses: Vec<AccountResponseInput>,
        ) -> (Self, Arc<Mutex<Vec<AccountHttpRequest>>>) {
            let seen = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    responses: Mutex::new(responses),
                    seen: seen.clone(),
                },
                seen,
            )
        }
    }

    impl AccountTransport for ScriptedTransport {
        fn execute(&self, request: &AccountHttpRequest) -> AccountResponseInput {
            self.seen.lock().unwrap().push(request.clone());
            let mut responses = self.responses.lock().unwrap();
            if responses.is_empty() {
                AccountResponseInput::transport_failure("no scripted response")
            } else {
                responses.remove(0)
            }
        }
    }

    struct Fixture {
        dir: PathBuf,
        store: JsonAccountSessionStore,
        credentials: InsecureDevCredentialStore,
        state: AccountSessionState,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-08-30T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn fixture(server_url: &str) -> Fixture {
        let dir = std::env::temp_dir().join(format!("ham-gui-account-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let store = JsonAccountSessionStore::new(dir.join("account-session.json"));
        let state = store.load_or_create(server_url, now()).unwrap();
        let credentials =
            InsecureDevCredentialStore::open(dir.join("dev-credentials.json"), true).unwrap();
        Fixture {
            dir,
            store,
            credentials,
            state,
        }
    }

    fn login_body() -> JsonValue {
        json!({
            "account": {
                "account_id": Uuid::new_v4(),
                "user_id": Uuid::new_v4(),
                "email": "operator@example.com",
                "display_name": "Operator"
            },
            "session": {
                "session_id": Uuid::new_v4(),
                "account_id": Uuid::new_v4(),
                "user_id": Uuid::new_v4(),
                "device_id": Uuid::new_v4(),
                "token": "session-secret",
                "issued_at": "2026-08-30T11:59:00Z",
                "expires_at": "2026-09-29T11:59:00Z",
                "active": true
            },
            "device": {"device_id": Uuid::new_v4(), "device_name": "Shack Desktop"},
            "logbooks": [],
            "refresh_token": "refresh-secret",
            "session_cookie": "ham_session=..."
        })
    }

    fn login_action() -> AccountAction {
        AccountAction::Login {
            email: "operator@example.com".to_owned(),
            display_name: None,
            device_name: Some("Shack Desktop".to_owned()),
        }
    }

    #[test]
    fn login_stores_tokens_in_the_credential_store_and_never_in_support_state() {
        let mut fixture = fixture("https://logger.example");
        let (transport, seen) =
            ScriptedTransport::new(vec![AccountResponseInput::success(200, login_body())]);
        let record = run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &login_action(),
            now(),
        )
        .unwrap();

        assert!(record.outcome.succeeded());
        assert_eq!(fixture.state.status, AccountSessionStatus::SignedIn);
        let session_credential = fixture.state.session_token_credential_id.unwrap();
        let refresh_credential = fixture.state.refresh_token_credential_id.unwrap();
        assert_ne!(session_credential, refresh_credential);
        assert_eq!(
            fixture
                .credentials
                .retrieve_secret(session_credential)
                .unwrap(),
            "session-secret"
        );
        assert_eq!(
            fixture
                .credentials
                .retrieve_secret(refresh_credential)
                .unwrap(),
            "refresh-secret"
        );

        let persisted = std::fs::read_to_string(fixture.store.path()).unwrap();
        assert!(!persisted.contains("session-secret"));
        assert!(!persisted.contains("refresh-secret"));

        let requests = seen.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].url, "https://logger.example/api/v1/auth/login");
        assert!(requests[0].bearer_token.is_none());
        assert!(!format!("{:?}", requests[0]).contains("operator@example.com"));
    }

    #[test]
    fn bearer_actions_send_the_stored_session_token() {
        let mut fixture = fixture("https://logger.example");
        let (transport, _) =
            ScriptedTransport::new(vec![AccountResponseInput::success(200, login_body())]);
        run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &login_action(),
            now(),
        )
        .unwrap();

        let (transport, seen) = ScriptedTransport::new(vec![AccountResponseInput::success(
            200,
            json!({"devices": []}),
        )]);
        run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &AccountAction::ListDevices,
            now(),
        )
        .unwrap();

        let requests = seen.lock().unwrap();
        assert_eq!(requests[0].method, "GET");
        assert_eq!(requests[0].url, "https://logger.example/api/v1/devices");
        assert_eq!(requests[0].bearer_token.as_deref(), Some("session-secret"));
    }

    #[test]
    fn rotation_injects_the_refresh_secret_into_the_planned_body() {
        let mut fixture = fixture("https://logger.example");
        let (transport, _) =
            ScriptedTransport::new(vec![AccountResponseInput::success(200, login_body())]);
        run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &login_action(),
            now(),
        )
        .unwrap();

        let mut rotated = login_body();
        rotated["session"]["token"] = json!("session-secret-2");
        rotated["refresh_token"] = json!("refresh-secret-2");
        let (transport, seen) =
            ScriptedTransport::new(vec![AccountResponseInput::success(200, rotated)]);
        run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &AccountAction::RotateSession,
            now(),
        )
        .unwrap();

        let requests = seen.lock().unwrap();
        assert_eq!(
            requests[0].body.as_ref().unwrap()["refresh_token"],
            "refresh-secret"
        );
        drop(requests);

        let session_credential = fixture.state.session_token_credential_id.unwrap();
        assert_eq!(
            fixture
                .credentials
                .retrieve_secret(session_credential)
                .unwrap(),
            "session-secret-2"
        );
    }

    #[test]
    fn logout_deletes_the_stored_credentials() {
        let mut fixture = fixture("https://logger.example");
        let (transport, _) =
            ScriptedTransport::new(vec![AccountResponseInput::success(200, login_body())]);
        run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &login_action(),
            now(),
        )
        .unwrap();
        let session_credential = fixture.state.session_token_credential_id.unwrap();

        let (transport, _) = ScriptedTransport::new(vec![AccountResponseInput::success(
            200,
            json!({"ok": true}),
        )]);
        run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &AccountAction::Logout,
            now(),
        )
        .unwrap();

        assert_eq!(fixture.state.status, AccountSessionStatus::SignedOut);
        assert!(fixture.state.session_token_credential_id.is_none());
        assert!(fixture
            .credentials
            .retrieve_secret(session_credential)
            .is_err());
    }

    #[test]
    fn logout_also_deletes_the_refresh_credential() {
        let mut fixture = fixture("https://logger.example");
        let (transport, _) =
            ScriptedTransport::new(vec![AccountResponseInput::success(200, login_body())]);
        run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &login_action(),
            now(),
        )
        .unwrap();
        let refresh_credential = fixture.state.refresh_token_credential_id.unwrap();

        let (transport, _) = ScriptedTransport::new(vec![AccountResponseInput::success(
            200,
            json!({"ok": true}),
        )]);
        run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &AccountAction::Logout,
            now(),
        )
        .unwrap();

        assert!(fixture
            .credentials
            .retrieve_secret(refresh_credential)
            .is_err());
        assert!(fixture.state.refresh_token_credential_id.is_none());
    }

    #[test]
    fn transport_failure_is_reported_without_losing_the_session() {
        let mut fixture = fixture("https://logger.example");
        let (transport, _) =
            ScriptedTransport::new(vec![AccountResponseInput::success(200, login_body())]);
        run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &login_action(),
            now(),
        )
        .unwrap();

        let (transport, _) = ScriptedTransport::new(vec![AccountResponseInput::transport_failure(
            "connection refused",
        )]);
        let record = run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &AccountAction::RefreshSession,
            now(),
        )
        .unwrap();
        assert_eq!(record.outcome, AccountOutcome::NetworkUnavailable);
        assert!(record.retryable);
        assert_eq!(fixture.state.status, AccountSessionStatus::SignedIn);
        assert!(fixture.state.session_token_credential_id.is_some());
    }

    #[test]
    fn insecure_public_transport_is_refused_before_any_request_is_sent() {
        let mut fixture = fixture("http://logger.example");
        let (transport, seen) = ScriptedTransport::new(vec![AccountResponseInput::success(
            200,
            json!({"ok": true}),
        )]);
        let error = run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &AccountAction::HostingStatus,
            now(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            AccountClientError::Plan(AccountError::InsecureTransport(_))
        ));
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn missing_server_url_is_reported_as_a_plan_error() {
        let mut fixture = fixture("");
        let (transport, seen) = ScriptedTransport::new(Vec::new());
        let error = run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &AccountAction::HostingStatus,
            now(),
        )
        .unwrap_err();
        assert!(matches!(
            error,
            AccountClientError::Plan(AccountError::MissingServerUrl)
        ));
        assert!(seen.lock().unwrap().is_empty());
    }

    #[test]
    fn expired_session_response_clears_stored_credentials() {
        let mut fixture = fixture("https://logger.example");
        let (transport, _) =
            ScriptedTransport::new(vec![AccountResponseInput::success(200, login_body())]);
        run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &login_action(),
            now(),
        )
        .unwrap();
        let session_credential = fixture.state.session_token_credential_id.unwrap();

        let (transport, _) = ScriptedTransport::new(vec![AccountResponseInput {
            status: 401,
            body: Some(json!({"error": "session expired", "code": "session_expired"})),
            request_id: Some("req-9".to_owned()),
            transport_error: None,
        }]);
        let record = run_account_action(
            &mut fixture.state,
            &fixture.store,
            &mut fixture.credentials,
            &transport,
            &AccountAction::RefreshSession,
            now(),
        )
        .unwrap();

        assert_eq!(record.outcome, AccountOutcome::SessionExpired);
        assert_eq!(record.request_id.as_deref(), Some("req-9"));
        assert_eq!(fixture.state.status, AccountSessionStatus::SessionExpired);
        assert!(fixture.state.session_token_credential_id.is_none());
        assert!(fixture
            .credentials
            .retrieve_secret(session_credential)
            .is_err());
    }
}
