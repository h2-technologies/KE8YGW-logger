use std::{env, fs, process};

use ham_core::plugin_sdk::{PluginCapability, PluginManifest, ServiceType};
use ham_core::sync::{
    HostedAccountAction, HostedAccountClient, HostedAccountConfig, HostedAccountError,
    HostedAccountResult, HostedAccountSecrets, HostedAccountSnapshot, HostedAdminAction,
    HostedAdminClient, HostedAdminHostingUpdate, HostedAdminInvitationStatus,
    HostedAdminLogbookRole, HostedAdminOperationMode, HostedAdminRegistrationMode,
    HostedAdminResult, HostedAdminSnapshot, HttpHostedAccountTransport, HttpHostedAdminTransport,
    JsonHostedAccountStore, JsonHostedAdminStore, HOSTED_ACCOUNT_CREDENTIAL_PROVIDER_ID,
};
use ham_core::{
    default_credential_store, default_official_event_log_path, export_adif, import_adif,
    AdifImportOptions, CredentialError, CredentialMetadata, CredentialStore, InMemoryEventBus,
    JsonlLogbookEventStore, LogbookEventStore, OperatorRole, ProposalContext, RuntimeLogConfig,
};
use uuid::Uuid;

const DEFAULT_LOGBOOK_ID: &str = "00000000-0000-4000-8000-000000000001";

/// Runs a one-shot command-line operation.
pub async fn run(args: Vec<String>) {
    let mut args = args;
    let json = args.iter().any(|arg| arg == "--json");
    args.retain(|arg| arg != "--json");
    let Some(command) = args.first().map(String::as_str) else {
        print_usage();
        process::exit(2);
    };

    if matches!(command, "--help" | "-h" | "help") {
        require_argument_count(&args, 1);
        print_usage();
        return;
    }
    if matches!(command, "--version" | "-V" | "version") {
        require_argument_count(&args, 1);
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "command": "version",
                    "cli_version": env!("CARGO_PKG_VERSION"),
                    "target_arch": env::consts::ARCH,
                    "target_os": env::consts::OS,
                })
            );
        } else {
            println!("ham-client {}", env!("CARGO_PKG_VERSION"));
        }
        return;
    }

    if command == "account" {
        run_account_command(&args, json);
        return;
    }

    if command == "admin" {
        run_admin_command(&args, json);
        return;
    }

    let path = match command {
        "import-adif" | "export-adif" => {
            require_argument_count(&args, 2);
            Some(args[1].as_str())
        }
        "verify-chain" | "rebuild-projections" => {
            require_argument_count(&args, 1);
            None
        }
        _ => usage_error(&format!("unknown command: {command}")),
    };

    let logbook_id = Uuid::parse_str(DEFAULT_LOGBOOK_ID).expect("default logbook ID is valid");
    let store = match JsonlLogbookEventStore::open(default_official_event_log_path()) {
        Ok(store) => store,
        Err(error) => {
            eprintln!("failed to open official event store: {error}");
            process::exit(1);
        }
    };

    match command {
        "import-adif" => {
            let path = path.expect("validated import path");
            let input = fs::read_to_string(path).unwrap_or_else(|error| {
                eprintln!("failed to read {path}: {error}");
                process::exit(1);
            });
            let bus = InMemoryEventBus::default();
            let summary = import_adif(
                &store,
                &bus,
                &proposal_context(),
                logbook_id,
                &input,
                &AdifImportOptions::mvp_default("KE8YGW", "ham-client", Uuid::new_v4()),
            )
            .await;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "command": "import-adif",
                        "imported": summary.imported_count,
                        "skipped": summary.skipped_count,
                        "duplicates": summary.duplicate_count,
                        "rejected": summary.rejected_count,
                        "errors": summary.errors.iter().map(ToString::to_string).collect::<Vec<_>>(),
                    })
                );
            } else {
                println!(
                    "imported={} skipped={} duplicates={} rejected={}",
                    summary.imported_count,
                    summary.skipped_count,
                    summary.duplicate_count,
                    summary.rejected_count
                );
                for error in summary.errors {
                    eprintln!("{error}");
                }
            }
        }
        "export-adif" => {
            let path = path.expect("validated export path");
            let projection = store
                .rebuild_projections(logbook_id)
                .await
                .unwrap_or_else(|error| {
                    eprintln!("failed to rebuild projections: {error}");
                    process::exit(1);
                });
            fs::write(path, export_adif(&projection, false)).unwrap_or_else(|error| {
                eprintln!("failed to write {path}: {error}");
                process::exit(1);
            });
            if json {
                println!(
                    "{}",
                    serde_json::json!({"command": "export-adif", "path": path})
                );
            } else {
                println!("exported ADIF to {path}");
            }
        }
        "verify-chain" => {
            store
                .verify_chain(logbook_id)
                .await
                .unwrap_or_else(|error| {
                    eprintln!("chain verification failed: {error}");
                    process::exit(1);
                });
            if json {
                println!(
                    "{}",
                    serde_json::json!({"command": "verify-chain", "valid": true})
                );
            } else {
                println!("official log chain verified");
            }
        }
        "rebuild-projections" => {
            let projection = store
                .rebuild_projections(logbook_id)
                .await
                .unwrap_or_else(|error| {
                    eprintln!("failed to rebuild projections: {error}");
                    process::exit(1);
                });
            let visible_qsos = projection.list(false).len();
            if json {
                println!(
                    "{}",
                    serde_json::json!({"command": "rebuild-projections", "visible_qsos": visible_qsos})
                );
            } else {
                println!("rebuilt QSO projection: {visible_qsos} visible QSOs");
            }
        }
        _ => unreachable!("command validated before opening the event store"),
    }
}

