use std::{env, fs, process};

use ham_core::{
    default_credential_store, default_official_event_log_path, export_adif, import_adif,
    AdifImportOptions, CredentialError, CredentialMetadata, CredentialStore, InMemoryEventBus,
    JsonlLogbookEventStore, LogbookEventStore, OperatorRole, ProposalContext, RuntimeLogConfig,
};
use ham_plugin_sdk::{PluginCapability, PluginManifest, ServiceType};
use ham_sync::{
    HostedAccountAction, HostedAccountClient, HostedAccountConfig, HostedAccountError,
    HostedAccountResult, HostedAccountSecrets, HostedAccountSnapshot, HttpHostedAccountTransport,
    JsonHostedAccountStore, HOSTED_ACCOUNT_CREDENTIAL_PROVIDER_ID,
};
use uuid::Uuid;

const DEFAULT_LOGBOOK_ID: &str = "00000000-0000-4000-8000-000000000001";

#[tokio::main]
async fn main() {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
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
            println!("ham-cli {}", env!("CARGO_PKG_VERSION"));
        }
        return;
    }

    if command == "account" {
        run_account_command(&args, json);
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
                &AdifImportOptions::mvp_default("KE8YGW", "ham-cli", Uuid::new_v4()),
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
            "ham-cli",
            "Ham CLI",
            env!("CARGO_PKG_VERSION"),
            vec![PluginCapability::QsoCreate],
        ),
        OperatorRole::Logger,
    )
}

fn print_usage() {
    eprintln!(
        "usage:
  ham-cli import-adif <file> [--json]
  ham-cli export-adif <file> [--json]
  ham-cli verify-chain [--json]
  ham-cli rebuild-projections [--json]
  ham-cli version [--json]
  ham-cli account status [--json]
  ham-cli account configure <server-url> [device-name] [--json]
  ham-cli account register <email> [display-name] [invitation-token] [--json]
  ham-cli account verify-email <token> [--json]
  ham-cli account recovery-start <email> [--json]
  ham-cli account recovery-complete <token> [--json]
  ham-cli account login <email> [display-name] [--json]
  ham-cli account session [--json]
  ham-cli account rotate [--json]
  ham-cli account logout [--json]
  ham-cli account logout-all [--json]
  ham-cli account devices [--json]
  ham-cli account revoke-device <device-id> [--json]
  ham-cli account revoke-all-devices [--json]
  ham-cli account delete --confirm [--json]

Options:
  --json       emit one stable JSON object on stdout
  -h, --help   show this help
  -V, --version show CLI and build version

The logging commands are offline-first and never prompt. The `account` commands
contact the configured hosted server, store session and refresh tokens in the
operating-system credential backend, and never print or persist those tokens.
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
