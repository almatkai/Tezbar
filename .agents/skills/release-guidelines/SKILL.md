---
name: release-guidelines
description: Procedure for building, signing, and publishing dual-platform (Windows x64 and macOS Apple Silicon) releases for Tezbar via Tailscale, minisign signing, and GitHub release feed publication. Use whenever building, releasing, or updating Alpha, Beta, or Stable channels.
---

# Release Guidelines Skill — Tezbar Cross-Platform Releases

This skill provides step-by-step instructions for releasing Tezbar simultaneously across **Windows (x64)** and **macOS (Apple Silicon)** from either host machine.

---

## 1. Dual-Platform Release Rule (Zero-Tolerance for Single-Platform Feeds)

Whenever cutting a release on `alpha`, `beta`, or `stable`:
- **Never publish a channel feed (`latest.json`) with only one platform present.**
- If `latest.json` on `channel-beta`, `channel-alpha`, or `channel-stable` is published with only Windows artifacts, macOS installations check the feed, find no `darwin-aarch64` section, and falsely report *"Tezbar is up to date"*, permanently missing the update (and vice versa).
- Both `windows-x86_64` (and `windows-x86_64-nsis`) and `darwin-aarch64` must be signed, uploaded, and verified together.

---

## 2. Remote Build Workflow via Tailscale

Tailscale network topology:
- **Windows PC**: Hostname `desktop-ob83s8j` | Tailscale IP `100.77.254.64` | User `User` | Project path `C:\Users\User\Desktop\Tezbar`
- **macOS MacBook**: Hostname `almats-macbook-pro-1` / `macbook` | Tailscale IP `100.67.107.38` | User `almat` | Project path `~/Downloads/Tezbar-channel-build-20261013` (or active checkout)

Both machines run OpenSSH (`sshd`) with configured SSH keys.

---

### Scenario A: Working from the Windows Host

1. **Check MacBook connectivity**:
   ```powershell
   tailscale status
   ssh -o ConnectTimeout=5 macbook "echo OK"
   ```
   *If unreachable (sleep, lid closed, or Tailscale disconnected)*: **Halt release publication** and alert the user to wake the MacBook or reconnect Tailscale before proceeding. Do NOT publish a Windows-only feed.

2. **Sync repository on the MacBook**:
   ```bash
   ssh macbook "cd ~/Downloads/Tezbar-channel-build-20261013 && git fetch origin && git checkout <branch> && git pull origin <branch>"
   ```

3. **Build and sign macOS artifacts**:
   ```bash
   ssh macbook "cd ~/Downloads/Tezbar-channel-build-20261013 && node scripts/build-release.mjs --channel <channel> --version <version>"
   ```
   - Produces clean `Tezbar_<version>_aarch64.app.tar.gz` (no `._*` metadata files), `.sig`, `.dmg`.

4. **Transfer macOS artifacts to Windows or direct upload**:
   ```powershell
   mkdir -Force build/release-<channel>-<version>-darwin-arm64
   scp -r macbook:~/Downloads/Tezbar-channel-build-20261013/build/release-<channel>-<version>-darwin-arm64/* build/release-<channel>-<version>-darwin-arm64/
   ```
   *Or directly upload from Mac via `gh release upload`.*

5. **Build and sign Windows release locally**:
   ```powershell
   node scripts/build-release.mjs --channel <channel> --version <version>
   ```

6. **Create joint release & publish universal feed**:
   ```powershell
   node scripts/publish-feed.mjs --channel <channel> --version <version>
   ```

---

### Scenario B: Working from the macOS Host (MacBook)

1. **Check Windows PC connectivity**:
   ```bash
   tailscale status
   ssh -o ConnectTimeout=5 User@100.77.254.64 "powershell -NoProfile -Command 'Write-Output OK'"
   ```
   *If unreachable*: **Halt release publication** and alert the user to check the Windows PC.

2. **Sync repository on the Windows PC**:
   ```bash
   ssh User@100.77.254.64 "powershell -NoProfile -Command 'cd C:\Users\User\Desktop\Tezbar; git fetch origin; git checkout <branch>; git pull origin <branch>'"
   ```

3. **Build and sign Windows artifacts remotely on PC**:
   ```bash
   ssh User@100.77.254.64 "powershell -NoProfile -Command 'cd C:\Users\User\Desktop\Tezbar; node scripts/build-release.mjs --channel <channel> --version <version>'"
   ```
   - Produces `Tezbar_<version>_x64-setup.exe` and `Tezbar_<version>_x64-setup.exe.sig`.

4. **Transfer Windows artifacts to macOS or direct upload**:
   ```bash
   mkdir -p build/release-<channel>-<version>-win32-x64
   scp -r User@100.77.254.64:C:/Users/User/Desktop/Tezbar/build/release-<channel>-<version>-win32-x64/* build/release-<channel>-<version>-win32-x64/
   ```

5. **Build and sign macOS release locally**:
   ```bash
   node scripts/build-release.mjs --channel <channel> --version <version>
   ```

6. **Create joint release & publish universal feed**:
   ```bash
   node scripts/publish-feed.mjs --channel <channel> --version <version>
   ```

---

## 3. Minisign Keys Reference

- **Private Key**:
  - Windows: `C:\Users\User\.tauri\tezbar.key`
  - macOS: `~/.tauri/tezbar.key`
- **Public Key**: Configured in `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.
