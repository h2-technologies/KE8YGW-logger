# Security Policy

Security reports are handled privately. Do not open a public issue, discussion,
or pull request that describes an unpatched vulnerability, exploit, credential
leak, private key, user data exposure, or bypass.

## Reporting A Vulnerability

Use GitHub private vulnerability reporting for this repository:

https://github.com/h2-technologies/KE8YGW-logger/security/advisories/new

If private vulnerability reporting is not available, contact a repository
maintainer through an existing private project channel and disclose only that you
need to report a security issue until a private channel is agreed. Do not post
vulnerability details in public issues, discussions, pull requests, or commits.

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

Do not submit secrets or real user, station, QSO, provider, backup, diagnostic,
or sync data in plaintext. Use synthetic test data and redact logs before
sharing. If maintainers need sensitive artifacts for reproduction, they will
coordinate a private transfer path first.

## Supported Release Lines

The project is pre-1.0. Security fixes are prioritized for active branches and
release lines that maintainers still distribute.

| Version or branch | Security support |
| --- | --- |
| `dev` | Active internal integration branch for security fixes. |
| `main` | Active beta branch; receives fixes through promotion from `dev`. |
| Latest `v0.2.x` tag | Supported while maintainers still distribute the beta artifact. |
| Older experimental tags and internal builds | Guidance only unless maintainers explicitly announce support. |

## Response Timelines

Maintainers aim to acknowledge private reports within 5 business days. For
accepted reports, maintainers aim to provide a status update within 10 business
days and then at least every 30 days until the issue is remediated, publicly
disclosed, or explicitly accepted as residual risk.

Severe issues involving authentication bypass, credential exposure, release
integrity, or real user/QSO/provider data may be handled on an accelerated
timeline.

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
