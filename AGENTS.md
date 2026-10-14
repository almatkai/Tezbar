# AGENTS.md — Tezbar Development & Release Guidelines

This document outlines mandatory guidelines for AI agents working on the **Tezbar** codebase.

---

## 1. Dual-Platform Release Rule (Windows & macOS)

Whenever a build or release is prepared for release channels (`alpha`, `beta`, or `stable`), **both supported platforms (Windows and macOS Apple Silicon) must be updated together in a single joint release**.

### Critical Principle:
- **Never publish a channel feed (`latest.json`) with only one platform present** unless the user explicitly requested a single-platform release.
- If `latest.json` in `channel-beta`, `channel-alpha`, or `channel-stable` is published with only Windows artifacts, macOS installations will inspect the feed, find no `darwin-aarch64` platform entry, and falsely report *"Tezbar is up to date"* (failing to receive updates).
- Both `windows-x86_64` (and `windows-x86_64-nsis`) and `darwin-aarch64` entries must be present and verified with valid cryptographic signatures.

---

## 2. Cross-Platform Build Workflow via Tailscale

When running on the **Windows host**:

1. **Locate the MacBook via Tailscale**:
   - Check Tailscale status:
     ```powershell
     tailscale status
     ```
   - Identify the MacBook (default host: `macbook` or `100.67.107.38`, hostname `almats-macbook-pro-1`, user `almat`).
   - Verify SSH connectivity:
     ```powershell
     ssh -o ConnectTimeout=5 macbook "echo OK"
     ```
   - If the MacBook is unreachable (sleep, lid closed, or Tailscale off), **stop and alert the user** to wake or reconnect the MacBook before attempting to publish the release feed.

2. **Locate and Sync the Repository on macOS**:
   - SSH to the MacBook and find the Tezbar working directory (e.g., `~/Downloads/Tezbar-channel-build-20261013` or active checkout):
     ```bash
     ssh macbook "cd ~/Downloads/Tezbar-channel-build-20261013 && git fetch origin && git checkout <branch> && git pull origin <branch>"
     ```

3. **Build and Sign the macOS Release Artifacts**:
   - Run the release build script on the MacBook:
     ```bash
     ssh macbook "cd ~/Downloads/Tezbar-channel-build-20261013 && node scripts/build-release.mjs --channel <channel> --version <version>"
     ```
   - Ensure the tarball has clean attributes (no `._*` AppleDouble metadata files inside `Tezbar_<version>_aarch64.app.tar.gz`).
   - Verify updater signature with minisign (`~/.tauri/tezbar.key`).

4. **Transfer Artifacts to Windows or Upload Directly**:
   - Transfer macOS artifacts via `scp` over Tailscale:
     ```powershell
     mkdir -Force build/release-<channel>-<version>-darwin-arm64
     scp -r macbook:~/Downloads/Tezbar-channel-build-20261013/build/release-<channel>-<version>-darwin-arm64/* build/release-<channel>-<version>-darwin-arm64/
     ```
   - *Alternative*: If GitHub CLI (`gh`) is authenticated on both machines, upload directly from the MacBook:
     ```bash
     ssh macbook "cd ~/Downloads/Tezbar-channel-build-20261013/build/release-<channel>-<version>-darwin-arm64 && gh release upload v<version> Tezbar_<version>_aarch64.app.tar.gz Tezbar_<version>_aarch64.app.tar.gz.sig Tezbar_<version>_aarch64.dmg --clobber"
     ```

5. **Build and Sign the Windows Release Artifacts**:
   - On the Windows host, execute:
     ```powershell
     node scripts/build-release.mjs --channel <channel> --version <version>
     ```
   - Produces signed NSIS installer: `Tezbar_<version>_x64-setup.exe` and `Tezbar_<version>_x64-setup.exe.sig`.

6. **Create Joint GitHub Release**:
   - Create or update tag `v<version>` with artifacts from **both** platforms:
     - Windows: `Tezbar_<version>_x64-setup.exe`, `Tezbar_<version>_x64-setup.exe.sig`
     - macOS: `Tezbar_<version>_aarch64.app.tar.gz`, `Tezbar_<version>_aarch64.app.tar.gz.sig`, `Tezbar_<version>_aarch64.dmg`

7. **Generate and Publish Universal Feed (`latest.json`)**:
   - Run the feed publisher script with all artifacts available:
     ```powershell
     node scripts/publish-feed.mjs --channel <channel> --version <version>
     ```
   - Verify that the resulting `latest.json` contains:
     - `platforms["windows-x86_64"]`
     - `platforms["windows-x86_64-nsis"]`
     - `platforms["darwin-aarch64"]`
   - Advance the mutable channel pointer releases (`channel-beta`, `channel-alpha`, `channel-stable`).

---

## 3. Signing Keys & Configuration

- **Tauri Minisign Secret Key**:
  - Windows: `C:\Users\User\.tauri\tezbar.key`
  - macOS: `~/.tauri/tezbar.key`
- **Public Key**:
  - Embedded in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.
  - Must remain matching across both platforms.

---

## 4. Code Quality & Pre-commit Verification

Before pushing code or cutting releases:
1. `pnpm typecheck` must pass without TypeScript errors.
2. `pnpm test` must pass (all unit/integration tests).
3. Ensure Git working tree is clean and synced to origin branches (`alpha`, `beta`, or `main`).
