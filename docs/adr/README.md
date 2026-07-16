# Architecture Decision Records

Architecture Decision Records, or ADRs, document accepted project decisions that
shape the repository's architecture, compatibility, data model, security model,
or release behavior.

Use [0000-template.md](0000-template.md) for new ADRs. Number new records with
the next available four-digit prefix and a short descriptive title.

## When To Add Or Update An ADR

Add or update an ADR when a change establishes, reverses, or materially changes:

- official event semantics
- runtime event or diagnostics behavior
- plugin, provider, or permission boundaries
- sync behavior and trust assumptions
- credential storage or redaction policy
- `/api/v1` compatibility
- storage, migration, backup, or rollback behavior
- desktop, hosted, self-hosted, or native-client architecture
- release or update-channel architecture

Do not add ADRs for speculative ideas that are not accepted or implemented.

## Records

- [0001: Append-Only Official Log](0001-append-only-official-log.md)
- [0002: Runtime vs Official Events](0002-runtime-vs-official-events.md)
- [0003: Plugin Proposal Write Model](0003-plugin-proposal-write-model.md)
- [0004: Global Event Log Plus Projections](0004-global-event-log-plus-projections.md)
- [0005: Local-First Sync](0005-local-first-sync.md)
- [0006: GUI Shell as Core Client](0006-gui-shell-as-core-client.md)
