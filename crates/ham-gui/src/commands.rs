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
    }
}
