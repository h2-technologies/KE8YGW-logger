# v0.5.1 Release Plan

`0.5.1` is a domain-foundation release. It adds the two shared contracts that
the contest (#11) and EmComm (#12) epics are blocked on, and closes the
repository/architecture baseline (#3) and accounts (#4) epics after auditing
their remaining child issues against the shipped code.

It adds no client surface. Nothing in this release changes `/api/v1`.

## Scope

| Item | Issue | Outcome |
| --- | --- | --- |
| Versioned contest rule and exchange schema | #67 | `ham_core::contest`, signed definition packs, definition catalog, built-in generic templates. |
| Append-only EmComm record model | #72 | `ham_core::emcomm`, `official.log.emcomm.*` events, `proposal.emcomm.*` proposals, `emcomm.*` capabilities, `EmCommProjection`. |
| Product version unification | — | `0.5.1` across Cargo, Tauri, iOS marketing version, OpenAPI product metadata, the CLI version assertion, iOS bridge fallbacks, the governance pin, and documentation. |

## Issues closed in this release

| Issue | Why it is closed |
| --- | --- |
| #67 | Schema, loader, signature verification, catalog, built-in pack, tests, and `docs/CONTEST_RULE_SCHEMA.md`. |
| #72 | Record model, events, proposals, capabilities, projection, tests, and `docs/EMCOMM_RECORD_MODEL.md`. |
| #15, #16, #17, #19 | Baseline audit in `PROJECT_STATE.md`: iOS integration, scope/doc consistency, single version source and channels, and cross-platform CI are all in place on `dev`. |
| #21, #22, #23, #24, #25 | Account-foundation audit in `PROJECT_STATE.md`, plus the v0.4 account/session client flows and the v0.5 administration UX that were the remaining client gap. |
| #3, #4 | Every child issue is closed. |

## Not in this release

- The contest session and logging engine (#68), Field Day and Winter Field Day
  templates (#69), the December/January definition packs (#70), and Cabrillo
  export (#71).
- The ICS 211 (#73), ICS 213/213RR (#74), and ICS 214 (#75) form workflows, and
  the cross-platform incident UI and PDF/JSON export (#76).
- Any contest or EmComm client surface on hosted web, desktop, or iOS.

## Validation

```sh
just fmt-check
just clippy
just test
just feature-matrix
just api-contract
just version-check
just docs-link-check
just governance-check
```

`version-check` reports `0.5.1`. `api-contract` is unchanged because this
release adds no route. The iOS build number moves to `3`; the frozen
`/api/v1` `info.version` stays `1.0.0`.

## Release action

Publishing a `v0.5.1` tag remains a separate action governed by `RELEASE.md`.
