# v0.5 Release Plan

Last audited: 2026-08-31

`0.5` is the server-administration milestone for the locked November 24, 2026 v1
release. It is not the complete v1 product. `0.3.0` delivered the offline-sync
foundation and `0.4.0` gave every platform one shared way to use the hosted
account, session, recovery, and device APIs. v0.5 closes the remaining
account-area client gap that [v0.4 Release Plan](V0_4_RELEASE_PLAN.md) named as
its next increment: the hosted administration routes were implemented and
tested server-side but had no client surface on any platform.

The milestone was chosen for the same reason v0.4 was: of the three
[v1 Execution Plan](V1_EXECUTION_PLAN.md) next goals, it is the only one with no
external blocker. Sync/reconciliation hardening needs physical devices and Apple
provisioning, and provider qualification needs provider credentials and
approvals; administration UX needs neither.

## Scope

One vertical slice, finished on every client surface in the locked v1 scope:
hosted web, Windows/macOS/Linux desktop, native iOS, and the CLI.

In scope:

- Hosting configuration read and update: operation mode, registration mode, and
  the session, refresh, invitation, verification, and recovery lifetimes.
- Invitation management: create, list, inspect, resend, expire, and revoke.
- Audit review.
- One-time instance-administrator bootstrap, so an operator can create the first
  administrator on a fresh server from a client instead of by hand.
- A durable, non-secret hosted administration record holding administrator
  rights, cached hosting configuration, invitations, audit records, and
  last-outcome diagnostics.

Explicitly out of scope for v0.5, unchanged from v0.4:

- Production email deliverability, Turnstile site/secret keys, privacy/support
  URLs, infrastructure sizing, retention, monitoring, and deployment secrets.
  Those are operations work, not client work.
- Editing the hosted email delivery and Turnstile blocks from a client. The
  hosted `PATCH` route accepts both, but they carry a webhook URL, an email
  credential reference, and a Turnstile secret. Configuring secrets belongs with
  the operations work above, so v0.5 reads their redacted state and does not
  offer to write it.
- Everything already listed as out of scope in
  [v0.3 Release Plan](V0_3_RELEASE_PLAN.md) and
  [v0.4 Release Plan](V0_4_RELEASE_PLAN.md), which v0.5 does not change.

## Implemented In v0.5

- Shared hosted administration contract in `ham_sync::admin`: bounded action
  vocabulary, request planning, response interpretation, and a versioned JSON
  support store with atomic writes and corrupt-file quarantine.
- Administration is session-scoped rather than separately configured. The
  endpoint and the session credential both come from the hosted account record
  in `ham_sync::account`, so an operator administers exactly the server they are
  signed in to and there is no second endpoint setting to drift. Changing the
  account endpoint resets the cached administration record, so hosting,
  invitation, and audit state can never be shown for the wrong server.
- Outcome classification is shared with the account contract rather than
  duplicated: the same hosted `code` field drives both, so one error vocabulary
  covers the whole hosted surface.
- Administrator rights are recorded rather than assumed. An accepted response on
  any admin-gated route proves rights; a `forbidden` response records that the
  signed-in account is not a server administrator and drops the cached state; an
  authentication failure returns rights to unknown. A transport failure keeps
  what the operator already loaded.
- Secret discipline: the single-use invitation token issued by create and resend
  is returned exactly once, excluded from every serialized form of the result
  and the snapshot, and never written to the support record or to platform
  secure storage. The operator sees it once; the hosted server also emails it.
- Hosting updates send only the fields an operator actually set, so an update
  never rewrites a hosting value they did not look at. Empty updates and
  non-positive lifetimes are rejected before a request is sent.
- Retention bounds on the durable record: invitations and audit records are
  stored newest-first and capped, so a long-lived server cannot grow the support
  file without limit.
- Shared blocking HTTPS transport in `ham-sync` behind the existing
  `hosted-http` feature for desktop, hosted web, and CLI use.
- Hosted web and desktop: `/api/admin/*` endpoints in `ham-gui`, an Admin screen
  with hosting, invitation, and audit sections, an Admin toolbar entry,
  `admin.*` command-palette entries, and redacted `admin.*` runtime events.
- Native iOS: `admin.snapshot`, `admin.plan`, `admin.apply`, and
  `admin.transport_failure` bridge commands, typed Swift bridge methods, a
  URLSession transport, an Admin workspace, and a dashboard quick action.
- CLI: `ham-cli admin` subcommands with stable `--json` output, deterministic
  usage errors, and exit codes that separate acceptance from rejection.
- Account bootstrap on every surface: `ham-cli account bootstrap`, a browser
  "Claim server administrator" form, and the `account.bootstrap` action in the
  shared contract.

## Not Implemented In v0.5

- Hosted email and Turnstile configuration writing, per the scope note above.
- Invitation acceptance flows beyond the existing account registration path.
- Pagination for very large audit logs. The hosted route returns the whole log
  and the client bounds what it retains and renders; a server with a long
  history should get a paged route before this is called finished.
- Cookie-based hosted web sessions; the clients use bearer tokens.

## Validation Targets

```powershell
cargo test -p ham-sync admin
cargo test -p ham-gui admin
cargo test -p ham-ios-ffi admin
cargo test --workspace
just ci
```

Xcode and iOS simulator validation must run on macOS with Xcode tooling; the
Swift administration tests in `KE8YGWLoggerTests` cover planning, transport,
bearer-token use, invitation-token handling, invitation lifecycle rules, partial
hosting updates, and offline classification without a network.

The canonical product version is `0.5.0` across Cargo workspace metadata,
Tauri, the iOS marketing version, API product metadata, the CLI version
assertion, and documentation. Production release tags still come only from
validated semantic-version tags contained in `main`; this document does not
authorize a tag or publication.
