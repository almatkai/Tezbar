# AGENTS.md — Tezbar AI Agent Instructions

Welcome to the Tezbar project! This document outlines general guidelines and points to specialized skills.

## Developer context

Разработчик Tezbar почти всегда использует бета-версию приложения. Учитывай это при диагностике проблем и проверке поведения приложения.

---

## 1. Core Architecture & Stack
- **Framework**: Tauri v2 (Rust backend in `src-tauri/`) with Web frontend (`src/renderer/`).
- **Runtime / Package Manager**: Node.js 22+ with `pnpm`.
- **Bundler & UI**: Vite, React 18, TypeScript, Tailwind CSS.

---

## 2. Quality Gates
Before submitting PRs or finalizing tasks:
- `pnpm typecheck` must pass without errors.
- `pnpm test` must pass (unit and integration test suites).
- Do not commit unstaged or corrupted build artifacts.

---

## 3. Specialized Skills
To prevent bloating everyday context with domain-specific procedures, detailed workflows are maintained as Agent Skills under `.agents/skills/`:

- [release-guidelines](.agents/skills/release-guidelines/SKILL.md): **Release Guidelines Skill**
  - Mandatory dual-platform release rule (Windows & macOS Apple Silicon must always be updated together in `latest.json`).
  - Bidirectional Tailscale remote build workflow:
    - If on Windows: connect to MacBook (`100.67.107.38`), build macOS artifacts, pull them to Windows (or direct upload), build Windows locally, release together.
    - If on macOS: connect to Windows PC (`100.77.254.64`), build Windows artifacts, pull them to Mac (or direct upload), build macOS locally, release together.
  - Tauri minisign signing and universal `latest.json` feed generation.
