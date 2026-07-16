# Support

This project uses GitHub issues for public bug reports and feature requests.
Security reports must follow [SECURITY.md](SECURITY.md) and should not be filed
publicly.

## Bug Reports

Use the bug report issue form. Include:

- version, commit, or branch
- platform and deployment mode
- reproduction steps
- expected and actual behavior
- relevant logs with secrets and personal data redacted
- offline or connectivity state
- provider involved, if applicable

## Feature Requests

Use the feature request issue form. Describe the problem, proposed behavior,
operator/user type, affected platforms, offline implications, security or
privacy implications, and alternatives considered.

## Usage Questions

Usage questions are welcome when they are specific and include what you tried.
For broad design questions, start with the README, developer guide, release
plans, and ADRs.

## Security Reports

Do not file public issues for vulnerabilities, leaked credentials, LoTW
certificate exposure, signing key exposure, authorization bypasses, sync
authorization failures, release signing issues, or private user/QSO data leaks.
Use the private process in [SECURITY.md](SECURITY.md).

## Provider-Specific Failures

Provider integrations can fail because of credentials, provider availability,
network conditions, rate limits, terms of service, certificate requirements, or
unsupported API behavior. Include the provider name, operation, fake/live mode,
sanitized error code, and whether the same action succeeds outside the app.

Do not include provider passwords, API keys, session tokens, LoTW certificate
material, or raw provider responses that contain private data.

## Self-Hosting Support

Self-hosting issues should include sanitized deployment mode, command, platform,
environment variable names used, logs, and whether the problem reproduces with a
fresh local test profile.

Do not include real secrets, database files, production backups, personal QSO
exports, sync tokens, pairing codes, or private URLs.

## Unsupported Deployment Modifications

Maintainers may decline support for modified deployments that change security
boundaries, remove authentication, bypass authorization, alter sync validation,
replace credential storage, alter release/update metadata, or patch generated
artifacts without source changes.

## Safe Diagnostics

Safe diagnostic information usually includes:

- app version or commit
- operating system and CPU architecture
- command run
- sanitized error message or error code
- provider name without credentials
- deployment mode
- whether the system was offline, online, or behind a proxy

Redact:

- tokens, passwords, API keys, cookies, authorization headers, and pairing codes
- LoTW certificates and signing keys
- real account IDs, private URLs, and production hostnames when sensitive
- personal QSO records, station profile details, backups, and diagnostic bundles
- local filesystem paths that reveal private usernames when not needed
