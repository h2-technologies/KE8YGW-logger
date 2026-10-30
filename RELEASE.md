# Release Policy

This policy describes release expectations for KE8YGW Logger v1. It
distinguishes repository policy from automation that already exists.

## Channels

- Production: public releases built only from `vMAJOR.MINOR.PATCH` tags on
  commits contained in `main`.
- Beta: builds from `main` for club testing before production.
- Internal: maintainer or CI builds from `dev`.

See [docs/BRANCHING_AND_RELEASE_CHANNELS.md](docs/BRANCHING_AND_RELEASE_CHANNELS.md)
for the branch and rollback policy.

## Versioning

`Cargo.toml` `[workspace.package].version` is the canonical product version.
The v1 line is at `1.0.0-alpha.0`. Pre-release identifiers (`-alpha.N`,
`-beta.N`, `-rc.N`) are used until the Winter Field Day release, which is
`1.0.0`. Release tags are `v<version>`.

## Current Automation

None. The v0 release workflow was retired with the v0 code. Milestone 6 of
[the design spec](docs/V1_DESIGN_SPEC.md) adds installers for Windows, macOS
and Linux, with Windows code signing and macOS notarization. Until then, no
release artifacts are published.

Do not push tags matching `v*.*.*` for anything that is not a release.

## Required Gates

Before a public release, maintainers confirm:

- CI passes on `main` (format, Clippy and tests on all three OSes, and the
  Raspberry Pi check).
- The soak test and hardware checks listed in section 14 of the spec were run,
  and their results are recorded.
- Compatibility impacts on event types, contest definitions, the sync protocol
  and the Node API are documented, with migration and rollback guidance.
- Security-sensitive changes received maintainer review.

Do not weaken CI gates to ship a release. If a gate is deferred, the release
notes explain the risk.

## Migrations And Rollback

The event log is append-only. A release that changes event types or their
fields must keep reading older logs, and rollback plans must never require
rewriting a station's log. The SurrealDB projection is disposable: a schema
change rebuilds it from the logs.

## Approval

The project owner or a delegated release maintainer approves public releases.
Approval covers scope, tag, CI status, artifacts, signing, known risks and
release notes.
