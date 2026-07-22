# Sync Protocol

Sync is local-first. LAN discovery and direct LAN replication are preferred. Cloud relay and self-hosted sync are fallback paths when devices cannot reach each other locally.

## Privacy Rules

- Do not sync runtime diagnostic logs.
- Do not sync credentials, API keys, private plugin configuration, or support cache data by default.
- Do not let peers mutate local official logs directly.
- Do not auto-merge divergent chains in MVP.
- Treat peers as untrusted unless they pass the durable LAN trust store or
  cloud/self-hosted sync authentication path.

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

### Offline Mutation Queue

The v0.3 queue contract is implemented in `ham-sync::offline` and persisted as
versioned JSON support state named `offline-mutations.json` by desktop and iOS
clients.

Each mutation envelope records:

- queue schema version
- operation, correlation, client, device, logbook, and optional target entity
  IDs
- deterministic per-logbook sequence number
- operation type and idempotency key
- dependencies on accepted operation IDs, event hashes, or minimum schema
  version
- redacted payload copy for replay/diagnostics
- status: `pending`, `sending`, `retrying`, `blocked`, `failed`, `accepted`,
  or `user_action_required`
- attempts, bounded exponential backoff, and next retry time
- accepted local official event ID/hash when the operation created official
  history

Clients persist the queue entry before acknowledging the local mutation. If core
domain validation rejects the proposal, the queue entry moves to
`user_action_required`; the official event stream is not edited. If the app
crashes while a send is `sending`, startup recovery moves it back to `retrying`.
Queued official events drain in deterministic logbook order, and a later event
does not bypass an earlier blocked, failed, retrying, or dependency-missing
operation.

Manual/startup recovery uses a redacted `OfflineQueueRecoveryReport`. The shared
Rust recovery path initializes absent or blank pre-v0.3/v0.2 queue state as an
empty current queue, migrates conservative legacy `version: 0` queue records into
current envelopes, promotes a valid interrupted atomic-write temp file when the
main queue file is missing, removes stale temp writes, and quarantines corrupt
queue JSON before creating a fresh empty current file. Unsupported current file
versions, mutation schema versions, invalid dependencies, and duplicate
per-logbook sequences still fail closed instead of being silently repaired.

Station/equipment commands are queued as support-state mutations and marked
accepted after the support store write succeeds. They remain support state and
are not official logbook history.

### Preview Pull

Preview pull compares local and remote chain metadata and reports how many
events would be fetched. Event metadata includes the official event ID, logbook
ID, optional entity ID, previous/event hashes, timestamp, event type, and schema
version. It writes nothing.

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

Desktop cloud push now uses the offline queue when queued local official events
are present. Queue entries are marked `sending` before transport and `accepted`
only after the cloud/self-hosted receiver accepts or ignores the matching event
hashes. The deterministic
`desktop_queue_recovers_restart_and_drains_to_cloud_without_duplicates` test
covers recovery of an interrupted desktop send, reconnect drain ordering,
accepted-by-hash queue cleanup, duplicate cloud replay handling, and local
official-log duplicate prevention. When cloud sync reconnects and auto-push is
enabled, the GUI runs a queue-only auto-drain: ready queued official mutations
are pushed and accepted by hash, while unrelated unqueued local official history
is not opportunistically published. `cloud_connect_auto_push_drains_recovered_desktop_queue`
and `cloud_connect_auto_push_skips_unqueued_local_history` cover the reconnect
auto-drain and queue-only guard paths. Divergence blocks the queued operations
for manual review.

iOS exposes the same Rust-owned drain policy through FFI commands.
`sync.offline_queue.retry_plan` recovers interrupted writes, applies bounded
batch sizing, optionally refuses to plan work while the native network monitor
reports no connectivity, marks planned mutations `sending`, returns the exact
official event envelopes and hashes for native transport, and moves queued
mutations without matching local official events to `user_action_required`.
`sync.offline_queue.retry_result` records accepted hashes, transient transport
failures, auth or validation failures, divergence, and missing-event failures
back into the queue. Transient failures move to bounded retry/backoff; auth,
validation, missing-event, permanent, and divergence results stop unattended
retry and require operator review.

### Manual Conflict Review

`ham-sync::offline` defines durable conflict-review records persisted as
`conflict-reviews.json` by desktop and exposed through the iOS FFI bridge.
Review records capture the structured conflict report, a stable fingerprint,
open/resolved status, timestamps, and the operator-selected recovery path.
Conflict reports now classify divergent heads, missing queue dependencies,
unsupported remote event schema versions, concurrent QSO corrections, and remote
QSO tombstone/restore events that affect QSOs with local pending mutations.
Those report classes are advisory client diagnostics; pull/push still rely on
the shared event-chain verifier before any append.

Allowed recovery decisions are explicit:

- `keep_local_history`
- `pull_remote_after_review`
- `create_corrective_events`
- `retry_after_dependency_arrives`
- `mark_user_action_required`

The shared validator rejects `pull_remote_after_review` when a report is
`diverged` or contains any non-auto-merge-safe conflict. It also requires
corrective event hashes before a review can be resolved as
`create_corrective_events`. Desktop endpoints can create a review from the
latest LAN/cloud preview, resolve it, mark related queued mutations as
`user_action_required`, and resolve a review by submitting explicit corrective
proposals through the normal core proposal pipeline. The GUI includes a guided
browser review surface for saved review selection, structured conflict
summaries, explicit recovery-path choices, and form-based corrective QSO note
events that record the resulting official event hash on the review. iOS can
create, resolve, snapshot, and resolve with corrective proposal events through
Rust bridge commands.

