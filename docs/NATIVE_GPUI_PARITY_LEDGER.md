# Native GPUI parity ledger

M0–M2 review dispositions: see [repair record](native-m01/m2-review-repairs.md).
Native update status handling deliberately surfaces feed failures more strictly
(F7). Temporary Tauri extension-tray removal and interrupted Vault-auth blur
suppression are preserved old defects; native acceptance must verify real tray
removal and cancellation-safe blur restoration. No UI row becomes done from a
portable-host unit test alone.

Companion to [NATIVE_GPUI_APP_SPEC.md](NATIVE_GPUI_APP_SPEC.md). Source state: `5a5d9da`.
Every row starts as `todo`. An implementer changes the status only together with evidence:
a test name, a log path or a screenshot path. Final statuses are `native` (GPUI view and
ui-model), `host` (OS adapter), `worker` (stays in Bun), `core` (already Rust, wired to a
native control) or `not-reachable` (proof cited).

Abbreviations: `CB` = `src/renderer/CommandBar.tsx`, `SV` = `src/renderer/SettingsView.tsx`,
`lib` = `src-tauri/src/lib.rs`, `nc/` = `src-tauri/src/native_core/`, `um::` =
`crates/tezbar-ui-model`, `ui::` = `crates/tezbar-ui`, `app/` = `apps/tezbar/src/`. Test
names in the acceptance column are the existing tests to port. *(new)* marks a test that has
to be written.

Reachability method: a static import walk from `src/renderer/main.tsx` that follows
relative static imports, re-exports and literal dynamic `import()`. It found 144 reachable
renderer/shared modules. Unreachable non-test modules are listed in §X. Bridge members with no
caller were found by exact word search across `src/renderer` (excluding tests and the bridge).

---

## Implementation checkpoints

M3 controllers: source `86e9ab7`, 337 combined tests on Rust 1.98.1 and 1.85;
[repair and mapping evidence](native-m03/agent-settings-m03b-evidence.md).
M4 foundation: source `ac03a37`, normal 52 UI/app tests and 23 control-feature
app tests; [handoff and manual gates](native-m04/foundation.md). Parent verified
all source hashes and a clean locked debug build from `9474bdb` without WebKit.
These are implementation checkpoints. Opus closure, actual surface integration
and the listed OS/visual gates remain open, so rows below are not promoted to
final parity by model tests or a foundation build.

M0–M1 implementation: `48fa44e`. See [verification](NATIVE_GPUI_VERIFICATION.md),
[test retention](native-m01/test-retention.json), and [lock report](native-m01/lock-drift.json).
Mac release and Windows debug compile gates passed. The core/Host extraction, legacy
adapter and prototype compatibility are validated; native window/surface parity is
still pending. Existing W/S/L/etc. rows remain `todo`: a Host seam is not a native UI.

