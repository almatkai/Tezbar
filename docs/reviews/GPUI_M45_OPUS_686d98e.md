# Tezbar GPUI: Opus 5.5 review of M5 command bar and the final M4 residuals at `686d98e`

- **Reviewer:** Claude Opus 5.5. One agent, no subagents, no delegation, no external model, API or provider calls.
- **Target:** commit `686d98e56c967c98c5b38dd284c1d8a81ce0a14f` on `feat/native-gpui-app`.
  - Parent: `f3234cd`. Commit tree: `127f66f612bf4edc9f10e1bc0ee856a796d092b4`.
  - GPUI pin: `c84af5b99cbc366d675064e52b23664a015b6e35`.
- **Use of the live worktree:** read-only, for `git cat-file`, `git archive`, `git diff`, `git show` and `git ls-tree` only.
  - Nothing was edited, staged, reset, committed or stashed.
  - No live modified source was read as evidence. M6 is editing `TextInput` and new utility files concurrently; none of that was opened.
- **Scope:** the M4 closure (R-03a, R-04a and the original six repairs), and the full M5 correctness and design review.
- **Not covered:** this is **not** an M5 acceptance, an M3A/B model acceptance or a full-migration verdict.

## 1. Verdict

| Area | Disposition |
| --- | --- |
| M4 R-03a (complex Markdown lists) | **Closed.** Source and render traced against the pinned GPUI. Probe-confirmed for nested lists, code-first items, quote-in-item and list-in-quote, HTML lists, task lists, and streaming followed by settle. |
| M4 R-04a (semantic text on the footer) | **Closed.** The real composited footer is now checked for danger/success/warning/link. §5.2 now lists the AA corrections. |
| M4 N-01, N-02, N-05, N-06 and the text-input part of N-03 | **Still closed.** None of the touched paths changed after `b31de67`: `app.rs`, `controls.rs`, `text_input.rs`, `review_tests.rs` and the persist/drain code in `mod.rs` (`git diff b31de67 686d98e`). Only `tokens.rs` changed, adding the 5 R-04a lines. |
| M3 dependencies from `f3234cd` | **Sound for M5 use.** The AI cache is selected by input mode for accepted results and for the pending preview. A successful `ActivatePin` cancels the queued Enter; a rejected one keeps it. |
| **M5** | **NOT ACCEPTABLE YET.** **8 actionable gate defects:** 4 Medium, 4 Low. 6 are probe-reproduced on the frozen source and 2 are strongly traced. Details in §3. |

**No High-severity defect was found.** The core lifecycle all holds against source: exactly-once tickets, stale-generation authority, the IME/Enter ordering, the strict `QueryOwned` owner filter, frame coalescing, the bounded request admission and the typed navigation payloads. The defects sit at the edges of activation routes, the input-mode parser, pin gestures, the icon scheduler and the hotkey route.

## 2. Pin, archive and evidence verification

| Item | Result |
| --- | --- |
| Commit object | `git cat-file -t 686d98e…` = commit; parent `f3234cd`. `686d98e` changes only the 12 M5 files (+3605/−505). `git diff f3234cd 686d98e` shows **no change** to `crates/`, `app.rs`, `events.rs` or `Cargo.lock`. |
| Frozen archive | `git archive 686d98e…` extracted to `/tmp/tezbar-m45-opus-686d98e-archive` (outside the tree, no `.git`). **1859/1859 blob ids match** `git ls-tree -r` (`git hash-object --no-filters`); 0 mismatches. |
| Parent compiled inputs | The 75 paths in `/tmp/tezbar-native-m05-evidence/final-v3-compiled-source-sha256.json`: **75/75 match the archive**. This confirms that `686d98e` contains the compiled v3 bytes. |
| Map hashes | All 13 `source_sha256` entries and all 18 `reference_sha256` entries in `command-bar-test-map.json` match the archive. |
| Preserved binary | `tezbar-m05-final-v3` SHA-256 = `4191758b…9c18`, which matches. I did not re-inspect its linkage, did not launch it, and did not rerun its gates. |
| Parent logs | Read only: `final-v3-normal-tests.log` (85 + 43 passed), `final-v3-bench-tests.log` (86 passed). I did not rerun the baselines because no new concern touched them. |
| Prior reports | `GPUI_M4_OPUS_b31de67.md` = `6f8e7888…ae13` (expected). `af1ccdd` = `75f2ae0f…66a3`; `82688f7` = `546c2a20…1273`. All are preserved unchanged. |
| XERJ | One bounded `def schedule_icon_viewport` (prefix `ax-65b00e69da24`) returned an unrelated legacy JSON file, so the index is stale. I did not reindex. Every conclusion comes from exact archive reads, `rg` and `git`. |

