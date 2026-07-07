# ADR 0003: Plugin Proposal Write Model

## Status

Accepted

## Context

Plugins should extend the platform without owning official log integrity.

## Decision

Plugins cannot write official events directly. They submit proposals. The shared core validates plugin grants, operator role permissions, scopes, schema, and domain rules before appending official events.

## Consequences

- Official log integrity is centralized.
- Permission enforcement has one primary path.
- GUI panels and plugin integrations stay advisory unless the user submits a proposal.
- Imports, sync, activation workflows, rig autofill, lookup suggestions, and AI helpers must use core validation paths.
