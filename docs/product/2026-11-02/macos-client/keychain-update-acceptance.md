# Persistent Mac identity authorization — 2026-11-02

Local ad hoc builds used a changing cdhash as their designated requirement. Pinning a local certificate stabilized the app identity, but the legacy login Keychain also retained a per-build partition restriction for non-Apple development signatures. This behavior is visible in Apple's [securityd client classification](https://github.com/apple-oss-distributions/Security/blob/main/securityd/src/clientid.cpp). The initial certificate-only approach therefore did not remove every authorization request.

The final implementation keeps a certificate-signed Keychain component at a fixed private Application Support path. It runs per request, authenticates Kite's parent process by bundle ID and pinned leaf certificate, and handles only the existing SSH and Sunshine identity entries. App builds do not replace it. Secrets use private pipes and remain in Keychain. Normal reads/writes disable authorization UI and return failures directly; only explicit one-time setup can display a system prompt.

The user completed the remaining legacy desktop-pairing authorization. Validation then confirmed:

- Two app builds with different resource/code hashes share the same persistent certificate identity.
- Both builds read the original SSH identity and existing Sunshine identity while the component's authorization UI is disabled.
- The SSH fingerprint matches the original device; the saved pairing record remains present.
- Re-running explicit setup in a new process reports **updated 0 items** and completes without entering an interactive path.
- An unrelated caller is rejected with exit status 77 and receives zero secret bytes.
- The installed component remains unchanged across app rebuilds; no authorization helper or SecurityAgent request remains running after validation.

Four offline Mac regressions still pass after the adapter change. The five-test native media/SwiftUI run and 134 shared-client Rust checks are recorded in [client acceptance](acceptance.md). Numeric authorization results are in [keychain-validation.json](keychain-validation.json). The component must be upgraded explicitly when its code or signing certificate changes; ordinary app updates retain the existing installation and authorization.