M2 libraries: runtime source `80a2aa1`. See the [M2 checkpoint and handoff](NATIVE_GPUI_VERIFICATION.md#m2-checkpoint),
[retention map](native-m01/m2-test-retention.json), [all-lock audit](native-m01/m2-lock-drift.json),
[source audit](native-m01/m2-source-audit.json) and [artifacts](native-m01/m2-artifacts.json).
Mac: core 564/8 ignored, host 39/0, update 22/1, UI-model 28/0, Tauri 13/0;
666 passed and the original 9 ignores, zero missing baseline cases. Prototype 8/0;
TS 744 passed/7 existing skipped; real worker fixture and explicit 1.85 checks passed.
Mac release and verified-archive Windows debug compile passed; Windows host 38/0,
UI-model 28/0 and update 16/1 passed. No ledger UI row changes status: the shared OS
implementations and updater are tested, while GPUI controls, bootstrap/event-loop wiring,
manual OS spikes and M3–M11 parity remain outstanding. Inactive services remain lazy.

## §W Windows, processes and OS integration

| ID | Behaviour | Current owner | New module | Acceptance | Status |
| --- | --- | --- | --- | --- | --- |
| W-01 | Launcher window: frameless, transparent, always-on-top, no taskbar entry, 760×640 default, min 480×360, hidden at start, shown on Ready | `tauri.conf.json` windows[0], `lib` `RunEvent::Ready` | `app/windows/launcher.rs` | gpui test of window options; screenshot light/dark | todo |
| W-02 | Placement: active monitor = frontmost app window's monitor, then cursor monitor; per-monitor persisted position (`windowPositionsByMonitorV3` macOS / `V2`), legacy key reset, clamp to work area, mixed DPI, Windows post-show placement finalization | `lib:553–1730` | `tezbar-host::placement` | port `position_updates_coalesce_into_latest_snapshot` + *(new)* monitor-selection and clamp tables; manual multi-monitor | todo |
| W-03 | Blur-hide: hide 120 ms after focus loss unless Settings is visible (then re-hide launcher), sessions panel focused, `suppress_blur_hide`, or not yet placed; persist position first | `lib:2999–3044` | `app/windows/launcher.rs` + `GpuiHost` | *(new)* gpui focus-sequence tests incl. tray activation flash case | todo |
| W-04 | Show/hide/toggle; `window-shown {resetUi:false}`, `window-hidden`, `window-visibility`; idle trim 5 s after hide (generation-checked) | `lib:268–318, 2561–2600` | `app/windows/launcher.rs` | *(new)* trim fires once after 5 s hidden, cancelled by show | todo |
| W-05 | Edge/corner resize tracking the physical mouse; per-monitor size+zoom profiles in `launcher-layouts-v1.json`; `launcher-layout-changed`, `launcher-viewport-changed`; reset per monitor | `window_layout.rs` (735 lines), `LauncherAppearance.tsx` | `tezbar-host::layout`, `app/windows/launcher.rs` | port `windowLayout.test.ts` (3), `launcherViewport.test.ts` (6), `LauncherAppearance.test.ts` (3), Rust layout tests | todo |
| W-06 | Interface zoom ⌘/Ctrl + − 0 (0.75–2.0, step 0.1); in Terminal those change text size and Alt+ changes UI zoom | `useLauncherScaleShortcuts.ts`, `shared/windowLayout.ts` | `um::launcher::zoom` | port `launcherZoomAfterKey` cases | todo |
| W-07 | Snap drag: start/end, slow-speed dwell 130 ms, thresholds 32/32 px, speed 420 px/s, centre/edge targets, overlay guides window (click-through), Windows cursor-driven drag task | `lib:347–470, 859–1240, 2239–2357, 2643–2726`; `App.tsx` `SnapOverlayApp`; `windowDrag.ts` | `tezbar-host::placement::snap`, `app/windows/snap_overlay.rs` | port `windowDrag.test.ts` (6) + Rust snap tests; manual drag | todo |
| W-08 | Settings window: 920×680 resizable, titled “Tezbar Settings”; opening and focusing hide launcher; close handshake (`settings-close-requested` generation, ack, finish, 3 s unacknowledged fallback); Permissions sub-surface inside window | `lib:2358–2420, 2601–2636, 2866–2905`; `App.tsx` `SettingsWindowApp`; `tauri-bridge.ts:128–170` | `app/windows/settings.rs`, `um::settings::WriteQueue` | port `settingsLifecycle.bridge.test.ts` + *(new)* close fallback test | todo |
| W-09 | Terminal sessions panel: 300 px, overhang 266, top 96, bottom 86, min height 260, parented, follows move/resize, hides with launcher on blur (80 ms grace) | `lib:2437–2505, 2907–2935`; `TerminalSessionsWindow.tsx` | `app/windows/terminal_sessions.rs` | *(new)* layout test from constants; manual | todo |
| W-10 | Tray: template icon, left-click-up toggles launcher, right-click menu “Show Tezbar”, “Settings”, “Quit Tezbar” | `lib:3158–3251` | `tezbar-host::status_items`, `app/app.rs` | O-TRAY spike; manual | todo |
| W-11 | Launcher hotkey (default Alt+Space macOS / Control+Space Windows, `raymesHotkey`), register-then-release with rollback, per-command hotkeys (`commandHotkeys`) → `execute_command_shortcut` | `nc/platform.rs:73–200`, `lib:2820–2845` | `tezbar-host::hotkeys` via `Host::shortcuts` | existing `platform_tests.rs`; O-HOTKEY spike | todo |
| W-12 | Single instance: second launch focuses visible Settings, else places and shows launcher | `lib:2801–2813` | `tezbar-host::single_instance` | *(new)* two-process test with isolated profile | todo |
| W-13 | Quit: `app:confirm-quit` dialog (“Quit Tezbar?”, detail about background tasks and terminals), unregister shortcuts, stop terminals, core shutdown ≤8 s, flush positions, exit | `lib:473–488`, `nc/platform.rs:345–351` | `app/app.rs` on_app_quit | *(new)* bounded shutdown test with TestHost | todo |
| W-14 | Native dialogs: confirm (`core:platform:confirm`, extensions `host.dialog.confirm`), Knowledge folder chooser, QR PNG save; blur-hide suppressed during dialogs | `nc/platform.rs:522`, `nc/knowledge/mod.rs:1863`, `qr_download.rs`, `tauri-bridge.ts:197–210` | `Host::dialog` → `Window::prompt`, `prompt_for_paths`, `prompt_for_new_path` | TestHost reply mapping tests | todo |
| W-15 | Notifications: extension timer completion (macOS UNUserNotification with bundleless fallback), `host.notifications.show/remove` | `timer_notifications.rs`, `nc/extensions/process_notifications.rs` | `tezbar-host::notifications` | existing `timerNotifications.test.ts` + Rust tests | todo |
| W-16 | macOS Regular activation policy (Dock icon); Dock click restores launcher | Tauri default (no override, no `LSUIElement`) | `app/app.rs` `on_reopen` | manual screenshot | todo |
| W-17 | Backend status: `backend-status` events; BackendConnection gate (starting, reconnecting, failed + Retry → `restart_backend`), revisions | `lib:97–140`, `BackendConnection.tsx`, `backendGeneration.ts` | `um::launcher::core_status`, launcher states L-02/L-03 | *(new)* failed start → Retry restarts `core.start()` | todo |
| W-18 | Logging: Info level, 5 MB rotation, platform log dir | `tauri_plugin_log` | `tezbar-host::logging` | O-LOG path equality check | todo |
| W-19 | Quick Look window state: launcher drops always-on-top while previewing, refocuses after | `lib:2736–2744` | `GpuiHost` | manual | todo |
| W-20 | Hide-for-paste: hide all Tezbar windows, `NSApp hide` (macOS), wait 80 ms, synthesize ⌘V/Ctrl+V | `nc/platform.rs:386–406` | `Host::surface(HideForPaste)` + `tezbar-host::input` | TestHost ordering test | todo |

## §S Surfaces and router

| ID | Surface / behaviour | Entry and exit (Verified) | Current owner | New module | States (light and dark screenshots each) | Status |
| --- | --- | --- | --- | --- | --- | --- |
| S-01 | Command bar | default, Esc back, `window-shown` focus | `CB` | `app/surfaces/command_bar/` | see §L | todo |
| S-02 | Onboarding | `hasCompletedOnboarding=false` at boot (one retry after 400 ms, then fail open), `start-onboarding`, tray surface | `OnboardingView.tsx` (1,500) | `app/surfaces/onboarding.rs` | §O | todo |
| S-03 | AI chat | AI modes, history rows, resume; stays mounted once visited (hidden, not unmounted) | `AgentChatView.tsx` | `app/surfaces/ai_chat/` | §C | todo |
| S-04 | Settings (window) | ⌘/Ctrl+, (AI tab), tray, commands, `app:open-surface settings-*` (7 tab variants) | `SV`, `App.tsx` | `app/windows/settings.rs`, `app/surfaces/settings/*` | §ST | todo |
| S-05 | Extensions store | `open-extensions`, Settings → Browse store (then closes Settings), `open_extensions_window`; remount on re-open clears search | `ExtensionsView.tsx` | `app/surfaces/extensions_store.rs` | §E | todo |
| S-06 | Extension runtime | view results from search, hotkey (`run-extension-command-from-hotkey`), AI task action | `ExtensionRuntimeView.tsx` + `src/raycast-api/*` | `app/surfaces/extension_runtime/` | §E | todo |
| S-07 | System monitor | `show-system-monitor`, tray surface | `SystemMonitorView.tsx` | `app/surfaces/system_monitor.rs` | loading, stats, battery absent, refresh warning | todo |
| S-08 | Indexing | `open-indexing`, AI task, tray | `IndexingView.tsx` | `app/surfaces/indexing.rs` | §N | todo |
| S-09 | Permissions | Settings → Open Permissions (window variant, back → Settings); launcher variant | `PermissionsView.tsx` | `app/surfaces/permissions.rs` | per-permission status chips: granted, denied, restricted, not-determined, unsupported | todo |
| S-10 | Clipboard history | native `open-clipboard-history`, tray | `ClipboardView.tsx` | `app/surfaces/clipboard.rs` | §U | todo |
| S-11 | Snippets | `open-snippets` (both ids) | `SnippetsView.tsx` | `app/surfaces/snippets.rs` | §U | todo |
| S-12 | Quick notes | `open-notes`, `open-quick-notes`, note search result (`createdAt` preselect) | `NotesView.tsx` | `app/surfaces/notes.rs` | §U | todo |
| S-13 | Vault | `open-vault`, vault item result (`itemId`), clipboard Save to Vault (draft; close returns to clipboard) | `VaultView.tsx` | `app/surfaces/vault.rs` | §U | todo |
| S-14 | Emoji picker | `open-emoji-picker` | `EmojiPickerView.tsx` | `app/surfaces/emoji.rs` | §U | todo |
| S-15 | Terminal | `>` mode submit, sessions list, sessions panel action; Esc returns with `>` sentinel | `TerminalView.tsx` | `app/surfaces/terminal/` | §T | todo |
| S-16 | Release notes | unread row on home, `show-updated-version`; marks read | `ReleaseNotesView.tsx`, `releaseNotes.ts` | `app/surfaces/release_notes.rs` | loading, error + Retry, notes | todo |
| S-17 | Snap overlay | drag | `App.tsx` `SnapOverlayApp` | `app/windows/snap_overlay.rs` | guides x, y, centred | todo |
| S-18 | Terminal sessions window | Terminal sessions toggle | `TerminalSessionsWindow.tsx` | `app/windows/terminal_sessions.rs` | empty, list, new session | todo |
| S-19 | Router rules: Esc semantics (AI → launcher with `' '`, terminal → `>`, other → plain launcher; command → escape-gate consumers, then hide); ⌘/Ctrl+N routed per surface (`newItemShortcut`); timed return (`extensionRuntimeTimeoutMs`, `aiModeTimeoutMs`, `terminalModeTimeoutMs`) reset by pointer, key, input or wheel; `app:open-surface` handling; dialog-open guard | `App.tsx:330–760`, `escapeGate.ts`, `newItemShortcut.ts` | `um::router` | port `newItemShortcut.test.ts` (2) + *(new)* router tests | todo |

## §L Command bar modes and states

| ID | Mode / state | Current owner | Acceptance | Status |
| --- | --- | --- | --- | --- |
| L-01 | Mode priority from `commandBarInputMode` (terminal > deep > AI > slash/app > search) | `commandBarInputMode.ts` | port 6 tests | todo |
| L-02 | Core starting: input usable, footer “Starting search…”, latest text searched when ready | BackendConnection + prototype | prototype session tests | todo |
| L-03 | Core failed: danger line + Retry | BackendConnection | *(new)* | todo |
| L-04 | Home (empty input): pins rail (up to 12, ⌥ slots), recents, unread release-notes row first, cached home results shown first frame, delayed home warmup not run in Deep Search | `CB:271–372`, `pinnedCommands.ts`, `releaseNotes.ts` | port `pinnedCommands.test.ts` (13), `releaseNotes.test.ts` (13), lifecycle “does not start delayed home warmup…” | todo |
| L-05 | Typing: 8 ms coalescing, ≤1 in flight, latest-wins, stale suppression, no flash of empty list | `latestSearch.ts`, `CB:1800–1900` | lifecycle tests “coalesces pending bursts…”, “does not let an unresolved previous backend generation block recovery” | todo |
| L-06 | IME: no searches during composition; recover when hide interrupts composition | `CB` composition handlers | lifecycle IME tests (2) | todo |
| L-07 | Results list: 36 px rows, icon, title ≤70 %, subtitle, kind tag, selection fill; same-frame highlight; fresh row content and icon replacement; image error recovery | `CB:5230–5570`, `GlideList.tsx` | lifecycle row tests (4) | todo |
| L-08 | Pointer-move selects, scroll doesn't | `CB` `handleRowMouseMove` | *(new)* | todo |
| L-09 | Single-character local matching does not extend the deadline | `CB` | lifecycle test | todo |
| L-10 | `launcher:invalidated` refreshes latest query; older in-flight ignored | `CB:2330` | lifecycle test | todo |
| L-11 | Hide drops pending query; reopen shows latest home result; one authoritative home refresh on hide | `CB` | lifecycle tests (2) | todo |
| L-12 | Deep Search (`!`): hidden prefix, ≥3 chars loading row, Knowledge results, navigation activation (Enter/Esc inside results), Backspace on empty leaves mode, two-step Esc | `CB:3900–3990`, `searchMode.ts` | port `searchMode.test.ts` (8); lifecycle Deep Search tests (2) | todo |
| L-13 | Deep Search recommendation row when no good metadata match (≥3 chars, layout variants) | `searchMode.ts` `buildDeepSearchRecommendation` | `searchMode.test.ts`, `searchQueryVariants.test.ts` (6) | todo |
| L-14 | AI mode: leading space, trailing double space, `/dir  task`; Enter opens AI chat (submit / newChat with cwd); Esc clears then leaves; pending search cancelled on entering AI | `commandBarInputMode.ts`, `aiChatSurface.ts` | lifecycle “cancels pending search when entering AI mode…” | todo |
| L-15 | AI mode chrome: model picker menu, identity, task-status slot; ⌘⇧S capture; ⌘Enter | `CB`, `ModelPicker.tsx` | `chrome.test.ts` (4) | todo |
| L-16 | Chat history list (in AI mode, empty task): Up/Down, Enter resumes, cached rows paint immediately | `CB`, `historyCache.ts` | port `historyCache.test.ts` (3) | todo |
| L-17 | Terminal mode (`>` on empty bar): prompt info, sessions list with pins, ⌘K settings drawer (save-for, keep-alive), ⌘X stop, ⌘P pin, ⌘D delete, Backspace on empty leaves | `CB:3580–3610, 1583–1730`, `terminalPreferences.ts` | port `terminalSessionSelection.test.ts` (6), `terminalSessionLabel.test.ts` (3), `terminalSessionHistory.test.ts` (3) | todo |
| L-18 | Slash path completion (`/`): `path:complete`, sections, Tab completes, space after path switches to AI mode, ⌘Enter opens folder, Enter on a typed path opens it, directory visits recorded | `CB:2287, 3720–3830` | *(new)* | todo |
| L-19 | Application grid (`` ` ``): 7 columns, arrow grid navigation, app icons, Backspace on empty leaves, two-step Esc | `CB:491–535, 4063–4105` | *(new)* | todo |
| L-20 | Extension arguments: inline chip, Tab/Shift+Tab between fields, declared argument metadata, launcher argument surface `extension:launcher-search`, clear-then-exit Escape | `CB:1488–1530, 2300–2340, 3947–3965`, `launcherArgument.ts` | lifecycle “uses declared argument metadata…”, `launcherArgument.test.ts` | todo |
| L-21 | Calculator rows (`=` prefix and expressions incl. `%`), copy on Enter | `calculator.ts` | port `calculator.test.ts` (6) | todo |
| L-22 | Currency rows (Frankfurter, preferred currencies, synonyms) | `currency/*`, `useCurrencyConversion.ts` | port `parseCurrencyQuery.test.ts` (10), `currencyPreferences.test.ts` (4) | todo |
| L-23 | Colour conversion rows | `colorConverter.ts` | *(new)* parity table from TS outputs | todo |
| L-24 | Answer card: `query` intent → `stream-token/done/error`; empty answer; providers and extensions intents redirect | `CB:3833–3850, 1757–1770` | *(new)* | todo |
| L-25 | Action message card (password 9 s, copied/toggle/info 7 s, plain 4 s) with native command results | `CB:1409–1430, 581–630` | *(new)* | todo |
| L-26 | Vault copy feedback: “Copying…”, “Waiting for Touch ID…”, done or error 2.5 s; ⌘C on vault row unless text is selected | `CB:3611–3637`, `vaultCopy.ts` | port `vaultCopy.test.ts` (4) | todo |
| L-27 | Update status row states (checking, available, manualAvailable, downloading, ready, upToDate “checked in this window”, error) and its actions | `CB:2040–2150, 3055–3100` | port `updater.test.ts` (28) | todo |
| L-28 | Pins: ⌘P pin picker / unpin, “Temporary results can’t be pinned”, drag to reorder (threshold 6 px), drag-out to unpin, tooltip, ⌥ slot run | `CB:3640–3675`, `pinnedCommands.ts` | `pinnedCommands.test.ts` | todo |
| L-29 | Query history recall (Up on empty input) | `launcherQueryHistory.ts` | port 8 tests | todo |
| L-30 | Quick Look: Space on a file result | `CB` `canQuickLookSelectedResult`, `file:quick-look` | port `searchResultPreview.test.ts` (5) | todo |
| L-31 | Rolling hint placeholder and command hints (`/directory`, `` ` ``, `!`, `SPACE`, `>`) | `ui/RollingText.tsx`, `CB:138–145` | *(new)*; static first frame | todo |
| L-32 | Footer hints per state (Open ↵, Pin ⌘P, Quick Look Space, Save note, Deep Search…) | `CB:5740–5840` | screenshot review | todo |

## §A Result actions and commands

| ID | Action / command | Current owner | Acceptance | Status |
| --- | --- | --- | --- | --- |
| A-01 | `open-app` | `nc/launcher` → `core:platform:open-app` | facade test promoted | todo |
| A-02 | `open-file` | core | same | todo |
| A-03 | `open-url` | core | same | todo |
| A-04 | `open-with-app` | core | same | todo |
| A-05 | `copy-text` (snippets show “Copied”) | core + `CB:3370` | TestHost clipboard | todo |
| A-06 | `copy-and-paste-text` | core → `HideForPaste` | W-20 | todo |
| A-07 | `add-note` | core | one usage record | todo |
| A-08 | `run-extension-command`: no-view (message + effects), view (runtime surface), preference setup, arguments, AI-powered titles, colour conversion pending | `CB:3262–3345`, `extensionRuntimeStatus.ts` | port `extensionRuntimeStatus.test.ts` (13) | todo |
| A-09 | `invoke-command` ids: `open-settings`, `activate-deep-search`, `open-extensions-settings`, `open-extensions`, `open-snippets`, `open-vault`, `open-notes`, `open-emoji-picker`, `open-indexing`, `start-onboarding`, `download-app-update`, `restart-app-update`, `check-for-updates`, `open-settings-updates`, `show-updated-version`, `open-providers`, `quit-tezbar` | `CB:3038–3157` | *(new)* table test: id → navigation effect | todo |
| A-10 | `run-native-command`: 43 descriptors (macOS and Windows JSON; Windows equivalents per WINDOWS_SUPPORT) via `core:native:execute` with safety | `nc/system/commands.rs` | existing Rust tests | todo |
| A-11 | Native commands that open surfaces: `show-system-monitor`, `open-clipboard-history`, `open-snippets`, `open-vault`, `open-quick-notes`, `open-emoji-picker`; `quit-tezbar` → confirm | `CB:3181–3260` | *(new)* | todo |
| A-12 | `run-shell` | core + safety | existing | todo |
| A-13 | `install-extension` | core + safety authorization UI | E-rows | todo |
| A-14 | `open-vault-item` (opens Vault on item; ⌘C copies) | `CB:3158–3180` | *(new)* | todo |
| A-15 | Usage recording: `search:execute` records once; handled-locally commands call `search:record-usage` once | `CB:3014–3030` | single-record test | todo |
| A-16 | Exactly-once activation ticket, held Enter ignored, pending generation Enter (1 s) | prototype `search_session.rs` | 6 prototype tests | todo |
| A-17 | `query` fallback when nothing is selected | `CB:3833` | L-24 | todo |
| A-18 | Path direct open `path-direct:` | `CB:3820–3826` | *(new)* | todo |
| A-19 | Directory visit record | `CB:3487, 3743` | *(new)* | todo |
| A-20 | TTS speak of answer (`voice:tts:speak`) | `CB:2766` | *(new)* | todo |
| A-21 | Hide after success (window hides, input clears) and failure keeps window with message | `CB:3346–3372` | prototype rule | todo |
| A-22 | Recent extension commands list (20) | `CB:249–270` | *(new)* | todo |
| A-23 | Command aliases (`commandAliases`) and disabled commands (`disabledCommands`) affect search | core `get-settings` | existing core tests | todo |
| A-24 | Extension command hotkey run (`run-extension-command-from-hotkey`) opens runtime view | `App.tsx:552–563`, `nc/mod.rs:118–131` | *(new)* | todo |
| A-25 | Command shortcuts that open surfaces (`execute_command_shortcut` surface map incl. `check-for-updates` → settings) | `nc/mod.rs:132–170` | existing + *(new)* | todo |

## §K Keyboard map

| ID | Context | Keys (macOS / Windows) | Action | Status |
| --- | --- | --- | --- | --- |
| K-01 | Global | launcher hotkey | toggle launcher | todo |
| K-02 | Global (in app) | ⌘Esc / Ctrl+Esc | hide from anywhere | todo |
| K-03 | Any surface | Esc | back rules (S-19) | todo |
| K-04 | Any surface | ⌘, / Ctrl+, | open Settings at AI tab | todo |
| K-05 | Any surface | ⌘N / Ctrl+N | new item for surface | todo |
| K-06 | Launcher | ↑ ↓ ↵, no wrap | select and activate | todo |
| K-07 | Launcher | ⌘↵ / Ctrl+Enter | secondary (open folder in completion; AI submit variants) | todo |
| K-08 | Launcher | Tab / ⇧Tab | complete path, argument fields | todo |
| K-09 | Launcher | Space | Quick Look (file selected, plain Space) | todo |
| K-10 | Launcher | ⌘P / Ctrl+P | pin or unpin | todo |
| K-11 | Launcher | ⌥1…9,0,-,= / Alt+… | run pinned slot | todo |
| K-12 | Launcher | ⌘C / Ctrl+C on vault row | copy secret | todo |
| K-13 | Launcher | ⌘H / Ctrl+H | history toggle (`CB:4163`) | todo |
| K-14 | Launcher | `>` on empty bar | terminal mode | todo |
| K-15 | Launcher | Backspace on empty mode input | leave mode | todo |
| K-16 | Terminal mode | macOS ⌘K/⌘X/⌘P/⌘D; Windows provisional Ctrl+Shift+K/X/P/D | settings, stop, pin, delete; Ctrl+letter stays shell input; O-TERM-KEYS preference and physical acceptance pending | todo |
| K-17 | App grid | ← → ↑ ↓ | grid navigation by 1 and by row | todo |
| K-18 | AI | ⌘⇧S | attach active screen | todo |
| K-19 | AI | ⌘. | stop | todo |
| K-20 | AI | ⌘N | new chat | todo |
| K-21 | AI | Enter / ⇧Enter | send / newline (composer) | todo |
| K-22 | AI approval | Tab, Enter on focused button only | approve, deny | todo |
| K-23 | Notes | ⌘B ⌘I ⌘U, ⌘N, ⌘D (outside editor), ⌘↵ copy, ↑↓ ↵ edit | formatting and list | todo |
| K-24 | Extension runtime | ⌘K, ↵, ⌘↵, action shortcuts, Esc pop | action panel and navigation | todo |
| K-25 | Clipboard | ↑↓, ↵ restore, pin, delete, Save to Vault, convert chips | list actions | todo |
| K-26 | Snippets | ⌘N new, ↵ copy, edit, delete, jump | list actions | todo |
| K-27 | Vault | ⌘N, ↵ copy, edit, lock | list actions | todo |
| K-28 | Emoji | arrows, ↵ copy, recent | grid actions | todo |
| K-29 | Terminal | ⌘/Ctrl +/−/0 text zoom, Alt-modified UI zoom; macOS ⌘K/⌘X/⌘P/⌘D; Windows provisional Ctrl+Shift+K/X/P/D; sessions toggle | terminal; O-TERM-KEYS preference and physical keyboard acceptance pending | todo |
| K-30 | Settings | Enter save, Esc close, Tab order | settings | todo |
| K-31 | Text fields | select all, word/line movement, undo/redo, IME | all inputs | todo |

## §ST Settings window and §O Onboarding

| ID | Tab / row | Behaviour and channels | Acceptance | Status |
| --- | --- | --- | --- | --- |
| ST-01 | Tabs: General, AI, Voice, Knowledge, Extensions, Permissions, Storage, Advanced; initial tab from `settingsInitialTab`/stored tab, live switch from launcher | `SV`, `App.tsx:101–122, 252–320` | *(new)* | todo |
| ST-02 | General → Launcher appearance (size, scale, reset) | `LauncherAppearance.tsx` | W-05 | todo |
| ST-03 | General → Startup “Launch Tezbar at login” (read-only checked; **inert**, U3) | `SV:1951–1958` | parity screenshot | todo |
| ST-04 | General → Onboarding toggle (show on next launch) | `SV:1961` | *(new)* | todo |
| ST-05 | General → Tezbar Hotkey recorder (`core:platform:update-launcher-hotkey`, conflict error keeps old) | `HotkeyRecorder.tsx`, `nc/platform.rs` | existing Rust + *(new)* recorder test | todo |
| ST-06 | General → Last Screen retention seconds (`uiStateRetentionMs`; **inert**, U3) | `SV:2047` | parity | todo |
| ST-07 | General → Extension/AI/Terminal Return timeouts | `SV:2080–2160` | S-19 | todo |
| ST-08 | General → Appearance theme (white, dark, system) | `SV:2163–2200`, `theme.ts` | port `theme.test.ts` (3) | todo |
| ST-09 | General → Updates: status, check, download, restart, release notes, channel selector, auto-update | `AppUpdateSettings.tsx`, `UpdateChannelSettings.tsx` | updater tests | todo |
| ST-10 | General → Currency conversion preferences | `CurrencySettings.tsx` | L-22 | todo |
| ST-11 | AI → provider list (built-in + custom), active/selected/configured state | `SV:2216–2414` | §4.2 tests | todo |
| ST-12 | AI → provider form: protocol, base URL, API key, headers (secret refs), models discovery (`llm-list-models`), per-model selector, token limits, thinking/reasoning, timeout | `SV:2414–2790` | port first 12 `SettingsView.knowledge.test.ts` cases | todo |
| ST-13 | AI → GitHub Copilot device flow (`github-device-start/poll/cancel`) | `SV`, core | existing core + *(new)* UI state test | todo |
| ST-14 | AI → Antigravity accounts panel (`ai-accounts:antigravity:*`: status, list, sign-in, paste callback URL, switch, remove, cancel) | `ai/AiAccountsPanel.tsx` | *(new)* | todo |
| ST-15 | AI → AI Memory (enabled, max items, include private) | `SV:2792` | *(new)* | todo |
| ST-16 | AI → Action Mode (require permission, redaction) | `SV:2830` | *(new)* | todo |
| ST-17 | AI → AI Extensions section (C-20…C-30) | `ai/AiExtensionsSection.tsx`, `AiContributionsEditor.tsx` | port `AiExtensionsSection.test.ts` (4) | todo |
| ST-18 | AI → task provider/model overrides | `SV` | *(new)* | todo |
| ST-19 | Voice → Push to Talk row (**copy only**, U3), Detected Modes (`voice:stt:modes`), STT model list, download/delete/select with `voice:model:progress` | `SV:2867–2920` | *(new)* | todo |
| ST-20 | Permissions → Open Permissions (settles AI saves first) | `SV:2922–2946` | S-09 | todo |
| ST-21 | Extensions → installed list, filter, enable toggle, alias, hotkey, preferences editor, uninstall, Browse store | `ExtensionsSettingsTab.tsx`, `ExtensionPreferencesEditor.tsx` | port `ExtensionPreferencesEditor.test.ts` (2) | todo |
| ST-22 | Knowledge → Depth default, per-root depth, folders (add, add major, remove, enable), semantic search model variants, chunking speed, index status, processing, pause/resume/start | `SV:2951–3420`, `KnowledgeEmbeddingSettings.tsx`, `useKnowledgeSettings.ts` | port the 60 Knowledge cases | todo |
| ST-23 | Storage → Clipboard (watch, capture images, max megapixels, clear images) | `SV:3435–3510` | *(new)* | todo |
| ST-24 | Storage → Breakdown (`storage:breakdown`) | `SV:3511` | *(new)* | todo |
| ST-25 | Storage → Cleanup (vacuum search DB; clear WebView cache, which natively clears the legacy WebView data after a successful import, §6.3) | `SV:3531` | *(new)* guard test | todo |
| ST-26 | Advanced → Safety Dry-Run toggle | `SV:3565` | *(new)* | todo |
| ST-27 | Advanced → Danger Zone Quit | `SV:3592` | W-13 | todo |
| ST-28 | Footer hint bar (Save Enter with saved feedback, Close/Back Esc) | `SV:3605–3625` | flattened “Saved” (§5.6) | todo |
| ST-29 | Settings messages (success, error) and close error alert with Retry saved changes | `SV`, `tauri-bridge.ts:150–168` | W-08 | todo |
| ST-30 | Settings reacts to `llm-config-changed` and `ai-extensions-changed` without clobbering drafts | `SV` | *(new)* | todo |
| ST-31 | Settings initial-tab persistence (`tezbar:settings-tab` → ui-state) | `App.tsx` | §P | todo |
| O-01 | Step rail: Shortcut, Try the bar, AI model, DeepSearch, Permissions (macOS only); done/detail summaries | `OnboardingView.tsx:29–42, 1223` | *(new)* | todo |
| O-02 | Shortcut step: press keys, recorder (“Press the keys together…”), conflict errors | same | *(new)* | todo |
| O-03 | Try the bar: practice command bar with tasks (math `=`, folder `/`, terminal `>`, AI Space), uses calculator | same:215–220 | *(new)* | todo |
| O-04 | AI model step: providers (Anthropic, OpenAI, Ollama…), key entry, model list (`llm-list-models`), open provider site | same:650–670 | *(new)* | todo |
| O-05 | DeepSearch step: folders (choose, add major, remove), indexing progress, pause/resume, path validation (“No folder matches that path.”) | same | *(new)* | todo |
| O-06 | Permissions step (macOS): per-permission request and status | same | *(new)* | todo |
| O-07 | Finish/skip sets `hasCompletedOnboarding` and returns to launcher | same | *(new)* | todo |

## §C AI chat, providers and AI Extensions

| ID | Behaviour | Current owner | Acceptance (port) | Status |
| --- | --- | --- | --- | --- |
| C-01 | Boot variants: panel, submit (prompt + cwd), newChat (cwd), resume (sessionId), screen; nonce semantics | `shared/aiChatSurface.ts`, `AgentChatView` | ownership “starts fresh on every launcher new-chat event…”, “honors repeat screen and resume nonces…” | todo |
| C-02 | Per-conversation live owner; A output never in B | `agentChat/ownership` | “keeps A output and persistence out of B…” | todo |
| C-03 | Hidden runs continue; background approval status | same | “continues hidden runs and publishes background approval status…” | todo |
| C-04 | Unsaved background response survives persistence failure | same | “keeps an unsaved background response available…” | todo |
| C-05 | Reload settled background conversation after releasing owner | same | “reloads a settled background conversation…” | todo |
| C-06 | Stop persists partial text and stage, no active approval, only own run | `transport.ts` | “stop persists partial text…”, “preserves partial output on stop…”, “finalizes interrupted steps…” | todo |
| C-07 | Retry replaces failed assistant id | same | “successful retry replaces…”, “reuses the assistant message id on retry…” | todo |
| C-08 | Deletion tombstones (send/retry preparation, done overtakes delete, failed delete recovery, retained owner) | same | 6 delete tests | todo |
| C-09 | Approval card, focus, keyboard, persistence capability, expiry, mode picker independence | `AgentChatView`, `ApprovalModePicker.tsx` | “mode changes leave destructive approval pending…”, “preserves an active approval when resuming…” | todo |
| C-10 | Composer: draft kept when history resolves late; stale init ignored after different boot | same | 2 tests | todo |
| C-11 | Screen capture attach/remove; stale capture discarded after navigation or hide; dedupe | same | 3 capture tests | todo |
| C-12 | cwd and image stay with owner if config prep finishes after new chat | same | 1 test | todo |
| C-13 | Unmount cancels retained streams only on actual unmount | same | 1 test | todo |
| C-14 | Streaming coalescing to frame rate; follow-output near end | `streaming.ts` | 3 tests | todo |
| C-15 | Timeline order (text/stage) | `timeline.ts` (unreachable) | 3 tests (kept as ui-model tests) | todo |
| C-16 | Events before IPC response; canonical text and tool output kept; start failures visible and retryable; tool-only completion rejected; typed stage replay; provider failure without network | `transport.ts` | 9 transport tests | todo |
| C-17 | Message rendering: markdown (streaming fences), code copy, stages, diagnostics, errors with dismiss, thinking stage only | `agentChat/messages.tsx`, `ui/Markdown.tsx` | `codeBlockTheme.test.ts` (2) + *(new)* | todo |
| C-18 | History list (Past chats), delete turn, delete chat, titles and icons from `chat:generate-meta`, `chat:update-title` | `AgentChatView`, `shared/chatMeta.ts` | port `chatMeta.test.ts` (8) | todo |
| C-19 | Model picker (search, provider groups, manage providers link), agent profile chip, approval mode picker | `ModelPicker.tsx`, `ApprovalModePicker.tsx` | `model.test.ts` (1) + *(new)* | todo |
| C-20 | Composer `/` menu (`ai-extensions:composer-items`), prompt argument row (`ai-extensions:prompt-resolve`), Insert in draft | `ai/ComposerExtensions.tsx` | `ComposerExtensions.test.ts` (0 cases; *(new)* required) | todo |
| C-21 | Project extension notice (once per folder per session) → Review in Settings (`ai-extensions:review-project`, `dismiss-project-review`, `trust-project`) | `ai/ProjectExtensionNotice.tsx` | *(new)* | todo |
| C-22 | AI Extensions list, add (paste/import `parse-import`), update, remove, enable | `AiExtensionsSection.tsx` | 4 tests | todo |
| C-23 | Secrets: set, reset (`reset-secrets`), never echoed into UI | same | “fixture-token” not in DOM → a11y-tree assertion | todo |
| C-24 | OAuth (`oauth-start/status/cancel`), scopes from challenge, browser sign-in | same + core `mcp/oauth.rs` | existing Rust | todo |
| C-25 | Connection test (`ai-extensions:test`), logs (`logs`), read resource | same | existing Rust | todo |
| C-26 | Tool policy (`set-tool`, Allow all with typed confirmation, Ask every time), clear allowance | same | existing + *(new)* | todo |
| C-27 | Review flow: pending catalog with before/after descriptors, accept-review, stale review rejection | same | R17 tests | todo |
| C-28 | Instructions (`set-instructions`), prompts and commands contributions, profiles | `AiContributionsEditor.tsx` | *(new)* | todo |
| C-29 | Pi extensions alias list (read-only, not loaded) | `pi-extensions:list` | existing | todo |
| C-30 | Providers status (`llm-provider-statuses`) and errors (`llmErrors.ts`) | `shared/llmErrors.ts` | port 4 tests | todo |
| C-31 | Agent events (`agent:event` start, stage, message, answer, approval, approval-expired, done, error, log) typed | `shared/agent.ts` | serde round-trip tests | todo |
| C-32 | Chat surface chrome shares components with launcher (same header/footer geometry) | `chrome.test.ts` | 4 tests | todo |
| C-33 | Task actions from chat: open Indexing, Extension runtime, update | `App.tsx:640–650` | *(new)* | todo |
| C-34 | AI providers catalog single source (core `provider_defaults.json`) replaces `shared/aiProviders.ts` | `shared/aiProviders.ts` | port `aiProviders.test.ts` (9) against core catalog | todo |

## §E Extensions

M9 local headless implementation evidence (2026-11-07, unstaged on `db4d0df`): the linked native Store/runtime and all nine actual Bun → initialized NativeCore/TestHost → RuntimeView scenarios pass in the normal native suite (360 tests) and bench suite (362 tests). The [M9 receipt](native-m09/completion.md), [final APIs](native-m09/final-api.md) and [case map](native-m09/final-source-case-map.json) record exact assertions and limits. All 115 original expanded cases and 45 E rows are retained; this note does not change row acceptance. M7 retains E-04/ST-21 settings ownership. Windows, physical IME/AX/GPU/RAM, live installation and fresh independent design/closure review remain pending.

| ID | Behaviour | Current owner | Acceptance | Status |
| --- | --- | --- | --- | --- |
| E-01 | Store search (`extension:search-store` paged), paste GitHub URL to install, Raycast catalog (macOS) / Tezbar-loved (Windows) | `ExtensionsView.tsx`, `extensionCatalogState.ts` | port `ExtensionsView.test.ts` (5), `extensionCatalogState.test.ts` (8) | todo |
| E-02 | Detail pane: author, commands, extension id, runtime, supports, resources, checked, screenshots, Open README, Native Rust Extension label | `ExtensionsView.tsx` | screenshot | todo |
| E-03 | Install/remove with progress (`extension:install-progress`), safety authorization, catalog updated event | same | *(new)* | todo |
| E-04 | Settings → Extensions tab (ST-21): schema (`get-installed-extensions-settings-schema`), hotkeys (`update-command-hotkey`), aliases (`save-settings`), toggles, uninstall | `ExtensionsSettingsTab.tsx` | *(new)* | todo |
| E-05 | Preferences editor (`preferences:setup/get/set`), required fields, password masking | `ExtensionPreferencesEditor.tsx` | 2 tests | todo |
| E-06 | Session lifecycle: run, invoke, search text, refresh (`refreshMs`), load more, dispose, invalidation | `ExtensionRuntimeView.tsx`, `extensionSessionRefresh.ts` | port `extensionSessionRefresh.test.ts` (3), `visibleEventRefresh.test.ts` (1) | todo |
| E-07 | Navigation stack (Push/Pop), `backActionId`, Esc | runtime | fixture 5 | todo |
| E-08 | Action panel: ⌘K overlay, sections, filter, shortcuts, primary/secondary, destructive, notice states | `action-runtime-overlay.tsx` | fixture 5 | todo |
| E-09 | Action kinds: action, copy, open, push, pop, submit-form, show-in-finder | `shared/extensionRuntime.ts` | fixture 5 | todo |
| E-10 | Effects: clipboard, open, show-in-finder, toast, hud (apple-script and powershell executed in worker/core) | runtime | fixture 7 | todo |
| E-11 | `List` (searchBarPlaceholder, navigationTitle, searchText, filtering, isLoading, pagination) | `list-runtime.tsx` | fixture 1 | todo |
| E-12 | `List.Section` | same | fixture 1 | todo |
| E-13 | `List.Item` (title, subtitle, icon, accessories, keywords, actions) | same | fixture 1 | todo |
| E-14 | `List.Item.Detail` + `.Metadata.{Label,Link,TagList(.Item),Separator}` | same | fixture 1 | todo |
| E-15 | `List.EmptyView` | same | fixture 1 | todo |
| E-16 | `List.Dropdown` (+ `.Section`, `.Item`) searchBarAccessory | same | fixture 1 | todo |
| E-17 | `Grid` (columns, filtering, isLoading) | `grid-runtime.tsx` | port `grid-runtime.test.ts` (2); fixture 2 | todo |
| E-18 | `Grid.Section`, `Grid.Item` (content image/colour, title, subtitle) | same, `grid-content.ts` | fixture 2 | todo |
| E-19 | `Grid.EmptyView` | same | fixture 2 | todo |
| E-20 | `Detail` (markdown, metadata, navigationTitle) | `detail-runtime.tsx` | port `detail-runtime.test.ts` (1); fixture 3 | todo |
| E-21 | `Detail.Metadata.*` | same | fixture 3 | todo |
| E-22 | Colour detail rows (`buildColorFormatRows`) | `detail-runtime.tsx`, `colorConverter.ts` | `native-v2-visuals.test.ts` (3) | todo |
| E-23 | `Form` container, submit | `form-runtime.tsx` | port `form-runtime.test.ts` (12), `ExtensionRuntimeView.form.test.ts` (2) | todo |
| E-24 | `Form.TextField` | same | same | todo |
| E-25 | `Form.TextArea` | same | same | todo |
| E-26 | `Form.PasswordField` | same | same | todo |
| E-27 | `Form.Checkbox` | same | same | todo |
| E-28 | `Form.DatePicker` | same | same | todo |
| E-29 | `Form.Dropdown` (+ `.Section`, `.Item`) | same | same | todo |
| E-30 | `Form.TagPicker` (+ `.Item`) | same | same | todo |
| E-31 | `Form.FilePicker` (allowMultipleSelection; blur suppression) | same | same | todo |
| E-32 | `Form.Description`, `Form.Separator` | same | same | todo |
| E-33 | Native colour wheel (`nativeColorWheel`) via `color-picker-helper` | same | *(new)* | todo |
| E-34 | `Tezbar.PreferenceSetup` inline setup | `ExtensionRuntimeView.tsx` | fixture 4 | todo |
| E-35 | `MenuBarExtra` (+ `.Item`, `.Section`, `.Submenu`, `.Separator`) as status items (`host.tray.set/remove`); in-list rendering of MenuBarExtra items | `process_host.rs:274–345`, `list-runtime.tsx` | fixture 8 | todo |
| E-36 | Extension images: built-in icons, assets, remote URLs (fetch moves to Rust), tint, theme, emoji glyphs, cache trimming on idle | `extensionImages.ts`, `launcherIcons.tsx` | port `extensionImages.test.ts` (6) | todo |
| E-37 | Runtime status messages (rust, mixed, worker states, `extension-worker-status`) | `extensionRuntimeStatus.ts` | 13 tests | todo |
| E-38 | Hotkey-run extension view | A-24 | — | todo |
| E-39 | Launcher argument search (`extension:launcher-search`, unavailable event) | `CB:2300–2340` | L-20 | todo |
| E-40 | Nested view chrome shares header and footer geometry | `nestedViewChrome.test.ts` (9) | port 9 | todo |
| E-41 | Bun worker lifecycle (lazy start, framed IPC, deadlines, EOF cleanup, supervisor) | `nc/extension_worker.rs`, `src/main/extensionWorker/*` | `worker`; existing tests + real-Bun test | todo |
| E-42 | Native V2 extensions (process host APIs: clipboard, shell, selection, window, dialog, screen, apps, settings, ai, background, tray, notifications, safety, launcher publish) | `nc/extensions/process_host.rs` | existing Rust tests via TestHost | todo |
| E-43 | Native (Rust-ported) extension commands | `nc/extensions/native.rs` | existing | todo |
| E-44 | Native V2 CLI (`dispatch_native_v2_cli`) | `native_v2_cli.rs` | existing | todo |
| E-45 | Extension integrity/reinstall/install-error channels (no first-party UI caller; X-API) | core | not-reachable proof | todo |

## §T Terminal

| ID | Behaviour | Current owner | Acceptance | Status |
| --- | --- | --- | --- | --- |
| T-01 | Create session (login shell / PowerShell ConPTY, cwd, initial command) | `native_terminal.rs:1079` | existing Rust | todo |
| T-02 | Attach/detach with recent output replay | `:1091, 1139` | existing + port `terminalNativeTransport.test.ts` (4) | todo |
| T-03 | Write, resize (cols/rows from cell metrics) | `:1155, 1164` | *(new)* grid metrics test | todo |
| T-04 | cwd tracking (lsof / process) and persist on change | `:326–395` | existing | todo |
| T-05 | Kill, delete, prune history; save-for (day/week/month/forever), keep-alive (3h/8h/day/until-stop) | `:961–1023`, `terminalPreferences.ts` | existing + port | todo |
| T-06 | Session list, labels, pins, selection | `terminalSession*.ts` | L-17 | todo |
| T-07 | VT emulation, colours (light/dark palettes), cursor bar blink, scrollback bound | xterm.js `TerminalView.tsx:333–360` | *(new)* VT fixture tests via `alacritty_terminal` | todo |
| T-08 | Key handling (`attachCustomKeyEventHandler` rules), app shortcuts, bracketed paste | `TerminalView.tsx:364–420` | *(new)* key-sequence table | todo |
| T-09 | Text zoom ⌘+/−/0 with buttons | `TerminalView.tsx:277` | *(new)* | todo |
| T-10 | Selection, copy, URL open | xterm | *(new)* | todo |
| T-11 | IME composition into PTY | xterm | manual | todo |
| T-12 | `terminal:data`, `terminal:exit`, `terminal:sessions-changed` | core events | serde + ordering tests | todo |
| T-13 | Sessions panel window (W-09) and new session from panel | `TerminalSessionsWindow.tsx` | — | todo |
| T-14 | Shutdown all sessions on quit | `native_terminal::shutdown_all` | W-13 | todo |

## §U Utilities

| ID | Behaviour | Current owner | Acceptance | Status |
| --- | --- | --- | --- | --- |
| U-01 | Clipboard list, search, pinned section, image previews (`clipboard:image`), secure items | `ClipboardView.tsx` | *(new)* | todo |
| U-02 | Restore, delete, pin, reveal, clear history, `clipboard:changed` refresh | same | *(new)* | todo |
| U-03 | Save to Vault (draft) | same → S-13 | *(new)* | todo |
| U-04 | Convert chips: JSON, Base64, Encode, Hash (md5/sha256/sha512), Count, Case, QR; inline result; Copy result | `clipboardActions.tsx`, `shared/textTransform*.ts` | port `textTransform.test.ts` (8), `textTransformers.test.ts` (52) | todo |
| U-05 | QR image copy (`clipboard:write-png`) and Save PNG | `qrImage.ts`, `qr_download.rs` | port `qrImage.test.ts` (1); O-QR | todo |
| U-06 | Clipboard Unicode correctness | worker tests | existing | todo |
| U-07 | Snippets list (built-in + user), search, preview, placeholders (`${date}`, `${time}`, `${uuid}`…) | `SnippetsView.tsx` | *(new)* | todo |
| U-08 | Snippet create/edit/delete/copy, validation (title, trigger) | same | *(new)* | todo |
| U-09 | Notes list (title and summary derivation, dates), search, select by `createdAt` | `NotesView.tsx` | *(new)* port of title/summary helpers | todo |
| U-10 | Notes editor rich text subset, sanitize, autosave states (idle, saving, error) | same | *(new)* | todo |
| U-11 | Notes create (⌘N), delete (⌘D), copy (⌘↵) | same | *(new)* | todo |
| U-12 | Vault status (macOS only message on Windows), unlock (Touch ID / password), lock | `VaultView.tsx` | *(new)* | todo |
| U-13 | Vault list, search, create/edit (name, value, note, icon picker), delete, copy concealed (30 s clear) | same, `VaultIconPicker.tsx`, `vaultIcons.tsx` | port `VaultIconPicker.test.ts` (5), `vaultIcons.test.ts` (3) | todo |
| U-14 | Vault detection of secret-like clipboard values | `vaultDetect.ts` | port 4 tests | todo |
| U-15 | Emoji grid, fuzzy search (name, mood, slang), recent (`tezbar:recent-emojis`), copy | `EmojiPickerView.tsx`, `emoji/*` | *(new)* top-result fixtures from TS | todo |
| U-16 | Calculator (L-21) | — | — | todo |
| U-17 | Currency (L-22) | — | — | todo |
| U-18 | Colour converter (L-23) | — | — | todo |
| U-19 | Quick notes from launcher (`add-note` row “Add quick note: …”) | core | A-07 | todo |
| U-20 | Notes `notes:*` channels | core | existing | todo |
| U-21 | Snippet `snippets:*` channels | core | existing | todo |
| U-22 | Clipboard storage config (ST-23) | core | — | todo |
| U-23 | Text selection outside app (`core:platform:selected-text`) for AI action | core | existing | todo |
| U-24 | Generate password native command result card | A-10/L-25 | — | todo |
| U-25 | String cache utility | `shared/stringCache.ts` | port 2 tests (or replaced by `image_cache`) | todo |
| U-26 | System text selection behaviour in views (`systemTextSelection.test.ts`) | renderer CSS | port 3 cases as selectable-text rules | todo |
| U-27 | Markdown rendering (§5.7) | `ui/Markdown.tsx` | *(new)* corpus | todo |
| U-28 | Code block theme (light/dark) | `codeBlockTheme.test.ts` | port 2 | todo |
| U-29 | Icons: command glyph set, launcher icons, squircles | `CB:778–1090`, `launcherIcons.tsx` | screenshot review | todo |
| U-30 | Asset icon loading (`asset-icon:data-url`, `app-icon:data-url`), viewport-only requests, LRU | `CB:1093–1215` | prototype icon tests | todo |
| U-31 | Background tasks indicator (indexing, timer, update) with actions (open extension runtime, restart) | `BackgroundTaskStatus.tsx` | *(new)* | todo |
| U-32 | Hints/Kbd/Message primitives | `ui/primitives.tsx`, `ui/CommandSurface.tsx` | screenshot review | todo |
| U-33 | GlideList selection slide (optional ≤90 ms, off under reduced motion) | `ui/GlideList.tsx` | *(new)* | todo |

## §N Knowledge, Indexing, Deep Search

| ID | Behaviour | Current owner | Acceptance | Status |
| --- | --- | --- | --- | --- |
| N-01 | Indexing view: status (state colour as flat chip), progress, sources list with filter and paging (`knowledge:sources:list`), open file (`shell:open`), refresh, Edit in Settings | `IndexingView.tsx` | *(new)* | todo |
| N-02 | `knowledge:status` event updates | core | serde | todo |
| N-03 | Snapshot recovery and bounded loads (ST-22) | — | 60 cases | todo |
| N-04 | Embedding variants (built-in, Q4/Q8) download/cancel/remove/select | core + ST-22 | existing Rust + port | todo |
| N-05 | Roots add, add major, remove, enable, per-root depth | core | existing | todo |
| N-06 | Indexing speed, start/pause/resume | core | existing | todo |
| N-07 | Deep Search results in launcher (L-12) | — | — | todo |
| N-08 | Deep Search recommendation (L-13) | — | — | todo |
| N-09 | OCR helper usage (`screenocr-helper`) for images | core | existing | todo |
| N-10 | Choose folder dialog (W-14) | — | — | todo |
| N-11 | Onboarding DeepSearch step (O-05) | — | — | todo |
| N-12 | Knowledge startup failure and repair flows | core + ST-22 | existing + port | todo |

## §Y System, updater and misc

| ID | Behaviour | Current owner | Acceptance | Status |
| --- | --- | --- | --- | --- |
| Y-01 | System monitor stats (CPU, load, cores, memory, swap, compressed, storage, GPU, display, battery charge/capacity/condition/cycles, fans, temperature, Wi-Fi, local IP, network rates, processes, uptime, Low Power Mode); refresh | `SystemMonitorView.tsx`, `system-stats:get` | *(new)* formatting tests | todo |
| Y-02 | Permissions snapshot and request (accessibility, automation, input-monitoring, microphone, calendar, screen-recording) | `PermissionsView.tsx`, core | existing | todo |
| Y-03 | Release notes fetch (moves from renderer `fetch` to Rust), unread tracking | `releaseNotes.ts` | 13 tests | todo |
| Y-04 | Update check at startup (+3 s host), renderer auto-check (+5 s, 4 h), auto-install preference | `lib:3253`, `App.tsx:426–460`, `shared/updater.ts` | `updater.test.ts` (28) | todo |
| Y-05 | Update states and events, manual-available installer link, restart | `updater.rs` | existing Rust | todo |
| Y-06 | Update channel get/set (stable, beta, alpha) | `updater.rs`, `release_channels.rs` | existing | todo |
| Y-07 | Safety: descriptors, dry-run, log (log UI has no caller, X-API) | core | existing | todo |
| Y-08 | Pinned commands (L-28) | — | — | todo |
| Y-09 | Recents (A-22) | — | — | todo |
| Y-10 | Background tasks (U-31) | — | — | todo |
| Y-11 | Voice: TTS speak, STT transcribe channel (no UI caller beyond Settings model management) | core | existing | todo |
| Y-12 | GitHub-only external URL opener (`open-external-url`) | core | existing | todo |
| Y-13 | Frankfurter rates fetch | core | existing | todo |
| Y-14 | Search benchmark channels (no UI caller) | core | not-reachable | todo |
| Y-15 | Idle memory trim incl. image caches (`tezbar-idle-trim`) | `main.tsx:112–126`, `lib:282–305` | W-04 + image cache trim test | todo |
| Y-16 | Theme sync across windows and system appearance | `main.tsx:28–44`, `theme.ts` | ST-08 | todo |
| Y-17 | App version source and display | `package.json`, `releaseNotes.ts` | `packaging/app-identity.json` | todo |

## §I Core → UI events

`agent:event`, `ai-extensions-changed`, `app:open-surface`, `backend-status`,
`clipboard:changed`, `command-error`, `extension-worker-status`, `extension:install-progress`
(forwarded from the worker), `extension:launcher-unavailable`, `extension:session-invalidated`,
`extensions-catalog-updated`, `extensions-changed`, `knowledge:status`, `launcher-layout-changed`,
`launcher-viewport-changed`, `launcher:invalidated`, `llm-config-changed`,
`run-extension-command-from-hotkey`, `settings-close-requested`, `stream-done`,
`stream-error`, `stream-token`, `terminal:data`, `terminal:exit`,
`terminal:sessions-changed`, `update-status`, `voice:model:progress`, `window-hidden`,
`window-shown`, `window-visibility`, `window:snap-guides`. **31 events.**
`notes:quick-save-shortcut` has a listener but no emitter (§X). Host-only events
(`launcher-*`, `window*`, `settings-close-requested`, `backend-status`, `update-status`) become
direct in-app messages. Core events decode in `app/events.rs` into `CoreEvent`. A serde
round-trip test per event uses payloads captured from the existing Rust tests.

## §P Renderer-owned persistent keys → `ui-state-v1.json`

| ID | Key | Shape | Import rule |
| --- | --- | --- | --- |
| P-01 | `tezbar:pinned-commands` | `PinnedCommand[]` ≤12 | normalize slots |
| P-02 | `tezbar:recent-extension-commands` | `string[]` ≤20 | keep order |
| P-03 | `tezbar:launcher-query-history:v1` | history list | `parseLauncherQueryHistory` |
| P-04 | `tezbar:home-search-results:v2` | cache | **not imported** (rebuilt) |
| P-05 | `tezbar:search-candidates:v2` | cache | **not imported** |
| P-06 | `tezbar:terminal-defaults` | `{saveFor, keepAliveFor}` | validate enums |
| P-07 | `tezbar:terminal-pinned-sessions` | `string[]` | keep |
| P-08 | `tezbar:recent-emojis` | `string[]` | keep limit |
| P-09 | `tezbar:currency:prefs:v1` | prefs | validate |
| P-10 | `tezbar:theme` | `white\|dark\|system` | default system |
| P-11 | `tezbar:release-notes-version` | version | keep |
| P-12 | `tezbar:last-update-check` | ms | keep |
| P-13 | `tezbar:auto-update-enabled` | `'true'\|'false'` | bool |
| P-14 | `tezbar:settings-tab` | tab | normalize |
| P-15 | (window positions and layout profiles) | already native (`config.json` / `launcher-layouts-v1.json`) | none |

## §B First-party TypeScript logic moving to Rust (32 groups)

`calculator.ts`; `colorConverter.ts`; `currency/{currencyConversion,currencyPreferences,currencySynonyms,frankfurter,parseCurrencyQuery}.ts` + `hooks/useCurrencyConversion.ts`;
`shared/textTransform.ts`; `shared/textTransformers.ts`; `qrImage.ts`; `emoji/{emojiData,emojiSearch}.ts`;
`shared/{searchMode,searchQueryVariants,applicationSearch}.ts`; `optimisticSearch.ts` (**not ported**:
in-process core makes it redundant; first-frame home cache retained; Decision, latency measured in §9.4);
`latestSearch.ts`; `commandBarInputMode.ts`; `launcherArgument.ts`; `escapeGate.ts`;
`newItemShortcut.ts`; `pinnedCommands.ts`; `launcherQueryHistory.ts`; `releaseNotes.ts`;
`shared/updater.ts`; `settingsLifecycle.ts` (+ SettingsView merge logic); `useKnowledgeSettings.ts`;
`agentChat/{transport,streaming,model,historyCache,timeline}.ts` + `shared/chatMeta.ts`, `shared/llmErrors.ts`;
`extensionCatalogState.ts`; `extensionRuntimeStatus.ts`; `extensionSessionRefresh.ts`;
`extensionPreferences.ts`; `extensionImages.ts`; `terminalSession{Selection,Label,History}.ts` + `terminalPreferences.ts`;
`vault{Detect,Copy}.ts` + `vaultIcons.tsx`; `windowDrag.ts` + `launcherViewport.ts`; `theme.ts`;
`visibleEventRefresh.ts`; `shared/aiProviders.ts` (replaced by the core catalog).
Test mapping lives in `tests/PORT_MAP.md` (created at M3).

TypeScript that **stays**: `src/main/**` (Bun worker), the `src/shared` files the worker and
scripts import (`extensionRuntime.ts`, `extensions.ts`, `extensionRepository.ts`,
`lovedExtensions.ts`, `applicationSearch.ts` if still imported by the worker, plus types they
need), `scripts/**`, `tests/manual/**`, and the TS tests of retained modules.

## §X Not reachable, pre-existing discrepancies, deliberate visual deviations

| ID | Item | Proof | Disposition |
| --- | --- | --- | --- |
| X-01 | `ProvidersView.tsx` | import walk; `open-providers` routes to Settings AI | not ported |
| X-02 | `Sidebar.tsx` | import walk | not ported |
| X-03 | `hooks/useHoldToSpeak.ts` | import walk | not ported; Push to Talk row is U3 |
| X-04 | `agentChat/timeline.ts` | import walk | rules kept as tests (C-15) |
| X-05 | `currency/regionToCurrency.ts` | import walk | not ported |
| X-06 | `shared/ipc.ts` | import walk | not ported |
| X-07 | 26 bridge members with no renderer caller (spec §10.4; table §H) | exact word search | core channels kept; no UI |
| X-08 | `onQuickNoteSaveShortcut` (`notes:quick-save-shortcut`) | `rg` finds no emitter | not ported |
| X-09 | `resetUi: true` branch | every emitter sends `false` | not ported; ST-06 is U3 |
| X-10 | “Launch Tezbar at login” | `rg autostart` = 0, read-only checkbox | U3 |
| X-11 | `raycast-api/hooks.ts` renderer hooks (`useFetch`, `usePromise`…) | extension code never runs in the renderer | not ported |
| X-12 | `window_set_content_height` / `setWindowContentHeight` | no-op in Rust | not ported |
| X-13 | Tauri `native_input` commands (`move_mouse`, `click`, …) exposed on the bridge | no renderer caller; core uses `native_input` directly | functions kept in `tezbar-host::input`; commands removed |
| X-14 | Flattened decorations: `.glass-*` gradients/blur, Knowledge banner gradient (`SV:2953`), gradient progress (`SV:3268`), Kbd glow (`SV:3618`), coloured stat tiles | spec §5.6 | deliberate deviation, Opus review |
| X-15 | Pinned-row amber side bar | prototype spec §2.8 | trailing pin glyph instead |
| X-16 | List runtime selection by id (was index only) | spec §4.4 | deliberate behaviour improvement, tested |

## §Q Platforms

| ID | Platform | Status |
| --- | --- | --- |
| Q-01 | macOS arm64 | ship target |
| Q-02 | Windows x64 | ship target |
| Q-03 | macOS x86_64 | updater path exists, no artifact, unbuilt with GPUI; not planned |
| Q-04 | Windows arm64 | listed as a goal in WINDOWS_SUPPORT, no build; not planned |
| Q-05 | Linux | core `cfg(unix)` fallbacks, GPUI supports it, no tray/hotkey/packaging; not planned |
| Q-06 | Web / mobile | out of scope |

## §H Host commands and bridge API

### Tauri commands (51) → new owner

| Command | New owner |
| --- | --- |
| `call_backend` | `app/core_client.rs` → `NativeCore::request` (in process; per-channel timeouts kept: default 30 s, install 15 min, runtime 2 min) |
| `get_backend_status`, `restart_backend` | `um::launcher::core_status` |
| `open_settings_window_cmd`, `hide_launcher_for_settings`, `close_current_window`, `finish_settings_close`, `ack_settings_close` | `app/windows/settings.rs` |
| `terminal_sessions_show`, `terminal_sessions_hide`, `terminal_sessions_sync` | `app/windows/terminal_sessions.rs` |
| `open_extensions_window` | router (S-05) |
| `toggle_window`, `hide_window`, `show_window` | `GpuiHost` + `app/windows/launcher.rs` |
| `quit_app` | `app/app.rs` (W-13) |
| `start_window_snap_drag`, `end_window_snap_drag`, `get_window_snap_guides` | `tezbar-host::placement::snap` + `app/windows/snap_overlay.rs` |
| `set_suppress_blur_hide` | `GpuiHost` |
| `set_quick_look_window_state` | `GpuiHost` (W-19) |
| `window_set_content_height` | removed (X-12) |
| `window_layout::{get_launcher_layout,set_launcher_layout,start_launcher_resize}` | `tezbar-host::layout` |
| `update_raymes_shortcut` | `core:platform:update-launcher-hotkey` (existing) |
| `native_input::{move_mouse,click,double_click,press_key,type_text,scroll,screenshot,is_physical_key_down}` | `tezbar-host::input` (functions), commands removed (X-13); `screenshot` → `Host::capture_active_display` |
| `native_terminal::{create,attach,cwd,detach,write,resize,kill,delete_history,prune_history}` | `tezbar-core::terminal` channels |
| `updater::{get_update_status,get_update_channel,set_update_channel,check_for_updates,download_and_install_update,restart_app,open_release_page}` | `tezbar-update` |
| `qr_download::save_qr_png_data_url` | `Window::prompt_for_new_path` + write PNG (U-05) |

### Bridge members (206): call target → new owner

| Member | Current target(s) | New owner | Renderer caller |
| --- | --- | --- | --- |
| hide | `invoke:hide_window` | GpuiHost / windows/* | used |
| show | `invoke:show_window` | GpuiHost / windows/* | used |
| openSettingsWindow | `invoke:open_settings_window_cmd` | GpuiHost / windows/* | used |
| hideLauncherForSettings | `invoke:hide_launcher_for_settings` | GpuiHost / windows/* | used |
| closeCurrentWindow | `invoke:close_current_window` | GpuiHost / windows/* | used |
| onWindowCloseRequested | `settings-close-requested` handshake | windows/settings close handshake | used |
| query | `query` | core_client → same tezbar-core channel | used |
| cancel | `cancel` | core_client → same tezbar-core channel | used |
| getExtensions | `get-extensions` | core_client → same tezbar-core channel | no caller (X-07) |
| listInstalledExtensions | `extensions:listInstalled` | core_client → same tezbar-core channel | no caller (X-07) |
| searchStoreExtensions | `extensions:searchStore` | core_client → same tezbar-core channel | no caller (X-07) |
| installExtension | `extensions:install` | core_client → same tezbar-core channel | no caller (X-07) |
| uninstallExtension | `extensions:uninstall` | core_client → same tezbar-core channel | no caller (X-07) |
| inspectExtension | `extensions:integrity` | core_client → same tezbar-core channel | no caller (X-07) |
| reinstallExtension | `extensions:reinstall` | core_client → same tezbar-core channel | no caller (X-07) |
| getExtensionInstallError | `extensions:install-error` | core_client → same tezbar-core channel | no caller (X-07) |
| extensionList | `extension:list` | core_client → same tezbar-core channel | used |
| extensionSearchStore | `extension:search-store` | core_client → same tezbar-core channel | used |
| extensionInstall | `extension:install` | core_client → same tezbar-core channel | used |
| extensionUninstall | `extension:uninstall` | core_client → same tezbar-core channel | used |
| extensionRunCommand | `extension:run-command` | core_client → same tezbar-core channel | used |
| extensionInvokeAction | `extension:invoke-action` | core_client → same tezbar-core channel | used |
| extensionSearchTextChanged | `extension:search-text-changed` | core_client → same tezbar-core channel | used |
| extensionRefreshSession | `extension:refresh-session` | core_client → same tezbar-core channel | used |
| onExtensionSessionInvalidated | `event:extension:session-invalidated` | apps/tezbar events.rs (typed CoreEvent) | used |
| extensionDisposeSession | `extension:dispose-session` | core_client → same tezbar-core channel | used |
| extensionLoadMore | `extension:load-more` | core_client → same tezbar-core channel | used |
| clipboardReadText | `clipboard:read` | core_client → same tezbar-core channel | used |
| clipboardWriteText | `clipboard:write` | core_client → same tezbar-core channel | used |
| clipboardWritePng | `clipboard:write-png` | core_client → same tezbar-core channel | used |
| saveQrPngDataUrl | `invoke:save_qr_png_data_url` | tezbar-host capture / Window::prompt_for_new_path | used |
| shellOpen | `shell:open` | core_client → same tezbar-core channel | used |
| getAppIconDataUrl | `app-icon:data-url` | core_client → same tezbar-core channel | used |
| getAssetIconDataUrl | `asset-icon:data-url` | core_client → same tezbar-core channel | used |
| getExtensionPreferences | `preferences:get` | core_client → same tezbar-core channel | used |
| getExtensionPreferenceSetup | `preferences:setup` | core_client → same tezbar-core channel | used |
| saveExtensionPreferences | `preferences:set` | core_client → same tezbar-core channel | used |
| extensionLauncherSearch | `extension:launcher-search` | core_client → same tezbar-core channel | used |
| onLauncherInvalidated | `event:launcher:invalidated` | apps/tezbar events.rs (typed CoreEvent) | used |
| searchAll | `search:all` | core_client → same tezbar-core channel | used |
| listSearchCandidates | `search:candidates` | core_client → same tezbar-core channel | used |
| completePath | `path:complete` | core_client → same tezbar-core channel | used |
| resolveDirectory | `path:resolve-directory` | core_client → same tezbar-core channel | used |
| recordDirectoryVisit | `directory-visit:record` | core_client → same tezbar-core channel | used |
| runSearchBenchmark | `search:benchmark:run` | core_client → same tezbar-core channel | no caller (X-07) |
| getSearchBenchmarkHistory | `search:benchmark:history` | core_client → same tezbar-core channel | no caller (X-07) |
| quickLookFiles | `file:quick-look`, `invoke:set_quick_look_window_state` | GpuiHost / windows/* | used |
| executeSearchAction | `search:execute` | core_client → same tezbar-core channel | used |
| recordSearchActionUsage | `search:record-usage` | core_client → same tezbar-core channel | used |
| runAiAction | `ai:action` | core_client → same tezbar-core channel | no caller (X-07) |
| voiceSpeak | `voice:tts:speak` | core_client → same tezbar-core channel | used |
| voiceStop | `voice:tts:stop` | core_client → same tezbar-core channel | no caller (X-07) |
| voiceTranscribe | `voice:stt:transcribe` | core_client → same tezbar-core channel | used |
| setSuppressBlurHide | `invoke:set_suppress_blur_hide`, `window:suppress-blur-hide` | GpuiHost SuppressBlurHide | used |
| listVoiceSttModes | `voice:stt:modes` | core_client → same tezbar-core channel | no caller (X-07) |
| listVoiceModels | `voice:models:list` | core_client → same tezbar-core channel | used |
| downloadVoiceModel | `voice:model:download` | core_client → same tezbar-core channel | used |
| deleteVoiceModel | `voice:model:delete` | core_client → same tezbar-core channel | used |
| getSelectedVoiceModel | `voice:model:get-selected` | core_client → same tezbar-core channel | used |
| setSelectedVoiceModel | `voice:model:set-selected` | core_client → same tezbar-core channel | used |
| onStreamToken | `event:stream-token` | apps/tezbar events.rs (typed CoreEvent) | used |
| onStreamDone | `event:stream-done` | apps/tezbar events.rs (typed CoreEvent) | used |
| onStreamError | `event:stream-error` | apps/tezbar events.rs (typed CoreEvent) | used |
| getLlmConfig | `llm-config-get` | core_client → same tezbar-core channel | used |
| setLlmConfig | `llm-config-set` | core_client → same tezbar-core channel | used |
| getLlmProviderStatuses | `llm-provider-statuses` | core_client → same tezbar-core channel | used |
| listPiExtensions | `pi-extensions:list` | core_client → same tezbar-core channel | used |
| listLlmModels | `llm-list-models` | core_client → same tezbar-core channel | used |
| getWindowZoomFactor | cached layout zoom | tezbar-host::layout + windows/launcher | used |
| getLauncherLayout | `invoke:get_launcher_layout` | tezbar-host::layout + windows/launcher | used |
| setLauncherLayout | `invoke:set_launcher_layout` | tezbar-host::layout + windows/launcher | used |
| onLauncherLayoutChanged | `event:launcher-layout-changed` | tezbar-host::layout + windows/launcher | used |
| onLauncherViewportChanged | `event:launcher-viewport-changed` | tezbar-host::layout + windows/launcher | used |
| startLauncherResize | `invoke:start_launcher_resize` | tezbar-host::layout + windows/launcher | used |
| setWindowContentHeight | `invoke:window_set_content_height` | GpuiHost / windows/* | no caller (X-07) |
| openExternalUrl | `open-external-url` | core_client → same tezbar-core channel | used |
| githubDeviceStart | `github-device-start` | core_client → same tezbar-core channel | used |
| githubDevicePoll | `github-device-poll` | core_client → same tezbar-core channel | used |
| githubDeviceCancel | `github-device-cancel` | core_client → same tezbar-core channel | used |
| onWindowShown | `event:window-shown` | apps/tezbar events.rs (typed CoreEvent) | used |
| onWindowHidden | `event:window-hidden` | apps/tezbar events.rs (typed CoreEvent) | used |
| onWindowVisibility | `event:window-visibility` | apps/tezbar events.rs (typed CoreEvent) | used |
| startWindowSnapDrag | `invoke:start_window_snap_drag` | GpuiHost / windows/* | used |
| endWindowSnapDrag | `invoke:end_window_snap_drag` | GpuiHost / windows/* | used |
| getWindowSnapGuides | `invoke:get_window_snap_guides` | GpuiHost / windows/* | used |
| onWindowSnapGuides | `event:window:snap-guides` | apps/tezbar events.rs (typed CoreEvent) | used |
| getPermissions | `permissions:snapshot` | core_client → same tezbar-core channel | used |
| requestPermission | `permissions:request` | core_client → same tezbar-core channel | used |
| checkPiAgent | `pi:check` | core_client → same tezbar-core channel | no caller (X-07) |
| installPiAgent | `pi:install` | core_client → same tezbar-core channel | no caller (X-07) |
| getSafetyDescriptors | `safety:descriptors` | core_client → same tezbar-core channel | no caller (X-07) |
| getSafetyLog | `safety:log` | core_client → same tezbar-core channel | no caller (X-07) |
| clearSafetyLog | `safety:log-clear` | core_client → same tezbar-core channel | no caller (X-07) |
| getSafetyDryRun | `safety:dry-run:get` | core_client → same tezbar-core channel | used |
| setSafetyDryRun | `safety:dry-run:set` | core_client → same tezbar-core channel | used |
| getNativeCommands | `native-commands:list` | core_client → same tezbar-core channel | no caller (X-07) |
| getSystemStats | `system-stats:get` | core_client → same tezbar-core channel | used |
| listClipboardEntries | `clipboard:list` | core_client → same tezbar-core channel | used |
| restoreClipboardEntry | `clipboard:restore` | core_client → same tezbar-core channel | used |
| deleteClipboardEntry | `clipboard:delete` | core_client → same tezbar-core channel | used |
| toggleClipboardPin | `clipboard:toggle-pin` | core_client → same tezbar-core channel | used |
| revealClipboardEntry | `clipboard:reveal` | core_client → same tezbar-core channel | used |
| readClipboardImage | `clipboard:image` | core_client → same tezbar-core channel | used |
| clearClipboardHistory | `clipboard:clear` | core_client → same tezbar-core channel | used |
| onClipboardChanged | `event:clipboard:changed` | apps/tezbar events.rs (typed CoreEvent) | used |
| listSnippets | `snippets:list` | core_client → same tezbar-core channel | used |
| copySnippet | `snippets:copy` | core_client → same tezbar-core channel | used |
| addSnippet | `snippets:add` | core_client → same tezbar-core channel | used |
| updateSnippet | `snippets:update` | core_client → same tezbar-core channel | used |
| deleteSnippet | `snippets:delete` | core_client → same tezbar-core channel | used |
| fetchFrankfurterLatest | `currency:frankfurter-latest` | core_client → same tezbar-core channel | used |
| listQuickNotes | `notes:list` | core_client → same tezbar-core channel | used |
| appendQuickNote | `notes:append` | core_client → same tezbar-core channel | used |
| updateQuickNote | `notes:update` | core_client → same tezbar-core channel | used |
| deleteQuickNote | `notes:delete` | core_client → same tezbar-core channel | used |
| vaultStatus | `vault:status` | core_client → same tezbar-core channel | used |
| vaultUnlock | `vault:unlock`, `invoke:show_window` | GpuiHost / windows/* | used |
| vaultLock | `vault:lock` | core_client → same tezbar-core channel | used |
| vaultList | `vault:list` | core_client → same tezbar-core channel | used |
| vaultGet | `vault:get` | core_client → same tezbar-core channel | used |
| vaultSave | `vault:save` | core_client → same tezbar-core channel | used |
| vaultDelete | `vault:delete` | core_client → same tezbar-core channel | used |
| vaultCopy | `vault:copy` | core_client → same tezbar-core channel | used |
| vaultReset | `vault:reset`, `invoke:show_window` | GpuiHost / windows/* | no caller (X-07) |
| terminalCreate | `invoke:native_terminal_create` | core `terminal` channel via core_client | used |
| terminalAttach | `invoke:native_terminal_attach` | core `terminal` channel via core_client | used |
| terminalDetach | `invoke:native_terminal_detach` | core `terminal` channel via core_client | used |
| terminalList | `terminal:list` | core_client → same tezbar-core channel | used |
| terminalUpdate | `terminal:update` | core_client → same tezbar-core channel | used |
| terminalWrite | `invoke:native_terminal_write` | core `terminal` channel via core_client | used |
| terminalResize | `invoke:native_terminal_resize` | core `terminal` channel via core_client | used |
| terminalGetCwd | `invoke:native_terminal_cwd` | core `terminal` channel via core_client | used |
| terminalKill | `invoke:native_terminal_kill` | core `terminal` channel via core_client | used |
| terminalDelete | `terminal:delete` | core_client → same tezbar-core channel | used |
| getTerminalPromptInfo | `terminal:get-prompt-info` | core_client → same tezbar-core channel | used |
| terminalSessionsShow | `invoke:terminal_sessions_show` | GpuiHost / windows/* | no caller (X-07) |
| terminalSessionsHide | `invoke:terminal_sessions_hide` | GpuiHost / windows/* | used |
| terminalSessionsSync | `invoke:terminal_sessions_sync` | GpuiHost / windows/* | no caller (X-07) |
| terminalSessionsAction | `emitTo:terminal-sessions:action` | in-app message (windows/terminal_sessions → launcher) | used |
| onTerminalSessionsAction | `event:terminal-sessions:action` | apps/tezbar events.rs (typed CoreEvent) | used |
| onTerminalSessionsChanged | `event:terminal:sessions-changed` | apps/tezbar events.rs (typed CoreEvent) | used |
| onTerminalData | `event:terminal:data` | apps/tezbar events.rs (typed CoreEvent) | used |
| onTerminalExit | `event:terminal:exit` | apps/tezbar events.rs (typed CoreEvent) | used |
| getStorageBreakdown | `storage:breakdown` | core_client → same tezbar-core channel | used |
| getClipboardStorageConfig | `storage:clipboard-config:get` | core_client → same tezbar-core channel | used |
| setClipboardStorageConfig | `storage:clipboard-config:set` | core_client → same tezbar-core channel | used |
| clearClipboardImages | `storage:clear-clipboard-images` | core_client → same tezbar-core channel | used |
| vacuumSearchDatabase | `storage:vacuum-search-db` | core_client → same tezbar-core channel | used |
| clearChromiumCache | `storage:clear-chromium-cache` | core_client → same tezbar-core channel | used |
| listBackgroundTasks | `background-tasks:list` | core_client → same tezbar-core channel | used |
| getKnowledgeSnapshot | `knowledge:snapshot` | core_client → same tezbar-core channel | used |
| setKnowledgeEmbeddingModel | `knowledge:embedding:set` | core_client → same tezbar-core channel | used |
| downloadKnowledgeEmbeddingModel | `knowledge:embedding:download` | core_client → same tezbar-core channel | used |
| cancelKnowledgeEmbeddingDownload | `knowledge:embedding:cancel-download` | core_client → same tezbar-core channel | used |
| removeKnowledgeEmbeddingModel | `knowledge:embedding:remove` | core_client → same tezbar-core channel | used |
| listKnowledgeSources | `knowledge:sources:list` | core_client → same tezbar-core channel | used |
| chooseKnowledgeFolder | `knowledge:choose-folder` | core_client → same tezbar-core channel | used |
| addKnowledgeRoot | `knowledge:root:add` | core_client → same tezbar-core channel | used |
| addMajorKnowledgeRoots | `knowledge:roots:add-major` | core_client → same tezbar-core channel | used |
| removeKnowledgeRoot | `knowledge:root:remove` | core_client → same tezbar-core channel | used |
| setKnowledgeRootEnabled | `knowledge:root:set-enabled` | core_client → same tezbar-core channel | used |
| setKnowledgeDepth | `knowledge:depth:set` | core_client → same tezbar-core channel | used |
| setKnowledgeRootDepth | `knowledge:root:set-depth` | core_client → same tezbar-core channel | used |
| setKnowledgeIndexingSpeed | `knowledge:speed:set` | core_client → same tezbar-core channel | used |
| startKnowledgeIndexing | `knowledge:index:start` | core_client → same tezbar-core channel | used |
| pauseKnowledgeIndexing | `knowledge:index:pause` | core_client → same tezbar-core channel | used |
| resumeKnowledgeIndexing | `knowledge:index:resume` | core_client → same tezbar-core channel | used |
| onKnowledgeStatus | `event:knowledge:status` | apps/tezbar events.rs (typed CoreEvent) | used |
| onQuickNoteSaveShortcut | `event:notes:quick-save-shortcut` | apps/tezbar events.rs (typed CoreEvent) | no caller (X-07) |
| onAppSurfaceOpen | `event:app:open-surface` | apps/tezbar events.rs (typed CoreEvent) | used |
| aiAccounts | `ai-accounts:antigravity:${action}` | core_client → same tezbar-core channel | used |
| onAiExtensionsChanged | `event:ai-extensions-changed` | apps/tezbar events.rs (typed CoreEvent) | used |
| aiExtensions | `ai-extensions:${action}` | core_client → same tezbar-core channel | used |
| agentRun | `agent:run` | core_client → same tezbar-core channel | used |
| captureActiveScreen | `invoke:screenshot` | tezbar-host capture / Window::prompt_for_new_path | used |
| agentCancel | `agent:cancel` | core_client → same tezbar-core channel | used |
| agentApprove | `agent:approve` | core_client → same tezbar-core channel | used |
| onAgentEvent | `event:agent:event` | apps/tezbar events.rs (typed CoreEvent) | used |
| onExtensionInstallProgress | `event:extension:install-progress` | apps/tezbar events.rs (typed CoreEvent) | used |
| onExtensionCatalogUpdated | `event:extensions-catalog-updated` | apps/tezbar events.rs (typed CoreEvent) | used |
| onLlmConfigChanged | `event:llm-config-changed` | apps/tezbar events.rs (typed CoreEvent) | used |
| chatRun | `chat:run` | core_client → same tezbar-core channel | no caller (X-07) |
| chatList | `chat:list` | core_client → same tezbar-core channel | used |
| chatGet | `chat:get` | core_client → same tezbar-core channel | used |
| chatAppend | `chat:append` | core_client → same tezbar-core channel | used |
| chatUpdateTitle | `chat:update-title` | core_client → same tezbar-core channel | used |
| chatGenerateMeta | `chat:generate-meta` | core_client → same tezbar-core channel | used |
| chatDeleteTurn | `chat:delete-turn` | core_client → same tezbar-core channel | used |
| chatDelete | `chat:delete` | core_client → same tezbar-core channel | used |
| chatClear | `chat:clear` | core_client → same tezbar-core channel | no caller (X-07) |
| openExtensionStore | `invoke:open_extensions_window` | GpuiHost / windows/* | used |
| getInstalledExtensionsSettingsSchema | `get-installed-extensions-settings-schema` | core_client → same tezbar-core channel | used |
| updateCommandHotkey | `update-command-hotkey` | core_client → same tezbar-core channel | used |
| toggleCommandEnabled | `toggle-command-enabled` | core_client → same tezbar-core channel | used |
| getSettings | `get-settings` | core_client → same tezbar-core channel | used |
| saveSettings | `save-settings` | core_client → same tezbar-core channel | used |
| onRunExtensionCommandFromHotkey | `event:run-extension-command-from-hotkey` | apps/tezbar events.rs (typed CoreEvent) | used |
| appQuit | `app:confirm-quit`, `invoke:quit_app` | GpuiHost / windows/* | used |
| getUpdateStatus | `invoke:get_update_status` | tezbar-update + ui-model::update_policy | used |
| getUpdateChannel | `invoke:get_update_channel` | tezbar-update + ui-model::update_policy | used |
| setUpdateChannel | `invoke:set_update_channel` | tezbar-update + ui-model::update_policy | used |
| checkForUpdates | `invoke:check_for_updates` | tezbar-update + ui-model::update_policy | used |
| downloadAndInstallUpdate | `invoke:download_and_install_update`, `invoke:open_release_page` | tezbar-update + ui-model::update_policy | used |
| restartApp | `invoke:restart_app` | tezbar-update + ui-model::update_policy | used |
| openReleasePage | `invoke:open_release_page` | tezbar-update + ui-model::update_policy | used |
| onUpdateStatus | `event:update-status` | tezbar-update + ui-model::update_policy | used |
