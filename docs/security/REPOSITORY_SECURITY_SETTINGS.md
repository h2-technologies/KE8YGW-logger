# Repository Security Settings Checklist

Last reviewed: 2026-07-18

The following settings cannot be completed honestly from source code alone.
Maintainers should configure them in GitHub repository or organization settings.

## Branch Protection And Rulesets

- Require at least one approving review for pull requests into `dev` and `main`.
- Dismiss stale approvals after new commits.
- Require approval of the most recent push.
- Require CODEOWNERS review for protected and security-sensitive paths,
  including `.github/**`, `Cargo.toml`, `Cargo.lock`, `deny.toml`,
  `Dockerfile.sync-server`, `ios/**`, `src-tauri/**`, `crates/**`, and
  `docs/security/**`.
- Require the CI, iOS Native, Branch promotion policy, and Security scanning
  checks that apply to the changed paths.
- Require branches to be current before merge where practical.
- Prevent force pushes and branch deletion on `dev` and `main`.
- Apply rules to administrators where appropriate.
- Preserve the policy that normal work targets `dev` and `main` is promoted
  from `dev`.

## Security Features

- Enable private vulnerability reporting for the repository if it is not already
  enabled.
- Keep Dependabot security updates enabled. If GitHub routes security updates
  only to the default branch, either set the default branch to `dev` or configure
  repository rules so security PRs against the default branch are immediately
  synchronized back into `dev` before promotion.
- Review historical releases separately. The source now attests future release
  archives and checksums, but existing release artifacts are not retroactively
  signed or attested by this pass.

