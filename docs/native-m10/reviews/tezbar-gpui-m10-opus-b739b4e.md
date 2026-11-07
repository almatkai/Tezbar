# M10 Terminal — first full Opus 5.5 acceptance review @ b739b4e

Reviewer: Claude Opus 5.5, worked alone, no subagents. Date: 2026-11-07.
Pin: `b739b4ebdf90d906f67efa00a4a9c523fdc58f83` on `feat/native-gpui-app`, worktree `Tezbar-native-rust-agent`.

**Verdict: NOT ACCEPTED.** I found 8 actionable defects (1 high, 3 medium, 4 low), and 5 of them are reproduced by probes. No T/L/S/K/W row reaches green. The core stream/replay architecture held up well under scrutiny. The problems are in the native input mapping, pin ownership and visibility lifecycle. Full first-party migration remains incomplete. No Windows, manual, AX, GPU, IME or RAM acceptance was done or is claimed. I have no screenshots and claim none.

## 0. Method and integrity

- I ran `git archive` on the PIN into a private scratch directory (`.../scratchpad/pin`). All **2175 blobs** match their `git hash-object` values (0 mismatches). I verified this before any build and again after removing my probes. The live worktree was never edited, built, staged or switched. HEAD is still b739b4e, with no tracked diff under apps/ or crates/.
- Builds used a private target dir, an APFS `cp -c` clone of `target/debug/{.fingerprint,build,deps}` with no real space consumed. I never used the shared target. Toolchain: rustc 1.98.1 (48a229cea), `--locked --offline -j2`, `CARGO_INCREMENTAL=0`. The native crate used `--features runtime-shaders`, plus `TEZBAR_NATIVE_SOURCE_SHA=<PIN>` because the archive has no `.git`; `build.rs` requires either a git SHA or that env var.
- I did not run XERJ (it is known to be stale or unavailable). Exact git/rg reads were authoritative.
- Untracked M9/M11 drafts are outside the PIN and outside my scope. I did not re-verify the parent's hash manifests. I also did not re-verify the documented intentional Windows-only `assets_windows` override (`971a2ea6…`).

### Commands I ran (all on the archive, all status 0)

| Log (sha256 prefix) | Command | Result |
|---|---|---|
| ui.log `f5f14c7a` | `cargo test -p tezbar-ui --lib -- terminal_grid` | 5 passed |
| core.log `16da7b01` | `cargo test -p tezbar-core --lib -- terminal::pty::stream terminal_bridge m10_whole_paste` | 6 passed (no PTY) |
| pty.log `a4bc35f7` | `cargo test -p tezbar-core --lib -- --exact terminal::pty::m10_tests::m10_same_owner_private_pty_binary_input_replay_and_shutdown` | 1 passed; no fixture process left (`pgrep`) |
| native.log `b90ab242` | `cargo test -p tezbar-native --features runtime-shaders --lib -- surfaces::terminal command_bar::terminal_integration app::m10 windows::terminal_sessions` | **44 passed** |
| probe2.log `2c0c8c0d` | the same, filtered `opus_probe`, with my temporary probe module | 5 passed. Each probe asserts the *defective* behaviour, so a pass confirms the defect. Probe source: `scratchpad/opus_probe.final.rs` (sha256 `f809b95a…`), removed from the archive afterwards. |

`model.log`: a `tezbar-ui-model -- terminal` filter matched 0 tests. The model's terminal tests have other names, and the parent's full counts cover them.

I inspected the PTY fixture before running it:
- explicit `CommandBuilder("/bin/sh") -c "printf 'M10_READY\n'; exec /bin/cat"`, non-login, so no rc files run;
- tempdir data, profile, home and cwd via `CoreContext::headless`;
- cleanup is kill on the owned id plus a `shutdown_all` Drop guard, and it asserts that its own PID is gone.

Caveat: `CommandBuilder` inherits the process environment. The fixture never reads it.

