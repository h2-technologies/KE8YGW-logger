# Event Catalog

This catalog lists event types implemented or planned. Official events are append-only logbook history. Runtime events are diagnostics and operational telemetry.

## Official Events

### QSO

- `official.log.qso.created` - create a QSO record.
- `official.log.qso.corrected` - append corrections to an existing QSO.
- `official.log.qso.deleted` - tombstone a QSO.
- `official.log.qso.restored` - restore a tombstoned QSO.
- `official.log.qso.note_added` - append QSO note history.
- `official.log.qso.activation_linked` - link a QSO to an activation.
- `official.log.qso.activation_unlinked` - unlink a QSO from an activation.

### Activation

- `official.log.activation.created`
- `official.log.activation.updated`
- `official.log.activation.started`
- `official.log.activation.ended`
- `official.log.activation.cancelled`
- `official.log.activation.note_added`

### Upload Status

- `official.log.upload.queued` - record a queued upload job without mutating QSOs.
- `official.log.upload.completed` - record provider upload completion.
- `official.log.upload.failed` - record provider upload failure.

### Net Control

- `official.log.net.template.created`
- `official.log.net.template.updated`
- `official.log.net.session.started`
- `official.log.net.session.ended`
- `official.log.net.session.cancelled`
- `official.log.net.checkin.created`
- `official.log.net.checkin.updated`
- `official.log.net.checkin.deleted` - tombstone a check-in.
- `official.log.net.traffic.created`
- `official.log.net.traffic.updated`
- `official.log.net.report.exported`

### Planned

- Award submissions, EmComm forms, contest contacts, map annotations, and conflict-resolution branch metadata.

## Proposal Events

### QSO

- `proposal.qso.create`
- `proposal.qso.correct`
- `proposal.qso.delete`
- `proposal.qso.restore`
- `proposal.qso.note.add`
- `proposal.qso.activation.link`
- `proposal.qso.activation.unlink`

### Activation

- `proposal.activation.create`
- `proposal.activation.update`
- `proposal.activation.start`
- `proposal.activation.end`
- `proposal.activation.cancel`
- `proposal.activation.note.add`

### Net Control

- `proposal.net.template.create`
- `proposal.net.template.update`
- `proposal.net.session.start`
- `proposal.net.session.end`
- `proposal.net.session.cancel`
- `proposal.net.checkin.create`
- `proposal.net.checkin.update`
- `proposal.net.checkin.delete`
- `proposal.net.traffic.create`
- `proposal.net.traffic.update`
- `proposal.net.report.export`

## Runtime Events

Runtime event categories are dotted strings. Current and planned category roots:

- `ui.*`
- `plugin.*`
- `proposal.*`
- `projection.*`
- `official.log.*`
- `storage.*`
- `import.adif.*`
- `export.adif.*`
- `activation.*`
- `qso.activation.*`
- `lookup.*`
- `rig.*`
- `network.*`
- `sync.*`
- `diagnostics.*`
- `service.*`
- `station.*`
- `awards.*`
- `search.*`
- `upload.*`
- `credential.*`
- `net.*`
- `app.*`

## Implemented Runtime Event Examples

### Proposal and Projection

- `proposal.qso.create.received`
- `proposal.qso.create.accepted`
- `proposal.qso.create.rejected`
- `official.log.event.appended`
- `projection.qso.updated`
- `projection.qso.rebuilt`

### Storage

- `storage.opened`
- `storage.error`
- `official.log.chain.verified`

### ADIF

- `import.adif.started`
- `import.adif.record.accepted`
- `import.adif.record.rejected`
- `import.adif.completed`
- `export.adif.started`
- `export.adif.completed`

### Sync

