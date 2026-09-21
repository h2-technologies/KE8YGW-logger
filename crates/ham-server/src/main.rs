//! Server binary: the hosted API and the self-hosted sync service, one process.

use std::{
    env,
    net::TcpListener,
    process,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::Duration,
};

use ham_server::projector::{
    ProjectorConfig, DEFAULT_PROJECTION_BATCH_SIZE, DEFAULT_PROJECTION_POLL_INTERVAL_SECONDS,
};
use ham_server::sync_storage::{DurableCloudSyncPaths, DurableCloudSyncServer};
use ham_server::{
    http, CloudServerConfig, CloudServiceMode, HostedServer, MergedServer, SurrealHostedConfig,
    DEFAULT_CLOUD_SYNC_SESSION_TTL_SECONDS,
};

const DEFAULT_BIND: &str = "127.0.0.1:9750";

fn main() {
    let args = env::args().skip(1).collect::<Vec<_>>();
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        print_usage();
        return;
    }
    // A full projection rebuild is destructive to the projection (never to the
    // official log) and is therefore only ever an explicit operator action.
    let rebuild_projection = args.iter().any(|arg| arg == "--rebuild-projection");
    if let Some(unknown) = args
        .iter()
        .find(|arg| !matches!(arg.as_str(), "--rebuild-projection"))
    {
        eprintln!("unknown argument: {unknown}");
        print_usage();
        process::exit(2);
    }

    let addr = env::var("HAM_SERVER_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_owned());

    let metadata_config = SurrealHostedConfig::from_env();
    let metadata_label = metadata_config.label();
    let hosted = match HostedServer::with_surreal_config(metadata_config) {
        Ok(server) => server,
        Err(error) => {
            eprintln!(
                "failed to open ham-server SurrealDB metadata store at {metadata_label}: {error}"
            );
            process::exit(1);
        }
    };

    let sync_paths = DurableCloudSyncPaths::from_env();
    let sync = match DurableCloudSyncServer::open(sync_config(), sync_paths.clone()) {
        Ok(server) => server,
        Err(error) => {
            eprintln!("failed to initialize durable sync storage: {error}");
            process::exit(1);
        }
    };

    let listener = match TcpListener::bind(&addr) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("failed to bind ham-server to {addr}: {error}");
            process::exit(1);
        }
    };
    let runtime = tokio::runtime::Runtime::new().expect("ham-server runtime should start");
    let sync = Arc::new(sync);
    let projection_shutdown = start_projector(&sync, rebuild_projection);
    let server = MergedServer::new(hosted, sync);

    println!("ham-server listening on http://{addr}");
    println!("hosted metadata store: {metadata_label}");
    println!(
        "self-hosted sync metadata store: {}",
        sync_paths.metadata_store_path.display()
    );
    println!(
        "self-hosted official event log: {}",
        sync_paths.official_event_log_path.display()
    );
    println!(
        "self-hosted report directory: {}",
        sync_paths.report_dir.display()
    );

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                if let Err(error) = http::handle_stream(&server, &runtime, &mut stream) {
                    eprintln!("failed to handle request: {error}");
                }
            }
            Err(error) => eprintln!("failed to accept request: {error}"),
        }
    }

    projection_shutdown.store(true, Ordering::Relaxed);
}

fn print_usage() {
    eprintln!(
        "usage:
  ham-server [--rebuild-projection]

Options:
  --rebuild-projection  Wipe the SurrealDB projection and replay the whole
                        official event log before serving. This rebuilds the
                        projection only; the append-only JSONL official log is
                        never modified. Without this flag the projector resumes
                        from its stored checkpoint.
  -h, --help            Show this help.

The server reads its configuration from the environment; see .env.example."
    );
}

