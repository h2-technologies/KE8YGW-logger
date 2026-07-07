# Net Control Plugin

The Net Control MVP is implemented as a built-in plugin-style workflow that uses
the shared proposal pipeline. It never writes official events directly from the
GUI.

## Permissions

The plugin requests:

- `net.view`
- `net.template.create`
- `net.template.update`
- `net.session.start`
- `net.session.end`
- `net.checkin.create`
- `net.checkin.update`
- `net.checkin.delete`
- `net.traffic.manage`
- `net.report.export`

Plugin grants and operator role checks are both required before proposals are
accepted.

## Event Model

Official event types:

- `official.log.net.template.created`
- `official.log.net.template.updated`
- `official.log.net.session.started`
- `official.log.net.session.ended`
- `official.log.net.session.cancelled`
- `official.log.net.checkin.created`
- `official.log.net.checkin.updated`
- `official.log.net.checkin.deleted`
- `official.log.net.traffic.created`
- `official.log.net.traffic.updated`
- `official.log.net.report.exported`

Deletes are tombstones. Deleted check-ins are hidden from normal projection
views but remain in the append-only event log.

## Proposal Workflow

The GUI submits proposals such as:

- `proposal.net.session.start`
- `proposal.net.checkin.create`
- `proposal.net.traffic.create`
- `proposal.net.report.export`

The core validates:

- plugin permission
- operator role
- active session requirements
- required callsign or tactical-only mode
- ended sessions blocking new check-ins
- schema and timestamp fields

Accepted proposals become official hash-chained events.

## Projection

`NetControlProjection` rebuilds current state from official events:

- active and historical sessions
- check-in roster
- late check-in count
- duplicate warnings
- traffic count
- emergency traffic count
- report export history

## GUI Workflow

The Net Control workspace includes:

- Net Session Control
- Check-In Entry
- Check-In Roster
- Traffic Queue
- Net Report

Keyboard/command palette actions include opening Net Control, starting/ending a
net, focusing check-in entry, adding a late check-in, opening traffic, and
exporting a report event.

## Current Limitations

- Template create/edit UI is not fully built yet.
- Report export currently stores a markdown summary in an official report event;
  native file export formats are future work.
- Multi-net concurrent operation and directed scripts are future work.

## Future Work

- Directed net scripts
- Recurring schedules
- ICS-309 export
- Tactical roster import
- Linked voice recorder
- Multi-net support
