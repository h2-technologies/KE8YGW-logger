use ham_desktop::{
    desktop_dialog_open, desktop_dialog_save, desktop_runtime_config,
    desktop_select_app_data_directory, DesktopCommandError, DesktopDialogKind,
    DesktopDialogRequest, DesktopDialogResult, DialogSpec, NativeDialogBackend,
};
use serde::{Deserialize, Serialize};
use std::{env, path::PathBuf};

#[derive(Debug, Clone, Serialize)]
struct DesktopRuntimePayload {
    app_name: String,
    frontend_dist_dir: PathBuf,
    app_data_dir_env: String,
    hosted_server_url_env: String,
    server_url: String,
    release_requires_dev_server: bool,
    native_dialog_commands: Vec<ham_desktop::NativeDialogCommand>,
}

#[derive(Debug, Clone, Deserialize)]
struct DesktopApiRequest {
    path: String,
    method: Option<String>,
    body: Option<String>,
    server_url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct DesktopApiResponse {
    status: u16,
    content_type: Option<String>,
    body: String,
}

struct RfdDialogBackend;

impl NativeDialogBackend for RfdDialogBackend {
    fn open_file(&self, spec: &DialogSpec) -> Result<Option<PathBuf>, DesktopCommandError> {
        Ok(configure_dialog(spec).pick_file())
    }

    fn save_file(&self, spec: &DialogSpec) -> Result<Option<PathBuf>, DesktopCommandError> {
        Ok(configure_dialog(spec).save_file())
    }