## 3. M5 findings (gate defects, by severity)

### F-01 (Medium, probe-confirmed): a directory-AI prompt containing `>` turns into terminal mode and loses the prompt

- **Location:** `apps/tezbar/src/surfaces/command_bar/mod.rs:474-482`. `query()` runs `text.split_once('>')` on every committed text that does not start with `!`, space or `` ` ``. It switches to terminal whenever the part before `>` is "absolute" (`is_absolute_path` only checks for a leading `/`).
- **Trigger:**
  1. Type the directory-AI form `/Users/me/proj  why is a > b` (spec L-14, `commandBarInputMode.ts`).
  2. The moment `>` is typed, the bar becomes `>` terminal mode with `terminal_cwd = "/Users/me/proj  why is a"`. The AI task text is wiped.
  3. Enter then opens a Terminal whose working directory is a nonsense path.
- **Probe:** `opus_probe_directory_ai_prompt_with_gt`:
  - before the `>`: `mode=Ai`;
  - after the `>`: `mode=Terminal terminal=true input=">" cwd=Some("/fixture  why is a") field=""`.
- **TS reference:** `CommandBar.tsx:4010-4025` enters terminal only on a `>` keypress when `(!value || isSlashInput) && !isAiMode && !launcherArgumentMode && !pendingAction`. A slash value that is in AI mode never converts.
- **Fix:** gate the conversion on `launcher::input_mode(&text_before_gt, false).mode == Mode::Path` and on the absence of `"  "` (or reuse the `is_ai` result), plus no form or pending action. Add a native regression with the probe's text.

### F-02 (Medium, traced; parity and safety): "Quit Tezbar" quits immediately, without the confirmation dialog

- **Location:**
  - `mod.rs:1099-1101` maps `RouteEffect::QuitConfirm` to `Event::Quit`;
  - `app.rs:891` maps `Event::Quit` to `self.quit(cx)`, which begins quitting immediately (`app.rs:311-`).
- **Unused path:** `Request::ConfirmQuit` (`core_client.rs:88,145`, `app:confirm-quit`) exists but is never sent.
- **Trigger:** type "quit", then Enter on *Quit Tezbar*. The app shuts down without asking. Terminals and background tasks are killed.
- **Reference behaviour:**
  - TS `tauri-bridge.ts:500-503` calls `app:confirm-quit` first.
  - Core `platform.rs:346-351` shows "Quit Tezbar? … Background tasks and active terminal sessions will stop." with Cancel/Quit.
  - Ledger A-11 says "`quit-tezbar` → confirm".
  - The native host already supports `DialogRequest::Confirm` (`app.rs:1117-1169`) and `SurfaceOp::Quit` (`app.rs:469`).
- **Test-map issue:** `saved_note_pin_preserves_note_id_and_quit_command_only_emits_explicit_intent` only asserts that `Event::Quit` is emitted, so A-09/A-11 "implemented" overstates the behaviour.
- **Fix:** send `Request::ConfirmQuit` through the adapter, and let core's confirm decide `SurfaceOp::Quit`. Test that a confirm is requested and that Cancel leaves the launcher usable (see F-03).

### F-03 (Medium, probe-confirmed): non-navigating route activations leave the launcher visible but dead

- **Affected routes:** "Check for updates", "Download update", "Restart to update", and the quit route once F-02 is fixed and the user cancels.
- **Location:**
  - `mod.rs:1086-1124` calls `finish_local(&a)` before the route effects. That is `navigation_complete`, `mod.rs:931-940`, which sends `ActivationComplete Ok`.
  - The model handles that with `reset_hidden()` (`controller.rs:383-395`): `model.visible=false`, input cleared, `session.hide()`. Its `Effect::Hide` is discarded.
  - Navigating routes then call `navigate()`. `ActivateDeepSearch` re-sets `model.visible = true` (`mod.rs:1104`, the same patch as `:1036`).
  - `UpdateAction` (`:1102`) and `QuitConfirm` (`:1099`) get no such patch. The view stays `visible=true` with a hidden model.
- **User-visible consequence:**
  - The query vanishes and the footer stays on "Searching…", because `pending()` stays true when nothing is in flight.
  - Pin-rail clicks and ⌥ slots do nothing: `ActivatePin` requires `self.visible` (`controller.rs:330-336`).
  - Enter only arms a 1 s deferred Enter that expires.
  - `refresh_update_rows` returns early while `pending()` (`mod.rs:1373-1378`), so the "Checking for updates…/Downloading…" row never appears. TS keeps the query and live-updates the status row (`CommandBar.tsx:3053-3100`).
  - The launcher recovers only after the next keystroke or a hide/show.
- **Probe:** `opus_probe_{update,quit}_route_keeps_launcher_usable`:
  - after activation: `view.visible=true model.visible=false pending=true rows=0 navigation=false`;
  - then `alt-1` on a saved pin: **0 executions**.
- **Fix:** give non-navigating route effects a model completion that keeps the session open. Either add a controller input such as `ActivationHandled { ticket }` that clears the ticket without `reset_hidden`, or finish with `Err`-free retention and re-commit the current input. Do not hand-patch `model.visible`. Regression: after the update route, pins work, the footer is not "Searching…", and the status row updates.

### F-04 (Medium, probe-confirmed): a no-view or failed hotkey extension run takes over the next launcher open

- **Location:** `mod.rs:1490-1510`. The handler for `run-extension-command-from-hotkey` navigates to `Surface::ExtensionRuntime` for **every** `Ok(result)`, including `{ok:false}` and no-view `{ok:true,message}` results.
- **TS reference:** `App.tsx:568-575` navigates only when `result.ok && result.mode === 'view'`.
- **Trigger:**
  1. Bind a hotkey to a no-view command (a toggle or HUD command) and press it while the launcher is hidden.
  2. `navigate()` stores `self.navigation`, and `show()` never clears it.
  3. The next launcher open shows the "Extension / Back to commands" placeholder. `query()` returns early, so typing is ignored until Escape.
  4. The command's toast/HUD notice is never shown.
- **Probe:** `opus_probe_no_view_hotkey_result_hijacks_next_open`: after `show()` and typing `abc`, `navigation=Some(ExtensionRuntime) model.input="" searches=1` (the home search only).
- **Test map:** A-24 has no test.
- **Fix:** reuse the result classification from `execute_activation` (`mod.rs:1186-1200`), with the same view/root/session/preferenceSetup predicate and `ok != false`. Present `WorkerRecords`/`message` notices otherwise. Keep the handoff for view results only. Regression for no-view, failed and view results.

### F-05 (Low, probe-confirmed): the first pin click after a reorder or unpin drag is swallowed

- **Location:**
  - `view.rs:1297-1300` sets `suppress_pin_click = true` on any active drop.
  - The flag is cleared only inside a pin's `on_click` (`view.rs:1225-1236`).
  - A drop on another slot, or outside the rail, produces no click for the dragged pin (GPUI fires a click only when down and up land on the same hitbox), so the flag survives.
- **Consequence:** the next intended pin click does nothing.
- **Probe:** `opus_probe_pin_click_after_reorder_drag`:
  - after reordering: `suppress=true`;
  - first click: **executes=0**;
  - second click: 1.
- **Test gap:** the existing test (`pin_drag_requires_six_pixels…`) never clicks after a drag.
- **Fix:** clear the flag at the next pin mouse-down, or record the suppressed drag id and compare it in `on_click`.

### F-06 (Low, probe-confirmed): the icon frame flag can stick, leaving the list without icons

- **Location:** `interactions.rs:23-30`. When the generation changed before the frame, the `on_next_frame` closure returns **without** clearing `icon_frame_pending`. Only `query()` (`mod.rs:490`) and `hide()` (`:557`) reset it.
- **Trigger:**
  1. A `launcher:invalidated` event (or a deep-search route `Input::Edit`) lands between a scheduled viewport frame and that frame.
  2. Every later `schedule_icon_viewport` call returns early.
  3. Meanwhile each search completion runs `icons.clear()` (`mod.rs:827-829`).
- **Consequence:** the refreshed list shows glyphs only until the next keystroke or hide. Scrolling does not recover it.
- **Probe:** `opus_probe_icon_frame_flag_survives_generation_race`:
  - baseline: 20 icon requests;
  - after the race, the new generation's rows are shown with `pending_flag=true` and **0** icon requests for them, including after scrolling.
- **Fix:** clear the flag before the generation check, then reschedule if the generation is current.

### F-07 (Low, probe-confirmed): commands whose arguments are all optional open a form instead of running

- **Location:** `presentation.rs:246-252` skips the form only when `submit()` succeeds **and** `argumentValues` is supplied.
- **TS reference:** `CommandBar.tsx:3308-3341` runs immediately when no definition is required. The inline-argument case is handled separately.
- **Trigger:** an extension command with only optional `commandArgumentDefinitions` gets an extra form step natively.
- **Probe:** `opus_probe_optional_only_arguments_open_form`: `form_open=true run_calls=0`.
- **Fix:** mirror TS. Show the form for inline arguments or required definitions; otherwise execute.

### F-08 (Low, contract and handoff): the `NavigationInbox` is single-slot, and update actions masquerade as a Settings navigation

- **Location:** `navigation.rs:70-86`; `app.rs:816-829`.
  - `put` overwrites any pending intent.
  - `Event::Update` stores `Navigation { nonce: 0, surface: Settings("general"), payload: Update(..) }` without opening or notifying anything.
- **Consequences once M7/M11 consumers land:**
  - A Settings owner calling `take(&Settings("general"))` on mount receives a stale `Update(Restart/Download)` intent.
  - Any later `Navigate` silently drops an unconsumed update action.
  - `nonce: 0` breaks the monotonic-nonce contract stated in `navigation.rs:2`.
  - Today nothing consumes the inbox and shipping defaults to the old app, so there is no user impact yet. But the stated "lossless handoff" is not true as built.
- **Fix:** give updater intents their own typed slot or channel with a monotonic nonce. Make the inbox per-surface, or a bounded queue, and document the take-on-mount contract with a test that sends two intents before consumption.

## 4. Observations and hypotheses (not counted as gate defects)

- **Dead or confusing code:**
  - In `mod.rs:790-793`, kind `2` returns at the `path:complete` branch. That is correct, because core handles `` ` `` launchpad completion (`providers.rs:657-690`). The `kind==2` Search filter below it is unreachable.
  - The `history` popover (`mod.rs:151`, `view.rs:1441-1466`) is never enabled; Cmd+H correctly routes to Clipboard instead.
  - `search` uses magic `u8` kinds.
