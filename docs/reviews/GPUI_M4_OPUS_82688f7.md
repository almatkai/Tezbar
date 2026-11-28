# Tezbar GPUI M4 foundation — Opus 5.5 correctness / spec / standards / design review

- Reviewer: Claude Opus 5.5 (single agent, no subagents)
- Worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-gpui-app` (read-only use)
- Pin: `82688f7ed2538603ee906106adc61e3d40f3a13b`. Source commit `ac03a37`. `git diff ac03a37 82688f7 -- apps crates` is empty, so later commits change docs only.
- Method: I extracted a frozen `git archive 82688f7` copy into the session scratchpad and read everything from it. I did not read the live working files, which M5 is editing concurrently. I made no checkout, branch, staging, commit, edit, build of the app, app launch, install or CUA input.
- Scope: the 27 M4 source files, plus how they interact with `tezbar-core`/`tezbar-host` and with GPUI at `c84af5b` (read from `~/.cargo/git/checkouts/zed-a70e2ad075855582/c84af5b`). The launcher, Settings and sessions views are intentionally shells, and missing M5–M10 content is **not** reported as an M4 defect.

## 1. Verdict

**M4 foundation: NOT ACCEPTED in its current state. Repairs are required before M5–M10 build on it.**

- What is sound: the overall architecture, thread ownership, the single Tokio runtime, the save → retire-instance → core-stop ordering, the macOS `applicationShouldTerminate:` adapter, preview profile isolation, the safe Markdown/HTML subset (no execution), the image-cache texture release paths, and the TextInput UTF-8/UTF-16/IME commit model.
- What blocks acceptance:
  - **two High defects**: keyboard activation fires twice in every shared `action_button`, and event-hub retention plus blocking backpressure can stall Tokio and deadlock quit;
  - **eleven Medium defects**, covering quit bounding, dialog anchoring, panel geometry, per-monitor persistence, Settings-open focus, the snap drag lifecycle, resize tracking, Markdown streaming divergence, light-theme code highlighting, macOS line-movement keys and Windows Alt+F4.
- Even after these repairs, M4 is not a full W-row pass. The manual gates in §6 remain open, and the full migration is **not** done.

## 2. Scope coverage and evidence used

| Item | Result |
| --- | --- |
| 27 source files (`foundation-test-map.json` `source_sha256`) | All 27 read in full from the frozen pin archive. sha256 recomputed for all 27 against the archive: **27/27 match, 0 mismatch**. |
| Reference sources | Spec §3–7 and §8 (M4)/§9. Ledger W-01…W-20 (with K/B context). `foundation.md`, the test map, the verification checkpoint, `MACOS_SIGNING.md` (not touched by M4: no signing, bundling or identity change in scope), and AGENTS.md. Legacy comparisons: `src-tauri/src/lib.rs`, `src-tauri/src/tauri_host.rs` and `src/renderer/styles.css` (late `--ln-*` block). |
| GPUI behaviour checked at the pin | `elements/div.rs` (hover-gated `on_mouse_move` at :303–313; keyboard click on key-up at :3075–3130; paint order at :2641 then :2650), `gpui_macos/window.rs` (prompt = `beginSheetModalForWindow` at :1680–1755), `gpui_macos/platform.rs` (`quit` via GCD at :576–594, `will_terminate` at :1389), `app.rs` (`on_quit` `try_borrow_mut` at :1166–1183, `drop_image` all-windows at :3186), `gpui_tokio` (dropping the Task aborts the Tokio task, :54–69). |
| Recorded evidence (preserved, not modified) | `/tmp/tezbar-native-m04-evidence/final-tests.log`: **22 app + 30 UI passed**. `final-control-tests.log`: **23 passed**. `final-lifecycle-smoke.json`, `final-termination-smoke.json` and `owned-cleanup.json` (all owned PIDs dead, endpoint absent). Clean receipt `/tmp/tezbar-native-m04-parent-clean-evidence/receipt.json` (source `9474bdb`, dirty:false, bench_control:false, sha256 `2bc8820d…7c41`, no_webkit:true). These are state/control facts, not visual or OS-input proof. |
| My probes | **1 isolated probe, 3 cases.** `pulldown-cmark =0.13.6` (the locked version), `--offline`, in the scratchpad (not the repo). It replicates `closed_prefix`/`Document::update` and reproduces **3/3 divergences** (M4-10). No GPUI or app build was justified, because every other finding is proven from source at the pin. |
| New builds or runs of Tezbar | None. No app launch, no control fixture, no CUA. No visual, VoiceOver, IME or Windows claims. |

## 3. Findings (ranked)

Evidence labels: **reproduced** (I ran a probe), **code-proven** (follows from source at the pin, including GPUI's source), **platform hypothesis** (depends on OS behaviour not observed here).

### High

**M4-01 — `action_button` activates twice for every real Enter/Space press** (code-proven)
- File: `crates/tezbar-ui/src/controls.rs:226-252`. Test: `:290-311`.
- Trigger:
  1. The button is focused and the user presses Enter or Space.
  2. GPUI's own keyboard-click recorder for focused `on_click` elements runs first (it is registered in `paint_mouse_listeners`, div.rs:2641, recorder at :3085-3096). It is not default-prevented, so it records the key-down.
  3. The custom `on_key_down` (registered later, at :2650/:3342) then calls `activate` and `stop_propagation()`. It does not call `prevent_default()`.
  4. On key-up, GPUI emits `ClickEvent::Keyboard` (div.rs:3099-3130), so `on_click` calls `activate` a second time.
- Consequence:
  - Toggles end up unchanged.
  - Settings theme and Close run twice.
  - Launcher **Retry** runs `start_core` twice, and the second run drops and aborts the first `core.start()` (see M4-16).
  - Every future M5–M9 consumer (Install, Delete, the "Allow once" approval) runs twice from the keyboard. This contradicts the claim "exactly-once guarded activation".
  - The unit test sends only a key-down (`simulate_keystrokes`), so it cannot catch this.
- Fix: delete the custom `on_key_down` and rely on GPUI's keyboard click, which already fires exactly once, ignores held repeats and is fenced by focus generation. (Alternatively, call `window.prevent_default()` before activating.) Extend the test to dispatch key-down **and** key-up, plus a held-repeat case.

**M4-02 — The event hub keeps every unsubscribed event, then blocks Tokio workers; quit can deadlock** (code-proven by construction; how fast it triggers depends on load)
- Files:
  - `apps/tezbar/src/events.rs:404-406`: "reliable" means *any* channel not starting with `window-`/`launcher-`.
  - `:495-502`: unsubscribed reliable events are kept without limit until 4 MiB, then `enable_core(false)`.
  - `:221-229`: producers block on a `std::sync::Condvar` while the core queue holds ≥256 events or ≥4 MiB.
  - `app.rs:1014-1016`: Tokio has `worker_threads(2)`.
  - `app.rs:319-346`: quit awaits a `gpui_tokio` drain before reaching `events.dispose()`, which is the only thing that re-enables the queue.
- Trigger: in the M4 shell **nothing subscribes**, so every core broadcast is retained (`knowledge:status`, `extension-worker-status`, `terminal:sessions-changed`, `agent:event`, `llm-config-changed`, …). The same happens in M5+ whenever a surface is closed while its stream continues, for example a chat still generating or indexing progress while Settings is closed.
- Consequence:
  1. Once retained output reaches 4 MiB, all core delivery pauses, including channels that *do* have live subscribers (head-of-line blocking).
  2. When ≥256 events/4 MiB more arrive, each emitting thread blocks. `GpuiHost::emit` is called from core async tasks on Tokio workers.
  3. With both workers blocked, every core request hangs.
  4. Quit then hangs: its drain is a Tokio task, and `dispose()` runs only after that task.
- The memory also stays retained indefinitely. Once desktop services are on, that includes clipboard/agent payloads.
- Fix:
  - Never block async workers: use async backpressure (await a `Notify`/semaphore), or `block_in_place` for sync producers only.
  - Make retention opt-in per channel. An owner declares the durable channels it will consume; other channels are dropped or coalesced with a log.
  - Bound retention per channel.
  - In quit, call `dispose()`/`enable_core(true)` before awaiting any Tokio drain.
  - Add a test with two blocked producers on a 2-worker runtime.

### Medium

**M4-03 — Quit is not bounded at 8 s (W-13). After a timeout the app is left half torn down, and native termination replies NO** (code-proven)
- Files: `apps/tezbar/src/core_client.rs:56-77`, `app.rs:346-373`.
- Trigger: `NativeCore::shutdown` takes more than 8 s (hung child, slow Knowledge).
- Consequence:
  - Events are already disposed, the tray and preview hotkey removed, single-instance retirement started and the launcher set to `ready=false`, yet the process stays alive.
  - `replyToApplicationShouldTerminate(NO)` cancels macOS logout/restart.
  - Each retry waits again on the same task, so a core that never finishes makes the app unquittable without force-kill.
  - The ledger (W-13: "core shutdown ≤8 s … exit") and Tauri (`src-tauri/src/lib.rs:359-373`) exit after the bound.
- Sub-defect: if the shutdown task panics, `Ok(Err(e))` (`core_client.rs:72`) leaves the completed `JoinHandle` in place. Polling it again panics with "JoinHandle polled after completion", so every later quit fails.
- Fix: after the 8 s bound, log the timeout and continue to exit (supervisor/Job Objects reap the children). At minimum, reply YES to native termination after the bound. Call `take()` on the handle on `JoinError`.

**M4-04 — Confirm dialogs are a sheet on the launcher even when the launcher or the whole app is hidden** (anchor code-proven; invisibility on macOS is a platform hypothesis, high confidence)
- File: `apps/tezbar/src/app.rs:948-956` (`self.launcher.update(… w.prompt(…))`). GPUI uses `beginSheetModalForWindow` on that window (`gpui_macos/window.rs:1745-1752`). `hide()` also calls `cx.hide()` (NSApp hide, `app.rs:241-246`).
- Trigger: any of
  - a command hotkey bound to `quit-tezbar` (`Event::Hotkey` → `execute_command_shortcut` → `app:confirm-quit`, `tezbar-core/src/lib.rs:207-208`, `platform.rs:346-350`) while hidden;
  - an extension `core:platform:confirm` from a background or menu-bar command;
  - any confirm while Settings is open (the launcher is then hidden).
- Consequence:
  - The sheet sits on an ordered-out window of a hidden app, so it is invisible.
  - The core request waits indefinitely and the blur-suppression counter stays raised.
  - With Settings open, `restore()` always brings Settings forward, so the launcher (and the sheet) never appear.
  - Tauri used an app-level dialog (`tauri_host.rs:103-146`). The native path also ignores `options.type` and the Tauri message/title mapping.
- Fix: choose a visible anchor (the frontmost Settings window, otherwise show the launcher before prompting), or use an app-modal alert scheduled outside the GPUI borrow. Map `type` to the prompt level.

**M4-05 — Terminal sessions panel is placed on the wrong side of the launcher** (code-proven)
- Files: `apps/tezbar/src/windows/terminal_sessions.rs:8-19` (`x = parent.right() - (300-266)`). Its test at `:62-69` asserts this wrong value (746).
- Tauri: `x = main.x - 266*scale` (`src-tauri/src/lib.rs:1309`), i.e. a **left** overhang. Spec W-09 says "left overhang 266".
- Also missing compared with spec §3.5: transparent window background and no shadow (`app.rs:526-533`).
- Consequence: M10 inherits a panel on the right edge, overlapping 34 px, mirrored from the product.
- Fix: `point(parent.left() - px(OVERHANG), parent.top() + px(TOP))`. The fixed test should expect x = −246. Add transparent and no-shadow options.

**M4-06 — Window position and size/zoom are saved under the monitor from the last show, not the current one** (code-proven)
- File: `apps/tezbar/src/app.rs:193-213`. `snapshot()` uses `self.monitor`, which is set only in `show()` at `:180`.
- Trigger: show on monitor A, drag the launcher to monitor B (or resize/zoom there), then hide.
- Consequence:
  - B's coordinates are written under A's key, and B's entry is never updated.
  - The next show on A gets an off-monitor position (clamped or jumping), and A's size/zoom profile is overwritten.
  - This breaks W-02/W-05 per-monitor persistence. Tauri persists by the window's current monitor.
- Fix: in `snapshot()`, resolve `w.display(cx)` → `native::monitor_key` and `display_scale` each time, and update `self.monitor`.

**M4-07 — Opening Settings the first time hides the whole app or gives foreground away before Settings opens** (ordering code-proven; Windows consequence is a platform hypothesis)
- File: `apps/tezbar/src/app.rs:257-258` → `hide()` `:241-246`, which runs while `lifecycle.settings == false`.
- Consequence:
  - macOS: `NSApp hide` followed by re-activation flickers focus to the previous app and back.
  - Windows: `SetForegroundWindow(previous)` runs and only then does Settings call `activate_window`. The foreground lock will likely leave Settings behind the other app, with a flashing taskbar button.
  - Tauri's `hide_main_window_for_settings` hides only the window (`lib.rs:1222-1229`).
- Fix: hide only the launcher when the next step is Settings. For example, give `hide(cx, keep_app_active: bool)` a flag, or set `lifecycle.settings` before the hide.

**M4-08 — Snap drag lifecycle is incomplete (W-07)** (code-proven gaps; OS consequences are platform hypotheses)
- Files:
  - `launcher.rs:172-177, 206-213`: drag ends on an in-window `on_mouse_up` after `w.start_window_move()`.
  - `app.rs:411-415, 556-641, 724-728`.
  - `windows/native.rs:35-38`: `ShowWindow(SW_SHOW)`.
  - `native.rs:103-124`: `WS_EX_LAYERED` without `SetLayeredWindowAttributes`.
- Gaps compared with Tauri/ledger:
  - No Windows cursor-driven drag task that ends on `GetAsyncKeyState(VK_LBUTTON)` release (`lib.rs:1166-1204`).
  - No blur suppression during the drag (`lib.rs:1532`).
  - The snap lock moves the window during the OS move loop, so the two fight over position.
- Consequence:
  - The OS move loop usually consumes the mouse-up, so `snap.active` and the overlay can stay alive after the drag. Later `Bounds` events then keep snapping the window.
  - On Windows, the overlay `SW_SHOW` can activate the overlay and blur-hide the launcher mid-drag.
  - A layered window without attributes is documented by Microsoft as not displayed, so the overlay may be invisible.
- Fix:
  - Port the Windows manual drag loop; on macOS, end on `pressedMouseButtons == 0` or a settle.
  - Suppress blur while dragging.
  - Use `SW_SHOWNA` for the overlay and panel, and call `SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA)`.
  - Size the overlay to the work area and hide it from AX (spec §3.5/§5.8).

**M4-09 — Edge/corner resize stops tracking once the cursor leaves the window (W-05)** (code-proven against GPUI; actual feel needs the manual gate)
- Files: `launcher.rs:163-167` (`on_mouse_move` on the sheet), `windows/resize.rs:10-58`. GPUI only calls `on_mouse_move` when the hitbox is hovered (`div.rs:303-313`).
- Trigger: dragging an edge outward faster than about 2 px per event.
- Consequence: the gesture stalls. The ledger requires the handles to track the physical mouse. (TextInput drag-selection has the same limit, `text_input.rs:1091`; minor.)
- Fix: while a gesture is active, register an ungated `window.on_mouse_event::<MouseMoveEvent>`, or use a pointer-capture/drag API, and end it on any mouse-up.

**M4-10 — A streamed Markdown message renders differently after it settles than when reloaded settled** (**reproduced**, parser-level probe)
- Files: `crates/tezbar-ui/src/markdown.rs:69-104` (on settle only the remaining tail is parsed; earlier blank-line chunks are never reparsed), `:207-227` (`closed_prefix` commits at every blank line outside fences).
- Probe: scratchpad `mdprobe`, pulldown-cmark 0.13.6. Results:
  - reference links (`[docs][1]` … `[1]: url`): link lost when streamed;
  - footnote reference: lost;
  - loose list-item continuation: split out of the item.
  - All three differ from a single settled parse (3/3).
- The same splitting also breaks `<details>` blocks that span blank lines, because HTML state is per chunk.
- Consequence: AI chat (M8) and other streamed content show broken links and lists until the message is reopened from history.
- Fix: when `settled` becomes true, reparse the full source once. Optionally commit only at blank lines outside list, blockquote and HTML-block context.

**M4-11 — Code highlighting is always dark-themed, uncached by theme, and runs for all blocks on the main thread** (code-proven)
- Files: `markdown.rs:127` (`themes["base16-ocean.dark"]` always), `:429-438` (highlights *every* code block in `render`), `:120-123` (loads SyntaxSet/ThemeSet lazily on the main thread).
- Consequence:
  - In the light theme, code is pale dark-theme text on the near-white `--ln-field`, which is a contrast failure.
  - Colours are not recomputed after a theme switch.
  - Settling a long transcript runs N × 12 KB of syntect highlighting synchronously on the GPUI thread. This violates "views never block the main thread" (§3.4), and `foundation.md`'s claim that only *visible* code is highlighted is inaccurate.
- Fix: map the light and dark token palettes to the highlight.js theme parity, invalidate on appearance change, and highlight off the main thread for visible blocks only.

**M4-12 — Missing macOS line-movement and line-delete keys in every text field (§5.5)** (code-proven)
- File: `crates/tezbar-ui/src/text_input.rs:1138-1261`. Only home/end are bound.
- Missing:
  - macOS `cmd-left/right` (+shift), `cmd-up/down` (+shift), `cmd-backspace`, `alt-delete`;
  - Windows `ctrl-delete`.
- MacBook keyboards have no Home/End keys, so line movement is effectively unavailable on macOS.
- Fix: add the bindings and actions (line start/end per visual line for multiline, document start/end, delete to line start, forward word delete) with tests.

**M4-13 — Windows Alt+F4 quits the whole app** (code-proven binding; Windows GUI not observed)
- File: `apps/tezbar/src/app.rs:1061-1070` binds `alt-f4` → `RequestQuit` globally.
- Consequence: Alt+F4 on Settings skips the W-08 close handshake and quits everything. On the launcher it quits instead of hiding. Tauri never bound this, and `on_window_should_close` already routes window close correctly.
- Fix: do not bind Alt+F4. Let it reach each window's should-close path (Settings → `CloseSettings`, launcher → `Hide`).

### Low

**M4-14 — The production-hotkey exclusion is a string match and is bypassable; the tray is always on** (code-proven)
- File: `host_adapter.rs:164-172`.
- `"Option+Space"`, `"Alt + Space"`, `"Space+Alt"` and Windows `"CmdOrCtrl+Space"` pass the check but parse to the production hotkey (`tezbar-host/desktop.rs:15-21`, global-hotkey aliases).
- Spec §6.4 also says tray and hotkey are off unless `--allow-desktop-integration`. The preview always creates a tray icon, and if Alt+F19 fails to register, the tray is dropped too.
- Fix: compare the parsed `HotKey` (modifiers + code) against the production defaults. Add the opt-in flag, or record the deviation in the ledger.

**M4-15 — On Windows the sessions panel takes activation from the launcher** (code-proven call; consequence is a hypothesis)
- Files: `native.rs:35-38` (`SW_SHOW` = activate), `app.rs:187-191` (the panel is shown after the launcher).
- The panel takes activation away from the freshly focused launcher input, which also changes the blur and focus sequence.
- Fix: `SW_SHOWNA`/`SW_SHOWNOACTIVATE` for non-activating windows.

**M4-16 — Dropping a GPUI task aborts core work, not just the await (§3.4)** (code-proven)
- `gpui_tokio::Tokio::spawn` aborts its Tokio task when the GPUI Task is dropped (`gpui_tokio.rs:54-69`).
- `quit()` sets `self.startup = None` (`app.rs:313`), which aborts an in-flight `NativeCore::start_with_options` mid-`join!` (`tezbar-core/src/lib.rs:107-147`) before shutdown.
- `RetryCore` replaces `startup` (`app.rs:77`).
- `tasks.clear()` aborts running `execute_command_shortcut` calls.
- Fix: run owner-independent core work with `tokio::spawn`, keep the `JoinHandle` (do not abort), and only await it from the GPUI task.

**M4-17 — A cancelled quit drops the Settings-close fallback timer** (code-proven)
- The 3 s fallback is pushed into `self.tasks` (`app.rs:490-493`), and `quit()` clears that list (`:312`).
- If a close is pending but not yet acknowledged and the quit fails, `settings_close` stays pending forever. `request_settings_close` then returns early (`:479`), so Settings cannot close.
- Fix: keep the timer in its own field, or re-arm it on cancel.

**M4-18 — Lease-release events can be dropped when the control queue is full** (code-proven, unlikely)
- `DialogLease`/`CaptureLease` `Drop` (`events.rs:86-99`) go through `send()`, which rejects when the queue holds ≥256 events (`:182-184`).
- Consequence: the suppression counter stays raised and hidden windows are not restored.
- Fix: reserve capacity for lease releases (they bypass the cap).

**M4-19 — The layout writer dies on its first error, and quit then blames Settings** (code-proven)
- Files: `windows/persistence.rs:25-38` (`??` ends the task), `app.rs:334-341`.
- After one transient write error, every later snapshot fails. The first quit is cancelled with "Couldn't save Settings…" although only the layout failed.
- Fix: keep the loop alive and record the last error. Use a distinct message and retry.

**M4-20 — Accessibility details** (hypotheses pending the VoiceOver/Narrator gate)
- TextInput keyboard focus is on a `Role::Group` wrapper; the `TextInput` role is on a child node (`text_input.rs:1019-1066, 777-790`).
- Character lengths are counted per `char`, not per grapheme (`:803-808`, `markdown.rs:783-788`).
- Passwords expose no masked value, while spec §5.8 says "value (masked)".
- Disabled buttons use `aria_description("Disabled")` instead of the AccessKit disabled state (`controls.rs:237`).
- Task checkboxes are labelled "Task" (`markdown.rs:493-498`).
- Tables and lists expose a role but no row/cell or list container.

**M4-21 — Markdown ignores interface zoom** (code-proven)
- `markdown.rs:484-491, 505` use literal px sizes, while TextInput and controls scale by `t.zoom`. Spec §5.5 says zoom multiplies rem-equivalent sizes.

**M4-22 — Token and spec gaps** (code-proven)
- `tokens.rs` `bar` holds `--ln-bar-float`; the real `--ln-bar` (0.16 / 0.025), `--c-accent-strong` and `--c-panel` are missing.
- There is no `tokens_contrast_test` (§5.8).
- `Metrics.reduced_motion` is never read from the OS.
- The launcher uses `Transparent` rather than macOS `Blurred` (`app.rs:1161` vs §3.5).
- All other `--ln-*` and text-ramp values match `styles.css:2092-2125` and `:40-68`.

**M4-23 — `<details>` only collapses its first paragraph; footnote references disappear** (code-proven by trace)
- `markdown_html.rs:77-115` plus the paragraph flush at `markdown.rs:318-322`: the second and later markdown paragraphs inside `<details>` render outside the collapsed block.
- `Event::FootnoteReference` is ignored (`markdown.rs:375`).

### Standards and non-gating smells (no concrete failure; do not gate M4)

- Several lines were evidently left unformatted by rustfmt (`app.rs:340-341, 544`; `events.rs:405`) despite the "formatting passed" note. Reformat by splitting the expressions.
- Test quality:
  - The Markdown copy test emits `DocumentEffect::Copy` itself instead of dispatching the action (`markdown.rs:992-995`).
  - The button test sends key-down only (M4-01).
  - The panel test encodes the wrong side (M4-05).
- TextInput undo takes one snapshot per keystroke with no word coalescing, so the 100-step limit covers only about 100 characters (`text_input.rs:345-354`).
- Image cache: no ticket-cancel API (a cancelled decode holds one of 4 slots until `release`), no negative cache for failing URLs, and evicting a still-painted image re-uploads a texture the cache no longer tracks.
- The `blur_ticket()` generation is shared with idle trim, so a `PanelFocused(false)` after hide cancels the 5 s trim (`windows/mod.rs:37-40`, `app.rs:680-692`).
- Tray menu ids carry a `preview-` prefix (spec says "same ids"). This is fine for the preview but must be reverted at M11.
- The Settings shell reuses the launcher sheet (radius 18 with rim) inside a titled window. M7 should use flat panels and hairlines.
- `text_input::Quit` action is unused.

### Design review (anti-template rules, §5.6)

- No decorative gradients, glass cards, coloured side stripes, NEW/BETA chips, ALL-CAPS eyebrows, spinners/skeletons, emoji icons or new palette appear in the M4 code.
- Indigo is used only as the tokens prescribe: selected row `#4f52d9`, primary/focus `#5b5ef0`, Settings link `--c-accent`.
- Radii and spacing follow the late cascade.
- Design issues that actually need changes: dark-only syntax colours in light mode (M4-11), sizes that ignore zoom (M4-21), the missing macOS blur material (M4-22), the panel side (M4-05), and the Settings shell's sheet-in-window treatment (smell).
- Visual parity is **unverified**: there are no screenshots, and I did not convert old screenshots into proof.