fn require_argument_count(args: &[String], expected: usize) {
    if args.len() != expected {
        usage_error("invalid command arguments");
    }
}

fn usage_error(message: &str) -> ! {
    eprintln!("{message}");
    print_usage();
    process::exit(2);
}

fn proposal_context() -> ProposalContext {
    ProposalContext::local_admin(
        PluginManifest::new(
            "ham-client",
            "Ham Client",
            env!("CARGO_PKG_VERSION"),
            vec![PluginCapability::QsoCreate],
        ),
        OperatorRole::Logger,
    )
}

fn print_usage() {
    eprintln!(
        "usage:
  ham-client serve [bind-address]
  ham-client import-adif <file> [--json]
  ham-client export-adif <file> [--json]
  ham-client verify-chain [--json]
  ham-client rebuild-projections [--json]
  ham-client version [--json]
  ham-client account status [--json]
  ham-client account configure <server-url> [device-name] [--json]
  ham-client account register <email> [display-name] [invitation-token] [--json]
  ham-client account verify-email <token> [--json]
  ham-client account recovery-start <email> [--json]
  ham-client account recovery-complete <token> [--json]
  ham-client account bootstrap <email> [display-name] [--json]
  ham-client account login <email> [display-name] [--json]
  ham-client account session [--json]
  ham-client account rotate [--json]
  ham-client account logout [--json]
  ham-client account logout-all [--json]
  ham-client account devices [--json]
  ham-client account revoke-device <device-id> [--json]
  ham-client account revoke-all-devices [--json]
  ham-client account delete --confirm [--json]
  ham-client admin status [--json]
  ham-client admin hosting [--json]
  ham-client admin set-hosting <field> <value> [--json]
  ham-client admin invitations [--json]
  ham-client admin invite <logbook-id> <email> <role> [--json]
  ham-client admin invitation <invite-id> [--json]
  ham-client admin resend <invite-id> [--json]
  ham-client admin expire <invite-id> [--json]
  ham-client admin revoke <invite-id> --confirm [--json]
  ham-client admin audits [--json]

Options:
  --json       emit one stable JSON object on stdout
  -h, --help   show this help
  -V, --version show client and build version

`serve` starts the local web UI server (default 127.0.0.1:9467).
The logging commands are offline-first and never prompt. The `account` commands
contact the configured hosted server, store session and refresh tokens in the
operating-system credential backend, and never print or persist those tokens.

The `admin` commands administer the server the `account` commands are signed in
to; they need a signed-in session that belongs to a server administrator. The
`invite` and `resend` subcommands print a single-use invitation token once. That
token is never stored by the CLI, so capture it from that one output if you are
delivering it yourself.

`set-hosting` accepts one field per call: operation_mode, registration_mode,
session_ttl_seconds, refresh_ttl_seconds, invitation_ttl_seconds,
verification_ttl_seconds, or recovery_ttl_seconds. Fields you do not name are
left exactly as the server has them.

Exit code 0 means success, 1 means an operational/data error, and 2 means
invalid command usage."
    );
}

/// Bridges hosted account secrets to the operating-system credential backend.
///
/// The CLI never prints, logs, or persists a session or refresh token; the
/// durable account record keeps only credential identifiers.
struct CliHostedAccountSecrets {
    store: Box<dyn CredentialStore>,
}

