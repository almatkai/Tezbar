# Tezbar release channels

| Channel          | Installation | Updates                                | Data             |
| ---------------- | ------------ | -------------------------------------- | ---------------- |
| Stable (default) | Tezbar       | Newer Stable versions                  | Existing profile |
| Beta             | Tezbar       | Newer Beta and Stable versions         | Same profile     |
| Alpha            | Tezbar       | Newer Alpha, Beta, and Stable versions | Same profile     |

Users can switch between all three channels in **Settings → App updates → Release channel**. Normal Tezbar starts on Stable, the recommended channel. An explicitly installed prerelease starts on its matching channel only when no preference already exists. Selection persists across upgrades and is independent of the currently installed version. Changing channels clears the old pending update and immediately checks the selected feed. Changes are disabled during an update check/download or while a downloaded update awaits restart.

Every release uses the same app identifier, executable, installer upgrade identity, shortcut, settings, webview storage, databases, terminal history, and Pi agent state. Channels do not create separate installations or copy/migrate profiles. All channel installers must use the shared `com.tezbar.app` / `Tezbar` identity so updates replace the current installation and retain its data.

Beta and Alpha are explicit opt-ins for early releases. Switching back to Stable never downgrades: a running prerelease waits for a newer eligible Stable release. For example, `0.3.0-alpha.1` on Stable waits for `0.3.0` or a newer Stable version; it does not install `0.2.7`. Switching to Beta from Alpha similarly waits for a newer Beta/Stable release. Before introducing an incompatible database migration in Alpha/Beta, implement and test a backup/migration recovery path; channel selection cannot reverse a schema migration. Users testing prereleases should back up important data.

These channels separate update cohorts, not access permissions. Public Alpha/Beta installers can be downloaded by anyone. If Alpha must be invite-only, use a private distribution host and authenticated feeds; a hidden setting or prerelease label does not enforce membership.

## Version and feed policy

Use `0.3.0-alpha.1`, `0.3.0-beta.1`, then `0.3.0`. Increase the positive numeric sequence for each prerelease. Build metadata and other prerelease labels are rejected. Every channel pointer advances strictly forward. For hotfixes use a new version; never replace a published installer or silently downgrade a user's data.

Installers and updater signatures live at immutable `v<version>` GitHub release URLs. Three mutable **prerelease** pointer releases (`channel-alpha`, `channel-beta`, `channel-stable`) each hold only `latest.json`, preventing pointer releases from becoming GitHub's latest stable release. A newer Stable release also advances Beta and Alpha; a newer Beta release also advances Alpha. Each pointer advances only if that version is newer than its current release. Alpha publication never changes Beta or Stable. Every real Stable release includes `latest.json` for older app versions that use GitHub's latest release endpoint.

The old standalone Windows test-signing workflow is retired. All publications now use verified platform receipts and the reviewed release pipeline; no helper overwrites signatures on an existing release.

`updates/*.json` are static bootstrap files, not live mirrors. All channels bootstrap the existing 0.2.7 Windows Stable release. The native updater tries its selected pointer first, then its bootstrap, and retains the legacy Stable endpoint. A missing platform produces no offered update. Bootstrap data can be stale during a feed outage; the next successful pointer check supplies the current release. Never put Alpha releases in Beta/Stable feeds or Beta releases in Stable.

The updater independently checks SemVer eligibility, app identity, an immutable repository download URL, and the configured updater signature before installation. Checks, channel changes, and downloads are serialized so a pending update cannot survive a channel switch. Windows installers disallow downgrade and retain the existing normal WiX upgrade identity.

## One-time repository setup

These are repository settings, not settings created by committing workflow YAML. Configure them before calling the pipeline production-ready:

1. Require the **Quality checks** jobs on PRs to `main`, prohibit direct writes, and merge only separately reviewed feature/fix PRs. Do not merge all of `dev` to release experiments.
2. Create GitHub environments `release-build`, `release-alpha`, `release-beta`, and `release-stable`. Add required reviewers, prevent self-review, and restrict deployment branches to approved workflow branches. A trusted reviewer other than the person triggering the run must approve it. Signing approval grants that reviewed source access to signing credentials; verify its full diff before approval. Publication approval is a separate final gate. The pipeline checks these rules through GitHub's API and fails closed if they are missing or weakened. GitHub's availability of reviewer protection depends on the repository/account plan.
3. Put signing credentials in `release-build` environment secrets. Do not put them in repository-wide secrets available to unrelated workflows:
   - `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`: the existing updater key matching `src-tauri/tauri.conf.json`. Retain its public key; changing it without a key migration breaks installed clients.
   - `WINDOWS_CERTIFICATE` (base64 PFX), `WINDOWS_CERTIFICATE_PASSWORD`, `WINDOWS_CERTIFICATE_THUMBPRINT`: a valid trusted code-signing certificate. Hardware-backed/cloud certificates require adapting the signing command to that provider rather than exporting the private key.
   - `APPLE_CERTIFICATE` (base64 Developer ID Application P12), `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_API_ISSUER`, `APPLE_API_KEY`, and `APPLE_API_PRIVATE_KEY` (the P8 contents). Tauri signs and notarizes; the build script verifies codesign, Gatekeeper assessment, and stapling.
