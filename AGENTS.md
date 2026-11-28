# AGENTS.md — Tezbar AI Agent Instructions

Welcome to the Tezbar project! This document outlines general guidelines and points to specialized skills.

## Developer context

Разработчик Tezbar почти всегда использует бета-версию приложения. Учитывай это при диагностике проблем и проверке поведения приложения.

---

## ⚠️ СТРОГОЕ ПРАВИЛО: НИКАКИХ БИЛДОВ НА GITHUB ACTIONS — ТОЛЬКО ЛОКАЛЬНО ЧЕРЕЗ TAILSCALE

- **НИКОГДА НЕ ЗАПУСКАТЬ СБОРКИ НА GITHUB ACTIONS / CI СЕРВЕРАХ**: Запрещено тратить GitHub Actions кредиты на сборку приложений (`pnpm build`, `tauri build`, релизные workflow).
- **ВСЕ СБОРКИ ДЕЛАЮТСЯ ТОЛЬКО ЛОКАЛЬНО**:
  - На этом MacBook (Apple Silicon): сборка локально через скрипты проекта (`node scripts/build-release.mjs ...`).
  - На Windows-компьютере / ноутбуке (или любом другом ноутбуке/MacBook): **находи машину через Tailscale** (`tailscale status`), они всегда подключены к единой сети Tailscale.
  - **Удаленная сборка через Tailscale SSH**:
    - Проверить активные устройства: `tailscale status` (например, Windows PC `desktop-ob83s8j` на `100.77.254.64` или ноутбук `ako` на `100.73.150.54`).
    - Подключиться по SSH к удаленной машине через Tailscale, стянуть актуальную ветку, собрать билд локально на ней и забрать готовые артефакты через `scp`.
    - Подробный воркфлоу расписан в [.agents/skills/release-guidelines/SKILL.md](.agents/skills/release-guidelines/SKILL.md).

---

## 1. Core Architecture & Stack
- **Application**: Rust core with native GPUI rendering for every screen and extension view.
- **JavaScript compatibility**: Start the isolated Bun worker only for JS/Raycast extension use. Startup, built-in services, installed-extension metadata and settings stay in Rust.
- **Tooling**: Node.js and `pnpm` build/test tools. The retired React renderer is a regression oracle under `tests/oracles/legacy-renderer`; the Tauri host is archived under `docs/native-m11/legacy-tauri-archive`. Keep both outside application entrypoints.
- **Native build and packaging**: Read `scripts/native/README.md` before changing startup/build routes. Signing pins and resource policies live in `packaging/`.

---

## 2. Quality Gates
Before building, signing, re-signing, or publishing a macOS app, read [macOS signing continuity](docs/MACOS_SIGNING.md). Release builds must pass its pinned certificate-identity check, including Alpha/Beta builds with `--allow-unsigned`. Updater signatures alone do not satisfy this check.

Before submitting PRs or finalizing tasks:
- `pnpm typecheck` must pass without errors.
- `pnpm test` must pass (unit and integration test suites).
- Do not commit unstaged or corrupted build artifacts.

---

## 3. Specialized Skills
To prevent bloating everyday context with domain-specific procedures, detailed workflows are maintained as Agent Skills under `.agents/skills/`:

- [release-guidelines](.agents/skills/release-guidelines/SKILL.md): **Release Guidelines Skill**
  - **Категорический запрет на GitHub Actions**: все сборки выполняются строго локально (на Mac и через Tailscale SSH на Windows).
  - **Dual-platform release rule**: Windows & macOS Apple Silicon всегда должны быть подписаны и обновлены вместе в одном `latest.json`.
  - **Предотвращение редиректа в браузер (manualAvailable)**: `gh release create` и `publish-feed.mjs` ДОЛЖНЫ выполняться непрерывно в единой цепочке, после чего сразу обновляется `updates/<channel>.json`. Если релиз на GitHub появится раньше фида на `channel-<channel>`, приложение посчитает релиз ручным и перекинет пользователя в браузер вместо тихого автообновления.
  - Пошаговый алгоритм удаленной сборки и публикации подробно расписан в [.agents/skills/release-guidelines/SKILL.md](.agents/skills/release-guidelines/SKILL.md).
