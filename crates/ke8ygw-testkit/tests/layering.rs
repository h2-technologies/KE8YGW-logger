//! Enforces the crate layering from section 3 of docs/V1_DESIGN_SPEC.md.
//!
//! `ALLOWED` is the single place the permitted edges between workspace crates
//! are written down. A crate may depend on fewer crates than listed, never on
//! one that is not listed. Adding an edge is an architecture change: update
//! the spec first, then this table.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const ALLOWED: &[(&str, &[&str])] = &[
    ("ke8ygw-types", &[]),
    ("ke8ygw-proposal", &["ke8ygw-types"]),
    ("ke8ygw-hlc", &["ke8ygw-types"]),
    ("ke8ygw-identity", &["ke8ygw-types"]),
    (
        "ke8ygw-eventlog",
        &["ke8ygw-types", "ke8ygw-hlc", "ke8ygw-identity"],
    ),
    (
        "ke8ygw-merge",
        &[
            "ke8ygw-types",
            "ke8ygw-hlc",
            "ke8ygw-identity",
            "ke8ygw-eventlog",
        ],
    ),
    (
        "ke8ygw-sync",
        &[
            "ke8ygw-types",
            "ke8ygw-hlc",
            "ke8ygw-identity",
            "ke8ygw-eventlog",
        ],
    ),
    ("ke8ygw-netsetup", &[]),
    ("ke8ygw-contest", &["ke8ygw-types"]),
    ("ke8ygw-export", &["ke8ygw-types", "ke8ygw-contest"]),
    (
        "ke8ygw-projector",
        &[
            "ke8ygw-types",
            "ke8ygw-eventlog",
            "ke8ygw-merge",
            "ke8ygw-contest",
        ],
    ),
    (
        "ke8ygw-connector-n3fjp",
        &["ke8ygw-types", "ke8ygw-proposal"],
    ),
    (
        "ke8ygw-connector-flrig",
        &["ke8ygw-types", "ke8ygw-proposal"],
    ),
    (
        "ke8ygw-node",
        &[
            "ke8ygw-types",
            "ke8ygw-proposal",
            "ke8ygw-hlc",
            "ke8ygw-identity",
            "ke8ygw-eventlog",
            "ke8ygw-merge",
            "ke8ygw-sync",
            "ke8ygw-netsetup",
            "ke8ygw-projector",
            "ke8ygw-contest",
            "ke8ygw-export",
            "ke8ygw-connector-n3fjp",
            "ke8ygw-connector-flrig",
        ],
    ),
    (
        "ke8ygw-gui",
        &["ke8ygw-types", "ke8ygw-proposal", "ke8ygw-node"],
    ),
    (
        "ke8ygw-tui",
        &["ke8ygw-types", "ke8ygw-proposal", "ke8ygw-node"],
    ),
    (
        "ke8ygw-relay",
        &[
            "ke8ygw-types",
            "ke8ygw-identity",
            "ke8ygw-eventlog",
            "ke8ygw-sync",
        ],
    ),
    (
        "ke8ygw-testkit",
        &[
            "ke8ygw-types",
            "ke8ygw-proposal",
            "ke8ygw-hlc",
            "ke8ygw-identity",
            "ke8ygw-eventlog",
            "ke8ygw-merge",
            "ke8ygw-sync",
            "ke8ygw-projector",
            "ke8ygw-contest",
            "ke8ygw-export",
        ],
    ),
];

/// The one crate allowed `unsafe`, in its OS-specific modules only.
const UNSAFE_ALLOWED: &str = "ke8ygw-netsetup";

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("testkit lives at crates/ke8ygw-testkit")
        .to_path_buf()
}

