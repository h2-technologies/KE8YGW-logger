//! Server binary: the hosted API and the self-hosted sync service, one process.

use std::{env, net::TcpListener, process};

use ham_server::sync_storage::{DurableCloudSyncPaths, DurableCloudSyncServer};
use ham_server::{
    http, CloudServerConfig, CloudServiceMode, HostedServer, MergedServer, SurrealHostedConfig,
    DEFAULT_CLOUD_SYNC_SESSION_TTL_SECONDS,
};

const DEFAULT_BIND: &str = "127.0.0.1:9750";

fn main() {
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
    let server = MergedServer::new(hosted, std::sync::Arc::new(sync));

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
