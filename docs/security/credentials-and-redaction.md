# Credentials and Redaction

Provider integrations must assume credentials are sensitive and runtime diagnostics may be exported in reports.

## Credential Rules

- Do not store passwords, API keys, sync tokens, session tokens, or certificates in provider metadata.
- Hosted session, refresh, invitation, email-verification, recovery, and API
  tokens must be persisted only as hashes. Raw token values may appear only in
  the immediate creation/consumption response or the test email outbox used by
  deterministic unit tests.
- Do not log credential values.
- Do not include credentials in runtime events.
- Do not include credentials in diagnostic bundles.
- Do not store credentials in the official log.
- Do not sync credentials.

The MVP exposes configuration schemas and placeholders only. Secure storage is a TODO and should use OS keychain or a comparable secret store.

## Config Keys

Providers may declare required and optional config keys such as `qrz.username`, `qrz.token`, `hamqth.username`, `lotw.certificate_path`, `clublog.email`, `clublog.token`, `dxcluster.host`, and `dxcluster.port`.

Declaring a key does not mean storing the secret in plain text. It means the UI can explain what is missing.

## Redaction

Runtime diagnostics and report bundles must redact fields whose names suggest passwords, tokens, secrets, API keys, credentials, authorization, sessions, or sync tokens.

Redaction reports should describe categories removed, not expose removed values.

## Service Cache

Service cache is not official log data. Cache entries may contain safe provider results and safe metadata, but must not include raw secret-bearing provider responses.
