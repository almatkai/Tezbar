# Tezbar GPUI M7 closure: independent Opus review at b626f28

- **Reviewer:** Claude Opus 5.5, acting as the review sub-agent. I did the work myself, with no subagents.
- **Pin:** `b626f28476d136a292ed2385e0e1bc2ddedd1489`, branch `feat/native-gpui-app`, worktree `Tezbar-native-rust-agent`.
- **Primary diff:** `dd7bc19..b626f28` (one commit). It touches:
  - `settings/{store,mod,view,tests}.rs`
  - `app/m7.rs`
  - the `app.rs` failed-start hunk
  - additive M7 tests in `app/m8_tests.rs`
  - the closure docs and logs
- **Scope:** closure of R-1–R-6 and the D-1 acronym copy from my 409dbf3 report, plus a recheck that every ID accepted at 409 still holds.
  - The M8 defects in `docs/native-m08/reviews/OPUS_dd7bc19.md` are assigned separately and are not judged here.
  - **This is not acceptance of the full migration, the full UI suite, Windows, or the GUI.**

## Verdict

**M7 closure: not yet acceptable, by one narrow actionable regression.** R-1, R-3, R-4, R-5, R-6 and D-1 are closed at their real callers. R-2 is closed for the Settings caller it targeted.

However, the new "a dropped waiter releases its rejection claim" rule introduced a regression in Onboarding:

- **N-B1 (Low–Medium, regression versus dd7bc19).** The user re-records an Onboarding shortcut while the earlier, OS-rejected shortcut save is still in flight.
  - The new shortcut **is** saved and registered.
  - Onboarding still shows the old shortcut, **"Shortcut unavailable. Your previous shortcut is unchanged."**, and leaves the step not done.
  - A probe reproduces this with the real Onboarding key handler. The same probe passes at dd7bc19.

No other actionable defect survived verification.

## 1. Method and integrity

- **Snapshot.** `git archive b626f28 | tar -x` into `/tmp/tezbar-gpui-m7-opus-b626f28-evidence/src`.
  - **2076/2076** blobs match `git ls-tree -r b626f28` (`git hash-object`), with 0 mismatches. See `archive-verify.txt`.
  - The live worktree was read only, through `git log/diff/ls-tree/rev-parse/archive`. Its working files hold uncommitted M8/M10 work and were never used as evidence.
  - I made no edits, staging, commits, checkouts, pushes or GUI launches in the repo.
- **Copies.** Each copy is an APFS clone in the evidence folder with its own private target. Shared or parent targets were never used.
  - `probe/`: pin plus appended probes only (`probes/settings_probe.rs`, `probes/onboarding_probe.rs`).
  - `mut/`: pin with the fixes reverted (`probes/mutation.diff`).
  - `old/`: a `git archive dd7bc19` of `apps crates Cargo.* rust-toolchain.toml docs/releases`, plus the same probes.
- **Build settings.** Rust `+1.98.1`, `--locked --offline -j2`, `CARGO_INCREMENTAL=0`, `--features runtime-shaders` (plus `bench-control` for the bench run), `TEZBAR_NATIVE_SOURCE_SHA=b626f28…`.
  - The mutation and old builds use placeholder 40-hex SHAs, because `build.rs` requires the format.
- **Parent artifacts.**
  - The six `owned_sources` SHA-256 values in `closure-repair-case-map.json` all match the pin.
  - The preserved binary `/tmp/tezbar-native-m7-closure-repairs/tezbar-normal-preview` has SHA `3f90290a…4904`, which matches the receipt. Its metadata says `source dd7bc19, dirty=true`. It is the combined M10 preview, used only for hash checking. **It is not a clean pin artifact.**
  - The receipt honestly records the combined working source (native 305, bench 306, full UI 56/1 failed, foundation 52 with 5 filtered). I did not expect those hashes or counts to match the pin, and I did not count them for the pin.
  - The SIGTERM logs (`targeted-1` exit 143, `bench-final` exit −15) and `native-1` (2 failed) are preserved. I did not count them as passes.
