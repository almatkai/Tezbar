# Tezbar GPUI M9 review: root repairs D1–D15 at fec3257

Reviewer: Opus 5.5, acting as the review sub-agent. I did all the work myself, with no subagents, delegation or repair implementation.
Pin: `fec32571c4046f83823e1ca0fcee9548a85e65f2` (root M810 implementation `37164d5` plus docs-only `fec3257`). Worktree `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-gpui-app`. Review date: 2026-11-07.
Scope: the root fixes for D1–D15 from `docs/native-m09/reviews/tezbar-gpui-m9-opus-0513321.md` (SHA256 `f7206c9e…04296f4`, verified). Root implementation `2baa290` + `c01fa31`, root test-boundary correction `151df16`. The M810 SVG/terminal internals and the M11 updater/migration are other reviewers' scopes. I only checked where they meet D-scope code.

Evidence labels used below:
* **[S]** source trace in the pinned archive.
* **[P]** a probe I wrote and ran.
* **[R]** a root test I re-ran in my own build.
* **[E]** recorded root evidence that I hash-verified but did not re-run.

## 0. Verdict

**No High defect remains. M9 is still not accepted, and no E row changes status.**

* **Repaired, with executed headless evidence:** D1 (the High regression), D3, D4, D5, D6 (production behaviour), D8, D9, D11 and D14 (core repair).
* **Repaired by source only:** D2. There was no Tauri GUI run.
* **Partially repaired, with actionable functional residuals:**
  * **D13 (Low-Medium).** The launcher's hide path still orphans live extension sessions. I reproduced this with the production CoreClient and the real Bun worker.
  * **D12 (Low-Medium).** An uncontrolled `List.Dropdown` shows the wrong current value after the user picks one.
* **Low residuals:**
  * D10: the gear is drawn twice (bitmap plus "⚙" title).
  * D7/D15: success notices are drawn as errors.
  * D14: the quit-refusal copy still carries the misleading prefix and a Rust `Err("…")` wrapper.
  * D1: a no-view `openExtensionPreferences()` is admitted by the core and then dropped by the native UI.
  * D15: a failed Store run drops the worker's message.
* **One test-hermeticity defect:** the registered `sdk_form` real-SDK gate **fails in every timezone west of UTC** after the D6 fix. It passes only because the root pins `TZ=Asia/Almaty`.
* **Platform gaps (OPEN):** Windows scoped-read/OneDrive/ConPTY/NSIS (BLOCKED by remote provider OAuth/quota; no fallback was attempted), physical GUI/IME/AX/GPU/RAM, signing, real profile discovery and updater rehearsal. Tauri remains the default product, and this review cannot authorize cutover.

**The live worktree moved during this review (not by me).** I started at HEAD = pin with a clean tree. At the end, HEAD was `8fc0f67`: root commits `9832852` (11:08 +05) and `8fc0f67` (13:57 +05), plus 12 uncommitted entries (10 modified, 2 untracked). These include D-scope files (`extension-runner.ts`, `extension_worker.rs`, `extension_runtime/mod.rs`, `command_bar/mod.rs`).
* All my evidence comes from the immutable archive. It verified 2921/0/0 before and after.
* My verdicts apply to `fec3257` only.
* `8fc0f67` changes D13/E-35 semantics: background MenuBarExtra sessions are now exempt from worker pruning (limit 64), and status signatures are retained. My prune statements are therefore pin-only.
* A grep of the committed `8fc0f67` objects shows that the code patterns behind F1–F5 are still present. I did not inspect the uncommitted edits. Re-run my probes at the new head before relying on any closure.

## 1. Method, isolation and integrity

