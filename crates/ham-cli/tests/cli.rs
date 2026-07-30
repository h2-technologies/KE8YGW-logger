use std::process::Command;

fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_ham-cli"))
}

#[test]
fn version_json_is_machine_readable_and_product_specific() {
    let output = cli()
        .args(["version", "--json"])
        .output()
        .expect("run ham-cli");
    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("version output is JSON");
    assert_eq!(value["command"], "version");
    assert_eq!(value["cli_version"], "0.3.0");
    assert!(output.stderr.is_empty());
}

#[test]
fn invalid_command_has_deterministic_usage_exit_code() {
    let output = cli().arg("not-a-command").output().expect("run ham-cli");
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert!(stderr.contains("unknown command: not-a-command"));
    assert!(stderr.contains("usage:"));
}

#[test]
fn help_is_non_interactive_and_successful() {
    let output = cli().arg("--help").output().expect("run ham-cli");
    assert!(output.status.success());
    assert!(String::from_utf8(output.stderr)
        .expect("stderr is UTF-8")
        .contains("The CLI is offline-first and never prompts"));
}
