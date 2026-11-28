# Tezbar GPUI M5 command bar: Opus 5.5 closure audit of `35dca9b`

- **Reviewer:** Claude Opus 5.5, working alone. No subagents, delegation, external models, providers, network or MCP calls.
- **Target:** `35dca9bad4c44198dbc1c1e2e7b687ab0b6a9b2f` on `feat/native-gpui-app`.
  - Commit tree: `1af3cb42194b34773d8d62a4ae41a120bc6e067a`.
  - Fixed point: `686d98e`. Intermediate: `30f7084`, which contains only the M6 note-selection and HTTP contracts and is out of scope.
- **Scope:** a targeted closure of original findings F-01 to F-08, plus regressions introduced by those repairs.
  - This is **not** an M5 acceptance or a full-migration verdict.
  - It makes no Windows, GUI, OS IME, AX, GPU or RAM claim.

## 1. Verdict

| ID | Severity | Disposition |
| --- | --- | --- |
| F-01 | Medium | **Closed** |
| F-02 | Medium | **Closed.** One narrow residual is reported as new finding R-2. |
| F-03 | Medium | **Closed for the update and quit routes.** The neighbouring Deep Search row route regressed: see R-1. |
| F-04 | Medium | **Closed** |
| F-05 | Low | **Closed** |
| F-06 | Low | **Closed** |
| F-07 | Low | **Closed** |
| F-08 | Low | **Closed for its original symptom** (no overwrite, no Settings masquerade). The repair introduced a new problem: see R-3. |

**Actionable repair regressions: 3** (one Medium, two Low). All three are probe-reproduced on the pinned source. M5 cannot close this repair round until R-1 to R-3 are fixed and have regressions.

M4 residuals: nothing in this diff touches the M4 durable-state, text-input or footer paths. The three M4 durable-state fixtures pass on the clean archive. Nothing is reopened.

## 2. Immutable pin and status proof

- **Worktree:** `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-gpui-app`.
  - HEAD was `35dca9b…` at the start of the audit.
  - Uncommitted M6 files (`app.rs`, `core_client.rs`, `command_bar/{mod,view}.rs`, `tezbar-ui`, utilities) were present then. **I did not read any of them.**
- **HEAD moved during the audit, by the parent and not by me.**
  - The parent committed `7e21f21 feat(gpui): add native utilities and canonical rich notes editor`. The worktree is now clean.
  - `35dca9b` is an ancestor of `7e21f21`.
  - `7e21f21` is **not reviewed here.** It changes `app.rs`, `command_bar/mod.rs` and `view.rs` and adds `utilities_integration*.rs`.
  - Every conclusion below is about `35dca9b` only.
- **My own actions in the worktree:** I did not edit, stage, reset, switch, commit, push, install or launch anything there. I used only `git rev-parse`, `log`, `diff`, `show`, `ls-tree`, `archive`, `merge-base` and `grep`.
- **Archive:** `git archive 35dca9b` produced a tar with SHA-256 `1676bd17…426b`; `get-tar-commit-id` returns the pin.
  - It was extracted to the session scratchpad as `src/` (pristine) and `probe/` (a copy with probes appended).
  - **1864/1864 blobs match `git ls-tree -r`** (`git hash-object --no-filters`; symlinks hashed by their target). There were 0 mismatches.
- **Original report:** `docs/reviews/GPUI_M45_OPUS_686d98e.md` in the archive and `/tmp/tezbar-gpui-m45-opus-686d98e.md` both hash to `1be0dcfd…cd8` (expected). Both are unchanged.
- **XERJ:** not used. The `ax-65b00e69da24` index is known to be stale, and I was told not to reindex. Every claim below comes from exact archive reads, `rg` and `git`.

## 3. Per-ID closure

### F-01: directory-AI prompt containing `>` (Closed)

- **Repair:** `mod.rs:474-484` now converts to terminal only when all of these hold:
  - not in terminal mode;
  - no `form` and no `form_activation`;
  - the text before `>` is empty, **or** it is `input_mode(path).mode == Path`, has no `"  "`, and is an absolute path.
- **Why it now matches TS:** this agrees with TS `CommandBar.tsx:4010-4025`, which converts only when `(!value || isSlashInput) && !isAiMode && !launcherArgumentMode && !pendingAction`.
- **Earlier exclusions are still covered by the mode check:**
  - the old `!text.starts_with(['!',' ','`'])` check is gone, but Deep, AI and Applications inputs all classify as non-`Path`;
  - in `input_mode` (`launcher.rs:129-160`), `dir` AI mode requires a `"  "` separator, so the check is consistent.
