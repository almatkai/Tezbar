# macOS signing continuity

Read this before building, signing, re-signing, or publishing any macOS Tezbar app.

macOS tracks privacy permissions and Keychain access through the app's code-signing requirement. Ad-hoc signing (`codesign --sign -`) defaults to a requirement bound to one binary hash. Every rebuild changes that identity and can repeat Documents, Accessibility, and Vault Keychain prompts. Keeping only the bundle ID or checking the Tauri updater signature does not preserve the macOS identity.

## Required release behavior

- Alpha, Beta, and Stable apps use the certificate identity pinned in `src-tauri/macos-signing.json`. The app identifier remains `com.tezbar.app` and installation path remains `/Applications/Tezbar.app`.
- `scripts/build-release.mjs` preserves the configured certificate even with `--allow-unsigned`. That flag permits the documented prerelease native-distribution exception; it does not permit ad-hoc signing of the macOS app.
- Before writing a build receipt, the script checks the sealed app with `codesign --verify --deep --strict`, verifies the pinned requirement, and rejects missing/different Team IDs, different requirements, and hash-bound/ad-hoc identities. It records the observed identity in the receipt.
- The backend build signs staged native resources, including extensionless `vec0`, with the same certificate before Tauri seals the app. Packaging verifies every Mach-O resource has the app's Team ID. Keep macOS library validation enabled; mismatched/ad-hoc libraries must not be bundled into a certificate-signed app.
- Ad-hoc signing is reserved for explicit `--local-preview` builds. Their receipts are not publishable. Never re-sign a distributed app with `-` or replace its requirement with an identifier-only check.
- Every distributed updater artifact also needs its existing Tauri updater signature. This is a separate download-integrity check, not a replacement for certificate signing.
- Both macOS arm64 and Windows x64 must be built from the same exact commit, saved locally, checked, and included together in the release and channel feed.

## Current beta identity

The available beta certificate is Apple Development under Team ID `UBRTH86SZ3`. The committed requirement pins its Apple certificate chain and identity. Beta.4 introduces this identity; subsequent betas must satisfy the same requirement. The build records identity continuity separately from `platformVerified`, which remains false for a beta without Developer ID notarization. Do not claim this beta is notarized or bypass the protected signed-release publisher by changing receipt fields.

Normal public distribution uses Developer ID Application signing and notarization. Obtaining that certificate and migrating the pin is an explicit release task; never silently choose a different certificate, team, or ad-hoc fallback when signing credentials are unavailable.

## Verification and migration

Run `pnpm release:test`. On the signing Mac, also run the real two-build regression with `TEZBAR_SIGNING_TEST_IDENTITY` set to the existing certificate name or fingerprint. The test builds two different executables, checks both against the committed requirement, and verifies that ad-hoc signing is rejected. No Vault item or privacy database is accessed.

The transition from beta.3's hash-bound identity can require one final permission/Keychain approval. An updater cannot inherit an older binary's hash or silently approve macOS security prompts. Tell users about this transition; do not promise zero prompts on that first update. Once approved, subsequent releases using the pinned identity should retain those grants.

Preserve the user's existing Vault, Keychain items, profile paths, and privacy decisions. Do not reset TCC, delete/recreate Vault keys, grant all applications Keychain access, or disable macOS security checks to hide prompts.

References: [Apple on unstable ad-hoc identities and TCC](https://developer.apple.com/forums/thread/751658), [Apple on designated requirements and Keychain access](https://developer.apple.com/library/archive/technotes/tn2206/_index.html).
