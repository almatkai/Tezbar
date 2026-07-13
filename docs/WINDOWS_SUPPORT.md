# Windows support

Tezbar uses the same React interface and TypeScript backend on macOS and Windows. Platform-specific behavior lives behind small adapters in the Tauri host and backend so features keep the same user-facing meaning even when the operating systems expose different APIs.

## Launcher and Launchpad

The backtick application grid is Tezbar's cross-platform Launchpad. On macOS it reads application bundles; on Windows it merges recursive Start Menu shortcuts with `Get-StartApps`, which also includes Microsoft Store applications. Selecting an item opens its shortcut or `shell:AppsFolder` identity. The ordinary launcher uses the same catalog.

Folder search uses Finder on macOS and Explorer on Windows. "Open with" resolves a Windows Start Menu shortcut and starts it with the selected path instead of invoking the macOS `open` command.

## Supported on both platforms

| Area                          | macOS implementation              | Windows implementation                       |
| ----------------------------- | --------------------------------- | -------------------------------------------- |
| Global launcher               | Tauri global shortcut             | Tauri global shortcut                        |
| Application grid              | `.app` bundles and Spotlight      | Start Menu shortcuts and `Get-StartApps`     |
| Terminal                      | login shell through a PTY         | PowerShell through ConPTY                    |
| Mouse and keyboard automation | CoreGraphics                      | Win32 `SendInput`                            |
| Screenshots                   | CoreGraphics display capture      | Win32 GDI virtual-desktop capture            |
| App/file opening              | `open` and Finder                 | Explorer, shell app IDs, and `Start-Process` |
| File search fallback          | Spotlight                         | bounded PowerShell search of user folders    |
| Knowledge folder picker       | AppKit dialog                     | Windows Forms folder dialog                  |
| Text-to-speech                | `say`                             | `System.Speech`                              |
| Confirmation dialogs          | AppKit/AppleScript dialog         | Windows Forms dialog                         |
| Clipboard text/images         | native desktop adapter            | PowerShell/native desktop adapter            |
| Open ports                    | `lsof`                            | `netstat.exe` and `tasklist.exe`             |
| System commands               | AppleScript and system tools      | PowerShell and Win32 tools                   |
| Extension app discovery       | Raycast/macOS application catalog | Tezbar Windows application catalog           |
| Runtime installation          | existing Bun installation         | existing Bun or first-launch Bun bootstrap   |

Windows equivalents are implemented for appearance, audio, display sleep, computer sleep, keep-awake, lock, Wi-Fi, DNS flush, VPN settings, public/network information, Downloads, All Apps, AppData, Explorer path copying, Recycle Bin, OS/CPU/memory/disk/battery information, ports, and Git root copying.

## Intentionally platform-specific

Some capabilities cannot translate literally because they expose a macOS product rather than a user goal:

- Dock, menu bar, Finder restart, Homebrew, and AppleScript commands remain labeled macOS-only. A Windows feature should be added under its own accurate name rather than presenting taskbar, Explorer, or WinGet behavior as if it were the macOS feature.
- Raycast extensions that directly execute arbitrary AppleScript remain macOS-only. Extensions using Tezbar's cross-platform APIs continue to work.

## Remaining native parity work

These areas still need Windows-native implementations before Windows can be called feature-complete:

1. Replace the Swift Vision ScreenOCR helper with Windows OCR and port the interactive `NSColorSampler` color picker.
2. Add a Windows UI Automation tree reader to match the macOS accessibility snapshot helper. Input injection and screenshot-based agent actions already work on Windows, but semantic control discovery does not.
3. Read and restore Windows clipboard file lists using `CF_HDROP`; text and image history already work.
4. Extract icons for packaged Microsoft Store applications. Classic shortcut/executable icons are supported.
5. Add Windows-specific automatic installation guidance for local Whisper/Moonshine runtimes.
6. Produce and smoke-test signed x64 and ARM64 installers in Windows CI. The Tauri config now targets MSI and per-user NSIS packages, but installer behavior must be validated on an actual Windows host.

## Build behavior

Run `pnpm build:windows` on Windows to produce both a per-user NSIS installer and an MSI package. The command checks for Rust and Cargo, uses the Windows-specific Tauri configuration, and writes installers under `src-tauri/target/release/bundle/`.

The generic `pnpm build` command is also platform-aware. The native helper step builds Swift helpers only on macOS and is a no-op on Windows. Tauri merges `tauri.macos.conf.json` or `tauri.windows.conf.json` and packages only resources valid for that operating system. The Windows host downloads Bun on first launch if it is not already installed.

Cross-compilation from macOS can type-check Rust code, but a complete Windows Tauri build also requires a Windows resource compiler and cannot replace an installer test on Windows.