## 1. Architecture falsification (what holds)

- **Single PTY owner holds.**
  - The native transport is `Request::Terminal` over the existing `CoreClient`, with the same semaphore and Tokio runtime. `core.ctx.request` is used only for static `core:terminal:*` channels.
  - There is no new process, runtime or PTY in apps/ or ui/.
  - `terminal_grid` uses Alacritty only for `Term`/`Processor`. There is no `tty` or event_loop in that code.
- **Internal routes cannot be reached from extensions.**
  - `NativeCore::request` rejects every `core:*` channel.
  - The worker `HOST_CHANNELS` list contains no terminal channel.
  - `Request::wire` returns `&'static str`, so no dynamic channel can reach the `starts_with("core:terminal:")` bypass.
  - The only references to the new routes are in `terminal_bridge.rs`, `request.rs` and tests.
- **Stream invariants hold.**
  - Reads are capped at 64 KiB and validated in both core and UI.
  - Positions are exact `start/end/watermark`.
  - There are at most 32 opaque cursors with a 60 s idle expiry. Effective worst-case lifetime is about 90 s because the reaper ticks every 30 s.
  - Release, shutdown `clear()` and stale-completion release + detach all work.
  - There is no whole-log `String` and no text-overlap matching. Parsing starts at byte 0 through one retained Processor.
  - Live gaps (`start > position`) recover through the durable cursor.
  - Data published while `replaying` is dropped but kept as `target`, and replay continues until `position >= target`.
  - Exit is emitted only after `output_thread.join()`, with the final position. `finish_exit` waits until the position reaches it.
- **Disk truthfulness holds.**
  - A failed append sets `durable=false`, after which replay refuses with an error.
  - Inode/dev identity on Unix (volume/file-index on Windows, uncompiled here) and file length are checked on every open/read. Truncation, replacement and deletion return errors.
  - `delete-history` refuses running sessions, and `prune-history` is a no-op.
  - The UI reports an unrecoverable gap when the stream is non-durable.
- **Lock order holds.** sessions→stream, replay→stream, reader holds stream only during `append`, and nothing is published under a lock. I found no inversion.
- **Back-pressure holds.** `core:terminal:data` is never dropped. The Relay blocks the producer (the PTY reader thread) at 256 events / 512 KiB per channel. Only `*-changed` and `*status` channels coalesce. Legacy `terminal:data` is unowned in the native app, so it is dropped at enqueue rather than parsed twice.
- **Input invariants hold.**
  - Input is one base64 request of at most 1 MiB, including the bracketed-paste wrappers.
  - It is fenced by generation and `exited`.
  - `TerminalInput::send` admits all of it or none of it. The core test proves no prefix is admitted and that in-flight bytes count against the budget.
  - Raw X10 bytes travel as `Vec<u8>` (`[27,'[','M',32,233,233]` is tested).
  - Write `false` cancels queued writes and reports an error. Nothing is retried.
- **Replay suppresses side effects.** OSC52 is disabled in `Config`, and the listener drops clipboard events. Device replies for bytes before the watermark are discarded. Note the side effect on fresh sessions in F6.
- **DECSET 1049 test.** This is a legitimate correction, not a weakening. In Alacritty 0.26 `swap_alt()` (`term/mod.rs:714-735`), `inactive_grid.cursor = grid.cursor.clone()` copies the primary cursor. After `aR界` the cursor is at column 4 (界 is wide). The test asserts `A,L,T` at columns 4/5/6, then an explicit `ESC[2J ESC[H` puts `ALT` at the start, and `?1049l` restores `aR界`. This matches xterm/xterm.js semantics, and production code does not move the cursor.
- **No render-time requests.** `render()` builds the frame only when dirty, bounded by the viewport. Note that paint's geometry callback *defers* `layout_changed`, which enqueues `Resize`. This contradicts the `mod.rs:153` comment but is a benign self-scheduled layout follow-up with no loop.
- **No PTY at startup.** `TerminalView` is created only in `activate_terminal` on a `Surface::Terminal` navigation. `gpui_readonly_render_never_starts_transport` confirms this.
- **Quit preparation.** `prepare_lifecycle_quit` fences input without disposing. `drain_terminal` runs only after every unrelated barrier. It hides (detach), retries failed cleanup and waits for quiescence inside the single 8 s App budget. On cancel, `revive_after_quit` runs and then `restore → show_terminal → Resume`. Grid, preedit and configure are preserved (test `repair_quit_preparation…`). The shared M8 quit flow is out of scope; I traced only its terminal caller.

