# EmComm Incident Record Model

The EmComm record model is the append-only domain behind ICS 211, 213, 213RR,
and 214. Every state change is an official logbook event; nothing edits an
earlier record in place.

Canonical implementation: `crates/ham-core/src/emcomm.rs`.
Payload schema version: `EMCOMM_SCHEMA_VERSION` (currently `1`).

## Entities

| Entity | Opened by | Terminal event | Supports |
| --- | --- | --- | --- |
| Incident | `official.log.emcomm.incident.opened` | `.closed` | The container for everything below. |
| Operational period | `.period.opened` | `.period.closed` | ICS 214 period scoping. |
| Person | `.person.checked_in` | `.person.checked_out` | ICS 211 check-in list. |
| Assignment | `.assignment.created` | `.assignment.released` | ICS 211 assignment columns. |
| Message | `.message.created` | `.message.cancelled` | ICS 213 and ICS 213RR. |
| Activity entry | `.activity.logged` | — | ICS 214 activity and communications log. |

Each entity also has an `.updated` event used for corrections. Every event is
submitted as a proposal and validated before it is appended, exactly like a
QSO.

## Corrections are appends

`EmCommRecord` keeps two things: `payload`, the merged current view, and
`history`, every event that produced it in order. A correction merges into the
payload and pushes a `RecordChange` carrying the correcting payload, the event
hash, and its timestamp. Reading `history()` shows what a record said before a
correction, which is what makes an exported incident package auditable.

A message that is transmitted and then cancelled keeps its transmission entry.
Delivery state moves forward through new events; it is never overwritten.

An empty correction is rejected — a correction that changes nothing would add
an audit entry without an audit meaning.

## Offline message numbering

A message number is the originating station's identity plus that station's own
sequence, formatted `PREFIX-NNNN` (for example `KE8YGW-0007`). Because the
prefix is the station's, two stations working the same incident while
disconnected cannot mint the same number, and neither needs to ask the other
for permission to allocate one.

`EmCommProjection::next_message_number(incident_id, station_prefix)` counts only
numbers minted by that prefix, so the sequence stays correct offline. A message
number is assigned once: `proposal.emcomm.message.update` rejects any attempt
to change it.

## Precedence and forms

Precedence is `emergency`, `priority`, `immediate`, or `routine`, and
`messages_for_incident` orders most urgent first, then by message number.
`unacknowledged_messages` lists traffic that was transmitted and not yet
acknowledged.

Forms serialize under their official identifiers (`ICS-211`, `ICS-213`,
`ICS-213RR`, `ICS-214`), so an export preserves which form version a record
belongs to.

## Incident package

`incident_package(incident_id)` produces the complete ordered record set for one
incident — the incident, its operational periods, personnel, assignments,
messages, and activity log — each with its own event history. `summary()`
produces counts suitable for a dashboard or a runtime event.

## Capabilities

`emcomm.view`, `emcomm.incident.manage`, `emcomm.period.manage`,
`emcomm.person.manage`, `emcomm.assignment.manage`, `emcomm.message.manage`,
and `emcomm.activity.log`. Message management is a high-risk capability and
requires an explicit grant.

## Scope

This document covers the shared domain model, its proposal validation, and its
projection. The ICS 211, 213, 213RR, and 214 form workflows, the cross-platform
incident UI, and PDF/JSON export are separate v1 deliverables and are not
implemented here.
