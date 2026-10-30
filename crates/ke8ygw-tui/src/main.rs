//! `ke8ygw-tui`: the terminal UI.
//!
//! Milestone 0 skeleton: only `--version` and `--help` work. The TUI arrives in Milestone 5.

use std::process::ExitCode;

const USAGE: &str = "\
Usage: ke8ygw-tui [--version | --help]

KE8YGW Logger terminal UI. Milestone 0 skeleton: the TUI is not built
yet.";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("--version" | "-V"), None) => {
            println!("ke8ygw-tui {}", env!("CARGO_PKG_VERSION"));
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
