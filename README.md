<div align="center">
  <br />
  <img src="assets/logo.svg" width="128" height="128" alt="Tezbar Logo" />
  <br />
  <br />

  <h1>Tezbar</h1>

  <h3>Spotlight · AI Agent · Terminal — fused into one floating macOS command surface.</h3>

<p>
  <strong>Tezbar</strong> is the app that should have shipped with macOS. One hotkey (<kbd>Alt</kbd>+<kbd>Space</kbd>) opens a single floating window that lets you search everything, command an AI coding agent, and drop into a real terminal — without ever leaving the bar.
</p>

<p>
  <a href="#"><img src="https://img.shields.io/badge/macOS-000000?logo=apple&logoColor=white&style=flat-square" /></a>
  <a href="#"><img src="https://img.shields.io/badge/Electron-47848F?logo=electron&logoColor=white&style=flat-square" /></a>
  <a href="#"><img src="https://img.shields.io/badge/React-0A0A0A?logo=react&logoColor=61DAFB&style=flat-square" /></a>
  <a href="#"><img src="https://img.shields.io/badge/Rust-0A0A0A?logo=rust&logoColor=white&style=flat-square" /></a>
  <a href="#"><img src="https://img.shields.io/badge/Swift-0A0A0A?logo=swift&logoColor=F05138&style=flat-square" /></a>
  <a href="#"><img src="https://img.shields.io/badge/TypeScript-0A0A0A?logo=typescript&logoColor=3178C6&style=flat-square" /></a>
  <a href="#"><img src="https://img.shields.io/badge/License-Private-0A0A0A?style=flat-square" /></a>
</p>

<img src="assets/screenshot.png" width="800" alt="Tezbar floating command bar" style="border-radius:12px; box-shadow:0 24px 80px rgba(0,0,0,0.4);" />

</div>

---

## 🔥 The Big Idea

Tezbar combines three tools you already use every day into one keyboard-first window:

```text
┌─────────────────────────────────────────────────────────────────────────────┐
│                                                                             │
│     ┌──────────────┐        ┌──────────────┐        ┌──────────────┐       │
│     │   🔦 SPOT    │   +    │   🤖 AGENT   │   +    │   🖥️  TERMINAL│       │
│     │   LIGHT      │        │   (PI)       │        │              │       │
│     └──────┬───────┘        └──────┬───────┘        └──────┬───────┘       │
│            │                       │                       │               │
│            └───────────────────────┼───────────────────────┘               │
│                                    ▼                                        │
│                         ┌─────────────────┐                                 │
│                         │     TEZBAR      │  ←  Alt + Space                │
│                         │  one floating   │                                 │
│                         │     window      │                                 │
│                         └─────────────────┘                                 │
│                                    │                                        │
│            ┌───────────────────────┼───────────────────────┐                │
│            ▼                       ▼                       ▼                │
│     📋 Clipboard            🎤 Voice            🧩 Extensions             │
│     💱 Converter            🔒 Safety           📝 Notes / Snippets       │
│     🌐 Ports / Processes    🎨 Color Picker     ⚙️  System Commands        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**No tab switching. No separate apps. One shortcut, every workflow.**

---

## 🤖 AI Coding Agent

A real coding agent lives inside the spotlight interface, powered by the **PI agent** runtime.

- **Read** files on your machine
- **Run** bash commands and shell scripts
- **Edit & write** code across projects
- **Search** with ripgrep (`grep`), glob patterns (`find`), and directory listings (`ls`)
- **See** screenshots via vision models
- **Reason** with extended-thinking models
- **Watch** every tool call unfold live in the HUD

**Supported LLM providers:**

| Provider | Integration |
|---|---|
| **OpenAI** | Chat Completions API |
| **DeepSeek** | Official API |
| **Anthropic** | Messages API |
| **Gemini** | OpenAI-compatible / Google endpoint |
| **Ollama** | Local Ollama server |
| **GitHub Copilot** | Copilot Chat access token |
| **OpenCode** | opencode.ai CLI |
| **OpenAI Compatible** | Any OpenAI-style endpoint (vLLM, Groq, Together, etc.) |

---

## 🖥️ Built-in Terminal

Tezbar embeds a full terminal emulator inside the floating window. No context switching.

```text
┌─────────────────────────────────────────────────────────────┐
│  tezbar ~                                                    │
│  $ git status                                                │
│  On branch main                                              │
│  Your branch is up to date with 'origin/main'.               │
│                                                              │
│  nothing to commit, working tree clean                       │
│  $ _                                                        │
└─────────────────────────────────────────────────────────────┘
```

- **Summon** with `> terminal` from the command bar or a dedicated shortcut.
- **Focused sessions** — the shell stays alive while the terminal surface is open and closes when you leave it.
- **Agent-aware** — the AI agent can run commands in the terminal and read its output.
- **Native shell** — your default shell (`zsh`, `fish`, `bash`) with `$PATH`, colors, and cursor support.

---

## 🎁 Everything Else

Tezbar is more than the trinity. It also ships with:

| Feature | What It Does |
|---|---|
| **Voice Interface** | Hold `Alt+Space` to dictate with local Whisper or Moonshine models. |
| **Text-to-Speech** | Built-in TTS with selectable voices. |
| **Smart Tools** | Calculator, currency converter (Frankfurter ECB rates), and color converter inline. |
| **Clipboard History** | Up to 200 entries, searchable and filterable by text/image/file. Image capture is opt-in. |
| **Snippets** | Trigger-keyword text expansion. |
| **Quick Notes** | Save, search, edit, and delete notes inline. |
| **Emoji Picker** | Search by name/mood/category; recently used floats to top. |
| **Raycast Extensions** | Browse, install, and run Raycast extensions from the built-in store — port managers, process killers, and thousands more. |
| **Native macOS Control** | Lock, sleep, toggle Wi-Fi/Bluetooth, volume, dark mode, empty trash, etc. |
| **Safety Controls** | Dry-run previews, required confirmations, and audit logging for safety-aware shell, extension-install, and native system actions. |
| **Privacy First** | Local voice transcription and a permission viewer. Extension install/uninstall counts use a persistent anonymous machine ID. |

---

## 🛠️ Architecture

```text
tezbar/
├── src/
│   ├── main/              # Electron main process
│   │   ├── agent/         # PI agent runtime (loop, bridge, tools)
│   │   ├── chat/          # Chat persistence (SQLite)
│   │   ├── llm/           # LLM provider adapters
│   │   ├── search/        # SQLite FTS5 index + ranking
│   │   ├── extensions/    # Raycast API shim
│   │   ├── voice/         # STT / TTS
│   │   ├── terminal/      # Built-in terminal backend
│   │   ├── safety/        # Confirmation dialogs + audit log
│   │   ├── nativeCommands/# macOS system commands
│   │   └── permissions/   # macOS permission management
│   ├── renderer/          # React UI
│   ├── preload/           # Electron IPC bridge
│   └── shared/            # Shared types & constants
├── native/
│   ├── input/             # Rust: mouse, keyboard, screenshot, HID
│   ├── axhelper/          # Swift: accessibility tree
│   └── color-picker/      # Swift: macOS color picker
└── SuperCmd-main/         # Raycast extension ecosystem (vendor)
```

### Stack

| Layer | Technology |
|---|---|
| Desktop shell | Electron + Vite |
| UI | React 18 + Tailwind CSS 3 |
| Agent runtime | PI coding agent (JSONL RPC) |
| Terminal | Embedded terminal emulator |
| Native input | Rust (core-graphics, napi-rs) |
| Accessibility | Swift (AppKit, AXUIElement) |
| Search index | SQLite FTS5 (better-sqlite3) |
| LLM providers | Anthropic, OpenAI, Ollama, custom adapters |
| Speech | whisper.cpp, moonshine_voice |

---

## 🚀 Getting Started

### Prerequisites

- macOS (Sonoma+ recommended)
- [pnpm](https://pnpm.io/) — `npm install -g pnpm`
- [Homebrew](https://brew.sh/)
- Rust toolchain — `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