## 2. Actionable findings (most severe first)

### F1 — HIGH — regression — confidence: source + probe + TS source
**Ctrl+K / Ctrl+X / Ctrl+P / Ctrl+D inside a live macOS terminal are app actions. Ctrl+D deletes the session.**

- **Code:** `apps/tezbar/src/surfaces/terminal/input.rs:38-58`. `primary = m.command || m.control`, then `k→Configure`, `x→Stop`, `p→Pin`, `d→Delete`, regardless of platform.
- **Caller:** `TerminalView::key` (`mod.rs:376-413`) passes `control = m.control` straight through to `state.handle_key → delete()`. `delete()` enqueues `TerminalRequest::Delete` (core kill + `wait_stopped` + metadata/history removal) with no confirmation.
- **Trigger:** on macOS, press Ctrl+D to send EOF and exit a shell or a REPL.
- **Consequence:** the session is killed and its durable history is deleted. In addition:
  - Ctrl+X kills the shell (emacs prefix chords such as Ctrl+X Ctrl+E are lost);
  - Ctrl+K (kill-line) opens the drawer;
  - Ctrl+P (previous history) toggles a pin.
- **Why it is a regression:** TS `TerminalView.tsx:364-376, 463-485` intercepted only `metaKey` + `[`/`k`, Esc, zoom and session digits. Stop, Pin and Delete were command-bar `>`-mode actions on a selected row (`CommandBar.tsx:3597-3607`). Spec §5.5 says "macOS uses ⌘, Windows uses Ctrl".
- **Test encodes the bug:** `key_sequences_app_interception_and_bracketed_paste` asserts that `control:true` maps to Configure/Stop/Pin/Delete. Probe `probe_control_letters_are_app_actions_on_every_platform` reproduces the Delete request and the absence of `\x04`.
- **Fix:**
  - Choose the primary modifier per platform: `command` on macOS, `control` on Windows. On macOS, Ctrl+letter must fall through to the control-character table.
  - Split the test per platform.
  - On Windows, intercepting Ctrl+D/K/P/X in a shell conflicts with bash/WSL/PSReadLine. Spec K-29 maps ⌘→Ctrl there. TS on Windows never intercepted these (`metaKey` is the Win key). That is a user decision to make before Windows acceptance.

### F2 — MEDIUM — regression — confidence: source + pinned GPUI source + probe
**On macOS, Option+key is always sent as ESC+base key, so international layouts and dead keys can't type.**

- **Code:** `input.rs:139-143` (`m.alt && !m.command && key.chars().count()==1 → "\x1b"+key`). The view passes `&k.key`, never `key_char`.
- **Upstream:** at gpui_macos c84af5b `events.rs:433-466`, `key` is `charactersIgnoringModifiers` ("l") and `key_char` holds the produced character ("@" on German). `prefer_character_input` is always false on macOS.
- **Trigger:** German, French, Swiss, Nordic and other layouts typing `@ [ ] { } | \ ~` (all Option combos), Option-dead-key accents, or US Option+1 = `¡`.
- **Consequence:** the shell receives `ESC l` and similar sequences instead of the character. TS xterm.js used the default `macOptionIsMeta:false`.
- **Test fidelity:** `option_symbols_and_bare_numbers_are_text` feeds `key="¡"`, which real dispatch never provides (it gives `"1"`). It therefore only proves "not a session shortcut", not "is text". Probe `probe_macos_option_character_is_meta_encoded` gives `"\x1bl"` and `"\x1b1"`.
- **Fix:** on macOS, when `alt` is held and `key_char` is `Some` and differs from `key`, return `PlatformText` so `insertText` delivers the character. Keep the ESC prefix only behind an explicit Option-as-Meta preference. Windows AltGr is already correct via `prefer_character_input`.

