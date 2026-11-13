# Tezbar ⚡️

> **Meet Tezbar: Your Mac’s command bar on steroids.**  
> A fast, keyboard-first desktop launcher and autonomous command surface built on **Rust + GPUI**. Full Raycast extension compatibility, built-in terminal, multimodal AI agent, and zero lock-in.

---

## Key Features

### 🔌 Bring Your Own Extensions (Raycast Compatible)

- **Zero-approval installs**: Run any Raycast extension natively.
- **Install from GitHub**: Simply paste any GitHub repository URL into Tezbar — it clones, builds, and registers the extension into your launcher in seconds. No centralized store approvals required.

### 🤖 Autonomous AI Agent (Pi Agent Under the Hood)

- **Active Screen Vision (`⌘⇧S`)**: Attach a snapshot of your active display or specific window directly into your chat.
- **Multimodal context**: Ask the agent to debug code, inspect terminal outputs, analyze error stacks, or redesign UI components with full visual and OCR context.
- **Local knowledge search**: Vector index and local document parsing (PDFs, text, OCR) accessible directly by the agent and command bar.

### 💻 Built-in Native Terminal

- Launch shell commands, run one-liners, inspect logs, and manage background processes directly from the overlay without switching windows.
- Persistent session management and quick shortcuts.

### 🔓 Zero Lock-In & Local-First

- **Offline with Ollama**: Run completely local, private LLMs on your machine.
- **Bring Your Own Keys**: First-class support for Google Gemini, Anthropic Claude, DeepSeek, OpenAI, and GitHub Copilot.
- Your data and keys stay entirely on your device.

### 🛠 Developer Swiss Army Knife

- **Folder Navigation (`/`)**: Press `/` in the command bar to rapidly search directories, launch IDEs, or open terminal sessions.
- **Open Ports Inspector**: View active processes bound to local ports (3000, 8080, etc.) and kill rogue processes in one click.
- **System Monitor**: Live CPU, RAM, and GPU metrics in a compact glass interface.
- **Clipboard History & Snippets**: Searchable clipboard history with image support and dynamic text expansion templates.
- **Color Picker**: Interactive screen color magnifier and palette extractor.

---

## Tech Stack

- **Core and interface**: [Rust](https://www.rust-lang.org/) + GPUI
- **Extension compatibility**: isolated Bun worker for JavaScript/Raycast commands; every extension view is rendered by GPUI
- **Native Modules**: Swift helpers on macOS (Screen OCR, Accessibility, Color Picker), ConPTY on Windows
- **Storage & Indexing**: SQLite with local vector embeddings

The Rust core owns search, settings, persistence, AI orchestration, Knowledge,
clipboard history, system commands and terminal sessions. Raycast JavaScript
extensions run in a separate Bun worker, started only when extension execution
or installation needs it. Starting Tezbar and using its built-in features does
not start Bun. See [Rust core architecture and validation](docs/RUST_CORE.md).

---

## Getting Started

### Prerequisites

- **Node.js**: v22+ & [pnpm](https://pnpm.io/) for development and builds
- **Rust toolchain**: cached Rust 1.98.1 for GPUI (pinned in `rust-toolchain.toml`); core/updater retain their documented MSRV
- **macOS** (12+) or **Windows** (10/11)

### Setup & Run in Development

```bash
# Clone the repository
git clone https://github.com/almatkai/Tezbar.git
cd Tezbar

# Install dependencies
pnpm install

# Build native Swift helpers (macOS)
pnpm build:native

# Start an isolated native GPUI preview
pnpm dev

# Inspect its build/launch command without starting the UI
pnpm dev --command
```

---

## Useful Scripts

The default app entrypoints build and run Rust/GPUI. `pnpm dev` uses a fresh,
minimal UI-only temporary profile and does not access the production profile.
Extension execution, native helpers and embedding features need prepared resources;
use `pnpm dev --help` for explicit local resource inputs. Development has no hot reload.

| Script | Description |
| --- | --- |
| `pnpm dev` | Build and run the isolated GPUI development app |
| `pnpm preview` | Run the same GPUI preview with a release-profile binary |
| `pnpm build` | Compile the native GPUI executable in Cargo release mode |
| `pnpm native:preview:package --resources <stage> --output <fresh-temp-child>` | Assemble an explicit private macOS preview; not a publication artifact |
| `pnpm release:build --help` | Native release builder inputs and signing policy |
| `pnpm build:windows --help` | Native Windows build/installer entrypoint |
| `pnpm ui:check` | Reject web UI in default routes and normal native dependencies |
| `pnpm build:native` | Compile platform helpers |
| `pnpm typecheck && pnpm test` | Check worker contracts and archived behavior oracles |
| `cargo test --locked --offline -j2 -p tezbar-native --features runtime-shaders` | Check native UI, services and lifecycle fixtures |
| `pnpm test:extension-worker` | Build and exercise the isolated extension worker; requires Bun |
| `pnpm icons:sync` | Copy reviewed icons from `packaging/icons` into resource locations |

The previous React UI lives under `tests/oracles/legacy-renderer` solely as a
behavioral reference and regression-test input. Its React/Tauri libraries are
build/test dependencies, not an application frontend. The retired Tauri host and
frontend build configuration are archived under `docs/native-m11`; active signing,
icon and runtime-catalog inputs live under `packaging`.

---

## License

Apache License 2.0 © [Almat](https://github.com/almatkai)
