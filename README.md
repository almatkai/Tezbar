# Tezbar

Tezbar is a macOS command surface for search, AI help, terminal access, notes, snippets, clipboard history, and a handful of small utility tools. It is built for a keyboard-first workflow and aims to keep the common stuff in one place.

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
- macOS system commands and safety prompts

## Tech Stack

- Electron + Vite for the main desktop app
- React + TypeScript for the UI
- Rust, Swift, and native helpers for platform features
- SQLite for local persistence and search

## Requirements

- macOS
- [pnpm](https://pnpm.io/)
- Homebrew for some native dependencies
- Rust toolchain for native modules

## Setup

```bash
pnpm install
pnpm build:native
pnpm dev
```

## Useful Scripts

- `pnpm dev` - start the Electron app in development mode
- `pnpm build` - build the Electron app
- `pnpm build:native` - build native helpers
- `pnpm dist` - build the app and package with Electron Builder
- `pnpm tauri:dev` - build the backend and run the Tauri app in dev mode
- `pnpm tauri:build` - build the backend and package the Tauri app

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
- This repository contains both Electron and Tauri configuration, but the primary app flow is Electron/Vite.
- If macOS Finder shows an old app icon after rebuilding, that is usually icon cache lag rather than a bad build.
