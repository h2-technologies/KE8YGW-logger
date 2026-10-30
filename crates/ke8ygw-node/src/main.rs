//! `ke8ygw-node`: the headless node for Raspberry Pi and servers.
//!
//! Milestone 0 skeleton: only `--version` and `--help` work. The node itself arrives in Milestone 1.

use std::process::ExitCode;

const USAGE: &str = "\
Usage: ke8ygw-node [--version | --help]

KE8YGW Logger headless node. Milestone 0 skeleton: the node does not
run yet.";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    match (args.next().as_deref(), args.next()) {
        (Some("--version" | "-V"), None) => {
            println!("ke8ygw-node {}", env!("CARGO_PKG_VERSION"));
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
