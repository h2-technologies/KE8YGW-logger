//! GUI shell models for the ham radio platform.
//!
//! The GUI crate owns presentation shell configuration only. Business rules stay
//! in `ham-core`; future desktop, web, and plugin surfaces should consume these
//! serializable layout and registration models instead of hardcoding behavior in
//! panels.

pub mod bridge;
pub mod commands;
pub mod mock;
pub mod shell;

pub use bridge::{GuiRuntimeBridge, RuntimeBridgeStatus, RuntimeEventInput};
pub use commands::{CommandDefinition, CommandRegistry};
pub use shell::{
    default_panel_registry, default_workspaces, GuiShellState, PanelDefinition, PanelPlacement,
    PanelRegion, WorkspaceDefinition, WorkspaceId, WorkspaceLayout,
};
