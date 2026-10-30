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

Do not include station keys, PINs, join tokens, signing keys, or real club
logs in the first message.

## What To Include

Helpful reports include:

- affected version, commit, branch, or release artifact
- affected platform and deployment mode
- concise vulnerability summary
- reproduction steps using test data
- expected and actual security boundary
- affected stations, roles, or peers
- logs or diagnostics with secrets and personal data redacted
- whether the issue is already public or privately shared elsewhere

Do not submit secrets or real station, operator, or QSO data in plaintext. Use synthetic test data and redact logs before
sharing. If maintainers need sensitive artifacts for reproduction, they will
coordinate a private transfer path first.

## Supported Release Lines

The project is pre-1.0. Security fixes are prioritized for active branches and
release lines that maintainers still distribute.

| Version or branch | Security support |
| --- | --- |
| `dev` | Active internal integration branch for security fixes. |
| `main` | Active beta branch; receives fixes through promotion from `dev`. |
| v1 release tags | Supported once published (none yet). |
| `archive/v0` and v0.x tags | Retired Oct 30, 2026. No security fixes. |

## Response Timelines

Maintainers aim to acknowledge private reports within 5 business days. For
accepted reports, maintainers aim to provide a status update within 10 business
days and then at least every 30 days until the issue is remediated, publicly
disclosed, or explicitly accepted as residual risk.

Severe issues involving authentication bypass, credential exposure, release
integrity, or real station, operator, or QSO data may be handled on an accelerated
timeline.

## Coordinated Disclosure

Before remediation, do not publicly disclose:

- exploit steps
- authentication, authorization, or session bypass details
- station keys, PINs, join tokens, or signing keys
- station, operator, or QSO data
- release signing weaknesses

After maintainers have remediated or accepted the risk, public disclosure should
be coordinated through release notes, a security advisory, or a maintainer
approved issue.

## Security-Sensitive Areas

The following areas require extra care and maintainer review:

- Station keys, event signing, and hash-chain verification.
- Operator accounts, admin/checker PINs, and the role matrix enforced at merge.
- Join tokens, join approval, and station revocation.
- Sync transport: TLS, certificate pinning to station keys, and handling of
  malformed or hostile network input.
- The relay, which must store and forward signed events without being able to
  forge them.
- Network setup helpers that change operating-system settings.
- Release signing, installer packaging, and release artifact provenance.
- Station, operator, and QSO data exposure.

Security work should avoid broad refactors unless required for the fix.
