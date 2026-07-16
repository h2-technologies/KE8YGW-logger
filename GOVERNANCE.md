# Governance

KE8YGW Logger is a small, owner-led open-source project. Governance is designed
to keep decisions practical, traceable, and safe for users without adding heavy
process.

## Ownership

The repository is owned by `h2-technologies`. Austin Hadley is the project owner
identified in the 2026 MIT license notice. The project owner has final authority
over scope, security posture, releases, maintainer access, and repository
settings.

## Maintainer Authority

Maintainers may:

- triage issues and pull requests
- request changes or close work that is out of scope
- approve, merge, revert, or defer changes
- coordinate security fixes privately
- cut releases and update release notes
- update repository policy when the project changes

Maintainers should explain decisions when practical, especially for rejected
architecture changes or security-sensitive work.

## Contributor Expectations

Contributors are expected to follow [CONTRIBUTING.md](CONTRIBUTING.md), the
[Code of Conduct](CODE_OF_CONDUCT.md), and the existing architecture decisions.
Contributions should be focused, reviewable, tested, and linked to an issue.

## Review Expectations

Pull requests require maintainer review before merge. Reviewers should check
scope, correctness, tests, documentation, compatibility, and user impact.

Security-sensitive changes require explicit maintainer review from someone with
repository authority. CODEOWNERS should request review for high-risk areas, but
branch protection and required owner approval must be configured in GitHub
repository settings.

## Security-Sensitive Review

Changes involving authentication, authorization, sessions, account boundaries,
provider credentials, desktop credential stores, LoTW certificates, backups,
diagnostics, sync authorization, release signing, auto-update metadata, or
private user/QSO data require heightened review and documented testing.

Private vulnerability work may happen on private branches, advisories, or other
private maintainer channels until disclosure is coordinated.

## Release Approval

The project owner or a delegated maintainer approves public releases. Release
approval should confirm version, branch or tag, CI status, artifacts, checksums,
known migrations, rollback guidance, release notes, and security implications.

## Architecture Decisions

Architecture decisions are recorded in `docs/adr`. Use the ADR template for
accepted or proposed decisions that materially affect architecture,
compatibility, data, security, or release behavior. ADRs should be reviewed by a
maintainer before acceptance.

## Conflict Resolution

Maintainers should first resolve disagreements through written technical
discussion in the relevant issue, pull request, or ADR. If consensus is not
clear, the project owner decides. Decisions can be revisited with new evidence,
but repeated re-litigation without new information may be closed as out of
scope.

## Adding Maintainers

Maintainers may be added when they have a sustained record of useful
contributions, sound judgment, respectful collaboration, and care with security
and user data. The project owner grants access and defines the permission level.

## Removing Maintainers

Maintainer access may be removed for inactivity, loss of trust, repeated policy
violations, security mishandling, or a change in project needs. Emergency access
removal may happen immediately when user safety, credentials, or repository
integrity are at risk.

## Abandoned Work

Issues and pull requests may be marked stale, reassigned, closed, or continued
by another contributor when work is inactive and blocks project progress.
Maintainers should preserve useful context and credit, but unfinished work does
not reserve an area indefinitely.