## 4. Checks that passed (nothing to repair)

- Thread ownership:
  - `GpuiHost` is built inside the GPUI launch callback with a macOS `MainThreadMarker` assertion.
  - OS handlers capture a weak mailbox and are installed once (OnceCell upstream).
  - `NativeDesktop` runs operations through the relay, and only the GPUI receiver executes them.
  - The receiver awaits `Notify` with no lost-wakeup window and no polling timer.
- Exactly one Tokio runtime (`init_from_handle` plus a `RuntimeOwner` global). The handle guard covers main-thread `tokio::spawn` for the writer.
- Quit ordering:
  1. snapshot/flush, then drain Settings and layout;
  2. only on success: dispose events, `begin_shutdown`, remove tray/hotkey;
  3. core shutdown;
  4. close relay, release instance, `complete`/`cx.quit`.
- Saves complete before single-instance retirement and before core stop.
- macOS termination adapter:
  - checks the class is exactly `GPUIApplicationDelegate`, refuses an existing `applicationShouldTerminate:`, and verifies `Q@:@`;
  - keeps a weak relay; a second native quit while draining still gets `TerminateLater`, and a post-drain `cx.quit()` gets `TerminateNow`;
  - the reply is issued outside the RefCell borrow; GPUI's `will_terminate` uses `try_borrow_mut`, so there is no double-borrow panic.
  - This matches the recorded fixture (held for 0.4 s, re-entry, cooperative exit).
