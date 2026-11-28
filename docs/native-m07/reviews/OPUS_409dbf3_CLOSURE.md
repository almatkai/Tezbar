# Tezbar GPUI M7 repair closure: independent Opus review at 409dbf3

- **Reviewer:** Claude Opus 5.5, acting as the review sub-agent. No subagents, no delegation.
- **Pin:** `409dbf3c5950b0065aff900b2906b548660bc264`, tree `6de9344d5e8bf7083d57c47be465e1999790e61d`, branch `feat/native-gpui-app`, worktree `Tezbar-native-rust-agent`.
- **Primary diff:** `9d0c910..409dbf3` (18 files). Regressions were checked against the original review pin `290e414`.
- **Scope:** closure of the M7 repairs for Opus `290e414` (N-1–N-10, D-1–D-6, the Extensions render-mutation standards item and the M6 S-2 historical label), plus the changed shared callers (`WriteQueue`, `SettingsStore`, `controls::pin`, utility pin row, app boot/layout).
  - Standalone M8 and core asset seams in intervening commits are **not** accepted here.
  - **This is not acceptance of the full migration.**
- **Original report** `docs/native-m07/reviews/OPUS_290e414.md`: SHA-256 `ca8ea258…5e03`, verified. It was not modified.

## Verdict

**M7 closure: not yet acceptable.** Most IDs are closed at their real callers. But two actionable functional defects remain, and a probe reproduces each of them at this pin:

1. **R-1 (Medium, regression versus 290e414):** an alias save that lands in the pump's exit window is **silently lost on close or quit**. The close barrier returns `Ok`, and an unrelated owner's drain stays stuck. This is a residual of N-4, made worse by the new `drain()`.
2. **R-2 (Medium):** a shortcut rejection that settles while a save from the same Settings owner is still running is shown as **"Saved"**. The stale rejection then cancels the user's next unrelated follow-up, such as Copilot sign-in. This is a residual of N-1 and N-3 at the Settings caller.

Four P3 defects also remain:
- **R-3 (traced):** the queued layout refresh undoes a stepper click, so a rapid third click is lost.
- **R-4 (probe):** the confirmation sheet does not block the footer or error controls painted after it.
- **R-5 (probe):** a validation error shows the durable **Retry saved changes** line, and Retry replaces the field-specific message.
- **R-6 (traced, pre-existing):** a Settings intent made during a failed start is dropped.

Nonfunctional copy, AX and design notes are listed separately.

## 1. Method and integrity

- **Snapshot.** `git archive 409dbf3 | tar -x` into my own scratch folder `…/c6e2c188-…/scratchpad/pin`.
  - **1950/1950** blobs match `git ls-tree -r 409dbf3` (`git hash-object --no-filters`), with 0 mismatches.
  - I re-verified after all runs: still 0 mismatches.
- **Probe copy.** All probes ran in an APFS clone, `scratchpad/probe`. The only change there is code appended to `settings/tests.rs` (`probe-settings-tests.diff`).
- **Comparison copy.** A separate archive of `290e414` (`scratchpad/old`) got only the alias probe (`probe-old290-tests.diff`).
- **Live worktree.** It was read only, through `git diff/show/log/ls-tree/rev-parse/archive`. Its working files (concurrent M8/M10 WIP) were never used as evidence. Nothing was edited, staged, committed, switched, pushed, installed or launched.
- **Parent evidence.**
  - All **281/281** `review-repair-receipt.json` source hashes match the pin.
  - The preserved binary `/tmp/tezbar-native-m7-review-repairs/tezbar-normal-preview` has SHA `36523e8c…bb64`, which matches the receipt. It is a `9d0c910` dirty=true preview, used only for hashing. It is **not** a clean 409dbf3 binary.
  - The recorded final logs show native 232, bench 233, UI 51, models 379 (1.98.1) and models 379 (1.85).
  - `review-tests-3.log` is the preserved hung run. I did not count it.