impl HostedAccountSecrets for CliHostedAccountSecrets {
    fn read_secret(&mut self, credential_id: Uuid) -> Result<Option<String>, HostedAccountError> {
        match self.store.retrieve_secret(credential_id) {
            Ok(secret) => Ok(Some(secret)),
            Err(CredentialError::NotFound(_)) => Ok(None),
            Err(error) => Err(HostedAccountError::SecretStorage(error.to_string())),
        }
    }

    fn write_secret(
        &mut self,
        credential_id: Uuid,
        label: &str,
        secret: &str,
    ) -> Result<(), HostedAccountError> {
        if self.store.credential_exists(credential_id) {
            self.store
                .update_credential(credential_id, secret, None)
                .map(|_| ())
                .map_err(|error| HostedAccountError::SecretStorage(error.to_string()))
        } else {
            let mut metadata = CredentialMetadata::new(
                HOSTED_ACCOUNT_CREDENTIAL_PROVIDER_ID,
                HOSTED_ACCOUNT_CREDENTIAL_PROVIDER_ID,
                ServiceType::Authentication,
                label,
            );
            metadata.credential_id = credential_id;
            self.store
                .store_credential(metadata, secret)
                .map(|_| ())
                .map_err(|error| HostedAccountError::SecretStorage(error.to_string()))
        }
    }

    fn clear_secret(&mut self, credential_id: Uuid) -> Result<(), HostedAccountError> {
        match self.store.delete_credential(credential_id) {
            Ok(()) => Ok(()),
            Err(CredentialError::NotFound(_)) => Ok(()),
            Err(error) => Err(HostedAccountError::SecretStorage(error.to_string())),
        }
    }
}

fn account_support_dir() -> std::path::PathBuf {
    RuntimeLogConfig::default_for_app()
        .directory
        .join("support")
}

fn account_client() -> HostedAccountClient {
    HostedAccountClient::new(JsonHostedAccountStore::new(
        account_support_dir().join("hosted-account.json"),
    ))
}

fn account_argument<'a>(args: &'a [String], index: usize, field: &str) -> &'a str {
    match args.get(index).map(String::as_str) {
        Some(value) if !value.trim().is_empty() => value,
        _ => usage_error(&format!("account command requires <{field}>")),
    }
}

