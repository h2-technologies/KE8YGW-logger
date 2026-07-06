use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{RuntimeEventEnvelope, RuntimeEventSeverity, RUNTIME_LOG_FILE_NAME};

pub const REPORT_FORMAT_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticReportType {
    Basic,
    Sync,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionSummary {
    pub secret_fields_redacted: usize,
    pub private_profile_fields_redacted: usize,
    pub official_log_excluded: bool,
    pub ai_payloads_excluded: bool,
    pub raw_provider_metadata_excluded: bool,
    pub categories_removed: Vec<String>,
}

impl Default for RedactionSummary {
    fn default() -> Self {
        Self {
            secret_fields_redacted: 0,
            private_profile_fields_redacted: 0,
            official_log_excluded: true,
            ai_payloads_excluded: true,
            raw_provider_metadata_excluded: true,
            categories_removed: vec![
                "credentials/tokens/api keys".to_owned(),
                "official QSO logs".to_owned(),
                "full AI prompts/responses".to_owned(),
                "raw lookup/provider metadata".to_owned(),
                "private profile/address fields".to_owned(),
            ],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActionTimelineEntry {
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub severity: RuntimeEventSeverity,
    pub source: String,
    pub source_plugin_id: Option<String>,
    pub correlation_id: Uuid,
    pub workspace_id: Option<String>,
    pub payload_summary: String,
    pub error_summary: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticBundleManifest {
    pub report_format_version: u32,
    pub report_type: DiagnosticReportType,
    pub generated_at: DateTime<Utc>,
    pub app_version: String,
    pub core_version: String,
    pub platform: String,
    pub device_id: Uuid,
    pub session_id: Uuid,
    pub account_id: Option<String>,
    pub included_files: Vec<String>,
    pub bundle_hash: String,
    pub redaction_summary: RedactionSummary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiagnosticBundlePreview {
    pub file_name: String,
    pub report_type: DiagnosticReportType,
    pub included_files: Vec<String>,
    pub redaction_summary: RedactionSummary,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticBundle {
    pub file_name: String,
    pub manifest: DiagnosticBundleManifest,
    pub files: Vec<DiagnosticBundleFile>,
    pub zip_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosticBundleFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct DiagnosticBundleInput {
    pub report_type: DiagnosticReportType,
    pub runtime_log_dir: PathBuf,
    pub runtime_events: Vec<RuntimeEventEnvelope>,
    pub app_version: String,
    pub core_version: String,
    pub device_id: Uuid,
    pub session_id: Uuid,
    pub account_id: Option<String>,
    pub plugins: Value,
    pub sync_status: Option<Value>,
    pub user_notes: String,
}

impl DiagnosticBundleInput {
    pub fn preview(&self) -> DiagnosticBundlePreview {
        let mut files = base_file_list(&self.runtime_log_dir);
        if self.report_type == DiagnosticReportType::Sync {
            files.push("sync-status.json".to_owned());
        }
        files.extend([
            "manifest.json".to_owned(),
            "system-info.json".to_owned(),
            "app-info.json".to_owned(),
            "plugins.json".to_owned(),
            "action-timeline.json".to_owned(),
            "redaction-report.json".to_owned(),
            "user-notes.txt".to_owned(),
        ]);
        files.sort();
        files.dedup();
        DiagnosticBundlePreview {
            file_name: report_file_name(self.report_type),
            report_type: self.report_type,
            included_files: files,
            redaction_summary: RedactionSummary::default(),
        }
    }
}

pub fn build_diagnostic_bundle(input: DiagnosticBundleInput) -> io::Result<DiagnosticBundle> {
    let generated_at = Utc::now();
    let timeline = action_timeline(
        &input.runtime_events,
        generated_at - Duration::minutes(15),
        200,
    );
    let mut redaction_summary = RedactionSummary::default();
    let mut files = Vec::new();

    for file in runtime_log_files(&input.runtime_log_dir)? {
        files.push(file);
    }

    let (plugins, plugin_redaction) = redact_for_report(input.plugins);
    merge_redaction_summary(&mut redaction_summary, &plugin_redaction);
    files.push(json_file("plugins.json", &plugins)?);

    let system_info = json!({
        "platform": platform_name(),
        "os": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "family": std::env::consts::FAMILY,
    });
    files.push(json_file("system-info.json", &system_info)?);

    let app_info = json!({
        "app_version": input.app_version,
        "core_version": input.core_version,
        "report_generated_at": generated_at,
    });
    files.push(json_file("app-info.json", &app_info)?);

    if input.report_type == DiagnosticReportType::Sync {
        let sync_status = input
            .sync_status
            .unwrap_or_else(|| json!({"state": "unknown"}));
        let (sync_status, sync_redaction) = redact_for_report(sync_status);
        merge_redaction_summary(&mut redaction_summary, &sync_redaction);
        files.push(json_file("sync-status.json", &sync_status)?);
    }

    files.push(json_file("action-timeline.json", &timeline)?);
    files.push(DiagnosticBundleFile {
        name: "user-notes.txt".to_owned(),
        bytes: input.user_notes.into_bytes(),
    });

    let redaction_file = json_file("redaction-report.json", &redaction_summary)?;
    files.push(redaction_file);

    let content_hash = bundle_content_hash(&files);
    let mut included_files = files
        .iter()
        .map(|file| file.name.clone())
        .collect::<Vec<_>>();
    included_files.push("manifest.json".to_owned());
    included_files.sort();

    let manifest = DiagnosticBundleManifest {
        report_format_version: REPORT_FORMAT_VERSION,
        report_type: input.report_type,
        generated_at,
        app_version: app_info["app_version"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        core_version: app_info["core_version"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        platform: platform_name(),
        device_id: input.device_id,
        session_id: input.session_id,
        account_id: input.account_id,
        included_files,
        bundle_hash: content_hash,
        redaction_summary,
    };
    files.push(json_file("manifest.json", &manifest)?);
    files.sort_by(|left, right| left.name.cmp(&right.name));
    let zip_bytes = write_zip(&files)?;
    Ok(DiagnosticBundle {
        file_name: report_file_name(input.report_type),
        manifest,
        files,
        zip_bytes,
    })
}

pub fn export_diagnostic_zip(bundle: &DiagnosticBundle, output_path: &Path) -> io::Result<()> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_path, &bundle.zip_bytes)
}

pub fn action_timeline(
    events: &[RuntimeEventEnvelope],
    since: DateTime<Utc>,
    limit: usize,
) -> Vec<ActionTimelineEntry> {
    let mut entries = events
        .iter()
        .filter(|event| event.timestamp >= since)
        .filter(|event| is_timeline_event(event))
        .map(|event| ActionTimelineEntry {
            timestamp: event.timestamp,
            event_type: event.event_type.clone(),
            severity: event.severity,
            source: event.source.clone(),
            source_plugin_id: event.source_plugin_id.clone(),
            correlation_id: event.correlation_id,
            workspace_id: event.workspace_id.clone(),
            payload_summary: event.payload_summary.clone(),
            error_summary: event.error.clone(),
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|event| event.timestamp);
    if entries.len() > limit {
        entries.drain(0..entries.len() - limit);
    }
    entries
}

pub fn redact_for_report(value: Value) -> (Value, RedactionSummary) {
    let mut summary = RedactionSummary::default();
    (redact_value(value, &mut summary), summary)
}

pub fn bundle_content_hash(files: &[DiagnosticBundleFile]) -> String {
    let mut hasher = Sha256::new();
    let mut refs = files.iter().collect::<Vec<_>>();
    refs.sort_by(|left, right| left.name.cmp(&right.name));
    for file in refs {
        hasher.update(file.name.as_bytes());
        hasher.update([0]);
        hasher.update(&file.bytes);
        hasher.update([0]);
    }
    format!("{:x}", hasher.finalize())
}

fn runtime_log_files(directory: &Path) -> io::Result<Vec<DiagnosticBundleFile>> {
    let mut files = Vec::new();
    for name in base_file_list(directory) {
        let path = directory.join(&name);
        if path.exists() {
            files.push(DiagnosticBundleFile {
                name,
                bytes: fs::read(path)?,
            });
        }
    }
    Ok(files)
}

fn base_file_list(directory: &Path) -> Vec<String> {
    let mut files = vec![RUNTIME_LOG_FILE_NAME.to_owned()];
    for index in 1..=5 {
        files.push(format!("{RUNTIME_LOG_FILE_NAME}.{index}"));
    }
    files
        .into_iter()
        .filter(|name| directory.join(name).exists() || name == RUNTIME_LOG_FILE_NAME)
        .collect()
}

fn json_file(name: &str, value: &impl Serialize) -> io::Result<DiagnosticBundleFile> {
    let bytes = serde_json::to_vec_pretty(value).map_err(io::Error::other)?;
    Ok(DiagnosticBundleFile {
        name: name.to_owned(),
        bytes,
    })
}

fn redact_value(value: Value, summary: &mut RedactionSummary) -> Value {
    match value {
        Value::Object(map) => {
            let mut redacted = Map::new();
            for (key, value) in map {
                let key_lc = key.to_ascii_lowercase();
                if is_secret_key(&key_lc) {
                    summary.secret_fields_redacted += 1;
                    redacted.insert(key, Value::String("[REDACTED]".to_owned()));
                } else if is_private_profile_key(&key_lc) {
                    summary.private_profile_fields_redacted += 1;
                    redacted.insert(key, Value::String("[REDACTED]".to_owned()));
                } else if key_lc.contains("raw_metadata") || key_lc.contains("raw_provider") {
                    summary.raw_provider_metadata_excluded = true;
                    redacted.insert(key, Value::String("[REDACTED]".to_owned()));
                } else if key_lc.contains("prompt")
                    || key_lc.contains("completion")
                    || key_lc.contains("response")
                {
                    summary.ai_payloads_excluded = true;
                    redacted.insert(key, Value::String("[REDACTED]".to_owned()));
                } else {
                    redacted.insert(key, redact_value(value, summary));
                }
            }
            Value::Object(redacted)
        }
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(|value| redact_value(value, summary))
                .collect(),
        ),
        other => other,
    }
}

fn is_secret_key(key: &str) -> bool {
    [
        "secret",
        "token",
        "password",
        "api_key",
        "apikey",
        "credential",
        "authorization",
        "session",
        "sync_token",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn is_private_profile_key(key: &str) -> bool {
    [
        "home_address",
        "address",
        "street",
        "profile_notes",
        "private_notes",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn is_timeline_event(event: &RuntimeEventEnvelope) -> bool {
    event.severity >= RuntimeEventSeverity::Warn
        || matches!(
            event.category(),
            "ui" | "plugin"
                | "sync"
                | "rig"
                | "network"
                | "proposal"
                | "projection"
                | "diagnostics"
                | "app"
        )
}

fn report_file_name(report_type: DiagnosticReportType) -> String {
    let kind = match report_type {
        DiagnosticReportType::Basic => "basic",
        DiagnosticReportType::Sync => "sync",
    };
    format!(
        "ham-report-{}-{}-{kind}.zip",
        Utc::now().format("%Y%m%dT%H%M%SZ"),
        &Uuid::new_v4().to_string()[..8]
    )
}

fn platform_name() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn merge_redaction_summary(target: &mut RedactionSummary, source: &RedactionSummary) {
    target.secret_fields_redacted += source.secret_fields_redacted;
    target.private_profile_fields_redacted += source.private_profile_fields_redacted;
}

fn write_zip(files: &[DiagnosticBundleFile]) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let mut central = Vec::new();
    for file in files {
        let offset = output.len() as u32;
        let crc = crc32(&file.bytes);
        let name = file.name.as_bytes();
        write_u32(&mut output, 0x0403_4b50)?;
        write_u16(&mut output, 20)?;
        write_u16(&mut output, 0)?;
        write_u16(&mut output, 0)?;
        write_u16(&mut output, 0)?;
        write_u16(&mut output, 0)?;
        write_u32(&mut output, crc)?;
        write_u32(&mut output, file.bytes.len() as u32)?;
        write_u32(&mut output, file.bytes.len() as u32)?;
        write_u16(&mut output, name.len() as u16)?;
        write_u16(&mut output, 0)?;
        output.write_all(name)?;
        output.write_all(&file.bytes)?;

        write_u32(&mut central, 0x0201_4b50)?;
        write_u16(&mut central, 20)?;
        write_u16(&mut central, 20)?;
        write_u16(&mut central, 0)?;
        write_u16(&mut central, 0)?;
        write_u16(&mut central, 0)?;
        write_u16(&mut central, 0)?;
        write_u32(&mut central, crc)?;
        write_u32(&mut central, file.bytes.len() as u32)?;
        write_u32(&mut central, file.bytes.len() as u32)?;
        write_u16(&mut central, name.len() as u16)?;
        write_u16(&mut central, 0)?;
        write_u16(&mut central, 0)?;
        write_u16(&mut central, 0)?;
        write_u16(&mut central, 0)?;
        write_u32(&mut central, 0)?;
        write_u32(&mut central, offset)?;
        central.write_all(name)?;
    }
    let central_offset = output.len() as u32;
    output.write_all(&central)?;
    write_u32(&mut output, 0x0605_4b50)?;
    write_u16(&mut output, 0)?;
    write_u16(&mut output, 0)?;
    write_u16(&mut output, files.len() as u16)?;
    write_u16(&mut output, files.len() as u16)?;
    write_u32(&mut output, central.len() as u32)?;
    write_u32(&mut output, central_offset)?;
    write_u16(&mut output, 0)?;
    Ok(output)
}

fn write_u16(output: &mut Vec<u8>, value: u16) -> io::Result<()> {
    output.write_all(&value.to_le_bytes())
}

fn write_u32(output: &mut Vec<u8>, value: u32) -> io::Result<()> {
    output.write_all(&value.to_le_bytes())
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RuntimeEventSeverity;

    #[test]
    fn redaction_helper_removes_secret_like_fields() {
        let (value, summary) = redact_for_report(json!({
            "api_key": "secret",
            "profile": {"home_address": "123 Main", "callsign": "K1ABC"},
            "raw_provider_state": {"token": "abc"}
        }));
        assert_eq!(value["api_key"], "[REDACTED]");
        assert_eq!(value["profile"]["home_address"], "[REDACTED]");
        assert_eq!(value["profile"]["callsign"], "K1ABC");
        assert!(summary.secret_fields_redacted >= 1);
        assert!(summary.private_profile_fields_redacted >= 1);
    }

    #[test]
    fn action_timeline_filters_recent_important_events() {
        let mut old = RuntimeEventEnvelope::new(
            "app.old",
            RuntimeEventSeverity::Info,
            "test",
            None,
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            None,
            "old",
            None,
            None,
        );
        old.timestamp = Utc::now() - Duration::minutes(30);
        let mut recent = old.clone();
        recent.timestamp = Utc::now();
        recent.event_type = "sync.pull.failed".to_owned();
        recent.severity = RuntimeEventSeverity::Error;
        let entries = action_timeline(&[old, recent], Utc::now() - Duration::minutes(1), 10);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].event_type, "sync.pull.failed");
    }

    #[test]
    fn basic_report_includes_expected_files_and_manifest() {
        let dir = unique_temp_dir();
        fs::write(dir.join(RUNTIME_LOG_FILE_NAME), b"{}\n").unwrap();
        let bundle =
            build_diagnostic_bundle(sample_input(DiagnosticReportType::Basic, dir.clone()))
                .unwrap();
        let names = bundle
            .files
            .iter()
            .map(|file| file.name.as_str())
            .collect::<Vec<_>>();
        assert!(names.contains(&"manifest.json"));
        assert!(names.contains(&"runtime-events.jsonl"));
        assert!(names.contains(&"action-timeline.json"));
        assert!(!names.contains(&"sync-status.json"));
        assert!(!bundle.manifest.bundle_hash.is_empty());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn sync_report_includes_sync_status() {
        let dir = unique_temp_dir();
        let bundle =
            build_diagnostic_bundle(sample_input(DiagnosticReportType::Sync, dir.clone())).unwrap();
        assert!(bundle
            .files
            .iter()
            .any(|file| file.name == "sync-status.json"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn zip_export_has_zip_structure() {
        let dir = unique_temp_dir();
        let bundle =
            build_diagnostic_bundle(sample_input(DiagnosticReportType::Basic, dir.clone()))
                .unwrap();
        let output = dir.join("report.zip");
        export_diagnostic_zip(&bundle, &output).unwrap();
        let bytes = fs::read(output).unwrap();
        assert_eq!(&bytes[..4], [0x50, 0x4b, 0x03, 0x04]);
        assert!(bytes
            .windows("manifest.json".len())
            .any(|w| w == b"manifest.json"));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bundle_hash_changes_with_content() {
        let left = bundle_content_hash(&[DiagnosticBundleFile {
            name: "a.txt".to_owned(),
            bytes: b"one".to_vec(),
        }]);
        let right = bundle_content_hash(&[DiagnosticBundleFile {
            name: "a.txt".to_owned(),
            bytes: b"two".to_vec(),
        }]);
        assert_ne!(left, right);
    }

    fn sample_input(
        report_type: DiagnosticReportType,
        runtime_log_dir: PathBuf,
    ) -> DiagnosticBundleInput {
        DiagnosticBundleInput {
            report_type,
            runtime_log_dir,
            runtime_events: Vec::new(),
            app_version: "0.1.0".to_owned(),
            core_version: "0.1.0".to_owned(),
            device_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            account_id: Some("account".to_owned()),
            plugins: json!([{"plugin_id": "core.gui", "enabled": true}]),
            sync_status: Some(json!({"state": "ok", "sync_token": "secret"})),
            user_notes: "notes".to_owned(),
        }
    }

    fn unique_temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ham-report-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
