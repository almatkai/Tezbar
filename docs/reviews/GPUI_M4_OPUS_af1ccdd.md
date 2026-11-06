# Tezbar GPUI M4 repairs — fresh Opus 5.5 closure review at `af1ccdd`

- Reviewer: Claude Opus 5.5, one agent, no subagents or delegation.
- Worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-gpui-app`. I used it read-only.
- Pin: `af1ccddc49f8efc14e0d498b04d262ead1b6c290`. The later API commit `9dcf23d` is outside this review.
- Method: I extracted a frozen `git archive af1ccdd` into the session scratchpad, with no `.git`. I read all source from that copy and from `git show`/`git diff 82688f7..af1ccdd`. I did not read live working files.
  - No checkout, worktree, branch, edit, stage, commit, push or build.
  - No app launch, CUA, OS clipboard, profile, credential or provider use.
  - The original report and all evidence files are untouched.
- Scope: closure of M4-01…M4-23 and the shared M5 boundaries that consume M4 APIs:
  - `windows/launcher.rs` re-export of `command_bar`;
  - `CoreEvents::subscribe`;
  - paint-time pointer capture;
  - `drain_state`/`state_write_error`;
  - `CoreClient` request ownership.
- Out of scope: missing M6–M10 screens are not M4 defects, and this is not a parity verdict for M5–M11.

## 1. Verdict

**M4 foundation repair scope: NOT CLOSED YET. One Medium and five Low actionable findings remain. All are small and local.**

- **21 of 23 original findings are closed** in source at the pin. I checked each against real callers, not only the ledger.
- **M4-20 is partially open** (accessibility node structure, N-03).
- **M4-22 is reopened** by a token regression the repair introduced (N-02), plus a §5.8 contrast-test gap (N-04).
- The main new defect is **N-01 (Medium)**: one failed launcher write keeps cancelling every later quit. It sits in the cross-module path between the M4 quit gate and the M5 `drain_state` contract.
- The properties the original review found sound are still intact:
  - one GUI-thread owner and one Tokio runtime;
  - the capture lease;
  - the macOS `applicationShouldTerminate:` ABI and replies outside the borrow;
  - preview isolation;
  - image texture ownership.
- Manual gates stay open: physical focus, sheets, snap and resize, mixed DPI, IME, VoiceOver/Narrator, Windows GUI, screenshots and GPU pressure. **This review is not a full-migration verdict.**

## 2. Evidence and coverage

| Item | Result |
| --- | --- |
| Original report | `docs/reviews/GPUI_M4_OPUS_82688f7.md` at the pin: SHA-256 `546c2a20…1273`, **match**. Read in full; all 23 IDs re-checked. |
| Repair docs | Read `docs/native-m04/{review-repairs.md, foundation-review-repair-evidence.md/.json, parent-repair-check.md}`. Historical `foundation.md` and its map are unchanged and not relabelled. |
| Spec / ledger | Spec §3.4, §3.5, §5.2–5.8 and §6.4 re-read for the claims checked. Ledger W-01…W-20 also re-read. `MACOS_SIGNING.md` is unaffected (no identity or signing change). |
| Source hashes | Repair map: **25/25** `source_sha256` match the frozen archive. Parent `source-after.json`: **35/35** match. I also read `apps/tezbar/src/lib.rs` (adds `surfaces`) and the 2-line `windows/launcher.rs` re-export, which are not in the map. |
| Proof artifacts | **14/14** `proof_artifacts` hashes match under `/tmp/tezbar-native-m04-repair-evidence/`. Clean binary `/tmp/tezbar-native-m04-parent-repair-evidence/tezbar-af1ccdd` hashes to `83bd72c0…3ff0`, which matches `clean-normal-receipt.json`: source `af1ccdd`, `dirty:false`, `bench_control:false`, rustc `48a229ce`, GPUI `c84af5b`, `runtime_shaders:true`, `no_webkit:true`. `full-bench-ui.log` reports 57 + 37 passed and 0 failed. These are the parent's results, not mine. |
| M5 contracts read | `surfaces/command_bar/{mod,view,interactions}.rs`, as far as they consume M4 APIs: subscription, request adapter, pointer capture, `SnapBegin`/`SnapEnd`, persistence/`drain_state`, `action_button`, `controls::footer`, text-input keys. |
| GPUI upstream (`c84af5b`) | `elements/div.rs:3078-3140`: the keyboard click records the key-down and fires once on key-up, fenced by focus generation. `gpui_macos/window.rs:2189` (`start_window_move` uses `currentEvent`). `gpui_windows/window.rs:580` (`focus:false` → `SW_SHOWNOACTIVATE`) and `display.rs` (logical bounds). Tokio 1.52.3 `block_in_place` (`multi_thread/worker.rs:359-439`); blocking-pool cap = `max_blocking + workers` (`builder.rs:1865`). |
| My probes and builds | **0 builds, 0 app launches, 0 GUI probes.** Every finding below follows from source. One bounded XERJ query against stale prefix `ax-65b00e69da24`: it returned only legacy TS/Tauri hits, so I did not reindex and used `rg`/`git show`. |
| Not my results | All test counts, the three Cocoa fixtures (0.4 s hold plus second instance, layout-save failure and retry, 8.051998292 s core-stop exit), and the clean build are recorded parent evidence. |

On the failed M5 full bench: when `block_in_place` is removed (the negative `red-event-bridge.py` mutant), both Tokio workers block on the Condvar. Multi-thread `block_on` threads do not drive the timer, so `tokio::time::sleep` in `closing_owned_queues…` never returns (hang), and the two-worker flood test hits its 5 s watchdog. That is exactly the observed M5 failure pair. This **supports** the build-race explanation, but it does not prove which bytes were compiled. The parent's frozen-hash rerun stays the only green evidence.

## 3. Per-ID closure (all 23)

Labels: **code-proven** = follows from source at the pin; **gate** = needs physical, OS or manual observation.

| ID | Verdict | Source / trigger / proof |
| --- | --- | --- |
| M4-01 | **Closed** | `controls.rs:229-247`: the custom key handler is gone, and GPUI `on_click` is the only activation path. Upstream records the key-down on bubble and fires once on key-up with the same focus generation. Held repeats re-arm but cannot fire again before key-up, and modified keys are rejected. Disabled state is guarded in the callback. `tab_index(-1)` and AccessKit `set_disabled` are in `button()` (`:22-26`). The test drives down / 3×held / up for Enter and Space, plus disabled. Only real M5 consumer: `retry-core`. The launcher key handler returns early for Enter/Space when focus is not on the input (`view.rs:292-300`), so there is no second activation. `start_core` also refuses while startup is running (`app.rs:76`). |
| M4-02 | **Closed** (smells §5) | `events.rs`: subscribing admits only owned channels (`:276-278`); unknown output is dropped. Status channels and `-changed`/builtin channels are coalesced (`:279-287`). Budgets are per channel (256 events / 512 KiB, `:288-295`), with one oversized payload allowed. At most 32 channels. Dequeue never pauses unrelated channels (`:149-160`). `send_core` uses `block_in_place` only on multi-thread workers off the GUI thread (`:258-267`); the GUI thread gets `Err` instead of blocking. The cap is 16 blocking threads plus 2 workers (`app.rs:1211-1212`). There is no per-emit task: the caller keeps its own payload. Hidden live streams: the launcher subscription lives with the entity (`command_bar/mod.rs:235-258`). `dispose()` (`close_core`) runs before the core-shutdown await (`app.rs:398` → `:416`). No path in the tree can panic inside `block_in_place`: no `LocalSet`, and the current-thread runtime in knowledge db takes the direct branch. |
| M4-03 | **Closed** for the bound; see **N-01** for save retry | `core_client.rs:232-254`: `shutdown` runs as an independent `tokio::spawn`. The 8 s `timeout` polls `&mut JoinHandle` and never aborts it. On completion or panic it calls `take()` (`:27`). The start task is awaited inside the same bound. `app.rs:416-431`: errors and timeout are logged, and the app proceeds to `relay.close()`, takes the control server, releases the instance, then `complete(true)` or `cx.quit()`. Saves run before the bound, and a failure cancels the quit visibly (`:361-397`). Each JoinHandle is consumed once. |
| M4-04 | **Closed** (code); gate for the visible sheet | `app.rs:1101-1159`: with Settings open → `restore()` and prompt on Settings; otherwise `show()` (activates the app and window, `command_bar/mod.rs:410-411`) and prompt on the launcher. Title, message and detail are mapped; `type` maps to `Info`/`Warning`/`Critical`. The lease is released on any outcome (`DialogClosed` → `suppress(false)` + `restore()`, `:1178-1183`). Side effect: **N-05**. |
| M4-05 | **Closed**; visual gate | `terminal_sessions.rs:8-16`: left overhang 266 (test expects x = −246). `app.rs:578-596`: transparent, `no_shadow` (macOS `setHasShadow:NO`; Windows DWM NC rendering disabled), `focus:false`. |
| M4-06 | **Closed**; DPI gate | `app.rs:197-212`: each snapshot resolves `w.display()` → `monitor_key`/`display_scale`, then `persistence::capture` (physical position, logical size, zoom) and updates `self.monitor`. Windows display bounds are logical at the pin (`gpui_windows/display.rs:45-67`), so the scale ratio is valid. |
| M4-07 | **Closed**; focus gate | `app.rs:256`: `lifecycle.settings = true` before `hide()`, so `cx.hide()`/`restore_foreground` are skipped (`:239-244`). Open failure resets the flag and shows the launcher (`:296-300`). |
| M4-08 | **Closed** (code); physical gate | Owned 16 ms drag task ends on the physical button (`app.rs:709-758`, `native.rs:380-397`). Blur is suppressed and released once (`:703`, `:767`). On macOS, snap placement waits for button-up (`:642`, `:764-766`); on Windows the window follows the cursor delta manually. Overlay: work-area bounds (`:666`, `:680`), `show:false`, `focus:false`; `WS_EX_NOACTIVATE` + `SetLayeredWindowAttributes(255, LWA_ALPHA)` (`native.rs:113-121`); AX root hidden (`snap_overlay.rs:12-14`); `SW_SHOWNA` (`native.rs:37`). Platform hypothesis, unchanged and pre-existing: `start_window_move` runs one relay hop after mouse-down and reads `NSApp.currentEvent`. |
| M4-09 | **Closed** | M5 `view.rs:1022-1062` registers a capture-phase window `MouseMove`/`MouseUp` from a canvas paint. Any up, or a move without a pressed left button, ends the resize. The gesture follows the OS cursor (`resize.rs:10-58`). TextInput registers capture listeners in paint while selecting and ends on any release (`text_input.rs:1046-1059`, `:355-358`). |
| M4-10 | **Closed** | `markdown.rs:75-82`: the move to `settled` (even with the same revision and source) does one full `parse()`. The regression covers reference links, footnotes, loose lists and multi-paragraph `<details>`, and checks the parse count. |
| M4-11 | **Closed** (smells §5) | Highlighting is admitted from `RichBlock::paint` only for blocks intersecting the viewport and only when settled (`:995-1001`, `:495-546`). One job per view, two global (atomic admission), at most 12 KB per block, 32 cached blocks. Work runs on `background_executor`, with lazy `OnceLock` init on that thread. The `light` flag clears the cache on appearance change; results are fenced by revision and theme. Light theme: `InspiredGitHub` (`:157-161`). Visual and pressure gates stay open. |
| M4-12 | **Closed**; IME gate | `text_input.rs:1231-1251`: macOS Cmd-←/→ (+Shift) for line, Cmd-↑/↓ (+Shift) for document, Cmd-⌫ for delete to line start, Alt-Delete and Windows Ctrl-Delete for forward word delete. Real GPUI key-dispatch test. M5 note (not an M4 defect): the launcher's capture handler consumes `up`/`down` without checking modifiers (`view.rs:605-623`), so Cmd-↑/↓ never reach the launcher input. Check against ledger §K in M5. |
| M4-13 | **Closed** for launcher and Settings; see **N-06** | No `alt-f4` binding exists anywhere. Launcher should-close → `Hide` (`app.rs:1363-1366`). Settings → `CloseSettings` → handshake (`:280-283`, `:877-879`). |
| M4-14 | **Closed** | `host_adapter.rs:173-180` compares parsed `HotKey` ids against `Alt+Space`/`Control+Space`; aliases are tested. `--allow-desktop-integration` defaults to false (`cli.rs`); the registry refuses without it (`:191-194`); the tray is created only when opted in (`app.rs:1371-1379`). A hotkey failure keeps the tray (`:157-164`). |
| M4-15 | **Closed** (code); Windows gate | `native.rs:37` uses `SW_SHOWNA`. Explicit `activate_window` is called only in launcher/Settings flows. GPUI already maps `focus:false` to `SW_SHOWNOACTIVATE` at creation. |
| M4-16 | **Closed** | Startup owns its own `JoinHandle` (`core_client.rs:202-227`); dropping the GPUI waiter (`app.rs:321`) releases only the waiter. Retry is guarded (`:76`). Shutdown awaits the same start under the 8 s bound. Command shortcuts run in a detached `tokio::spawn` (`:917-921`). `owned_request` takes one of 16 permits with `try_acquire_owned` and then works independently (`:320-339`). M5 searches stay single-flight through `take_request`, and icons are capped at 4, so normal M5 use cannot exhaust the 16 permits. |
| M4-17 | **Closed** | `settings_close_timer` is its own field (`app.rs:542-545`). Quit no longer clears `tasks` or that timer. |
| M4-18 | **Closed** | Admission is capped at 128 (`events.rs:240-248`). Releases bypass control capacity and are bounded by outstanding leases (`:206-226`) and served first (`:136-144`). Every GUI path produces exactly one release: closed reply → direct `DialogClosed`; otherwise one `DialogLease`/`CaptureLease` Drop. |
| M4-19 | **Closed** for the layout writer | `persistence.rs:36-107`: a latest-value `watch`, 3 attempts per snapshot, the writer stays alive, the final drain retries the latest snapshot and returns a layout-specific error. Cancelling a quit recreates the writer from the retained Owner maps (`app.rs:366-371`). The launcher-state part of the same gate is **N-01**. |
| M4-20 | **Partially open → N-03** | Fixed: password value masked only; graphemes in `TextRun`/offsets; AccessKit disabled; task labels; table rows/cells. Remaining (code-proven structure, AX consequence is a gate): duplicated text-input role, and one list container per item. |
| M4-21 | **Closed**; visual gate | Markdown text sizes, gaps, padding and code padding scale with `t.zoom` (`markdown.rs:604-615`, `:629-636`). Root text is 13·zoom with line-height 1.5. Only hairlines stay at 1 px. |
| M4-22 | **Reopened → N-02, N-04** | Fixed: real `--ln-bar` values (`tokens.rs:86,122`); `bar_float` plus Settings panel/accent-strong added; macOS `Blurred` (`app.rs:1353-1357`); OS reduce-motion read at launch and restore. Regression: the only `bar` consumer is now wrong (N-02). The contrast test does not test the tokens (N-04). No motion consumer exists yet, so `Metrics::for_window` is plumbing only. |
| M4-23 | **Closed** | `markdown_html.rs:76-90`: nested `<details>` counter, with closing through its own `flush`. `markdown.rs:260-266`: paragraphs inside details join the block. Footnote definitions get `Kind::Footnote(label)`. References link to `#footnote:<label>`, which `open_link` scrolls to (`:478-492`; children map 1:1 to blocks). |

