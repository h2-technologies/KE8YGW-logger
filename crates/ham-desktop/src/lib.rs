use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopRuntimeConfig {
    pub app_name: String,
    pub frontend_dist_dir: PathBuf,
    pub app_data_dir_env: String,
    pub hosted_server_url_env: String,
    pub release_requires_dev_server: bool,
    pub native_dialog_commands: Vec<NativeDialogCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeDialogCommand {
    pub command: String,
    pub kind: DesktopDialogKind,
    pub purpose: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DesktopDialogKind {
    ImportAdif,
    ExportAdif,
    ExportBackup,
    ImportBackup,
    ExportDiagnosticBundle,
    ExportDivergenceReport,
    SelectAppDataDirectory,
}

pub fn desktop_runtime_config() -> DesktopRuntimeConfig {
    DesktopRuntimeConfig {
        app_name: "KE8YGW Logger".to_owned(),
        frontend_dist_dir: PathBuf::from("../ham-gui/web"),
        app_data_dir_env: "HAM_DESKTOP_APP_DATA_DIR".to_owned(),
        hosted_server_url_env: "HAM_DESKTOP_SERVER_URL".to_owned(),
        release_requires_dev_server: false,
        native_dialog_commands: vec![
            dialog(
                "desktop_dialog_open",
                DesktopDialogKind::ImportAdif,
                "Import ADIF",
            ),
            dialog(
                "desktop_dialog_save",
                DesktopDialogKind::ExportAdif,
                "Export ADIF",
            ),
            dialog(
                "desktop_dialog_save",
                DesktopDialogKind::ExportBackup,
                "Export backup",
            ),
            dialog(
                "desktop_dialog_open",
                DesktopDialogKind::ImportBackup,
                "Import backup",
            ),
            dialog(
                "desktop_dialog_save",
                DesktopDialogKind::ExportDiagnosticBundle,
                "Export diagnostic bundle",
            ),
            dialog(
                "desktop_dialog_save",
                DesktopDialogKind::ExportDivergenceReport,
                "Export divergence report",
            ),
            dialog(
                "desktop_dialog_open",
                DesktopDialogKind::SelectAppDataDirectory,
                "Select app data directory",
            ),
        ],
    }
}

fn dialog(command: &str, kind: DesktopDialogKind, purpose: &str) -> NativeDialogCommand {
    NativeDialogCommand {
        command: command.to_owned(),
        kind,
        purpose: purpose.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_release_does_not_require_dev_server() {
        let config = desktop_runtime_config();
        assert!(!config.release_requires_dev_server);
    }

    #[test]
    fn native_dialog_contract_covers_required_flows() {
        let config = desktop_runtime_config();
        let kinds = config
            .native_dialog_commands
            .iter()
            .map(|command| command.kind)
            .collect::<Vec<_>>();

        assert!(kinds.contains(&DesktopDialogKind::ImportAdif));
        assert!(kinds.contains(&DesktopDialogKind::ExportAdif));
        assert!(kinds.contains(&DesktopDialogKind::ExportBackup));
        assert!(kinds.contains(&DesktopDialogKind::ImportBackup));
        assert!(kinds.contains(&DesktopDialogKind::ExportDiagnosticBundle));
        assert!(kinds.contains(&DesktopDialogKind::ExportDivergenceReport));
    }
}