/// Every `crates/<name>/Cargo.toml`, keyed by directory name.
fn manifests() -> BTreeMap<String, String> {
    let crates = workspace_root().join("crates");
    fs::read_dir(&crates)
        .expect("read crates/")
        .map(|entry| entry.expect("read crates/ entry").path())
        .filter(|dir| dir.join("Cargo.toml").is_file())
        .map(|dir| {
            let name = dir.file_name().unwrap().to_string_lossy().into_owned();
            let text = fs::read_to_string(dir.join("Cargo.toml")).expect("read Cargo.toml");
            // Windows checkouts may use CRLF line endings.
            (name, text.replace("\r\n", "\n"))
        })
        .collect()
}

/// Workspace crates named in the given dependency tables of a manifest.
/// A deliberately small line scanner: our manifests write one dependency per
/// line, as `name.workspace = true` or `name = { ... }`.
fn internal_deps(manifest: &str, tables: &[&str]) -> BTreeSet<String> {
    let mut deps = BTreeSet::new();
    let mut in_table = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            let header = line.trim_matches(|c| c == '[' || c == ']');
            let header = header.rsplit('.').next().unwrap_or(header);
            in_table = tables.contains(&header);
            continue;
        }
        if !in_table {
            continue;
        }
        let key = line.split('=').next().unwrap_or("").trim();
        let key = key.strip_suffix(".workspace").unwrap_or(key);
        if key.starts_with("ke8ygw-") {
            deps.insert(key.to_owned());
        }
    }
    deps
}

fn allowed() -> BTreeMap<&'static str, BTreeSet<&'static str>> {
    ALLOWED
        .iter()
        .map(|(name, deps)| (*name, deps.iter().copied().collect()))
        .collect()
}

#[test]
fn every_crate_is_in_the_layering_table_and_the_workspace() {
    let allowed = allowed();
    let root = fs::read_to_string(workspace_root().join("Cargo.toml")).expect("read root manifest");
    for name in manifests().keys() {
        assert!(
            allowed.contains_key(name.as_str()),
            "{name} is missing from ALLOWED in tests/layering.rs"
        );
        assert!(
            root.contains(&format!("\"crates/{name}\"")),
            "{name} is not a workspace member"
        );
    }
    assert_eq!(
        manifests().len(),
        allowed.len(),
        "ALLOWED lists a crate that does not exist under crates/"
    );
}

#[test]
fn crates_depend_only_on_allowed_workspace_crates() {
    let allowed = allowed();
    let mut violations = Vec::new();
    for (name, manifest) in manifests() {
        let permitted = &allowed[name.as_str()];
        for dep in internal_deps(&manifest, &["dependencies", "build-dependencies"]) {
            if !permitted.contains(dep.as_str()) {
                violations.push(format!("{name} -> {dep}"));
            }
        }
        // Tests may also pull in the testkit.
        for dep in internal_deps(&manifest, &["dev-dependencies"]) {
            if dep != "ke8ygw-testkit" && !permitted.contains(dep.as_str()) {
                violations.push(format!("{name} -> {dep} (dev)"));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "edges not allowed by the spec: {violations:?}"
    );
}

#[test]
fn only_netsetup_may_use_unsafe() {
    for (name, manifest) in manifests() {
        if name == UNSAFE_ALLOWED {
            assert!(
                manifest.contains("unsafe_code = \"deny\""),
                "{name} must deny unsafe_code so only opted-in modules can use it"
            );
        } else {
            assert!(
                manifest.contains("[lints]\nworkspace = true"),
                "{name} must inherit the workspace lints, which forbid unsafe_code"
            );
        }
    }
}

#[test]
fn scanner_reads_both_dependency_spellings() {
    let manifest = "[package]\nname = \"x\"\n\n[dependencies]\nke8ygw-types.workspace = true\nke8ygw-hlc = { path = \"../ke8ygw-hlc\" }\nserde = \"1\"\n\n[target.'cfg(unix)'.dependencies]\nke8ygw-sync.workspace = true\n\n[dev-dependencies]\nke8ygw-testkit.workspace = true\n";
    let deps = internal_deps(manifest, &["dependencies"]);
    assert_eq!(
        deps.into_iter().collect::<Vec<_>>(),
        ["ke8ygw-hlc", "ke8ygw-sync", "ke8ygw-types"]
    );
}
