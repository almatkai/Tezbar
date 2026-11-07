# Tezbar native application (Rust/GPUI) — full migration spec

Status: design and architecture spec, 2026-11-06. **Nothing in it is implemented.** Branch
`feat/native-gpui-app`, base `5a5d9da676b8de7bab8f6e3e5ffed134988b9143` (the accepted launcher
prototype, not `origin/main`). This document replaces the scope limits of
[NATIVE_LAUNCHER_PROTOTYPE_SPEC.md](NATIVE_LAUNCHER_PROTOTYPE_SPEC.md) §1. Its measured facts,
visual brief (§2) and session rules (§3) still apply and are promoted here.

Companion: [NATIVE_GPUI_PARITY_LEDGER.md](NATIVE_GPUI_PARITY_LEDGER.md) is the row-by-row
inventory. Every row has an ID, its current owner, its new module and its acceptance check.
Migration is complete only when every ledger row is `native`, `host`, `worker` or
`not-reachable (proved)`. Section 9 has the full definition of done.

Labels:

- **Verified**: checked in this tree at `5a5d9da`, or in the GPUI source pinned at
  `c84af5b99cbc366d675064e52b23664a015b6e35` (local Cargo git checkout), with a reference.
- **Decision**: chosen by this spec. The implementer changes it only with a recorded reason
  and a review note.
- **Open**: unknown until implementation, a spike or a host check. Each has an exit test.
- **User**: a product or release choice that belongs to the user (§10.1). There are three.

Method: I read the selected worktree end to end at its seams: `App.tsx`, `main.tsx`,
`tauri-bridge.ts`, `desktop-api.ts`, `CommandBar.tsx` (modes, keys, actions, states),
every surface component, `src/shared`, `src-tauri/src/lib.rs`, `native_core` dispatch and
every `AppHandle` site, the extension worker host, packaging and updater code, the prototype
and the reports listed in the brief. Search used one bounded XERJ query (`ax-65b00e69da24`,
server healthy) and then exact `rg`. Reachability came from an import walk out of
`src/renderer/main.tsx` (script kept outside the repo; method in ledger §X). XERJ never served as
proof that something is absent.

---

## 0. Summary

1. **One product, one core.** The Tauri/WebView app becomes a GPUI application. The
   existing `native_core` moves to `crates/tezbar-core` without forking. It loses its
   `tauri` dependency behind a single `Host` port. The temporary Tauri app and the new
   GPUI app both link the same core crate until cutover. Then `src-tauri`, the React
   renderer, WebKit/WebView2 and all Tauri plugins leave the shipped product.
2. **Bun stays only for extension logic.** The Raycast-compatible worker (`src/main`, Bun)
   already renders extension React into serialized node trees
   (`src/shared/extensionRuntime.ts`, Verified). GPUI renders those trees natively and
   sends callbacks back through the existing `extension:*` channels. **No arbitrary
   HTML/WebView extension contract exists** (§1.4), so no hybrid host is needed.
3. **First-party renderer logic moves to Rust.** That covers calculator, currency, colour,
   text transforms and QR, emoji search, pins and recents, query history, Settings write
   ownership, Knowledge settings state, AI chat ownership, extension catalog and session state,
   terminal selection, release notes, updater policy and remote image fetching. It lands in a
   GPUI-free crate, `tezbar-ui-model`, with ported tests (§2.4, ledger §B).
4. **The visual identity is preserved, not redesigned.** The current `--ln-*` launcher
   sheet tokens and `--c-*` ramp become the native token set. Leftover glass and gradient
   treatments in Settings are flattened onto the same tokens (§5.6).
5. **Identity, data and signing continuity.** The bundle keeps `com.tezbar.app`, executable
   `tezbar`, the pinned designated requirement, the helper Team ID checks, the existing
   profile paths and the updater feed. The first GPUI release must install through the
   **existing Tauri updater** on both platforms (§7.3).
6. **Twelve milestones (M0–M11).** Core extraction comes first and keeps the Tauri app
   working (§8).

---

## 1. Facts that shape the plan

### 1.1 Host and core coupling (Verified)

| Fact | Reference |
| --- | --- |
| `NativeCore` is a private module of crate `tezbar` (`app_lib`). Its public surface is `new`, `start`, `request(channel, args)`, `execute_command_shortcut`, `trim_idle_memory` and `shutdown`. `request` rejects `core:` channels and waits up to 30 s for readiness. | `src-tauri/src/native_core/mod.rs` |
| `CoreContext { paths, config, app: Option<tauri::AppHandle>, desktop_services, events, router, foreground_until }`. Events go through an `EventSink` closure that calls `app.emit`. | `native_core/common.rs:9–60` |
| Every `AppHandle` or crate-root dependency inside the core: `common.rs:9,31,39`; `mod.rs:52` (vault gets `ctx.app`); `vault/mod.rs:14,38,48,63,175` (`crate::show_main_window_native` after unlock); `platform.rs:12–14,74,125–130` (global shortcuts), `:214–235` (`crate::show_window/hide_window/close_current_window/start/end_window_snap_drag`), `:294,327,393` (`crate::set_backend_app_visibility`), `:297–302` (suppress blur), `:333` (`crate::open_settings_window`), `:344,349` (`crate::quit_app_now`), `:356–362` (clear WebView data), `:378` (`crate::native_input::active_capture`), `:398` (`press_key`), `:522` (confirm via `tauri_plugin_dialog`); `terminal_bridge.rs:3,5,40,64` (PTY lives in crate-root `native_terminal`, `tauri::async_runtime::block_on`); `launcher/terminal.rs:4,213`; `knowledge/mod.rs:1863–1864` (folder dialog); `knowledge/runtime_bundle.rs:75,308,329` (`tauri::is_dev`, `crate::resource_directory`); `extensions/process_host.rs:19,275–345` (extension tray icons via `tauri::tray`); `extension_worker.rs:138` (`crate::locate_bun`), `:311–312` (`crate::timer_notifications`). | `rg "crate::\|tauri::\|ctx\.app" src-tauri/src/native_core` |
| `launcher/mod.rs:134,160,354,777` already use `ctx.runs_desktop_services()` (prototype facade). | prototype commit `fa0e1b5` |
| Host-only modules at crate root: window placement, per-monitor positions, snapping and snap overlay, settings close handshake, idle trim, tray, single instance, log plugin, updater plugin, terminal sessions panel (`lib.rs`, 3,337 lines); `window_layout.rs`; `native_input.rs`; `native_terminal.rs`; `updater.rs`, `updater_temp.rs`, `manual_releases.rs`, `release_channels.rs`; `timer_notifications.rs`; `qr_download.rs`; `resource_directory.rs`; `child_supervisor.rs` and `windows_job.rs`; `native_v2_cli.rs`; `desktop_bench.rs`. | `src-tauri/src/*.rs` |
| `main.rs` dispatches `child_supervisor::dispatch()` and `dispatch_native_v2_cli()` **before** creating the app. Extension supervisors re-launch `current_exe()` with `--tezbar-private-extension-supervisor-v1`. | `main.rs`, `extensions/process.rs:72–79` |
| GPUI's `Application::run` and tao's `EventLoop::run` both take the main thread and the NSApplication delegate, so they can't share a process. | prototype spec §4.4 |
| 51 Tauri commands are registered, 44 distinct `invoke(...)` names are used by the renderer, 143 distinct `call_backend` channels sit in the bridge, and `RaymesApi` has 206 members. Core event emitters: 25 named channels plus `terminal:data/exit`, `stream-token/done/error`, worker-forwarded `extension:install-progress` and `backend-status`. | `lib.rs:3053–3105`, `tauri-bridge.ts`, `desktop-api.ts`; ledger §H, §I |
| Rust test inventory: 620 `#[test]`/`#[tokio::test]` attributes in `src-tauri/src`. Renderer: 51 test files, 339 cases. Shared: 14 files, 141 cases. Worker (`src/main`): 82 cases. | `rg -c`, `grep -cE "^\s*(it\|test)\("` |

### 1.2 GPUI at the pin (Verified in `~/.cargo/git/checkouts/zed-a70e2ad075855582/c84af5b`)

| Fact | Consequence |
| --- | --- |
| `gpui`, `gpui_platform`, `gpui_macos`, `gpui_windows` and `gpui_tokio` are **Apache-2.0**. Zed's `markdown`, `terminal`, `terminal_view`, `editor`, `ui`, `ui_input`, `theme`, `menu`, `picker` and `component` are **GPL-3.0-or-later**. | Tezbar is Apache-2.0 (`Cargo.toml`, `package.json`). **Don't depend on or copy any GPL Zed crate.** Markdown, editor, terminal and component code are written here, on top of Apache/MIT crates (§3.8). |
| Element-level AccessKit exists: `Div::role`, `aria_label`, `aria_description`, `aria_selected`, `aria_expanded`, `aria_toggled`, `aria_value`, `aria_placeholder`, `aria_active_descendant`, `aria_position_in_set`, `aria_size_of_set`, row and column indices, `Element::a11y_role`/`write_a11y_info`/`a11y_synthetic_children`, `AccessibleAction` handling, the `accesskit_macos` and `accesskit_windows` adapters, an `_accessibility.rs` guide with text-selection synthetic children, and `examples/a11y.rs`. | The brief's premise that GPUI exposes only the standard window doesn't hold at this pin. The prototype simply set no roles. Real accessibility is still **our work**: every control needs roles, names, states and actions, and VoiceOver and Narrator need manual verification (§5.8). Parity isn't automatic. |
| `WindowKind::{Normal, PopUp, Popup(parent-anchored), Floating, Dialog}`; `WindowBackgroundAppearance::{Opaque, Transparent, Blurred, MicaBackdrop…}`; macOS `NSFloatingWindowLevel` and `CanJoinAllSpaces \| FullScreenAuxiliary` for PopUp. | Launcher, side panel and popovers can all be built. **No mouse-passthrough option**: the snap overlay needs a raw-window-handle call (`setIgnoresMouseEvents:` / `WS_EX_TRANSPARENT \| WS_EX_LAYERED`) (§3.6). |
| `Window::prompt` (`gpui_macos/src/window.rs:1680`, `gpui_windows/src/window.rs:721`; Open O-PROMPT: confirm the Windows prompt is a native dialog, not a drawn overlay), `App::prompt_for_paths`, `prompt_for_new_path`, `reveal_path`, `open_url`, `read/write_to_clipboard`, `displays()`, `on_keyboard_layout_change`, `set_menus` and `on_reopen` all exist. | Replaces `tauri_plugin_dialog` (confirm, folder picker, QR save). |
| No system-wide hotkey API and no status-bar (tray) API. | Use `global-hotkey 0.8.0` and `tray-icon 0.23.1`/`muda 0.19.2`. They're already in `src-tauri/Cargo.lock` (Verified), and they're the crates Tauri's plugins wrap, so accelerator parsing semantics stay the same. |
| `uniform_list` and `list` elements, plus an `input.rs` example with `EntityInputHandler` (IME, marked text, `bounds_for_range`). | Promote the prototype's `text_input.rs` (derived from that Apache example). |
| `gpui` feature `test-support` exists. Defaults are `font-kit` and `windows-manifest`. `runtime_shaders` lives in `gpui_platform`. | View tests use GPUI's test context (§9). |

### 1.3 Packaging and identity (Verified)

