use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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

impl DesktopDialogKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ImportAdif => "import-adif",
            Self::ExportAdif => "export-adif",
            Self::ExportBackup => "export-backup",
            Self::ImportBackup => "import-backup",
            Self::ExportDiagnosticBundle => "export-diagnostic-bundle",
            Self::ExportDivergenceReport => "export-divergence-report",
            Self::SelectAppDataDirectory => "select-app-data-directory",
        }
    }

    pub fn expects_open(self) -> bool {
        matches!(self, Self::ImportAdif | Self::ImportBackup)
    }

    pub fn expects_save(self) -> bool {
        matches!(
            self,
            Self::ExportAdif
                | Self::ExportBackup
                | Self::ExportDiagnosticBundle
                | Self::ExportDivergenceReport
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopDialogMode {
    Open,
    Save,
    SelectDirectory,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopDialogRequest {
    pub kind: DesktopDialogKind,
    pub suggested_file_name: Option<String>,
    pub default_directory: Option<PathBuf>,
}

impl DesktopDialogRequest {
    pub fn new(kind: DesktopDialogKind) -> Self {
        Self {
            kind,
            suggested_file_name: None,
            default_directory: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopDialogResult {
    pub kind: DesktopDialogKind,
    pub mode: DesktopDialogMode,
    pub canceled: bool,
    pub selected_path: Option<PathBuf>,
    pub redacted_path_for_logs: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogSpec {
    pub title: String,
    pub default_file_name: Option<String>,
    pub filters: Vec<DialogFileFilter>,
    pub default_directory: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogFileFilter {
    pub name: String,
    pub extensions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopCommandError {
    pub code: String,
    pub message: String,
}

impl DesktopCommandError {
    pub fn invalid_mode(kind: DesktopDialogKind, mode: DesktopDialogMode) -> Self {
        Self {
            code: "invalid_dialog_mode".to_owned(),
            message: format!("{} cannot be used with {mode:?}", kind.as_str()),
        }
    }

    pub fn backend(message: impl Into<String>) -> Self {
        Self {
            code: "native_dialog_failed".to_owned(),
            message: message.into(),
        }
    }
}

impl std::fmt::Display for DesktopCommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for DesktopCommandError {}

pub trait NativeDialogBackend {
    fn open_file(&self, spec: &DialogSpec) -> Result<Option<PathBuf>, DesktopCommandError>;
    fn save_file(&self, spec: &DialogSpec) -> Result<Option<PathBuf>, DesktopCommandError>;
    fn select_directory(&self, spec: &DialogSpec) -> Result<Option<PathBuf>, DesktopCommandError>;
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
                "desktop_select_app_data_directory",
                DesktopDialogKind::SelectAppDataDirectory,
                "Select app data directory",
            ),
        ],
    }
}

pub fn desktop_dialog_open<B: NativeDialogBackend>(
    backend: &B,
    request: DesktopDialogRequest,
) -> Result<DesktopDialogResult, DesktopCommandError> {
    if !request.kind.expects_open() {
        return Err(DesktopCommandError::invalid_mode(
            request.kind,
            DesktopDialogMode::Open,
        ));
    }
    let spec = dialog_spec(&request, DesktopDialogMode::Open);
    let selected_path = backend.open_file(&spec)?;
    Ok(dialog_result(
        request.kind,
        DesktopDialogMode::Open,
        selected_path,
    ))
}

pub fn desktop_dialog_save<B: NativeDialogBackend>(
    backend: &B,
    request: DesktopDialogRequest,
) -> Result<DesktopDialogResult, DesktopCommandError> {
    if !request.kind.expects_save() {
        return Err(DesktopCommandError::invalid_mode(
            request.kind,
            DesktopDialogMode::Save,
        ));
    }
    let spec = dialog_spec(&request, DesktopDialogMode::Save);
    let selected_path = backend.save_file(&spec)?;
    Ok(dialog_result(
        request.kind,
        DesktopDialogMode::Save,
        selected_path,
    ))
}

pub fn desktop_select_app_data_directory<B: NativeDialogBackend>(
    backend: &B,
    request: DesktopDialogRequest,
) -> Result<DesktopDialogResult, DesktopCommandError> {
    if request.kind != DesktopDialogKind::SelectAppDataDirectory {
        return Err(DesktopCommandError::invalid_mode(
            request.kind,
            DesktopDialogMode::SelectDirectory,
        ));
    }
    let spec = dialog_spec(&request, DesktopDialogMode::SelectDirectory);
    let selected_path = backend.select_directory(&spec)?;
    Ok(dialog_result(
        request.kind,
        DesktopDialogMode::SelectDirectory,
        selected_path,
    ))
}

pub fn dialog_spec(request: &DesktopDialogRequest, mode: DesktopDialogMode) -> DialogSpec {
    let default_file_name = request
        .suggested_file_name
        .clone()
        .or_else(|| default_file_name(request.kind));
    DialogSpec {
        title: dialog_title(request.kind, mode).to_owned(),
        default_file_name,
        filters: dialog_filters(request.kind),
        default_directory: request.default_directory.clone(),
    }
}

pub fn redact_path_for_logs(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| format!("<user-selected-path>/{name}"))
        .unwrap_or_else(|| "<user-selected-path>".to_owned())
}

fn dialog_result(
    kind: DesktopDialogKind,
    mode: DesktopDialogMode,
    selected_path: Option<PathBuf>,
) -> DesktopDialogResult {
    DesktopDialogResult {
        kind,
        mode,
        canceled: selected_path.is_none(),
        redacted_path_for_logs: selected_path.as_deref().map(redact_path_for_logs),
        selected_path,
    }
}

fn dialog(command: &str, kind: DesktopDialogKind, purpose: &str) -> NativeDialogCommand {
    NativeDialogCommand {
        command: command.to_owned(),
        kind,
        purpose: purpose.to_owned(),
    }
}

fn dialog_title(kind: DesktopDialogKind, mode: DesktopDialogMode) -> &'static str {
    match (kind, mode) {
        (DesktopDialogKind::ImportAdif, DesktopDialogMode::Open) => "Import ADIF",
        (DesktopDialogKind::ExportAdif, DesktopDialogMode::Save) => "Export ADIF",
        (DesktopDialogKind::ExportBackup, DesktopDialogMode::Save) => "Export Backup",
        (DesktopDialogKind::ImportBackup, DesktopDialogMode::Open) => "Import Backup",
        (DesktopDialogKind::ExportDiagnosticBundle, DesktopDialogMode::Save) => {
            "Export Diagnostic Bundle"
        }
        (DesktopDialogKind::ExportDivergenceReport, DesktopDialogMode::Save) => {
            "Export Divergence Report"
        }
        (DesktopDialogKind::SelectAppDataDirectory, DesktopDialogMode::SelectDirectory) => {
            "Select App Data Directory"
        }
        _ => "Choose File",
    }
}

fn default_file_name(kind: DesktopDialogKind) -> Option<String> {
    match kind {
        DesktopDialogKind::ExportAdif => Some("ke8ygw-logbook.adi".to_owned()),
        DesktopDialogKind::ExportBackup => Some("ke8ygw-backup.json".to_owned()),
        DesktopDialogKind::ExportDiagnosticBundle => Some("ke8ygw-diagnostics.zip".to_owned()),
        DesktopDialogKind::ExportDivergenceReport => {
            Some("ke8ygw-divergence-report.json".to_owned())
        }
        _ => None,
    }
}

fn dialog_filters(kind: DesktopDialogKind) -> Vec<DialogFileFilter> {
    match kind {
        DesktopDialogKind::ImportAdif | DesktopDialogKind::ExportAdif => vec![DialogFileFilter {
            name: "ADIF".to_owned(),
            extensions: vec!["adi".to_owned(), "adif".to_owned()],
        }],
        DesktopDialogKind::ImportBackup
        | DesktopDialogKind::ExportBackup
        | DesktopDialogKind::ExportDivergenceReport => vec![DialogFileFilter {
            name: "JSON".to_owned(),
            extensions: vec!["json".to_owned()],
        }],
        DesktopDialogKind::ExportDiagnosticBundle => vec![DialogFileFilter {
            name: "ZIP".to_owned(),
            extensions: vec!["zip".to_owned()],
        }],
        DesktopDialogKind::SelectAppDataDirectory => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[derive(Default)]
    struct MockDialogBackend {
        next_open: RefCell<Option<PathBuf>>,
        next_save: RefCell<Option<PathBuf>>,
        next_directory: RefCell<Option<PathBuf>>,
    }

    impl NativeDialogBackend for MockDialogBackend {
        fn open_file(&self, _spec: &DialogSpec) -> Result<Option<PathBuf>, DesktopCommandError> {
            Ok(self.next_open.borrow_mut().take())
        }

        fn save_file(&self, _spec: &DialogSpec) -> Result<Option<PathBuf>, DesktopCommandError> {
            Ok(self.next_save.borrow_mut().take())
        }

        fn select_directory(
            &self,
            _spec: &DialogSpec,
        ) -> Result<Option<PathBuf>, DesktopCommandError> {
            Ok(self.next_directory.borrow_mut().take())
        }
    }

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
        assert!(kinds.contains(&DesktopDialogKind::SelectAppDataDirectory));
    }

    #[test]
    fn command_helpers_return_cancel_without_error() {
        let backend = MockDialogBackend::default();
        let result = desktop_dialog_open(
            &backend,
            DesktopDialogRequest::new(DesktopDialogKind::ImportAdif),
        )
        .unwrap();
        assert!(result.canceled);
        assert!(result.selected_path.is_none());
    }

    #[test]
    fn command_helpers_redact_selected_paths_for_logs() {
        let backend = MockDialogBackend {
            next_save: RefCell::new(Some(PathBuf::from(
                "C:/Users/Example/Documents/secret-backup.json",
            ))),
            ..MockDialogBackend::default()
        };
        let result = desktop_dialog_save(
            &backend,
            DesktopDialogRequest::new(DesktopDialogKind::ExportBackup),
        )
        .unwrap();
        assert_eq!(
            result.redacted_path_for_logs.as_deref(),
            Some("<user-selected-path>/secret-backup.json")
        );
        assert!(!result
            .redacted_path_for_logs
            .unwrap()
            .contains("Users/Example"));
    }

    #[test]
    fn invalid_dialog_mode_is_rejected() {
        let backend = MockDialogBackend::default();
        let error = desktop_dialog_save(
            &backend,
            DesktopDialogRequest::new(DesktopDialogKind::ImportAdif),
        )
        .unwrap_err();
        assert_eq!(error.code, "invalid_dialog_mode");
    }

    #[test]
    fn dialog_specs_use_expected_filters_and_defaults() {
        let spec = dialog_spec(
            &DesktopDialogRequest::new(DesktopDialogKind::ExportAdif),
            DesktopDialogMode::Save,
        );
        assert_eq!(
            spec.default_file_name.as_deref(),
            Some("ke8ygw-logbook.adi")
        );
        assert_eq!(spec.filters[0].extensions, vec!["adi", "adif"]);

        let spec = dialog_spec(
            &DesktopDialogRequest::new(DesktopDialogKind::ExportDiagnosticBundle),
            DesktopDialogMode::Save,
        );
        assert_eq!(spec.filters[0].extensions, vec!["zip"]);
    }
}