fn optional_account_argument(args: &[String], index: usize) -> Option<String> {
    args.get(index)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn run_account_command(args: &[String], json: bool) {
    let Some(subcommand) = args.get(1).map(String::as_str) else {
        usage_error("account command requires a subcommand");
    };
    let client = account_client();
    let now = chrono::Utc::now();

    if subcommand == "status" {
        require_argument_count(args, 2);
        let snapshot = client
            .snapshot(&HostedAccountConfig::default(), now)
            .unwrap_or_else(|error| {
                eprintln!("failed to read hosted account state: {error}");
                process::exit(1);
            });
        print_account_snapshot(&snapshot, json);
        return;
    }

    if subcommand == "configure" {
        let base_url = account_argument(args, 2, "server-url").to_owned();
        let current = client
            .snapshot(&HostedAccountConfig::default(), now)
            .unwrap_or_else(|error| {
                eprintln!("failed to read hosted account state: {error}");
                process::exit(1);
            });
        let device_name =
            optional_account_argument(args, 3).unwrap_or_else(|| current.device_name.clone());
        let snapshot = client
            .configure(
                &HostedAccountConfig {
                    base_url,
                    device_name,
                },
                now,
            )
            .unwrap_or_else(|error| {
                eprintln!("invalid hosted account configuration: {error}");
                process::exit(1);
            });
        print_account_snapshot(&snapshot, json);
        return;
    }

    let action = match subcommand {
        "register" => HostedAccountAction::Register {
            email: account_argument(args, 2, "email").to_owned(),
            display_name: optional_account_argument(args, 3),
            invitation_token: optional_account_argument(args, 4),
            turnstile_token: None,
        },
        "verify-email" => HostedAccountAction::VerifyEmail {
            token: account_argument(args, 2, "token").to_owned(),
        },
        "recovery-start" => HostedAccountAction::RecoveryStart {
            email: account_argument(args, 2, "email").to_owned(),
        },
        "recovery-complete" => HostedAccountAction::RecoveryComplete {
            token: account_argument(args, 2, "token").to_owned(),
        },
        "bootstrap" => HostedAccountAction::Bootstrap {
            email: account_argument(args, 2, "email").to_owned(),
            display_name: optional_account_argument(args, 3),
        },
        "login" => HostedAccountAction::Login {
            email: account_argument(args, 2, "email").to_owned(),
            display_name: optional_account_argument(args, 3),
        },
        "session" => {
            require_argument_count(args, 2);
            HostedAccountAction::Session
        }
        "rotate" => {
            require_argument_count(args, 2);
            HostedAccountAction::SessionRotate
        }
        "logout" => {
            require_argument_count(args, 2);
            HostedAccountAction::Logout
        }
        "logout-all" => {
            require_argument_count(args, 2);
            HostedAccountAction::LogoutAll
        }
        "devices" => {
            require_argument_count(args, 2);
            HostedAccountAction::DeviceList
        }
        "revoke-device" => {
            let device_id = account_argument(args, 2, "device-id");
            HostedAccountAction::DeviceRevoke {
                device_id: Uuid::parse_str(device_id)
                    .unwrap_or_else(|_| usage_error("device-id must be a UUID")),
            }
        }
        "revoke-all-devices" => {
            require_argument_count(args, 2);
            HostedAccountAction::DeviceRevokeAll
        }
        "delete" => {
            if args.get(2).map(String::as_str) != Some("--confirm") {
                usage_error("account delete requires --confirm");
            }
            require_argument_count(args, 3);
            HostedAccountAction::AccountDelete
        }
        other => usage_error(&format!("unknown account subcommand: {other}")),
    };

    let config = client
        .snapshot(&HostedAccountConfig::default(), now)
        .map(|snapshot| snapshot.config())
        .unwrap_or_default();
    let mut secrets = CliHostedAccountSecrets {
        store: default_credential_store(
            account_support_dir(),
            env::var("HAM_PLATFORM_ALLOW_INSECURE_DEV_CREDENTIALS").as_deref() == Ok("1"),
        ),
    };
    let result = client
        .execute(
            &action,
            &config,
            &HttpHostedAccountTransport::new(),
            &mut secrets,
            now,
        )
        .unwrap_or_else(|error| {
            eprintln!("hosted account request rejected: {error}");
            process::exit(1);
        });

    print_account_result(&result, json);
    if !result.outcome.is_accepted() {
        process::exit(1);
    }
}

fn admin_client() -> HostedAdminClient {
    HostedAdminClient::new(JsonHostedAdminStore::new(
        account_support_dir().join("hosted-admin.json"),
    ))
}

fn admin_invite_id(args: &[String], index: usize) -> Uuid {
    let value = account_argument(args, index, "invite-id");
    Uuid::parse_str(value).unwrap_or_else(|_| usage_error("invite-id must be a UUID"))
}

/// Builds a single-field hosting patch.
///
/// One field per call keeps the CLI honest: an operator can never overwrite a
/// hosting value they did not name.
fn admin_hosting_update(field: &str, value: &str) -> HostedAdminHostingUpdate {
    let mut update = HostedAdminHostingUpdate::default();
    let seconds = || -> i64 {
        value
            .parse::<i64>()
            .unwrap_or_else(|_| usage_error(&format!("{field} must be a whole number of seconds")))
    };
    match field {
        "operation_mode" => {
            update.operation_mode = Some(
                value
                    .parse::<HostedAdminOperationMode>()
                    .unwrap_or_else(|error| usage_error(&error.to_string())),
            )
        }
        "registration_mode" => {
            update.registration_mode = Some(
                value
                    .parse::<HostedAdminRegistrationMode>()
                    .unwrap_or_else(|error| usage_error(&error.to_string())),
            )
        }
        "session_ttl_seconds" => update.session_ttl_seconds = Some(seconds()),
        "refresh_ttl_seconds" => update.refresh_ttl_seconds = Some(seconds()),
        "invitation_ttl_seconds" => update.invitation_ttl_seconds = Some(seconds()),
        "verification_ttl_seconds" => update.verification_ttl_seconds = Some(seconds()),
        "recovery_ttl_seconds" => update.recovery_ttl_seconds = Some(seconds()),
        other => usage_error(&format!("unknown hosting field: {other}")),
    }
    if let Err(error) = update.to_body() {
        usage_error(&error.to_string());
    }
    update
}

fn run_admin_command(args: &[String], json: bool) {
    let Some(subcommand) = args.get(1).map(String::as_str) else {
        usage_error("admin command requires a subcommand");
    };
    let admin = admin_client();
    let account_client = account_client();
    let now = chrono::Utc::now();

    let account = account_client
        .snapshot(&HostedAccountConfig::default(), now)
        .unwrap_or_else(|error| {
            eprintln!("failed to read hosted account state: {error}");
            process::exit(1);
        });

    if subcommand == "status" {
        require_argument_count(args, 2);
        let snapshot = admin.snapshot(&account, now).unwrap_or_else(|error| {
            eprintln!("failed to read hosted administration state: {error}");
            process::exit(1);
        });
        print_admin_snapshot(&snapshot, &account, json);
        return;
    }

    let action = match subcommand {
        "hosting" => {
            require_argument_count(args, 2);
            HostedAdminAction::HostingRead
        }
        "set-hosting" => {
            require_argument_count(args, 4);
            let field = account_argument(args, 2, "field");
            let value = account_argument(args, 3, "value");
            HostedAdminAction::HostingUpdate {
                update: admin_hosting_update(field, value),
            }
        }
        "invitations" => {
            require_argument_count(args, 2);
            HostedAdminAction::InvitationList
        }
        "invite" => {
            require_argument_count(args, 5);
            let logbook_id = Uuid::parse_str(account_argument(args, 2, "logbook-id"))
                .unwrap_or_else(|_| usage_error("logbook-id must be a UUID"));
            let email = account_argument(args, 3, "email").to_owned();
            let role = account_argument(args, 4, "role")
                .parse::<HostedAdminLogbookRole>()
                .unwrap_or_else(|error| usage_error(&error.to_string()));
            HostedAdminAction::InvitationCreate {
                logbook_id,
                email,
                role,
                expires_at: None,
            }
        }
        "invitation" => {
            require_argument_count(args, 3);
            HostedAdminAction::InvitationGet {
                invite_id: admin_invite_id(args, 2),
            }
        }
        "resend" => {
            require_argument_count(args, 3);
            HostedAdminAction::InvitationResend {
                invite_id: admin_invite_id(args, 2),
            }
        }
        "expire" => {
            require_argument_count(args, 3);
            HostedAdminAction::InvitationExpire {
                invite_id: admin_invite_id(args, 2),
                expires_at: None,
            }
        }
        "revoke" => {
            let invite_id = admin_invite_id(args, 2);
            if args.get(3).map(String::as_str) != Some("--confirm") {
                usage_error("admin revoke requires --confirm");
            }
            require_argument_count(args, 4);
            HostedAdminAction::InvitationRevoke { invite_id }
        }
        "audits" => {
            require_argument_count(args, 2);
            HostedAdminAction::AuditList
        }
        other => usage_error(&format!("unknown admin subcommand: {other}")),
    };

    let mut secrets = CliHostedAccountSecrets {
        store: default_credential_store(
            account_support_dir(),
            env::var("HAM_PLATFORM_ALLOW_INSECURE_DEV_CREDENTIALS").as_deref() == Ok("1"),
        ),
    };
    let mut result = admin
        .execute(
            &action,
            &account,
            &HttpHostedAdminTransport::new(),
            &mut secrets,
            now,
        )
        .unwrap_or_else(|error| {
            eprintln!("hosted administration request rejected: {error}");
            process::exit(1);
        });

    let invitation_token = result.take_invitation_token();
    print_admin_result(&result, invitation_token.as_deref(), json);
    if !result.outcome.is_accepted() {
        process::exit(1);
    }
}

fn admin_rights_label(snapshot: &HostedAdminSnapshot) -> &'static str {
    match snapshot.administrator {
        Some(true) => "administrator",
        Some(false) => "not-an-administrator",
        None => "unchecked",
    }
}