- **Original reports are unchanged.**
  - `OPUS_290e414.md`: SHA-256 `ca8ea258…5e03`.
  - `OPUS_409dbf3_CLOSURE.md` at the pin: byte-identical to my original `/tmp/tezbar-gpui-m7-opus-409dbf3-closure.md` (SHA-256 `a2b822ca…a6e9`).
- **XERJ:** not used. Exact `git`/`grep` were authoritative, with no reindex.

### Execution summary (pin unless stated)

| Run | Result | Log |
| --- | --- | --- |
| Native lib, pristine pin | **263 passed, 0 failed** (all 9 case-map tests listed as `ok`) | `native-pin.log` |
| Native lib + `bench-control`, pristine pin | **264 passed, 0 failed** | `bench-pin.log` |
| `tezbar-ui` lib, pristine pin | **52 passed, 0 failed, 0 filtered**. Matches the expected UI count without M10. | `ui-pin.log` |
| Opus probes at pin (4) | **2 failed**: both N-B1, at store level and in real Onboarding. **2 passed**: reverse-order barriers, and alias close-only (20k cycles, 0 lost, 0 stranded). | `probe-pin.log` |
| Same probes at dd7bc19 | **Both N-B1 probes pass** (old behaviour correct). Alias close-only: **4/20k stranded** (old R-1). Reverse order passes. | `probe-dd7.log` |
| Mutation copy (fixes reverted), `review_` filter | **7 of the 9 new tests fail.** 57 others pass. | `mutation.log` |

Every run had a per-test deadline (`tokio::time::timeout`). No probe was unbounded, and no 200k loop was run.

## 2. Per-ID verdicts

| ID | Verdict | Evidence |
| --- | --- | --- |
| **R-1** alias in the pump exit window | **CLOSED** | See R-1 below. |
| **R-2** rejection during another save; follow-ups | **CLOSED for Settings, with an Onboarding regression (N-B1)** | See R-2 below. |
| **R-3** queued layout before content refresh | **CLOSED (composition only)** | See R-3 below. |
| **R-4** modal over footer and errors | **CLOSED** | See R-4 below. |
| **R-5** validation versus durable Retry | **CLOSED** | See R-5 below. |
| **R-6** pending Settings after a failed start | **CLOSED (helper-level)** | See R-6 below. |
| **D-1** acronym copy | **CLOSED** | See D-1 below. |

**R-1 — alias in the pump exit window: CLOSED**
- **Pump exit.** The exit decision is now `revision != captured || aliases_pending` → `continue`, taken under the same lock that clears `running` (`store.rs:497-504`). `alias()` sets `aliases_pending` under that lock, so either the pump sees it or `running` is already false.
- **Restart.** `drain_owner` calls `start_worker()` on every unsettled loop iteration (`store.rs:286`). A waiter woken by the exit notify therefore restarts an idle pump. `start_worker` is a no-op while the pump is running or the executor has failed, so there is no busy loop. The executor-failed check returns first.
- **Close path.** `drain()` treats `aliases_pending` as unsettled (`:325`), and so does `pending()` (`:181`). The close/quit path (`drain_with`) cannot return `Ok` with an alias unwritten.
- **Probes.**
  - The parent's 50k two-owner race passes.
  - My close-only probe has **no alias-owner waiter**: an alias races a peer pump exit, then only `drain()` runs. Result: 20k/20k saved, 0 stranded.
  - The same probe strands 4/20k at dd7bc19.
  - The mutation that reverts the three lines fails the parent test (`owner stranded at iteration 6248`).

**R-2 — rejection during another save; follow-ups**
- **Store side.**
  - `rejected` is now `BTreeMap<OwnerId, BTreeSet<rev>>`.
  - `waiters` is registered at **capture**, not first poll, so the order of simultaneous barriers is fixed when they are created (`:218-229`).
  - A waiter claims a rejection `r ≤ own target` unless a live same-owner waiter has a target in `[r, own)`.
  - `owner_pending` stays true while any rejection is unclaimed.
- **Settings side.**
  - `Ok` with `pending` → no "Saved", re-flush. The next waiter claims the rejection → `Err(REJECTED)`.
  - `after_save` is kept, the actions run when not pending, and the rejection message is restored after them.
