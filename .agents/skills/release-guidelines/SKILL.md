---
name: release-guidelines
description: Assemble, build, sign, publish, or repair Tezbar Alpha, Beta, and Stable releases. Reconcile channel branches and version history before choosing release contents or a version.
---

# Tezbar releases

## 1. Establish the release contents and version

Before editing versions, building, tagging, or publishing:

1. Select the checkout under the user's worktree rules. Inspect its branch/status, fetch origin, and read `AGENTS.md` and `docs/MACOS_SIGNING.md` there. Preserve unrelated work and use only this selected local tree.
2. Inspect published GitHub releases, the live channel manifest, local/remote release branches, and existing tags. Distinguish **published version**, **prepared release branch**, and **installed app version** in the result.
3. For a cumulative Beta/Alpha release, audit every integration/release branch in that channel, including local commits not pushed yet. Compare ancestry and effective patches against the intended target (`git merge-base --is-ancestor`, `git log`, `git cherry`, and current source diffs). Account for every functional change already merged into those channel branches. Classify it as included, patch-equivalent, superseded with evidence, or deliberately excluded by the user. An unrelated unmerged feature/prototype branch is not part of this inventory.
4. Continue the existing unpublished target branch and consolidate the authorized accumulated changes into it. Preserve merged fixes, dependencies, and release notes. A fresh branch from the last published tag is appropriate only when the user requested that restricted scope. Development branches on `dev` do not authorize a whole-branch production merge; Stable follows the user's selected feature/fix PRs.
5. Choose the next sequential version from the published channel history or the version the user explicitly named. **A prepared branch does not reserve a version or justify skipping it.** If beta.7 is published and beta.8 is prepared, finish beta.8; the existence of that branch is not a reason to create beta.9. Reconcile competing prepared branches under the intended version. If requirements conflict, finish the inventory and present the concrete conflict before asking the user.
6. Review the full diff and commit list from the published baseline. Show a concise inventory of included work and the exact target version before starting builds. Keep package, Cargo, Tauri versions and notes consistent; commit the source and record one exact build SHA.

Completion: every relevant channel branch is accounted for, the target includes the authorized accumulated work, and the chosen version follows the intended sequence. Do not start builds before this is true.

## 2. Build both platforms from that commit

- Build locally on the Mac and a Windows host discovered with `tailscale status`; never run application builds on GitHub Actions/CI.
- Read `docs/MACOS_SIGNING.md` before every macOS build/sign/publication. Preserve its pinned certificate, Team ID, designated requirement, native-helper inventory, identifier, and resource signatures. Verify real signing continuity tests with the configured identity. Updater signing is a separate integrity check.
- Use the repository's current build script and policy. `--local-preview` is private testing only: its ad-hoc app/receipts are not publication artifacts. For the established Alpha/Beta native-distribution exception, `--allow-unsigned` still requires the pinned macOS certificate and valid updater signatures; preserve the truthful `testing-preview`/`platformVerified` receipt values. Stable requires the verified signed-release path. Never relabel receipts or bypass signing checks.
- Run required typecheck/tests and release/signing checks. Save logs without exposing credentials. Existing updater keys are normally at `~/.tauri/tezbar.key` (Mac) and `%USERPROFILE%\.tauri\tezbar.key` (Windows); let the build script load them. Request missing secrets through the private secret tool.
- Before syncing Windows, inspect its status and exact checkout. Switch only when safe, fetch the authorized source branch and verify its exact SHA. Preserve unrelated changes. Restore only identified build-generated changes from this task; never use a blanket checkout/reset/clean.

Typical established Beta build, executed separately on each platform from the same clean source SHA:

```sh
node scripts/build-release.mjs --channel beta --version <version> --allow-unsigned
```

Use `tailscale status` to discover current addresses. Known Windows checkout: `User@desktop-ob83s8j`, `C:\Users\User\Desktop\Tezbar`; Mac checkout: `/Users/almat/Desktop/code/Tezbar APP/Tezbar`. Inspect connectivity and paths rather than assuming a machine is available.

Transfer the Windows output directory into the selected Mac checkout's `build/` directory. Create a fresh `build/release-staging-v<version>/` containing:

- macOS arm64 DMG, updater `.app.tar.gz`, and its `.sig`;
- Windows x64 installer `.exe` and its `.sig`;
- both original platform receipts and `SHA256SUMS`.

Verify receipt versions, channel, identifier, **matching exact source SHA**, artifact hashes, updater signatures against the committed public key, and macOS signing continuity. Keep original receipts intact. Both platforms must pass before publication.

## 3. Publish and verify the complete release

Release authorization covers the requested release; do not ask again for already authorized push/tag/publication. A request only to prepare or merge does not authorize publication.

1. Check again that the intended version is unpublished and the channel has not advanced during the builds. Review the final source diff/commits and staged inventory.
2. Push the source branch and tag at the exact build SHA. Publish both platforms together. `gh release create` and `node scripts/publish-feed.mjs --channel <channel> --version <version> --source-commit <SHA> --artifacts <staging-directory>` must run continuously in one command chain: a visible release without its new feed triggers manual/browser installation fallback.
3. Preserve all required feed entries: `darwin-aarch64`, `windows-x86_64`, and `windows-x86_64-nsis`. Follow the repository's channel-promotion policy. Copy the generated manifest to `updates/<channel>.json`, commit and push it in the selected release branch.
4. Download the published version/channel manifests and verify version, complete platform entries, URLs, and signatures. Compare GitHub artifact SHA256 digests with the staged checksums, verify tag/build SHA, and check download availability. Reconcile any failed publication step before reporting completion.
5. Report the actual published version and link, included changes, supported platforms, and updater state. A local merge, prepared branch, build, and published release are distinct outcomes.

## 4. Repair an erroneous release when explicitly authorized

Normal releases remain immutable; fix them with a new version. Deleting/withdrawing a published version requires explicit user authorization. When that authorization is present, carry it out without asking again:

1. Identify every channel pointer and fallback manifest referencing the erroneous version. Restore complete, verified previous manifests first so users are not directed to missing assets.
2. Delete only the named erroneous GitHub release, its tag, and authorized release branches. Preserve its functional changes in the correct target branch; omit its wrong version/feed metadata. Avoid resurrecting the removed version in subsequent merges or notes.
3. Reconcile and build the intended version using sections 1–3. Verify the removed release/tag/branches are absent and all channel pointers lead to an existing complete release.
4. Check the installed app version. Semver auto-update will not downgrade an already installed higher version; provide the intended signed installer and state that a manual reinstall is needed. Do not claim the lower version will arrive automatically on that installation.