- **Formatting:** the body of `search()` (`mod.rs:786-831`) and several `view.rs` render chains (`:1414-1432`, `:1470-1480`) are unformatted one-liners. `rustfmt --check` passes only because rustfmt gives up on them. This is a readability and review cost, not a behaviour defect.
- **Duplicated drop logic:** pin-drop handling is copied in three places (the capture handler around `view.rs:1308`, `on_mouse_up` around `:1356`, and `on_mouse_up_out` at `:1371-1383`). This duplication is how F-05 survived.
- **Icon blink:** `icons.clear()` runs on every search completion, including stale ones (`mod.rs:827`). This forces a one-frame icon blink per keystroke before the viewport refresh refills them from cache. Visual gate.
- **Currency row shifts selection:** the currency row is inserted at index 0 after the search completes (`mod.rs:879`). That shifts the selected index if the user pressed Down during the network fetch.
- **Raw vs effective update status:**
  - The search path's update row and `update_action` use the raw `update_status` (`mod.rs:807`, `:1411`).
  - `refresh_update_rows` uses `update_policy::effective` (`:1390`). TS acts on `effectiveUpdateStatus`.
  - They can disagree once the "checked recently" policy matters.
- **Windows keycaps:** the footer keycaps hardcode `⌘K`/`⌘P` on Windows (`view.rs:1588-1591`).
- **Answer cap:** the 1 MiB answer cap (`mod.rs:1529`) drops an oversize token but accepts later smaller ones, so the text becomes non-contiguous. Edge case.
- **Hypothesis, admission exhaustion:** admission is `try_acquire_owned` with 16 permits (`core_client.rs:174`, `:330`). On a hanging network, each currency keystroke queues behind core's currency mutex for up to 2×15 s while holding a permit. That could exhaust admission and fail searches with "Native operations are still finishing". Not reproduced.
- **Accessibility:** the model picker's keyboard highlight (`model_choice`) is not exposed as an active descendant; `aria_selected` marks the active model instead (`model_picker.rs:118-140`). This needs a VoiceOver gate.
- **Migration note for M11:** `recent_extension_commands` uses native row ids. TS uses `buildRecentExtensionCommandId(extensionId, commandName)`, so the import must map them.
- **Markdown (pre-existing, outside R-03a):**
  - HTML `<ol>` items lose their numbers, because `start_item(true)` computes a marker and discards it (`markdown_html.rs:113-118`). The probe shows `"Three"`/`"Four"` with no `3.`/`4.`.
  - While streaming, a loose list that is split at a blank line shows as two lists until the settled reparse (probe: `[0..1,1..2]`, then `[0..2]`).

