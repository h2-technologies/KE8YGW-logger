# v0.4 Release Plan

Last audited: 2026-08-31

`0.4` is the account-and-session milestone for the locked November 24, 2026 v1
release. It is not the complete v1 product. `0.3.0` delivered the offline-sync
foundation; the hosted account, session, recovery, and device APIs shipped with
the v1 account foundation but had no client surface. v0.4 gives every platform
in scope one shared, Rust-authoritative way to use them.

The milestone was chosen because it is the only one of the three
[v1 Execution Plan](V1_EXECUTION_PLAN.md) next goals with no external blocker.
Sync/reconciliation hardening needs physical devices and Apple provisioning, and
provider qualification needs provider credentials and approvals; account UX
needs neither.

## Scope

One vertical slice, finished on every client surface in the locked v1 scope:
hosted web, Windows/macOS/Linux desktop, native iOS, and the CLI.

In scope:

- Account registration, email verification, and account recovery.
- Sign in, session read, session rotation, sign out, and sign out everywhere.
- Device listing, single-device revocation, and revoke-all.
- Hosted account deletion.
- Durable, non-secret hosted account record with connection state, account
  identity, cached devices and logbooks, and last-outcome diagnostics.
- Secure storage of session and refresh tokens in the operating-system
  credential backend or the iOS Keychain, referenced only by credential ID.

Explicitly out of scope for v0.4, tracked as the next increment:

- Hosted server administration UX: hosting-mode configuration, invitation
  create/list/resend/expire/revoke, and audit review. The hosted routes exist
  and are tested; only the client surfaces are missing.
- Production email deliverability, Turnstile site/secret keys, privacy/support
  URLs, infrastructure sizing, retention, monitoring, and deployment secrets.
  Those are operations work, not client work.

## Implemented In v0.4

- Shared hosted account contract in `ham_sync::account`: bounded action
  vocabulary, request planning, response interpretation, outcome
  classification, transport-failure classification, and a versioned JSON
  support store with atomic writes and corrupt-file quarantine.
- Stable outcome classification driven by the hosted `code` field first and the
  HTTP status only as a fallback, with explicit `retryable` and
  `user_action_required` flags.
- Secret discipline: issued session and refresh tokens are returned exactly
  once, excluded from every serialized form, and stored only in platform secure
  storage under Rust-assigned credential identifiers. Authentication failures
  clear both the identifiers and the stored secrets.
- Shared blocking HTTPS transport in `ham-sync` behind the `hosted-http`
  feature for desktop, hosted web, and CLI use.
- Hosted web and desktop: `/api/account/*` endpoints in `ham-gui`, an Account
  screen with sign-in, registration, verification, recovery, session, device,
  and deletion flows, an Account settings summary card, command-palette
  entries, and redacted `account.*` runtime events.
- Native iOS: `account.snapshot`, `account.configure`, `account.plan`,
  `account.apply`, and `account.transport_failure` bridge commands, typed Swift
  bridge methods, a URLSession transport, Keychain secret storage, an Account
  workspace, and a dashboard quick action.
- CLI: `ham-cli account` subcommands with stable `--json` output, deterministic
  usage errors, and exit codes that separate acceptance from rejection.

## Not Implemented In v0.4

- Hosted server administration client surfaces.
- Cookie-based hosted web sessions; the clients use bearer tokens.
- Password or passkey authentication. The hosted login route currently issues a
  session for a verified account by email, and the clients follow that route
  rather than inventing their own credential model.
- Everything already listed as out of scope in
  [v0.3 Release Plan](V0_3_RELEASE_PLAN.md), which v0.4 does not change.

## Validation Targets

```powershell
cargo test -p ham-sync account
cargo test -p ham-gui account
cargo test -p ham-ios-ffi account
cargo test --workspace
just ci
```

Xcode and iOS simulator validation must run on macOS with Xcode tooling; the
Swift hosted-account tests in `KE8YGWLoggerTests` cover planning, transport,
Keychain storage, credential clearing, and offline classification without a
network.

Production release tags still come only from validated semantic-version tags
contained in `main`; this document does not authorize a tag or publication.