## 4. New and remaining actionable findings

### N-01 (Medium, code-proven) — One failed launcher write cancels every later quit, and an unrelated success can hide a real unsaved state

- Files:
  - `apps/tezbar/src/app.rs:313-316, 333-343, 361-397`;
  - `surfaces/command_bar/mod.rs:96, 754-789, 1252-1257`;
  - `surfaces/command_bar/interactions.rs:107-147`.
- Trigger A:
  1. A launcher `persist()` fails transiently (disk full, permissions, file locked), or the model picker's write fails. Its follow-up config read failing also counts, as does an admission rejection ("Native operations are still finishing").
  2. That sets the shared `write_error` slot.
  3. The user quits. `drain_state()` returns only tasks still pending (none), and `state_write_error()` still returns the stale error.
  4. The quit is cancelled: macOS `replyToApplicationShouldTerminate(NO)`, and Settings opens with "Retry".
  5. The cancel path re-arms only the layout writer (`:366-371`). Nothing re-persists launcher state, and `Launcher::hide` does not persist.
  6. So every retry fails the same way, even after the disk or permission problem is gone, until some unrelated launcher action persists successfully. Normal quit and macOS logout/restart stay cancelled.
- Trigger B: a launcher persist fails, then a model selection succeeds. `interactions.rs:146` overwrites `write_error` with `None`, so quit proceeds and the unsaved launcher state is lost.
- Not covered by the recorded fixture: it exercised the **layout** writer path only.
- Fix:
  1. Separate durable-state error ownership: keep a per-owner error for launcher state, independent of model-config UI errors.
  2. On quit, when launcher state has an error or is dirty, start a fresh `persist()` of the current state and await that result instead of reading the sticky slot. Clear the error only on that owner's success.
  3. Do not gate quit on read failures or UI-only failures such as the config re-read.
  4. Add a regression: write fails → condition clears → second quit succeeds. And: persist fails, then model save succeeds → quit is still blocked or re-persists.