### F3 — MEDIUM — regression — confidence: source + upstream xterm.js 6.0.0 (`node_modules`) + probe
**Mouse-wheel and trackpad scrolling diverge from TS: no alternate-scroll, sub-line pixel deltas lost, one report per event.**

- **Code:** `view.rs:104-144` and `input.rs:169-191`.
  - (a) In the alternate screen without mouse reporting, `input::mouse` returns `None` and `grid.scroll()` has no history to move, so `less`, `man`, `git log` and other pagers cannot be wheel-scrolled. xterm.js (bundled 6.0.0) sends `ESC[A`/`ESC[B` (or `ESCO A/B` under APP_CURSOR) when `!hasScrollback`. Alacritty's default `TermMode` includes `ALTERNATE_SCROLL`, but nothing reads it.
  - (b) `scroll((delta/height).round())` runs per event with no accumulator, so macOS trackpad pixel deltas below half a line (about 7 px at 12 pt) never scroll the scrollback.
  - (c) In mouse mode, every scroll event sends exactly one report regardless of magnitude, and a horizontal-only event (`delta == 0`) maps to `Wheel(false)`, a **wheel-down** report (`\x1b[<65;…M`). xterm.js `consumeWheelEvent` accumulates per cell height and ignores `deltaY == 0`.
- **Probe:** `probe_mouse_wheel_has_no_alternate_scroll_and_zero_delta_is_wheel_down` reproduces (a) and (c) at the encoder level. (b) is confirmed from source.
- **Fix:**
  - accumulate pixel deltas (as the zoom accumulator already does) and emit one line or report per whole cell;
  - ignore zero-y events;
  - when `ALT_SCREEN && ALTERNATE_SCROLL && !MOUSE_MODE`, send APP_CURSOR-aware arrow keys per line;
  - add encoder and accumulator tests.

### F4 — MEDIUM — new composition defect — confidence: exact source grep
**The terminal view's pin list goes stale and later overwrites UiState: pins are lost or resurrected, and order is reshuffled.**

- **Code:**
  - `TerminalView::new` copies `ui.terminal_pinned_sessions` into `State.sessions.pins` (a `BTreeSet`), `mod.rs:71`.
  - `TerminalView::update_pins` (`mod.rs:273`) has **zero callers** in the PIN.
  - The command bar's own ⌘P (`command_bar/interactions.rs:105-113`) and delete mutate only `Launcher.state.terminal_pinned_sessions`, as an ordered `insert(0)` list.
  - The terminal's `pin()`/`delete()` (`state.rs:910-930`) emit `Pins(stale_set ± id)`, and `terminal_pins` (`terminal_integration.rs:218-221`) assigns that and persists it.
- **Trigger:**
  1. Open the terminal (view created, pins {}).
  2. Go back to `>` and press ⌘P on session B (UiState [B]).
  3. Return to the terminal on A and press ⌘P.
- **Consequence:** UiState becomes [A] and B's pin is lost. A session deleted from the command bar is re-added on the next terminal pin toggle. The order becomes lexicographic by id instead of most-recent-first, and the command bar sorts by list position (`mod.rs:1805-1811`).
- **Fix:** make UiState the single owner. Emit `TerminalEffect::TogglePin(id)` / `Unpin(id)` and apply them to the launcher's ordered list, or call `update_pins` on every persist and drop the `BTreeSet` copy. Add a two-surface integration test.

