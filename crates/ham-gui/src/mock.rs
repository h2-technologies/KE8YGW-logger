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
            ["logbook.read", "diagnostics.read"],
        ),
        plugin(
            "plugin.rig-control",
            "Rig Control",
            false,
            ["rig.read", "rig.control"],
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
                "log.qso.suggest_fields",
            ],
        ),
        plugin("plugin.maps", "Maps", false, ["map.view"]),
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
        PluginCapability::ActivationCreate,
        PluginCapability::ActivationUpdate,
        PluginCapability::ActivationEnd,
        PluginCapability::ActivationView,
        PluginCapability::AdifExport,
        PluginCapability::LookupCallsign,
        PluginCapability::LookupEntity,
        PluginCapability::LookupGrid,
        PluginCapability::LookupCacheRead,
        PluginCapability::LookupCacheWrite,
        PluginCapability::QsoSuggestFields,
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