4. Keep GitHub release immutability disabled for the three pointer releases; only their manifest assets change. If enabling repository-level immutable releases, move mutable pointers to a separate update host first.

See [GitHub environment protection](https://docs.github.com/en/actions/how-tos/deploy/configure-and-manage-deployments/manage-environments), [Tauri updater signing](https://v2.tauri.app/plugin/updater/), and [macOS signing/notarization](https://v2.tauri.app/distribute/sign/macos/).

## Release procedure

1. Add reviewed notes at `docs/releases/<version>.md` in the selected source branch. Review the complete diff and commit list relative to `origin/main`. Keep release-channel changes independent from optional embedding-model changes.
2. Merge the exact approved changes into `main` before a Stable release. Alpha/Beta may use a separately reviewed feature/release ref. The pipeline rejects `dev` as a named source and rejects Stable commits outside `origin/main` history.
3. Manually run **Reviewed channel release** with the channel, exact version, and reviewed source ref/commit. Refs are resolved once to an exact commit; every build and test checks out that commit. Nothing publishes on push, tag, or PR merge.
4. Approve signing in `release-build`. CI tests Windows x64 and both macOS architectures, builds signed installers, verifies updater signatures and native signing/notarization, and records source commit and SHA-256 receipts. All three platforms are required; incomplete or mixed builds cannot publish.
5. Review the uploaded `release-*` build artifacts and notes, then approve the matching publication environment. The workflow validates every receipt again, creates a draft versioned release, publishes it, and advances only the authorized feed(s). It does not commit generated manifests to `main`.

Example configuration preview (no signing or publication):

```sh
pnpm release:build --channel alpha --version 0.3.0-alpha.1 --notes docs/releases/0.2.7.md --dry-run
pnpm release:test
```

For local Alpha/Beta development, `--allow-unsigned` builds have `platformVerified: false` and cannot enter the protected updater release pipeline. Stable requires verified signing for public updater releases. To privately test a normal Stable installation with its default channel and the complete channel selector, use `--local-preview`, for example `pnpm release:build --channel stable --version 0.2.7 --notes docs/releases/0.2.7.md --local-preview`. Local previews disable updater artifact signing, retain `platformVerified: false`, and explicitly record `distribution: local-preview`; the publisher rejects their receipts. They do not change public releases or updater feeds. Use the wrapper instead of manually changing the main app's identity/version. Generated configuration and artifacts stay in `build/release-*`; existing unrelated build folders are untouched. Choose a fresh bundle output when stale artifacts cause validation to fail.

## Interrupted publication and recovery

Global release-workflow concurrency serializes feed promotion. A failure never justifies overwriting versioned assets.

For an interruption after the complete draft/versioned release exists, rerun the workflow with the **same channel, version, exact source commit**, and set `artifact_run_id` to the original workflow run. It downloads the original artifacts rather than rebuilding (signing timestamps can change bytes), reruns quality checks, and requires publication approval again. Recovery verifies remote artifact hashes, signatures, metadata, and tag commit before finishing publication/pointer updates. A pointer already on that same verified release is accepted; a newer pointer is never rolled back.

If upload failed before the draft had every required asset/manifest, recovery fails closed. Delete only the unpublished incomplete draft through an explicit reviewed maintenance action, then run a fresh build. Never delete or modify an already published release to repair it; publish a new version. Build artifacts are retained for 30 days; preserve the original verified receipts/artifacts securely if longer recovery retention is needed.

## Required smoke test before the first live release

Local Windows tests do not prove signed installer behavior or macOS notarization. Verify clean install defaults to Stable, opt-in to Beta/Alpha, signed Stable-to-Beta-to-Alpha updates, Alpha-to-newer-Beta/Stable promotion, subscription persistence across builds, preservation of settings/history/indexes, returning to Stable without downgrade, offline checking, and interrupted downloads on real test installations. Establish crash/error monitoring with explicit user consent and use the observed Alpha/Beta results to decide when a release qualifies for Stable.