- Settings close handshake: generation, ack and finish with failure retained; the 3 s fallback applies only to unacknowledged closes (except M4-17).
- Capture lease: crossing the oneshot preserves restore-once; cancelling after the capture starts holds the restore until the blocking capture finishes; restore is generation-fenced; suppression is counted.
- Preview isolation:
  - the profile must be absolute, non-symlink, a canonical child of OS temp, carry the marker, and contain no symlinks anywhere (recursive check);
  - resources must sit outside the profile, have a non-symlink manifest, and match the source SHA;
  - `desktop_services=false` with `ISOLATED_PREVIEW` (no clipboard/app/fs watchers, no background extensions or Knowledge, no shortcut initialisation);
  - no eager model, Bun or terminal work; clipboard is read only on the user's paste action;
  - `bench-control` is compiled out of normal builds (receipt `bench_control:false`).
- CLI order: supervisor dispatch, then native-v2 CLI, then `--version`/`--help`, then preview parse, then single instance, then GPUI.
- Markdown safety: the link scheme allow-list (http/https/mailto), images limited to HTTPS or a strict relative sandbox asset path, tokenizer-only HTML subset, script/style/iframe/object content suppressed, no DOM or WebView.
- Image cache: entry and byte LRU, 4 decode admissions, duplicate suppression, generation fencing; `drop_image` on replace, evict, remove, release and entity release (`App::drop_image` covers all windows); 2 MiB / 2048² / 16 MiB `max_alloc` limits; BGRA conversion.
- TextInput: UTF-8/UTF-16 conversion with surrogate rounding; grapheme deletion; `Committed` fires only when text is actually committed, not during IME composition; composition cancel restores; single-line newline normalisation; password text never reaches AX, copy or cut.
- Tokens: every `--ln-*` value present matches the final cascade. Font fallback is Geist → `.SystemUIFont`/Segoe UI, and mono is Geist Mono → SF Mono/Menlo/Consolas.