fn print_admin_snapshot(
    snapshot: &HostedAdminSnapshot,
    account: &HostedAccountSnapshot,
    json: bool,
) {
    let now = chrono::Utc::now();
    let pending = snapshot
        .invitations_with_status(HostedAdminInvitationStatus::Pending, now)
        .len();
    if json {
        println!(
            "{}",
            serde_json::json!({
                "command": "admin",
                "action": "admin.status",
                "base_url": snapshot.base_url,
                "administrator": snapshot.administrator,
                "signed_in": account.connection_state.is_signed_in(),
                "hosting": snapshot.hosting,
                "invitations": snapshot.invitations.len(),
                "pending_invitations": pending,
                "audits": snapshot.audits.len(),
                "last_action": snapshot.last_action,
                "last_outcome": snapshot.last_outcome,
            })
        );
    } else {
        println!(
            "rights={} server={} signed_in={} invitations={} pending={} audits={}",
            admin_rights_label(snapshot),
            snapshot.base_url,
            account.connection_state.is_signed_in(),
            snapshot.invitations.len(),
            pending,
            snapshot.audits.len()
        );
    }
}

/// Prints one administration result.
///
/// The invitation token is printed exactly once, on its own line, and is never
/// written to the durable record.
fn print_admin_result(result: &HostedAdminResult, invitation_token: Option<&str>, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "command": "admin",
                "action": result.action,
                "outcome": result.outcome,
                "status": result.status,
                "message": result.message,
                "error_code": result.error_code,
                "request_id": result.request_id,
                "retryable": result.retryable,
                "user_action_required": result.user_action_required,
                "administrator": result.snapshot.administrator,
                "hosting": result.snapshot.hosting,
                "invitation": result.invitation,
                "invitations": result.snapshot.invitations,
                "audits": result.snapshot.audits,
                "invitation_token": invitation_token,
            })
        );
    } else {
        println!(
            "{} outcome={} rights={} {}",
            result.action,
            result.outcome,
            admin_rights_label(&result.snapshot),
            result.message
        );
        let now = chrono::Utc::now();
        // Listings are only useful if the operator can read the identifiers the
        // other subcommands need, so print the rows rather than a count.
        if result.action == "admin.invitation.list" {
            for invitation in &result.snapshot.invitations {
                println!(
                    "  {} {} {} expires={} resends={}",
                    invitation.invite_id,
                    invitation.invited_email.as_deref().unwrap_or("unknown"),
                    invitation.status(now).as_str(),
                    invitation
                        .expires_at
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_else(|| "never".to_owned()),
                    invitation.resend_count
                );
            }
        }
        if result.action == "admin.audit.list" {
            for audit in result.snapshot.audits.iter().take(50) {
                println!(
                    "  {} {} {} {}",
                    audit
                        .occurred_at
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_else(|| "unknown".to_owned()),
                    audit.action.as_deref().unwrap_or("unknown"),
                    audit.outcome.as_deref().unwrap_or("unknown"),
                    audit.target.as_deref().unwrap_or("")
                );
            }
        }
        if let Some(invitation) = &result.invitation {
            println!(
                "  {} {} {}",
                invitation.invite_id,
                invitation.invited_email.as_deref().unwrap_or("unknown"),
                invitation.status(now).as_str()
            );
        }
        if let Some(token) = invitation_token {
            println!("invitation_token={token}");
            eprintln!("this token is shown once and is not stored; deliver it now if needed");
        }
        if !result.outcome.is_accepted() {
            eprintln!(
                "retryable={} user_action_required={}",
                result.retryable, result.user_action_required
            );
        }
    }
}