## 5. M4 closure details

### R-03a: closed

- **Parser** (`markdown.rs:316-507`; `ListContext` `:49-104`):
  - each flushed block records `list_path` from the frames that have an open item;
  - `Tag::List`/`TagEnd::List` push and pop a frame with a fresh id;
  - `Tag::Item`/`TagEnd::Item` open and close an item;
  - code, paragraph, quote and rule blocks inside an item inherit its path.
- **Incremental parsing:** chunks seed `next_id = offset`. A list needs at least one byte, so id ranges are disjoint between the prefix and the tail.
- **HTML lists:** `<ul>`/`<ol>`/`<li>` share the context with `html=true` frames. Whitespace between items is ignored (`markdown_html.rs:96-120,170`).
- **Render** (`render_block_groups`, `:660-700`):
  - recursion per depth: a `List` container per list id and a `ListItem` per contiguous item id;
  - leaf rows are `take()`n exactly once;
  - every block pushes exactly one row, including details blocks, so the indices stay aligned.
- **Footnote scroll:** `block_groups` at depth 0 indexes the root children, which is consistent with `ScrollHandle::scroll_to_item`.
- **Probe** (`opus_686_probe`, on a copy):
  - list-in-quote `[0..2,2..3]`; quote-in-item `[0..3]`; code-first item `[0..3]`; HTML `<ol>` `[0..2]`; tasks `[0..2]`;
  - streaming then settled: `[0..1,1..2]` → `[0..2]`;
  - real GPUI render consumed every leaf for 4 complex sources.
