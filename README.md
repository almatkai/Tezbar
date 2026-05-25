# Raymes

![Raymes Screenshot](assets/screenshot.png)

**Raymes** is a high-performance, lightweight **Spotlight alternative** for macOS that blends the best of **Raycast** with the power of an **integrated AI coding agent**. It lives in a single floating window summoned by `Alt+Space` — ready to search files, run commands, convert currencies, chat with an AI, and even modify code on your machine.

Built with **Electron + Vite + React** and a stack of native Rust/Swift helpers, Raymes stays fast, minimal, and privacy-conscious. No cloud dependency for voice transcription; no telemetry.

---

## ✨ Features

### 🔍 Universal Search Bar

The heart of Raymes is its command bar. Start typing and Raymes instantly searches across:

| Category | Description |
|---|---|
| **Commands** | Built-in actions (open clipboard, manage ports, open emoji picker, settings, etc.) |
| **Files & Folders** | Indexed via FTS + Spotlight fallback; type `/` or `~/` for file path completion |
| **Applications** | Type `` ` `` to browse and launch macOS apps |
| **Extensions** | Installed Raycast-compatible extensions and their commands |
| **Clipboard** | Historical clipboard entries with text, images, URLs |
| **Quick Notes** | Full-text search over saved notes |
| **Snippets** | Searchable text templates and boilerplate |
| **Quick Links** | Frequently accessed URLs |
| **Open Ports** | Active TCP listeners — type "port" to see and kill them |

The search engine uses BM25+FTS with per-category ranking boosts, recency weighting, frequency tracking, and fuzzy matching via Fuse.js. It learns from your usage — frequently used actions rank higher over time.

### 🤖 AI Agent (The Best Part)

Raymes ships with a **full coding agent** built directly into the spotlight interface, powered by the **PI agent** runtime. It can:

- **Read** files on your machine
- **Run bash commands** and shell scripts
- **Edit and write code** across your projects
- **Search** files with ripgrep (`grep`), glob patterns (`find`), and directory listings (`ls`)
- **Understand context** — ask questions about your system, installed apps, or codebase

The agent supports **vision** (send screenshots), **thinking** (extended reasoning), and **tool use** (plan-and-execute). Every tool call is visualized live in the HUD with progress stages so you see exactly what the agent is doing.

**Supported LLM providers:**

| Provider | Type |
|---|---|
| **OpenAI** | GPT-4o, GPT-4o-mini, o3-mini via Chat Completions API |
| **DeepSeek** | DeepSeek V4 Flash/Pro, R1 via official API |
| **Anthropic** | Claude 3.5 Haiku/Sonnet via official API |
| **Gemini** | Google Gemini 2.0 Flash / 1.5 Pro via OpenAI-compatible endpoint |
| **Ollama** | Local models (Llama 3.2, LLaVA, and any other OLLAMA-served model) |
| **GitHub Copilot** | Copilot Chat via access token |
| **OpenCode** | opencode.ai via CLI |
| **OpenAI Compatible** | Any OpenAI-style endpoint (vLLM, groq, Together, etc.) |

### 🔊 Voice & Speech

- **Hold-to-Speak** — Press and hold `Alt+Space` to dictate using local models. Release to transcribe and submit.
- **Local transcription** — Supports **Whisper (via whisper.cpp)** and **Moonshine/Parakeet** for ultra-low latency, fully offline voice-to-text.
- **Text-to-Speech** — Built-in TTS support with selectable voices.
- **No cloud dependency** — All audio processing stays on your machine.

### 💱 Smart Tools (Inline in the Search Bar)

| Tool | Example Queries |
|---|---|
| **Calculator** | `42 * 3.14`, `sqrt(144)`, `sin(45°)` |
| **Currency Conversion** | `100 USD to EUR`, `5000 yen in pounds`, `₸15000 в тенге` |
| **Color Converter** | `#ff5500`, `rgb(100,200,50)`, `hsl(220,50%,40%)` |

Currency data is fetched from the **Frankfurter API** (European Central Bank rates). Supports 40+ currencies with fuzzy name matching in multiple languages.

