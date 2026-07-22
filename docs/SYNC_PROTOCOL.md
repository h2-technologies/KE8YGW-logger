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
retry and require operator review. The native Swift bridge decodes the typed
queue snapshot, recovery report, retry plan, retry result, and affected
mutations. The iOS Sync workspace displays Rust queue health/mutation status
and asks Rust for retry plans using the native network monitor state; it marks
mutations `sending` only through the Rust retry-plan command when Swift
transport is ready to process the returned events. Swift also decodes the
Rust-planned official event envelopes and can construct both the self-hosted
logbook-scoped `/api/v1/logbooks/{logbook_id}/push` request and the hosted
`/api/v1/sync/push` request from those envelopes without creating or validating
official history itself. The native retry executor now performs the
Rust-plan -> Swift-transport -> Rust-result sequence for the configured
sync-token push path, including accepted-prefix recording when a receiver
accepts early events and rejects a later event.
Application settings include an additive `sync_endpoint_style` value. The
default `logbook_scoped` value preserves self-hosted sync-server behavior, while
`hosted_sync` routes native manual and background retry through hosted
`/api/v1/sync/push` and `/api/v1/sync/pull` endpoints. Missing legacy values
default to `logbook_scoped`; unsupported persisted Rust enum values fail to
deserialize instead of silently changing transport semantics.
The native iOS bundle declares the permitted background retry task identifier
and background processing mode. Swift schedules that `BGProcessingTask` only
when Rust settings enable background sync, a valid sync server URL and Keychain
sync token are present, and either the Rust queue snapshot reports pending work
or Auto Pull is enabled. The task handler delegates to the same Rust-plan ->
Swift-transport -> Rust-result executor; after a clean accepted push or a
no-ready-events push plan, Auto Pull can fetch remote official envelopes through
the configured endpoint style and pass them to `sync.remote_events.apply`. It
does not create official events or classify domain failures in Swift, and it
does not pull after auth, validation, divergence, missing-event, blocked,
partial-failure, or transient push results. Simulator tests cover both the
clean-push and no-ready queue-plan Auto Pull paths; the no-ready test proves no
push transport is invoked before the pull/apply path.
`cross_client_golden_partial_push_accepts_prefix_and_blocks_rejected_tail`
proves the shared Rust cloud/queue path accepts the valid prefix, blocks the
rejected tail as `user_action_required`, avoids local and cloud duplicates, and
can complete the reviewed tail by accepted event hash.
`cross_client_golden_revoked_cloud_auth_blocks_queue_until_repaired` proves a
queued push whose cloud auth is revoked stops as `user_action_required`, appends
nothing remotely, plans no unattended retry, and drains only after re-pairing
and accepted-hash acknowledgment.
`cross_client_golden_expired_cloud_auth_blocks_queue_until_repaired` proves the
same queue behavior for a bounded cloud session whose token has expired; the
remote remains unchanged until the device re-pairs and accepted hashes drain the
queue.
`sync_retry_plan_recovers_terminated_send_and_blocks_without_network`
proves a terminated `sending` operation is recovered before planning and that a
poor-network state returns a blocked no-op plan without losing queued work.

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
Rust bridge commands. The native Swift bridge decodes the saved review list,
selected recovery path, structured conflict messages, and review health; the
iOS Sync workspace displays open reviews, recommended actions, peer IDs, and
conflict details without owning merge or validation rules.

## LAN Trust

`ham-sync::offline` includes durable LAN trust records persisted as
`lan-trust.json` by GUI and iOS bridge clients. The trust model includes:

- explicit operator approval before issuing a pairing token
- short-lived single-use pairing tokens stored only as hashes
- trusted device records scoped to logbook IDs
- `auth_credential_id` references for LAN endpoint-auth secrets created during
  pairing or rotation
- credential-reference rotation for recovery when a LAN auth code must change
- optional public-key fingerprint metadata for future signed transport
- immediate revocation
- replay nonce hashing and rejection

`local-sync-identity.json` is separate durable support state used by desktop
and iOS clients to keep the local device ID stable across restarts. The file
stores version, device ID, display name, optional user hash, and timestamps.
Runtime discovery session IDs are regenerated on load and are not persisted.

The GUI exposes trust-state, pairing-token, pairing-accept, pairing-complete,
auth-rotation, and revoke endpoints. The browser Sync panel wraps those
endpoints in guided LAN pairing/trust controls for issuing local one-time
codes, entering peer token/code/fingerprint values, completing reciprocal
pairing, generating replacement auth codes, rotating LAN auth, and revoking
trusted peers. `pairing-complete` posts the operator-entered peer token and
pairing code plus a browser-generated `auth_code` to the selected peer, stores
that generated endpoint-auth code through `CredentialStore` on both sides, and
records only the resulting credential IDs in durable trust state. The
`pairing-accept` endpoint requires a distinct endpoint `auth_code` and rejects
requests that omit it or try to reuse the one-time pairing code as the
long-lived LAN endpoint auth secret.
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
preview/pull.

