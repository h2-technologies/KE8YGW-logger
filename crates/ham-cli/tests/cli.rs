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
fn json_flag_can_precede_the_command() {
    let output = cli()
        .args(["--json", "version"])
        .output()
        .expect("run ham-cli");
    assert!(output.status.success());
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("version output is JSON");
    assert_eq!(value["command"], "version");
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
    let stderr = String::from_utf8(output.stderr).expect("stderr is UTF-8");
    assert!(stderr.contains("The logging commands are offline-first and never prompt"));
    assert!(stderr.contains("ham-cli account status [--json]"));
    assert!(stderr.contains("ham-cli account login <email>"));
    assert!(stderr.contains("never print or persist those tokens"));
}

#[test]
fn account_subcommands_have_deterministic_usage_errors() {
    for args in [
        &["account"][..],
        &["account", "not-a-subcommand"][..],
        &["account", "login"][..],
        &["account", "revoke-device", "not-a-uuid"][..],
        &["account", "delete"][..],
        &["account", "status", "unexpected"][..],
    ] {
        let output = cli().args(args).output().expect("run ham-cli");
        assert_eq!(output.status.code(), Some(2), "arguments: {args:?}");
        assert!(String::from_utf8(output.stderr)
            .expect("stderr is UTF-8")
            .contains("usage:"));
    }
}

#[test]
fn missing_or_extra_arguments_have_usage_exit_code() {
    for args in [
        &["import-adif"][..],
        &["export-adif"][..],
        &["verify-chain", "unexpected"][..],
    ] {
        let output = cli().args(args).output().expect("run ham-cli");
        assert_eq!(output.status.code(), Some(2), "arguments: {args:?}");
        assert!(String::from_utf8(output.stderr)
            .expect("stderr is UTF-8")
            .contains("usage:"));
    }
}