### N-02 (Low, design parity regression introduced by the repair) — Launcher footer now uses `--ln-bar` instead of `--ln-bar-float`

- Files: `crates/tezbar-ui/src/controls.rs:159` (`footer` → `.bg(t.bar)`), `tokens.rs:86,122`. Legacy reference: `styles.css:2527-2536, 2775-2778` (`.tezbar-launcher > .launcher-footer` and `[data-command-surface-footer]` use `--ln-bar-float`).
- Consequence:
  - Before the repair, `bar` held the float value, so the footer was correct.
  - The repair changed `bar` to `rgba(0,0,0,.16)` / `rgba(15,23,42,.025)` and added `bar_float`, but nothing uses `bar_float`.
  - The command-bar footer (`view.rs:1373`) now uses the wrong token in both themes, which is visible in light mode.
  - The test only checks that `bar != bar_float`.
- Fix: use `t.bar_float` in `controls::footer`. Keep `t.bar` for the surfaces that use `--ln-bar` (sidebar, extension-store footers). Add a test that maps `footer` to `bar_float`.

### N-03 (Low, code-proven structure; AX consequence is a VoiceOver/Narrator gate) — M4-20 accessibility structure is still wrong in two places

- Text field:
  - The focused wrapper `div#field` (`text_input.rs:1094-1105`) now has role `TextInput`/`PasswordInput`/`MultilineTextInput` plus `aria_value`.
  - Its child `TextElement` (`:832-839`, `:841-880`) still has the same role, value, `TextRun` children and text selection.
  - Result: two nested editable fields. The focused one has no runs or selection, so caret and selection announcements attach to an unfocused node.