- **Evidence:**
  - the permanent regression `review_directory_ai_prompt_preserves_greater_than` passes on the clean archive;
  - non-regression probe `opus_probe_path_terminal_still_converts`: `/fixture ` then `> ls` gives `terminal=true input="> ls" cwd=Some("/fixture")`.

### F-02: Quit skips confirmation (Closed; residual in R-2)

- **The route:**
  - `RouteEffect::QuitConfirm` now calls `confirm_quit` (`mod.rs:1362-1383`), which sends `Request::ConfirmQuit` (`app:confirm-quit`);
  - no direct `Event::Quit` is emitted;
  - the activation is not released before the dialog (`mod.rs:1096-1101` skips `finish_open` when `QuitConfirm` is present).
- **Core path:** `platform.rs:346-351` shows the dialog and calls `SurfaceOp::Quit` only on confirm.
  - Core test `host_quit_occurs_only_after_accepted_confirmation` covers this (not rerun here).
  - The host `Event::Dialog` path suppresses blur-hide and shows the launcher (`app.rs:1113-1150`). This matches spec §W, "Blur-hide is suppressed while they're open".
- **Evidence:** the strengthened fixture `saved_note_pin_preserves_note_id_and_quit_command_only_emits_explicit_intent` passes. It asserts exactly one `app:confirm-quit` request and no `Event::Quit` within 5 ms.

### F-03: non-navigating routes leave a dead launcher (Closed for update and quit)

- **Model change:** `Input::ActivationHandled` (`controller.rs:399-406`) releases only a matching ticket.
  - It calls `session.activation_finished` and leaves `visible`, `input` and `rows` unchanged.
  - Pure regression `handled_local_activation_preserves_live_query_and_rejects_stale_ticket` passes; 91 of 91 launcher model tests pass.
- **Update route:** `finish_open`, then `update_action`.
- **Quit route:** `finish_open` runs after the confirm reply.
- **Real-view regression:** `review_cancelled_quit_and_update_keep_the_current_query_usable` passes.
  - Model visible, not pending, not activating, query kept.
  - The "Checking" row refreshes.
  - The ⌥1 pin executes once.
- **Normal navigation keeps its reset contract:**
  - the session, terminal, vault, note and extension-view paths still call `finish_local` (`ActivationComplete Ok`);
  - router `Navigate` and `NewItem` effects now call `finish_open` first, but `navigate()` dispatches `Input::Hide` (`mod.rs:1325`), which resets the model anyway.
  - Probe `opus_probe_router_navigation_still_resets…`: `navigation=Some(Notes) model.visible=false input="" activating=false rows=0`. The resulting state is the same as before.
- **Message completion** (`mod.rs:1221-1240`) is unchanged.
- **Exception:** the `activate-deep-search` row route regressed. See R-1.

### F-04: failed or no-view hotkey run hijacks the next open (Closed)

- **Shared predicate:** `presentation::extension_has_view` (`presentation.rs:425-431`) requires `ok != false` plus one of view mode, `root`, `sessionId` or `preferenceSetup`.
- **Callers:** both the activation path (`mod.rs:1198`) and the hotkey handler (`mod.rs:1531`) use it.
- **Other results:**
  - `ok:false` becomes `error`;
  - anything else presents only `WorkerRecords::notice` or `message` (`presentation.rs:432-444`);
  - no effect is replayed and no `search:execute` is sent.
- **Evidence:** the three real-view fixtures (no-view, failed view-shaped, successful view) pass on the clean archive.
- **Observations, not defects:**
  - TS (`App.tsx:568-575`) shows nothing for a non-view hotkey result. The native side stores a notice or error even while the launcher is hidden, so it can appear on the next open for up to 4 s, or until typing clears the error.
  - This is a harmless superset of TS. Visual QA may decide whether it is wanted.

### F-05: first click after a pin drag is swallowed (Closed)

- **Repair:** the pin `on_mouse_down` handler clears `suppress_pin_click` (`view.rs:1213`).
- **The drag's own click stays suppressed:** a drag released on the same pin has its mouse-up handled in the capture phase, which sets suppress and takes `drag`. The bubble-phase `on_click` then sees suppress, skips activation and clears the flag.
- **Evidence:** `review_pin_click_after_reorder_runs_on_first_click` passes. It runs a real reorder gesture, then one click, and gets exactly one execute.
- **Quality debt:** the triplicated drop logic noted in the original §4 is still present. It is not a functional defect.