* **Archive.** `git archive fec3257` was extracted to `/tmp/tezbar-m9d-opus-fec3257-N4q6PP/src`. The ls-tree digest is `3726ce92…bd4bf4bd`. Blob identity (sha1 and mode) was checked before and after every probe: `checked=2921 bad=0 extra=0` both times (`logs/blob-verify-{before,after}.txt`). No live file was used as pin evidence.
* **Owned-file truth.**
  * All 39 files in `root-repairs/owned-files.json` hash-equal `c01fa31` and are unchanged through `fec3257`. In other words, they are c01 snapshots, and they are still current for these files.
  * The only later D-scope source delta is the test correction in `151df16`. Its recorded before/after hashes (`b83019e9…` / `3904b952…`) equal `c01fa31` and `fec3257`.
  * All 10 M810 owned-source hashes match `37164d5` and `fec3257`.
* **Compiled-source truth.**
  * The root's recorded build inventory for the preserved M810 debug binary (`native-build-sources.json`, 2877 files) equals `fec3257` on every non-docs file: 2876 identical; the only difference is `docs/NATIVE_GPUI_VERIFICATION.md`, and 44 later docs evidence files are absent (`logs/m810-build-sources-vs-pin.json`).
  * The preserved binary `5ec21f89…` is unchanged. I did not execute it. It is not a clean-pin or RAM claim.
* **Probe copy.** `src-probe` is an APFS clone of the archive. It adds only 3 test-only files and 3 `#[cfg(test)] mod` lines, with no production change (`logs/probe-copy-production-diff.txt`).
* **Build.**
  * Rust 1.98.1 (`48a229cea`), `--locked --offline -j2`, `CARGO_INCREMENTAL=0`, `TEZBAR_NATIVE_SOURCE_SHA=<pin>`, features `runtime-shaders` and separately `runtime-shaders,bench-control`.
  * The private target was an APFS clone of `deps/build/.fingerprint` from the read-only handed-back `/tmp/tezbar-m9-c01-validation-Rffw07/target`. There was no incremental directory and no symlinks, and I removed 96 914 workspace outputs before the first Cargo run. All six workspace crates recompiled.
  * Dep-info: 310 first-party inputs = 304 byte-identical to the pin + 3 probe files + 3 module-line files, 0 unexpected (`logs/compiled-firstparty-inputs.json`).
  * I deleted my target afterwards and kept the probe binaries: normal `f4caea85…`, bench `27976a8c…` (`probes/bin/`).
* **Isolation.**
  * Tests ran with `env -i`, a private `HOME`/`TMPDIR`, an explicit `TZ` and 2 test threads.
  * Real-SDK work used only `/var/folders/…/tezbar-m09-fixtures-d06CfD/prepared.json`, `/opt/homebrew/bin/bun` (1.4.0) and the fake TestHost. All 40 resource hashes and all 22 first-party worker-graph inputs match the root's `worker-bundle.json`, before and after (`logs/bundle-verify-{before,after}.json`).
  * There were no unfiltered core tests, no PTY, no TS/vitest runs (vitest writes caches into the live `node_modules`) and no dist-backend edits.
  * There was no GUI/OS/clipboard/Keychain/credential/profile/provider/remote/push/install/signing activity.
* **Preserved originals.**
  * Original M9 report `f7206c9e…` (repo and `/tmp`) and M810 original `a44f9595…`: unchanged.
  * All 463 pinned files under `docs/native-m09` and `docs/native-m10/root-m810-repairs` are byte-identical in the live tree.
  * Root-validation `evidence-sha256.json`: 187/187. M810 `evidence-sha256.json`: 43/43. Root-repairs focused logs: 43/43.
  * The c01 validation binary `5e57d2c5…` is unchanged.
* **Processes.** None of mine remain: no cargo, rustc, test binary or Bun worker.
* **XERJ.** Not used (known stale). Exact git objects and `rg`/`grep` reads are authoritative.

## 2. Recorded results vs my reruns