## LAN Trust

`ham-sync::offline` includes durable LAN trust records persisted as
`lan-trust.json` by the GUI. The trust model includes:

- explicit operator approval before issuing a pairing token
- short-lived single-use pairing tokens stored only as hashes
- trusted device records scoped to logbook IDs
- `auth_credential_id` references for pairing-derived LAN request secrets
- credential-reference rotation for recovery when a LAN auth code must change
- optional public-key fingerprint metadata for future signed transport
- immediate revocation
- replay nonce hashing and rejection

The GUI exposes trust-state, pairing-token, pairing-accept, pairing-complete,
auth-rotation, and revoke endpoints. The browser Sync panel wraps those
endpoints in guided LAN pairing/trust controls for issuing local one-time
codes, entering peer token/code/fingerprint values, completing reciprocal
pairing, generating replacement auth codes, rotating LAN auth, and revoking
trusted peers. `pairing-complete` posts the
operator-entered peer token and pairing code to the selected peer, stores the
accepted pairing code as a LAN auth credential through `CredentialStore`, and
records only the resulting credential ID in durable trust state.
`/api/sync/lan/rotate-auth` lets an operator replace a trusted peer's LAN auth
credential reference for the current logbook; the replacement secret is stored
through `CredentialStore`, the trust record is updated only after the new
credential is stored, and the previous credential reference is deleted after a
successful rotation. This is the recovery path for missing or intentionally
rotated LAN endpoint-auth credentials. It also exposes a manual LAN peer-add
endpoint that probes another GUI instance over a numeric
loopback/private/link-local `http://ip:port`, reads `/api/sync/state` for the
peer identity, stores the peer with its advertised API port, then uses protected
`/api/sync/get-head` and `/api/sync/events-since` requests for direct
preview/pull. LAN `list-logbooks`, `get-head`, `events-since`, and
`event-metadata` requests must include these headers:

- `x-ke8ygw-lan-device-id`: requester device ID
- `x-ke8ygw-lan-replay-nonce`: fresh requester nonce
- `x-ke8ygw-lan-signature-version`: `hmac-sha256-v1`
- `x-ke8ygw-lan-signature`: lowercase hex HMAC-SHA256 signature

The signature covers the signature version, requester device ID, target
logbook ID, HTTP method, exact request target, and replay nonce. The serving
peer verifies the signature with the trusted peer's stored auth credential,
then authorizes the request against durable trust state, logbook scope,
revocation state, and replay-nonce history before returning logbook or event
data. `/api/sync/state` remains unauthenticated for discovery identity probes
and must not include secrets or log contents.

Existing trust records that predate `auth_credential_id` load safely because
the field is optional, but they cannot authorize protected LAN reads. Re-pair
those peers or rotate auth for an existing trusted record to create a
credential-store-backed LAN auth secret.

When LAN discovery is started, the GUI runs an IPv4/IPv6 multicast discovery
worker. Each cycle binds reusable discovery sockets, sends the local discovery
packet, listens for packets, ignores malformed datagrams and self packets, and
probes the sender IP plus advertised API port at `/api/sync/state`. A peer is
recorded only when the probed identity matches the discovery packet. Unscoped
IPv6 link-local sources are ignored because they cannot be used for direct HTTP.
Stale peers expire by the configured timeout. Automatic discovery requires the
remote GUI instance to be participating in discovery and to serve its API from a
LAN-reachable bind address; loopback-only peers remain supported through manual
loopback URLs.

Mutating LAN pull rejects untrusted, revoked, wrong-logbook, or replayed peers
before appending any remote official events, and serving LAN read endpoints
reject untrusted, revoked, wrong-logbook, or replayed requesters before
returning logbook or event data. The current threat boundary is: discovery
packets contain no secrets or log contents; discovery identity probes prove
reachability and reduce spoofing; official event writes remain local and
trust-gated; protected LAN read endpoints require reciprocal trust state,
fresh nonces, and HMAC-SHA256 request proof. The current LAN HTTP transport is
still not encrypted and must not be exposed outside trusted local networks.
Production iOS reciprocal LAN pairing UX, stronger LAN key-exchange hardening,
physical-device LAN validation, and iOS Local Network permission validation
remain before unattended LAN sync is considered complete.

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

The current self-hosted server uses durable local storage by default: embedded SurrealDB metadata/support state, append-only JSONL official-event storage, and filesystem-backed diagnostic report payloads. Durable SurrealDB storage is exposed through the `ham-sync` `surreal-storage` feature so GUI, iOS, and other protocol-only clients can avoid the database dependency. The in-memory backend remains for deterministic tests.

## Deferred Work

- Production iOS reciprocal LAN pairing UX over the durable trust store.
- Signed official events.
- End-to-end encrypted relay.
- Stronger LAN key-exchange hardening and production iOS reciprocal LAN pairing UX.
- Physical-device LAN and iOS Local Network permission validation.
- End-to-end cross-client branch review and reconciliation workflow beyond the
  current guided browser review surface and explicit corrective-event commands.
- Durable cloud server database.
