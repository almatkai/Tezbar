# Tezbar GPUI M9 review: Store, runtime, worker/Host boundary, M3 extension controller

Reviewer: Opus 5.5, acting as the review sub-agent. All work was done in this agent. No subagents, delegation or polling of other tasks.
Pin: `05133210b78f125b83ffdf2482e09e72533ff219`. Worktree `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-gpui-app`. HEAD equalled the pin before and after the review. Review date: 2026-11-07.

## 0. Verdict

**M9 is not accepted, and no E row changes status.** All 45 E rows stay unaccepted. The nine registered real-SDK scenarios pass again in my independent rerun. However, nine probes I wrote against the same real Bun → NativeCore/TestHost → RuntimeView path, or against the real App, found **15 new actionable defects**. Two are High:

* **D1 (High, regression on both hosts).** No-view commands that call `closeMainWindow()`, `openExtensionPreferences()` or `clearSearchBar()` now fail with `ok:false "Unknown SDK source"`. This is the canonical Raycast no-view pattern, and it worked before M9 because these calls were no-ops. The worker and core are shared, so the shipping Tauri app is affected too.
* **D2 (High, regression in the shipping Tauri app).** The shared core now turns every SDK `MenuBarExtra` reply into a real Tauri tray icon. `TauriHost::remove` never removes the icon, and every menu update builds another one. Submenu children are dropped.

The parent's concern about `drain_extensions` is **confirmed on the real App**: a cancelled final quit disposes the active extension session, and reopening shows a stale replacement view without the typed form state (D4).

Platform-blocking gaps remain open: Windows/ConPTY/assets (both remote attempts failed before tests), physical IME/AX/GPU/RAM, screenshots and manual matrices.

## 1. Method, isolation and integrity

* **Archive.** I extracted the pin with `git archive` to `/tmp/tezbar-m9-opus-0513321/src`. I checked all 2415 blobs (sha1 and mode) against `git ls-tree -r` before and after every probe: `checked=2415 bad=0 extra=0` both times (`logs/blob-verify-before.txt`, `logs/blob-verify-after.txt`). The ls-tree digest is `e2f3f7ea355c8b4c910c89f4641edda053b022890152fd28050eb51564ad1b8d`. No live source file was read as pin evidence; diffs against db4d0df came from git objects only.
* **Probe copy.** `src-probe` is an APFS clone of the archive. It adds only 3 test-only files and 3 `#[cfg(test)]` module lines (`logs/probe-copy-extra-files.txt`, `logs/probe-copy-production-diff.txt`). No production code was changed. Its `node_modules` is a symlink to the live dependency tree (read-only use; see the disclosure below).
* **Build.** Rust 1.98.1, `--locked --offline -j2`, `--features runtime-shaders`, `TEZBAR_NATIVE_SOURCE_SHA=<pin>`, `CARGO_INCREMENTAL=0`. The target dir was private: an APFS clone of the live `deps/build/.fingerprint` cache (no `incremental`). Only the workspace crates recompiled. The private target was deleted afterwards; only the 233 MB probe test binary is kept.
* **Isolation.** Tests ran with `HOME` and `TMPDIR` inside the work dir. Fixture profiles were tempdirs and the TestHost had no OS fall-through. No signing, installs, live store, providers, network, clipboard, Keychain, GUI launch or remote actions. No Git mutation.
* **Retained worker bundle.** All 33 resource SHA-256 values match `final-worker-bundle.json`, and all 22 first-party worker input sizes match the pin (`logs/retained-bundle-verify.txt`). I also rebuilt the bundle from the pin with the actual `prepare.mjs` into an owned dir. It differs from the retained bundle only in the fuse.js module label, because the symlinked `node_modules` resolves to a realpath (`logs/bundle-rebuild-vs-retained.diff`, 8 diff lines).
* **Disclosure.** My focused vitest run was rooted in `src-probe`, so it may have updated vitest's gitignored results cache, `node_modules/.vite/vitest/da39…/results.json`, in the **live** tree through the symlink. That file was rewritten again at 08:21:18 by a full-suite run that was not mine; a `pnpm` write to `.pnpm-workspace-state-v1.json` at 08:20:59 shows another actor working in the live tree. No live source, build output or Git state was touched.
* **Processes.** No owned processes remain. The unrelated `tezbar-supervisor-fixture` processes from `/private/tmp/tezbar-owner-r4.*` belong to another task and were not touched.
* **XERJ.** Not used. The brief said it is known to be stale or unavailable, so I used exact `rg`/git/Read throughout.

