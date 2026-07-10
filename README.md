# Tezbar

Tezbar is a desktop command surface for search, AI help, terminal access, notes, snippets, clipboard history, and a handful of small utility tools. It is built for a keyboard-first workflow and aims to keep the common stuff in one place.

## What’s in the app

- Command bar for launching actions quickly
- AI chat and agent-style workflows
- Embedded terminal
- Clipboard history
- Notes and snippets
- Emoji picker
- Currency and calculator helpers
- Extension browsing and execution
- Voice input and text-to-speech
- System commands and safety prompts (platform-dependent)

## Tech Stack

- Electron + Vite for the main desktop app
- React + TypeScript for the UI
- Rust, Swift, and native helpers for platform features
- SQLite for local persistence and search

## Requirements

- macOS or Windows 10/11
- [pnpm](https://pnpm.io/)
- Homebrew for some native dependencies
- Rust toolchain for native modules

## Setup

```bash
pnpm install
pnpm build:native
pnpm dev
```

On Windows, skip `pnpm build:native`: the currently bundled native helper
scripts target macOS. Install dependencies, then start Electron with `pnpm dev`.

## Useful Scripts

- `pnpm dev` - start the Electron app in development mode
- `pnpm build` - build the Electron app
- `pnpm build:native` - build native helpers
- `pnpm dist` - build the app and package with Electron Builder
- `pnpm dist:win` - package a Windows NSIS installer and portable executable
- `pnpm tauri:dev` - build the backend and run the Tauri app in dev mode
- `pnpm tauri:build` - build the backend and package the Tauri app

## Windows support status

### Working in the Electron app

- Launcher UI, hotkey (`Alt+Space` by default), AI chat, notes, snippets,
  calculator, currency, emoji picker, extensions, and voice UI.
- Built-in PowerShell terminal through Windows ConPTY.
- Search and launch applications from Start Menu shortcuts.
- Clipboard history for text and images.
- Open Ports using `netstat.exe`, with process names resolved through
  `tasklist.exe`.
- Windows system helpers: dark mode, lock screen, suspend, keep-awake,
  volume up/down/mute, Downloads, network, CPU, memory, disk, and battery
  information. Wi-Fi adapter toggling asks for Windows UAC confirmation.
- Windows packaging command: `pnpm dist:win`.

### Remaining work for a full Windows port

- Replace the Swift OCR and color-picker helpers with Windows-native helpers.
- Add Windows UI-automation/accessibility support for agent actions.
- Replace AppleScript and Finder-specific extension actions with Explorer and
  WinAPI equivalents.
- Add native Windows clipboard file-list support and installed-app icons.
- Port the Tauri configuration and Rust backend, which still use macOS-private
  APIs and resources.
- Make Electron-native module rebuilding reliable on machines without Visual
  Studio's Spectre-mitigated libraries; `better-sqlite3` must match Electron's
  ABI and `node-pty` should continue to use its Windows prebuilt binary.
- Complete and test a signed Windows installer build in CI.

## Tauri Builds

Tauri is configured separately in [`src-tauri/tauri.conf.json`](/Users/almatkairatov/Desktop/code/Raymes/src-tauri/tauri.conf.json). It uses the app’s branded icon set and can produce macOS DMG output.

To build the Tauri app:

```bash
pnpm tauri:build
```

The macOS DMG is emitted under:

```text
src-tauri/target/release/bundle/dmg/
```

## Icon Assets

The Electron build uses the branded icon files in [`build/`](/Users/almatkairatov/Desktop/code/Raymes/build). Tauri now uses the same source artwork, so both builds should present Tezbar branding instead of the default placeholder icon.

## Notes

- The app’s current package manager is `pnpm`.
- Windows supports the launcher UI, search, Start Menu applications, terminal,
  clipboard text/images, Open Ports, and the cross-platform system helpers.
  AppleScript, Finder automation, macOS accessibility snapshots, and the Swift
  OCR/color-picker helpers remain macOS-only.
- This repository contains both Electron and Tauri configuration, but the primary app flow is Electron/Vite.
- If macOS Finder shows an old app icon after rebuilding, that is usually icon cache lag rather than a bad build.