- **Reverse completion order.** Finish awaited to completion first, while the shortcut waiter is registered but unpolled: `finish=Ok`, `shortcut=Err(REJECTED)`, close `Ok`, `owner_pending=false`.
- **No poisoning of Skip/Finish.** A later Finish never claims an earlier shortcut's rejection while that shortcut waiter is alive. Onboarding's `finish()` is unchanged. The 409 Finish/Skip regressions pass.
- **Old behaviour is caught.** Under mutation (exact-match claim, `after_save` cleared), the in-flight, queued-sign-in and dropped-barrier tests all fail.
- **Residual: the dropped-waiter rule breaks Onboarding re-recording.** See N-B1.

**R-3 — queued layout before content refresh: CLOSED (composition only)**
- `m7_continue_layout` applies the queued ticket first. `m7_layout_ticket` sets `view.zoom` synchronously (`m7.rs:556`) before placement or the write. `m7_refresh_content` runs after it, so the snapshot reads the queued zoom.
- **Rapid third click.** Content shows 1.10, so the third click computes 1.15. If it lands during the new write, `schedule_layout`/`merge_layout` coalesce it.
- The write-completion closure calls the helper (`:637`).
- Under mutation (refresh first), the test fails with `zoom - 1.10`.
- **Limit:** TestPlatform rejects placement, so the test exercises the error branch of `m7_layout_ticket`. Physical geometry and the durable write are unproven. The test's second assertion (`zoom + 0.05 == 1.15`) is arithmetic only.

**R-4 — modal over footer and errors: CLOSED**
- **Paint order.** The overlay is now the last child of the root, after the durable error, the action error and the footer (`view.rs:1575-1660`). It uses `absolute().inset_0().occlude()`.
- **Action guard.** `action()` returns early for anything except `Confirm(_)` while a confirmation is open (`mod.rs:1532`). Cancel is `Confirm(false)` and Esc maps to `Confirm(false)` (`view.rs:1655, 1680`), so neither is blocked by the guard. The guard also covers keyboard Enter/Esc mappings.
- **Test note:** the test cannot tell whether occlusion or the guard stopped the click, because either one blocks it. I did not mutate it; the 409 probe was its failing baseline. Occlusion of text inputs, which bypass `action()`, is source-traced only.

**R-5 — validation versus durable Retry: CLOSED**
- **Durable line.** `durable_error()` uses `durable_failures()`, which counts queue failures, alias reads and the executor, but not validation. Validation is shown through the action-error line (`.or(validation_error)`).
- **Retry.** With only a validation error, Retry is a no-op plus `notify`. `action()` keeps `error` while validation is present.
- **Precedence.** `drain_owner` returns the specific validation text ahead of `CLOSE_ERROR`. `flush` keeps the local text.
- **Immediate paint.** All four early returns in `field_changed` now call `cx.notify()`.
- **Close still blocked.** Validation still blocks close, because `failures()` includes it and `drain()` uses it.
- Under mutation (`failures()`), the test fails.

**R-6 — pending Settings after a failed start: CLOSED (helper-level)**
- Both the `app.rs` failed-start branch (`fail_open(); m7_present_ready`) and the `m7_ready` decided branch call `m7_present_ready`. That delivers `pending_settings.take()` through `settings_intent` and clears `show_on_ready`.
- The later `m7_ready` reuses the same window, so delivery happens once.
- Under mutation, the test fails (`pending_settings.is_none()`).
- **Limit:** the helper is driven directly. No real failed core start or OS window was exercised.

**D-1 — acronym copy: CLOSED**
- The new `display_name` produces `baseURL`→"Base URL", `url`→"URL", `apiKey`→"API Key" and `clientId`→"Client ID". Traced: `HTTPHeaders`→"HTTP Headers".
- **Nit (non-blocking):** digits do not split words (`gpt4Model`→"Gpt4Model").

### Prior IDs accepted at 409

These still hold at the pin. The diff does not touch their mechanisms in a way that reopens them:
- **N-2:** operation errors still go only to their ticket.
- **N-5:** the F-key handling is unchanged.
- **N-6:** the durable line now excludes validation, which narrows it as intended.
- **N-7 / N-8:** unchanged.
- **N-10:** `fail_open` still runs before present.
- **D-2, D-3 (Esc and Cancel focus unchanged), D-5, D-6:** still hold.
- **Extensions render mutation:** still holds.
- **M6 S-2:** still holds.
- **N-1 (onboarding):** Finish/Skip still hold. The other partials (N-3, N-4, N-9) are now resolved through R-1–R-3.
- The pristine 263-test run includes every earlier regression.