## 5. Counts

- Source files reviewed: **27/27** (hash-verified 27/27).
- Recorded tests (parent evidence, not rerun): normal **52** (22 app + 30 UI) passed; bench-control **23** passed; 0 failed/ignored. These are pure/state tests and do not prove live UI.
- My probes: **1** (pulldown-cmark, 3 cases, 3/3 divergences reproduced). Builds/launches by me: **0**.
- Findings: **2 High, 11 Medium, 10 Low**, plus non-gating smells.
- Evidence split: reproduced 1 (M4-10). Code-proven: all others, with M4-04/07/08/09/13/15 carrying platform-dependent consequences labelled as hypotheses.

## 6. Remaining manual gates (not new blockers; acceptance belongs to later checks)

- macOS: real tray left-up/right menu and Alt+F19 delivery; Dock reopen; blur-hide and panel focus sequence; snap drag, overlay and pass-through; edge resize; mixed-DPI multi-monitor placement; Settings first-open focus; confirm dialog visibility with the launcher hidden (after M4-04 is repaired).
- macOS VoiceOver over the field, list and markdown roles; the OS IME matrix (Latin/Cyrillic/Kazakh/dead keys/Japanese/Pinyin).
- Windows: **no current M4 pass**. The Windows M2/M3 gates failed on disk-full infrastructure and ran no tests. Still pending on Windows: tray/hotkey delivery, Alt+F4/close/console, foreground restore, snap/overlay, mixed DPI, Narrator.
- A light/dark visual comparison against the current app (no screenshots exist for M4).
- Real GPU texture-pressure measurement.
- After the repairs: rerun the 52+23 tests plus new tests for M4-01/02/03/05/06/10/12, and repeat the control fixture on the clean repaired source.

The full Rust/GPUI migration is **not** complete. M5–M11 and their acceptance remain.