- Markdown lists:
  - Every list-item row is wrapped in its own `Role::List` (`markdown.rs:617`) around a `ListItem` (`:600`).
  - Result: N one-item lists, not one list with N items.
- Fix:
  - Put the editable role, value, runs and selection on one node: keep the focus on the node that owns the selection, and make the other a plain container.
  - Group consecutive `Kind::List` blocks under one `List` node.

### N-04 (Low, spec §5.8 gap) — `tokens_contrast_test` does not test the tokens

- File: `tokens.rs:225-240`.
- The test checks 7 hard-coded hex pairs. It never reads `tokens()` output, so a token regression cannot fail it.
- It also omits §5.2 pairs such as danger, success and warning on the sheet, and text-3 on the sheet/footer.
- §5.8 requires every text-on-fill pair from §5.2 (with text-4 exempt only for placeholder and disabled text).
- Fix: build `Tokens` for light and dark, then iterate the real text/fill pairs, compositing translucent fills over the sheet.

### N-05 (Low, code-proven; consequence needs multiple monitors) — Closing any dialog re-runs launcher placement

- Files: `app.rs:1178-1183` (`DialogClosed` → `restore()`), `:1123-1127` (Confirm → `show()`), `:121-183`.
- `show()` re-plans the position on the cursor/frontmost monitor and calls `place_scaled` even when the launcher is already visible.
- Consequence: after a QR save panel, a folder chooser opened from the launcher, or a confirm, the launcher can jump to the monitor the cursor moved to. Before the repair, `DialogClosed` only lowered suppression.
- Fix: if the launcher is already visible, only activate and focus it. Run placement only on a hidden → visible transition.

