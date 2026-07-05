use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceId {
    Dashboard,
    CasualLogger,
    PotaSota,
    NetControl,
    EmComm,
    Contesting,
}

impl WorkspaceId {
    pub const ALL: [Self; 6] = [
        Self::Dashboard,
        Self::CasualLogger,
        Self::PotaSota,
        Self::NetControl,
        Self::EmComm,
        Self::Contesting,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Dashboard => "Dashboard",
            Self::CasualLogger => "Casual Logger",
            Self::PotaSota => "POTA/SOTA",
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
            ["logbook.read"],
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
            ["rig.read", "rig.control"],
            WorkspaceId::ALL,
        ),
        panel(
            "sync-status",
            "Sync Status",
            "core.sync",
            ["sync.read"],
            WorkspaceId::ALL,
        ),
        panel(
            "event-bus-monitor",
            "Event Bus Monitor",
            "core.diagnostics",
            ["diagnostics.read"],
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
                WorkspaceId::EmComm,
            ],
        ),
        panel(
            "pota-sota-activation",
            "POTA/SOTA Activation",
            "plugin.pota-sota",
            ["activation.read", "activation.propose"],
            [WorkspaceId::PotaSota],
        ),
        panel(
            "dx-cluster",
            "DX Cluster",
            "plugin.dx-cluster",
            ["network.read"],
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
            ["plugins.read"],
            WorkspaceId::ALL,
        ),
        panel(
            "diagnostic-reports",
            "Diagnostic Reports",
            "core.diagnostics",
            ["diagnostics.read"],
            WorkspaceId::ALL,
        ),
    ]
}

fn workspace_description(id: WorkspaceId) -> &'static str {
    match id {
        WorkspaceId::Dashboard => "Operational overview and platform health.",
        WorkspaceId::CasualLogger => "General QSO entry and recent contact context.",
        WorkspaceId::PotaSota => "Activation planning, map context, and portable logging.",
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
        ],
        WorkspaceId::CasualLogger => vec![
            place("callsign-entry", PanelRegion::Center, 10),
            place("recent-qsos", PanelRegion::Center, 20),
            place("rig-control", PanelRegion::RightInspector, 10),
            place("dx-cluster", PanelRegion::Bottom, 10),
        ],
        WorkspaceId::PotaSota => vec![
            place("pota-sota-activation", PanelRegion::Center, 10),
            place("map-placeholder", PanelRegion::Center, 20),
            place("callsign-entry", PanelRegion::RightInspector, 10),
            place("sync-status", PanelRegion::Bottom, 10),
        ],
        WorkspaceId::NetControl => vec![
            place("callsign-entry", PanelRegion::Center, 10),
            place("recent-qsos", PanelRegion::Center, 20),
            place("event-bus-monitor", PanelRegion::RightInspector, 10),
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
        assert_eq!(decoded.workspaces.len(), 6);
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
    }
}
