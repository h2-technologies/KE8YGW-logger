//! Local-first client: the web UI server and the offline command-line tools.
//!
//! `serve` runs the local HTTP server that backs the bundled web UI. Every
//! other command is a one-shot operation over the same local data.

mod cli;
mod mock;
mod serve;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();

    if args.first().map(String::as_str) == Some("serve") {
        serve::run(args.get(1).cloned());
        return;
    }

    let runtime = tokio::runtime::Runtime::new().expect("client runtime should start");
    runtime.block_on(cli::run(args));
}