    fn select_directory(&self, spec: &DialogSpec) -> Result<Option<PathBuf>, DesktopCommandError> {
        Ok(configure_dialog(spec).pick_folder())
    }
}

fn configure_dialog(spec: &DialogSpec) -> rfd::FileDialog {
    let mut dialog = rfd::FileDialog::new().set_title(&spec.title);
    if let Some(default_directory) = &spec.default_directory {
        dialog = dialog.set_directory(default_directory);
    }
    if let Some(default_file_name) = &spec.default_file_name {
        dialog = dialog.set_file_name(default_file_name);
    }
    for filter in &spec.filters {
        let extensions = filter
            .extensions
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        dialog = dialog.add_filter(&filter.name, &extensions);
    }
    dialog
}

fn run_dialog(kind: DesktopDialogKind) -> Result<DesktopDialogResult, DesktopCommandError> {
    let backend = RfdDialogBackend;
    let request = DesktopDialogRequest::new(kind);
    match kind {
        DesktopDialogKind::ImportAdif | DesktopDialogKind::ImportBackup => {
            desktop_dialog_open(&backend, request)
        }
        DesktopDialogKind::ExportAdif
        | DesktopDialogKind::ExportBackup
        | DesktopDialogKind::ExportDiagnosticBundle
        | DesktopDialogKind::ExportDivergenceReport => desktop_dialog_save(&backend, request),
        DesktopDialogKind::SelectAppDataDirectory => {
            desktop_select_app_data_directory(&backend, request)
        }
    }
}

fn default_server_url() -> String {
    env::var("HAM_DESKTOP_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:9467".to_owned())
}

fn desktop_api_url(server_url: Option<&str>, path: &str) -> Result<String, DesktopCommandError> {
    if !path.starts_with("/api/") || path.contains('\r') || path.contains('\n') {
        return Err(DesktopCommandError::backend(
            "desktop API proxy only accepts /api/* paths",
        ));
    }
    let server_url = server_url
        .filter(|url| !url.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(default_server_url);
    Ok(format!("{}{}", server_url.trim_end_matches('/'), path))
}

fn proxy_api_response(response: ureq::Response) -> Result<DesktopApiResponse, DesktopCommandError> {
    let status = response.status();
    let content_type = response.header("content-type").map(str::to_owned);
    let body = response
        .into_string()
        .map_err(|error| DesktopCommandError::backend(error.to_string()))?;
    Ok(DesktopApiResponse {
        status,
        content_type,
        body,
    })
}

#[tauri::command]
fn desktop_runtime() -> DesktopRuntimePayload {
    let config = desktop_runtime_config();
    DesktopRuntimePayload {
        app_name: config.app_name,
        frontend_dist_dir: config.frontend_dist_dir,
        app_data_dir_env: config.app_data_dir_env,
        hosted_server_url_env: config.hosted_server_url_env,
        server_url: default_server_url(),
        release_requires_dev_server: config.release_requires_dev_server,
        native_dialog_commands: config.native_dialog_commands,
    }
}

#[tauri::command]
fn desktop_api_request(
    request: DesktopApiRequest,
) -> Result<DesktopApiResponse, DesktopCommandError> {
    let method = request.method.unwrap_or_else(|| "GET".to_owned());
    let method = method.to_ascii_uppercase();
    if !matches!(method.as_str(), "GET" | "POST") {
        return Err(DesktopCommandError::backend(
            "desktop API proxy only supports GET and POST",
        ));
    }
    let url = desktop_api_url(request.server_url.as_deref(), &request.path)?;
    let agent = ureq::AgentBuilder::new().build();
    let http_request = agent.request(&method, &url);
    let response = if let Some(body) = request.body {
        http_request
            .set("Content-Type", "application/json")
            .send_string(&body)
    } else {
        http_request.call()
    };
    match response {
        Ok(response) => proxy_api_response(response),
        Err(ureq::Error::Status(_, response)) => proxy_api_response(response),
        Err(error) => Err(DesktopCommandError::backend(error.to_string())),
    }
}

#[tauri::command]
fn import_adif_dialog() -> Result<DesktopDialogResult, DesktopCommandError> {
    run_dialog(DesktopDialogKind::ImportAdif)
}

#[tauri::command]
fn export_adif_dialog() -> Result<DesktopDialogResult, DesktopCommandError> {
    run_dialog(DesktopDialogKind::ExportAdif)
}

#[tauri::command]
fn export_backup_dialog() -> Result<DesktopDialogResult, DesktopCommandError> {
    run_dialog(DesktopDialogKind::ExportBackup)
}

#[tauri::command]
fn import_backup_dialog() -> Result<DesktopDialogResult, DesktopCommandError> {
    run_dialog(DesktopDialogKind::ImportBackup)
}

#[tauri::command]
fn export_diagnostic_bundle_dialog() -> Result<DesktopDialogResult, DesktopCommandError> {
    run_dialog(DesktopDialogKind::ExportDiagnosticBundle)
}

#[tauri::command]
fn export_divergence_report_dialog() -> Result<DesktopDialogResult, DesktopCommandError> {
    run_dialog(DesktopDialogKind::ExportDivergenceReport)
}

#[tauri::command]
fn select_app_data_directory_dialog() -> Result<DesktopDialogResult, DesktopCommandError> {
    run_dialog(DesktopDialogKind::SelectAppDataDirectory)
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            desktop_runtime,
            desktop_api_request,
            import_adif_dialog,
            export_adif_dialog,
            export_backup_dialog,
            import_backup_dialog,
            export_diagnostic_bundle_dialog,
            export_divergence_report_dialog,
            select_app_data_directory_dialog,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run KE8YGW Logger desktop app");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_desktop_server_url_points_to_local_gui_api() {
        env::remove_var("HAM_DESKTOP_SERVER_URL");
        assert_eq!(default_server_url(), "http://127.0.0.1:9467");
    }

    #[test]
    fn runtime_payload_is_release_bundle_oriented() {
        let payload = desktop_runtime();
        assert!(!payload.release_requires_dev_server);
        assert!(payload
            .native_dialog_commands
            .iter()
            .any(|command| command.command == "import_adif_dialog"));
    }

    #[test]
    fn desktop_api_url_rejects_non_api_paths() {
        let error = desktop_api_url(None, "https://example.com/api/shell").unwrap_err();
        assert_eq!(error.code, "native_dialog_failed");
    }
}