### N-06 (Low, PLAUSIBLE; Windows GUI not observed) — Sessions panel has no close route

- File: `app.rs:578-596`. The panel is clickable and can become active (that is why `PanelFocused` exists), but it registers no `on_window_should_close`.
- Consequence: on Windows, Alt+F4 while the panel is active lets GPUI close it. `self.panel` stays `Some`, later `panel.update` calls fail silently, and the panel is never recreated.
- Fix: register should-close → `PanelHide` and return `false`. Clear `self.panel` when an update fails.

## 5. Standards, spec and non-gating smells (do not block)

- `CoreEvents::subscribe` panics through `.expect` if more than 32 distinct channels are owned, or if it is called after `relay.close()` (`events.rs:520-527`). It is not reachable at the pin (one launcher subscription at startup), but it is a crash-on-limit API for M6–M10. Return a `Result` and log instead.
- `CoreEvent::bytes()` serializes the full JSON payload just to measure it (`events.rs:493-499`). This doubles CPU and allocation for large PTY or agent payloads. Use a size estimate.
- Coalescing by channel suffix (`status`, `-changed`, builtins including `launcher:invalidated`) ignores payload identity. That is fine for today's global core statuses, but per-item extension `…:status` or text-scoped invalidations would lose earlier items.
- `publish` keeps an unused `delivered` flag (`let _ = delivered;`).
- Highlighting:
  - "Visible" means intersecting the window viewport, not the scroll clip.
  - A rejected job retries only on an unrelated repaint.
  - The 32-entry cache could thrash if more than 32 code blocks are on screen.