/// Starts the JSONL -> SurrealDB projector and returns its shutdown flag.
///
/// The projector is a one-way reader of the official log: it never writes to the
/// log, and the SurrealDB projection it maintains is never authoritative.
fn start_projector(sync: &Arc<DurableCloudSyncServer>, rebuild: bool) -> Arc<AtomicBool> {
    let shutdown = Arc::new(AtomicBool::new(false));
    if env::var("HAM_SYNC_PROJECTION_ENABLED").as_deref() == Ok("0") {
        println!("projection: disabled by HAM_SYNC_PROJECTION_ENABLED=0");
        return shutdown;
    }

    let mut config = ProjectorConfig::new(sync.official_event_log_path());
    config.batch_size = parse_env_usize(
        "HAM_SYNC_PROJECTION_BATCH_SIZE",
        DEFAULT_PROJECTION_BATCH_SIZE,
    );
    config.poll_interval = Duration::from_secs(parse_env_u64(
        "HAM_SYNC_PROJECTION_POLL_SECONDS",
        DEFAULT_PROJECTION_POLL_INTERVAL_SECONDS,
    ));
    if let Ok(writer_id) = env::var("HAM_SYNC_PROJECTION_WRITER_ID") {
        config.writer_id = writer_id;
    }

    let projector = match sync.projector(config) {
        Ok(projector) => Arc::new(projector),
        Err(error) => {
            report_projection_failure(&format!("failed to open the projector: {error}"));
            return shutdown;
        }
    };

    if rebuild {
        println!("projection: full rebuild requested; replaying the official event log");
        match projector.rebuild() {
            Ok(report) => println!(
                "projection: rebuilt {} entries into SurrealDB in {} ms (generation {})",
                report.events_projected, report.elapsed_ms, report.checkpoint.rebuild_generation
            ),
            Err(error) => {
                report_projection_failure(&error.to_string());
                return shutdown;
            }
        }
    }

    match projector.checkpoint() {
        Ok(checkpoint) if checkpoint.is_halted() => {
            report_projection_failure(&format!(
                "the projection is halted at official event log entry {}: {}",
                checkpoint.halt_sequence.unwrap_or(checkpoint.sequence),
                checkpoint
                    .halt_reason
                    .as_deref()
                    .unwrap_or("unknown reason")
            ));
            return shutdown;
        }
        Ok(checkpoint) => println!(
            "projection: resuming from entry {} (byte offset {})",
            checkpoint.sequence, checkpoint.byte_offset
        ),
        Err(error) => {
            report_projection_failure(&format!(
                "failed to read the projection checkpoint: {error}"
            ));
            return shutdown;
        }
    }

    thread::spawn({
        let projector = Arc::clone(&projector);
        let shutdown = Arc::clone(&shutdown);
        move || {
            if let Err(error) = projector.tail(&shutdown) {
                report_projection_failure(&error.to_string());
            }
        }
    });
    shutdown
}

/// Prints a projection failure so it cannot be mistaken for routine log noise.
///
/// The server keeps serving: the append-only official log, the sync protocol,
/// and the hosted API are unaffected by a frozen read model, and taking the
/// server down would remove the very access an operator needs to investigate.
/// The halt is also recorded durably in the projection checkpoint.
fn report_projection_failure(detail: &str) {
    eprintln!(
        "\n\
         ======================================================================\n\
         ham-server: SURREALDB PROJECTION STOPPED\n\
         {detail}\n\
         The SurrealDB projection is frozen and is now STALE. Anything reading \n\
         it (GUI, plugins, queries) is seeing out-of-date data. Sync, the hosted \n\
         API, and the append-only official event log are unaffected.\n\
         Investigate the official event log, then restart with \n\
         `ham-server --rebuild-projection` to replay it from the start.\n\
         ======================================================================\n"
    );
}

fn parse_env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn parse_env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn sync_config() -> CloudServerConfig {
    let public_url = env::var("HAM_SYNC_PUBLIC_URL").unwrap_or_else(|_| {
        format!(
            "http://{}",
            env::var("HAM_SERVER_BIND")
                .as_deref()
                .unwrap_or(DEFAULT_BIND)
        )
    });
    let pairing_code =
        env::var("HAM_SYNC_PAIRING_CODE").unwrap_or_else(|_| "local-dev-pairing-code".to_owned());
    let sync_session_ttl_seconds = match env::var("HAM_SYNC_SESSION_TTL_SECONDS") {
        Ok(value) => match value.parse::<i64>() {
            Ok(seconds) if seconds > 0 => Some(seconds),
            _ => {
                eprintln!("HAM_SYNC_SESSION_TTL_SECONDS must be a positive integer");
                process::exit(1);
            }
        },
        Err(_) => Some(DEFAULT_CLOUD_SYNC_SESSION_TTL_SECONDS),
    };
    let mode = match env::var("HAM_SYNC_SERVICE_MODE")
        .unwrap_or_else(|_| "self_hosted".to_owned())
        .as_str()
    {
        "hosted" => CloudServiceMode::Hosted,
        _ => CloudServiceMode::SelfHosted,
    };

    CloudServerConfig {
        mode,
        public_url,
        pairing_code,
        sync_session_ttl_seconds,
    }
}