### F-06: icon frame flag stuck after a generation race (Closed)

- **Repair:** the `on_next_frame` closure always clears `icon_frame_pending` and calls `refresh_icon_viewport` (`interactions.rs:20-28`).
- **Why that is enough:** `refresh_icon_viewport` re-reads the live mode, viewport and rows, and re-prepares icons whenever the generation differs.
- **After hide:** a late closure returns early on `!visible`.
- **Evidence:** `review_icon_frame_tracks_the_latest_generation` passes. An invalidation between scheduling and paint still requests `/new/` icons.

### F-07: all-optional arguments open a form (Closed)

- **Repair:** `presentation::arguments` returns `None` when there is no inline argument and no `required:true` definition (`presentation.rs:201-207`).
- **Required paths keep the form:** legacy `argumentName` (`required:true`) and inline arguments still open it.
- **Evidence:** `review_optional_only_arguments_execute_without_a_form` passes: no form, exactly one `extension:run-command`.
- **Pre-existing nuance, outside this ID:** an explicit empty `commandArgumentDefinitions: []` short-circuits to direct execution, while TS falls back to `argumentName`. This is an edge case and does not block closure.

### F-08: single-slot inbox; updates masquerade as Settings (Original symptom closed; new defect R-3)

- **Navigation inbox:** `NavigationInbox` is now an ordered `VecDeque`.
  - `put` deduplicates identical intents and refuses the 17th entry with an error.
  - `take(surface)` returns the first match; `acknowledge(intent)` removes an exact entry.
- **Update inbox:** `UpdateInbox` is separate and typed, with a checked monotonic nonzero nonce.
- **App wiring:** `Event::Update` now goes only to `UpdateInbox` (`app.rs:822-835`), so a Settings `take` can no longer receive an update action.
- **Evidence:** the three pure contracts pass.
- **New problem:** no consumer exists at the pin, so the bound turns into permanent refusal. See R-3.

## 4. Actionable repair regressions

### R-3 (Medium, probe-confirmed): the bounded navigation inbox has no consumer, so the 17th navigation in a process fails permanently, Settings included

- **Call sites:**
  - `app.rs:796-806` calls `NavigationInbox::put` for **every** `Event::Navigate`, including `Surface::Command` returns and Settings.
  - `rg` over the pinned `apps/` and `crates/` finds **no** non-test caller of `take` or `acknowledge`.
  - `UpdateInbox::take` is also never called.
- **Why the cap is never relieved:** intents carry fresh nonces (`mod.rs:1312`), so the dedupe never fires.
- **Trigger:** 16 navigations of any kind during one app run, for example opening Notes, Clipboard or Settings from the launcher, or core `app:open-surface` events.
- **On the 17th navigation:**
  - `put` returns `Err("That screen is still opening. Try again shortly.")` and `app.rs` returns early;
  - `receive_navigation` is skipped, and `self.settings(cx)` is never called, so **the real native Settings window no longer opens from the launcher**;
  - the launcher has already run `navigate()`: the placeholder is set and the model is hidden. It is left showing the error.
  - Nothing recovers this short of restarting the process.
- **Update inbox:** separately, the 17th "Check for updates" shows "Update requests are still pending" in preview.
- **Before the repair:** `put` overwrote, so navigation never failed. This is a regression of a working path.
- **Probe:** `opus_probe_inbox_without_consumer_blocks_seventeenth_navigation`, using the same `put` sequence as `app.rs`, gives `errors=[(17, "That screen is still opening…")]` and `rejected_after_17_checks=1`.
- **Fix:** admission must be paired with retirement for every surface that exists at the pin.
  - Acknowledge an intent as soon as its current owner has received it:
    - after `receive_navigation` for placeholder surfaces;
    - immediately after `self.settings(cx)` for Settings;
    - never enqueue `Surface::Command`.
  - Alternatively, keep only the latest intent per surface for surfaces with no mounted owner, which bounds the inbox by the number of surfaces.
  - Return-home, Back and hide must acknowledge the view's current navigation.
  - Updates need an equivalent drain: the preview acknowledges after retaining the action, or the inbox keeps the latest action per kind until M11.
- **Regression to add:** at least 17 sequential Settings and placeholder navigations through `Owner::event` (or an extracted helper) all succeed. Settings opens every time, and the inbox length stays bounded.
- **Post-pin note, not reviewed:** a quick look at `7e21f21` shows `utilities_integration.rs:73` acknowledging utility intents only. Settings, Command, ExtensionRuntime, AI and Terminal intents should be re-checked there.

