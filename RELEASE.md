# Release Policy

This policy describes release expectations for KE8YGW Logger. It distinguishes
repository policy from automation that already exists.

## Channels

- Stable: public releases intended for normal users.
- Beta: pre-release builds for broader validation before stable.
- Internal: maintainer or CI-produced builds used for development and validation.

Pre-1.0 releases may change faster than stable releases, but compatibility risks
must still be documented when user data, APIs, or sync behavior are affected.

## Versioning

The workspace currently uses version `0.2.0` and Rust edition 2021. Versions
should be updated consistently across workspace metadata, crate metadata,
release notes, tags, artifacts, and documentation.

Release tags should use `vMAJOR.MINOR.PATCH`, for example `v0.2.0`. Additional
pre-release identifiers may be used for beta releases when needed.

## Branches And Tags

`main` is the active integration branch. Release branches may be created when a
release line needs stabilization or security fixes separate from `main`.

The existing GitHub release workflow runs on tags matching `v*.*.*` and builds
release binaries for Linux, Windows, and macOS. It packages the `ham-gui` binary
and uploads archives to the GitHub Release.

## Required Gates

Before a public release, maintainers should confirm:

- `just ci` passes.
- Platform-specific desktop or server checks relevant to the release pass.
- JavaScript syntax checks pass when web UI files changed.
- Documentation and governance validation pass.
- Known migrations, compatibility impacts, and rollback guidance are documented.
- Security-sensitive changes received maintainer review.

Do not weaken CI gates to ship a release. If a gate is intentionally deferred,
the release notes must explain the risk.

## Signing, Checksums, And SBOM

Release artifacts should have checksums. Release signing and SBOM generation are
expected before production-quality stable distribution, but this repository does
not currently automate signing or SBOM publication. Until automation exists,
release notes must clearly state which provenance, checksum, signing, and SBOM
steps were completed manually.

## Migrations And Rollback

Releases that change official event schemas, support storage, credential
metadata, sync protocol behavior, database layout, backup format, provider
configuration, or `/api/v1` compatibility must document:

- forward migration path
- rollback or recovery path
- projection rebuild expectations
- backup guidance
- compatibility with older clients, peers, and servers

Official event history must remain append-only. Rollback plans must not require
rewriting user history.

## Approval

The project owner or delegated release maintainer approves public releases.
Approval should cover scope, tag, CI status, artifacts, known risks, release
notes, and security posture.

## Emergency Security Releases

Emergency security releases may use a narrowed process to reduce exposure. The
release should still preserve required CI gates where practical, avoid unrelated
changes, document the fixed versions, and coordinate public disclosure through a
security advisory or maintainer-approved release notes.

## Post-Release Verification

After publishing a release, maintainers should verify:

- GitHub Release assets are present.
- Archive names match the documented platform matrix.
- Checksums and signatures, if produced, match the uploaded artifacts.
- Release notes include compatibility, migration, and security notes.
- Install or startup smoke checks pass on the targeted platforms when practical.
- Any manual repository settings or follow-up tasks are tracked.