### Test-map judgment

All 9 case-map names exist and passed at the pin. 7 fail under the reverting mutation:
- `alias_racing`
- `inflight_save`
- `already_queued_signin`
- `dropped_barrier`
- `queued_zoom`
- `validation`
- `failed_core_start`

`confirmation_sheet` and `acronyms` were not mutated. The 409 probe was their failing baseline, and `display_name` was trivially different before.

However, `review_closure_dropped_barrier_releases_its_shortcut_rejection_claim` **encodes the behaviour that causes N-B1.** It asserts that a newer waiter receives an abandoned waiter's rejection. This needs revisiting together with the fix.

## 3. New actionable defect

### N-B1 — LOW–MEDIUM (regression versus dd7bc19) — a re-recorded Onboarding shortcut inherits the earlier rejection

**Trigger.** On the Onboarding Shortcut step:
1. The user records shortcut A, which the OS refuses. The core returns `{"ok":false}`.
2. While A's `llm-config-set` is still in flight, the user clicks the recorder again and presses B, which is accepted.

A is "in flight" whenever the core round trip is slow, or another owner's save is ahead of it in the shared queue. The recorder button has no guard against this (`onboarding.rs:556-560`).

**Caller chain.**
1. `onboarding.rs:475-476`: `self.shortcut_save = Some(cx.spawn(..))` drops A's GPUI task.
2. `gpui_tokio::Tokio::spawn` aborts the Tokio task when the GPUI task drops (`gpui_tokio.rs:54-66`).
3. A's `OwnerWaiter` drops, which releases its claim (`store.rs:584-596`).
4. When B's waiter settles, the filter at `store.rs:245-259` finds `r_A ≤ owner_target_B` with no live earlier waiter, so B claims A's rejection and returns `Err(REJECTED_CHANGE)`.
5. Onboarding's B completion takes the error branch.

**Consequence.**
- B is durably saved and registered.
- Onboarding shows the **old** hotkey, the error "Shortcut unavailable. Your previous shortcut is unchanged.", and `done` lacks `shortcut`. The step summary also shows the wrong shortcut.
- The user is told the opposite of what happened. There is no data loss, and re-recording B again fixes it.
- At dd7bc19, the exact-revision match made B `Ok`.

**Evidence** (`probe-pin.log` versus `probe-dd7.log`).

At the pin:
```
B6 onboarding rerecord: first_in_flight=true durable="Control+J" shown_hotkey="Alt+Space" error=Some("Shortcut unavailable. Your previous shortcut is unchanged.") shortcut_done=false
B6 dropped-waiter: second_result=Err("Shortcut unavailable. …") durable_hotkey="Control+J" owner_pending=false
```

At dd7bc19:
```
B6 onboarding rerecord: first_in_flight=true durable="Control+J" shown_hotkey="Control+J" error=None shortcut_done=true
B6 dropped-waiter: second_result=Ok(()) durable_hotkey="Control+J"
```

**Fix (recommended).** Supersede the rejection by key: a rejection of `(owner, key, r)` is discarded once a later same-owner intent for the same key is admitted or saved.
- `WriteQueue::reject` already records `rejected[key]`. Keep the key next to the revision in `State.rejected`.
- Then a waiter only reports a rejection whose key has no newer same-owner revision at or below its target.
- **Alternative:** have an abandoned `OwnerWaiter` consume, rather than release, the claims it would have owned. This only makes sense if no Settings caller depends on release; no current Settings caller drops a live flush job while its view lives.
- **Either way:** add the real-Onboarding probe as a regression, and rework `review_closure_dropped_barrier_releases_its_shortcut_rejection_claim`. Its second edit is a different key, so it can stay valid under the key-superseded rule.

## 4. Non-blocking observations

