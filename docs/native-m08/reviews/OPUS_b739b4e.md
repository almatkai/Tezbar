# Opus 5.5 review: M8 repair and narrow M7 N-B1 closure at b739b4e

- **Worktree:** `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`
- **Branch:** `feat/native-gpui-app`
- **Pin:** `b739b4ebdf90d906f67efa00a4a9c523fdc58f83`. This is HEAD. Its parent `05e12cb` is N-B1.
- **How the review was done:**
  - One reviewer, no subagents.
  - No live-tree edits, builds, stage, commit, fetch or push.
  - No GUI, clipboard, Keychain, provider, remote or Windows actions.
  - XERJ was not used. Exact `git`, `rg` and `grep` reads were authoritative.

## Verdict

**NOT ACCEPTED.** Three of the nine original findings are not fully closed:

- **M1:** the SVG bomb has new bypasses, confirmed by probe.
- **L1:** quit preparation starts a new agent run, confirmed by probe.
- **Windows:** the repair adds a likely attachment regression. This is source-traced only and has not run on Windows.

Everything else is closed at its real caller:

- H1, H2, M2, M3 (Unix), L2, L3 and L4
- the narrow M7 N-B1 repair

This is not acceptance of Windows, RAM, GPU, AX, IME, M9/M10/M11 or the full migration.

| ID | Verdict | Basis |
| --- | --- | --- |
| H1 | **CLOSED** | Source trace of `app.rs` quit, `chat_integration`, `ChatView`. Pristine: 3 actual-App failed-quit cases pass. |
| H2 | **CLOSED** | Source trace of all chat decode sites. Pristine fixture passes. |
| M1 | **OPEN, incomplete repair** | Release probe with the verbatim pin code. Marker and CSS-filter bypasses (N1 below). |
| M2 | **CLOSED** | Source trace. Recorded `core-assets-final` passed. Not rerun by me. |
| M3 | **CLOSED on Unix**. Windows not executed. | Source trace. Recorded core gates. |
| L1 | **PARTIAL** | Durability is closed. A run starts during quit preparation (N2, probe). |
| L2 | **CLOSED** | Source trace. Strengthened test fills 64 slots. |
| L3 | **CLOSED** | The real render caller passes full `options.len()` to `uniform_list`, and `model_options` does not truncate. There is no render fixture. |
| L4 | **CLOSED for the §4.1.7 extension tool card** | Core `to_string_pretty` → scrub → 4000-char cut is traced. Native renders `approval_body` unchanged. |
| Windows root | **OPEN, platform-blocking** | The `Prefix::VerbatimDisk` correction is sound by reading. One new likely defect (N3) and one preexisting (N4). Nothing ran on Windows. |
| M7 N-B1 | **CLOSED** | Source trace. Pristine GREEN 4/4. My 3 falsification probes pass. The RED 3/1 is the recorded log only. |

## Integrity and environment

- **Archive check:**
  - `git archive b739b4e` → `/tmp/tezbar-gpui-m78-opus-b739b4e-work/src`
  - 2175/2175 blobs match `git ls-tree -r`, 0 mismatches (tar SHA-256 `82ee9d2a4dfb8c7f…`)
  - `crates/tezbar-core/src/assets_windows.rs` = `971a2ea6…8dda`, which equals the documented parent override in `repair-parent-verification.json`. This is the explicit Windows-only parent delta, not a match against the child manifest's old hash.
  - The untracked M9/M11 drafts in the live tree are not in the pin and were ignored.
- **Gate logs:** all 33 `repair-gates.json` `log_sha256` values match their logs at the pin. Recorded totals:
  - native-frozen-final: 316 passed
  - bench-frozen-final: 317 passed
  - ui-final: 57 passed
  - model-final: 379 passed in total
- **Original reports are byte-unchanged at the pin:**
  - `OPUS_dd7bc19.md` was only ever touched by `b626f28`.
  - `OPUS_b626f28_CLOSURE.md` was only ever touched by `05e12cb`. Its SHA-256 `9dcaf30b…` matches my original `/tmp/tezbar-gpui-m7-opus-b626f28-closure.md`.
  - `OPUS_409dbf3_CLOSURE.md` is `a2b822ca…` and `OPUS_290e414.md` is `ca8ea258…`, both as before.