- The text-input wrapper clones the full content into `aria_value` on every render.
- `aria_description("Disabled")` duplicates the AccessKit disabled state (`controls.rs:240`).
- A cancelled quit can leave the old layout writer finishing after the new writer starts. The files are atomic, but a slightly older snapshot can win in a narrow race.
- Pre-existing, outside the repair diff: `NativeDesktop::call_with_timeout` blocks a Tokio worker for up to 5 s without `block_in_place`. Desktop services are disabled in the preview.
- Pre-existing: password word movement and deletion follow the secret's real word boundaries.
- `bar_float`, `settings_panel`, `settings_accent_strong` and `Metrics::for_window` have no production consumer yet (M7/M5 will use them). That is fine, apart from N-02.
- Design anti-template check on the diff: no gradients, glass cards, coloured side stripes, badges or new palette introduced. Indigo appears only through the prescribed tokens.

## 6. Counts and remaining manual gates

- Original findings: **23 assessed**. 21 closed, 1 partially open (M4-20), 1 reopened (M4-22).
- New findings: **N-01 Medium; N-02…N-06 Low** (N-06 PLAUSIBLE, Windows). Plus non-gating smells in §5.
- Hash verification:
  - 25/25 repair-map sources;
  - 35/35 parent frozen sources;
  - 14/14 proof artifacts;
  - 1/1 clean binary against its receipt.
- My builds, launches and GUI probes: **0**. XERJ: 1 bounded query (stale index, no reindex).
- Manual gates still open, not new blockers for the foundation:
  - macOS: sheet visibility and focus, Settings first-open flicker, tray/Alt+F19 (opt-in), Dock reopen, snap/overlay pass-through and post-loop placement, edge resize outside the window, mixed-DPI moves.
  - Light/dark/zoom screenshots against the current app.
  - OS IME matrix and VoiceOver.
  - Windows build/GUI/Alt+F4/foreground/Narrator: no Windows M4 pass is inferred.
  - Real GPU texture pressure.
  - The earlier CUA `-10005 cgWindowNotFound` limit still applies.
- After the fixes: add regressions for N-01 (stale-error retry, and masking) and N-02 (footer token), then rerun the integrated suite on frozen hashes. N-03 still needs the VoiceOver/Narrator gate.

The full Rust/GPUI migration is **not** complete. M5–M11 and their acceptance remain open.