### R-1 (Low to Medium, probe-confirmed): the Deep Search row route leaves the model hidden, so pins, ⌥ slots and invalidation refresh stop working

- **Call site:** the `activate-deep-search` branch of `execute_activation` (`mod.rs:1042-1055`).
  - It calls `finish_local` (`ActivationComplete Ok`, then `reset_hidden`, so `model.visible=false`) and then dispatches `Input::Edit("!q")`.
  - The repair **removed** the old `self.model.visible = true` patch here without switching this branch to `finish_open`.
  - The router `ActivateDeepSearch` arm was correctly moved to `finish_open`, but core rows use `invoke-command`. Both the `deep-search:prompt` and `deep-search:<q>` rows (`tezbar-core/src/launcher/mod.rs:675,752`) reach this branch, not the router arm.
- **Trigger:** search for something with no strong match, then Enter or click on "Deep Search “q”".
- **Consequence:** the view is visible and deep-searching, but `model.visible=false`.
  - `Input::ActivatePin` requires `self.visible` (`controller.rs:330-336`), so pin-rail clicks and ⌥1… do nothing until hide and reopen.
  - `Invalidate` and `BackendChanged` only mark the model dirty instead of re-committing.
  - This is the F-03 class of defect, reintroduced on a neighbouring route.
- **Probe:** `opus_probe_deep_search_row_route_keeps_model_visible` gives `view.visible=true model.visible=false input="!needle"` and **0** pin executions after `alt-1`.
- **Fix:** replace `finish_local` with `finish_open` in this branch, as already done for the router arm.
- **Regression to add:** activate a Deep Search row, then ⌥1 a saved pin, and expect exactly one execution.
- **Map:** L-12 has no fixture for row activation.

### R-2 (Low, probe-confirmed): cancelling quit after a generation change leaves the activation held

- **Call site:** `confirm_quit` (`mod.rs:1372-1375`) returns early when `activation.generation != session.generation`, **without** `finish_open`.
  - Only typing (`Edit`/`retire_activation`), composition, hide or a backend change clears the held activation.
  - `Input::Invalidate` increments the generation without retiring the activation (`controller.rs:434-447`).
- **Trigger:** a `launcher:invalidated` core event while the quit dialog is open (for example an app index or extension change), followed by Cancel.
- **Consequence:**
  - `session.activating` stays true;
  - Enter, row clicks and pin or ⌥ slots do nothing until the user types;
  - the query and rows look live.
- **Probe:** `opus_probe_quit_cancel_after_generation_change_releases_activation`:
  - after cancel: `activating=true rows=1 input="fixture"`;
  - pin executions: **0**.
- **Fix:** always call `finish_open(&activation)`. `ActivationHandled` already ignores stale tickets, and the normal execute path also releases on a generation mismatch (`mod.rs:1186-1189`). Gate only the error presentation on the generation.
- **Regression to add:** the probe sequence, with assertions.

## 5. Spec and functional observations (not counted)

- **Hidden-launcher hotkey notices and errors** (see F-04): a superset of TS; a visual QA decision.
- **F-07 empty-array edge** (see F-07): pre-existing.
- **Router navigation order:** it now relies on `navigate()`'s `Input::Hide` reset instead of `finish_local`. The effect is equivalent today, but a future `navigate()` that stops hiding the model would silently break it. A short comment or using `finish_local` for `Navigate`/`NewItem` would make it explicit. This is optional.
- **Remaining original §4 observations:** icon blink, the currency row shifting selection, raw vs effective update status, Windows keycaps, the answer cap, the admission-exhaustion hypothesis, the model-picker active descendant, the M11 recent-ID import and the Markdown `<ol>` numbers. They are unchanged and remain assigned as `review-repairs.md` states.

## 6. Standards and test-map observations

- **Map source hashes:** all 16 `source_sha256` entries match the pin's blobs, so I verified per-file hashes and did not rely on the root tree.
  - The recorded `source_tree 5866ade…` differs from the pin tree only in the 3 documentation files: the test map, `review-repairs.md` and the 686 report. That matches the "pre-doc index" note.
- **Map reference hashes:** 17 of 18 `reference_sha256` entries match. `crates/tezbar-ui-model/src/launcher/controller.rs` is recorded as `77032962…`, the f3234cd/686d98e version, while the pin has `00f9a3d7…`.
  - The same file is correctly hashed under `source_sha256`.
  - The stale reference entry should be updated or labelled historical.
