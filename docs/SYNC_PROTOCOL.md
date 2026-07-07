# Sync Protocol

Sync is local-first. LAN discovery and direct LAN replication are preferred. Cloud relay and self-hosted sync are fallback paths when devices cannot reach each other locally.

## Privacy Rules

- Do not sync runtime diagnostic logs.
- Do not sync credentials, API keys, private plugin configuration, or support cache data by default.
- Do not let peers mutate local official logs directly.
- Do not auto-merge divergent chains in MVP.
- Treat peers as untrusted until future pairing/authentication work is complete.

## LAN Discovery

Defaults:

- Protocol name: `ke8ygw-logger-sync`
- Protocol version: `1`
- IPv4 multicast: `239.73.89.71`
- IPv6 multicast: `ff12::73:5947`
- Discovery port: `9737`
- Local sync port: `9738`
- Peer timeout: 45 seconds
- Discovery interval: 5 seconds

Discovery packets advertise:

- protocol name/version
- device ID
- session ID
- optional user/account hash
- node display name
- capabilities
- optional local API port
- timestamp

They must not broadcast secrets, tokens, exact private profile data, or log contents.

## Handshake

Handshake requests include:

- protocol version
- device ID and session ID
- supported capabilities
- logbook IDs
- current head hash per logbook
- event count hints per logbook

Responses include:

- accepted/rejected status
- reason if rejected
- peer device ID
- protocol version
- supported capabilities
- matching logbook IDs
- peer head hash per matching logbook
- head comparison status

Event counts are hints only. A matching head hash means the logbook heads match. Different heads require ancestry comparison before safe replication.

## Head Comparison States

- `unknown` - not enough information to compare safely.
- `match` - head hashes match.
- `local_ahead` - local count is greater and ancestry appears compatible.
- `remote_ahead` - remote count is greater and ancestry appears compatible.
- `diverged` - chains do not share the expected ancestor or contain conflicting events.

## Safe Replication

### Preview Pull

Preview pull compares local and remote chain metadata and reports how many events would be fetched. It writes nothing.

### Pull Missing Events

Pull fetches full official event envelopes and accepts them only when:

- every event hash recalculates correctly
- schema version is supported
- event type is supported or safely storable
- logbook ID matches the request
- first incoming `previous_hash` connects to the local head or a known ancestor
- incoming events chain together
- duplicate event IDs are identical before being ignored
- duplicate IDs with different content are rejected

Accepted remote events are appended through the official event store without rewriting event metadata or hash input.

### Push

Push sends local official events to a peer or cloud server. The receiver applies the same verification rules and stores only valid append-only events.

## Cloud Relay and Self-Hosted Sync

Cloud sync reuses the same event envelopes and verification path. MVP auth uses pairing-code/token sessions.

Current REST surface:

- `GET /health`
- `POST /api/v1/auth/pair`
- `GET /api/v1/logbooks`
- `GET /api/v1/logbooks/{logbook_id}/head`
- `GET /api/v1/logbooks/{logbook_id}/events`
- `POST /api/v1/logbooks/{logbook_id}/preview-pull`
- `POST /api/v1/logbooks/{logbook_id}/pull`
- `POST /api/v1/logbooks/{logbook_id}/push`
- `GET /api/v1/sync/status`

The current self-hosted server uses an in-memory MVP backend. Durable server storage is required before real hosted use.

## Deferred Work

- Device pairing/trust UX.
- Signed official events.
- End-to-end encrypted relay.
- Real peer-to-peer HTTP transport for LAN replication.
- Divergence branch review and conflict resolution.
- Durable cloud server database.