| Fact | Reference |
| --- | --- |
| Bundle `Tezbar.app`, `CFBundleExecutable = tezbar`, `CFBundleIdentifier = com.tezbar.app`, icon `icon.icns`, `LSMinimumSystemVersion 10.13`, `NSHighResolutionCapable`. Usage strings for Microphone, AppleEvents and Calendars come from `native_core/system/macos-permissions.plist`. **No `LSUIElement` and no activation-policy override**, so the current app is a regular app with a Dock icon. | `src-tauri/target/release/bundle/macos/Tezbar.app/Contents/Info.plist`; `rg ActivationPolicy` = 0 hits |
| Resources: `dist-backend/{extension-worker.js, vec0, node_modules, libsqlite3.dylib, resources/icons}`, `embedding-runtime/`, `native/{axhelper,screenocr,color-picker,image-colors}`, `node_modules/{esbuild,@esbuild}`, `licenses/…`, `trayIconTemplate@2x.png`. On Windows: `vec0.dll` and the two `.ps1` helpers. | `tauri.conf.json`, `tauri.macos.conf.json`, `tauri.windows.conf.json` |
| `src-tauri/build.rs` checks: the macOS arm64 embedding payload hash, release identity (`release_channels::default_channel(identifier, version)`), `productName == "Tezbar"`, and the Windows Common-Controls manifest. | `build.rs` |
| `macos-signing.json` pins `com.tezbar.app`, Team `UBRTH86SZ3`, the designated requirement and four helper paths. `scripts/build-release.mjs` and `lib/macos-signing.mjs` verify them. | `docs/MACOS_SIGNING.md` |
| The updater is `tauri-plugin-updater 2.10.1` with minisign pubkey `dW50cnVzdGVk…` and channel endpoints from `release_channels.rs`. On macOS it extracts `.app.tar.gz` over the running bundle. On Windows it runs the NSIS installer with install-mode args, `/UPDATE` and `/ARGS <current args>`. | `tauri.conf.json`; `~/.cargo/registry/.../tauri-plugin-updater-2.10.1/src/updater.rs:787–860` |
| The Windows bundle target is NSIS only, `installMode: currentUser`, `allowDowngrades: false`. | `tauri.windows.conf.json` |
| Data dir = Tauri `app_local_data_dir` (macOS `~/Library/Application Support/com.tezbar.app`, Windows `%LOCALAPPDATA%\com.tezbar.app`), unless overridden by `TEZBAR_DATA_DIR`. Config is `~/.openray/config.json` (`openray_config_path`). Layout profiles are in `launcher-layouts-v1.json` beside `config.json`. Per-monitor positions live in config key `windowPositionsByMonitorV3` (macOS) / `V2`. | `lib.rs:3106–3140`, `docs/launcher-layout.md`, `lib.rs:76–80` |

### 1.4 The extension UI contract is a serialized tree (Verified)

- The Bun worker runs extension code against Tezbar's own React shim
  (`src/main/extension-runner.ts:1093,1124`, `createReactShim`). It returns
  `ExtensionRunCommandResult` with `{ sessionId, generation, refreshMs, title, root:
  ExtensionRuntimeNode, actions: ExtensionRuntimeAction[], effects }`. Callbacks travel as
  `extension:invoke-action`, `:search-text-changed`, `:refresh-session`, `:load-more` and
  `:dispose-session`. Invalidations are pushed as `extension:session-invalidated`.
- Node types emitted: `List`, `List.Section`, `List.Item`, `List.Item.Detail(.Metadata.*)`,
  `List.EmptyView`, `List.Dropdown(.Section/.Item)`, `Grid`, `Grid.Section`, `Grid.Item`,
  `Grid.EmptyView`, `Detail`, `Detail.Metadata.{Label,Link,TagList(.Item),Separator}`, `Form`,
  `Form.{TextField,TextArea,PasswordField,Checkbox,DatePicker,Dropdown(.Section/.Item),TagPicker(.Item),FilePicker,Description,Separator}`,
  `ActionPanel(.Section)`, `Action(.CopyToClipboard/.OpenInBrowser/.Paste/.Push/.Pop/.ShowInFinder/.SubmitForm)`,
  `MenuBarExtra(.Item/.Section/.Submenu/.Separator)`, `Tezbar.PreferenceSetup`. Full list in
  ledger §E.
- `BrowserWindow` in the worker is a stub (`src/main/desktop-runtime.ts:584`). It creates no
  window. The only HTML the worker produces is the OAuth loopback reply page shown in the
  system browser (`extension-runner.ts:2847,2918`).
- **Conclusion:** there's nothing irreducible here. Every reachable extension UI is a
  finite node vocabulary, and each kind gets a native renderer (§4.4).
- One related HTML path: `ui/Markdown.tsx` renders markdown (Detail, List detail, AI
  answers) through `marked` and DOMPurify, so **sanitized raw HTML inside markdown**
  renders today. The native policy is in §5.7 (Decision D-MD).

### 1.5 Renderer-owned persistent state (Verified)

There are 15 `localStorage` keys, and they hold real user data that only the WebView can
read. `tezbar:pinned-commands` (up to 12 pins with ⌥-slot order),
`tezbar:recent-extension-commands`, `tezbar:launcher-query-history:v1`,
`tezbar:home-search-results:v2`, `tezbar:search-candidates:v2`, `tezbar:terminal-defaults`,
`tezbar:terminal-pinned-sessions`, `tezbar:recent-emojis`, `tezbar:currency:prefs:v1`,
`tezbar:theme`, `tezbar:release-notes-version`, `tezbar:last-update-check`,
`tezbar:auto-update-enabled`, `tezbar:settings-tab`. Migration is in §6.3.

### 1.6 Measured prototype facts (not production guarantees)

There were 20 valid matched background-idle runs at artifact `7434ba1`, compiler 1.98.1.
Median footprint A/B: hidden 141.27/58.86 MiB, visible 244.35/67.69, retained 1262.22/65.81.
The strict CPU gate **failed** because of the core's periodic 10 s refresh. The service
graphs differ (tray, hotkey and updater ran in A only; A retained icons while B released
them). The futures crate also differs. Causality isn't isolated, and the root cause of the
1.2 GiB retained A memory isn't known. **No savings figure carries over to the full app.**
§9.4 requires a fresh whole-app measurement after parity.

---

## 2. Parity inventory (summary; exhaustive rows in the ledger)

### 2.1 Coverage counts

| Area | Rows | Ledger |
| --- | ---: | --- |
| Windows, processes and OS integration (main launcher, Settings, terminal sessions panel, snap overlay, tray, dialogs, single instance, hotkeys, notifications, Dock, backend status, logging, paste) | 20 | §W |
| App surfaces routed by `App.tsx` (16 `Surface` values + Settings window + snap overlay + terminal sessions window) | 19 | §S |
| CommandBar input modes and states | 32 | §L |
| CommandBar result actions, built-in command IDs and native commands | 25 | §A |
| Keyboard map (launcher and global) | 31 | §K |
| Settings tabs, rows and subpanels; Onboarding | 31 + 7 | §ST, §O |
| AI chat, history, approvals, providers, OAuth, AI Extensions | 34 | §C |
| Extensions: store, settings, runtime nodes, prompts, menubar, preferences | 45 | §E |
| Terminal | 14 | §T |
| Utilities: clipboard, snippets, notes, vault, emoji, calculator, currency, colour, QR and transforms | 33 | §U |
| Knowledge, Indexing, Deep Search | 12 | §N |
| System monitor, permissions, background tasks, release notes, updater, recents and pins | 17 | §Y |
| Tauri commands (51) and bridge API members (206) mapped to owners | 51 + 206 | §H |
| Core → UI events | 31 | §I |
| Browser persistent keys (plus native layout row P-15) | 14 | §P |
| First-party TS business logic moving to Rust | 32 logic groups (plus Markdown and the notes editor as `tezbar-ui` components) | §B |
| Not reachable or pre-existing discrepancies | 16 | §X |
| Platforms | 6 | §Q |

### 2.2 Surface map (abridged)

| Surface | Entry points (Verified) | Native owner |
| --- | --- | --- |
| Command bar (`command`) | default; `window-shown`; Esc back from any surface | `apps/tezbar/src/surfaces/command_bar/` + `tezbar-ui-model::launcher` |
| Onboarding | `hasCompletedOnboarding == false` at boot; `start-onboarding` command; tray/shortcut `onboarding` surface | `surfaces/onboarding.rs` |
| AI chat (`ai-chat`) | Space prefix, double-space suffix, `/dir  task`, chat history rows; stays mounted after first visit | `surfaces/ai_chat/` + `ui-model::chat` |
| Settings (separate window, 8 tabs) | ⌘/Ctrl+, (opens AI tab), tray “Settings”, `open-settings*` commands, `app:open-surface settings-*` | `windows/settings.rs` + `surfaces/settings/*` |
| Permissions | Settings → Permissions → Open Permissions (inside the Settings window); in-launcher variant | `surfaces/permissions.rs` |
| Extensions store | `open-extensions`, Settings → Browse store, `open_extensions_window` | `surfaces/extensions_store.rs` |
| Extension runtime | `run-extension-command` (view), hotkey-run, AI task actions | `surfaces/extension_runtime/` |
| System monitor, Indexing, Clipboard, Snippets, Notes, Vault, Emoji, Terminal, Release notes | respective commands, native commands and tray surfaces (ledger §S) | one module each under `surfaces/` |
| Terminal sessions window | ⌘/Ctrl sessions toggle in Terminal | `windows/terminal_sessions.rs` |
| Snap overlay | launcher drag with snap | `windows/snap_overlay.rs` |

Timed return to the command bar (`extensionRuntimeTimeoutMs`, `aiModeTimeoutMs`,
`terminalModeTimeoutMs`, reset by pointer, key, input or wheel) is part of the surface
router (L-29).

### 2.3 Command bar (abridged; see ledger §L, §A, §K)

