# Secure Credential Storage

The platform treats provider credentials as security/support state, not official
logbook data. Credentials are never written to official events, synced as log
history, or included in runtime diagnostic payloads.

## Backends

`ham-core::CredentialStore` defines the storage boundary:

- store credential
- retrieve secret by credential ID
- update credential
- revoke credential
- test availability
- list safe metadata without exposing secrets
- rotate credential placeholder

The MVP includes:

- `OsCredentialStore`: stores safe credential metadata in local support storage
  and stores secret values in the platform credential backend.
- `UnsupportedOsCredentialStore`: reports a clear unavailable/unsupported
  backend when the platform backend or required helper tooling is unavailable.
- `InsecureDevCredentialStore`: an explicit opt-in development fallback that
  writes a local JSON file. It is marked insecure and must not be silently
  enabled for production use.

The current OS backend wiring is:

- Windows Credential Manager through the Windows credential APIs.
- macOS Keychain through the `security` command-line interface.
- Linux Secret Service/libsecret through `secret-tool`.

## Fallback Behavior

The GUI/local desktop path selects `OsCredentialStore` when the platform backend
is available. It only enables the development fallback when
`HAM_PLATFORM_ALLOW_INSECURE_DEV_CREDENTIALS=1` is set. Otherwise the credential
screen shows a secure backend unavailable/configuration error rather than
silently writing plaintext credentials.

## Provider Usage

Service providers declare `required_credentials`, and provider configuration
stores credential references such as:

```json
{
  "qrz.lookup.credential_id": "..."
}
```

Raw tokens, passwords, API keys, and certificates should not be stored in
provider config. Providers retrieve the secret through `CredentialStore` only
after plugin permission and operator role checks pass.

Online Services providers use this pattern for LoTW certificates, eQSL
passwords, Club Log credentials, QRZ API keys, HamQTH credentials, HRDLog upload
codes, and future weather/map tokens. Runtime events and diagnostics should
only mention provider IDs and credential IDs; they must never include the secret
material itself.

## Redaction

Runtime events, diagnostic bundles, and safe metadata pass through the existing
redaction helpers. Secret-like fields such as `password`, `token`, `secret`,
`api_key`, `credential`, and `sync_token` are masked.

## Current Limitations

- OS credential access is implemented but still needs validation on clean
  Windows/macOS/Linux release runners and packaged desktop builds.
- The development fallback is plaintext and exists only to test UI and provider
  integration behavior.
- Credential rotation is a placeholder.