| Gate | Root record | My run at the pin (own build) |
| --- | --- | --- |
| 15 root D regressions + 9 real-SDK + admission + retained-store test | 420/422 full suites, Asia/Almaty [E] | **26/26 pass** [R] (`logs/run-final-almaty.log`) |
| Quit/admission under `bench-control` | bench 422 [E] | **6/6 pass**, including my D4/D14 probes [R][P] (`logs/run-bench-d4-d14.log`) |
| D6 date probe across timezones | Almaty, LA [E] | **7/7 pass**: Almaty, LA, Kiritimati +14, Pago Pago −11, St John's −3:30, Kathmandu +5:45, UTC [R] |
| `sdk_form` across timezones | Almaty only [E] | **FAILS** in LA, Pago Pago and St John's (`"2026-11-06"` ≠ `"2026-11-07"`); passes in UTC, Almaty, Kathmandu and Kiritimati [R] (`logs/final-date-*.log`) |
| My 16 probes | — | **10 GREEN, 6 RED.** The 6 RED are exactly the 6 failures in the consolidated run (36 pass / 6 fail) |
| TS rejecting-host test, typecheck, UI 57, models 381 (1.98.1/1.85), Host 45, Tauri compatibility | pass [E] | not re-run: unchanged components, so the root's reuse statement holds |

## 3. Findings (actionable, most severe first)

### F1 — D13 is only partly repaired: hiding the launcher orphans a live extension session — **Low-Medium, functional (claimed fixed)**
* **Trigger.** The user runs an extension view command and dismisses the launcher (Esc, hotkey toggle or blur-hide) before the reply arrives. A hotkey run hidden early behaves the same way.
* **Cause.**
  * The activation runs as `self.tasks.push(cx.spawn(…))` (`command_bar/mod.rs:1399`; hotkey path `:1811`).
  * `Launcher::hide` calls `self.tasks.clear()` (`:733`). Dropping a GPUI task aborts its gpui_tokio waiter (`gpui_tokio.rs`: "the Tokio task will be cancelled if the GPUI task is dropped").
  * So the root's discard branch `a.generation != … || !this.visible → dispose_dropped_extension` (`:1404-1406`) is **never reached after a real hide**.
  * In production, `CoreClient` runs the request as a detached `handle.spawn` (`core_client.rs:576`). The core therefore completes the run and registers the session (`extension_worker.rs:228-254`), and nobody receives the reply.
* **Rejected views (same family).** The model can refuse a view: duplicate form field ids (a common SDK author error), more than 20 000 nodes, more than 2 MiB of props, or an unknown node type. In that case `receive_navigation` (`mod.rs:1586-1600`), `route_extension` Err (`extension_integration.rs:223`) and the Store's invalid-view branch (`extensions_store/mod.rs:494`) all skip Dispose.
* **Probes.**
  * `opus_d13_hidden_launcher_disposes_late_view_reply` → RED (`disposed=[]`).
  * `opus_d13_rejected_view_reply_is_disposed` → RED.
  * Controls: `opus_d13_new_query_disposes_late_view_reply` → GREEN (`["orphan"]`).
  * **Production path:** `opus_d13_real_worker_hidden_launcher_orphans_live_session` (real CoreClient + Bun; only reply *delivery* gated) → **RED**. The `invalidation` session fired its 100 ms effect timer *after* hide, no `extension:session-disposed` was emitted, and an explicit core dispose returned `true` (the core still owned it). The control `opus_d13_real_worker_new_query_control_disposes` → GREEN (core dispose `false`).
  * Logs: `logs/run-launcher-d13*.log`.
* **Consequence.**
  * A zombie worker session keeps executing extension effects (timers, polling, network) after the user dismissed the command.
  * At the pin, its core session bookkeeping lasts until the same command is re-run, the worker prunes (30), the extension is disabled or uninstalled, or the worker stops (EOF).
  * Hypothesis, not executed with GpuiHost: an effect that writes the clipboard would really run.
  * The root disposition "Stale/hidden foreground Run replies are disposed by Launcher" is false for the launcher's hide path. The root test calls the helper through a test seam (`review_m9_discard`), so it cannot detect this.