fn print_account_snapshot(snapshot: &HostedAccountSnapshot, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "command": "account",
                "action": "account.status",
                "connection_state": snapshot.connection_state,
                "base_url": snapshot.base_url,
                "device_name": snapshot.device_name,
                "email": snapshot.email,
                "email_verified": snapshot.email_verified,
                "device_id": snapshot.device_id,
                "session_expires_at": snapshot.session_expires_at,
                "devices": snapshot.devices,
                "logbooks": snapshot.logbooks,
                "last_action": snapshot.last_action,
                "last_outcome": snapshot.last_outcome,
            })
        );
    } else {
        println!(
            "state={} server={} device={} email={}",
            snapshot.connection_state_label(),
            snapshot.base_url,
            snapshot.device_name,
            snapshot.email.as_deref().unwrap_or("none")
        );
    }
}

fn print_account_result(result: &HostedAccountResult, json: bool) {
    if json {
        println!(
            "{}",
            serde_json::json!({
                "command": "account",
                "action": result.action,
                "outcome": result.outcome,
                "status": result.status,
                "message": result.message,
                "error_code": result.error_code,
                "request_id": result.request_id,
                "retryable": result.retryable,
                "user_action_required": result.user_action_required,
                "connection_state": result.snapshot.connection_state,
                "devices": result.snapshot.devices,
                "logbooks": result.snapshot.logbooks,
            })
        );
    } else {
        println!(
            "{} outcome={} state={} {}",
            result.action,
            result.outcome,
            result.snapshot.connection_state_label(),
            result.message
        );
        if !result.outcome.is_accepted() {
            eprintln!(
                "retryable={} user_action_required={}",
                result.retryable, result.user_action_required
            );
        }
    }
}