### 🖥️ Native macOS Control

Raymes provides a comprehensive set of native system commands accessible from the search bar:

- **System**: Lock Screen, Sleep Display, Sleep Mac, Start Screen Saver, Restart, Shut Down, Log Out, Empty Trash, Toggle Dark Mode
- **Connectivity**: Toggle Bluetooth, Toggle Wi-Fi, Show Network Info
- **Navigation**: Open Applications folder, Open Downloads, Open Documents, Reveal Library folder
- **Hardware**: Eject all disks, Volume Up/Down, Toggle Mute
- **Developer**: List Listening Ports, Kill process on port, Git root path, Homebrew update/upgrade, Disk/Memory/CPU usage info
- **Finder**: Show/Hide hidden files

All destructive actions (shell commands, process kills, system shutdown) go through a **safety confirmation dialog** with structured logging.

### 🧩 Raycast Extension Support

Raymes implements a substantial subset of the **Raycast extension API**, allowing you to install and run thousands of community extensions directly:

- **Extension Store** — Browse, search, and install extensions from the Raycast store via an API/backend
- **Command runtime** — Run extension commands with form inputs, list views, detail views, and grid views
- **Action panels** — Full support for Raycast actions (Copy, Open, Run, etc.)
- **Screenshot previews** — View extension screenshots before installing
- **Install tracking** — Anonymous install/uninstall reporting (opt-out via settings)
- **Fallback install** — Git sparse-checkout + npm install when the API is unavailable

### 📋 Productivity Suite

| Feature | Description |
|---|---|
| **Clipboard History** | Automatically tracks everything you copy. Browse, search, filter by text/image/URL/file, and paste directly. Stores up to 1000 entries. |
| **Snippets** | Create text templates with an optional trigger keyword. Type the trigger in the search bar to auto-expand. |
| **Quick Notes** | A lightweight local notepad. Save notes inline with `⌘N`, search through them instantly, edit or delete as needed. |
| **Emoji Picker** | Search emojis by name, mood, or category. Recently used emojis float to the top. Paste directly into any app. |
| **Port Manager** | See all active TCP listeners, identify which process owns each port, and kill listeners with one click (safety-confirmed). |

### 🔒 Privacy & Safety

- **Self-hosted voice models** — No data leaves your machine
- **Permission viewer** — See exactly which macOS permissions Raymes has (Accessibility, Screen Recording, Microphone, Full Disk Access, Input Monitoring, Automation) with system links to grant/revoke each
- **Safety confirmation dialog** — Every destructive action (shell command, process kill, system shutdown, file deletion) requires explicit user confirmation
- **Audit log** — All safety-relevant actions are logged with timestamps, context, and outcome
- **Dry run mode** — Test what would happen without actually executing

---

## 🛠️ Architecture

```
raymes/
├── src/
│   ├── main/           # Electron main process
│   │   ├── agent/      # PI agent integration (loop, bridge, observer, tools)
│   │   ├── chat/       # Chat session persistence (SQLite)
│   │   ├── llm/        # LLM providers (OpenAI, Anthropic, Ollama, Copilot, etc.)
│   │   ├── search/     # Search index (SQLite FTS5), providers, ranking
│   │   ├── extensions/ # Raycast API shim, extension runtime
│   │   ├── voice/      # Speech-to-text / text-to-speech (Whisper, Moonshine)
│   │   ├── safety/     # Confirmation dialogs, audit logging
│   │   ├── nativeCommands/ # macOS system commands executor
│   │   └── permissions/    # macOS permission management
│   ├── renderer/       # React UI (Electron renderer)
│   │   ├── ui/         # Reusable components (GlideList, Markdown, primitives)
│   │   ├── currency/   # Currency conversion parsing & preferences
│   │   ├── emoji/      # Emoji data & search
│   │   └── hooks/      # React hooks (hold-to-speak, currency)
│   ├── preload/        # Electron preload (IPC bridge)
│   └── shared/         # Shared types & constants
├── native/
│   ├── input/          # Rust native addon (mouse, keyboard, screenshot, HID polling)
│   ├── axhelper/       # Swift accessibility tree helper
│   └── color-picker/   # Swift macOS color picker
├── assets/             # Screenshots
└── SuperCmd-main/      # Raycast extension ecosystem (vendor)
```