- **Remaining:** a VoiceOver/Narrator announcement gate.

### R-04a: closed

- `tokens.rs:267-271` adds `check(text, sheet.blend(t.bar_float), "{name} on footer")` for danger/success/warning/link, for both themes.
- The spec table now has hover `#6063e6` and light success/warning `4 120 87`/`180 83 9`, plus the dark text-3 `#979ba7` note (`NATIVE_GPUI_APP_SPEC.md:641-654`).
- **M5 informational text:** `text4` appears only in disabled buttons (`controls.rs:33-34`) and the empty-field placeholder (`text_input.rs:872-874`). The M5 hint, path headings, kind labels and model metadata use `text3`. The b31 observation is resolved.

## 6. Test-map truthfulness

- **Structure:** 50 listed tests. Every listed name exists, and every real `tests.rs` test is listed. There are 88 rows (L 32, A 25, K 31): 76 `implemented-unaccepted` and 12 `destination-later-milestone`. All hashes match.
- **Implemented rows with no test:** L-09, L-21, L-23, L-32, A-14, A-20, A-22, A-24, A-25, K-01, K-02, K-04 and K-05.
  - A-24 is defective (F-04).
- **Overstated rows:**
  - **A-09/A-11:** quit "→ confirm"; the test only checks the intent (F-02).
  - **L-27:** "status row … and its actions"; the status row never refreshes after activation (F-03).
  - **L-28:** "drag to reorder"; a click after a drag is lost (F-05).
  - **L-14:** the `/dir  task` form is broken by `>` (F-01).
  - **L-07:** icon replacement can stall (F-06).
  - **L-20:** argument parity with TS's optional-only rule (F-07).
