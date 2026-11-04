# Windows support

Tezbar uses the same React interface and Rust core on macOS and Windows. Platform-specific behavior lives behind native adapters. JavaScript is confined to the on-demand extension compatibility worker; see [Rust core architecture](RUST_CORE.md). The native core has been exercised on macOS and a real Windows x64 host. Installer and desktop UI smoke validation remain separate release checks.

## Launcher and Launchpad

The backtick application grid is Tezbar's cross-platform Launchpad. On macOS it reads application bundles; on Windows it merges recursive Start Menu shortcuts with `Get-StartApps`, which also includes Microsoft Store applications. Selecting an item opens its shortcut or `shell:AppsFolder` identity. The ordinary launcher uses the same catalog.

Folder search uses Finder on macOS and Explorer on Windows. "Open with" resolves a Windows Start Menu shortcut and starts it with the selected path instead of invoking the macOS `open` command.

## Supported on both platforms

| Area                           | macOS implementation                    | Windows implementation                       |
| ------------------------------ | --------------------------------------- | -------------------------------------------- |
| Global launcher                | Tauri global shortcut                   | Tauri global shortcut                        |
| Application grid               | `.app` bundles and Spotlight            | Start Menu shortcuts and `Get-StartApps`     |
| Terminal                       | login shell through a PTY               | PowerShell through ConPTY                    |
| Mouse and keyboard automation  | CoreGraphics                            | Win32 `SendInput`                            |
| Screenshots                    | CoreGraphics display capture            | Win32 GDI virtual-desktop capture            |
| App/file opening               | `open` and Finder                       | Explorer, shell app IDs, and `Start-Process` |
| File search fallback           | Spotlight                               | bounded PowerShell search of user folders    |
| Knowledge folder picker        | AppKit dialog                           | Windows Forms folder dialog                  |
| Text-to-speech                 | `say`                                   | `System.Speech`                              |
| Confirmation dialogs           | AppKit/AppleScript dialog               | Windows Forms dialog                         |
| Clipboard text/images/files    | arboard and NSPasteboard                | arboard and Win32, including CF_HDROP        |
| Open ports                     | `lsof`                                  | `netstat.exe` and `tasklist.exe`             |
| System commands                | AppleScript and system tools            | PowerShell and Win32 tools                   |
| Extension app discovery        | Raycast/macOS application catalog       | Tezbar Windows application catalog           |
| Extension store                | Raycast community catalog               | Tezbar-loved GitHub repositories             |
| Extension runtime installation | existing Bun or on-demand Bun bootstrap | existing Bun or on-demand Bun bootstrap      |

Windows equivalents are implemented for appearance, audio, display sleep, computer sleep, keep-awake, lock, Wi-Fi, DNS flush, VPN settings, public/network information, Downloads, All Apps, AppData, Explorer path copying, Recycle Bin, OS/CPU/memory/disk/battery information, ports, and Git root copying.

## Intentionally platform-specific

Some capabilities cannot translate literally because they expose a macOS product rather than a user goal:

- Dock, menu bar, Finder restart, Homebrew, and AppleScript commands remain labeled macOS-only. A Windows feature should be added under its own accurate name rather than presenting taskbar, Explorer, or WinGet behavior as if it were the macOS feature.
- The Raycast extension catalog is not shown on Windows because many entries assume macOS APIs. Windows instead shows Tezbar-loved GitHub repositories, including direct repository installs for extensions that declare Windows support.
- Raycast extensions that directly execute arbitrary AppleScript remain macOS-only. Extensions using Tezbar's cross-platform APIs continue to work.

## Remaining native parity work

These areas still need Windows-native implementations before Windows can be called feature-complete:

1. Replace the Swift Vision ScreenOCR helper with Windows OCR.
2. Add a Windows UI Automation tree reader to match the macOS accessibility snapshot helper. Input injection and screenshot-based agent actions already work on Windows, but semantic control discovery does not.
3. Validate native clipboard text/image/file restoration, shortcuts, mixed-monitor capture and process cleanup on Windows. The CF_HDROP implementation is present; macOS fixture tests cannot establish Win32 behavior.
4. Extract icons for packaged Microsoft Store applications. Classic shortcut/executable icons are supported.
5. Add Windows-specific automatic installation guidance for local Whisper/Moonshine runtimes.
6. Produce and smoke-test signed x64 and ARM64 installers on actual Windows hosts using the project's Tailscale release workflow; do not use GitHub Actions. The Tauri config now targets MSI and per-user NSIS packages, but installer behavior must be validated on an actual Windows host.

## Build behavior

Run `pnpm build:windows` on Windows to produce both a per-user NSIS installer and an MSI package. The command checks for Rust and Cargo, uses the Windows-specific Tauri configuration, and writes installers under `src-tauri/target/release/bundle/`.

The generic `pnpm build` command is also platform-aware. The native helper step builds Swift helpers only on macOS and is a no-op on Windows. Tauri merges `tauri.macos.conf.json` or `tauri.windows.conf.json` and packages only resources valid for that operating system. If Bun is not already available, the Tauri host downloads the pinned runtime into Tezbar's app-data directory when an extension first requires the worker. Built-in features do not require this download. It does not modify the user's PATH, shell files, Homebrew installation, or system directories.

Cross-compilation from macOS can type-check Rust code, but a complete Windows Tauri build also requires a Windows resource compiler and cannot replace an installer test on Windows.

## Native AI branch verification

The native AI implementation uses PowerShell with an explicit UTF-8 preamble and
Windows Job Objects. Child processes start suspended, are attached to the job,
then resume; cancellation owns descendants rather than only the immediate shell.
Canonical path comparisons account for case and extended `\\?\`/UNC prefixes.
Regular-file writes and edits use atomic replacement; extensions use the same
owned process helper. Connection keys use Windows DPAPI; the encrypted secret
payload uses the existing AES-GCM/KeyStore abstraction with no plaintext fallback.

Verification runs on `DESKTOP-OB83S8J` through Tailscale, under Rust
`1.90.0-x86_64-pc-windows-msvc`. Source comes from a SHA-256-verified `git archive`
of the committed branch into a task-specific temporary staging directory. The
release checkout `C:\Users\User\Desktop\Tezbar` is read-only and stays clean.
No app is installed or launched, and no real profile or provider credential is
used. The backend resource build checks existing pins; Cargo uses a separate task
target directory. See [NATIVE_AI_VERIFICATION.md](NATIVE_AI_VERIFICATION.md) for
exact source, counts and test evidence.

The Windows tests exercise native provider HTTP/SSE/NDJSON dispatch, compiled
MCP stdio fixtures, modern/legacy HTTP/OAuth, file paths and atomic writes,
PowerShell Unicode, real process-tree termination, and isolated DPAPI key
round-trips. `cargo check --lib --locked` also compiles non-test platform code.
This does not establish installer behavior, Windows OCR, mixed-monitor capture,
accessibility integration or production Keychain/DPAPI migration behavior.