### F5 — LOW/MEDIUM — new — confidence: source + probe
**Hiding the launcher while an Attach is in flight never detaches the session in core, so keep-alive never starts.**

- **Code:** `state.rs:267-274`. `detach_hidden` sets `visibility_detached = true` even when `stream_generation` is `None`, and `detach_request` then sends nothing. When the attach completes, `install_stream → detach_hidden()` (`state.rs:605`) does nothing because the flag is already set. `dispose/select → detach_active` also skip detaching because of the flag (`state.rs:229`).
- **Trigger:** the user hides the launcher, or blur hides it, before Attach or Create→Attach returns. This happens on navigation to `>` terminal, ⌘1-9 selection, or panel selection. The window is the attach round trip, which includes a config metadata write.
- **Consequence:** core never receives `terminal:detach`, `keepAliveUntil` is never set, and the shell runs past its keep-alive policy (3h/8h/day) until the next show/hide cycle or app quit.
- **Probe:** `probe_hide_while_attach_in_flight_never_detaches` installs the stream while hidden, disposes, and sees no `DetachGeneration`.
- **Fix:** set `visibility_detached` only when a `DetachGeneration` is actually enqueued (that is, a generation exists). Otherwise leave it false so that `install_stream` sends the detach.

### F6 — LOW (Windows: PLAUSIBLE) — new — confidence: source + probe + upstream portable-pty 0.9.0
**On a fresh session, device queries printed before the attach watermark are never answered.**

- **Code:** `state.rs:640-650`. Every byte before `watermark` is treated as historical and its replies are discarded. After a *new* Create, those bytes belong to the current live process, which no other viewer has answered.
- **Upstream:** portable-pty 0.9.0 `win/psuedocon.rs:87` creates ConPTY with `PSEUDOCONSOLE_INHERIT_CURSOR`. ConPTY then emits `ESC[6n` at startup and waits for the cursor report. Depending on the Windows build this is a timeout delay or a stall. TS answered it through xterm `onData` after the replay.
- **Probe:** `probe_fresh_create_startup_device_query_is_never_answered` shows 0 Input requests for a pre-watermark `\x1b[6n` after Create.
- **Consequence:** likely a delayed first prompt or a lost cursor inherit on Windows. macOS bash/zsh are unaffected; shells that query DA at startup can wait for a timeout.
- **Fix:** have core return the generation's origin offset (the Stream's initial length) in create/attach. For the first attach after this owner's own Create, treat bytes at or beyond origin as live for replies. Keep suppression for restored prior-process history and for re-attach. Verify on real Windows.

### F7 — LOW — new — confidence: source
**Mouse motion reports are sent for every pointer-move event, even when the cell has not changed.**

- **Code:** `view.rs:85-102`. There is no last-reported `(col,row,button)` check. Each report is a separate serial CoreClient Input request.
- **Consequence:** under 1002/1003 drags (vim `mouse=a`, tmux, htop), the PTY receives duplicate reports at pointer rate. The 60-slot work queue can refuse keystrokes with "Terminal input is still draining". xterm and Alacritty report only on cell change.
- **Fix:** remember the last reported cell and buttons and skip duplicates. Optionally coalesce queued motion.

### F8 — LOW — new — confidence: source
**Keys and IME commits typed during replay are silently discarded.**

- **Code:** `state.rs:767-800`. `write`/`write_bytes` return early when `replaying`, with no error. Replay always runs from byte 0 in serial 64 KiB round trips with GUI-thread parsing, so a long durable log takes many turns.
- **Consequence:** right after selecting a large-history session, typed input vanishes with no feedback. TS forwarded input immediately.
- **Fix:** keep a small bounded pending-input buffer (already fenced by generation) and flush it when replay finishes, or show an explicit status. Add a long-history RAM/latency gate.

