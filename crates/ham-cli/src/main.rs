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
    let args = env::args().collect::<Vec<_>>();
    let Some(command) = args.get(1).map(String::as_str) else {
        print_usage();
        return;
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
            let Some(path) = args.get(2) else {
                eprintln!("missing ADIF input file");
                process::exit(1);
            };
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
        "export-adif" => {
            let Some(path) = args.get(2) else {
                eprintln!("missing ADIF output file");
                process::exit(1);
            };
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
            println!("exported ADIF to {path}");
        }
        "verify-chain" => {
            store
                .verify_chain(logbook_id)
                .await
                .unwrap_or_else(|error| {
                    eprintln!("chain verification failed: {error}");
                    process::exit(1);
                });
            println!("official log chain verified");
        }
        "rebuild-projections" => {
            let projection = store
                .rebuild_projections(logbook_id)
                .await
                .unwrap_or_else(|error| {
                    eprintln!("failed to rebuild projections: {error}");
                    process::exit(1);
                });
            println!(
                "rebuilt QSO projection: {} visible QSOs",
                projection.list(false).len()
            );
        }
        _ => print_usage(),
    }
}

fn proposal_context() -> ProposalContext {
    ProposalContext {
        plugin_manifest: PluginManifest {
            plugin_id: "ham-cli".to_owned(),
            name: "Ham CLI".to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            capabilities: vec![PluginCapability::QsoCreate],
        },
        operator_role: OperatorRole::Logger,
    }
}

fn print_usage() {
    eprintln!(
        "usage:
  ham-cli import-adif <file>
  ham-cli export-adif <file>
  ham-cli verify-chain
  ham-cli rebuild-projections"
    );
}
