use std::{env, fs, process};

use ham_core::{
    default_official_event_log_path, export_adif, import_adif, AdifImportOptions, InMemoryEventBus,
    JsonlLogbookEventStore, LogbookEventStore, OperatorRole, ProposalContext,
};
use ham_plugin_sdk::{PluginCapability, PluginManifest};
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

Options:
  --json       emit one stable JSON object on stdout
  -h, --help   show this help
  -V, --version show CLI and build version

The CLI is offline-first and never prompts for these commands. Exit code 0 means
success, 1 means an operational/data error, and 2 means invalid command usage."
    );
}