- **Build:**
  - Toolchain: `cargo +1.98.1 test -p tezbar-native --lib --features runtime-shaders --locked --offline -j2`, with `CARGO_INCREMENTAL=0` and `TEZBAR_NATIVE_SOURCE_SHA=<pin>`.
  - Private target: an APFS clone of the old reviewer target `/tmp/tezbar-m8-opus-target`, which was read-only and not mutated. It was not a shared target. Real disk use grew by about 4 GB.
  - Every run had a `perl alarm` bound. No owned processes remain. The one `cargo` process visible in `pgrep` belongs to another session's scratchpad and was not touched.
- **Preserved binary:** `/tmp/tezbar-native-m8m10-b626f28-verified-preview` was not launched. As stated, it is a dirty b626f28 preview, not a pin build or RAM proof.

### Runs I performed (pristine pin archive)

| Filter | Result |
| --- | --- |
| `repair_` | 16 passed (316 listed, which matches native316) |
| `actual_blocking_decodes` | 1 passed |
| `nb1` | 4 passed (GREEN) |
| `review_closure` | 7 passed (includes the unchanged dropped-barrier test) |
| `onboarding::` | 8 passed |
| `settings::` | 49 passed |

- I did not rerun the full native, bench, UI, model, core or MSRV suites. I accept them as recorded evidence only.
- I did not rerun the N-B1 RED myself.
- The recorded `nb1-red.log` shows 3 failed and 1 passed. The failures print the Onboarding re-record (`shown_hotkey="Alt+Space"`, rejection error) and the dropped-waiter `Err(REJECTED)`.
- `git diff b626f28 b739b4e` on `settings/tests.rs` and `onboarding.rs` has no deleted lines, so the tests are additive only.

### Probes I added (copy only: `/tmp/tezbar-gpui-m78-opus-b739b4e-work/probe`, diff `probe.diff`)

| Probe | Result |
| --- | --- |
| `opus_b739_nb1_replacement_also_rejected_reports_rejection` | pass: the replacement's own rejection reaches its waiter, and `owner_pending=false` |
| `opus_b739_nb1_two_rerecords_then_accept` | pass: K (held, rejected) → X (rejected) → J accepted. J returns `Ok`. A later unrelated same-owner edit inherits nothing. |
| `opus_b739_nb1_live_old_waiter_and_retire_midflight` | pass: the live old waiter gets `Ok` (superseded). Retiring the owner mid-flight leaves the other owner `Ok`. |
| `opus_b739_quit_preparation_does_not_start_agent_run` | **FAIL**: `runs_before=0 runs_during_quit_preparation=1 settlement=Ok(true)` (N2) |
| Release SVG probe (`svgprobe/`, verbatim `decode_svg` and `svg_preflight` from the pin) | see N1 |

Logs: `probe-run.log` and `pristine-*.log` in the work folder, plus the `svgcases/*.svg` and `*.time` files.

## Actionable findings

### N1 — MEDIUM–HIGH: M1 incomplete. Marker instancing and CSS filter functions bypass the SVG preflight (repair-scope, confirmed by probe)

- **Trigger and callers:**
  - Chat Markdown model output or a prompt-injected `data:image/svg+xml` or HTTPS SVG: `document_image` → `decode_image_job` → `decode_blocking` → `decode_chat_preview` → `decode_svg`.
  - Extension assets: `decode_asset` → `decode_svg`.
- **Why it gets through:** `svg_preflight` refuses only the local names `filter` and `pattern` and bounds `<use>`. usvg 0.46 still does two other things:
  - It instantiates `<marker>` content for **every path vertex** during `Tree::from_data`, before the post-parse 8192-node `budget()` runs. The vertices sit in one `d` attribute, so they add no `<` count.
  - It applies CSS **filter functions** from the `filter` attribute, the `style` attribute or a `<style>` sheet. No `<filter>` element is needed.