Lower-impact observations (not counted as findings):
- The grid accepts `mailto:`, but the host's `allowed_url` (`app/m10.rs:74`) allows only http/https, so ⌘-clicking a mailto link does nothing.
- On Windows, `validate()` reopens the history file on every 64 KiB read. That is a performance cost only, and the code is uncompiled on Mac.
- `TerminalGrid::new` sets LNM while the adapter forces CR on LF anyway. Behaviour equals xterm `convertEol`; the comment "programs can disable LNM" is inaccurate.
- `dispose()` has no production caller. The view is retained for the app's lifetime, and core shutdown is canonical.

## 3. Per-ID verdicts

PARTIAL means local evidence covers part of the row and gates remain. No row is green.

| ID | Verdict | Evidence | Blocking / remaining |
|---|---|---|---|
| T-01 create | PARTIAL | Create through the same core owner. Private real-PTY fixture passed (my rerun). Listeners are ready before Create. | Real login shell/ConPTY, F6 (Windows), manual |
| T-02 attach/detach + replay | PARTIAL | Bounded stream attach, positions and generation hold; replay registry tests and state tests pass. | F5 (detach lost), F8, long-history RAM/latency, Windows |
| T-03 write/resize | PARTIAL | Atomic base64 input ≤1 MiB, raw X10 bytes, cell-metric resize; core plus fixture tests. | Windows ConPTY, manual |
| T-04 cwd tracking | PARTIAL | 80/350/1200 ms command-triggered retries, stale-revision fences; label tests. | Real lsof/process cwd on both OSes |
| T-05 kill/delete/prune/save-for/keep-alive | PARTIAL / defect | Same core retention owner; running history is protected. | **F1** (Ctrl+D/X destructive on macOS), F5 keep-alive not started |
| T-06 list/labels/pins/selection | **FAIL** | Selection and label cases ported. | **F4** pin desync and reordering |
| T-07 VT/colours/cursor/scrollback | PARTIAL | Alacritty VT, palette identical to TS `terminalTheme`, 100k cap, DECSET 1049 verified upstream. | GPU shaping and font fallback, visual review, 100-cycle/RAM |
| T-08 keys/shortcuts/bracketed paste | **FAIL** | Key table, atomic paste, GPUI dispatch tests. | **F1**, **F2**, F7; AltGr/keypad/dead keys on real platforms |
| T-09 text zoom | PARTIAL | 8..24, default 12, buttons, distinct UI zoom. | Wheel zoom manual |
| T-10 selection/copy/URL | PARTIAL | Alacritty selection, typed Copy/OpenUrl, host scheme check. | Physical drag/URL hit-test, mailto mismatch, **F3** scroll |
| T-11 IME | PARTIAL | Synthetic EntityInputHandler preedit/commit; UTF-16 ranges; shaper bounds. | Real Mac/Windows IME matrix, F8 |
| T-12 data/exit/sessions-changed | PASS (local layer) | Ordered positions, gap recovery, exit ordering, generation fences; core fixture asserts contiguous start/end and exit position. | Windows, long-running real process |
| T-13 sessions panel | PARTIAL | Existing panel; virtualized `uniform_list` over all rows; 1..9 within full list; new session. | No active highlight, ⌘N hints, subtitle or window title "Terminal Sessions" (§5.8); parenting/blur/multi-monitor manual |
| T-14 shutdown on quit | PARTIAL | Non-destructive prepare, drain in the App budget, revive on cancel, core `shutdown_all` + reaper abort; fixture PID reaped. | Manual quit with running sessions, Windows Job/ConPTY |
| L-17 terminal mode | PARTIAL / defect | Command-bar `>` actions exist (M5). | F4 interplay; command-bar rows reviewed only where M10 touches them |
| S-15 terminal surface | PARTIAL | Nonce activation, `>` back sentinel, configure-first Escape; integration test. | F1 Ctrl conflicts |
| S-18 sessions window | PARTIAL | As T-13. | Same |
| K-16 ⌘K ⌘X ⌘P ⌘D | **FAIL** | Mapped in the terminal. | **F1**: Ctrl is also intercepted on macOS; Windows decision |
| K-29 terminal keys | **FAIL** | Text vs UI zoom split tested. | F1, F2 |
| W-06 interface zoom | PARTIAL | `Alt`+⌘ → `Event::Zoom` ±0.1 / 0; terminal font stays local. | Global shortcut ordering, manual |
| W-09 panel geometry | PARTIAL | Constants test (300/266/96/86/260); follows bounds; 80 ms blur grace via `PanelFocused`. | Multi-monitor, DPI, parenting manual |