### Install & Run

```bash
cd tezbar
pnpm install
pnpm build:native   # Rust input addon + Swift axhelper
pnpm dev
```

### Optional Voice Models

```bash
brew install whisper-cpp
pip3 install --user moonshine_voice
```

### Production Build

```bash
pnpm build
pnpm preview
```

---

## ⌨️ Keyboard Shortcuts

| Shortcut | Action |
|---|---|
| `Alt+Space` | Toggle command bar (hold to dictate) |
| `Alt+Enter` | Open command bar directly |
| `⌘N` | New note / snippet / chat (context-aware) |
| `⌘,` | Open settings |
| `⌘Escape` | Hide Tezbar |
| `Escape` | Back to command surface / dismiss |

---

## 🧪 Testing

```bash
pnpm test           # Vitest watch mode
pnpm test -- --run  # Single run
```

Tests cover currency parsing, LLM model normalization, search ranking, extension registry, safety registry, native commands, and routing.

---

## 💬 Agent Examples

| Query | Result |
|---|---|
| `what files are on my desktop?` | `ls ~/Desktop` |
| `show my node version` | `node --version` |
| `edit README.md and add a features section` | Reads, edits, saves |
| `find all png files in ~/Downloads` | `find` tool |
| `make a directory called test-project` | `mkdir` |
| `what apps are installed?` | Lists `/Applications` |

Prefix queries with `$` or `>` to force agent mode.

---

## ⚖️ Credits

- **[PI Agent](https://pi.ai)** — Agentic core and tool runtime.
- **[Raycast](https://raycast.com)** — UI inspiration and extension ecosystem.
- **[SuperCmd](https://github.com/SuperCmdLabs/SuperCmd)** — Raycast API shim reference.
- **[mac-cli](https://github.com/guarinogabriel/mac-cli)** — Native macOS command inspiration.
- **[Frankfurter API](https://www.frankfurter.app)** — Exchange rates.
- **[whisper.cpp](https://github.com/ggerganov/whisper.cpp)** — Local speech-to-text.
- **[moonshine_voice](https://github.com/usefulsensors/moonshine)** — On-device voice model.

---

## 📄 License

Private — all rights reserved.
