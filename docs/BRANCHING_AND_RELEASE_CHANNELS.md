# Branching And Release Channels

This repository uses three long-lived channels:

- `dev` is the internal integration branch for normal feature and fix work.
- `main` is the beta-tester branch. It is updated by promotion pull requests from `dev`.
- `production` is a release channel represented by immutable `vMAJOR.MINOR.PATCH` tags on commits contained in `main`.

## Normal Flow

1. Create a feature or fix branch from the latest `dev`.
2. Open a pull request targeting `dev`.
3. Let internal validation and any internal artifacts complete.
4. Promote validated work with a `dev` to `main` pull request.
5. Let beta validation and beta artifacts complete on `main`.
6. Create a production tag only on a commit already contained in `main`.

Ordinary feature branches must not target `main` directly.

## Hotfix Exception

Emergency hotfixes may branch from `main` and open a pull request to `main` only when the PR description documents the immediate synchronization plan back into `dev`. The `Main promotion policy` job rejects other PRs targeting `main`.

## Pull Requests To `dev`

Pull requests targeting `dev` run repository validation without publishing beta or production releases. The expected checks include formatting, Clippy, tests, API contract validation, governance validation, and targeted platform checks when those workflows are enabled.
Security scanning also runs for pull requests into `dev` and uploads SARIF only
when the event is allowed to use repository code-scanning permissions.

## Pushes To `dev`

Pushes to `dev` are internal integration builds. Internal artifacts must:

- include the commit SHA and workflow run number,
- use names beginning with `internal-dev`,
- use short retention unless an internal publishing destination is configured,
- never use production credentials or production environments,
- never overwrite production assets.

The current source-controlled fallback is a GitHub Actions artifact containing internal build metadata. No external internal publishing destination is configured in this repository.

## Promotion Pull Requests To `main`

A normal PR targeting `main` must originate from `dev`. Hotfix branches must use `hotfix/*` and document follow-up synchronization into `dev`.

The beta gate should include complete Rust quality validation, Windows/macOS/Linux validation, Tauri checks, iOS validation, release-mode compilation where needed, packaging validation, container validation, API contract checks, and governance checks.
It should also include the Security scanning workflow so Cargo advisory checks,
Semgrep SAST, and workflow linting stay current before promotion.

## Pushes To `main`

Pushes to `main` represent the beta channel. Beta artifacts must be clearly named with `beta-main`, include the commit SHA and workflow run number, and must not overwrite or resemble production assets.

The current fallback is a GitHub Actions beta manifest artifact. If maintainers want a GitHub prerelease or external beta publishing destination, configure that destination explicitly with a separate `beta` environment and non-production credentials.

## Production Tags

Production release automation only runs for tags matching `vMAJOR.MINOR.PATCH`. Before building, the workflow verifies:

- the tag format,
- sufficient git history is available,
- the tagged commit is contained in `origin/main`,
- the workspace version in `Cargo.toml` matches the tag,
- the tagged commit has a successful `CI` run on `main`.

Production tag workflows must not use branch concurrency cancellation. They publish GitHub Release assets only from valid production tags.

## Environments And Permissions

Recommended GitHub environment names are:

- `internal` for dev-only publishing,
- `beta` for beta-tester publishing from `main`,
- `production` for protected production release publication.

Do not copy production secrets into internal or beta environments. Production should require environment protection or approval before any signing, notarization, registry push, or release-publication secret is usable.

## Artifact Naming

- Internal: `internal-dev-<sha>-<run_number>`
- Beta: `beta-main-<sha>-<run_number>`
- Production: `ham-platform-<platform>` release archives attached to a validated production tag

Production archives keep their existing names for compatibility. Internal and beta artifacts are deliberately prefixed by channel.

Future production release archives and their `.sha256` checksum files receive
GitHub artifact attestations before the release assets are uploaded. Historical
release assets are not retroactively attested by the workflow.

## Version Rules

Workspace version lives in `Cargo.toml` under `[workspace.package]`. Production tags must match that version exactly as `v<version>`. Do not create production tags from commits not contained in `main`.

## Rollback

- Internal rollback: revert or fix forward on `dev`.
- Beta rollback: revert or fix forward on `dev`, then promote `dev` to `main`; remove or supersede beta artifacts if needed.
- Production rollback: publish a new patch version from a commit contained in `main`. Do not move or overwrite an existing production tag.

## Required Repository Settings

Source files cannot fully configure repository branch protection. Maintainers should configure:

- `dev` branch protection requiring CI checks for normal PRs.
- `main` branch protection requiring the main promotion policy and the complete beta gate.
- Required Security scanning checks for protected branches.
- CODEOWNERS review for workflow, dependency, release, security, and core source paths.
- Stale approval dismissal and approval of the most recent push where supported.
- Rules preventing direct pushes to `main` except approved maintainers or release automation.
- Tag protection or rulesets for `v*.*.*` production tags.
- A protected `production` environment for production signing and release secrets.
- Separate `internal` and `beta` environments if external publishing is added.
- Required status checks that complete successfully when change-aware jobs intentionally skip expensive work.

## Manual External Configuration

No container registry, internal artifact store, beta distribution destination, Apple signing identity, notarization profile, or production signing secret is currently represented in source. Until those are configured, workflows should build and upload clearly marked GitHub Actions artifacts instead of inventing destinations.
