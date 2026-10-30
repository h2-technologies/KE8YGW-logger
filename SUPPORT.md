# Support

This project uses GitHub issues for public bug reports and feature requests.
Security reports must follow [SECURITY.md](SECURITY.md) and should not be filed
publicly.

## Bug Reports

Use the bug report issue form. Include:

- version, commit, or branch
- platform and component (GUI, TUI, headless node, relay, connector)
- reproduction steps
- expected and actual behavior
- relevant logs with secrets and personal data redacted
- offline or connectivity state
- external program involved (N3FJP, FLRig), if applicable

## Feature Requests

Use the feature request issue form. Describe the problem, proposed behavior,
operator/user type, affected platforms, offline implications, security or
privacy implications, and alternatives considered.

## Usage Questions

Usage questions are welcome when they are specific and include what you tried.
For broad design questions, start with the README and the design spec in
`docs/V1_DESIGN_SPEC.md`.

## Security Reports

Do not file public issues for vulnerabilities, leaked station keys, PINs or
join tokens, signing key exposure, role or sync authorization bypasses, release
signing issues, or private operator/QSO data leaks.
Use the private process in [SECURITY.md](SECURITY.md).

## Connector Failures

N3FJP and FLRig connectors can fail because the other program is closed, its
API is disabled, a firewall blocks the port, or its version behaves
differently. Include the program and version, host and port (not public
addresses), what the station status panel shows, and whether the program
works on its own.

## Relay Support

Self-hosted relay issues should include the platform, command, sanitized logs,
and whether stations on the same LAN sync without the relay.

Do not include join tokens, station keys, PINs, or private URLs.

## Unsupported Deployment Modifications

Maintainers may decline support for modified builds that change security
boundaries, bypass role checks, alter sync or signature validation, or patch
release artifacts without source changes.

## Safe Diagnostics

Safe diagnostic information usually includes:

- app version or commit
- operating system and CPU architecture
- command run
- sanitized error message or error code
- the connector or component involved
- whether the station was offline, on a LAN, or online

Redact:

- station keys, PINs, join tokens, and signing keys
- real club logs and operator details beyond callsigns already public
- private URLs and hostnames when sensitive
- local filesystem paths that reveal private usernames when not needed