* **Fix.**
  1. Keep extension-run activations owned independently of `hide()`, for example in a task set that `hide` does not clear, so the `!visible` branch disposes. Better still, make the transport own orphan disposal: when the waiter is gone, the spawned run task disposes any non-MenuBarExtra view session it created.
  2. Dispose rejected or invalid view replies in `receive_navigation`, `route_extension` and the Store.
  3. Replace the seam test with a real-path test such as `opus_d13_real_worker_…`.

### F2 — D12 is only partly repaired: an uncontrolled `List.Dropdown` reverts to its default after a pick — **Low-Medium, functional (display and AX)**
* **Cause.**
  * The native selection is derived only from props: `value → defaultValue → first item` (`extension_runtime/view.rs:1360`).
  * The worker re-serializes the same uncontrolled props after `onChange` and does not inject the value (`extension-runner.ts:3514-3527`).
  * The TS renderer (the parity reference) keeps a local `accessoryValue` (`list-runtime.tsx:541-567`, `grid-runtime.tsx:152`).
* **Probes.**
  * `opus_d12_uncontrolled_dropdown_shows_users_choice`: with `defaultValue: "jp"`, picking Kazakhstan invokes the real callback with `kz` and the re-render is applied, but the trigger stays `jp` → RED.
  * The variant without a default falls back to the first item after the user picks Japan → RED.
  * The controlled control is GREEN; the root's test covers only a controlled `value`.
* **Consequence.** For the common uncontrolled pattern, the visible current filter and `aria-selected` are wrong after every change. This is the original D12 consequence ("current filter invisible"). Filtering itself works.
* **Fix.** Keep a per-session chosen value, set in `choose_dropdown`. Use the precedence `props.value` (controlled) > local choice (if still offered) > `defaultValue` > first, and reset it on session change. Add uncontrolled tests.

### F3 — The `sdk_form` real-SDK gate is not hermetic across timezones — **Low, test-only (gating for any machine west of UTC)**
* **Cause.** `tests/fixtures/extensions/src/form.tsx:14` uses `new Date('2026-11-07T00:00:00Z')`, and `real_sdk.rs:246` expects `"2026-11-07"`. The D6 fix correctly uses the local calendar date. The root gates set `TZ=Asia/Almaty`.
* **Executed.** Fails in LA, Pago Pago and St John's; passes in UTC and east of it.
* **Consequence.** Production behaviour is correct, but a US/Americas developer or a UTC−x CI host gets a red registered gate. The fixture also no longer guards against D6-style regressions in the other direction.
* **Fix.** Use a local-midnight literal `new Date(2026, 10, 7)`, or derive the expected value from the local calendar. Run the SDK gates in at least one west-of-UTC timezone.

### F4 — D10 residual: the gear is drawn twice, and every SDK icon is replaced by a generic gear — **Low, cosmetic (OS painting unverified)**
* **Probe.** `opus_d10_status_title_and_icon_actual_values` → RED.
  * An icon-only `Icon.Gear` root produces status title `"⚙"` *and* the built-in 16×16 gear bitmap (`extension_worker.rs:808`, `process_host.rs:990`).
  * A titled root (`Fixture 0`) also gets the gear bitmap.
  * A display-only row is a correct disabled label.
* **Consequence (hypothesis).** The host sets both the title and the icon (`tezbar-host/src/desktop.rs:233-239`), so macOS should show both gears. A non-gear icon-only root shows the command name as menu-bar text next to a gear. Declared SDK icons are never shown.
* **Fix.** When the built-in bitmap is attached, do not synthesize "⚙". Keep the command title in the tooltip, not the menu-bar text. Resolve real icons through the canonical asset scope later.

