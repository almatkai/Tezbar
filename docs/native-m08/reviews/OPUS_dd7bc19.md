# Independent Opus 5.5 critical review — M8 native chat/composition/assets at dd7bc19

- Worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-gpui-app`
- Immutable pin: `dd7bc194d335cd33caee45e1ce4d4cc1ef54a9a3` (HEAD at review time is the same commit; the live tree's dirty entries belong to M10 and were not touched).
- Reviewer worked alone. No subagents, no live-tree edits/builds/stage/commit/push, no GUI/clipboard/Keychain/provider/MCP use. The frozen binary was not launched.
- **Verdict: NOT ACCEPTED.** There are 2 high, 3 medium and 4 low actionable findings, plus the Windows root finding that remains open. This is not full-migration, RAM, GPU or platform acceptance.

## Method and probe environment

| Item | Value |
| --- | --- |
| Pristine archive | `git archive dd7bc19` → `/tmp/tezbar-m8-opus-dd7-archive` (2054 files = `git ls-tree -r` count; archive stream SHA-256 `e2a9a3f7…84804cb`) |
| Probe copy | `/tmp/tezbar-m8-opus-dd7-probe` (APFS clone of the archive). The only edits are two appended probe tests, in `apps/tezbar/src/app/m8_tests.rs` and `crates/tezbar-ui/src/image_cache.rs`. |
| Target | `/tmp/tezbar-m8-opus-target` (APFS clone of the live `target/debug`, 81 s, copy-on-write). One transient `rustcXXXX` tmp file was mid-write by M10 and skipped; this is harmless and cargo re-fingerprints. |
| Toolchain | `cargo +1.98.1 … --locked --offline -j2`, `TEZBAR_NATIVE_SOURCE_SHA=dd7bc19…` |
| SVG probe | Standalone crate `/tmp/tezbar-m8-opus-svgprobe` on the same locked `resvg/usvg 0.46.0` with both href resolvers denied (same options as `decode_svg`) |
| Deadlines | Every probe ran under `perl -e 'alarm N'`. One debug SVG run (depth 14) was killed with 137 by the outer bound and not retried at that size. No owned processes remain (`pgrep` empty). |
| Frozen binary | `/tmp/tezbar-native-m08-parent-evidence/tezbar-frozen-preview`, SHA-256 re-verified `f86c946a…ef544`. Not launched. |
| XERJ | Not used, so there is no stale index risk. Exact `git`/`rg`/`grep` reads were authoritative. |

Commands and results:

1. `cargo test -p tezbar-ui --lib opus_probe -- --nocapture`: **FAILED as predicted.** `OPUS_PROBE round=2 admitted=4 total_inflight=12`, `in-flight decodes exceed MAX_DECODE_TASKS=4: 12`. 52 baseline tests were filtered, which matches the UI52 gate.
2. `cargo test -p tezbar-native --lib --features runtime-shaders opus_probe -- --nocapture`: **FAILED as predicted.** `OPUS_PROBE mounted=false controller_visible=false runs_before=1 runs_after=1`, panic at the `chat surface stranded after cancelled quit` assertion. 254 tests were filtered, which matches native254.
3. `cargo test -p tezbar-core --lib assets` (unchanged core source): 10 passed, 0 failed. This reproduces the core-assets10 gate.
4. SVG nested `<use>`, release build, 980×980 pixmap:
   - depth 10: 46 `<`, 906 B, render 103 ms
   - depth 11: 50 `<`, 980 B, render 203 ms
   - depth 12: 54 `<`, 1054 B, render 403 ms
   - Cost doubles with each level. Debug build at depth 10: 2.47 s.
   - usvg's only cap is 1,000,000 nodes, so depth 19 (≈524k nodes, about 80 `<`) is admitted. Extrapolated cost is ≈50 s release and ≈20 min debug, per image. Larger sizes were deliberately not run.

I did not rerun the full native/bench/UI/model/TS/MSRV gates. Their logged results are accepted only as compile/test evidence; my findings below are not covered by those gates.

## Prioritized actionable findings

### H1 — A cancelled quit permanently strands chat and cancels every run, even when chat was not the cause (NEW, confirmed by probe)

- **Call sites:**
  - `app.rs:433` calls `m8_prepare_quit` first, before the Settings, layout and launcher drains.
  - `m8.rs:110` → `chat_integration.rs:312 dispose_chat` sets `ChatIntegration.disposed = true` and calls `ChatView::dispose` (`mod.rs:927`). That sets `disposed = true`, runs `controller.unmount()` (`stop` → `Cancel` for every run) and retires the SettingsStore owner.
  - On failure, `app.rs:494-522` cancels the quit, restores and restarts the core, but nothing resets either `disposed` flag or re-registers the store owner (`rg "disposed\s*=\s*false"`: no hits).
  - From then on:
    - `chat_mounted()` is false (`chat_integration.rs:45`).
    - `route_chat_navigation`'s deferred activation returns early (`:105`).
    - `ChatView::action`/`activate` return early or `Err("Chat has been disposed")`.
    - `core_event` drops `llm-config-changed`.
    - The TaskIndicator hides chat.
- **Trigger:** any quit-cancelling error. That includes a Settings/layout/launcher write failure or "Settings changed while quitting", not only a chat append failure. Every in-flight agent run, including hidden background runs and pending approvals, has already been cancelled. After that the user cannot open, resume or send chat until relaunch. The existing fixture only covers "fail, then quit again".
- **Probe:** `opus_probe_failed_quit_then_continue_new_send_runs`: after the failed quit, `submit(2)` is not mounted and no `agent:run` is issued.
- **Fix:**
  - (a) Split quit into a non-destructive `prepare` step. That step fences new input, retains runs and drains only persistence. Destructive `dispose` (unmount/Cancel/retire) runs only after every barrier in `prepared` succeeds, immediately before `events.dispose()` and core shutdown.
  - (b) Alternatively, if cancel-before-drain is a product decision, add `ChatIntegration::resume_after_cancelled_quit`. It clears `disposed`, calls a new `ChatView::revive` (clear `disposed`, re-acquire `store.owner()`, restart the clock, `set_visible` per Launcher state) and keeps dirty turns and retry ability.
  - Either way, the quit-failure path in `app.rs` must call it next to `restore`/`start_core`.
  - **Tests:** failed quit (one test each for a chat append failure and a Settings drain failure), then resume of the old session, new send, approve, and a later successful quit. Assert exactly one Cancel per run overall and no duplicate append.

### H2 — Chat preview decodes are not bounded to 4. ImageCache `release()` forgets in-flight tickets and decode jobs hold no slot (NEW, confirmed)

- **Call sites:**
  - `references.rs:533-592 decode_image_job` runs `tokio::task::spawn_blocking(decode_chat_preview)`. Each decode can take up to 128 MiB of decoder allocation plus an 8 MiB source copy, and holds no `AssetService` permit or other slot.
  - The only bound is `ImageCache::request`'s 4 pending tickets (`image_cache.rs:54`). `OwnershipCache::release` (`:127`) clears `pending`.
  - `release` is called on:
    - every `activate`/navigation (`mod.rs:419`)
    - `set_visible(false)` (`:484`)
    - `RetryImages` (`:633`)
    - clear (`runtime.rs:150/884`)
  - Dropping the GPUI task aborts only the async wrapper; the blocking decode keeps running.
- **Trigger:** history or Markdown with large images. Toggle hide/show, navigate between chats, or press "Retry image previews" repeatedly while decodes run. Each cycle admits 4 more blocking decodes. The effective ceiling is `jobs.len() < 128`, about 128 × 136 MiB ≈ 17 GiB of decoder allocation (plus the tokio blocking pool). The documented "four decode slots" and "slot retained until actual completion" are false for chat.
- **Probe:** `opus_probe_release_forgets_inflight_tickets` ends with 12 tickets in flight against a cap of 4.
- **Fix:**
  - Give chat a decode semaphore that is independent of cache generations: `Arc<Semaphore>(4)`, or reuse `AssetService::decode_permit`, with an owned permit moved into the `spawn_blocking` closure.
  - Acquire it before spawning; on refusal, queue the request (`pump_document_images`) instead of marking it unavailable.
  - Alternatively, have `OwnershipCache` keep an `inflight` counter that `release()` does not reset, decremented only by `finish` of the original ticket.
  - **Test:** start 4 slow decodes (inject a blocking decoder via a test seam), release 3 times, assert no 5th decode starts until one finishes, and assert late results are dropped without upload.

### M1 — SVG decode is a small-input CPU bomb holding a non-preemptible slot (NEW, confirmed)

- **Call sites:** `asset_transport.rs:341 decode_svg`. The `<` count ≤ 8192 does not bound `<use>` expansion; usvg expands `use` into up to 1,000,000 nodes. Paths:
  - Chat Markdown: model output or prompt-injected `data:image/svg+xml;base64,…` or HTTPS SVG, via `document_image` → `decode_chat_preview`. This path also has no slot (H2).
  - Extension/catalog assets via `decode_asset`, which holds one of only 4 shared `AssetService` permits until completion. A few such SVGs starve every image consumer, and chosen-file attachment (`chosen_image` uses the same `try_acquire`) for minutes.
- **Probe:** each nesting level doubles render time; ≈80 `<` gives ≈50 s release.
- **Fix:**
  - Parse with `usvg::Tree::from_data`, then walk the resulting tree. Reject when total node count exceeds about 10,000, and count `use` and filter primitives, which are amplifiers. `Tree` exposes its root group, so a recursive count is cheap.
  - Additionally, pre-scan raw XML for `<use`, `<pattern` and `<filter`, and cap counts.
  - Render on a bounded worker with an expected-work estimate.
  - **Tests:** a nested-`use` fixture is rejected in under 50 ms; a legitimate icon still renders.

### M2 — The shared `AssetService` uses non-waiting admission: attachment and asset loads fail spuriously "busy" (NEW, source-traced)

- **Call sites:** `assets.rs:66` `try_acquire_owned`; `chosen_image` (`:155`), `remote` (`:94`), `local` and `decode_asset` share 4 permits.
- **Trigger:** 4 chat Markdown HTTPS images in flight. Each holds a permit for up to 10 s; chat holds 4 cache tickets, matching the 4 permits. During that window, "Attach image" fails: `chosen_chat_images` → `Err("Image service is busy")`, mapped to the generic "Could not attach image…". Remote images that lose the race become permanently "unavailable" until the user retries manually. Once M9/M3 consumers are wired, contention grows.
- **Fix:** use bounded waiting (`acquire_owned` inside the existing 10 s deadline), or give user-initiated chosen-file reads their own reserved slot. Keep the drop-releases-permit property. **Test:** with 4 held permits, a chosen-image read completes after one is released, not instantly fails.

### M3 — `chosen_image` releases its slot while blocking I/O continues; FIFO/special files can hang blocking threads (NEW, source-traced)

- **Call sites:** `assets.rs:154-159, 303-319`. `tokio::fs::canonicalize/File::open/read` each run inside internal `spawn_blocking`, while `_permit` lives in the outer future. The 10 s timeout drops the permit, but the blocking open keeps running.
- **Trigger:** the dialog returns a FIFO or a stalled network/FUSE path. `open(O_RDONLY)` on a FIFO blocks forever with no `O_NONBLOCK`. `is_file()` is checked only after open, so each retry leaks a blocking-pool thread while the slot is freed. Similarly, `local()` acquires its permit before `authorize`'s canonicalize; if that times out, the permit drops while canonicalize keeps running.
- **Fix:**
  - Do the whole read in one `spawn_blocking` that owns the permit.
  - Open with `O_NONBLOCK | O_CLOEXEC`, `fstat` and require a regular file, then clear `O_NONBLOCK` or just read.
  - Move `authorize` inside the same permit-owning blocking job.
  - **Tests:** a FIFO is rejected without blocking; the permit count is restored only after the blocking job ends.

### L1 — The user-turn append during preparation is not a mutation job; quit drops it mid-flight

`runtime.rs:202-212` omits `Completion::Prepared` (`chat:append` of the user turn) and `PreparationConfig` from `mutation_jobs`. `dispose` (`mod.rs:932`) drops them.

- `stop` restores the draft in memory only, so `quit_settlement` returns `Ok(true)` while an append may or may not have committed.
- After restart, a half-landed user turn has no assistant reply, and the restored draft is lost.

**Fix:** treat `Prepared` appends as mutations, and settle them before reporting a durable quit. Alternatively, record the turn as dirty when its append job is cancelled.

### L2 — The effect queue silently drops past 64 entries while `cx.emit` still fires

`mod.rs:373-379`. A dropped `ChooseImages`/`ClipboardImage` leaves its `attachment_tickets` entry, so Send reports "Wait for the image action to finish" until mount changes.

**Fix:** refuse at emit time and complete the ticket with an error. Alternatively, coalesce `Background` (keep only the latest) so it cannot consume capacity.

### L3 — The model dropdown silently truncates to 256 options

`view.rs:1025` (`count = options.len().min(256)`). Without a filter there is no "more" affordance, so large catalogs (OpenRouter/Ollama) hide models. **Fix:** show an "N more, refine search" row, or virtualize the whole list (uniform_list already supports it).

### L4 — Unresolved approval body formatting parity

The approval body renders `approval.command` verbatim (`view.rs:446-451`). The spec (§4.1.7) requires formatted JSON arguments. I did not trace whether the core already pretty-prints. **Action:** assert in a loopback test that a JSON-object tool argument renders as indented JSON.

## Root finding (parent-checkpoint): Windows `read_scoped` TOCTOU — STILL OPEN, actionable platform security issue

`assets.rs:234-249` (non-Unix branch):

- It iterates only the relative components (the root and its ancestors are not re-checked) and calls `symlink_metadata().is_symlink()` per component.
- Rust's Windows `is_symlink` covers name-surrogate reparse tags, i.e. symlinks and junctions.
- It then does a path-based `File::open(current)`. Between the check and the open, any component, including an ancestor of the issued root, can be swapped for a junction/symlink and redirect the read.

I cannot prove this impossible, and the Unix `openat`/`O_NOFOLLOW` regression proves nothing about Windows. Classification: **outstanding, actionable, platform-blocking for Windows acceptance.** It is not merely an unperformed GUI check.

Proposed handle-based fix (do not weaken canonical-root policy):

1. Open each directory from the volume root down to the file via `CreateFileW` (or `NtCreateFile` with `RootDirectory` = the parent handle, for true relative opens) with these flags:
   - `FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT`
   - share mode `FILE_SHARE_READ | FILE_SHARE_WRITE` with **no** `FILE_SHARE_DELETE`, so held ancestors cannot be renamed or replaced while the handles stay open
2. On each handle, `GetFileInformationByHandleEx(FileAttributeTagInfo)`; reject `FILE_ATTRIBUTE_REPARSE_POINT` and require directory (intermediate) or regular file (final).
3. Keep all ancestor handles open until the final read finishes.
4. Defense in depth: compare `GetFinalPathNameByHandleW(final, FILE_NAME_NORMALIZED)` against the canonical issued root, and refuse `Component::Prefix` mismatches, ADS (`:`) components, DOS device names and short-name (`~`) components.
5. Apply the same file-type check before reading (no device/pipe).

Tests needed on real Windows:

- a loop thread swapping an ancestor/root/nested directory with a junction during repeated reads (must never return outside bytes)
- file symlink, directory symlink and junction denial
- ADS, `CON`/`NUL` and 8.3 names
- legacy exact-file grant under swap
- removal races

Also note that hard links inside an extension root bypass path policy on every platform. That is acceptable only because extensions already run with user file authority. Document this explicitly in the threat model rather than claiming filesystem isolation.

## Validated invariants (attempted falsification; held at this pin)

- **RunSelection:**
  - `prepare` (`runtime.rs:415`) drains the same SettingsStore owner, then calls `llm-config-get(cwd)` and `resolve_selection`.
  - The frozen `{provider, model}` goes into `Prepared` → `prepared_with_selection`.
  - `agent:run` emits only `runSelection` and omits `approvalMode` (`request.rs:100`).
  - Core `RunSelection::parse` has `deny_unknown_fields` and enforces trimmed, nonempty, ≤512-char, non-control identifiers. `for_run` uses canonical credentials and rejects disabled providers.
  - Mode edits (`Action::Mode`) only write `agentApprovalMode` and never `Approve`.
  - Enter approves only through a focused native button. The composer Enter path returns before Send when the input is unfocused, and IME composition is checked across all fields.
- **Legacy history:** canonical store first, then the legacy `chat_turns` row (excluding stages, timeline and image bodies; text ≤4 MiB, otherwise an explicit error; tombstoned sessions excluded), then dirty fallback, by exact IDs. Retry sends no image bytes and re-reads the canonical user turn via `chat:get`. Display truncation (title 128, history 200, diagnostics) never feeds `priorTurns`.
- **`chat:image`:**
  - nondeleted USER-turn ownership check, index < 8, no index shifting
  - blob hash validated as 64 hex
  - size ≤ `IMAGE_BYTES`
  - Not in `HOST_CHANNELS`, so there is no extension-worker authority leak. No `core:routes` seam exists at this pin.
- **`chat:clear`:** ≤64 owned IDs, validated before clearing, merged with active runs under the lifecycle lock. Native settles every ticket and executes deferred dirty writes on failure. History beyond 200 rows is cleared canonically, not by approximation from displayed rows.
- **Effect routing:**
  - one queue with an emit-time mount stamp; the parent consumes only via `observe` → `take_host_effects` (no EventEmitter subscription)
  - `m8_effect` re-checks the exact `EntityId`, mount and `chat_mounted`
  - stale chooser/clipboard/document results are refused at `chat_image_job`/`chat_document_image` completion and by view ticket fences
  - the dialog lease is reserved at relay admission and released on refusal or completion (release admission is independent of control capacity)
- **Capture:** uses the existing `CaptureLease` with `native::visible(false)`; no logical `set_visible`, so the capture ticket mount survives.
- **Persistent subscriptions:** installed in `ensure_chat` before `activate`; hidden runs keep streaming (`core_event` does not check visibility; only frames do).
- **Remote fetch:**
  - HTTPS only; userinfo denied, including on redirect
  - ≤4 redirects, auto-decompression disabled, `Content-Encoding` ≠ identity refused
  - declared and streamed caps enforced; overall 10 s timeout; drop cancels
  - data URI: exact MIME header, strict STANDARD base64, length pre-check
- **SVG:** both `ImageHrefResolver` closures return `None`. Unpremultiply-then-swap to straight BGRA is correct. Raster paths use `rgba_to_bgra`.
- **Raster bombs:** header dimension caps (16,384) plus `max_alloc` 128 MiB on both header and decode readers, so there is no decompression-bomb path for raster formats. SVG is the exception (M1).
- **Markdown:** discovery happens in `set_content`, not render. It keeps `Weak` handles, the strong owner is `ImageCache` with `drop_image` on hide/replace/dispose, and `ImageTicket` generations reject removed or re-added sources.
- **Quit timing:** `drain_chat` has a single 8 s bound. The release fallback (8 s) runs only when not disposed, so the two never chain. `GenerateMeta`/auxiliary HTTP are non-mutation jobs, dropped at dispose, and cannot veto saves. A failed quit, then a successful quit, issues exactly one Cancel (existing fixture). The stranding case is H1.
- **Lock/manifest:** the diff `409dbf3..dd7bc19` adds only the app→resvg edge (`Cargo.lock` +1 line) and exact `resvg =0.46.0`.

## Spec, standards and design judgments

- **Design:**
  - Token-based (`t.rim`, `t.text2`, `t.danger`, `controls::sheet/header/footer`).
  - None of these forbidden patterns appear: gradients, glass effects, purple/blue default accents, side accent stripes, badge clutter.
  - Copy is specific ("Allow once", "Always allow this tool/command", explicit failure guidance).
  - Shared footer suppresses the duplicate chat status list.
  - Minor: chat tasks in the shared TaskIndicator are titled generically "AI Chat" rather than with the conversation title, which weakens background-approval routing clarity.
  - No screenshot or pixel claims are made here.
- **Roles and AT:** roles are present (`AlertDialog`, `ListBox`, `Status`, `Alert`). TestWindow has no AccessKit tree, so physical VoiceOver/Narrator/IME acceptance remains open. The case map honestly closes zero ledger rows.
- **Architecture:** a single ChatController owner, finite `ChatRequest` vocabulary with redacted Debug, no second runtime or HTTP client per consumer, lazy shared reqwest client. H1 and H2 are lifecycle/ownership violations of the spec's §4.1 "one live owner" and of the resource-bound requirements.
- **Privacy note (parity, not a defect):** chat Markdown auto-fetches HTTPS images from model output, which allows tracking or exfiltration via the URL. The legacy app did the same with `csp: null`, and was weaker (it allowed HTTP). Consider click-to-load for model-originated remote images as a product decision.

## File coverage at the pin

Read in full:

- `ai_chat/{mod,runtime,request,references}.rs`
- `app/m8.rs`, `command_bar/chat_integration.rs`, `asset_transport.rs`
- `tezbar-core/src/assets.rs`, `extensions/asset_scope.rs`
- the core seam diffs `ba3a227`/`0c3dbea` (`chat.rs`, `config.rs`, `mod.rs`, `runtime/agent.rs`, `runtime/store.rs`)
- ui-model `ChatController` stop/unmount/persist/clear/delete/decide/attach
- `image_cache.rs`, `events.rs` Relay/leases
- `app.rs` quit and capture paths
- M8 diffs of `view.rs`, `task_indicator.rs`, `host_adapter.rs`, `core/lib.rs`, `extensions/mod.rs`

Read in part:

- `ai_chat/view.rs` (key routing, approval, composer, models, footer)
- `markdown.rs` (image tickets, `safe_link`/`safe_image`)
- `m8_tests.rs`, `chat_integration_tests.rs` fixtures
- `extension_worker` `HOST_CHANNELS`

Docs read: `AGENTS.md`, `parent-checkpoint`, `integration`, `parent-contracts`, `composition-contracts`, `composition-verification`, `composition-receipt`, `composition-case-map`, `native-m09/{asset-parent-api,parent-contracts}`, and the spec's §4.1, §4.2 and §5.7. Logs were excluded from code review, as instructed.

Not exhaustively re-read: `ai_chat/extensions.rs` (prompt/resource resolve uses core-provided items only), `tests.rs`/`core_loopback.rs` (byte-identical to the first gate per receipt), and the provider wire protocols (previously Opus-closed; no new caller found).

## Limits

- There was no real Windows build/race run, no GUI, IME, AX, GPU or RAM measurement, no live provider, no real HTTPS server and no clipboard. L4 and M2/M3 are source-traced, not probed.
- The probe edits exist only in `/tmp/tezbar-m8-opus-dd7-probe`. `/tmp/tezbar-m8-opus-dd7-archive` is pristine, and the live tree was never written.
- Artefacts left for inspection:
  - `/tmp/tezbar-m8-opus-app-probe.log`
  - `/tmp/tezbar-m8-opus-target` (≈98 GB logical; APFS clone, little real space). Delete it when no longer needed.