- `network.discovery.started`
- `network.discovery.stopped`
- `network.peer.discovered`
- `network.peer.updated`
- `network.peer.expired`
- `sync.handshake.accepted`
- `sync.preview_pull.started`
- `sync.preview_pull.completed`
- `sync.pull.started`
- `sync.pull.progress`
- `sync.remote_event.accepted`
- `sync.remote_event.rejected`
- `sync.pull.completed`
- `sync.pull.failed`
- `sync.divergence.detected`
- `sync.cloud.connect.started`
- `sync.cloud.connect.succeeded`
- `sync.cloud.connect.failed`
- `sync.cloud.push.started`
- `sync.cloud.push.completed`
- `sync.cloud.pull.started`
- `sync.cloud.pull.completed`

### Lookup

- `lookup.callsign.started`
- `lookup.callsign.cache_hit`
- `lookup.callsign.cache_miss`
- `lookup.callsign.completed`
- `lookup.callsign.failed`
- `lookup.entity.inferred`
- `lookup.grid.validated`
- `lookup.suggestion.created`
- `lookup.cache.cleared`

### Rig

- `rig.provider.loaded`
- `rig.connect.started`
- `rig.connect.succeeded`
- `rig.connect.failed`
- `rig.disconnected`
- `rig.state.changed`
- `rig.frequency.changed`
- `rig.mode.changed`
- `rig.ptt.changed`
- `rig.command.sent`
- `rig.command.failed`
- `rig.autofill.suggestion.created`

### Diagnostics and Permissions

- `diagnostics.report.started`
- `diagnostics.bundle.created`
- `diagnostics.redaction.completed`
- `diagnostics.export.completed`
- `diagnostics.upload.completed`
- `diagnostics.upload.failed`
- `plugin.permission.requested`
- `plugin.permission.granted`
- `plugin.permission.denied`
- `plugin.permission.revoked`
- `plugin.permission.check.allowed`
- `plugin.permission.check.denied`
- `plugin.manifest.loaded`
- `plugin.manifest.invalid`
- `plugin.disabled.permission_missing`

### Services

- `service.provider.registered`
- `service.provider.enabled`
- `service.provider.disabled`
- `service.provider.health_changed`
- `service.request.started`
- `service.request.completed`
- `service.request.failed`
- `service.request.cache_hit`
- `service.request.cache_miss`
- `service.provider.fallback_used`
- `service.permission.denied`
- `service.config.missing`

### Daily Driver Logging

- `station.profile.created`
- `station.profile.updated`
- `station.profile.selected`
- `station.equipment.created`
- `station.equipment.updated`
- `station.configuration.selected`
- `awards.rebuild.started`
- `awards.rebuild.completed`
- `awards.progress.updated`
- `awards.definition.loaded`
- `search.query.started`
- `search.query.completed`
- `search.query.failed`
- `search.saved.created`
- `upload.queue.created`
- `upload.job.started`
- `upload.job.completed`
- `upload.job.failed`
- `upload.provider.missing_config`
- `upload.adif.generated`
- `logger.form.focused`
- `logger.qso.submitted`
- `logger.duplicate.warning`
- `logger.lookup_suggestion.accepted`
- `logger.rig_fields.applied`

### Credentials and Net Control

- `credential.store.available`
- `credential.store.unavailable`
- `credential.created`
- `credential.updated`
- `credential.deleted`
- `credential.test.started`
- `credential.test.completed`
- `credential.test.failed`
- `credential.access.denied`
- `credential.redaction.applied`
- `net.session.started`
- `net.session.ended`
- `net.checkin.received`
- `net.checkin.accepted`
- `net.checkin.rejected`
- `net.checkin.duplicate_warning`
- `net.traffic.created`
- `net.report.exported`

### Maps, Weather, and Propagation

- `map.loaded`
- `map.layer.enabled`
- `map.layer.disabled`
- `map.marker.selected`
- `map.center.changed`
- `grid.converted`
- `distance.calculated`
- `bearing.calculated`
- `grayline.updated`
- `grayline.recalculated`
- `propagation.updated`
- `solar.updated`
- `band.conditions.updated`
- `weather.updated`

## Catalog Rules

- Every official state change must add an official event and projection update path.
- Every new workflow should publish runtime events for diagnostics.
- Runtime events must redact secret-like data before persistence.
- New event types should be documented here before public use.