- **Modes**, with priority from `commandBarInputMode.ts`: terminal (`>` on an empty bar,
  sentinel kept on return), Deep Search (`!`), AI (`' '` prefix, trailing double space, or
  `/path  task`), slash path completion (`/`), application grid (`` ` ``, 7-column
  Launchpad grid), extension argument mode (inline chip and Tab between fields; the
  `extension:launcher-search` argument surface), chat history list, and normal search. Each
  mode has the two-step Escape (clear the query, then leave the mode) from `CommandBar.tsx:3520–3570`.
- **States**: core starting or failed (BackendConnection), empty home (pins rail, recents,
  unread release-notes row, home cache), typing (latest-wins at 8 ms, stale suppression,
  IME composition suppression), results, Deep Search loading and recommendation row,
  answer card (`query` → `stream-token`), action message card (password, copied, toggle,
  info; TTLs 9/7/4 s), vault copy feedback (Touch ID wait), update status row
  (checking, available, manualAvailable, downloading, ready, upToDate, error), pin
  picker, pinned-command tooltip, drag to reorder and unpin, terminal settings drawer,
  model menu, and activation in flight.
- **Instant rows** computed in the renderer today: calculator (`=` and expressions,
  `12% of 480`), currency (Frankfurter rates, prefs and synonyms), colour conversion, the
  Deep Search recommendation, and the release-notes row. All move to Rust (§2.4).
- **Actions** (`search:execute` types): `open-app`, `open-file`, `open-url`,
  `open-with-app`, `copy-text`, `copy-and-paste-text`, `add-note`, `run-extension-command`
  (view, no-view, arguments, preference setup), `invoke-command` (15 typed `TezbarCommandId` values plus
  `open-providers` and `quit-tezbar`; ledger A-09), `run-native-command` (43 descriptors per platform in `system/native-{macos,windows}.json`; 6 open surfaces, 1 quits),
  `run-shell`, `install-extension`, `open-vault-item`, Quick Look (Space), reveal, record
  usage, directory visit, `query` fallback (answer streaming / providers / extensions
  intents).

### 2.4 First-party TypeScript logic that must become Rust

Ledger §B has file paths and test mappings. Every ported module keeps or replaces each
existing test case with an equivalent Rust test named after the original. Deleting a test
without a mapped replacement fails review.

| Logic (current file) | New home |
| --- | --- |
| `calculator.ts` (uses `new Function`) | `tezbar-core::instant::calculator` (safe parser and evaluator, no eval) |
| `currency/*` (Fuse synonyms, parse, prefs), `hooks/useCurrencyConversion.ts`, `frankfurter.ts` | `tezbar-core::instant::currency` (rates fetched by the existing `currency:frankfurter-latest` handler) |
| `colorConverter.ts` | `tezbar-core::instant::color` |
| `shared/textTransform.ts`, `shared/textTransformers.ts` (52 tests), `qrImage.ts` | `tezbar-core::instant::transform`, `::qr` (`qrcode` crate, PNG via `image`) |
| `emoji/emojiData.ts`, `emojiSearch.ts` (Fuse) | `tezbar-core::instant::emoji` |
| `shared/searchMode.ts`, `searchQueryVariants.ts`, `applicationSearch.ts`, `optimisticSearch.ts`, `latestSearch.ts`, `commandBarInputMode.ts`, `launcherArgument.ts`, `escapeGate.ts`, `newItemShortcut.ts` | `tezbar-ui-model::launcher` (reusing core's keyboard-layout variants if they already exist in `launcher/text.rs`; check before porting, never duplicate) |
| `pinnedCommands.ts`, `launcherQueryHistory.ts`, recents, recent emojis, terminal pins and defaults, theme, currency prefs, release-notes version, update-check keys, settings tab | `tezbar-ui-model::ui_state` (one durable store, §6.3) |
| `releaseNotes.ts` (fetches the GitHub API in the renderer), auto-update scheduling in `App.tsx` | `tezbar-update::notes`, `tezbar-ui-model::update_policy` |
| `settingsLifecycle.ts` and the SettingsView provider-draft merge (revision and ownership rules; ledger §C) | `tezbar-ui-model::settings` |
| `useKnowledgeSettings.ts`, `KnowledgeEmbeddingSettings.tsx` state machine (72 regression tests) | `tezbar-ui-model::knowledge_settings` |
| `agentChat/{transport,streaming,ownership,model,historyCache}.ts`, `shared/chatMeta.ts`, `llmErrors.ts` | `tezbar-ui-model::chat` |
| `extensionCatalogState.ts`, `extensionRuntimeStatus.ts`, `extensionSessionRefresh.ts`, `extensionPreferences.ts`, `extensionImages.ts` (remote image fetch and cache), raycast-api runtimes (list filtering, form state and validation, grid, detail colours) | `tezbar-ui-model::extensions` + `tezbar-core::extensions::images` |
| `terminalSessionSelection/Label/History.ts`, `shared/terminal.ts` | `tezbar-ui-model::terminal` |
| `vaultDetect.ts`, `vaultCopy.ts`, `vaultIcons.tsx` mapping | `tezbar-ui-model::vault` |
| `windowDrag.ts`, `launcherViewport.ts`, `useLauncherScaleShortcuts.ts`, `shared/windowLayout.ts` | `tezbar-host::{placement, layout}` |
| `ui/Markdown.tsx` (marked, highlight.js, DOMPurify, streaming fence balancing) | `tezbar-ui::markdown` (§5.7) |
| `NotesView` rich-HTML editor (`execCommand`, allowed tags `div br p strong b em i u ul ol li`) | `tezbar-ui::rich_text` + `ui-model::notes` (same stored HTML subset, §5.7) |
| `shared/aiProviders.ts` provider catalog used by Settings | Read from core's `provider_defaults.json` through a new core channel. One source; the TS copy goes. |

---

## 3. Production architecture

### 3.1 Crates and dependency graph (Decision)

```
                    ┌──────────────────────────── apps/tezbar (bin "tezbar", edition 2024, Rust 1.98.1)
                    │   main.rs: supervisor/CLI dispatch → single instance → Tokio → GPUI
                    │   host_adapter.rs (impl tezbar_core::Host), core_client.rs (typed façade),
                    │   events.rs (typed decode), windows/*, surfaces/*
                    │
        ┌───────────┼───────────────┬────────────────────┬──────────────────┐
        ▼           ▼               ▼                    ▼                  ▼
  tezbar-ui     tezbar-ui-model  tezbar-host        tezbar-update      gpui, gpui_platform,
  (GPUI comps)  (pure state)     (OS adapters)      (feed/verify/      gpui_tokio @ c84af5b
   │  └─ gpui    │                │                   install)
   │             ▼                ▼                    │
   └──────► tezbar-core ◄─────────┴────────────────────┘   (edition 2021, rust-version 1.85)
              (was src-tauri/src/native_core + native_terminal + terminal_bridge)

  src-tauri (temporary until M11): tezbar-core + tezbar-host + tezbar-update via path;
  its TauriHost adapter implements tezbar_core::Host. Deleted at cutover.
  src/main (Bun worker, TypeScript): unchanged process contract, spawned by tezbar-core.
```

| Crate | Owns | Must not depend on |
| --- | --- | --- |
| `crates/tezbar-core` | Everything in `native_core` today, plus the PTY (`native_terminal` minus Tauri commands), `fts_rank`, `instant::*` (§2.4), `extensions::images`, and the `Host` port. Edition 2021, `rust-version = "1.85"`. | `tauri*`, `gpui*`, `wry`, `tao`, any windowing crate |
| `crates/tezbar-host` | `global-hotkey` and `tray-icon`/`muda` adapters, single instance, `native_input` (synth input, capture), notifications (from `timer_notifications.rs`), placement and snap math, per-monitor positions and layout profiles (from `lib.rs` and `window_layout.rs`), profile path resolution (§6.1), logging, `child_supervisor`, `windows_job`, `resource_directory`. Edition 2021, 1.85 where its deps allow. | `gpui*`, `tauri*` (except the temporary `tauri` feature for the migration adapter; removed at M11) |
| `crates/tezbar-update` | Channels (`release_channels.rs`), manual releases, feed parse, minisign verify (`minisign-verify 0.2.5`, already locked), download, staged install (macOS bundle swap, Windows NSIS launch), release-notes fetch, the `AppUpdateStatus` machine. | GUI crates |
| `crates/tezbar-ui-model` | All presentation state and logic without GPUI types: controllers that take input and return `(State, Vec<Effect>)` (§3.3). | `gpui*` |
| `crates/tezbar-ui` | GPUI components and tokens (§5). | `tezbar-core` internals (it consumes ui-model view types only) |
| `apps/tezbar` | Bootstrap, windows, surfaces, host adapter, typed core client, effect executor. | — |

Deletion test: removing `tezbar-ui-model` would push its logic back into GPUI views, where
it can't be tested headlessly. Removing `tezbar-host` would duplicate OS code between the
temporary Tauri adapter and the GPUI app. Both crates earn their keep. `tezbar-ui` is
separate from the app so the GPUI component work compiles and tests on its own, and so it
can be owned by a lane without touching surfaces.

### 3.2 The `Host` port (Decision)

`CoreContext.app: Option<tauri::AppHandle>` is replaced by `host: Arc<dyn Host>`. That
gives three real adapters: `TauriHost` (temporary), `GpuiHost`, and `TestHost` (records
calls and returns scripted replies). The port is small. Each method hides one OS concern.

```rust
// crates/tezbar-core/src/host.rs
pub trait Host: Send + Sync + 'static {
    /// Fire-and-forget UI event; same channel names and JSON payloads as today.
    fn emit(&self, channel: &str, payload: serde_json::Value);
    /// Window/surface operations formerly done by crate-root functions.
    fn surface(&self, op: SurfaceOp) -> BoxFuture<'static, Result<(), String>>;
    /// Modal OS interaction: confirm, choose folder, save file.
    fn dialog(&self, req: DialogRequest) -> BoxFuture<'static, Result<DialogReply, String>>;
    /// System-wide shortcuts; register-then-release semantics stay in PlatformCore.
    fn shortcuts(&self) -> &dyn ShortcutRegistry;
    /// Extension MenuBarExtra / status items (scoped ids, ≤128 per process, ≤128 items).
    fn status_items(&self) -> &dyn StatusItems;
    /// PNG of the active display, with Tezbar's own windows excluded or hidden.
    fn capture_active_display(&self) -> BoxFuture<'static, Result<Vec<u8>, String>>;
    /// True when OS desktop services (scan, watch, clipboard observer) may run.
    fn desktop_services(&self) -> bool;
    /// OS notification (extension timers, `host.notifications.*`). Default: no-op for headless use.
    fn notify(&self, _spec: NotificationSpec) {}
}
pub enum SurfaceOp {
    ShowLauncher, HideLauncher, ToggleLauncher, CloseCurrent,
    SetAppVisible(bool),            // set_backend_app_visibility semantics (remember/restore)
    SuppressBlurHide(bool),
    OpenSurface(Surface),           // validated list from platform.rs:303–320
    OpenSettings(Option<SettingsTab>),
    HideForPaste,                   // hide windows + NSApp hide (macOS) before synthetic ⌘V
    ShowAfterAuth,                  // vault unlock / reset return
    SnapDragStart, SnapDragEnd,
    Quit,                           // bounded shutdown path (§3.5)
}
```

- `platform.rs` keeps its channels. Each `crate::…` call becomes `self.ctx.host().surface(…)`,
  or `dialog`, `shortcuts` or `capture_active_display`.
- `core:platform:clear-webview-cache` keeps its channel. Under `GpuiHost` it deletes the
  **legacy** WebKit/WebView2 profile directory, and only after §6.3 import has succeeded.
  Under `TauriHost` it keeps today's behaviour (Decision; ledger ST-31).
- The PTY moves into `tezbar-core::terminal`. Today's Tauri commands
  `native_terminal_{create,attach,detach,write,resize,cwd,kill,delete_history,prune_history}`
  become core channels that the renderer couldn't reach before:
  `core:terminal:{create,attach,…}` for the GPUI client, plus the existing `terminal:*` ones.
  Output streams through `emit("terminal:data")`, as today. `block_on` inside
  `terminal_bridge.rs:64` is replaced by an awaited future.
- `knowledge:choose-folder` uses `dialog(ChooseFolder)`. `tauri::is_dev()` in
  `runtime_bundle.rs` becomes a `CorePaths.dev_mode` flag.
- `process_host::set_tray` uses `status_items().set(scoped_id, spec, callback)`. The spec
  (`title`, `tooltip`, PNG icon path ≤512 px, items with id/title/enabled/checked) and the
  limits stay byte-identical.
- `extension_worker.rs:138` (`locate_bun`) moves into core. `timer_notifications` becomes
  `tezbar-host::notifications`, reached through `Host::notify`, the only method with a
  default (a no-op, so headless tests stay unchanged).

Typed events and requests: the JSON channel interface stays the core's stable seam, so the
existing 620 Rust tests and `fixtures/legacy_channels.json` keep working unchanged. The app
gets **one** typed façade, `core_client.rs`, with serde types ported from `src/shared/*.ts`,
and one decoder, `events.rs`, from channel+JSON to `CoreEvent`. No surface calls
`request("…")` with a string literal (lint: `rg 'request\("' apps/tezbar/src/surfaces` = 0).

### 3.3 UI-model controllers (Decision)

Each feature controller is a deep module: a few methods, a lot of behaviour, and no side
effects.

```rust
pub trait Controller {
    type Input;   // user intent, core reply, timer tick, window event
    type Effect;  // CoreRequest{id, channel, args}, ScheduleTimer, Clipboard, OpenUrl, Navigate…
    fn update(&mut self, input: Self::Input, now: Instant) -> Vec<Self::Effect>;
}
```

GPUI views render from `controller.state()` and forward inputs. The app's effect executor
runs effects on Tokio and feeds results back as inputs carrying their request generation.
Stale generations get dropped inside the controller. That's the prototype's
`search_session.rs` rule, applied to every surface. Tests drive controllers with scripted
inputs. No window and no core needed.

### 3.4 Entrypoint and process ownership (Decision)

`apps/tezbar/src/main.rs`, in this order:

1. `#![cfg_attr(windows, windows_subsystem = "windows")]`.
2. `tezbar_host::child_supervisor::dispatch()`. Same flags
   (`--tezbar-private-child-supervisor-v1`, `--tezbar-private-extension-supervisor-v1`),
   same behaviour, and it exits before anything else.
3. `tezbar_core::native_v2_cli::dispatch()`.
4. Single instance (`tezbar-host::single_instance`): a per-user, per-identity lock (macOS:
   `flock` on `~/Library/Application Support/com.tezbar.app/.instance.lock` plus a
   `0600` Unix socket in the same directory; Windows: named mutex `Local\com.tezbar.app`
   plus a named pipe with an owner-only DACL, reusing the prototype's SID-bound pipe code). A
   second launch sends `activate` and exits. The first instance runs the
   `restore_main_window` semantics (focus Settings if it's visible, otherwise place and show the
   launcher).
5. Logging to the same directory and file name the Tauri log plugin uses (Open O-LOG:
   confirm the exact path from `tauri-plugin-log` at the locked version; 5 MB rotation, Info).
6. Build **one** multi-threaded Tokio runtime and own it in a GPUI global (prototype
   `RuntimeOwner`). `gpui_tokio::init_from_handle`.
7. `Application::new().with_assets(..).run(|cx| …)`: build `CorePaths` (§6.1),
   `CoreContext::new(paths, GpuiHost)`, `NativeCore::new`, and spawn `core.start()` on Tokio.
   Open the hidden launcher window, register the tray and launcher hotkey, then
   `restore_main_window` (today's `RunEvent::Ready`). Start the updater check at +3 s
   (§7.4).
8. `cx.on_app_quit`: `quit_app_now` semantics. Unregister shortcuts, `terminal::shutdown_all`,
   `core.shutdown()` bounded at **8 s** (as today), flush window positions, exit.
   Quit confirmation stays core-owned (`app:confirm-quit` → `dialog(Confirm)`).

Tokio ownership rules: views never block the main thread. Every core call goes through
`gpui_tokio::Tokio::spawn`, and its `Task` is held by the owning view or controller
executor, so dropping it cancels the await (the core work itself finishes or is cancelled by
its own owner). The child processes that keep role-specific ownership are the extension
worker (Bun, lazily started, stays alive until shutdown), MCP children, native-v2
extension processes, AI shell and tool processes (Job Objects on Windows), the PTY, and
Knowledge extract/embedding runtimes. They stay core-owned with their existing bounds. GPUI
code never spawns processes.

### 3.5 Windows in GPUI (Decision)

| Window | GPUI options | Behaviour preserved (ledger §W) |
| --- | --- | --- |
| Launcher (`main`) | `WindowKind::PopUp`, no titlebar, transparent, `Blurred` on macOS with sheet alpha 0.95; opaque sheet RGB on Windows (Mica only if visual QA confirms an identical look); `focus: true` on show; not shown in taskbar/Cmd-Tab as a separate window. Default 760×640, min 480×360, per-monitor size and zoom 0.75–2.0. | Placement on the active monitor (frontmost-window monitor, then cursor monitor), per-monitor persisted position, clamp to work area, mixed DPI, blur-hide with the 120 ms grace and suppress flag, Esc hides only from `command`, show resets nothing (`resetUi:false`), 5 s idle trim after hide, edge/corner resize handles that track the physical mouse, snap to centre/edges with dwell (130 ms, thresholds 32 px, slow speed 420 px/s), interface zoom ⌘/Ctrl +/−/0 (terminal: text zoom, Alt+ for the whole UI). |
| Settings | `WindowKind::Normal`, system titlebar, 920×680, resizable, title “Tezbar Settings”. | Opening hides the launcher. Focusing Settings hides the launcher. A second instance focuses Settings when it's visible. Close handshake: drain the Settings write queue, then close; on failure show the inline alert with **Retry saved changes**; 3 s fallback only when no owner acknowledged (§4.2). |
| Terminal sessions | `WindowKind::Floating` with parent = launcher, 300 px wide, left overhang 266, top offset 96, bottom 86, min height 260, transparent, no shadow. | Follows launcher move/resize. Hides with the launcher on blur unless focus moved to the panel (80 ms grace). Actions route to the launcher's Terminal surface. |
| Snap overlay | `WindowKind::PopUp`, transparent, non-focusable, sized to the monitor work area; mouse passthrough through a raw handle (`NSWindow setIgnoresMouseEvents:YES`; `WS_EX_TRANSPARENT \| WS_EX_LAYERED \| WS_EX_NOACTIVATE`). | Guide lines and centre indicator (`snap-overlay` CSS), shown only during a drag. |
| Native dialogs | `Window::prompt`, `prompt_for_paths`, `prompt_for_new_path`. | Confirm (quit, extension `host.dialog.confirm`, safety), Knowledge folder, QR PNG save. Blur-hide is suppressed while they're open. |

macOS activation policy stays **Regular** (Dock icon), matching today (§1.3). `on_reopen`
(a Dock click) runs `restore_main_window`. Tray: template icon `trayIconTemplate@2x.png`,
left-click up toggles the launcher, right-click opens a menu with “Show Tezbar”,
“Settings” and “Quit Tezbar” (same ids and behaviour as `lib.rs:3158–3251`).

### 3.6 Toolchain, workspace, lockfile, GPUI pin (Decision)

- Root `Cargo.toml`: a virtual workspace with `members = ["crates/*", "apps/tezbar"]` and
  `exclude = ["src-tauri", "prototypes/native-launcher", "native/input"]`. Root
  `rust-toolchain.toml`: `channel = "1.98.1"`, `profile = "minimal"`. rustup resolves the
  nearest toolchain file, so this also moves the temporary Tauri build to 1.98.1 (the matched
  A build already used 1.98.1). MSRV gates call `+1.85.0` explicitly.
- `Cargo.lock` at the root is **seeded by copying `src-tauri/Cargo.lock`**. GPUI is added
  without `cargo update`, using the prototype's targeted resolution (Linux-only
  `proc-macro-crate = "=2.0.0"`, `[patch.crates-io] async-task` = Zed rev `015845a…`). The
  prototype's `lock-drift.py` is promoted to `scripts/native/lock-drift.py` and runs in M0
  and before every measurement. Core watchlist versions must not move.
- GPUI deps in `[workspace.dependencies]`, all at rev `c84af5b99cbc366d675064e52b23664a015b6e35`:
  `gpui`, `gpui_platform` (default-features off; `font-kit`, plus `runtime_shaders`
  behind the app feature `runtime-shaders`), and `gpui_tokio`. **No framework update and no
  fork** unless a blocker is recorded with its exact error.
- MSRV: `tezbar-core`, `tezbar-ui-model`, `tezbar-update` and `tezbar-host` declare
  `rust-version = "1.85"` and edition 2021. The gate runs while `src-tauri` exists:
  `cargo +1.85.0 check --locked --manifest-path src-tauri/Cargo.toml --lib` covers the core
  through src-tauri's own lock domain. After M11 a dedicated `tools/msrv-core/` package with
  its own lock (seeded from the last src-tauri lock) runs `cargo +1.85.0 check -p tezbar-core`.
  If a future dependency needs a newer compiler, the gate is retired with a recorded reason
  (“where feasible”). It's never silently skipped.
- Shaders. macOS dev and local-preview builds use `runtime_shaders` (no Xcode needed). For
  production see **User decision U2** (§10.1). Windows release needs `fxc.exe` (present in
  the task cache per brief; re-verify with `where fxc` or the `GPUI_FXC_PATH` check at M0).
  Debug builds compile shaders at runtime.
- Windows manifest. GPUI's default `windows-manifest` feature embeds a manifest.
  `apps/tezbar/build.rs` must not add a second Common-Controls dependency. Open O-MAN:
  inspect GPUI's manifest at the pin and the Tauri one, and keep exactly one manifest that
  includes DPI awareness and Common-Controls 6 if `Window::prompt` needs it.
- Dev loop: `cargo run -p tezbar --features runtime-shaders -- --profile <isolated>` (§6.4).
  `pnpm build:backend` still builds the worker. A new `pnpm dev:native` script wraps both.
  Root `package.json` keeps TS typecheck and test scripts for `src/main`, `src/shared` and
  `scripts`.

### 3.7 Promoting the prototype (Decision)

| Prototype file | Becomes | Note |
| --- | --- | --- |
| `text_input.rs` | `tezbar-ui::text_input` | Extend with multi-line mode, undo/redo, password masking and AccessKit text selection. Single source for every field. |
| `search_session.rs` | `tezbar-ui-model::launcher::session` | Generations, 8 ms coalescing, held-Enter ticket, exactly-once activation. Keep its 6 tests verbatim. |
| `theme.rs` | `tezbar-ui::tokens` | Extend to the full token set (§5.2). |
| `launcher_view.rs` icon LRU and BGRA conversion, `drop_image` on hide, replace and evict | `tezbar-ui::image_cache` | Bounded LRU (128), GPU texture release on every eviction path (F1/F2 fix). |
| `bench_control.rs` | `apps/tezbar/src/bench_control.rs` behind feature `bench-control` | Token-authenticated, never in release packaging. |
| `scripts/{fixture,matched,measure,bundle-macos.sh,windows-*.ps1,lock-drift}.py` | `scripts/native/…` | Measurement harness for §9.4. |
| `launcher_facade.rs` | **Deleted** at M1 | The GPUI app uses `tezbar-core` directly. Its isolation checks move to `apps/tezbar --profile` (§6.4). |

The prototype package itself is removed at M11 after its evidence is archived (evidence
stays ignored; reports stay in `docs/performance`).

---

## 4. Feature state and lifecycle invariants

### 4.1 AI chat (from NATIVE_AI_SPEC, review fixes R1–R20, T-R1–T-R7, ownership tests)

These are invariants, each with a ported test (ledger §C):

1. **One live owner per conversation.** Each conversation has its own run, stream,
   approval and persistence. Output from conversation A never lands in B, even while B starts a
   run. Hidden runs keep going while another chat is open. Background approvals
   publish status (`BackgroundTaskStatus`) and navigate to their conversation.
2. **No repeated effects.** Every send, retry, approve, capture and new-chat carries a nonce
   or generation. Re-renders, surface switches and window show never resend. Launcher
   new-chat and resume nonces are honoured while the chat stays mounted. Unnonced launcher
   commands repeat through the router.
3. **Stop** persists partial text and terminal stage state, clears the active approval, and
   cancels only its own run. Late events after stop are ignored.
4. **Deletion tombstones.** Delete during send or retry preparation never starts a run. A
   `done` event that overtakes the delete reply can't resurrect the conversation. A failed
   delete recovers the conversation. Clear behaves the same way (native generations, R10).
5. **Retry** reuses the failed assistant message id, so history is updated, not duplicated.
   It keeps the original canonical images (R2, R18). Manual retry intent is explicit and
   prior tool outcomes are authoritative (R11).
6. **Thinking** shows only the “Thinking…” stage. Reasoning text is never rendered.
7. **Approvals**: the card title is `Allow {Extension} to use {tool}?`, the body is the
   formatted JSON arguments, and the actions are `Allow once`, `Deny`, `Always allow this
   tool`/`command` (only when persistence capability allows, R13). **Focus**: when a card
   appears, focus moves to `Allow once`. Enter activates only the focused button. Composer
   Enter never approves (R5). Mode picker changes leave destructive approvals pending
   (R4). `approval-expired` removes the card. The approval mode picker stays reachable during
   runs (R20).
8. **Streaming render** coalesces deltas to the frame rate. Follow output only while the
   reader is near the end. Stage order is execution order (`timeline.test.ts` rules).
9. Images: screen capture (⌘⇧S) is deduplicated. A capture that completes after navigation or
   hide is discarded. cwd and image stay with their owner if config preparation finishes after a
   new chat starts.
10. Composer `/` menu, prompt-argument row, agent profile chip, project-extension notice
    (once per folder per session) and diagnostics area, exactly as NATIVE_AI_SPEC §10.7.

### 4.2 Settings writes (from settingsLifecycle and the T-R2–T-R6 repairs)

`tezbar-ui-model::settings::WriteQueue`:

- One queue per Settings window. Each intent has a logical revision assigned at intent time.
  Retries keep their original revision. Queue order and intent order are separate.
- Owners register a drain and a retry. Retired owners (tab remounts) keep draining until
  success. `drain_all` lets newer owners flush even when an older draft failed, and a later
  successful edit of the same provider supersedes the older failure without replaying it.
- Provider maps merge by **owned IDs** on every non-removal write. A retry never
  resurrects a provider deleted later, never clears a newer live removal, and keeps newer
  credentials, definitions, model lists, activation (`@active`), protocol and token limit.
- Close: native close-request → `drain_all` (no deadline while acknowledged) → close. On
  failure keep the window and show `Couldn't save Settings. Retry the edit, or retry saved
  changes from a previous tab.` with **Retry saved changes**. Close when no owner
  acknowledges within 3 s (failed bootstrap only).
- Model/provider selection, hydration without writes, the discovery-write rejection that must
  not block close, and the protocol and token limit kept across rediscovery: every case in the
  first 12 tests of `SettingsView.knowledge.test.ts` becomes a ui-model test.
- `llm-config-changed` triggers re-fetch in other views. Settings never echoes its own write
  back into a dirty draft.

### 4.3 Knowledge settings

The 60 Knowledge cases in `SettingsView.knowledge.test.ts` define a state machine: snapshot
recovery, bounded hung loads, serial polling, visibility-paused polling, model variant
switches with a reconciliation lock, Cancel superseding hung operations, focus kept on a
button whose role changes (Download→Cancel, Retry start→Remove), single announcement of
failures, and progress below 100 % until done. Port it whole into
`ui-model::knowledge_settings` with one test per case. Focus rules map to GPUI focus handles
(§5.8).

### 4.4 Extension runtime bridge

**Rust-side ownership** (unchanged): project trust and review, secret reset, connection
tests, OAuth scopes and PKCE, MCP client lifecycle, and catalog acquisition all stay in
`tezbar-core::ai::ext` and `extensions`. The UI only displays and triggers flows. Every
`ai-extensions:*` channel (23, ledger §C) has a native control.

**Session model** (`ui-model::extensions::RuntimeSession`):

- Key: `(extension_id, command_name, session_id)`. Every reply carries `generation`, and a
  reply for an older generation is dropped. `extension:session-invalidated {sessionId,
  revision, generation}` triggers one refresh. Duplicate invalidations coalesce.
- Navigation stack: `Action.Push` produces a new root under the same session and `Pop` returns.
  `backActionId` on a root makes Back invoke that action instead of leaving. Esc pops the
  stack, then leaves the surface.
- Polling: `refreshMs` (null means event-driven; absent means V1 legacy polling) runs only while
  the surface is visible (`window-visibility`). Dispose on leave (`extension:dispose-session`).
- Search text: with `__hasServerSearch`, send `extension:search-text-changed` (latest-wins,
  no client filter). Otherwise filter on the client over title, subtitle and keywords, as
  `list-runtime.tsx:462` does.
- Selection: **Decision**: keep the selected item by stable `id` across tree updates, and
  fall back to the clamped index (today: clamped index only). This matches Raycast semantics
  and is a superset for unchanged trees. A test covers reorder, removal and pagination.
- Pagination: request `extension:load-more` when selection is within 3 rows of the end
  and `hasMore`.
- Action panel: ⌘K opens it. Sections, shortcuts (`modifiers` + `key`) and the primary
  action on Enter, secondary on ⌘Enter. Destructive style. The filter field inside the panel.
  Keyboard shortcuts dispatch from anywhere in the surface. Invocation shows
  `actionNotice {working, success, error}`.
- Forms: field values live in the controller. Validation errors from the tree (`error`
  prop) are shown per field. Submit sends `formValues`. Ported validation behaviour
  follows `form-runtime.test.ts` (12 cases). Dates, file pickers (`prompt_for_paths`, with
  blur-hide suppressed), tag pickers, dropdown sections, password masking and the native
  colour wheel (`nativeColorWheel` uses the existing `color-picker-helper`).
- Effects: `toast`, `hud` (footer and HUD), `clipboard`, `open`, `show-in-finder`. Script
  effects stay worker- and core-executed.
- `Tezbar.PreferenceSetup`: inline one-time setup form (required fields, password, dropdown,
  checkbox) → `preferences:set` → re-run.
- Icons: `extensions::images` resolves built-in `raycast-*` and Tezbar icons, files inside the
  extension's `assets`, and remote URLs (HTTPS only, ≤2 MiB, bounded LRU with byte budget,
  same eviction on hide as launcher icons), plus `tintColor`, theme variants and emoji glyphs.
- MenuBarExtra: `host.tray.set/remove` → `Host::status_items`. Item activation →
  `background_event`.
- Scope isolation: notices, polling, timers and image requests are keyed by session. Leaving a
  surface cancels them, and no callback from session X reaches session Y.

### 4.5 Launcher

The prototype rules (§3.2–3.4 of its spec) hold unchanged, extended to all modes: render is
pure. Activation is a ticket executed exactly once. Held Enter doesn't repeat. Enter waits
up to 1 s for the pending generation. IME: only committed text is searched (the IME regression
tests from `CommandBar.lifecycle.test.ts` are ported). Hide drops the pending query, and
reopen shows the latest home result with one authoritative refresh. `launcher:invalidated`
refreshes the latest query and ignores older in-flight replies.

---

## 5. Visual design assignment

The rule: **rebuild today's look natively**. The source of truth is `src/renderer/styles.css`
(the late `--ln-*` block at lines ~2084–2126 wins the cascade), `tailwind.config.ts`,
`ui/CommandSurface.tsx`, `ui/primitives.tsx` and each surface's classes. Capture reference
screenshots of the current app at M4 (light and dark, every surface and state in ledger §S/§L)
with the user's existing screenshot permission only, or ask the user to capture them.

### 5.1 Composition families

1. **Sheet surfaces** (`SHEET_SURFACES`: command, ai-chat, extensions, extension-runtime,
   system-monitor, indexing, permissions, clipboard, snippets, notes, vault, emoji-picker,
   terminal, release-notes). One edge-to-edge sheet: radius 18, `--ln-sheet`, 1 px rim
   inset. Header 44 px with a 1 px hairline. Floating 32 px footer with hint keycaps. No inner
   cards.
2. **List and detail split** (clipboard, snippets, notes, vault, indexing, AI history,
   extension list with detail). 36 px rows, radius 8, selected = `--ln-accent` fill with white
   text treatment, detail pane separated by a hairline.
3. **Settings window**: system titlebar. Tabs as the current segmented top row. Content max
   width 610 (850 for AI). Rows with label, detail and control. Hairline dividers. Footer hint
   bar (`Enter Save`, `Esc Close`).
4. **Onboarding**: step rail (Shortcut, Try the bar, AI model, DeepSearch, Permissions on
   macOS only), one task per step.
5. **Terminal**: sheet with a full-bleed terminal grid, sessions panel window, configure drawer.

### 5.2 Tokens (exact values; `tezbar-ui::tokens`)

Use the prototype spec §2.2 table for the `--ln-*` launcher tokens and text ramp, plus:

| Token | Dark | Light (`data-theme='white'`) |
| --- | --- | --- |
| `--ln-panel` | `rgba(255,255,255,0.025)` | `rgba(255,255,255,0.7)` |
| `--ln-field` | `rgba(0,0,0,0.22)` | `rgba(255,255,255,0.9)` |
| `--ln-popover` / solid | `rgba(32,33,38,0.98)` / `rgb(32,33,38)` | `rgba(255,255,255,0.98)` / `rgb(255,255,255)` |
| `--ln-selected` | `rgba(255,255,255,0.07)` | `rgba(15,23,42,0.06)` |
| `--ln-bar` | `rgba(0,0,0,0.16)` | `rgba(15,23,42,0.025)` |
| `--ln-accent-strong` / hover | `#5b5ef0` / `#6063e6` | same |
| `--c-shell`, `--c-panel` | `10 11 14`, `18 20 26` | `241 244 249`, `255 255 255` |
| `--c-success`, `--c-danger`, `--c-warning` | `52 211 153`, `248 113 113`, `251 191 36` | `4 120 87`, `220 38 38`, `180 83 9` |
| `--c-accent` / strong (non-launcher accents such as links and focus inside Settings) | `139 141 247` / `168 170 255` | `76 68 193` / `62 53 165` |
| Radii (the late `:root` override at `styles.css:2084–2090` wins the cascade) | window/sheet 18, panel 10, card 10, row 8, field 7, chip 5 | same |
| Spacing | 4/8/12/16/20/24 | same |
| Type | Geist when installed, else `.SystemUIFont`/Segoe UI; mono Geist Mono → SF Mono/Menlo/Consolas. Sizes in use: 15 input, 13.5/13/12.5/12/11/10.5/10. | same |
| Motion | fade-in 90 ms ease-out-expo for surface switch; scale-in 260 ms only on Settings AI panel open. All motion disabled under reduce-motion. | same |

AA correction (M4 closure): the palette above supersedes the legacy hover,
light success and warning values. Dark launcher text-3 is `#979ba7`, superseding
`#868a96` in the prototype table. These normal-text pairs otherwise failed §5.8;
see [the measured correction and regression](native-m04/closure-parent-repairs.md).
The live native token test is the authority for both themes and translucent fills.

Theme preference `white | dark | system` is a Tezbar setting (migrated from
`tezbar:theme`). `system` follows `window.appearance()` live. The setting applies to all
windows at once (today's `storage` event sync).

### 5.3 Shared components (`tezbar-ui`)

Text field (single line, multi-line, password, search badge variant), list (uniform 36 px,
virtualized, keyboard and pointer-move selection, section headers), grid (Launchpad 7-column,
extension grid with `columns`), split view, detail pane, metadata rows (label, link,
tag list, separator), markdown view, code block with language chip and Copy, footer hint bar
with keycaps, action message card, toast/HUD, button (primary uses `--ln-accent-strong`,
quiet, ghost, danger), toggle and checkbox, segmented control, dropdown and combobox popover,
model picker popover, approval mode picker, date picker, tag picker, file picker row, progress
(determinate and indeterminate, flat fill, no gradient), keycap, icon (SVG glyph set ported
from `CommandIconGlyph`/`launcherIcons.tsx`, app/file/extension images via `image_cache`),
empty, error and loading lines, Settings row, hotkey recorder, context menu (native `muda`
only for tray; in-window menus are popovers), terminal grid, rich-text note editor.

### 5.4 States (each needs light and dark reference screenshots)

Every list surface needs: loading (previous content kept, footer status after 150 ms, no
skeleton or shimmer), empty (one 13 px line with a next step), error (danger line with what
failed and how to retry), selected, pointer-over (selects on real pointer move only),
keyboard focus (caret, or a focus ring **only** for non-list controls: 1 px `--ln-accent-strong`
inset on fields, matching `.glass-field:focus`), disabled (text-4, no pointer), busy button
(label changes to the verb in progress, e.g. “Installing…”), destructive confirm. Per-surface
specifics are listed in ledger §S, column “States”.

### 5.5 Windows, scaling and input

- High DPI: logical sizes everywhere. Per-monitor scale changes re-layout without
  resizing the logical window. The interface zoom (0.75–2.0) multiplies rem-equivalent
  sizes, so it's independent of OS scaling.
- Multi-monitor: show on the active monitor (frontmost window, then cursor). Restore the
  per-monitor size, zoom and position. On hide, return focus to the previously active app
  (macOS: hide the app, not only the window; Windows: restore the previous foreground HWND).
- Keyboard: a full map per surface in ledger §K. Every in-app action has a menu-free shortcut
  as today. macOS uses ⌘, Windows uses Ctrl, except the documented AltGr/Alt cases (⌥1–0,-,= pin
  slots on macOS; Alt+digit on Windows).
- IME: marked text is underlined, candidate windows anchor at `bounds_for_range`, Enter and Esc
  go to the IME during composition, and only committed text drives searches or sends. This
  applies to every text field (launcher, chat composer, forms, notes, snippets, vault,
  settings).
- Selection, clipboard, undo and redo: all text fields support select-all, word and line
  movement, mouse drag, double-click word, copy, cut and paste, and undo/redo (⌘Z / ⇧⌘Z,
  Ctrl+Z / Ctrl+Y). That's an improvement over the prototype and a parity requirement against the
  WebView.

### 5.6 Prohibited patterns and legacy flattening

Rejected in review: decorative gradients, glass cards, colour stripes on any side of rows,
banners, cards or active nav items, default purple-blue accents beyond the existing tokens,
identical icon grids or badge grids, “NEW”/“BETA” chips, ALL-CAPS eyebrows, spinners or
skeletons in lists, generic copy, and emoji standing in for icons (extension emoji glyphs are
content, not chrome). Indigo appears only where the current tokens put it: selected row
`#4f52d9`, search badge and primary button `#5b5ef0`, and `--c-accent` text accents in
Settings.

The current app still has legacy decorations. They're **flattened**, not reproduced:
`.glass-shell`/`.glass-panel` gradients and backdrop blur in Settings (`styles.css:271–326,
564–660`), the Knowledge banner gradient (`SettingsView.tsx:2953`), the gradient progress fill
(`SettingsView.tsx:3268`), the Kbd glow on save (`SettingsView.tsx:3618`), and the coloured
`SystemMonitor` stat tile tints. They become flat `--ln-panel` surfaces, hairlines,
`--c-success` fill for progress, and a plain text label change for Saved. The pinned-row amber
side bar (prototype spec §2.8) becomes a pin glyph in the row's trailing slot. These are
the only visual deviations, and each is listed in ledger §X for the Opus review.

### 5.7 Rich content (Decision)

- **Markdown** (`tezbar-ui::markdown`): `pulldown-cmark` (MIT) with tables,
  strikethrough, task lists and footnotes, `breaks: true` (soft breaks become line breaks),
  bare-URL autolinks via the `linkify` crate, and syntax highlighting with `syntect`
  (MIT, `default-fancy` pure-Rust regex, bundled grammars covering highlight.js `common`).
  Highlighting runs only for settled content and blocks ≤12,000 chars (today's limit).
  Streaming balances an open fence exactly as `balanceStreamingFences` does and re-parses
  incrementally: the parse is cached up to the last closed block, and only the tail is
  re-parsed each frame. Code blocks have a language chip and a Copy button. Links open through
  `open_url` after a scheme check (http, https, mailto). Images: extension `assets` paths
  and HTTPS URLs through `image_cache` (`imageSrcResolver` parity). Text is selectable and
  copyable.
  **D-MD (raw HTML):** today DOMPurify renders sanitized raw HTML. Natively, a fixed subset
  is rendered: `br, p, div, span, b, strong, i, em, u, s, del, code, pre, kbd, sub, sup, a,
  img[src,width,height,alt], ul, ol, li, blockquote, h1–h6, hr, table, thead, tbody, tr,
  th, td, details/summary` (collapsible). `style` and `class` attributes are ignored. Other
  tags render their text content. A fixture corpus from installed-extension READMEs and
  Detail outputs (no user data) records which constructs change. This is a compatibility
  trade-off for the reviewer, not a user question, because Raycast's own Detail
  markdown doesn't promise raw HTML.
- **Notes rich text** (`tezbar-ui::rich_text`): a native editor over a document model
  limited to the stored subset (`div br p strong b em i u ul ol li`). It loads the existing
  stored strings (HTML or plain/markdown via today's `normalizeStoredRichText`) and saves the
  same sanitized HTML through `notes:update`, so the files stay compatible both ways. ⌘B/⌘I/⌘U
  toggles, list continuation on Enter, paste sanitization (`sanitizeRichHtml` rules), IME,
  undo/redo, and selection across runs.
- **Terminal** (`tezbar-ui::terminal_grid`): `alacritty_terminal` from crates.io
  (Apache-2.0; not Zed's fork) for VT parsing, grid, scrollback and selection, fed by the
  core PTY's `terminal:data`. Rendering: a GPUI element that paints glyph runs per row with
  cell metrics from the mono font, the cursor (bar, blinking, as xterm's `cursorStyle:'bar'`),
  selection, and an 8+8 ANSI palette for each theme (port `terminalTheme(isWhite)`).
  Scrollback: xterm is configured with 1,000,000 lines, so natively set a **bounded**
  100,000 lines (Decision: memory bound) plus the existing persisted history
  (`terminal-history`) for older output. Input: GPUI key events → xterm-compatible
  sequences (port the `attachCustomKeyEventHandler` rules at `TerminalView.tsx:364`, including
  app shortcuts ⌘K/⌘X/⌘P/⌘D, ⌘+/−/0 text zoom, Alt-modified whole-UI zoom), bracketed paste,
  mouse reporting when the program enables it, IME composition inserted at the cursor, URL
  detection and opening on ⌘-click, resize → `terminal:resize` (cols and rows from the cell
  size). Accessibility: expose the visible buffer as a read-only text region with the cursor
  line announced on change (§5.8).

### 5.8 Accessibility (what must be implemented)

GPUI at the pin provides the AccessKit plumbing (§1.2). Tezbar must provide, per component:

| Component | Role and properties | Actions |
| --- | --- | --- |
| Window | title (“Tezbar”, “Tezbar Settings”, “Terminal Sessions”), the snap overlay hidden from AX | — |
| Text field | `TextInput`/`SearchInput`/`PasswordInput`, label, placeholder, value (masked for passwords), synthetic text runs and selection per the `_accessibility.rs` guide, `aria_active_descendant` → selected option | focus, set value, set selection |
| Result list and grid | `ListBox`/`Grid` with `ListBoxOption`/`GridCell` children, `aria_selected`, `position_in_set`/`size_of_set` from the **full** row count, not the virtualized window | click (activate), focus |
| Buttons, toggles, segmented, dropdown | `Button`, `Switch`/`CheckBox` with `aria_toggled`, `RadioGroup`, `ComboBox` with `aria_expanded` | click, expand/collapse |
| Approval card | `Dialog`/`Alert` region named by the title; initial focus on Allow once | — |
| Status and footer | live region for status changes (“3 results”, “Copied”, errors). **Open O-LIVE**: AccessKit's `Live` property mapping at the pin isn't verified. If it's missing, announce through platform APIs (`NSAccessibilityPostNotification` announcement / UIA `RaiseNotificationEvent`) from `tezbar-host::a11y`. | — |
| Markdown and chat transcript | `Document`, headings with `aria_level`, links, code blocks as `Code` | open link |
| Terminal | `Terminal`/`Document` read-only text with the cursor line | — |

Acceptance needs manual VoiceOver runs (macOS) and Narrator runs (Windows interactive session)
over the scripts in ledger §K. Results are recorded, not assumed. Contrast: every text-on-fill
pair from §5.2 is checked against WCAG AA in a unit test (`tokens_contrast_test`). The known
low-contrast pair text-4 on sheet is acceptable only for placeholders and disabled text, as
today.

---

## 6. Security, data and platform continuity

### 6.1 Paths (Decision: byte-identical)

`tezbar-host::paths::profile()` reproduces Tauri's resolution exactly:

| Path | macOS | Windows | Override |
| --- | --- | --- | --- |
| data | `~/Library/Application Support/com.tezbar.app` | `%LOCALAPPDATA%\com.tezbar.app` | `TEZBAR_DATA_DIR` |
| config | parent of `openray_config_path()` (default `~/.openray`) | same | `TEZBAR_CONFIG_DIR` |
| cache (updater temp) | `~/Library/Caches/com.tezbar.app` | `%LOCALAPPDATA%\com.tezbar.app` (Open O-CACHE: confirm the Tauri v2 `app_cache_dir` on Windows) | — |
| resources | `Tezbar.app/Contents/Resources` | install dir beside `tezbar.exe` with the same relative layout | dev: repo root (existing `resource_directory::resolve`) |
| temp | `std::env::temp_dir()` | same | — |
| logs | Open O-LOG | Open O-LOG | — |

A unit test compares the new resolver with Tauri's `PathResolver` output, captured once from
the Tauri build under an isolated `HOME`-less test (the resolver functions are pure, so the
expectation table comes from the Tauri source at the locked version, not from the user's
machine).

### 6.2 Secrets, vault and permissions

- Vault keys, Keychain items (macOS Security framework, LAContext Touch ID) and DPAPI keys
  (Windows) are untouched. The same code moves with the core. Keychain access groups follow the
  designated requirement, so the same identity keeps access (§7.1).
- TCC grants (Accessibility, Input Monitoring, Screen Recording, Automation, Microphone,
  Calendars) are tied to `com.tezbar.app` and its designated requirement. They carry over
  only if signing continuity holds (§7.1). No new permission prompts or flows. The
  Permissions view only reflects and requests what `permissions:*` does today.
- Clipboard: the observer starts only with desktop services (`Host::desktop_services()`).
  Concealed vault copies clear after 30 s (`clear_if_sequence`), unchanged.
- No design or test step reads real secrets, profiles, Keychain items or the clipboard.

### 6.3 Migrating renderer-owned state (Decision)

The 14 `localStorage` keys (§1.5) move into `~/.openray/ui-state-v1.json` (atomic writes,
unknown keys preserved, versioned). On the first GPUI launch, `ui_state::import_legacy`:

1. Uses `ui-state-v1.json` if it exists.
2. Otherwise reads the **legacy WebView storage read-only**. macOS: the WebKit LocalStorage
   SQLite for origin `tauri://localhost` (`~/Library/WebKit/com.tezbar.app/WebsiteData/Default/*/*/LocalStorage/localstorage.sqlite3`,
   table `ItemTable(key TEXT, value BLOB UTF-16LE)`). Windows: WebView2 `EBWebView/Default/Local Storage/leveldb`
   for origin `http://tauri.localhost`, read with a pure-Rust LevelDB reader. Open O-LS: verify both
   locations, origin strings and encodings on a staged, disposable Tauri profile.
3. Parses each key with the same validation the TS code uses (pin normalization, max 12, slot
   order). Invalid values fall back to defaults and are logged, never fatal.
4. Never deletes legacy storage. Deletion happens only from Storage → Clear WebView cache,
   after a successful import is recorded.

The caches (`home-search-results`, `search-candidates`) aren't imported. They're rebuilt.

### 6.4 Isolated previews

- `apps/tezbar --profile <root>` (debug and `local-preview` builds only) applies the prototype
  `PrototypeProfile` checks: an absolute, non-symlink child of the OS temp dir, a marker file,
  and derived `data/config/home/temp`. It sets the paths explicitly, never through `HOME` or
  `CODEX_HOME`, and disables the global launcher hotkey and tray unless
  `--allow-desktop-integration` is given.
- Preview bundles use identifier `com.tezbar.preview.native`, are ad-hoc signed only under
  `--local-preview`, are staged in OS temp, and are never `com.tezbar.app`, never installed and
  never re-signing an existing app (`docs/MACOS_SIGNING.md`). The installed Tezbar is untouched.

### 6.5 Other platforms

Shipped targets stay macOS arm64 and Windows x64. Inventory: macOS x86_64 (the updater has a
`darwin-x86_64` path, but no artifact is shipped and Intel GPUI isn't built or verified),
Windows arm64 (WINDOWS_SUPPORT §6 lists it as a goal, but there's no build), Linux (core code
has `cfg(unix)` fallbacks; GPUI supports X11/Wayland; tray, hotkey and packaging are missing),
web/mobile (out of scope). No work is planned for any of them. The crate split doesn't block
them.

---

## 7. Packaging, update and build continuity

### 7.1 macOS bundle (Decision)

- `scripts/native/package-macos.mjs` assembles `Tezbar.app` from `apps/tezbar` and
  `packaging/macos/Info.plist.in`. The template carries the keys from §1.3, merges the
  existing `macos-permissions.plist`, takes `CFBundleExecutable` = `tezbar`, and adds no
  `LSUIElement`. The resource layout is identical to the Tauri bundle (§1.3).
- `scripts/build-release.mjs` gains `--app native` (the default after cutover) and keeps
  every guard: identity and channel check (moved from `src-tauri/build.rs` into
  `apps/tezbar/build.rs`, reading `packaging/app-identity.json`), the embedding payload hash,
  signing of staged native resources and helpers with the pinned certificate, the helper
  Team ID check, `codesign --verify --deep --strict`, designated requirement equality, ad-hoc
  rejection outside `--local-preview`, and a receipt with the observed identity.
- Updater artifact: `Tezbar.app.tar.gz` plus a minisign signature made with the existing
  private key, in the release pipeline only. The name check in `release-policy.mjs:135–136` stays.

### 7.2 Windows installer (Decision)

`packaging/windows/installer.nsi` is derived from the NSIS template of the locked
`tauri-bundler` (MIT/Apache-2.0; attribution kept), with WebView2 bootstrap removed. It keeps:
per-user install to the same default directory and registry/uninstall keys as the Tauri
installer, `allowDowngrades = false`, product name `Tezbar`, executable `tezbar.exe`, Start Menu
shortcut, and support for `/P`, `/S`, `/UPDATE` and `/ARGS` with relaunch. Open O-NSIS:
diff the generated installer's install directory and registry keys against a Tauri-built
installer before the first release. Code signing follows the existing Windows policy (no
change here).

### 7.3 First update from Tauri to GPUI (Decision)

The existing installed apps update with **their own Tauri updater**, so the first GPUI release
must be:

- macOS: an `.app.tar.gz` of `Tezbar.app` with the same name, identifier and designated
  requirement, signed with the existing minisign key. The Tauri updater replaces the bundle in
  place and relaunches `Contents/MacOS/tezbar`.
- Windows: an NSIS `…-setup.exe` that honours `/P`/`/S` + `/UPDATE` + `/ARGS` (verified from
  `tauri-plugin-updater 2.10.1`), installs over the Tauri install, and relaunches.
- The same `latest.json` schema and platform keys (`darwin-aarch64`, `windows-x86_64`),
  published on both platforms together from one commit (dual-platform rule). Feed and
  `channel-*` updates happen in one continuous sequence (the manualAvailable rule in AGENTS.md).

A pre-release gate (M10) proves both paths on disposable staged installs. Releasing is **not**
authorized by this spec.

### 7.4 Native updater (Decision)

`tezbar-update` implements `AppUpdateStatus` with the same states and the same events
(`update-status`): channels and endpoints from `release_channels.rs`, the manual-release
detection and download-URL validation in `updater.rs:119–155`, `validate_manifest_identity`, a
150 ms progress throttle, and `updater-temp` preparation. Install: macOS extracts into a
sibling temp dir, verifies the signature **before** extraction, then atomically swaps the
bundle, keeping the old one until relaunch succeeds. Windows writes the setup exe to temp and
launches it with `/P /R /UPDATE /ARGS`. Then it quits. Policy: startup check at +3 s (host), plus the
former renderer schedule (+5 s, then every 12 h) with auto-install only when
`autoUpdateEnabled` (moved from localStorage), both owned by `ui-model::update_policy`. No implementation detail leaks into the UI copy.
The UI says “Downloading update…”, “Restart to update”, “Download installer”.

### 7.5 Builds

Mac: local only. Windows: `User@100.77.254.64` over Tailscale. Use T3 environment link tools
first and SSH as the fallback. Build from a SHA-256-verified `git archive` into a task temp
staging directory. **Never** touch `C:\Users\User\Desktop\Tezbar`. No GitHub Actions.
Prerequisites are checked, not assumed (`rustup toolchain list`, `where fxc`, `where rc`, Bun
version, Node and pnpm). Results go into milestone evidence.

---

## 8. Milestones and file ownership

Each milestone ends with: tests green, the ledger rows it owns updated with evidence, and an
Opus critical review round for UI milestones (CLAUDE.md design flow). Lanes in the same
milestone have disjoint file ownership and can run in separate worktrees, alternating between
Codex and Pi as the coordinator chooses.

| M | Content | Owns (files) | Exit |
| --- | --- | --- | --- |
| **M0** | Workspace skeleton: root `Cargo.toml`, `rust-toolchain.toml`, seeded root `Cargo.lock`, empty crates, `apps/tezbar` that opens an empty hidden PopUp window, `lock-drift` | root manifests, `crates/*/Cargo.toml`, `apps/tezbar/{Cargo.toml,build.rs,src/main.rs}`, `scripts/native/lock-drift.py` | Mac release build (runtime shaders), Windows debug build; lock-drift has no watchlist movement; `src-tauri/Cargo.lock` unchanged |
| **M1** | Core extraction: `git mv src-tauri/src/native_core crates/tezbar-core/src`, plus `native_terminal`, `terminal_bridge`, `fts_rank*`, `child_supervisor`, `windows_job`, `resource_directory`, `native_v2_cli`; `Host` port; `TauriHost` in src-tauri; delete `launcher_facade` | `crates/tezbar-core/**`, `src-tauri/src/{lib.rs,tauri_host.rs}` | All 620 Rust tests pass in the new crate; src-tauri builds and passes `pnpm typecheck`, `pnpm test` and the extension worker fixture; 1.85 check; `rg "tauri" crates/tezbar-core` = 0 |
| **M2** | `tezbar-host` (placement, positions, snap math, layout profiles, native_input, capture, hotkeys, status items, notifications, single instance, paths, logging) and `tezbar-update`; src-tauri consumes them | `crates/tezbar-host/**`, `crates/tezbar-update/**`, `src-tauri/src/{lib.rs,window_layout.rs,updater.rs}` thinned | Existing lib.rs placement/position tests move and pass; updater tests (`updater.rs` cfg(test) + `src/shared/updater.test.ts` 28 cases) ported; Tauri app behaviour unchanged |
| **M3** | `tezbar-ui-model` + `tezbar-core::instant`: port ledger §B logic with tests (parallel lanes: launcher+instant / chat+settings+knowledge_settings / extensions+terminal+vault+ui_state) | `crates/tezbar-ui-model/**`, `crates/tezbar-core/src/instant/**` | Every mapped TS test has a Rust twin (§9.2); TS originals still pass (renderer not yet removed) |
| **M4** | `tezbar-ui` foundation and launcher window: tokens, text input (promoted), list and grid, popover, keycaps, buttons, image cache, markdown, AccessKit helpers; `GpuiHost`; launcher placement, blur-hide, resize, zoom, snap overlay; tray; hotkey; single instance; theme; `--profile` isolation; reference screenshots of the current app | `crates/tezbar-ui/**`, `apps/tezbar/src/{app.rs,host_adapter.rs,core_client.rs,events.rs,windows/**}` | W-rows green; Opus design review round on tokens, components and launcher shell |
| **M5** | Command bar full parity (ledger §L, §A, §K) | `apps/tezbar/src/surfaces/command_bar/**` | L/A/K rows green incl. IME, pins drag, app grid, path completion, Deep Search, AI/terminal sentinels, update row, answer card |
| **M6** | Utilities: clipboard (+convert, QR), snippets, notes (rich text), vault, emoji, system monitor, indexing, permissions, release notes, background tasks | `surfaces/{clipboard,snippets,notes,vault,emoji,system_monitor,indexing,permissions,release_notes}.rs`, `tezbar-ui::rich_text` | U/N/Y rows green; Opus review |
| **M7** | Settings window (8 tabs incl. AI providers, AI Extensions, voice, Knowledge, storage, advanced, extensions settings) and Onboarding | `windows/settings.rs`, `surfaces/settings/**`, `surfaces/onboarding.rs` | ST/O rows green; close handshake and write-queue tests |
| **M8** | AI chat | `surfaces/ai_chat/**` | C rows green incl. ownership tests and approval focus |
| **M9** | Extensions: store, settings, runtime renderer (all node kinds), preferences, MenuBarExtra status items, real Bun SDK fixtures | `surfaces/{extensions_store,extension_runtime}/**`, `tests/fixtures/extensions/**` | E rows green; fixture matrix §9.3 |
| **M10** | Terminal (alacritty_terminal grid, sessions panel window) | `tezbar-ui::terminal_grid`, `surfaces/terminal/**`, `windows/terminal_sessions.rs` | T rows green |
| **M11** | Packaging, updater, migration and cutover: macOS bundle and signing pipeline, NSIS, Tauri→GPUI update rehearsal, ui-state import, product default entrypoint, then removal of `src-tauri`, `src/renderer`, renderer deps (React, Vite, Tailwind, `@tauri-apps/*`, marked, highlight.js, dompurify, xterm, `ai`, `@ai-sdk/react`), `prototypes/native-launcher`, `index.html`/vite configs; `src/shared` pruned to worker/scripts imports | `packaging/**`, `scripts/native/**`, `scripts/build-release.mjs`, deletions | Definition of done (§9.1); full Opus correctness and design review with repairs |

Default entrypoint and scripts after M11: `pnpm build` → `node scripts/build-release.mjs
--app native`; `pnpm dev` → `cargo run -p tezbar --features runtime-shaders` with the worker
built first. Before M11 the default product stays the Tauri app. The GPUI app is opt-in
(`pnpm dev:native`), so users never get a half-migrated build.

---

## 9. Definition of done and validation matrix

### 9.1 Done means all of these

1. Every ledger row is `native`, `host`, `worker`, or `not-reachable` with proof. No
   “unsupported” placeholders, no hidden feature flags turning features off.
2. The shipped binary links no WebView: macOS `otool -L tezbar` shows no `WebKit.framework`,
   Windows imports no `WebView2Loader`. `cargo tree -p tezbar` has no `tauri`, `wry` or `tao`.
   `rg -n "WebView|wry|tauri" apps crates packaging` returns only migration notes. No
   first-party React/WebView fallback exists.
3. The ordinary product launch (`Tezbar.app` / `tezbar.exe`) is GPUI. Bun starts only for
   extensions, exactly as today.
4. Data, identity and signing: the same profile paths and the same core store formats (no
   core store format change in M0–M11, so a downgrade to the last Tauri release still reads
   them; only `ui-state-v1.json` is new, and a downgraded Tauri app falls back to its old
   `localStorage` values), the same designated requirement, helper Team IDs and updater pubkey.
5. All tests in §9.2–9.3 are green on Mac, with the Windows subset on the Windows host. The
   manual matrices in §9.5 are recorded.
6. A full Opus 5.5 design and correctness review with all findings repaired or dispositioned.
   The app is not published or installed automatically.

### 9.2 Automated tests

| Layer | Content | Count target |
| --- | --- | --- |
| Core | existing 620 (moved, unchanged) + Host port tests (TestHost: every former `crate::` call site exercised) | ≥620 + 25 |
| ui-model | one Rust test per ported TS case: renderer 339, shared 141 (minus worker-only `extensionRepository`/`lovedExtensions`, which stay TS) | ≥460, mapped in `tests/PORT_MAP.md` |
| update/host | updater 28 TS + existing Rust updater tests; placement/snap/layout; paths; single instance; ui-state import fixtures (WebKit SQLite and LevelDB samples generated in tests) | — |
| GPUI views | `gpui::test` per surface: render states, keyboard map, focus order, a11y tree snapshot (roles, names, set sizes) | one file per surface |
| Worker | existing 82 TS + `pnpm test:extension-worker` + the Rust ignored real-Bun test | unchanged, all run |
| Extension fixtures | §9.3 | 9 fixtures |

Tests are replaced, not layered (codebase-design): a TS test is deleted only in the same
commit that adds its mapped Rust twin, and `PORT_MAP.md` records both names. Adding skips or
`#[ignore]` to pass is a review failure.

### 9.3 Real extension SDK fixtures (Bun → core → GPUI)

`tests/fixtures/extensions/` holds small extensions written against `@raycast/api` types,
compiled by the real worker. Each one runs through the real Bun worker and core and renders in
a GPUI test window:

1. List: sections, accessories, detail pane with metadata, dropdown, client filter, server
   search (`onSearchTextChange`), pagination, `isLoading`, selection kept across refresh.
2. Grid: columns, sections, empty view.
3. Detail: markdown with code, image, table and allowed HTML subset; metadata.
4. Form: every field kind, validation errors, submit, `Tezbar.PreferenceSetup`.
5. ActionPanel: sections, shortcuts, destructive, Copy/Paste/Open/ShowInFinder (with fake host
   effects), Push/Pop navigation stack, `backActionId`.
6. Prompts: `confirmAlert` → `host.dialog.confirm` → native prompt (TestHost answers).
7. Toast/HUD and `showHUD` + close.
8. MenuBarExtra: status item set, update and remove, action callback.
9. Invalidation: background timer → `session-invalidated` → one refresh, stale generations
   dropped.

### 9.4 Performance and memory (measured after parity, not promised)

Rerun the matched harness (promoted scripts) with **full-feature** A (last Tauri) versus B
(GPUI) on the same fixture, same compiler, and service graphs listed side by side: hidden,
visible, retained, after an AI chat stream, after an extension list with 1,000 items, and with
terminal output at 10 MB. Bounded resources to verify: image cache LRU (count and bytes),
markdown parse cache per message, terminal scrollback 100k lines, extension session disposal,
chat transcript virtualization (`list` element), and no growth over 100 show/hide cycles (±5 %).
Report per §6.6 of the prototype spec. No ratio claims.

### 9.5 Manual matrices (recorded with screenshots by the user or an approved method)

- IME: Latin, Russian, Kazakh, dead keys, emoji palette, Japanese Kana, Pinyin, in the launcher,
  composer, form, notes and terminal (Mac). Microsoft Pinyin and Japanese IME on Windows in an
  interactive session.
- Accessibility: VoiceOver on launcher search, result navigation, Settings tabs, approval card
  and an extension form. Narrator for the same on Windows.
- Window lifecycle: hotkey toggle, tray, blur-hide grace, Settings focus rules, multi-monitor
  with mixed DPI, resize from all edges, zoom, snap, sessions panel following, Dock click,
  second-instance activation, quit with running terminals (confirm), sleep and wake.
- Theme: light, dark and system switching live in all windows.

---

## 10. Risks, decisions and unknowns

### 10.1 User decisions (with evidence; everything else is decided above)

| ID | Decision | Evidence | Recommendation |
| --- | --- | --- | --- |
| **U1** | Release cutover: which channel gets the first GPUI build, and whether a final Tauri release ships first | §7.3 needs the Tauri updater to install the GPUI build; §6.3 import works with or without a bridge release | Alpha channel first, after the M11 rehearsal; no bridge release needed |
| **U2** | Production shaders on macOS: install the Xcode Metal toolchain on the release Mac (a system change) **or** ship `runtime_shaders` | `xcrun --find metal` fails on this Mac (prototype spec §5.2); runtime shaders add first-launch shader compilation (not measured for the cold first launch) | Install the Metal toolchain before the first public GPUI release; runtime shaders acceptable for alpha only |
| **U3** | Three inert settings carried over: “Launch Tezbar at login” (a read-only checked box with no autostart code), “Push to Talk” (copy claims hold-to-dictate, but `useHoldToSpeak` is unreachable), “Last Screen” retention (saved, but nothing emits `resetUi:true`) | `SettingsView.tsx:1951–1958, 2870–2881, 2047`; reachability walk; `rg autostart` = 0 | Keep exact parity in M7 (same copy, same inert behaviour) unless you choose to implement or remove them |

### 10.2 Opens (each with an exit check)

O-PROMPT (Windows prompt), O-LOG (log path), O-CACHE (Windows cache dir), O-LS (legacy storage locations and
encodings), O-NSIS (installer keys), O-MAN (single manifest), O-LIVE (AccessKit live
regions), O-FXC (Windows release shader compiler present), O-TRAY/O-HOTKEY (prove
`tray-icon` and `global-hotkey` deliver events on GPUI's main run loop on macOS and the Win32
message loop on Windows; M4 spike, before any UI work depends on them), O-PASSTHRU (snap
overlay click-through through a raw handle), O-ALAC (`alacritty_terminal` crates.io version,
license and API at implementation time), O-SYNTECT (bundled grammar coverage vs highlight.js
`common`), O-QR (`qrcode` output equals current PNG dimensions and quiet zone; visual tolerance
test).

### 10.3 Risks

| Risk | Mitigation |
| --- | --- |
| Core extraction breaks behaviour invisibly | M1 is a move plus a port with zero logic change. All 620 tests and the Tauri app stay green. Review the diff with `git diff -M`. |
| Text editing quality (IME, selection, undo) across many fields | One `text_input` implementation, a test matrix, manual IME runs |
| GPL contamination via copy-paste from Zed crates | §1.2 rule. Reviewer checks `cargo about`/license list and greps for Zed file headers. |
| Accessibility regressions vs WebView | §5.8 table, a11y tree snapshot tests, manual runs; not claimed until recorded |
| Updater cutover failure strands users on Tauri | §7.3 rehearsal on staged installs; keep Tauri artifacts available for one more release on failure |
| Memory regressions in long sessions | §9.4 bounded caches and 100-cycle test |
| Scope creep into redesign | §5.6 lists the only deviations; anything else is a review failure |

### 10.4 Known pre-existing discrepancies (ledger §X)

Unreachable modules: `ProvidersView.tsx` (superseded by Settings AI), `Sidebar.tsx`,
`hooks/useHoldToSpeak.ts`, `agentChat/timeline.ts` (logic covered by streaming tests),
`currency/regionToCurrency.ts`, `shared/ipc.ts`. Bridge members with no caller (26):
`getExtensions`, `listInstalledExtensions`, `searchStoreExtensions`, `installExtension`,
`uninstallExtension`, `inspectExtension`, `reinstallExtension`, `getExtensionInstallError`,
`runSearchBenchmark`, `getSearchBenchmarkHistory`, `runAiAction`, `voiceStop`,
`listVoiceSttModes`, `setWindowContentHeight` (a no-op today), `checkPiAgent`, `installPiAgent`,
`getSafetyDescriptors`, `getSafetyLog`, `clearSafetyLog`, `getNativeCommands`, `vaultReset`,
`terminalSessionsShow`, `terminalSessionsSync`, `onQuickNoteSaveShortcut` (no emitter),
`chatRun`, `chatClear`. Their **core channels stay** (other callers, CLI, tests). No native
UI is created for them, which keeps parity with what users can reach today.

---

## 11. Codex implementation brief

Work only in `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch
`feat/native-gpui-app` (or a coordinator-assigned worktree off it per lane). Read this spec,
the ledger, `docs/MACOS_SIGNING.md` before any bundle or signing step, and the prototype README.

Start with **M0 then M1**. Don't write any GPUI surface before M1 is green.

M1 steps:

1. `git mv src-tauri/src/native_core crates/tezbar-core/src` (keep history). Move
   `native_terminal.rs` → `terminal/pty.rs`, `terminal_bridge.rs`, `child_supervisor*.rs`,
   `windows_job.rs`, `resource_directory.rs`, `native_v2_cli.rs` and `fts_rank*`. Create
   `crates/tezbar-core/Cargo.toml` with the subset of src-tauri dependencies the moved code uses
   (no `tauri*`). `build.rs` compiles `fts_rank_math.c`.
2. Add `host.rs` (§3.2). Replace each `AppHandle` site listed in §1.1 with a `Host` method, one
   call site per commit where practical, each with a `TestHost` test proving the old behaviour
   (window visibility before paste, vault show after unlock, shortcut register-then-release
   rollback, tray limits, confirm reply mapping).
3. In `src-tauri`, depend on `tezbar-core` by path, implement `TauriHost` by calling the
   existing crate-root functions, and delete `mod native_core` and `launcher_facade.rs`.
   `src-tauri/Cargo.lock` may change **only** by adding the new path packages. No registry
   package version may change. Run `lock-drift` and record it.
4. Gates: `cargo test --locked -p tezbar-core` (≥620 passing, existing ignores only);
   `cargo +1.85.0 check --locked --manifest-path src-tauri/Cargo.toml --lib`; `pnpm typecheck`;
   `pnpm test`; `pnpm test:extension-worker`; the real-Bun ignored test;
   `rg -n "tauri" crates/tezbar-core/src` = 0.

General rules: no behaviour changes in M1–M2; typed façade only in the app; controllers return
effects; one `text_input`; no GPL Zed code; every ledger row you touch gets evidence (test name,
screenshot path or command log). No push, PR, merge, release, GitHub Actions, installs, real
profiles, Keychain or clipboard use. Windows builds use the Tailscale host with archive staging
only. Report: commits, test counts, lock drift, ledger rows changed, open items hit.