Native iOS exposes the same durable trust store through Rust FFI commands:
`sync.lan_trust.snapshot`, `sync.lan_trust.issue_pairing_token`,
`sync.lan_trust.accept_pairing_token`, `sync.lan_trust.trust_peer`,
`sync.lan_trust.rotate_auth`, and `sync.lan_trust.revoke`. The iOS Sync
workspace can issue a local one-time code, accept a locally issued pairing code
for a typed peer, directly trust a peer, rotate Keychain-backed LAN auth
credentials, revoke trust, and complete reciprocal pairing against an
operator-entered peer URL. The accept-pairing command requires an
`auth_credential_id`; Swift creates the LAN auth secret in Keychain first and
Rust persists only that credential ID. In the reciprocal URL flow, Swift probes
the peer's `/api/sync/state` identity, posts the operator-entered peer
token/code plus a generated endpoint auth code to the peer's
`/api/sync/lan/pairing-accept`, then stores only a local credential reference
through the Rust trust command after the remote side accepts. `sync.snapshot`
returns the durable local identity, and issue-token uses that local device ID
when the caller does not provide an issuer device ID. The native Swift LAN pull
executor signs protected `get-head` and `events-since` requests for an
operator-entered trusted peer URL with the Keychain-backed LAN auth secret,
constructs the preview from the returned head/event range, and applies pulled
official envelopes only through `sync.remote_events.apply`. Pairing codes are
returned only from the issue-token command and are not present in snapshots.

LAN `list-logbooks`, `get-head`, `events-since`, and `event-metadata` requests
must include these headers:

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
Native iOS manual LAN pull also probes `/api/sync/state` and rejects a peer
whose published device ID does not match the selected trusted peer before
sending signed `get-head` or `events-since` reads.
The iOS Sync workspace can also start an IPv4/IPv6 multicast discovery scanner
using the same secret-free discovery packet shape. The scanner derives a
candidate peer URL from the sender address and advertised API port, probes
`/api/sync/state`, lists only peers whose probed device/session identity matches
the discovery packet, and lets the operator copy that discovered peer into the
existing pairing/pull controls. The iOS app target declares the Apple multicast
networking entitlement required by this scanner; Apple Developer account
approval/provisioning, physical-device LAN validation, and physical iOS Local
Network permission validation remain before unattended LAN sync is considered
complete.

## Cloud Relay and Self-Hosted Sync

Cloud sync reuses the same event envelopes and verification path. MVP auth uses
pairing-code/token sessions scoped to account, user, device, explicit logbook
IDs, and a bounded `expires_at` value. New self-hosted sessions default to a
30-day TTL through `HAM_SYNC_SESSION_TTL_SECONDS`; legacy stored sessions that
lack `expires_at` still deserialize for compatibility, but new bounded sessions
are rejected after expiry before any logbook read or write.

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

The hosted `ham-server` API exposes bearer/session-scoped sync push as
`POST /api/v1/sync/push`; the logbook-scoped routes above are the self-hosted
sync-server compatibility surface used by sync-token clients.
`ham-server` binary loopback TCP wire tests cover hosted admin bootstrap,
proposal-backed QSO creation, hosted sync pull, duplicate hosted sync push, and
durable JSONL official-event storage without duplicate replay.
`ham-sync-server` route and loopback TCP wire tests cover device pairing, scoped
logbook listing, canonical official-event push, duplicate replay handling, pull
of missing events, invalid-token rejection, and expired-token rejection against
the durable self-hosted backend.

Pull application uses the same Rust verification path for hosted, self-hosted,
LAN, desktop, and iOS clients. `ham-sync::pull_missing_events` accepts either a
full remote chain that contains the local head or a verified missing tail whose
first event directly follows the actual local store head. In both cases, every
accepted event is appended through `append_verified_remote_event`; divergent
heads, unsupported schemas, broken hashes, and wrong previous hashes remain
rejected before mutation. The iOS FFI command `sync.remote_events.apply`
exposes that path to native transports by accepting full official event
envelopes, returning the shared pull response, and refreshing the Rust-owned
sync/projection counts without letting Swift validate or create official
history. Native Swift builds self-hosted/logbook-scoped and hosted pull
requests using the Rust snapshot's `logbook_id` and `local_head_hash`, then
passes returned envelopes back through `sync.remote_events.apply`. After Rust
accepts remote events, manual iOS pull, trusted LAN pull, and background Auto
Pull refresh the native SwiftData QSO cache from the Rust `qso.list` projection;
SwiftData remains a projection cache, not an official state owner.

The current self-hosted server uses durable local storage by default: embedded SurrealDB metadata/support state, append-only JSONL official-event storage, and filesystem-backed diagnostic report payloads. Durable SurrealDB storage is exposed through the `ham-sync` `surreal-storage` feature so GUI, iOS, and other protocol-only clients can avoid the database dependency. The in-memory backend remains for deterministic tests.

## Deferred Work

- Apple Developer account approval/provisioning and release-device validation
  for the declared native iOS multicast entitlement over the durable trust
  store.
- Signed official events.
- End-to-end encrypted relay.
- Formal asymmetric LAN key exchange beyond the current distinct endpoint-auth
  code plus HMAC request-proof model.
- Release-device iOS LAN discovery validation.
- Physical-device LAN and iOS Local Network permission prompt validation.
- Release-device iOS BGTask execution and poor-network behavior beyond the
  current bundle declarations, scheduler eligibility policy, and
  simulator-safe Swift tests.
- Release-device cross-client branch review and reconciliation workflow beyond
  the current deterministic shared golden tests, guided browser review surface,
  desktop/iOS review stores, and explicit corrective-event commands.
- Durable cloud server database.