### Key Technologies

| Layer | Technology |
|---|---|
| **Desktop Shell** | Electron + Vite |
| **UI** | React 18 + Tailwind CSS 3 |
| **Agent Runtime** | PI coding agent (JSONL RPC) |
| **Native Input** | Rust (core-graphics, napi-rs) |
| **Accessibility** | Swift (AppKit, AXUIElement) |
| **Search Index** | SQLite FTS5 (via better-sqlite3) |
| **LLM Providers** | Anthropic SDK, OpenAI SDK, Ollama, custom adapters |
| **Speech** | whisper.cpp CLI, moonshine_voice Python package |
| **Extension API** | Custom Raycast API shim |

---

## 🚀 Getting Started

### Prerequisites

- **macOS** (Sonoma+ recommended)
- [pnpm](https://pnpm.io/) (`npm install -g pnpm`)
- [Homebrew](https://brew.sh/) (for optional voice model support)
- Rust toolchain (for native builds): `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`

### Installation

```bash
# Clone and enter the project
cd raymes

# Install dependencies
pnpm install

# Build native modules (Rust input addon + Swift axhelper)
pnpm build:native

# Start development
pnpm dev
```

### Optional: Voice Model Setup

```bash
# For Whisper support
brew install whisper-cpp

# For Moonshine/Parakeet support (ultra-low latency)
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
| `⌘N` | Quick note / new snippet / new chat (context-dependent) |
| `⌘,` | Open settings |
| `⌘Escape` | Hide Raymes |
| `Escape` | Back to command surface / dismiss |

---

## 🧪 Testing

```bash
pnpm test                   # Run all tests (Vitest)
pnpm test -- --run          # Single run (no watch)
```

Raymes includes unit tests for:
- Currency query parsing
- AI provider model normalization
- Search text matching & ranking
- LLM error formatting
- Extension registry logic
- Router, safety registry, and native command registry

---

## 🧰 Extensions

Raymes can run **Raycast extensions**. To browse and install:

1. Open Raymes (`Alt+Space`)
2. Search for an extension name (e.g., "kill process", "port manager")
3. Press Enter to install from the Raymes extension store
4. Installed extension commands appear in your search results

**Currently supported Raycast API surfaces:** List, Detail, Form, Grid, Action Panel, menus, and hooks.

---

## 💬 Agent Usage Examples

| Query | What Happens |
|---|---|
| `what files are in my desktop?` | Agent runs `ls ~/Desktop` and summarizes |
| `show me my node version` | Agent runs `node --version` |
| `edit README.md and add a features section` | Agent reads the file, applies edits |
| `find all png files in ~/Downloads` | Agent uses `find` tool |
| `make a new directory called test-project` | Agent runs `mkdir` |
| `what apps are installed on my mac?` | Agent lists `/Applications` contents |

The agent automatically detects whether your query is a question (answer inline) or a task (execute tools and show results). You can also prefix queries with code-like patterns (`$`, `>`) to force agent mode.

---

## ⚖️ Credits

- **[PI Agent](https://pi.ai)** — Lightweight agentic core and tool runtime.
- **[Raycast](https://raycast.com)** — UI inspiration, extension API design, and the Raycast extension ecosystem.
- **[SuperCmd](https://github.com/SuperCmdLabs/SuperCmd)** — Patterns and reference implementation for Raycast API shims.
- **[mac-cli](https://github.com/guarinogabriel/mac-cli)** — Native macOS command inspiration.
- **[Frankfurter API](https://www.frankfurter.app)** — Exchange rate data (European Central Bank).
- **[whisper.cpp](https://github.com/ggerganov/whisper.cpp)** — Local speech-to-text.
- **[moonshine_voice](https://github.com/usefulsensors/moonshine)** — Ultra-low-latency on-device voice model.

---

## 📄 License

Private — all rights reserved.
