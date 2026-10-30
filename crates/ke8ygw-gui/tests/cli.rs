use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_ke8ygw");

#[test]
fn version_prints_binary_name_and_workspace_version() {
    let out = Command::new(BIN)
        .arg("--version")
        .output()
        .expect("run binary");
    assert!(out.status.success());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        format!("ke8ygw {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn no_arguments_prints_usage() {
    let out = Command::new(BIN).output().expect("run binary");
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("Usage: ke8ygw "));
}

#[test]
fn unknown_argument_is_a_usage_error() {
    let out = Command::new(BIN)
        .arg("--bogus")
        .output()
        .expect("run binary");
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).starts_with("Usage: ke8ygw "));
}
