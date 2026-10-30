//! `ke8ygw`: the desktop GUI and the CLI for headless stations.
//!
//! Milestone 0 skeleton: only `--version` and `--help` work. The CLI
//! subcommands arrive in Milestone 1 and the Tauri 2 shell in Milestone 4.

use std::process::ExitCode;

const USAGE: &str = "\
Usage: ke8ygw [--version | --help]

KE8YGW Logger desktop app. Milestone 0 skeleton: the GUI and the CLI
subcommands are not built yet.";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("--version" | "-V"), None) => {
            println!("ke8ygw {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        (None | Some("--help" | "-h"), None) => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
