use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceId {
    Dashboard,
    CasualLogger,
    PotaSota,
    Maps,
    Awards,
    NetControl,
    EmComm,
    Contesting,
}

impl WorkspaceId {
    pub const ALL: [Self; 8] = [
        Self::Dashboard,
        Self::CasualLogger,
        Self::PotaSota,
        Self::Maps,
        Self::Awards,
        Self::NetControl,
        Self::EmComm,
        Self::Contesting,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::CasualLogger => "Casual Logger",
            Self::PotaSota => "POTA/SOTA",
            Self::Maps => "Maps",
            Self::Awards => "Awards",
            Self::NetControl => "Net Control",
            Self::EmComm => "EmComm",
            Self::Contesting => "Contesting",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceDefinition {
    pub id: WorkspaceId,
    pub title: String,
    pub description: String,
    pub layout: WorkspaceLayout,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkspaceLayout {
    pub workspace_id: WorkspaceId,
    pub placements: Vec<PanelPlacement>,
    pub dockable_movement_todo: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelPlacement {
    pub panel_id: String,
    pub region: PanelRegion,
    pub order: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PanelRegion {
    Center,
    RightInspector,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelDefinition {
    pub id: String,
    pub title: String,
    pub source: String,
    pub required_permissions: Vec<String>,
    pub supported_workspaces: Vec<WorkspaceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiShellState {
    pub active_workspace: WorkspaceId,
    pub workspaces: Vec<WorkspaceDefinition>,
    pub panels: Vec<PanelDefinition>,
}

impl GuiShellState {
    pub fn default_shell() -> Self {
        Self {
            active_workspace: WorkspaceId::Dashboard,
            workspaces: default_workspaces(),
            panels: default_panel_registry(),
        }
    }
}

pub fn default_workspaces() -> Vec<WorkspaceDefinition> {
    WorkspaceId::ALL
        .into_iter()
        .map(|id| WorkspaceDefinition {
            id,
            title: id.title().to_owned(),
            description: workspace_description(id).to_owned(),
            layout: default_layout(id),
        })
        .collect()
}

pub fn default_panel_registry() -> Vec<PanelDefinition> {
    vec![
        panel(
            "recent-qsos",
            "Recent QSOs",
            "core.gui",
            ["log.qso.view"],
            WorkspaceId::ALL,
        ),
        panel(
            "callsign-entry",
            "Callsign Entry",
            "core.gui",
            ["qso.propose"],
            [
                WorkspaceId::Dashboard,
                WorkspaceId::CasualLogger,
                WorkspaceId::PotaSota,
                WorkspaceId::NetControl,
                WorkspaceId::Contesting,
            ],
        ),
        panel(
            "rig-control",
            "Rig Control",
            "plugin.rig-control",
            ["rig.view", "rig.read.state", "rig.configure"],
            WorkspaceId::ALL,
        ),
        panel(
            "sync-status",
            "Sync Status",
            "core.sync",
            ["sync.lan.discovery"],
            WorkspaceId::ALL,
        ),
        panel(
            "event-bus-monitor",
            "Event Bus Monitor",
            "core.diagnostics",
            ["diagnostics.view_logs"],
            WorkspaceId::ALL,
        ),
        panel(
            "map-placeholder",
            "Map Placeholder",
            "plugin.maps",
            ["map.view"],
            [
                WorkspaceId::Dashboard,
                WorkspaceId::PotaSota,
                WorkspaceId::Maps,
                WorkspaceId::EmComm,
            ],
        ),
        panel(
            "interactive-map",
            "Interactive Map",
            "plugin.maps",
            ["map.view"],
            [WorkspaceId::Maps],
        ),
        panel(
            "map-layers",
            "Layers",
            "plugin.maps",
            ["map.view"],
            [WorkspaceId::Maps],
        ),
        panel(
            "map-selected-object",
            "Selected Object",
            "plugin.maps",
            ["map.view"],
            [WorkspaceId::Maps],
        ),
        panel(
            "map-search",
            "Map Search",
            "plugin.maps",
            ["map.view"],
            [WorkspaceId::Maps],
        ),
        panel(
            "map-filters",
            "Map Filters",
            "plugin.maps",
            ["map.view"],
            [WorkspaceId::Maps],
        ),
        panel(
            "propagation",
            "Propagation",
            "plugin.propagation",
            ["propagation.view"],
            [WorkspaceId::Maps, WorkspaceId::Dashboard],
        ),
        panel(
            "weather",
            "Weather",
            "plugin.weather",
            ["weather.view"],
            [WorkspaceId::Maps, WorkspaceId::EmComm],
        ),
        panel(
            "activation-setup",
            "Activation Setup",
            "plugin.pota-sota",
            ["activation.create", "activation.update", "activation.end"],
            [WorkspaceId::PotaSota],
        ),
        panel(
            "activation-progress",
            "Activation Progress",
            "plugin.pota-sota",
            ["activation.view"],
            [WorkspaceId::PotaSota],
        ),
        panel(
            "activation-recent-qsos",
            "Activation Recent QSOs",
            "plugin.pota-sota",
            ["activation.view", "log.qso.view"],
            [WorkspaceId::PotaSota],
        ),
        panel(
            "portable-logger-entry",
            "Portable Logger Entry",
            "plugin.pota-sota",
            ["log.qso.create"],
            [WorkspaceId::PotaSota],
        ),
        panel(
            "spots-alerts",
            "Spots/Alerts",
            "plugin.spotting",
            ["spotting.view"],
            [WorkspaceId::PotaSota],
        ),
        panel(
            "dx-cluster",
            "DX Cluster",
            "plugin.spotting",
            ["spotting.view"],
            [
                WorkspaceId::Dashboard,
                WorkspaceId::CasualLogger,
                WorkspaceId::Contesting,
            ],
        ),
        panel(
            "ai-assistant",
            "AI Assistant",
            "plugin.ai",
            ["ai.use"],
            WorkspaceId::ALL,
        ),
        panel(
            "plugin-permissions",
            "Plugin Permissions",
            "core.plugins",
            ["service.provider.enable"],
            WorkspaceId::ALL,
        ),
        panel(
            "service-providers",
            "Service Providers",
            "core.services",
            ["service.provider.enable", "service.cache.read"],
            WorkspaceId::ALL,
        ),
        panel(
            "credential-manager",
            "Credential Manager",
            "core.credentials",
            ["credential.view_metadata"],
            WorkspaceId::ALL,
        ),
        panel(
            "station-summary",
            "Station Summary",
            "core.station",
            ["station.profile.view"],
            WorkspaceId::ALL,
        ),
        panel(
            "station-profiles",
            "Station Profiles",
            "core.station",
            ["station.profile.view"],
            WorkspaceId::ALL,
        ),
        panel(
            "equipment-manager",
            "Equipment Manager",
            "core.station",
            ["station.equipment.view"],
            WorkspaceId::ALL,
        ),
        panel(
            "awards-summary",
            "Awards",
            "core.awards",
            ["log.qso.view"],
            [WorkspaceId::Awards, WorkspaceId::Dashboard],
        ),
        panel(
            "global-search",
            "Advanced Search",
            "core.search",
            ["log.qso.view"],
            [
                WorkspaceId::Awards,
                WorkspaceId::CasualLogger,
                WorkspaceId::Dashboard,
            ],
        ),
        panel(
            "uploads",
            "Uploads",
            "plugin.log-upload",
            ["upload.status.view"],
            [WorkspaceId::Awards, WorkspaceId::Dashboard],
        ),
        panel(
            "diagnostic-reports",
            "Diagnostic Reports",
            "core.diagnostics",
            ["diagnostics.view_logs"],
            WorkspaceId::ALL,
        ),
        panel(
            "net-session-control",
            "Net Session Control",
            "plugin.net-control",
            ["net.session.start", "net.session.end"],
            [WorkspaceId::NetControl],
        ),
        panel(
            "net-checkin-entry",
            "Check-In Entry",
            "plugin.net-control",
            ["net.checkin.create"],
            [WorkspaceId::NetControl],
        ),
        panel(
            "net-checkin-roster",
            "Check-In Roster",
            "plugin.net-control",
            ["net.view"],
            [WorkspaceId::NetControl],
        ),
        panel(
            "net-traffic-queue",
            "Traffic Queue",
            "plugin.net-control",
            ["net.traffic.manage"],
            [WorkspaceId::NetControl],
        ),
        panel(
            "net-report",
            "Net Report",
            "plugin.net-control",
            ["net.report.export"],
            [WorkspaceId::NetControl],
        ),
    ]
}

fn workspace_description(id: WorkspaceId) -> &'static str {
    match id {
        WorkspaceId::Dashboard => "Operational overview and platform health.",
        WorkspaceId::CasualLogger => "General QSO entry and recent contact context.",
        WorkspaceId::PotaSota => "Activation planning, map context, and portable logging.",
        WorkspaceId::Maps => "Map, propagation, weather, grid, and station geography.",
        WorkspaceId::Awards => "Award progress, advanced search, and upload queue context.",
        WorkspaceId::NetControl => "Directed net workflow placeholders.",
        WorkspaceId::EmComm => "Emergency communications coordination placeholders.",
        WorkspaceId::Contesting => "Contest operating surface placeholders.",
    }
}

fn default_layout(id: WorkspaceId) -> WorkspaceLayout {
    let placements = match id {
        WorkspaceId::Dashboard => vec![
            place("recent-qsos", PanelRegion::Center, 10),
            place("sync-status", PanelRegion::Center, 20),
            place("event-bus-monitor", PanelRegion::Bottom, 10),
            place("diagnostic-reports", PanelRegion::RightInspector, 10),
            place("service-providers", PanelRegion::RightInspector, 20),
            place("credential-manager", PanelRegion::RightInspector, 25),
            place("awards-summary", PanelRegion::RightInspector, 30),
        ],
        WorkspaceId::CasualLogger => vec![
            place("station-summary", PanelRegion::RightInspector, 5),
            place("callsign-entry", PanelRegion::Center, 10),
            place("recent-qsos", PanelRegion::Center, 20),
            place("rig-control", PanelRegion::RightInspector, 10),
            place("global-search", PanelRegion::Bottom, 5),
            place("dx-cluster", PanelRegion::Bottom, 10),
        ],
        WorkspaceId::PotaSota => vec![
            place("station-summary", PanelRegion::RightInspector, 5),
            place("activation-setup", PanelRegion::Center, 10),
            place("portable-logger-entry", PanelRegion::Center, 20),
            place("activation-progress", PanelRegion::RightInspector, 10),
            place("rig-control", PanelRegion::RightInspector, 20),
            place("activation-recent-qsos", PanelRegion::Bottom, 10),
            place("spots-alerts", PanelRegion::Bottom, 20),
        ],
        WorkspaceId::Maps => vec![
            place("interactive-map", PanelRegion::Center, 10),
            place("map-search", PanelRegion::Center, 20),
            place("map-layers", PanelRegion::RightInspector, 10),
            place("map-selected-object", PanelRegion::RightInspector, 20),
            place("station-summary", PanelRegion::RightInspector, 30),
            place("map-filters", PanelRegion::Bottom, 10),
            place("propagation", PanelRegion::Bottom, 20),
            place("weather", PanelRegion::Bottom, 30),
        ],
        WorkspaceId::Awards => vec![
            place("awards-summary", PanelRegion::Center, 10),
            place("global-search", PanelRegion::Center, 20),
            place("uploads", PanelRegion::RightInspector, 10),
            place("recent-qsos", PanelRegion::Bottom, 10),
        ],
        WorkspaceId::NetControl => vec![
            place("net-session-control", PanelRegion::Center, 10),
            place("net-checkin-entry", PanelRegion::Center, 20),
            place("net-checkin-roster", PanelRegion::Center, 30),
            place("net-traffic-queue", PanelRegion::RightInspector, 10),
            place("net-report", PanelRegion::Bottom, 10),
        ],
        WorkspaceId::EmComm => vec![
            place("map-placeholder", PanelRegion::Center, 10),
            place("sync-status", PanelRegion::Center, 20),
            place("diagnostic-reports", PanelRegion::RightInspector, 10),
        ],
        WorkspaceId::Contesting => vec![
            place("callsign-entry", PanelRegion::Center, 10),
            place("dx-cluster", PanelRegion::Center, 20),
            place("rig-control", PanelRegion::RightInspector, 10),
        ],
    };

    WorkspaceLayout {
        workspace_id: id,
        placements,
        dockable_movement_todo:
            "Future: persist user-controlled dock movement and custom layouts through core settings."
                .to_owned(),
    }
}

fn panel<const N: usize, const M: usize>(
    id: &str,
    title: &str,
    source: &str,
    permissions: [&str; N],
    workspaces: [WorkspaceId; M],
) -> PanelDefinition {
    PanelDefinition {
        id: id.to_owned(),
        title: title.to_owned(),
        source: source.to_owned(),
        required_permissions: permissions.into_iter().map(str::to_owned).collect(),
        supported_workspaces: workspaces.into(),
    }
}

fn place(panel_id: &str, region: PanelRegion, order: u16) -> PanelPlacement {
    PanelPlacement {
        panel_id: panel_id.to_owned(),
        region,
        order,
    }
}

#[cfg(test)]
mod tests {
    use super::{default_panel_registry, GuiShellState, WorkspaceId};

    #[test]
    fn workspaces_are_json_serializable() {
        let shell = GuiShellState::default_shell();
        let encoded = serde_json::to_string(&shell).unwrap();
        let decoded: GuiShellState = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.active_workspace, WorkspaceId::Dashboard);
        assert_eq!(decoded.workspaces.len(), 8);
        assert!(decoded
            .workspaces
            .iter()
            .any(|workspace| workspace.id == WorkspaceId::Maps));
    }

    #[test]
    fn panels_have_stable_unique_ids() {
        let panels = default_panel_registry();
        let mut ids = panels
            .iter()
            .map(|panel| panel.id.as_str())
            .collect::<Vec<_>>();
        ids.sort_unstable();
        ids.dedup();

        assert_eq!(ids.len(), panels.len());
        assert!(ids.contains(&"event-bus-monitor"));
        assert!(ids.contains(&"callsign-entry"));
        assert!(ids.contains(&"activation-setup"));
        assert!(ids.contains(&"interactive-map"));
        assert!(ids.contains(&"map-layers"));
    }
}