## 4. Test-map, design and standards judgments

### Test map
- `source-case-map.json` maps all **22** TS cases (History 3, Label 3, Selection 6, Transport 4, shared/terminal 6). I checked every title against the TS file and every native function name: no mismatches. The two hotkey/extension Transport cases are honestly marked parent-owned.
- However, the three History cases (`replay_*`) are **tautological** under the new design. Their pre-attach `stream_data` is dropped by design (`!running`), and the assertion only checks the replay bytes. They prove that text-overlap was replaced, not that overlap is handled. The positional property is really covered by `ordered_stream_repeated_bytes…` and `data_exit_before_attach…`.
- `option_symbols…` uses an unrealistic key value (see F2).
- `key_sequences…` encodes F1 as expected behaviour.
- The status "assertion subset only" is accurate. These cases must not be counted as parity proof.

### Test layers
These are distinct and honestly labelled:
- production state with a synthetic transport;
- GPUI TestPlatform;
- one private real-PTY core case (plus the non-PTY paste-admission case);
- no GUI, Windows, IME, AX or GPU.

### Design
- Flat `controls::sheet` / header / footer, Ghost/Quiet/Primary/Danger tones and tokens. No gradients, glass, side stripes, badges or default accents.
- The grid palette is byte-identical to TS `terminalTheme`. The dark-mode indigo selection `#6366f155` is the pre-existing token, not new chrome.
- The TS sessions window's `glass-card` / backdrop blur / emerald accents are correctly flattened.
- Gaps:
  - The native panel lost useful information that TS had: cwd subtitle, ⌘N hint, running count. There is no active-row selection state (`aria_selected=false` always).
  - The footer's mixed "⌘/Ctrl K" hint has no action label next to it.
- I made no visual acceptance; there are no screenshots.

### Standards
- Licensing: crates.io `alacritty_terminal =0.26.0` (Apache-2.0). No Zed GPL crate.
- Not re-verified by me (parent-recorded): lock delta of 7 new nodes from the Alacritty edge with no checksum drift; MSRV/check/build logs.
- `core:*` stays internal.
- Doc-comment inaccuracies, all harmless: the "Prepaint never schedules" comment and the LNM comment (section 1); `request.rs:1` matches the code.

## 5. Limits

- No Windows compilation or execution (the Windows prefix and file-identity code is uncompiled on Mac). No GUI launch, real IME, VoiceOver/Narrator, multi-monitor, GPU, RAM/long-history or 100-cycle measurement. No production profile, clipboard, credentials or live providers.
- The preserved binary `/tmp/tezbar-native-m8m10-b626f28…` is a dirty b626f28 preview, not a clean PIN build, and was not used.
- I did not rerun the full native suite (316 tests) or the full core suite. Those counts are the parent's recorded results.
- Sections 1 and 2 cover everything I read. That excludes command-bar code beyond what M10 touches and the M8 quit internals, which belong to a separate review.

## 6. Recommended fix order (root, small)

1. F1: platform-primary modifier.
2. F4: single pin owner.
3. F2: macOS Option text.
4. F5: one-line flag fix.
5. F3: wheel accumulator and alternate-scroll.
6. F7: motion dedupe.
7. F8: pending input during replay.
8. F6: needs a core origin offset, then real-Windows verification.

After the fixes, add regression tests for each (the probe file is a starting template), then run a fresh Opus recheck of the input and pin paths.