### F5 — D7/D15 residual: success notices are rendered as errors — **Low, design (source only)**
* **Cause.** `ExtensionEffect::Notice` is used by runtime toasts and by D7's new post-setup feedback. It is mapped to `launcher.error` (`app/m9.rs:39-43`), and the footer draws `error` in `t.danger` (`command_bar/view.rs:1679`). Store notices use the neutral `notice` slot (`extension_integration.rs:127`).
* **Fix.** Route these to `notice`, or carry the toast style (success/failure/animated).

### F6 — D14 residual: the quit-refusal copy — **Low, copy (executed)**
* **What the user sees.** `Could not save changes; quit cancelled: Err("Extension install in progress. Wait for it to finish, then quit.")` (`app.rs:589-591`, `{prepared:?}`). The root test only checks `contains(…)`.
* **Related [S].** Any other in-flight extension request (run or invoke, 120 s core timeout) makes the drain wait until the 8 s budget, and then reports "exceeded eight seconds" under the same "Could not save changes" prefix.
* **Fix.** Use a typed refusal reason with plain copy, and say "Extension command still running" for non-install work.

### F7 — D1 residual: a no-view `openExtensionPreferences()` is admitted but then a silent no-op in native — **Low, parity decision and doc precision**
* **Probes.**
  * `opus_d1_noview_controls_feedback_and_round_trips` (real worker) → GREEN: every no-view command returns `ok:true` and keeps its HUD. Close/Clear/HUD make **zero** host round-trips. Preferences is the only admitted control: one `extension:ui` event with `source:0` and the run's own session id.
  * `opus_d1_native_consumer_drops_admitted_noview_preferences` → GREEN: the only native consumer, `RuntimeView` (`extension_runtime/mod.rs:365`), drops that payload. Its own source-stamped payload opens Preferences.
* **Consequence.** Native behaves like the legacy Tauri no-op, not like Raycast. The in-flight grant has no user-visible effect in native today, and the "positive Preferences event" proves TestHost emission only.
* **Decision needed.** Either route admitted run-scoped Preferences to the run's extension settings (fenced by request and launcher visibility), or document it as an intentional no-op and narrow the grant.

### F8 — D15 residual: a failed Store run drops the worker's message — **Low, copy**
`extensions_store/mod.rs:504` shows a fixed "Command failed. Try again." (`error: Option<&'static str>`). The hotkey path was fixed and the Store path was not.

### Docs and source-map — **docs-only**
* In `root-validation/…/api-source-map.json`, the query "Worker grants / retired ownership" uses `RequestOwner|request_owners|provisional|session-retired`. None of these match code, so the actual grant (`in_flight_runs`, `runRequest`, `PendingRequest::drop`, `extension_worker.rs:52-66,176-188,431`) is not located, and `session-retired` is not an event (the event is `extension:session-disposed`).
* `root-repairs/dispositions.md`: the D13 claim is wrong for the hide path (F1), and the D1 wording should say that native drops the admitted no-view Preferences (F7).

**Non-gating notes.**
* Every grid row reserves an 18 px heading band so that `uniform_list` keeps uniform heights (`166 = 148 + 18`, consistent with the lease math). This is a density cost; visual review is pending.
* A non-integer `columns` value defaults to 4 in native, while TS uses `Math.round`.
* There is a narrow TOCTOU between `send()`'s `closing` check and the install-counter increment (`core_client.rs:529/563`).
* The pre-existing legacy `TauriHost` tray leak for non-SDK items is unchanged and not claimed (owner: legacy Tauri).
* `grid_ranges()` and `dropdown_choices()` are recomputed per render. This is acceptable at the current bounds.

## 4. Per-ID verdicts (D1–D15)