## 2. Recorded results vs my reruns

| Gate | Recorded (parent/M9) | My independent rerun at the pin (probe copy, same sources) |
| --- | --- | --- |
| 9 registered `sdk_*` real scenarios | 9 pass (freeze 06) | **9/9 pass**, 6.8 s, retained hash-verified bundle (`logs/rerun-real-sdk-9.log`) |
| Focused existing native extension tests (21) plus 2 existing App quit tests | in the 360-test suite | **23/23 pass** (`logs/rerun-focused-native.log`) |
| TS worker hooks (effects, contracts, extensionWorker/*) | in TS 745 / 7 skipped | **22/22 pass**, 5 files (`logs/rerun-ts-worker-focused.log`) |
| Actual worker smoke (`scripts/test-extension-worker.ts`) | pass | **pass** with my rebuilt bundle (`logs/rerun-worker-smoke.log`) |
| Full native 360 / bench 362 / UI 57 / model 380 / MSRV / Windows | recorded | **not rerun** (unchanged suites; the PTY-touching suites were deliberately skipped). Windows stays failed before tests; I make no source-only inference of a pass. |

The new probes go RED, and each control goes GREEN (section 3, `logs/probe-*.log`).

## 3. New actionable defects

Severity: High = shipping or user-visible breakage of a common path; Medium = clear functional defect or data loss on a narrower path; Low = copy or efficiency.

### D1 — SDK UI controls reject in no-view and before view registration — **High, regression (Tauri and native)**
* **Trigger.** A no-view command runs `await closeMainWindow()` (or `openExtensionPreferences()`, `openCommandPreferences()`, `clearSearchBar()`).
* **Cause.** `extension-runner.ts:2110-2122` now awaits `sessionControl` → `core:extension-ui` (`extensionWorker/desktop.ts:124`). The core accepts it only for sessions in `sessions` (`extension_worker.rs:405-418`). Those are registered only after a `mode:"view"` run reply (`:213-238`), so no-view sessions are never registered. The rejection propagates through `executePass` into `{ok:false,"Unknown SDK source"}`. At db4d0df these functions were `async () => {}`.
* **Probe.** `opus_probe_noview_sdk_controls`: plain HUD gives `ok:true`; close, prefs and clear each give `{"ok":false,"message":"Unknown SDK source"}`, and TestHost records no calls (`logs/probe-runtime.log`).
* **Consequence.** A very common Raycast pattern (`closeMainWindow(); open(...)`) fails before doing its work. This hits the beta users run today: `scripts/build-backend.ts:78` aliases the same worker host. A view command's async HUD or close fired right after its first render can race the registration done in `request()`.
* **Fix.**
  1. In the worker, make the controls best-effort: `.catch(() => {})`. Skip the round-trip when `!session.viewDelivered` for close and clear-search, matching the old no-op.
  2. In the core, accept `close` for an in-flight run, keyed by request id, or register a provisional identity before dispatch.
  3. Add no-view `closeMainWindow` + `showHUD` and `openExtensionPreferences` fixtures to the real-SDK gate, plus a rejecting `extensionControl` mock in `extension-runner-effects.test.ts`. Today's mock always resolves, which hides the defect.

### D2 — SDK MenuBarExtra now drives Tauri tray icons that leak and duplicate — **High, regression (shipping Tauri)**
* **Trigger.** In the Tauri app, run any SDK menu-bar command, update its state, then leave or re-run it.
* **Cause.** `ExtensionWorker::request` → `sync_sdk_status` (`extension_worker.rs:670-781`) runs for every host. `TauriHost::set` (`tauri_host.rs:211-258`) builds a new `TrayIconBuilder::with_id(scoped)` icon on every signature change. `TauriHost::remove` (`:261`) only drops the map handle. Tauri 2.11.2 registers each icon in `resources_table` (`tray/mod.rs:351-382`), and only `remove_tray_by_id` closes it (`manager/tray.rs:79-89`). The flat Tauri builder also turns `kind:"submenu"/"section"/"separator"` items into plain clickable entries, so submenu children are lost.
* **Consequence.** Duplicate and unremovable menu-bar icons accumulate in the shipping app. Clicks on retired icons are silently ignored (the session check is in the callback). Icon-only roots produce blank icons (see D10).
* **Evidence.** Source only: there is no Tauri GUI probe. The TestHost fixture (130 reruns stay at 1 item) proves only the well-behaved TestHost map.
* **Fix.** Gate SDK status sync behind a host capability, for example `fn sdk_status_items(&self) -> bool { false }`, overridden to true in GpuiHost and TestHost, so Tauri keeps its pre-M9 in-list-only behaviour. Separately, fix `TauriHost::remove` to call `app.remove_tray_by_id(scoped)` and update in place (`set_menu`/`set_title`).

### D3 — The default `Host::extension_control` changes Tauri behaviour and ignores source — **Medium, regression**
* **Cause.** `host.rs:136-160`: Close → `surface(CloseCurrent)`; Preferences → `app:open-surface` + `OpenSettings`; `source` is unused. TauriHost does not override it.
* **Trigger.** In Tauri, an action calls `closeMainWindow()` or `openExtensionPreferences()`, including a delayed call (`setTimeout`) after the user has moved on.
* **Consequence.** Before M9 these were no-ops in Tauri. Now they hide or close whatever Tauri surface is current and open Settings, with no session, visibility or source fence. That is the source-ownership bypass the brief prohibits. `clearSearchBar` resets the worker's `searchText`, but the Tauri renderer ignores `extension:ui`, so its input keeps the stale text.
* **Fix.** Make the default a no-op `Ok(())`, which preserves Tauri, and keep the fenced `extension:ui` path only in GpuiHost. The real-SDK `feedback` fixture currently asserts this *default* path (`calls` contains `CloseCurrent`), so it does not exercise the GPUI fence at all.

### D4 — A cancelled final quit disposes the active extension and reopens a stale view — **Medium, new in M9 (parent concern confirmed)**
* **Cause.** `app.rs:498-509` builds `(drain_chat, drain_terminal, drain_extensions)` in one tuple. `drain_extensions` (`extension_integration.rs:230-245`) synchronously disposes the runtime and Store, sends `Dispose`, clears subscriptions and sets the entities to `None`, all before the chat and terminal drains are awaited. When those drains fail, `resume_cancelled_quit` (`chat_integration.rs:328`) revives only chat and terminal. `restore()` → `show()` → `deliver_extension` then creates a **new** RuntimeView from the original navigation payload, which still carries the disposed session id.
* **Probe.** `opus_probe_cancelled_final_quit_restores_active_extension` uses the real Owner/Launcher with the chat `fail_append` final-drain failure (`logs/probe-runtime.log`):
  * `quitting=false`, save_error "Chat response could not be saved…";
  * `disposes=[{"sessionId":"probe-session"}]`, `after_cancel=(ExtensionRuntime, None)`;
  * reopen gives entity `50v1` instead of `12v3`, the same disposed `probe-session`, and form value `""` instead of `"typed before quit"`.

  The worker answers invokes on a disposed id with "Extension session not found." (`extension-runner.ts:4660-4663`), so the remounted view is a zombie whose actions fail. Its MenuBarExtra status item (if any) is also removed.
* **Secondary effect.** Dropping the entities triggers `on_release` → detached `transport.drain()` (`extension_runtime/mod.rs:156-159`, `extensions_store/mod.rs:101-104`). That acquires 16 permits on the **app-wide** admission semaphore (see D14), which can starve new requests after the cancel.
* **Fix.**
  1. Await the chat and terminal final drains first. Only then dispose and drain extensions.
  2. Keep the entities: mark them disposed instead of setting them to `None` and clearing subscriptions.
  3. On cancellation, revive extensions as chat and terminal are revived: re-attach events and `set_visible`.
  4. If disposal already happened, clear `navigation`/`delivered` and return home with an explicit notice. Never remount a payload whose session was disposed.

### D5 — Typing in a client-filtered search during an in-flight action desyncs the UI from the worker stack — **Medium, native controller defect (M3 model + M9 wiring)**
* **Cause.** `RuntimeSession::search` (`runtime.rs:612-613`) bumps `sequence` even when it issues no request. The in-flight Invoke token then fails `accepts()` and its reply is dropped (`complete()` stale path, `extension_runtime/mod.rs:660-670`). The worker's `push` and `pop` (`extension-runner.ts:1920-1926, 3327-3335, 4665-4669`) never call `markSessionDirty`, so no invalidation refresh repairs the drop. `refresh-session` then returns `unchanged` (`:3734`).
* **Probe.** `opus_probe_search_during_slow_push`: a List with a 600 ms async `push`, plus one typed character, leaves the observed state as List with `notice=Working`, while the worker's top view is the pushed Detail. The next Enter fails with "Extension request failed. Try again." The control without typing shows the Detail (GREEN).
* **Consequence.** The visible screen no longer matches the worker. "Working…" sticks, and every action on the screen the user sees fails until they leave.
* **Fix.** Bump `sequence` only for server search, or keep per-request-class sequences. Call `markSessionDirty` (or bump `revision`) on push, pop and `__nav_pop__`. Clear `Working` when an Invoke token is superseded.

### D6 — DatePicker defaults use the UTC calendar date — **Medium, new in M9**
* **Cause.** The new `sanitizeValue` serializes Dates with `toISOString()` (`extension-runner.ts:3184-3187`), and DatePicker then slices the result to 10 chars (`:3519-3521`). Changes, by contrast, are parsed as local midnight (`:3536`). The `DateTime` time component is discarded, and native offers only a YYYY-MM-DD text field.
* **Probe.** `opus_probe_date_default_local`: in `Asia/Almaty`, `new Date(2026,10,7)` becomes `"2026-11-06"`. In `America/Los_Angeles`, `new Date(2026,10,7,23,30)` becomes `"2026-11-08"`. Both pass only in UTC (`logs/probe-date-*.log`).
* **Consequence.** Users east of UTC, including Kazakhstan, see the previous day for local-midnight defaults; users west of UTC see the next day for late-evening defaults. The M9 `form` fixture uses a UTC-midnight literal, which masks the bug.
* **Fix.** Serialize DatePicker values as the local calendar date (`getFullYear/getMonth/getDate`) or as full ISO plus the field type. Convert back symmetrically. Run the fixture under a non-UTC `TZ` with a local-midnight default.

### D7 — Inline preference setup tells the user to re-run a no-view command that already ran — **Medium, new in M9**
* **Cause.** `save_setup` (`preferences.rs:146-215`) re-runs the command and passes the reply to `activate()`, which accepts only `mode:"view"`. A no-view reply therefore becomes "Preferences saved. Run the command again." (`:207`).
* **Probe.** `opus_probe_inline_setup_then_noview_command`: the side effect ran once (`side_effect_copies=1`), and the view stays on PreferenceSetup with that message (`logs/probe-inline-setup-noview.log`).
* **Consequence.** Following the prompt repeats the side effect: double toggle, double post, double copy.
* **Fix.** Handle `ok:true, mode:"no-view"` after setup. Show the `WorkerRecords` notice, then emit `Back` or `Close`. Treat only `ok:false` as failure.

### D8 — Grid defaults to one column and image admission assumes five — **Medium, regression vs the TS renderer**
* **Cause.** `view.rs:1195-1203` defaults to 1 and clamps 1–12. `images.rs:346-353` defaults to 5. The TS renderer defaults to 4 and clamps 2–8 (`grid-runtime.tsx:158-161`).
* **Probe.** `opus_probe_grid_default_columns`: Down from `g0` selects `g1`; the expected item is `g4`.
* **Consequence.** Grids without `columns` render one 148 px tile per row. Arrow-key steps are wrong. After scrolling, viewport image leases are taken for items 5× further down, so visible tiles show placeholders.
* **Fix.** Use one `grid_columns(root)` helper for render, keys and viewport: default 4, clamp 2–8, and map `itemSize` if supported.

### D9 — Left/Right are taken from the focused List search field — **Medium, regression vs the TS renderer**
* **Cause.** The `capture_action` handlers for `text_input::Left/Right` call `bound_key` → `key()`, which moves the selection and calls `stop_propagation` (`view.rs:1123-1155, 1414-1425`). In this GPUI version, bindings dispatch before listeners (`window.rs:6081-6230`), so the TextInput never receives them. The TS List does not handle arrows, and the TS Grid ignores them while the input is focused (`grid-runtime.tsx:268-273`).
* **Probe.** `opus_probe_list_search_left_arrow_moves_caret`: typing `abc`, then `down left X`, gives `"abcX"`; the expected text is `"abXc"` (`logs/probe-list-caret.log`).
* **Fix.** Never intercept Left/Right for List. For Grid, intercept them only when the search input is not focused (or its caret is at a boundary).

### D10 — MenuBarExtra status items: icon-only roots are blank and display-only items are dropped — **Medium, E-35 parity**
* **Cause.** `sync_sdk_status` never passes `root.props.icon` (`extension_worker.rs:769-779`) and keeps only items that have `actionIds` (`:696-700`).
* **Probe.** `opus_probe_menubar_info_items`: `MenuBarExtra icon={Icon.Gear}` produces status `("", icon=false, ["Refresh"])`, and "CPU 23%" is missing (`logs/probe-menubar-info.log`).
* **Consequence.** Most Raycast menu-bar commands are icon-only, so they produce an invisible or blank status item. Info rows vanish.
* **Fix.** Resolve the icon through the canonical asset service (registry scope, PNG ≤512, template) or a built-in glyph. Emit non-action items as disabled labels. Fall back to the command title.

### D11 — The action panel never scrolls the keyboard selection into view — **Low-Medium, native**
* **Cause.** `uniform_list("extension-action-rows")` (`view.rs:934-990`) has no `track_scroll`, and the list is 8 rows tall. `action_index` is not reset when the panel reopens or the action set changes.
* **Consequence.** With more than 8 actions, ↑/↓ moves the highlight off-screen. Enter can then target an invisible action.
* **Fix.** Add a scroll handle, call `scroll_to_item(row incl. section headers)`, and reset the index on `OpenActions` and when actions change.

### D12 — The `List.Dropdown` search-bar accessory is not a dropdown — **Low-Medium, E-16 parity**
* **Cause.** `view.rs:1204-1240` renders every item as an inline header button, with no current value, no selected state and no sections.
* **Consequence.** Realistic dropdowns overflow the header, and the current filter is invisible. E-16 "searchBarAccessory" semantics are not met. The fixture only checks that the node exists.
* **Fix.** Show one control with the current `value`/`defaultValue` that opens a popover list with sections, keyboard support and `aria-expanded`.

### D13 — Orphaned view replies are never disposed — **Low, new consequence of the M9 core session map**
* **Cause.** The launcher drops a view reply when `a.generation` changed or the launcher is hidden (`command_bar/mod.rs:1318-1321`); Store `run` does the same (`extensions_store/mod.rs:460-462`). Neither sends a `Dispose`.
* **Consequence.** A core session entry, a worker session and, for menu-bar commands, a status item stay alive until a re-run or EOF. Worker soft-limit pruning (30, `extension-runner.ts:237, 3895`) is not reported to the core, so status callbacks can outlive their worker session.
* **Fix.** When discarding a `mode:"view"` reply, send `ExtensionRequest::Dispose(sessionId)`. Have the worker emit or return pruned ids so the core retires them.

### D14 — Extension drain is app-wide, not extension-scoped — **Low-Medium, new in M9 (source-proven, not probed)**
* **Cause.** The "ordinary 16" extension permits are CoreClient's app-wide admission semaphore (`core_client.rs:222, 255, 483`). `drain()` acquires all 16 (`:519-536`). A Store install holds a permit for up to 900 s.
* **Consequence.** Quitting during any long native request, such as a Store install, always cancels after 8 s with the misleading "Could not save changes; quit cancelled". With D4, detached release-time drains starve new requests.
* **Fix.** Give extension operations their own semaphore and drain only that. Report "Extension install in progress" explicitly. Do not spawn detached drains from `on_release`.

### D15 — Copy and design defects — **Low**
* Store detail shows raw keys: `platforms: …`, `resources: …`, and buttons labelled `sourceUrl`/`website`. A second "Open README" can appear (`extensions_store/view.rs:124-136, 230-249`).
* Install and remove both report "Extension updated.", and failures drop the reason (`extensions_store/mod.rs:420-428`).
* Store Run replaces the worker's HUD/toast with "Command completed." (`:476`).
* The hotkey failure path discards the worker message (`command_bar/mod.rs:1734`).
* Grid tiles repeat the section title inside every tile (`view.rs:413-420`), and grid rows can mix sections.
* Preference-setup dropdowns show raw option values instead of titles and have no selected state (`view.rs:854-870`).
* Date accessories show raw ISO strings (`view.rs:362-368`).

No gradients, glass, side stripes or decorative badges were found in the extension surfaces.

## 4. Brief checklist (falsification results)

| Item | Verdict | Basis |
| --- | --- | --- |
| Desktop hooks run after validation | **Holds** | `platform.rs` validates text/html/file types, open, reveal and open-with, and NUL/empty targets, before `desktop_effect`. Clipboard files are validated before WriteFiles. `desktop_effect_tests` rerun is indirect (fixture records). |
| TestHost has no OS fall-through | **Holds for host effects** | `TestHost::desktop_effect` always returns `Some`. Real fixture runs still scan the real `/Applications` through `core:apps:list` at every run (read-only, not hermetic), and worker-local `osascript`/`execFile` cannot be intercepted by any Host. |
| Full 10,010-char copy vs truncated records | **Holds** | `sdk_actions` rerun: `copied_lengths == [10010]`, clipboard text 10010, records ≤50. |
| 60 waves / rolling 50 / zero replay | **Holds** | `sdk_actions` rerun: 60 waves, `diagnostics_count == 50`, 3 invalidation refreshes add no writes. |
| No delayed effect closes a new mount | **Holds on GPUI; fails on the Tauri default (D3)** | GPUI fences `extension:ui` by visible + `source == host_generation` + session (`mod.rs:353-363`). The fixture asserts only the TestHost default path. |
| AsyncLocalStorage keeps action identity | **Holds for actions** | `invokeExtensionAction` runs under `nativeControlSource.run`. Status-menu callbacks and initial-run controls carry source 0, so GPUI drops them: Preferences/HUD from a MenuBarExtra item do nothing. Gap, see E-35. |
| Source/entity/session fences across hide/back/reopen/remove | **Holds** except quit cancel (D4) and push desync (D5) | Integration test plus code. |
| Physical clipboard/open/reveal/selection/tray/capture not called by tests | **Holds** in all reruns and probes | TestHost and fixture GpuiHost deny. |
| Host default fallback preserves Tauri and source ownership | **Fails** (D1, D2, D3) | Shared worker and core behaviour changed for Tauri. |
| Native V2 status scopes and callback validator not spoofable | **Holds** | Scopes come from the core (`sdk-{uuid}`, `self.scoped(id)`). The callback re-checks the session map, and SDK icon paths are not admitted. The shared validator now accepts nested kinds, which TauriHost cannot render (D2). |
| Cleanup on rerun/disable/uninstall/shutdown/EOF | **Holds** for TestHost | `sdk_menubar` rerun: 130 reruns, disable, uninstall, stale callback. EOF drains sessions in the `finished` task. |
| Hidden core refresh does not consume the visible revision | **Holds** | `viewRevision` comparison (`extension-runner.ts:3734`); `sdk_menubar` hidden "Fixture 2" is then visible. |
| Cancellation keeps admitted mutations owned; 16 + reserved 16 bounded; one runtime | **Holds, but the scope is app-wide (D14)** | `core_client.rs:475-536`. Bound to the gpui_tokio handle (`command_bar/mod.rs:219`); no second runtime. |
| Quit does not irreversibly break extension UI | **Fails** (D4, probe) | |
| Event union of 25 channels, 100 reopens free allocations | **Recorded, not independently rerun** | The capacity is **96**, not 32 (`events.rs:334`). `subscribe` panics through `expect` when it is exceeded or after `close()`. The audit is a brittle source-text scan that asserts `< 96`. |

## 5. Per-ID verdicts (E-01 to E-45)

Columns: caller/source traced | probe or evidence | verdict. "Headless" means real worker + TestHost + RuntimeView, with no pixels, AX or platform checks.

| ID | Caller / source | Evidence | Verdict |
| --- | --- | --- | --- |
| E-01 | Launcher → `Surface::Extensions` → StoreView (`extension_integration.rs:83-149`); debounced search, paging, GitHub URL install | 4 scripted unit tests; no live store (by scope); Windows Tezbar-loved catalog unproven | Not accepted (manual/Windows) |
| E-02 | `extensions_store/view.rs:88-251` | No screenshot | Not accepted; D15 copy |
| E-03 | `mutate`, install-progress events | Scripted only; no authorization or progress test | Not accepted; D14, D15 |
| E-04 | M7 route (`StoreEffect::Settings`) | Integration test route reached; M7-owned | Out of M9 scope |
| E-05 | Inline setup (`preferences.rs`); M7 editor | `sdk_form` rerun | Not accepted; **D7**, D15 |
| E-06 | RuntimeSession + RuntimeView pump/complete | `sdk_invalidation` coalescing rerun | Not accepted; **D4, D5, D13** |
| E-07 | `backActionId` `__nav_pop__`, Esc → Back | `sdk_actions` push/escape rerun | Not accepted; **D5** (slow push + search) |
| E-08 | Action panel (`view.rs:889-990`) | `sdk_actions` sections/shortcut/destructive | Not accepted; **D11**; no AX/visual |
| E-09 | Copy/paste/open/reveal/push/pop/submit | `sdk_actions`, `sdk_form` rerun | Headless ok; platform pending |
| E-10 | Records never executed; controls via `extension:ui` | `sdk_actions`/`feedback` rerun; probe no-view RED | Not accepted; **D1, D3** |
| E-11 | List search/filter/isLoading/pagination | `sdk_list` 1000 rows rerun | Not accepted; **D9** |
| E-12 | Section headers on the first row | `sdk_list` | Headless ok; visual pending |
| E-13 | `row_view` | `sdk_list` | Headless ok; D15 date accessory |
| E-14 | `item_detail` + metadata | `sdk_list` | Headless ok; visual pending |
| E-15 | Empty title/description; icon ignored | Source only | Not accepted |
| E-16 | `dropdown()` | Node presence only | Not accepted; **D12** |
| E-17 | Grid columns/filter | `sdk_grid` (explicit columns=3); probe RED | Not accepted; **D8** |
| E-18 | `grid_row` | `sdk_grid` colour + 1×1 decode | Not accepted; D15 (section repeat) |
| E-19 | Grid empty | `sdk_grid` | Headless ok |
| E-20 | Markdown via MarkdownView, tickets | `sdk_detail` decode rerun | Headless ok; visual pending |
| E-21 | Detail metadata | `sdk_detail` | Headless ok |
| E-22 | Rust colour rows | Unit test; 3 TS visual cases unmatched | Not accepted (visual) |
| E-23–E-27 | Form, text, area, password, checkbox | `sdk_form` rerun incl. validation recovery and masking | Headless ok; IME/AX pending |
| E-28 | DatePicker text field | Probe RED | Not accepted; **D6** |
| E-29, E-30 | Dropdown and TagPicker button lists | `sdk_form` defaults | Headless ok; visual pending |
| E-31 | FilePicker via the App dialog lease | Unit test (ticket fence) | Not accepted (real dialog/blur manual) |
| E-32 | Description/Separator | `sdk_form` | Headless ok |
| E-33 | HSV wheel | Unit test | Not accepted (pointer/AX manual) |
| E-34 | PreferenceSetup | `sdk_form` view path; probe no-view RED | Not accepted; **D7** |
| E-35 | `sync_sdk_status` → status items | `sdk_menubar` (TestHost); probe RED | Not accepted; **D2, D10, D13**. Design gap: status items live only while their RuntimeView session is current (Esc, Back or opening another command removes them), and menu-item controls are dropped (source 0). Persistence semantics need a user decision. |
| E-36 | Viewport leases, 128 entries / 16 MiB cache | Fixture decodes | Not accepted; **D8** viewport mismatch |
| E-37 | Worker status → error strings | None | Not accepted |
| E-38 | Hotkey → visibility → `run-extension-command-from-hotkey` | None | Not accepted; D15 copy |
| E-39 | Launcher argument search | None in M9 | Not accepted |
| E-40 | Shared chrome | 9 nestedViewChrome TS cases unmatched | Not accepted (screenshots) |
| E-41 | Worker lifecycle | Smoke rerun; EOF cleanup | Not accepted (Windows); D13 prune |
| E-42 | Native V2 process host | Recorded status/host-port tests only | Not accepted; D2 for Tauri nested items |
| E-43, E-44 | Unchanged | Recorded only | Not accepted |
| E-45 | No not-reachable proof provided | None | Not accepted |

## 6. Spec, standards, design and test-map honesty

* **Honest.** `final-source-case-map.json` marks all 45 rows `acceptance:false`. It states that 97 of the 115 cases map to *model* filters, not native interaction tests, and it names the 18 unmatched cases (mostly nestedViewChrome and native-v2-visuals).
* **Not proven by the evidence it is cited for:**
  * "No delayed effects close a new mount" (`final-api.md`) is proven by the GPUI unit test, but the real-SDK `feedback` fixture asserts the *TestHost default* (Tauri-like) path.
  * "A failed preliminary quit does not irreversibly dispose extension state" is literally true but omits the final-drain failure (D4).
  * "Host default None follows the original native routine" holds for `desktop_effect`, but not for `extension_control` or SDK status sync (D1-D3).
  * The real-SDK harness uses its own `CoreTransport`, not the production `CoreClient`, so CoreClient admission (D14) is not exercised by the nine scenarios.
  * The fixtures are not hermetic: they scan the real app inventory.
  * The `form` fixture's date assertion relies on UTC semantics (D6).
* **Design.** No prohibited AI patterns were found in the extension surfaces. Remaining issues are layout parity (D8, D12, D15). Visual acceptance still needs reference screenshots in light and dark (spec §5.4).

## 7. Manual and platform evidence gaps (explicit, not inferred)

* Windows build/tests, ConPTY and assets: both attempts failed before tests (Pi OAuth 401, Codex quota).
* Physical IME, VoiceOver/Narrator, GPU paint, RAM/CPU idle and performance.
* Light and dark screenshots for E-02, E-12–E-22, E-29–E-33 and E-40.
* Real file dialog, blur suppression, real status bar on macOS and Windows (including icon-only items), real hotkey path, live store install and authorization, Tauri tray behaviour (D2/D3 are source-proven only).
* The dirty db4d0df preview binary (`e3bb9e91…`) is not clean pin or performance evidence. I did not execute it.

## 8. Paths and integrity

Work dir: `/tmp/tezbar-m9-opus-0513321`. Pristine archive: `src/`. Probe copy: `src-probe/`. Probe sources: `probes/src/{opus_probe.rs,opus_quit_probe.rs,opus_probe_access.rs}`. Probe seed: `probes/seed-probe/`, with `probes/prepared-probe.json` pointing at the owned rebuilt bundle in `tmp/tezbar-m09-fixtures-lsXLQ1/`. Probe binary: `probes/bin/tezbar_native-6a4ed6222e10c177`. SHA-256 values are listed in `logs/SHA256SUMS.txt`, written after this report.

Logs:
* `logs/blob-verify-{before,after}.txt`, `logs/retained-bundle-verify.txt`, `logs/bundle-rebuild-vs-retained.diff`, `logs/prepare-rebuild.log`
* `logs/build-native-tests{,-2,-3,-4,-5}.log` (the first is the RED compile with the probe-visibility error, kept)
* `logs/rerun-real-sdk-9.log`, `logs/rerun-focused-native.log`, `logs/focused-native-list.txt`, `logs/rerun-ts-worker-focused.log`, `logs/rerun-worker-smoke.log`
* `logs/probe-runtime.log` (D1, D4, D5, D8 and the control), `logs/probe-date-{Asia_Almaty,America_Los_Angeles,UTC}.log`, `logs/probe-menubar-info.log`, `logs/probe-list-caret.log`, `logs/probe-inline-setup-noview.log`
* `logs/ts-diff-db4-pin.patch`, `logs/core-diff-db4-pin.patch`, `logs/environment.txt`

Environment: rustc 1.98.1 (48a229cea), cargo 1.98.1, Bun 1.4.0, Node v26.7.0, macOS 26.3.
