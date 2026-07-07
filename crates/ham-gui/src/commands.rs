use serde::{Deserialize, Serialize};

use crate::shell::WorkspaceId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandDefinition {
    pub id: String,
    pub title: String,
    pub category: String,
    pub shortcut: Option<String>,
    pub target_workspace: Option<WorkspaceId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRegistry {
    pub commands: Vec<CommandDefinition>,
}

impl CommandRegistry {
    pub fn default_registry() -> Self {
        let mut commands = vec![
            command(
                "open.settings",
                "Open Settings",
                "Shell",
                Some("Ctrl/Cmd+,"),
                None,
            ),
            command("open.plugins", "Open Plugin Manager", "Shell", None, None),
            command(
                "open.diagnostics",
                "Open Diagnostic Report",
                "Diagnostics",
                None,
                None,
            ),
            command(
                "focus.callsign-entry",
                "Focus Callsign Entry",
                "Logging",
                None,
                None,
            ),
            command(
                "toggle.event-bus-monitor",
                "Toggle Event Bus Monitor",
                "Diagnostics",
                None,
                None,
            ),
            command(
                "event-bus.open",
                "Open Event Bus Monitor",
                "Diagnostics",
                None,
                None,
            ),
            command(
                "event-bus.pause",
                "Toggle Event Stream Pause",
                "Diagnostics",
                None,
                None,
            ),
            command(
                "event-bus.export",
                "Export Visible Runtime Events",
                "Diagnostics",
                None,
                None,
            ),
            command(
                "event-bus.copy-latest-error",
                "Copy Latest Error",
                "Diagnostics",
                None,
                None,
            ),
            command(
                "diagnostics.open-folder",
                "Open Diagnostics Folder",
                "Diagnostics",
                None,
                None,
            ),
            command(
                "diagnostics.report.problem",
                "Report a Problem",
                "Diagnostics",
                None,
                None,
            ),
            command(
                "diagnostics.report.export",
                "Export Diagnostic ZIP",
                "Diagnostics",
                None,
                None,
            ),
            command(
                "diagnostics.report.upload",
                "Upload Diagnostic Report",
                "Diagnostics",
                None,
                None,
            ),
            command(
                "diagnostics.report.copy-last-id",
                "Copy Last Report ID",
                "Diagnostics",
                None,
                None,
            ),
            command("adif.import", "Import ADIF", "Logging", None, None),
            command("adif.export", "Export ADIF", "Logging", None, None),
            command("lookup.callsign", "Lookup Callsign", "Lookup", None, None),
            command(
                "lookup.cache.clear",
                "Clear Lookup Cache",
                "Lookup",
                None,
                None,
            ),
            command(
                "lookup.provider-status",
                "Show Lookup Provider Status",
                "Lookup",
                None,
                None,
            ),
            command(
                "services.open",
                "Open Service Providers",
                "Services",
                None,
                None,
            ),
            command(
                "services.health.refresh",
                "Refresh Provider Health",
                "Services",
                None,
                None,
            ),
            command(
                "services.provider.enable",
                "Enable Provider",
                "Services",
                None,
                None,
            ),
            command(
                "services.provider.disable",
                "Disable Provider",
                "Services",
                None,
                None,
            ),
            command(
                "services.cache.clear",
                "Clear Service Cache",
                "Services",
                None,
                None,
            ),
            command(
                "services.lookup.test",
                "Test Callsign Lookup Providers",
                "Services",
                None,
                None,
            ),
            command(
                "services.spotting.test",
                "Test Spotting Providers",
                "Services",
                None,
                None,
            ),
            command("rig.connect", "Connect Rig", "Rig Control", None, None),
            command(
                "rig.disconnect",
                "Disconnect Rig",
                "Rig Control",
                None,
                None,
            ),
            command(
                "rig.refresh-state",
                "Refresh Rig State",
                "Rig Control",
                None,
                None,
            ),
            command(
                "rig.use-frequency-mode",
                "Use Rig Frequency/Mode",
                "Rig Control",
                None,
                None,
            ),
            command(
                "rig.open-panel",
                "Open Rig Control Panel",
                "Rig Control",
                None,
                Some(WorkspaceId::CasualLogger),
            ),
            command(
                "activation.start-pota",
                "Start POTA Activation",
                "POTA/SOTA",
                None,
                Some(WorkspaceId::PotaSota),
            ),
            command(
                "activation.start-sota",
                "Start SOTA Activation",
                "POTA/SOTA",
                None,
                Some(WorkspaceId::PotaSota),
            ),
            command(
                "activation.end-current",
                "End Current Activation",
                "POTA/SOTA",
                None,
                Some(WorkspaceId::PotaSota),
            ),
            command(
                "activation.workspace",
                "Open POTA/SOTA Workspace",
                "POTA/SOTA",
                None,
                Some(WorkspaceId::PotaSota),
            ),
            command(
                "activation.export-adif",
                "Export Current Activation ADIF",
                "POTA/SOTA",
                None,
                Some(WorkspaceId::PotaSota),
            ),
            command(
                "activation.link-selected-qso",
                "Link Selected QSO to Activation",
                "POTA/SOTA",
                None,
                Some(WorkspaceId::PotaSota),
            ),
            command(
                "activation.unlink-selected-qso",
                "Unlink Selected QSO from Activation",
                "POTA/SOTA",
                None,
                Some(WorkspaceId::PotaSota),
            ),
            command(
                "official-log.verify-chain",
                "Verify Log Chain",
                "Diagnostics",
                None,
                None,
            ),
            command(
                "projection.rebuild",
                "Rebuild Projections",
                "Diagnostics",
                None,
                None,
            ),
            command(
                "station.profiles.open",
                "Open Station Profiles",
                "Station",
                None,
                None,
            ),
            command(
                "station.equipment.open",
                "Open Equipment Manager",
                "Station",
                None,
                None,
            ),
            command(
                "station.profile.switch",
                "Switch Station Profile",
                "Station",
                None,
                None,
            ),
            command(
                "station.profile.create",
                "Create Station Profile",
                "Station",
                None,
                None,
            ),
            command(
                "station.equipment.create",
                "Create Equipment Item",
                "Station",
                None,
                None,
            ),
            command(
                "awards.open",
                "Open Awards",
                "Awards",
                None,
                Some(WorkspaceId::Awards),
            ),
            command(
                "awards.rebuild",
                "Rebuild Award Progress",
                "Awards",
                None,
                Some(WorkspaceId::Awards),
            ),
            command(
                "awards.needed.entities",
                "Show Needed Entities",
                "Awards",
                None,
                Some(WorkspaceId::Awards),
            ),
            command(
                "awards.needed.states",
                "Show Needed States",
                "Awards",
                None,
                Some(WorkspaceId::Awards),
            ),
            command("search.open", "Open Search", "Search", None, None),
            command(
                "search.save-current",
                "Save Current Search",
                "Search",
                None,
                None,
            ),
            command("search.run-saved", "Run Saved Search", "Search", None, None),
            command(
                "search.deleted",
                "Search Deleted QSOs",
                "Search",
                None,
                None,
            ),
            command("uploads.open", "Open Uploads", "Uploads", None, None),
            command(
                "uploads.queue-not-uploaded",
                "Queue Not Uploaded QSOs",
                "Uploads",
                None,
                None,
            ),
            command(
                "uploads.retry-failed",
                "Retry Failed Uploads",
                "Uploads",
                None,
                None,
            ),
            command(
                "uploads.export-adif",
                "Export Upload ADIF",
                "Uploads",
                None,
                None,
            ),
            command(
                "logger.submit-qso",
                "Submit QSO",
                "Logging",
                Some("Enter"),
                None,
            ),
            command("logger.clear-form", "Clear QSO Form", "Logging", None, None),
            command(
                "logger.use-rig-frequency",
                "Use Rig Frequency",
                "Logging",
                None,
                None,
            ),
            command(
                "logger.accept-lookup-suggestions",
                "Accept Lookup Suggestions",
                "Logging",
                None,
                None,
            ),
            command(
                "logger.toggle-activation-link",
                "Toggle POTA/SOTA Activation Link",
                "Logging",
                None,
                Some(WorkspaceId::PotaSota),
            ),
            command(
                "logger.open-recent-qsos",
                "Open Recent QSOs",
                "Logging",
                None,
                Some(WorkspaceId::CasualLogger),
            ),
            command(
                "logger.open-advanced-search",
                "Open Advanced Search",
                "Logging",
                None,
                None,
            ),
            command(
                "sync.discovery.start",
                "Start LAN Discovery",
                "Sync",
                None,
                None,
            ),
            command(
                "sync.discovery.stop",
                "Stop LAN Discovery",
                "Sync",
                None,
                None,
            ),
            command("sync.peers.refresh", "Refresh Peers", "Sync", None, None),
            command(
                "sync.handshake.selected",
                "Handshake with Selected Peer",
                "Sync",
                None,
                None,
            ),
            command(
                "sync.preview-pull.selected",
                "Preview Pull From Peer",
                "Sync",
                None,
                None,
            ),
            command(
                "sync.pull.selected",
                "Pull Missing Events From Peer",
                "Sync",
                None,
                None,
            ),
            command(
                "sync.verify-local-chain",
                "Verify Local Chain",
                "Sync",
                None,
                None,
            ),
            command(
                "sync.rebuild-projections",
                "Rebuild Projections",
                "Sync",
                None,
                None,
            ),
            command(
                "sync.diagnostics.copy",
                "Copy Sync Diagnostic Summary",
                "Sync",
                None,
                None,
            ),
            command(
                "sync.cloud.connect",
                "Connect Cloud Sync",
                "Sync",
                None,
                None,
            ),
            command(
                "sync.cloud.push",
                "Push Local Events to Cloud",
                "Sync",
                None,
                None,
            ),
            command(
                "sync.cloud.preview-pull",
                "Preview Pull From Cloud",
                "Sync",
                None,
                None,
            ),
            command(
                "sync.cloud.pull",
                "Pull Missing Events From Cloud",
                "Sync",
                None,
                None,
            ),
            command(
                "sync.cloud.settings",
                "Open Cloud Sync Settings",
                "Sync",
                None,
                None,
            ),
            command(
                "sync.cloud.diagnostics.copy",
                "Copy Cloud Sync Diagnostic Summary",
                "Sync",
                None,
                None,
            ),
            command(
                "sync.identity.copy",
                "Copy Local Sync Identity",
                "Sync",
                None,
                None,
            ),
        ];

        commands.extend(WorkspaceId::ALL.into_iter().map(|workspace| {
            command(
                &format!("workspace.{workspace:?}").to_ascii_lowercase(),
                &format!("Switch Workspace: {}", workspace.title()),
                "Workspace",
                None,
                Some(workspace),
            )
        }));

        Self { commands }
    }

    pub fn find(&self, query: &str) -> Vec<&CommandDefinition> {
        let query = query.trim().to_ascii_lowercase();
        if query.is_empty() {
            return self.commands.iter().collect();
        }

        self.commands
            .iter()
            .filter(|command| {
                command.title.to_ascii_lowercase().contains(&query)
                    || command.id.to_ascii_lowercase().contains(&query)
                    || command.category.to_ascii_lowercase().contains(&query)
            })
            .collect()
    }
}

fn command(
    id: &str,
    title: &str,
    category: &str,
    shortcut: Option<&str>,
    target_workspace: Option<WorkspaceId>,
) -> CommandDefinition {
    CommandDefinition {
        id: id.to_owned(),
        title: title.to_owned(),
        category: category.to_owned(),
        shortcut: shortcut.map(str::to_owned),
        target_workspace,
    }
}

#[cfg(test)]
mod tests {
    use super::CommandRegistry;

    #[test]
    fn command_registry_can_find_workspace_commands() {
        let registry = CommandRegistry::default_registry();
        let matches = registry.find("pota");

        assert!(matches
            .iter()
            .any(|command| command.title == "Switch Workspace: POTA/SOTA"));
    }

    #[test]
    fn command_registry_contains_required_shell_commands() {
        let registry = CommandRegistry::default_registry();
        let ids = registry
            .commands
            .iter()
            .map(|command| command.id.as_str())
            .collect::<Vec<_>>();

        assert!(ids.contains(&"open.settings"));
        assert!(ids.contains(&"open.plugins"));
        assert!(ids.contains(&"focus.callsign-entry"));
        assert!(ids.contains(&"toggle.event-bus-monitor"));
        assert!(ids.contains(&"event-bus.export"));
        assert!(ids.contains(&"event-bus.copy-latest-error"));
        assert!(ids.contains(&"diagnostics.report.problem"));
        assert!(ids.contains(&"diagnostics.report.export"));
        assert!(ids.contains(&"diagnostics.report.upload"));
        assert!(ids.contains(&"diagnostics.report.copy-last-id"));
        assert!(ids.contains(&"adif.import"));
        assert!(ids.contains(&"adif.export"));
        assert!(ids.contains(&"lookup.callsign"));
        assert!(ids.contains(&"lookup.cache.clear"));
        assert!(ids.contains(&"services.open"));
        assert!(ids.contains(&"services.cache.clear"));
        assert!(ids.contains(&"services.lookup.test"));
        assert!(ids.contains(&"services.spotting.test"));
        assert!(ids.contains(&"rig.connect"));
        assert!(ids.contains(&"rig.disconnect"));
        assert!(ids.contains(&"rig.refresh-state"));
        assert!(ids.contains(&"rig.use-frequency-mode"));
        assert!(ids.contains(&"rig.open-panel"));
        assert!(ids.contains(&"activation.start-pota"));
        assert!(ids.contains(&"activation.export-adif"));
        assert!(ids.contains(&"official-log.verify-chain"));
        assert!(ids.contains(&"projection.rebuild"));
        assert!(ids.contains(&"sync.discovery.start"));
        assert!(ids.contains(&"sync.preview-pull.selected"));
        assert!(ids.contains(&"sync.pull.selected"));
        assert!(ids.contains(&"sync.diagnostics.copy"));
        assert!(ids.contains(&"sync.cloud.connect"));
        assert!(ids.contains(&"sync.cloud.pull"));
        assert!(ids.contains(&"sync.identity.copy"));
        assert!(ids.contains(&"station.profiles.open"));
        assert!(ids.contains(&"station.equipment.open"));
        assert!(ids.contains(&"awards.open"));
        assert!(ids.contains(&"awards.rebuild"));
        assert!(ids.contains(&"search.open"));
        assert!(ids.contains(&"uploads.open"));
        assert!(ids.contains(&"uploads.queue-not-uploaded"));
        assert!(ids.contains(&"logger.submit-qso"));
        assert!(ids.contains(&"logger.clear-form"));
        assert!(ids.contains(&"logger.accept-lookup-suggestions"));
    }
}
