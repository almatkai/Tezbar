# Tezbar ⚡️

> **Meet Tezbar: Your Mac’s command bar on steroids.**  
> A fast, keyboard-first desktop launcher and autonomous command surface built on **Rust + Tauri**. Full Raycast extension compatibility, built-in terminal, multimodal AI agent, and zero lock-in.

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

- **Core**: [Tauri](https://tauri.app/) + [Rust](https://www.rust-lang.org/)
- **Frontend**: [React 18](https://react.dev/), [TypeScript](https://www.typescriptlang.org/), [Tailwind CSS](https://tailwindcss.com/)
- **Native Modules**: Swift helpers on macOS (Screen OCR, Accessibility, Color Picker), ConPTY on Windows
- **Storage & Indexing**: SQLite with local vector embeddings

---

## Getting Started

### Prerequisites

- **Node.js**: v18+ & [pnpm](https://pnpm.io/)
- **Rust toolchain**: latest stable (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
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

# Start development mode
pnpm dev
```

---

## Useful Scripts

When preparing a Tezbar release, add its user-facing Markdown notes to
`docs/releases/<package.json version>.md`. The frontend build requires this file
and bundles it into the app for the Show Updated Version launcher command and release-notes page.

| Script | Description |
|---|---|
| `pnpm dev` | Start the Tauri app in development mode with hot reload |
| `pnpm build` | Build and package the production desktop application |
| `pnpm build:native` | Compile macOS Swift native helpers |
| `pnpm build:windows` | Build Windows NSIS and MSI installers |
| `pnpm icons:generate` | Regenerate app icon assets across all formats from SVG |

---

## License

Apache License 2.0 © [Almat](https://github.com/almatkai)
