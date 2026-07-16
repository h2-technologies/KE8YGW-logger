# Security Policy

Security reports are handled privately. Do not open a public issue, discussion,
or pull request that describes an unpatched vulnerability, exploit, credential
leak, private key, user data exposure, or bypass.

## Reporting A Vulnerability

Use GitHub private vulnerability reporting or a GitHub Security Advisory when
available for this repository. If private reporting is not available, contact a
repository maintainer through an existing private project channel and disclose
only that you need to report a security issue until a private channel is agreed.

Do not include secrets, real provider passwords, LoTW certificates, signing
keys, API tokens, or production QSO data in the first message.

## What To Include

Helpful reports include:

- affected version, commit, branch, or release artifact
- affected platform and deployment mode
- concise vulnerability summary
- reproduction steps using test data
- expected and actual security boundary
- affected accounts, logbooks, providers, or sync peers
- logs or diagnostics with secrets and personal data redacted
- whether the issue is already public or privately shared elsewhere

## Supported Release Lines

The project is pre-1.0. Security fixes are prioritized for the active `main`
branch, the current beta line, and any currently supported tagged release that
maintainers still distribute. Older experimental or internal builds may receive
guidance instead of patches.

## Coordinated Disclosure

Before remediation, do not publicly disclose:

- exploit steps
- authentication, authorization, or session bypass details
- provider credentials, certificate material, tokens, or signing keys
- user, station, logbook, QSO, backup, diagnostic, or sync data
- release signing or auto-update weaknesses

After maintainers have remediated or accepted the risk, public disclosure should
be coordinated through release notes, a security advisory, or a maintainer
approved issue.

## Security-Sensitive Areas

The following areas require extra care and maintainer review:

- Authentication and sessions.
- Authorization and account boundaries.
- Provider credentials and credential references.
- LoTW certificates and signing keys.
- Backups, diagnostic bundles, report upload, and redaction.
- Sync authorization, pairing, device identity, and account scoping.
- Desktop credential stores, including Windows Credential Manager, macOS
  Keychain, Linux Secret Service, and the explicit insecure development fallback.
- Release signing, installer packaging, and release artifact provenance.
- Auto-update metadata and update channels.
- User, station, logbook, QSO, provider, and support-data exposure.

Security work should avoid broad refactors unless required for the fix.
