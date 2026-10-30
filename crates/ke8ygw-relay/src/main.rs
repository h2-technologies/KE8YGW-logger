//! `ke8ygw-relay`: the optional store-and-forward relay.
//!
//! Milestone 0 skeleton: only `--version` and `--help` work. The relay arrives in Milestone 5.

use std::process::ExitCode;

const USAGE: &str = "\
Usage: ke8ygw-relay [--version | --help]

KE8YGW Logger relay. Milestone 0 skeleton: the relay does not run yet.";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("--version" | "-V"), None) => {
            println!("ke8ygw-relay {}", env!("CARGO_PKG_VERSION"));
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
