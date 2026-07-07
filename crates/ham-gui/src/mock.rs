use ham_core::{RuntimeDiagnosticEvent, RuntimeEventSeverity};
use ham_plugin_sdk::PluginCapability;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MockPlugin {
    pub plugin_id: String,
    pub name: String,
    pub enabled: bool,
    pub requested_permissions: Vec<String>,
}

pub fn mock_plugins() -> Vec<MockPlugin> {
    vec![
        plugin(
            "core.gui",
            "Core GUI Panels",
            true,
            [
                "log.qso.view",
                "log.qso.create",
                "adif.import",
                "adif.export",
                "diagnostics.view_logs",
                "diagnostics.export",
                "diagnostics.upload",
                "sync.lan.discovery",
                "sync.cloud.connect",
                "service.provider.enable",
                "service.provider.disable",
                "service.cache.clear",
                "credential.view_metadata",
                "credential.create",
                "credential.update",
                "credential.delete",
                "credential.use",
                "credential.test",
            ],
        ),
        plugin(
            "plugin.rig-control",
            "Rig Control",
            true,
            [
                "rig.view",
                "rig.control.frequency",
                "rig.control.mode",
                "rig.control.ptt",
                "rig.control.split",
                "rig.read.state",
                "rig.configure",
                "log.qso.suggest_fields",
            ],
        ),
        plugin(
            "plugin.pota-sota",
            "POTA/SOTA Tools",
            true,
            [
                "activation.create",
                "activation.update",
                "activation.end",
                "activation.view",
                "log.qso.create",
                "log.qso.correct",
                "log.qso.note.add",
                "adif.export",
            ],
        ),
        plugin(
            "plugin.callsign-lookup",
            "Callsign Lookup",
            true,
            [
                "lookup.callsign",
                "lookup.entity",
                "lookup.grid",
                "cache.lookup.read",
                "cache.lookup.write",
                "service.cache.read",
                "service.cache.write",
                "service.cache.clear",
                "log.qso.suggest_fields",
            ],
        ),
        plugin(
            "plugin.log-upload",
            "Log Upload Providers",
            false,
            ["adif.export", "upload.log", "upload.confirmation_pull"],
        ),
        plugin(
            "plugin.net-control",
            "Net Control",
            true,
            [
                "net.view",
                "net.template.create",
                "net.template.update",
                "net.session.start",
                "net.session.end",
                "net.checkin.create",
                "net.checkin.update",
                "net.checkin.delete",
                "net.traffic.manage",
                "net.report.export",
            ],
        ),
        plugin(
            "plugin.spotting",
            "Spotting Providers",
            true,
            [
                "spotting.view",
                "spotting.configure",
                "network.external.spotting",
            ],
        ),
        plugin("plugin.maps", "Maps", false, ["map.view"]),
        plugin("plugin.weather", "Weather", false, ["weather.view"]),
        plugin(
            "plugin.propagation",
            "Propagation",
            false,
            ["propagation.view"],
        ),
        plugin("plugin.ai", "AI Assistant", false, ["ai.use"]),
    ]
}

pub fn mock_runtime_events() -> Vec<RuntimeDiagnosticEvent> {
    vec![
        diagnostic("GUI shell attached to in-memory event bus bridge"),
        diagnostic("Loaded default workspace layout registry"),
        diagnostic("Plugin proposal path available through ham-core"),
    ]
}

pub fn capability_labels() -> Vec<String> {
    [
        PluginCapability::QsoCreate,
        PluginCapability::QsoCorrect,
        PluginCapability::QsoDelete,
        PluginCapability::QsoRestore,
        PluginCapability::QsoNoteAdd,
        PluginCapability::QsoViewDeleted,
        PluginCapability::ActivationCreate,
        PluginCapability::ActivationUpdate,
        PluginCapability::ActivationEnd,
        PluginCapability::ActivationCancel,
        PluginCapability::ActivationView,
        PluginCapability::AdifImport,
        PluginCapability::AdifExport,
        PluginCapability::SyncLanDiscovery,
        PluginCapability::SyncLanPull,
        PluginCapability::SyncLanPush,
        PluginCapability::SyncCloudConnect,
        PluginCapability::SyncCloudPull,
        PluginCapability::SyncCloudPush,
        PluginCapability::LookupCallsign,
        PluginCapability::LookupEntity,
        PluginCapability::LookupGrid,
        PluginCapability::LookupCacheRead,
        PluginCapability::LookupCacheWrite,
        PluginCapability::ServiceProviderRegister,
        PluginCapability::ServiceProviderConfigure,
        PluginCapability::ServiceProviderEnable,
        PluginCapability::ServiceProviderDisable,
        PluginCapability::ServiceCacheRead,
        PluginCapability::ServiceCacheWrite,
        PluginCapability::ServiceCacheClear,
        PluginCapability::UploadLog,
        PluginCapability::UploadConfirmationPull,
        PluginCapability::SpottingView,
        PluginCapability::SpottingConfigure,
        PluginCapability::NetworkExternalSpotting,
        PluginCapability::MapView,
        PluginCapability::MapConfigure,
        PluginCapability::WeatherView,
        PluginCapability::PropagationView,
        PluginCapability::CredentialViewMetadata,
        PluginCapability::CredentialCreate,
        PluginCapability::CredentialUpdate,
        PluginCapability::CredentialDelete,
        PluginCapability::CredentialUse,
        PluginCapability::CredentialTest,
        PluginCapability::NetView,
        PluginCapability::NetTemplateCreate,
        PluginCapability::NetTemplateUpdate,
        PluginCapability::NetSessionStart,
        PluginCapability::NetSessionEnd,
        PluginCapability::NetCheckinCreate,
        PluginCapability::NetCheckinUpdate,
        PluginCapability::NetCheckinDelete,
        PluginCapability::NetTrafficManage,
        PluginCapability::NetReportExport,
        PluginCapability::QsoSuggestFields,
        PluginCapability::RigView,
        PluginCapability::RigControlFrequency,
        PluginCapability::RigControlMode,
        PluginCapability::RigControlPtt,
        PluginCapability::RigControlSplit,
        PluginCapability::RigReadState,
        PluginCapability::RigConfigure,
        PluginCapability::DiagnosticsViewLogs,
        PluginCapability::DiagnosticsExport,
        PluginCapability::DiagnosticsUpload,
        PluginCapability::UiPanelRegister,
        PluginCapability::UiCommandRegister,
        PluginCapability::SettingsRead,
        PluginCapability::SettingsWrite,
    ]
    .into_iter()
    .map(|capability| capability.as_str().to_owned())
    .collect()
}

fn plugin<const N: usize>(
    plugin_id: &str,
    name: &str,
    enabled: bool,
    permissions: [&str; N],
) -> MockPlugin {
    MockPlugin {
        plugin_id: plugin_id.to_owned(),
        name: name.to_owned(),
        enabled,
        requested_permissions: permissions.into_iter().map(str::to_owned).collect(),
    }
}

fn diagnostic(message: &str) -> RuntimeDiagnosticEvent {
    RuntimeDiagnosticEvent {
        event_id: Uuid::new_v4(),
        timestamp: chrono::Utc::now(),
        event_type: "diagnostics.demo".to_owned(),
        severity: RuntimeEventSeverity::Info,
        source: "ham-gui-demo".to_owned(),
        source_plugin_id: None,
        correlation_id: Uuid::new_v4(),
        session_id: Uuid::new_v4(),
        device_id: Uuid::new_v4(),
        workspace_id: Some("dashboard".to_owned()),
        payload_summary: message.to_owned(),
        redacted_payload: None,
        error: None,
    }
}
