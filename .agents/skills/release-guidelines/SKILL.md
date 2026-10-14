---
name: release-guidelines
description: Procedure for building, signing, and publishing dual-platform (Windows x64 and macOS Apple Silicon) releases for Tezbar via Tailscale, minisign signing, and GitHub release feed publication. Use whenever building, releasing, or updating Alpha, Beta, or Stable channels.
---

# Release Guidelines Skill — Tezbar Cross-Platform Releases

This skill provides step-by-step instructions for releasing Tezbar simultaneously across **Windows (x64)** and **macOS (Apple Silicon)**.

---

## 1. Dual-Platform Release Rule (Zero-Tolerance for Single-Platform Feeds)

Whenever cutting a release on `alpha`, `beta`, or `stable`:
- **Never publish a channel feed (`latest.json`) with only one platform present.**
- If `latest.json` on `channel-beta`, `channel-alpha`, or `channel-stable` is published with only Windows artifacts, macOS installations check the feed, find no `darwin-aarch64` section, and falsely report *"Tezbar is up to date"*, permanently missing the update.
- Both `windows-x86_64` (and `windows-x86_64-nsis`) and `darwin-aarch64` must be signed, uploaded, and verified together.

---

## 2. Remote Build Workflow via Tailscale (from Windows Host)

When operating from the **Windows development machine**:

### Step 1: Detect and ping the MacBook on Tailscale
1. Check device status:
   ```powershell
   tailscale status
   ```
2. Look for the macOS device:
   - Hostname: `almats-macbook-pro-1` / `macbook`
   - Tailscale IP: `100.67.107.38`
   - User: `almat`
3. Test SSH reachability:
   ```powershell
   ssh -o ConnectTimeout=5 macbook "echo OK"
   ```
4. **If unreachable**: The MacBook is asleep, lid-closed, or Tailscale is disconnected. **Halt the release publication** and alert the user to wake the MacBook or reconnect Tailscale before proceeding. Do NOT publish a Windows-only feed.

### Step 2: Sync repository on the MacBook
SSH into the MacBook and pull the target branch/commits:
```bash
ssh macbook "cd ~/Downloads/Tezbar-channel-build-20261013 && git fetch origin && git checkout <branch> && git pull origin <branch>"
```

### Step 3: Build and sign macOS artifacts
Run the release build script on the MacBook:
```bash
ssh macbook "cd ~/Downloads/Tezbar-channel-build-20261013 && node scripts/build-release.mjs --channel <channel> --version <version>"
```
- Produces:
  - `Tezbar_<version>_aarch64.app.tar.gz` (clean tarball without `._*` metadata files)
  - `Tezbar_<version>_aarch64.app.tar.gz.sig` (minisign signature)
  - `Tezbar_<version>_aarch64.dmg`
  - `receipt-darwin-aarch64.json`

### Step 4: Transfer macOS artifacts to Windows (or upload directly via GH CLI)
- **Transfer to Windows**:
  ```powershell
  mkdir -Force build/release-<channel>-<version>-darwin-arm64
  scp -r macbook:~/Downloads/Tezbar-channel-build-20261013/build/release-<channel>-<version>-darwin-arm64/* build/release-<channel>-<version>-darwin-arm64/
  ```
- **Or direct upload from Mac**:
  ```bash
  ssh macbook "cd ~/Downloads/Tezbar-channel-build-20261013/build/release-<channel>-<version>-darwin-arm64 && gh release upload v<version> Tezbar_<version>_aarch64.app.tar.gz Tezbar_<version>_aarch64.app.tar.gz.sig Tezbar_<version>_aarch64.dmg --clobber"
  ```

### Step 5: Build and sign Windows release
On the Windows host:
```powershell
node scripts/build-release.mjs --channel <channel> --version <version>
```
- Produces:
  - `Tezbar_<version>_x64-setup.exe`
  - `Tezbar_<version>_x64-setup.exe.sig`

### Step 6: Create or update joint GitHub Release
Ensure GitHub release `v<version>` contains:
- `Tezbar_<version>_x64-setup.exe`
- `Tezbar_<version>_x64-setup.exe.sig`
- `Tezbar_<version>_aarch64.app.tar.gz`
- `Tezbar_<version>_aarch64.app.tar.gz.sig`
- `Tezbar_<version>_aarch64.dmg`

### Step 7: Generate and publish universal feed (`latest.json`)
Run:
```powershell
node scripts/publish-feed.mjs --channel <channel> --version <version>
```
Verify that the output manifest contains:
- `platforms["windows-x86_64"]`
- `platforms["windows-x86_64-nsis"]`
- `platforms["darwin-aarch64"]`

Then advance channel pointers (`channel-beta`, `channel-alpha`, `channel-stable`).

---

## 3. Minisign Keys Reference

- **Private Key**:
  - Windows: `C:\Users\User\.tauri\tezbar.key`
  - macOS: `~/.tauri/tezbar.key`
- **Public Key**: Configured in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.
