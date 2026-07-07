# ADR 0006: GUI Shell as Core Client

## Status

Accepted

## Context

The project needs a rich operator interface while preserving shared business logic for CLI, web, desktop, and future external clients.

## Decision

The GUI is a client of the shared core. It renders workspaces, panels, commands, settings, and status. It submits proposals and consumes event bus/projection state. It does not own official logging rules.

## Consequences

- GUI panels must not write official events directly.
- Panel contribution does not grant data access.
- Future Tauri/web/TUI clients can share core behavior.
- Business rules belong in `ham-core` or plugin/core service crates, not JavaScript-only UI state.