- **Named tests:** all 61 exist in the archive (54 in `tests.rs` and `review_regressions.rs`, 3 in `navigation.rs`, plus the others); none is missing or unlisted.
  - The clean archive passes the whole command-bar namespace: 64 = 61 M5 and contract tests + 3 M4 `review_tests`.
  - The controller `ActivationHandled` regression passes in the `tezbar-ui-model` launcher suite (91/91).
- **Key bindings:** K-01 to K-31 now state their keys. The 11 `partial-unverified` rows (L-09, L-21, L-23, L-32, A-20, A-22, A-25, K-01, K-02, K-04, K-05) honestly say "No named native fixture".
  - The 12 `destination-later-milestone` rows are unchanged.
- **Evidence log references:**
  - The new tests cite `/tmp/tezbar-m5-review-parent-commandbar.log` (SHA-256 `d270722a…602e`), which the map itself says is an integration-tree run and not a clean archive.
  - Rows L-07, L-14, L-15, L-20, L-27, L-28 and A-11 list the new review tests but still give the historical `final-v3-normal-tests.log` as `evidence`.
  - The strengthened note/quit fixture's `log` is also the v3 log, which predates its new assertions.
  - These are documentation precision issues, not false claims. My clean-archive log below can be cited instead.
- **Historical build:** `f3234cd` and v3 remain historical. The map does not call them new binary proof. I built no binary.
- **Overstated rows:**
  - L-12 (Deep Search navigation activation) is affected by R-1;
  - A-09, A-11 and L-02 navigation rows are affected by R-3 after 16 navigations.
- **Smells (judgment only):** the one-line formatting in `review_regressions.rs:313-317` and the `search()` body; the triplicated pin-drop logic. They are not demanded for closure.

## 7. Tests actually run, and what I relied on

All runs used my own APFS clone of the prior review target, from the session scratchpad. No shared target was written; no lock, manifest or dependency was changed. Flags: `--offline --locked`, `+1.98.1`, `-j2`, `CARGO_INCREMENTAL=0`, `TEZBAR_NATIVE_SOURCE_SHA=35dca9b…`.

1. **Pristine archive:** `cargo test -p tezbar-native --features runtime-shaders --lib surfaces::command_bar -- --test-threads=2`.
   - Result: **64 passed, 0 failed, 0 ignored.**
   - Log: `pristine-commandbar.log`, SHA-256 `1850d2bb…22ae`.
2. **Pristine archive:** `cargo test -p tezbar-ui-model --features launcher-models --lib launcher -- --test-threads=2`.
   - Result: **91 passed**, including `handled_local_activation_preserves_live_query_and_rejects_stale_ticket`.
   - Log: `pristine-model-launcher.log`, SHA-256 `de8d1e54…838d`.
   - An earlier invocation without the feature matched 0 tests. An invocation before that failed with a shell `env`-quoting error and ran nothing. Neither is counted.
3. **Probe copy** (pristine + `opus_probes.rs` appended to `review_regressions.rs`): `cargo test … --lib opus_probe -- --test-threads=1 --nocapture`.
   - Result: 5 print-only probes ran.
   - Log: `probe-run.log`, SHA-256 `ff8b903f…0cc2`.
   - Probe source: `opus_probes.rs` `298f52d2…8df2`; patched file `d27317f9…cc8a`.
   - Two incidental lines in the probe log are not used as evidence: the deep-route line "after invalidate pending=true", and the quit-race "pending=true". Both reflect the 8 ms coalescing clock not being advanced.

- **Evidence location:** everything is copied to `/tmp/tezbar-gpui-m5-opus-35dca9b-evidence/`.
- **Processes:** none of my cargo or rustc processes remain.
- **Relied upon, not rerun:**
  - core `platform_tests::host_quit_occurs_only_after_accepted_confirmation`;
  - the parent's 370 combined model count and its 1.85 run;
  - the historical v3 gates.

## 8. Limits

- No GUI, live dialog, Quick Look, hotkey, clipboard, provider, network, updater or installed-app effect was exercised. All adapters were synthetic.
- The quit dialog's real modal and blur behaviour, OS IME, VoiceOver and Narrator, the Windows build and GUI, GPU textures and RAM remain open manual or platform gates.
- `7e21f21` and any M6, M7 or M11 consumers are outside this audit.
- **Closure statement:** F-01 to F-08 are closed against the pinned source. M5 should not be accepted until R-1, R-2 and R-3 are repaired with permanent regressions and briefly re-checked.