| Case (release build, verbatim pin code) | Size | Preflight | Result | Peak RSS |
| --- | --- | --- | --- | --- |
| marker, 20k vertices × 50 children | 178 KB | Ok | budget Err after 0.47 s | **482 MB** |
| marker, 60k vertices × 50 children | 529 KB | Ok | budget Err after 1.44 s | **1.44 GB** |
| marker via `<style>path{marker-mid:…}` | 178 KB | Ok | same as above | 482 MB |
| 100 rects `filter="blur(40)"` | 8.3 KB | Ok | renders in **7.0 s** | 55 MB |
| 300 rects `filter="blur(40)"` | 21 KB | Ok | **21.4 s** (linear) | 56 MB |
| 100 rects `drop-shadow(...)` | 10.6 KB | Ok | 11.1 s | 87 MB |
| 50 rects with one-line `<style>rect{filter:blur(40)}</style>` | 2.7 KB | Ok | 3.5 s | 55 MB |
| controls: nested `<use>` / `<filter>` element / legitimate `<use>` icon | — | rejected / rejected / Ok | as designed | — |

**Consequences:**
- Memory scales linearly with source size. An 8 MiB chat source reaches roughly 20+ GB, and up to 4 such decodes run concurrently: a reliable OOM of the app process.
- Filter functions cost about 70 ms per element. At the 8192-element cap that is about 10 minutes of release CPU per image (debug builds are slower). Each such decode holds one of the **4 shared** `AssetService` permits until it finishes, by design.
- Four such images therefore starve chosen attachments (10 s admission timeout → "Image read timed out"), remote and extension images, and chat previews.

**Fix (root, small):**
1. In `svg_preflight`, add `"marker"` to the refused local names. Without the element, a marker reference is inert however it is referenced.
2. In `budget()`, refuse any `Group` with non-empty `filters()`. This works after parsing and covers the attribute, `style` and CSS routes.
3. Optionally, put a small cap (about 64) on isolated layer groups (`should_isolate()`: opacity, mask, clip). An uncapped `opacity_n1000` took 2.7 s, which extrapolates to about 22 s at 8192.

**Tests:**
- the marker 60k×50 case is rejected in under 50 ms with no large allocation
- the attribute, `style` and `<style>` blur variants are all rejected
- the existing nested-use rejection and legitimate `<use>` icon test still pass

### N2 — LOW: L1 partial. A preparation completing during quit preparation starts a new `agent:run` (repair-scope, confirmed by probe)

- **Caller:**
  - `runtime.rs` `Completion::PreparationConfig` and `Completion::Prepared` use `preparation_current()`, which checks `!self.disposed` but not `quit_preparing`.
  - The App preliminary phase only sets `quit_preparing`, so a held `chat:append` that completes during preparation runs `controller.prepared_with_selection` and then `StartRun`.
  - That `Start` is a mutation job, so the preliminary barrier waits for it and then reports `Ok(true)`. Final `dispose` then sends Cancel.
- **Consequence:** pressing Send just before Quit makes the app issue a real provider run during quit, then cancel it. That means possible token cost and possibly an auto-approved tool step before the Cancel lands. The persisted result is a user turn plus "Stopped by you.".
  - This contradicts the input fence, whose intent is that no new runs start during quit.
  - The new L1 test asserts durability only. It does not count `agent:run`.
- **Fix:**
  - While `quit_preparing`, have `Prepared` store `(session, token, canonical, selection)` in a deferred slot instead of calling `prepared_with_selection`.
  - `revive_after_quit` replays the deferred slot, so a cancelled quit still sends. `dispose` drops it, so `stop` restores the draft and the canonical user turn is already durable.
  - `PreparationConfig` may continue to the `Append`, because that is durable work.
- **Test:** extend `repair_prepared_append_is_durable_work…` (or my probe) to assert `count("agent:run") == 0` during preparation, and `== 1` after `revive_after_quit`.

### N3 — MEDIUM (Windows only; PLAUSIBLE, source plus documented OS behaviour, not executed): chosen chat attachments now go through the scoped extension alias policy