- **XERJ was not used.** Exact `git` and `grep` were authoritative, with no reindex.
- **Build settings:**
  - Target: my own APFS clone of an older scratch target, at `scratchpad/target`. The shared or parent target was not touched.
  - Toolchain and flags: `+1.98.1`, `--locked --offline`, `-j2`, `CARGO_INCREMENTAL=0`, `--features runtime-shaders`, `TEZBAR_NATIVE_SOURCE_SHA=409dbf3…`.
- **No external access:** no network, providers, profiles, clipboard, credentials or GUI. All transports are the synthetic `Fixture`, plus a one-type `Opus2Conflict` wrapper.

### Execution summary

| Run | Result | Log |
| --- | --- | --- |
| Native lib at the pin (probe copy, my 4 probes skipped) | **232 passed, 0 failed**, 4 filtered | `native-baseline.log` |
| `tezbar-ui` + `tezbar-ui-model` libs | **51 + 175 passed** (lib targets only; the parent's 379 includes integration tests that I did not rerun) | `ui-models.log` |
| Opus probes, run 1 (4 probes) | **3 failed** (R-2, R-4, R-5). The alias probe **hung**: the untimed peer wait was stuck. The stack sample shows it parked. Killed. | `probe1.log`, `probe1-killed-hung-alias.log`, `hang-sample.txt` |
| Alias probe, timed, 3,000 cycles | 0/3000 | `probe2-alias.log` |
| Alias probe, timed, 200,000 cycles ×2 | **11/200k and 9/200k** strands. **Every** strand: close `Ok`, alias not saved | `probe3-alias-run{1,2}.log` |
| Same alias probe at **290e414** | Strands about once per 200 cycles (old N-4). Close **did** save the alias every time. Stopped after 831 cycles. | `old290-alias.log` |

All evidence was copied to `/tmp/tezbar-gpui-m7-opus-409dbf3-evidence/`.

## 2. Per-ID disposition

| ID | Disposition | Evidence at real callers |
| --- | --- | --- |
| **N-1** rejected shortcut traps onboarding | **PARTIAL** | **Onboarding: closed.** `WriteQueue::reject` settles `ok:false` without a `saved` stamp. It records `rejected[key]` so older failures are not replayed (model regression). The store reports the rejection only to `(owner, owner_revision)`. Finish/Skip use `drain_owner` captured before spawn, and `completed` makes `Finished` emit once. The real-Onboarding probe and the concurrent rejection/Finish regression pass. Close/quit do not retain the rejection. The core returns early only for the hotkey (`ai/mod.rs` `llm-config-set`), and every hotkey intent is a lone edit, so `reject` cannot drop sibling keys. **Settings caller: open, see R-2.** |
| **N-2** failed operations block close/admission | **CLOSED** | `failed` is removed. An operation's error goes only to its ticket (`operation_finished` → `results`). It never enters `failures()` or Retry, and admission counts only queued, active and in-transit work. The 15-failure-then-success and no-replay regressions pass. **The old assertion update is legitimate:** `failed_setter_retry…` still asserts `operation_result(id).is_err()`. It now asserts close `Ok`, plus one new no-replay count after Retry. Its newer-value and no-auth-replay assertions are unchanged. It does not hide a bug. |
| **N-3** foreign failures cancel own follow-ups | **PARTIAL** | Peer **config** failures, peer operation failures and unrelated held downloads no longer cancel `flush` follow-ups. The `drain_owner` result is decided by `owner_failure_count`, owner validation and the owner's alias read. `owner_pending` gates after-save actions. Close/quit keep `drain_with`. **Residual R-2:** a same-owner stale rejection still clears `after_save`. |
| **N-4** exiting-pump race | **PARTIAL** | `running`/`pump_generation` live in `State`, and the exit decision is made under the same lock. A `PumpLease` reports panic or drop, and no `try_lock`/`is_finished` check is left. The original 20,000-cycle stress passes inside the 232. **Residual R-1:** the `aliases_pending` exit window, a new close-path data loss. |
| **N-5** bare F | **CLOSED** | `accelerator` takes exact `f1`–`f24` (`f0`, `f25`, `f01`, `foo` are rejected). Modifier combinations keep the old path. Onboarding reuses the same function. Regressions pass. |
| **N-6** disappearing durable error | **CLOSED** | `durable_error()` derives from `store.failures()`, falling back to `parent_error`. Action errors use a separate line that filters out `CLOSE_ERROR`, so tab switches and transient errors no longer hide it. See R-5 for the validation interaction. |
| **N-7** missing selected controls | **CLOSED (AX unproven)** | `choice()` reads hydrated Theme, `update_channel`, Knowledge depth/speed and per-root depth, and sets `RadioButton` + `aria_toggled` + the `pill_strong` fill. A regression exercises `choice_selected`. Standards note: this is a Settings-local helper, not the shared `controls::segment`, and there is no `RadioGroup` container. Spec §5.6 asks for `RadioGroup`. The ledger's word "shared" is inaccurate. |
| **N-8** hidden 4 Hz timer | **CLOSED** | `needs_timer` = activated, visible, and Knowledge or device Waiting/Polling or AI pending account. It is re-evaluated after every `action`, request completion, `set_visible` and activation, and the loop self-cancels. Onboarding hides content outside AI, Knowledge and Permissions. Behaviour change (limit): Copilot device polling now pauses while the surface is hidden and resumes when shown. |
| **N-9** dropped rapid layout | **PARTIAL** | The parent coalesces queued effects (`schedule_layout`/`merge_layout`), supersedes the old queued ticket, and runs the queued effect after the pending write. A newer success clears earlier barrier failures. **Residual R-3 (traced):** the content refresh between writes undoes the stepper value. |
| **N-10** failed core Retry resets launcher | **CLOSED** | The failure branch calls `fail_open()` (`decided=true`). A later `m7_ready` only loads the channel and shows, with no hide and no gate re-run. A fixture covers the state; traced through `start_core` → `m7_ready`. Adjacent pre-existing gap: R-6. |
| **D-1** implementation copy | **PARTIAL (copy nits)** | Folder, channel, depth, speed and extension labels use product names. Voice uses `download_progress` with an unknown total, and details are labelled lines (env/header keys literal). **Nit:** `display_name` splits acronyms. `baseURL` becomes "Base U R L" and the `url` field becomes "Save Url for …". Keep known acronyms (URL, ID, API, MCP, JSON). |
| **D-2** zoom grid | **CLOSED** | A bounded stepper with the current value. Nit: Decrease/Increase are not disabled at 75%/200%, so the click is a no-op write. |
| **D-3** confirmation | **PARTIAL** | Copy names the target, the accept button uses the Danger tone, Cancel gets default focus, Tab is trapped, and Esc cancels. **R-4:** the overlay does not block controls painted after it. |
| **D-4** Retry on every error | **PARTIAL** | Transient read and operation errors lose the Retry button. **R-5:** validation errors still show the durable Retry line. |
| **D-5** content width | **CLOSED (no visual)** | `max_w(610 / 850 for AI)` + `mx_auto`. |
| **D-6** pin glyph | **CLOSED** | `controls::pin` uses the exact `ClipboardView.tsx:599` path, at 10×zoom. It is labelled `Role::Image` "Pinned" (TS is `aria-hidden`; acceptable). Nit: TS tints the glyph `amber-300/80`, while native uses `text3`. Consider `t.warning`. |
| Extensions render mutation | **CLOSED** | Fields are created in `prepare_extension_fields` on Installed, Schema and ExtensionSettings completion. Removed commands drop their field and subscription. Render only reads `labeled`. The Headers JSON field is now masked and single-line. |
| M6 S-2 | **CLOSED** | `parent-composition.md` and `source-case-map.json` (still valid JSON) label the original hashes and counts as historical, with values intact. The cited current `utilities_integration_tests.rs` SHA `f616ecad…` matches the pin and has 5 tests. |

## 3. Actionable defects

### R-1 — MEDIUM (regression) — An alias in the pump exit window is lost on close; an unrelated drain strands

**Mechanism**
- `store.rs` `pump()` checks `aliases_pending` under one lock at the top of the loop. It reads `(next_effect, revision, config_revision)` under a second lock.
- An `alias()` that runs between the two:
  - increments `revision` and `config_revision`;
  - sets `aliases_pending`.
- The pump then:
  - sees no effect;
  - skips `config_settled` because `aliases_pending` is set;
  - exits, because `s.revision == revision` (the revision it captured already includes the alias). It sets `settled=revision` and `running=false`.
- `drain_owner(a)` already called `start_worker` while `running` was true, and it never restarts the pump. Every `drain_owner` whose target is above the stale `config_settled` hangs, including **another owner's**.
- `drain()` (the close/quit path through `drain_with`) checks `!running && revision == settled` and returns **`Ok`**. `pending()` is false, so the window closes without writing the alias.
- At 290e414, the old `drain()` started the pump unconditionally and saved the alias. The data loss is new.

**Reproduction:** `opus2_probe_alias_admitted_in_pump_exit_window_is_not_lost`. Production `SettingsStore`, multi-thread runtime, 200k cycles of a peer enqueue + `drain_owner` racing `alias` + `drain_owner`:

```
OPUS2 strand at 7961: peer_owner_stranded=true alias_owner_stranded=true alias_saved_before_close=false close=Ok(Ok(())) alias_saved_after_close=false pending=false
OPUS2 alias exit window: n=200000 peer_owner_stranded=11 alias_owner_stranded=11 close_ok_with_alias_unsaved=11   (run 2: 9/9/9)
```

**Effect**
- A command alias saved in Settings can be silently dropped when Settings is closed or the app quits.
- The Settings save spinner, or the launcher picker's `drain_owner`, can hang until some other action starts a pump.
- Rare, but silent data loss.

**Fix**
- In the exit critical section, use `if s.revision != revision || s.aliases_pending { continue; }`.
- Also treat `aliases_pending` as unsettled in `drain()` and `pending()`.
- Optionally, `drain_owner` should call `start_worker()` inside its loop when `!running` and it is not settled.
- Make this probe a permanent regression, with timeouts on both waits.

### R-2 — MEDIUM — A Settings shortcut rejection that settles under an older flush is reported as Saved, then cancels the next follow-up

**Mechanism**
- `flush()` captures `drain_owner(owner)` only when no flush is running. If a save is in flight, the hotkey `queue(..)` (the key handler) only enqueues.
- The running drain's `owner_target` is the earlier revision. `rejected[owner]` (the hotkey revision) does not match, so the result is `Ok`, and `message="Saved"`. `owner_pending` is false by then, so there is no re-flush to report the rejection.
- The entry stays. The next `flush` that enqueues nothing (Sign-in, Permission request, Open Permissions, Quit) captures `owner_target == rejected` and gets `Err("Shortcut unavailable…")`. The `Err` arm runs `after_save.clear()`, so the follow-up is dropped.

**Reproduction:** `opus2_probe_settings_rejection_during_inflight_save_is_reported_and_does_not_cancel_next_action`. The toggle save is held, a rejected `raymesHotkey` is queued, the save is released, then Copilot Sign-in:

```
OPUS2 rejection-in-flight: hotkey_saves=1 after_save error=None message=Some("Saved") | signin starts=0 signin_error=Some("Shortcut unavailable. Your previous shortcut is unchanged.") failures=0
```

**Effect**
- The user is told the shortcut saved when the OS refused it.
- The next unrelated click is silently dropped, under a misleading error.

**Fix**
- Keep the rejected revisions per owner, for example `BTreeMap<OwnerId, BTreeSet<Revision>>`.
- Let a `drain_owner` report and consume every rejection at or below its captured `owner_target`, and re-flush when a rejection is outstanding.
- A rejection is definitive and non-retryable. `flush` should show its message but **not** clear `after_save`; the user's later actions do not depend on the refused shortcut.
- Keep the probe as a regression.

### R-3 — LOW (P3, traced) — A queued layout refresh undoes the stepper value

**Trace (`app/m7.rs`)**
1. The first write completes.
2. `m7_finish_effect` runs.
3. `m7_refresh_content` runs. `m7_layout_snapshot` reads the launcher's **current** `view.zoom`, which is still the first value, and `set_layout` overwrites `content.layout` unconditionally (`settings/mod.rs:502`).
4. Only after that does `m7_layout_ticket` apply the queued effect.

The content therefore shows, for example, 105% while 110% is being applied. A click in that window computes from 1.05 and re-requests 1.10. Three rapid clicks give 110%, not 115%.

The regression `review_scale_stepper…` exercises only `SettingsContent`, without the parent refresh, so it cannot see this.

**Fix**
- Apply the queued ticket before `m7_refresh_content`, or refresh after the synchronous apply in `m7_layout_ticket`.
- Or have `set_layout` keep the optimistic zoom while `queued_layout` is set.
- Add an Owner-level fixture.

### R-4 — LOW (P3) — The confirmation sheet does not block controls painted after it

The overlay is added to `root` **before** the durable-error line (with **Retry saved changes**), the action-error line and the footer (Save/Close). Later siblings paint above the `.occlude()` overlay, so they remain visible and hit-testable.

**Reproduction:** `opus2_probe_confirmation_sheet_occludes_footer_controls`. With "Uninstall…?" open, a real `simulate_click` on `settings-Close`:

```
OPUS2 modal: footer_close=Some(Bounds{…}) close_events=1 confirmation_still_open=true
```

**Fix:** add the overlay as the last child, after the footer and error lines, or render it with `deferred(anchored(..))` plus `occlude`. Add the probe.

### R-5 — LOW (P3) — A validation error looks like a failed save, and Retry erases the field-specific message

`store.validation(..)` entries count in `failures()`, so `durable_error()` returns `CLOSE_ERROR` and the line with **Retry saved changes** renders for a local validation error. That is the case D-4 cited.

Retry then:
- clears `self.error`;
- `retry()` cannot fix validation;
- `drain_owner` returns `CLOSE_ERROR`;
- the field-specific text is replaced by the generic save error.

**Reproduction:** `opus2_probe_validation_error_is_not_a_retryable_durable_save`, on `memoryMaxItems="abc"`:

```
local=Some("Enter a non-negative number.") durable=Some("Couldn't save Settings. …") | after Retry local=Some("Couldn't save Settings. …")
```

The same probe saw no Retry button in the last frame even though `durable_error()` was `Some`. `field_changed`'s validation early-return paths skip `cx.notify()`, so the new error line may not paint until some later notify. That part is indirect evidence.

**Fix**
- Exclude validation from the durable line. Keep it blocking close, but render its own message ("Correct the invalid … before closing.") with no Retry.
- Have Retry keep validation text.
- Call `cx.notify()` on the validation returns.

### R-6 — LOW (P3, traced, pre-existing in the touched N-10 path) — A Settings intent made during a failed start is dropped

1. `settings_intent` during `Boot::Waiting` stores `pending_settings`.
2. `start_core` fails, and the fail-open branch calls `show()`.
3. Later, `m7_ready`'s decided branch also only calls `show()`.

Only the gate and `m7_finished` deliver `pending_settings`, so the tray or menu request opens the launcher instead.

**Fix:** in both fail-open and decided paths, deliver `pending_settings.take()` through `settings_intent`, as `m7_finished` does.

## 4. Changed-file coverage and standards

| File | Read | Notes |
| --- | --- | --- |
| `ui-model/settings.rs` | full queue + diff | `reject`, `latest`, `rejected`, `owner_pending`. Correct. `rejected` grows by key only, so it is bounded. |
| `settings/store.rs` | full | R-1. The pump lease and generation are sound. The `results` cap of 64 can evict an unread result, but that is pre-existing and not reopened. |
| `settings/mod.rs` | diff + callers (flush, action, field_changed, set_layout, timer, confirm copy) | R-2, R-5. |
| `settings/view.rs` | diff + render/key | R-4, R-5. D-1 acronym nit. Choice IDs are unique (target-scoped `b()` IDs). |
| `onboarding.rs` | full | Closed. |
| `app/m7.rs`, `app.rs` | diff + boot/layout paths | R-3, R-6. |
| `tezbar-ui/controls.rs`, `utilities/view.rs` | diff | D-6 closed. The `"pin-marker"` ID repeats, but it is scoped under `pinned-{id}`. |
| tests (`settings/tests.rs`, `onboarding.rs`, `m7_tests.rs`, model) | diff | No test was weakened, skipped or ignored. The one changed assertion is justified (N-2). |
| M6 docs | diff | S-2 closed. |

**Spec checks**
- Clean in changed code: no gradients, glass, purple-blue defaults, side stripes, badges or ALL-CAPS.
- Tokens come from `tezbar_ui::tokens`.
- §5.6 `RadioGroup` is missing (N-7 note).
- No new `unsafe`, processes, WebView, unbounded channels or render-time I/O.

**Coding standards**
- `confirmation_copy` and the `b()` scope `match` are dense one-line formatting. `rustfmt` passed per the parent's record; I did not rerun fmt.

## 5. Limits (explicitly unproven)

- **Not run:** real macOS/Windows GUI, VoiceOver/Narrator, IME, DPI/multi-monitor, GPU, RAM, the idle budget, live providers, MCP, OAuth or credential stores, real hotkey registration, real disk-layout writes.
- **Traced only:** R-3 and R-6 (no Owner-level fixture).
- **Not rerun by me:** bench-control, check/build, fmt and the Rust 1.85 suites. I relied on them as recorded.
- **Full migration is not complete.** M8–M11 remain open.

## 6. Commands

```
git -C …/Tezbar-native-rust-agent archive 409dbf3c5950b0065aff900b2906b548660bc264 | tar -x -C …/scratchpad/pin
git ls-tree -r 409dbf3 | … git hash-object --no-filters --stdin-paths   # 1950/1950, re-run after: 0 mismatches
python3 -I receipt check: 281/281 source hashes match; shasum binary 36523e8c…bb64
cp -c -R …/17f6a676-…/scratchpad/target …/scratchpad/target ; cp -c -R pin probe
cd probe; export CARGO_TARGET_DIR=…/scratchpad/target CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2 TEZBAR_NATIVE_SOURCE_SHA=409dbf3…
nice -n 10 cargo +1.98.1 test -p tezbar-native --features runtime-shaders --locked --offline -j2 --lib opus2_probe -- --test-threads=2 --nocapture        # probe1.log (3 failed, alias hung → killed)
… --lib opus2_probe_alias -- --test-threads=1 --nocapture                                                                                               # probe2 (3k), probe3 run1/run2 (200k)
… --lib -- --test-threads=2 --skip opus2_probe                                                                                                          # 232 passed
nice -n 10 cargo +1.98.1 test -p tezbar-ui -p tezbar-ui-model --locked --offline -j2 --lib -- --test-threads=2                                          # 51 + 175
git archive 290e414 → scratchpad/old (+ alias probe); same command with TEZBAR_NATIVE_SOURCE_SHA=290e414…                                                # old290-alias.log
sample <pid> 1 (hung probe stack) ; kill / pkill of my own test binaries only
```

- **Processes:** none of mine remain (checked with `pgrep`). No GUI was launched.
- **Repo:** no edits, staging, commits, pushes, Actions or installs in the repo.
- **Evidence:** `/tmp/tezbar-gpui-m7-opus-409dbf3-evidence/`.