These are not defects:
- **Rejection versus own durable failure.** When a claim and an outstanding own durable failure coincide, `drain_owner` returns `REJECTED` first. Settings then runs `after_save` despite the failure, while the durable line with Retry stays visible.
  - The only follow-ups are Permission, Sign-in, OpenPermissions and Quit.
  - Quit still goes through `drain_with`, which refuses (`m7.rs:309-325`), so I found no harmful consequence.
  - If you want strict N-3 semantics, check `owner_failure_count` before returning `REJECTED`.
- **Rejection then "Saved".** A rejection reported while a newer edit is pending is replaced by "Saved" when that edit settles. This is cosmetic, and the hydrated field shows the canonical shortcut.
- **Carried over from earlier reviews** (not reopened, and not claimed fixed by the parent):
  - the Settings-local RadioButton group versus spec §5.6/§774 `RadioGroup`;
  - the pin tint;
  - physical AX;
  - Copilot polling paused while hidden.

## 5. Standards and design

- **Changed view code.**
  - No gradients, glass, default purple-blue accents, side stripes, badges or decorative additions.
  - Tokens come from `t.*`. The danger alert line keeps `Role::Alert`, and the dialog keeps `Role::AlertDialog`.
  - The layout follows spec §5.1/§4.2 (inline alert plus **Retry saved changes**).
- **Changed Rust code.**
  - No new `unsafe`, processes, unbounded channels or render-time I/O.
  - The comments explain why (capture-time registration, claim ownership).
  - `waiters` is bounded by live futures. `rejected` is bounded by the hotkey revisions an owner has not yet claimed.
- **Compiler warnings** in my logs are pre-existing, in `tezbar-core`, and not in M7 files.
- **Not run:** `rustfmt` and clippy. I relied on the parent's record.

## 6. Limits (explicitly unproven)

- **Not exercised:** real macOS/Windows GUI, VoiceOver/Narrator, IME, DPI/multi-monitor, real hotkey registration, real disk layout writes or placement, real failed core startup, live providers, profiles, clipboard, credentials, network.
- **Fixture-level only:** R-3 and R-6 are exercised at helper or composition level. R-4 occlusion of non-`action()` controls is source-traced.
- **Not run by me:** the full combined UI and full-migration suites, the MSRV 1.85 suites, check/build of the normal binary.
- **Full migration is not complete.** M8–M11 remain open, and M10 is unaccepted.

## 7. Commands

```
git -C …/Tezbar-native-rust-agent archive b626f28 | tar -x -C $E/src                     # $E=/tmp/tezbar-gpui-m7-opus-b626f28-evidence
git ls-tree -r b626f28 | git hash-object per blob                                        # 2076/2076, 0 mismatches
cp -c -R <prior 409 review scratch target> $E/target ; cp -c -R $E/target $E/target-mut ; cp -c -R $E/target-mut $E/target-old
cd $E/src   && cargo +1.98.1 test -p tezbar-native --features runtime-shaders --locked --offline -j2 --lib -- --test-threads=2               # 263
cd $E/src   && cargo +1.98.1 test -p tezbar-native --features runtime-shaders,bench-control --locked --offline -j2 --lib -- --test-threads=2 # 264
cd $E/src   && cargo +1.98.1 test -p tezbar-ui --locked --offline -j2 --lib -- --test-threads=2
cd $E/probe && cargo +1.98.1 test -p tezbar-native --features runtime-shaders --locked --offline -j2 --lib b6_probe -- --test-threads=1 --nocapture
cd $E/old   && (same b6_probe command, CARGO_TARGET_DIR=$E/target-old)
cd $E/mut   && cargo +1.98.1 test … --lib review_ -- --test-threads=2   (CARGO_TARGET_DIR=$E/target-mut)
```

- All runs used `nice -n 10`, `CARGO_INCREMENTAL=0` and `CARGO_BUILD_JOBS=2`.
- **Processes:** none of mine remain (checked with `pgrep`). The only `tezbar_native` rustc seen belongs to the live worktree's own `target/`; it is the parent's build, not mine, and I left it alone. No GUI was launched.
- **Re-verification after all runs:** 2076/2076 blobs still match, with 0 mismatches. Live `HEAD` is still `b626f28`.
- **Evidence:** `/tmp/tezbar-gpui-m7-opus-b626f28-evidence/`, which holds the logs, `probes/` (probe sources, `mutation.diff`) and `archive-verify.txt`.