| ID | Root repair (verified) | Evidence | Verdict |
| --- | --- | --- | --- |
| D1 | Best-effort `sessionControl`; pre-view Close/Clear skipped (`extension-runner.ts:60-68,2130-2132`); `withExtensionHostRequest(frame.id)` wraps every invoke (`main.ts:131`); RAII run grant keyed by request id + extension (`extension_worker.rs:52-66,176-188,421-441`) | [R] `review_m9_noview_sdk_controls`; [P] two D1 probes GREEN; [E] TS rejecting-host test | **Repaired** (High regression closed). Low residual F7 |
| D2 | `Host::sdk_status_items` defaults to false; `TauriHost` does not override it (`tauri_host.rs:20`); native and TestHost opt in; `sync_sdk_status` returns early (`extension_worker.rs:714`). The new `extension:session-disposed` event has no legacy consumer | [S]; [R] `sdk_menubar`; [E] Tauri compile | **Repaired by source** (no Tauri GUI) |
| D3 | Default `extension_control` is a no-op (`host.rs:141-150`); the GPUI fence is kept; the feedback fixture asserts the source-stamped event | [R] `sdk_feedback`; [P] D1 consumer probe | **Repaired** |
| D4 | Non-destructive extension barrier before chat/terminal cancellation (`app.rs:550-554`); final dispose only after the recheck (`:628`); cancel path resumes (`:585`) | [R] same entity/session/form, `disposes=[]`; [P] `opus_d4…` GREEN in normal and bench builds: real CoreClient admission reopens, directly and via the production launcher adapter (the root test uses an adapter that bypasses admission) | **Repaired.** The c01 RED test (left 2 / right 3 at `:204`) was obsolete; the correction asserts the non-destructive drain and then keeps the original 3-disposal assertion. **Legitimate.** Informational: a cancelled final quit still cancels the live chat run (`agent_cancel=1`), which is M8 chat-owned |
| D5 | Client filtering no longer bumps the sequence (`runtime.rs:627-640`); worker push/pop/`__nav_pop__` mark the session dirty; a superseded Invoke's "Working" is cleared by the newer accepted reply | [R] both push cases (`Detail`, notice `None`); [E] models | **Repaired.** The rewritten old model test (actual server search plus a separate client-filter control) is legitimate |
| D6 | Local-calendar components across VM realms, DateTime `HH:MM`, symmetric parse (`extension-runner.ts:3529-3540`) | [R] 7 timezones GREEN | **Repaired in production.** Test defect F3 |
| D7 | No-view reply after setup → fresh notice + one Back (`preferences.rs:201-221`) | [R] `copies=1`, `Back=1` | **Repaired.** Low residual F5 |
| D8 | One `grid_columns` policy, default 4, clamp 2..8 (`view.rs:1741`); shared `grid_ranges` (`:1497`) for render, keys and leases | [R] default/sections; [P] clamp probe GREEN (1→2, 20→8, 3→3) | **Repaired** |
| D9 | Left/Right left to the List search and the focused Grid search; native buttons keep the paired Enter/Space | [R] caret ×2, `sdk_grid` with grid focus, native Enter | **Repaired.** The grid-focus change to the old scenario is legitimate |
| D10 | Disabled info rows; built-in 16×16 gear; no SDK path is decoded by the tray | [R] info rows; [P] RED | **Partially repaired.** Low F4 |
| D11 | Scroll handle, section-aware uniform rows, reset on reopen/action change/search | [R] real viewport offset and row geometry | **Repaired** (no pixels or AX) |
| D12 | Grouped virtualized popup, keyboard support, `aria-expanded`, callback admitted (`runtime.rs:429-443`) | [R] controlled case; [P] uncontrolled RED ×2 | **Partially repaired.** F2 |
| D13 | Launcher/Store dispose discarded replies; worker retirement notifies the core (`extension_worker.rs:498-511`); MenuBarExtra intentionally kept by a background owner | [R] seam test and Store test; [P] new-query GREEN, hide RED (synthetic and real worker), rejected view RED | **Partially repaired.** F1. Store path OK; worker prune → core cleanup source-only at the pin (changed post-pin by `8fc0f67`) |
| D14 | Dedicated 16 extension permits + 16 reserved disposal permits; install lease inside admitted tasks; `closing` flag; release-time drains removed | [R] admission and busy-install tests; [P] detector control, refused-quit reopening (normal and bench) | **Repaired.** Low copy F6 |
| D15 | Human labels, one README, install/remove copy with reason, worker HUD for Store run, hotkey message, localized dates, titled and selected preference choices, one heading per row | [S]; [R] related tests | **Mostly repaired.** Low F5, F8. Visual review pending |