- **K rows:** their `behavior` column holds only a context label ("Launcher", "Any surface"), not the key binding. Their claims cannot be checked from the map alone. Add the key to each row.
- **Accurate rows:** the remaining limits text (manual gates, destination ownership, OS effects) is accurate. The map does not claim screenshots, AX, IME, Windows or RAM.

## 7. Design and standards verdict

- **Architecture:** sound.
  - Render is pure: it registers listeners and builds elements, and starts no request or timer.
  - All I/O goes through `RequestAdapter`, with canonical core channels and one execute envelope.
  - Admission is owned by the core task.
  - There is no WebView, JS, Tauri, Node/Pi, provider client or cloned ranking. Core keeps search, safety and stores.
  - `QueryOwned` is additive and validated (owner non-empty, at most 256 bytes; `ai/mod.rs:895-902`). The query returns its intent immediately and the stream is emitted as an `{owner,value}` envelope (`memory.rs`). The view filters on the exact owner.
- **Visual:** no AI-template anti-patterns. There are no gradients, glass cards, decorative badges or side accent stripes. The accent is the legacy brand `#5b5ef0` from the spec, and the pin-rail divider is a hairline separator.
- **Quality debt:** listed in §4 (formatting, dead branches, triplicated drop logic). It made F-03 and F-05 easier to miss, and should be paid down with the fixes.

## 8. Probes and commands (frozen source only)

- **Environment:**
  - probe copy: `git archive 686d98e` extracted into the session scratchpad (`…/fcfa730a-…/scratchpad/probe`); the pristine archive was left unmodified;
  - own target: an APFS clone of the prior review's target;
  - `CARGO_BUILD_JOBS=2 CARGO_INCREMENTAL=0 --offline --locked`, `+1.98.1`, `--test-threads=1 --nocapture`;
  - `TEZBAR_NATIVE_SOURCE_SHA=686d98e56c967c98c5b38dd284c1d8a81ce0a14f`, `--features runtime-shaders`.
- **Isolation:** synthetic `RequestAdapter`s only. No app launch, GUI, profile, clipboard, provider, network, hotkey or installed app.
- **Commands:**
  1. `cargo test -p tezbar-native --features runtime-shaders --lib opus_probe`: **7 probe tests ran, all printing the defect states quoted above.** The probes print and do not assert. Log: `/tmp/tezbar-gpui-m45-opus-686d98e-evidence/probe-run2.log`, SHA-256 `64807f84…f811`.
     - The first run (5 probes) is superseded by run 2 (7 probes, same results).
     - One intermediate compile error in my own probe (`HashMap` vs `Map`) is not counted.
  2. `cargo test -p tezbar-ui --lib opus_686_probe`: 2 passed (Markdown structure and render). Log: `probe-md.log`, `df8fd8cd…2eb5`. An earlier attempt failed on my own `test`-macro import and is not counted.
- **Probe sources:**
  - `probe-tests.rs`, `bec2bebd…e2af`: the archive's `tests.rs` (`2a9c33c7…a677`) with probes appended;
  - `probe-markdown.rs`, `3b5efca5…badd`: the archive's `markdown.rs` (`b6a414e3…0380`) with probes appended.
- **Location:** everything is under `/tmp/tezbar-gpui-m45-opus-686d98e-evidence/`. The parent's `/tmp/tezbar-native-m05-evidence/` was only read.

## 9. Not proven here (manual or platform gates still open)

- OS IME candidate windows; VoiceOver/Narrator for the result list, pins, comboboxes and Markdown lists; physical keyboards and Alt layouts.
- Native Quick Look and focus/blur, multi-monitor placement, and exact light/dark/zoom screenshots against the TS app.
- Icon pixels, GPU texture pressure and matched full-app RAM.
- The Windows build, GUI, ICU locale and Alt+F4 (all Windows gates).
- The real updater, the quit dialog, the extension worker/hotkey, PTY and provider streams.
- M6–M11 destination content and the M11 profile cutover.
- M3A/B models other than the two launcher controller repairs audited here.
- **M5 cannot be accepted** until F-01…F-08 are repaired with regressions and the manual gates above are run. M4 is closed on R-03a and R-04a, and still has its own manual AX/IME/window/Windows gates.