- **Caller:**
  - `AssetService::chosen_image` → `read()` → `windows::open_locked(&canonical)`. This path is new in the repair; dd7bc19 used a plain open.
  - `open_locked` refuses any ancestor or file with `FILE_ATTRIBUTE_REPARSE_POINT`.
  - `safe_name` refuses `~`, a trailing `.` or space, and the device-name stems, all **after** canonicalization.
- **Consequence:**
  - OneDrive Files-On-Demand placeholders are reparse points (cloud-files tags), even when hydrated. With Known Folder Move, Desktop, Documents and Pictures live under OneDrive, so attaching an image from them would fail with the generic "Could not attach image".
  - Long names that legitimately contain `~` (for example `icon~dark.png`) also fail.
  - For a user-chosen file, aliasing is not an authority boundary. The user already holds that authority.
- **Fix:**
  - For `chosen_image` on Windows: open the canonical final file without the ancestor walk, require `is_file` and `GetFileType == FILE_TYPE_DISK`, and keep the device, pipe and FIFO refusal.
  - Keep `open_locked` for extension-scoped reads only.
- **Real-Windows test:** attach from an OneDrive-synced folder, and attach a file whose name contains `~`.

### N4 — LOW (Windows only, PREEXISTING since dd7bc19, plausible): legacy icon grants are likely always empty on Windows

- `extensions/asset_scope.rs:41` inserts a grant only if `candidate.canonicalize() == candidate`.
- On Windows, `canonicalize` returns `\\?\C:\…`. Unless `extension.path` is already verbatim, the comparison never holds, so legacy registry icons outside the root silently disappear. This fails closed, so it is not a security issue.
- The Windows test calls `read_scoped` directly with an already-verbatim grant, so it bypasses `authorize`, `asset_scope` and ui-model `resolve`.
- **Fix:** compare on the verbatim spelling of `candidate`. Also add a real-Windows test through `CoreClient::load_asset`.

## Per-ID detail

### H1 — CLOSED

- **Preliminary phase (non-destructive):**
  - `m8_prepare_quit` → `prepare_chat_quit` fences input (`ChatView::action` and `activate` refuse while `quit_preparing`) and the terminal (`prepare_lifecycle_quit`).
  - It settles only mutation jobs and dirty persistence, with no unmount or Cancel.
- **Final phase:**
  - Starts only after the launcher, chat, Settings (`drain_with`), Settings window and layout barriers return `Ok(Ok(()))`, and after a synchronous recheck of `store.pending()` and `barrier.is_clean()`.
  - Then `dispose_chat` + `drain_chat` + `drain_terminal` run, and a post-recheck follows.
- **The existing `drain_chat` API is unchanged:** it disposes and then settles. `dispose` no longer drops `clock`.
- **On any `Err` (preliminary, final, timeout or post-recheck):**
  - `resume_cancelled_quit` runs before `restore` and `start_core`.
  - It clears `chat.disposed`. `revive_after_quit` clears `quit_preparing` and `disposed` and reacquires a fresh `store.owner()`. The retired owner had nothing pending, because the store was checked clean before dispose.
  - Subscriptions (`chat.events`, `observation`) are never dropped.
  - Unmount set `controller.visible=false`, so the later `show_chat` → `set_visible(true)` is not short-circuited.
- **Cancel counts:** each run is cancelled exactly once. The "append" case fails in the final phase after one Cancel. Approval is preserved for the settings and layout failures. A new send, approve and later successful quit all work.
- **Budget:** one 8 s timer covers preliminary plus final. Core shutdown gets `8s − elapsed` (zero means bounded exit), using the same `ShutdownOwner` task.
  - On timeout, the dropped futures abort only waiters. Settings pump and chat jobs are owned elsewhere and keep running, and the quit reports an error, so no unknown write is reported as settled.
- **Test-map gap (minor):** the fixture sets `v.visible = true` by hand before resuming, so it does not prove that `restore` re-shows the launcher. That is source-traced only.

### H2 — CLOSED

