use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkspaceId {
    Dashboard,
    CasualLogger,
    PotaSota,
    Maps,
    Awards,
    OnlineServices,
    NetControl,
    EmComm,
    Contesting,
}

impl WorkspaceId {
    pub const ALL: [Self; 9] = [
        Self::Dashboard,
        Self::CasualLogger,
        Self::PotaSota,
        Self::Maps,
        Self::Awards,
        Self::OnlineServices,
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
            Self::OnlineServices => "Online Services",
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

/// The shell layouts an operator can switch between. Each one arranges the same
/// workspaces and panels differently; none of them changes what data is
/// available, only where it sits and which surface owns the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShellLayoutId {
    OperatingDeck,
    CommandCenter,
    FieldNotebook,
    FocusConsole,
    TabbedWorkbench,
}

impl ShellLayoutId {
    pub const ALL: [Self; 5] = [
        Self::OperatingDeck,
        Self::CommandCenter,
        Self::FieldNotebook,
        Self::FocusConsole,
        Self::TabbedWorkbench,
    ];

    /// The stable identifier persisted in `DisplaySettings::desktop_shell_layout`.
    pub fn slug(self) -> &'static str {
        match self {
            Self::OperatingDeck => "operating-deck",
            Self::CommandCenter => "command-center",
            Self::FieldNotebook => "field-notebook",
            Self::FocusConsole => "focus-console",
            Self::TabbedWorkbench => "tabbed-workbench",
        }
    }