## 5. Cross-scope (owners named)
* **M810 (other reviewer).** Extension images go through `CoreClient::load_asset` → `spawn_blocking` + deadline → `decode_preview` → the bounded `decode_svg` (`asset_transport.rs:58-120,280-345`). The PNG fixture decodes in `sdk_grid` and `sdk_detail` pass. No extension SVG assets are exercised. The SVG bounds are not reviewed here.
* **M11.** `drain_extensions` is called only from the App final-quit preparation. The D repairs touch `app.rs` only in the quit block and in test module lines; the updater quiescence/resume and the 8 s budget are unchanged at the pin.
* **M8.** A cancelled final quit still cancels the live agent run (chat-owned; informational).

## 6. Standards, design and testing judgement
* **Design.** The repairs are coherent: RAII grants, a default-false capability, non-destructive barriers and one shared column policy. The D13 failure is an ownership split: extension-run ownership is tied to UI task lifetimes that `hide()` cancels, while the core work is detached. The transport should own the outcome.
* **Code standards.** `Option<&'static str>` for the Store error blocks dynamic messages (F8). The dense one-line style of the quit block predates these repairs. The test-only accessors (`review_extension_access.rs`) are `cfg(test)`, which is acceptable.
* **Tests.** Most root regressions run real paths: the real worker for D1/D5/D6/D7/D10 and the real App for D4/D14. Gaps:
  * D13's launcher test is a seam.
  * D12 covers only controlled dropdowns.
  * The D4 App test bypasses CoreClient admission.
  * `sdk_form` depends on the timezone.
  * The no-view test asserts only `ok` plus the presence of the event.
  My probes fill these gaps and should be promoted.
* **AI-pattern design check.** In the changed extension surfaces I found no gradients, glass, side stripes or decorative badges. The real design defects are F4 (two gears) and F5 (danger-coloured success).

## 7. E rows
No row is accepted. Affected rows:
* E-16 (F2).
* E-06, E-07 and E-41 (F1; prune semantics changed post-pin).
* E-35 and E-42 (F4, F7, F1). The MenuBarExtra background lifetime decision is still manual: the root keeps the core status owner, and source-0 controls cannot pass the native fence (D1 consumer probe).
* E-05/E-34 (F5), E-28 (F3), E-10 (F7), E-38 (F8).
Visual, physical and Windows gaps keep every row open.

## 8. Paths
* Work dir: `/tmp/tezbar-m9d-opus-fec3257-N4q6PP`.
* Probe sources: `probes/src/opus_d_probe.rs`, `opus_d_quit_probe.rs`, `opus_d_launcher_probe.rs`. Runner: `probes/run.sh` (`run-bin.sh` takes `BIN=`).
* Verifiers: `probes/verify_blobs.py`, `verify_bundle.py`, `depinfo_verify.py`, `clone_cache.py`.
* Logs: `logs/run-final-almaty.log`, `run-almaty-1.log`, `run-launcher-d13*.log`, `run-bench-d4-d14.log`, `final-date-*.log`, `run-date-*.log`, `run-sdkform-*.log`, `build-probe-{1,2,3,bench}.log`, `cache-clone.{json,log}`, `compiled-firstparty-inputs.json`, `m810-build-sources-vs-pin.json`, `bundle-verify-{before,after}.json`, `blob-verify-{before,after}.txt`, `worktree-after.txt`, `original-reports-after.txt`, `preserved-binaries-after.txt`, `environment.txt`.
* Hashes: `logs/SHA256SUMS.txt`.
* Environment: rustc 1.98.1 (48a229cea), Bun 1.4.0, macOS 26.3 (25D2125).