- The only chat decode site is `decode_image_job`. History, Markdown and data images all funnel through it.
- `decode_blocking` acquires an owned permit from the **shared** `AssetService` semaphore (`preview_admission`) and moves it into `spawn_blocking`.
- `cancel_decode_waiters` runs at every `images.release` caller (activate, hide, RetryImages, both clear paths, dispose). It drops only queued waiters; running jobs keep their permit.
- Late results still pass `document_images`/mount validation and the cache ticket check.
- **Test-map:** the fixture drives `decode_blocking` plus `OwnershipCache` directly. The ChatView release callers are source-traced.

### M2 — CLOSED

- Admission is FIFO `acquire_owned` inside the per-operation 10 s deadline, for `remote`, `blocking` and `decode_asset`.
- **Limit (not a defect):** `chosen_chat_images` reads up to 4 paths one after another, each with its own 10 s deadline. Worst case is about 40 s.
- N1 makes long permit holds attacker-reachable until it is fixed.

### M3 — CLOSED on Unix

- `blocking()` owns the permit across canonicalize, authorize, open, `fstat` and the bounded read in one closure.
- Opens use `O_NONBLOCK|O_CLOEXEC`, and `!is_file` (FIFO, device) is refused before reading.
- `read_scoped` still walks descriptors from `/` with `O_NOFOLLOW`.
- **Residual by design:** a stalled network or FUSE `canonicalize` holds a permit until it returns.

### L2 — CLOSED

- Background effects coalesce.
- At 64 entries, chooser and clipboard tickets are removed synchronously, document images complete with an error through the usual path, and nothing is emitted unqueued.
- The relay refusal path also settles tickets.
- The strengthened test really fills 64 slots.

### L4 — CLOSED for the spec's extension card

- **Observation, not an L4 defect:** the preexisting `providers/antigravity_tools.rs:117` (since 48fa44e) sends compact `args.to_string()` for "Allow Google search?" and "Allow image generation?". These are provider built-ins, not the `Allow {Extension} to use {tool}?` card, and the provider wire is out of scope.

### Windows root

- `is_verbatim_disk` correctly replaces the textual `starts_with`.
- The design matches my earlier proposal: handle walk, `BACKUP_SEMANTICS|OPEN_REPARSE_POINT`, READ-only share, reparse and `GetFileType` checks on the handle, held ancestors, and an exact `GetFinalPathNameByHandleW` match.
- Separately from N3 and N4, there is a functional risk: READ-only sharing on `C:\` and every ancestor makes admission fail if any other process holds write or delete access to those directories. This needs real-Windows observation and is not claimed as a defect.
- Nothing was compiled or executed for Windows here.

### N-B1 — CLOSED

- Rejected saves record their top-level patch keys. These equal the Global field names, from `WriteQueue::patch`.
- Same-owner `enqueue` removes that field from older rejections. A late result is filtered by `latest_globals`. Other fields and other owners are kept, and `retire` clears both maps.
- The waiter-capture claim logic is unchanged apart from iterating keys. The dropped-barrier test for a different key is unchanged and passes.
- In-flight and queued Copilot follow-ups and the revision order are covered by the passing `settings::` (49) and `review_closure` (7) suites.

## Standards and design

- **Design:** the changes are token-only. The new copy is specific ("Native action queue is busy. Retry.", "Chat is preparing to quit"). None of the forbidden patterns were introduced.
- **The earlier nit still stands:** shared TaskIndicator chat entries are titled generically.
- **Architecture:** one semaphore owner, no new dependency (`resvg` already locked, `roxmltree` re-export) and no second runtime.
- **Code style:** `app.rs` quit now nests `prepare` inside `prepared` with irregular indentation, and has several one-line `if` / `let _=` statements in the terminal wiring (M10). This is readable but not rustfmt-clean.
- **Not covered:** this review makes no RAM, GPU, AX, IME or physical-window claims.

## Limits

- No real Windows run.
- No full-suite rerun.
- No mutation run of the M8 fixtures.
- N3 and N4 are source plus OS-documentation reasoning only.
- I did not trace M10 terminal internals beyond the quit fence and revival, as agreed.
- Artifacts are in `/tmp/tezbar-gpui-m78-opus-b739b4e-work/`. The `target/` folder there is an APFS clone, about 165 GB logical. Delete it when no longer needed.