    pub fn from_slug(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|layout| layout.slug().eq_ignore_ascii_case(value.trim()))
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::OperatingDeck => "Operating Deck",
            Self::CommandCenter => "Command Center",
            Self::FieldNotebook => "Field Notebook",
            Self::FocusConsole => "Focus Console",
            Self::TabbedWorkbench => "Tabbed Workbench",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::OperatingDeck => {
                "Entry deck anchored across the bottom, band map rail on the left, log in the centre, callsign context on the right. Fastest for live operating."
            }
            Self::CommandCenter => {
                "Map fills the window; panels float above it. Best when propagation, parks, summits or net geography is what you are working from."
            }
            Self::FieldNotebook => {
                "Calm card board with the entry card as the hero. The most approachable layout and the easiest to read in daylight."
            }
            Self::FocusConsole => {
                "One centred column, a very large callsign field, and everything else collapsed to the menu bar and a single peek drawer."
            }
            Self::TabbedWorkbench => {
                "Logbooks, activations, contests and nets open as document tabs over a tree, with a table above an inspector. Built for bulk work."
            }
        }
    }

    /// Whether the layout keeps a permanent QSO entry surface. Operators
    /// choosing a layout for a contest care about this more than anything else.
    pub fn has_persistent_entry(self) -> bool {
        !matches!(self, Self::TabbedWorkbench)
    }

    pub fn density(self) -> LayoutDensity {
        match self {
            Self::OperatingDeck | Self::TabbedWorkbench => LayoutDensity::Dense,
            Self::CommandCenter => LayoutDensity::Balanced,
            Self::FieldNotebook | Self::FocusConsole => LayoutDensity::Relaxed,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutDensity {
    Dense,
    Balanced,
    Relaxed,
}

/// How the shell resolves light and dark. `System` follows the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    System,
    Light,
    Dark,
}

impl ThemeMode {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn slug(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn from_slug(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|mode| mode.slug().eq_ignore_ascii_case(value.trim()))
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::System => "Match system",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellLayoutDefinition {
    pub id: ShellLayoutId,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub density: LayoutDensity,
    pub has_persistent_entry: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemeModeDefinition {
    pub id: ThemeMode,
    pub slug: String,
    pub title: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShellAppearance {
    pub layout: ShellLayoutId,
    pub theme: ThemeMode,
}

impl Default for ShellAppearance {
    fn default() -> Self {
        Self {
            layout: ShellLayoutId::OperatingDeck,
            theme: ThemeMode::System,
        }
    }
}

pub fn default_layout_catalog() -> Vec<ShellLayoutDefinition> {
    ShellLayoutId::ALL
        .into_iter()
        .map(|id| ShellLayoutDefinition {
            id,
            slug: id.slug().to_owned(),
            title: id.title().to_owned(),
            description: id.description().to_owned(),
            density: id.density(),
            has_persistent_entry: id.has_persistent_entry(),
        })
        .collect()
}

pub fn default_theme_catalog() -> Vec<ThemeModeDefinition> {
    ThemeMode::ALL
        .into_iter()
        .map(|id| ThemeModeDefinition {
            id,
            slug: id.slug().to_owned(),
            title: id.title().to_owned(),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GuiShellState {
    pub active_workspace: WorkspaceId,
    pub workspaces: Vec<WorkspaceDefinition>,
    pub panels: Vec<PanelDefinition>,
    pub appearance: ShellAppearance,
    pub layouts: Vec<ShellLayoutDefinition>,
    pub themes: Vec<ThemeModeDefinition>,
}

impl GuiShellState {
    pub fn default_shell() -> Self {
        Self {
            active_workspace: WorkspaceId::Dashboard,
            workspaces: default_workspaces(),
            panels: default_panel_registry(),
            appearance: ShellAppearance::default(),
            layouts: default_layout_catalog(),
            themes: default_theme_catalog(),
        }
    }

    /// Build the shell from persisted display settings. Unknown slugs fall back
    /// to the defaults for the same reason `ApplicationSettings::normalized`
    /// does: a stale or newer client must not lock the operator out of the GUI.
    pub fn with_appearance(desktop_shell_layout: &str, appearance_mode: &str) -> Self {
        let mut shell = Self::default_shell();
        shell.appearance = ShellAppearance {
            layout: ShellLayoutId::from_slug(desktop_shell_layout).unwrap_or_default_layout(),
            theme: ThemeMode::from_slug(appearance_mode).unwrap_or(ThemeMode::System),
        };
        shell
    }
}

trait OrDefaultLayout {
    fn unwrap_or_default_layout(self) -> ShellLayoutId;
}

impl OrDefaultLayout for Option<ShellLayoutId> {
    fn unwrap_or_default_layout(self) -> ShellLayoutId {
        self.unwrap_or(ShellLayoutId::OperatingDeck)
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
            "divergence-review",
            "Divergence Review",
            "core.sync",
            ["sync.view"],
            WorkspaceId::ALL,
        ),
        panel(
            "backup-restore",
            "Backup and Restore",
            "core.backup",
            ["backup.export", "backup.import"],
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
            "online-accounts",
            "Accounts",
            "plugin.online-services",
            ["credential.view_metadata"],
            [WorkspaceId::OnlineServices],
        ),
        panel(
            "online-providers",
            "Providers",
            "plugin.online-services",
            ["service.cache.read"],
            [WorkspaceId::OnlineServices],
        ),
        panel(
            "online-upload-queue",
            "Upload Queue",
            "plugin.online-services",
            ["upload.status.view"],
            [WorkspaceId::OnlineServices],
        ),
        panel(
            "online-downloads",
            "Downloads",
            "plugin.online-services",
            ["upload.confirmation_pull"],
            [WorkspaceId::OnlineServices],
        ),
        panel(
            "confirmation-status",
            "Confirmation Status",
            "plugin.online-services",
            ["upload.status.view"],
            [WorkspaceId::OnlineServices],
        ),
        panel(
            "provider-health",
            "Provider Health",
            "plugin.online-services",
            ["service.cache.read"],
            [WorkspaceId::OnlineServices],
        ),
        panel(
            "service-cache",
            "Service Cache",
            "plugin.online-services",
            ["service.cache.read"],
            [WorkspaceId::OnlineServices],
        ),
        panel(
            "online-automation",
            "Automation",
            "plugin.online-services",
            ["automation.manage"],
            [WorkspaceId::OnlineServices],
        ),
        panel(
            "online-notifications",
            "Notifications",
            "plugin.online-services",
            ["notification.view"],
            [WorkspaceId::OnlineServices],
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
        WorkspaceId::OnlineServices => {
            "Accounts, providers, uploads, confirmations, spots, weather, propagation, automation, and notifications."
        }
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
            place("backup-restore", PanelRegion::RightInspector, 15),
            place("divergence-review", PanelRegion::RightInspector, 18),
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
        WorkspaceId::OnlineServices => vec![
            place("online-providers", PanelRegion::Center, 10),
            place("online-upload-queue", PanelRegion::Center, 20),
            place("dx-cluster", PanelRegion::Center, 30),
            place("spots-alerts", PanelRegion::Center, 40),
            place("online-accounts", PanelRegion::RightInspector, 10),
            place("provider-health", PanelRegion::RightInspector, 20),
            place("credential-manager", PanelRegion::RightInspector, 30),
            place("service-cache", PanelRegion::RightInspector, 40),
            place("online-downloads", PanelRegion::Bottom, 10),
            place("confirmation-status", PanelRegion::Bottom, 20),
            place("weather", PanelRegion::Bottom, 30),
            place("propagation", PanelRegion::Bottom, 40),
            place("online-automation", PanelRegion::Bottom, 50),
            place("online-notifications", PanelRegion::Bottom, 60),
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
    use super::{default_panel_registry, GuiShellState, ShellLayoutId, ThemeMode, WorkspaceId};
    use crate::{DEFAULT_APPEARANCE_MODE, DEFAULT_DESKTOP_SHELL_LAYOUT, DESKTOP_SHELL_LAYOUTS};

    #[test]
    fn layout_slugs_match_the_shared_settings_vocabulary() {
        let shell_slugs = ShellLayoutId::ALL
            .into_iter()
            .map(ShellLayoutId::slug)
            .collect::<Vec<_>>();
        // The GUI catalog and the persisted settings must agree, or an operator
        // picks a layout the settings layer then silently discards.
        assert_eq!(shell_slugs, DESKTOP_SHELL_LAYOUTS.to_vec());
        assert_eq!(
            ShellLayoutId::from_slug(DEFAULT_DESKTOP_SHELL_LAYOUT),
            Some(ShellLayoutId::OperatingDeck)
        );
        assert_eq!(
            ThemeMode::from_slug(DEFAULT_APPEARANCE_MODE),
            Some(ThemeMode::System)
        );
    }

    #[test]
    fn appearance_round_trips_and_unknown_slugs_fall_back() {
        let shell = GuiShellState::with_appearance("focus-console", "light");
        assert_eq!(shell.appearance.layout, ShellLayoutId::FocusConsole);
        assert_eq!(shell.appearance.theme, ThemeMode::Light);

        let stale = GuiShellState::with_appearance("holodeck", "solarized");
        assert_eq!(stale.appearance.layout, ShellLayoutId::OperatingDeck);
        assert_eq!(stale.appearance.theme, ThemeMode::System);
    }

    #[test]
    fn every_layout_is_offered_with_copy_the_settings_screen_can_show() {
        let shell = GuiShellState::default_shell();
        assert_eq!(shell.layouts.len(), ShellLayoutId::ALL.len());
        assert_eq!(shell.themes.len(), ThemeMode::ALL.len());
        for layout in &shell.layouts {
            assert!(!layout.title.is_empty(), "{} needs a title", layout.slug);
            assert!(
                layout.description.len() > 40,
                "{} needs a description an operator can choose from",
                layout.slug
            );
        }
        // The workbench is the one layout without an always-present entry field;
        // the settings screen warns about that, so the flag has to stay true.
        assert!(!ShellLayoutId::TabbedWorkbench.has_persistent_entry());
        assert!(ShellLayoutId::OperatingDeck.has_persistent_entry());
    }

    #[test]
    fn workspaces_are_json_serializable() {
        let shell = GuiShellState::default_shell();
        let encoded = serde_json::to_string(&shell).unwrap();
        let decoded: GuiShellState = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded.active_workspace, WorkspaceId::Dashboard);
        assert_eq!(decoded.workspaces.len(), 9);
        assert!(decoded
            .workspaces
            .iter()
            .any(|workspace| workspace.id == WorkspaceId::Maps));
        assert!(decoded
            .workspaces
            .iter()
            .any(|workspace| workspace.id == WorkspaceId::OnlineServices));
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
        assert!(ids.contains(&"online-providers"));
        assert!(ids.contains(&"online-upload-queue"));
        assert!(ids.contains(&"backup-restore"));
        assert!(ids.contains(&"divergence-review"));
    }
}
