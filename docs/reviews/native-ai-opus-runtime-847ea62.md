# Native AI runtime targeted closure re-review, scope B, at 847ea62: C-R1–C-R3 and touched Settings/close paths

- Reviewer: Claude Opus 5.5 (`claude-opus-5-5`). I did the review myself, with no subagents or delegates. I edited no repo source or docs, created no branch, checkout or worktree, and did not push, open a PR, launch the app, or use live models, credentials or MCP.
- Worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-rust-agent`.
- Pinned HEAD: `847ea628155238424808a888afa6e9307b461354`. HEAD was the same at the start and at the end, and `git status --porcelain` was empty both times.
- Tested source: `21d3e911a03bbea64ffb39324d6822ca343f9ad7`. `git diff --stat 21d3e91..847ea62` touches only `docs/NATIVE_AI_VERIFICATION.md` and `docs/reviews/NATIVE_AI_REVIEW_FIXES.md`.
- Baseline audit: `1623d47245390b4b32f30c0bed2b44a1d1a721e9`. The original report `docs/reviews/native-ai-opus-runtime-1623d47.md` is unchanged at HEAD; this file is separate.
- `git log --oneline 1623d47..HEAD`:
  ```
  847ea62 docs(ai): record eighteen repairs and exact-source validation
  21d3e91 fix(ai): keep stderr scrubbing within the bounded drain grace
  74c13ed fix(ai): serialize remounted settings and harden platform regressions
  96a55cf fix(ai): preserve unique revocation archives during reset rollback
  c4b8888 fix(settings): always destroy the hidden settings webview
  5dddcb4 fix(ai): repair eighteen native closure audit findings
  31bc3e2 docs(ai): retain completed Opus closure audits and P3 follow-ups
  ```
- What I read:
  - In full: `docs/reviews/native-ai-opus-runtime-1623d47.md`.
  - The 1623d47 inventory, the 18-repair table and the exact-source validation in `NATIVE_AI_REVIEW_FIXES.md`.
  - The complete `1623d47..21d3e91` diff for `src-tauri/src/lib.rs`, `src/renderer/SettingsView.tsx`, `settingsLifecycle.ts`, `tauri-bridge.ts`, `settingsLifecycle.bridge.test.ts`, the Settings tests, `AiExtensionsSection.tsx`, `src/shared/aiExtensions.ts`, `native_core/ai/mod.rs` and `runtime/agent.rs`.
  - The current production code for the Settings window lifecycle: `lib.rs` lines 2356-2431, 2599-2633, 2871-2902 and 2990-3003; the macOS/Windows placement helpers; `SettingsWindowApp` in `App.tsx`; the bridge event/backlog code; and Settings discovery, patch, persist, write, settle and Permissions.
- Search: I made no XERJ attempt, because the index is known to be incomplete. I used `rg` and direct paths only.
- Ledger claims were treated as claims. Each disposition below comes from the production call path plus my own probes.

## Verdict

**C-R1: closed. C-R2: closed. C-R3: closed on its reported trigger and on the assigned removal interleavings, but not closed for explicit-selection interleavings.**

- **New confirmed functional findings: 0 P0, 0 P1, 0 P2, 1 P3** (T-R1, below).
  - T-R1 was not introduced by `21d3e91`. It is present in the same form at `1623d47`; I ran the same probe on that archive. It comes from the N-R6 conditional-fallback design and is inside the assigned C-R3 axis ("explicit selection interleavings").
  - It is actionable and has a minimal fix.
- **No regression found** in the touched production paths:
  - the deletion fence and lifecycle mutex
  - per-owner chat
  - no effect replay
  - Settings → Permissions and native close
  - ownership of the external chat selection on the original triggers

  `App.tsx` and `AgentChatView.tsx` are unchanged in `1623d47..21d3e91`.
- **No-outstanding gate: NOT passed** because of T-R1.

## Per-ID closure

### C-R1: closed (source-proven against pinned Tauri 2.11.2; no OS window observed)

**Production order** (`src-tauri/src/lib.rs`, `finish_settings_close`, 2612-2624):
1. Label check.
2. Bump `settings_close.0` and set `settings_close.1 = true`. This invalidates every pending fallback generation.
3. `window.hide()?`
4. `let restored = restore_main_window(&app)`
5. `window.destroy()?`
6. Return `restored`.

**Why the launcher is now restored:**
- `tauri-runtime-wry-2.11.2/src/lib.rs`:
  - `hide()` (2263-2268) goes through `send_user_message`.
  - On the main thread, `send_user_message` (235-254) calls `handle_user_message` synchronously, which reaches `WindowMessage::Hide => window.set_visible(false)` (3495).
  - Off the main thread, Hide and the `IsVisible` getter (`window_getter!`, 2054-2055 and 3420) are queued on the same event-loop proxy in FIFO order.
  - `destroy()` (2279-2286) always posts `Destroy` last.
- `tao-0.35.3`:
  - macOS `set_visible(false)` calls `order_out_sync`, and `is_visible` returns `ns_window.isVisible()` (`macos/window.rs` 668-672 and 1075-1077).
  - Windows uses `execute_in_thread` for `set_visible`, which runs inline on the UI thread, and `is_visible` reads `IsWindowVisible` (`windows/window.rs` 164-172 and 664-666).
- So in both threading cases `focus_settings_window_if_visible` (2365-2388) reads `false` and does not hide the launcher. `restore_main_window` (2421-2432) then places, shows and focuses `main`.
- The sync command body runs inline in the IPC handler (`tauri-macros-2.6.2/src/command/wrapper.rs` 404-434, `body_blocking`), which matches the case analysis in the 1623d47 report.

**Destroy on a restore failure:**
- If `restore_main_window` fails, the Settings window is still destroyed before the error is returned.
- If `hide()` itself fails, the function returns before destroy. The window stays visible, the renderer shows the generic "Could not save Settings" alert, and the next close retries. That `hide()` fails on a live window is a hypothesis, not a reproduced trigger, and the failure is benign. I am not filing it.

**Stale fallback invalidation:**
- `CloseRequested` (2878-2898) sets `(gen+1, false)`, emits the generation, and spawns a 3 s timer.
- The timer returns unless the state is still exactly `(generation, false)`. The `if` condition's lock guard is dropped before the body runs, so `finish_settings_close` cannot deadlock.
- `ack_settings_close` (2627-2633) only sets the flag for the current generation, so a late acknowledgement of an older generation is ignored.
- `finish_settings_close` bumps the generation, so a timer from an earlier request can never destroy a window that is reopened later.

**Fallback thread:**
- The fallback calls `finish_settings_close` from a Tokio worker.
- The restore path is already called from async contexts elsewhere (2808, 3276).
- The macOS move uses `run_on_main_thread` (1614).
- The Windows `SetWindowPos` call is synchronous across threads, the CoreGraphics lookups are thread-safe, and the getters queue as described above.

**Limitation:** this is a source reading. I launched no real OS windows and observed no WKWebView or WebView2 events.

### C-R2: closed (real Settings plus real bridge fixtures; these are not OS event-delivery claims)

**(a) Optional discovery no longer poisons close:**
- `SettingsView.tsx` 775-780: the discovery write is `enqueueSettingsWrite(...).promise.catch(() => {})`, so the chain terminates.
- The repo test "a rejected discovery write must not silently block Settings close" passes (first and second close both resolve).
- My PROBE847-4 tested the harder ordering: the discovery `llm-config-set` is still pending when the native close arrives, and then rejects.
  - Result: `sets 1`, `ack_settings_close`, then `finish_settings_close`, with no alert.
  - Reason: `settleAiSaves` waits on the caught discovery chain before the bridge snapshots `configWrites`. The `aiEditRevision` bump (1610) stops any discovery that has not yet passed its revision check (729-730) from enqueuing later.

**(b) Close without a React consumer:**
- `tauri-bridge.ts` 140-187: the bridge owns a window-level `settings-close-requested` consumer, registered at init through the persistent backlog channel.
- It acknowledges the generation first, then loops:
  1. `drainSettingsOwners()`
  2. registered consumers
  3. tracked `configWrites`

  Only then does it call `finish_settings_close`.
- With no owners and no consumers it closes directly. This covers a failed lazy chunk and a render exception: the root unmounts, so the owner is retired and drained.
- If the webview is dead or the bridge never ran, there is no acknowledgement, and Rust destroys after 3 s (2889-2897).

**Acknowledged busy or error state is never destroyed:**
- The repo bridge test passes. A long write stays pending, each repeated request is acknowledged (7, then 8), and nothing finishes until the write resolves.
- My PROBE847-6 tested a retired owner whose drain fails:
  - Requests 4 and 5 are both acknowledged, and no finish is called.
  - The `#settings-close-error` alert is visible.
  - Its Retry button runs `retryRetiredSettingsOwners`, then closes.
  - The queue keeps order across a rejected predecessor (`[1,2]`).
- Once a request is acknowledged, Rust has no deadline, so a visible failure keeps the window open until the person acts.

**Remount and queued-get seam** (my 1623d47 residual note):
- `settingsLifecycle.ts`:
  - One queue per `window.tezbar` object.
  - Owners are registered in `useEffect` (`SettingsView.tsx` 1621-1635).
  - On unmount an owner is marked retired and drained. It stays registered on failure, so a failed save of a retired tab still blocks close and is visible.
- Effect cleanup order (owner, then the unmount flush) puts the pending debounce inside the retired settle.
- The repo parametrised test "retired Settings drains a queued get and preserves newer drafts" passes in all four combinations. It holds a `getLlmConfig` before `set`, remounts, closes, and covers:
  - success and failure
  - the visible retry
  - a newer draft superseding without replay (the save count is 2, and the final key is `newer-draft`)

**Supersession:**
- `hasNewerSettingsSave` compares the per-provider successful revision with the failed revision. Revisions grow monotonically on the shared serialized queue.
- `drainSettingsOwners` drains the failed owners again after the newer owners have flushed. A stale draft is replayed only through the explicit Retry button.

**Permissions:**
- Open Permissions (`SettingsView.tsx` 2839-2842) still settles before navigating, and is gated by `.catch`.
- The `SettingsWindowApp` no-op consumer (`App.tsx` 261-267) is unchanged and is now redundant with the bridge default. That is harmless.

**Escape/back** still settles and then calls `closeCurrentWindow`. That re-enters the full window drain.

**Residual behaviour, judged acceptable (not findings):**
- **One-time block from an untracked write.** An immediate non-AI write that rejects during the drain (memory toggle, retention, and so on) blocks that one close and shows the alert. The next close succeeds (PROBE847-5).
  - The `Promise.all(configWrites)` part existed before this change.
  - The write's own "Could not save" message is shown.
  - This is visible and recoverable, so it is not a finding.
- **Destruction without a drain.** Rust destroys without draining in two cases:
  - the renderer is alive but its JS is blocked for more than 3 s at the moment of close
  - a close arrives in the first instant after page load, before `listen()` has registered (the event is lost, and edits made within those 3 s could be dropped)

  Both are the stated design tradeoff for "unacknowledged". I have no trigger observed in practice. Unproven hypothesis.
- **No escape from a permanently failing backend.** If the backend keeps failing (for example, the disk is full), an acknowledged failing save makes Settings closable only by quitting the app. This is intended ("never destroy an acknowledged error state"), and it is visible.

### C-R3: closed on its trigger and on the removal interleavings; NOT closed for explicit-selection interleavings (T-R1)

**What is closed:**
- `pendingAiOverrides` now accumulates `removedModels` (`SettingsView.tsx` 1410-1417).
- `writeAiPatch` (1442-1458) runs per provider. It falls back only when the persisted selection is one of the removed IDs, and it picks the first non-removed entry of the final `patch.providerModels` list.
- Repo tests pass:
  - A then C gives B.
  - A then the interim fallback B gives C.
  - Both assert `model` and `taskModelOverrides.chat`.
- My PROBE847-3 passes:
  - With an external selection C, removing the unselected D keeps C.
  - Removing A then C quickly falls back to B, for `model` and the chat override too.

**Other providers:**
- `patch.providerModels` is the full map, and the loop runs per provider.
- Removing provider X never touches provider Y's selection.
- This was checked in source only, with no probe.

#### T-R1 (P3, probe-confirmed, actionable): an explicit model choice made in the same debounce as a removal is reverted or ignored

**Where:**
- `SettingsView.tsx` 1442-1458, the removal fallback in `writeAiPatch`.
- It decides only from `current.providerSelectedModels`, the persisted selection. It ignores `patch.providerSelectedModels`, the explicit choice carried in the same patch.
- The explicit choice comes from one of two places:
  - the model-row click: `persistAiSettings(true, { model })` (2592)
  - Add model: `persistAiSettings(false, { model })` (1171)

  Either way it is merged with the pending `removedModels` (1410-1417).

**Trigger 1, revert:**
- Models are `[A (selected), B, C, D]`.
- Click Remove on A, then click model D within 300 ms.
- The immediate write sees persisted A ∈ removed and replaces the explicit D with B.
- PROBE847-1:
  ```
  UI fixture-d CONFIG {"models":["fixture-b","fixture-c","fixture-d"],"selected":"fixture-b","model":"fixture-b","chat":"fixture-b"}
  ```

**Trigger 2, removed model persisted:**
- Add model Z, which selects Z. Then click Remove on Z within 300 ms.
- The persisted selection is A, which is not in the removed list, so no fallback runs, and the stale explicit Z is written.
- PROBE847-2:
  ```
  UI fixture-a CONFIG {"models":["fixture-a","fixture-b","fixture-c","fixture-d"],"selected":"fixture-z","model":"fixture-z","chat":"fixture-z"}
  ```
- So the saved chat model is a model that is not in the list.

**Impact:**
- Settings shows one model while the persisted `providerSelectedModels`, the active `model` and `taskModelOverrides.chat` point at another. That is the C-R3 class of mismatch.
- In trigger 2, chat requests a model the person removed, and the provider may serve it silently.
- Only one write is affected. The next Settings or picker selection repairs it.

**Provenance:**
- The same probes fail identically on a `git archive 1623d47`; see `/private/tmp/tezbar-runtime-probe-847ea62-base1623.log`.
- So this is a pre-existing gap from the N-R6 conditional fallback, not a regression from the C-R3 repair.
- Before N-R6 every patch carried Settings' local selection, so both triggers probably saved the right model. That is inferred from the 1623d47 report and not probed.

**Minimal fix** (renderer only, in `writeAiPatch`):
- For each provider, take the effective target: `desired = patch.providerSelectedModels?.[provider] ?? selected`.
- Fall back only when `desired` is a removed ID. When an explicit `desired` survives the removal, keep it.
- Compute the result into `next` or a local copy instead of assigning to the closed-over `patch`. That also stops retry closures from carrying an earlier fallback (see the smell below).
- Add PROBE847-1 and PROBE847-2 as permanent regressions.

**Related smell (judgment, not a reachable failure):**
- `writeAiPatch` mutates the closed-over `patch.providerSelectedModels`.
- A Retry of a failed removal (`retry: () => writeAiPatch(patch, scope)`) therefore replays the first attempt's fallback, even if the persisted selection changed since then.
- I found no UI path that changes the selection while Settings is visible: the launcher is mutually exclusive with Settings (`focus_settings_window_if_visible`), and a newer Settings save of that provider prunes the old failure.
- The minimal fix above removes it anyway.

## Touched-path regression sweep (no findings)

- **`lib.rs` close commands and events:**
  - `ack_settings_close` is registered (3066) and label-checked.
  - `window.emit` broadcasts the generation. Only the `settings` label subscribes (`tauri-bridge.ts` 183), and a main-window acknowledgement is rejected.
  - The `close_current_window` route for `settings` is unchanged (`window.close()` → `CloseRequested`).
- **Bridge:**
  - `onWindowCloseRequested` now only registers a consumer, and the bridge owns the loop. A thrown consumer goes to the same visible catch.
  - `closing` deduplicates concurrent drains, and repeated requests still acknowledge.
  - The alert is removed on a successful close.
- **`settingsLifecycle.ts`:**
  - `lifecycle()` is keyed by `window.tezbar`. The bridge calls it at event time, after `window.tezbar = tezbar` (618), so the key always exists.
  - Tests that replace `window.tezbar` stay consistent, because both sides read it at call time.
- **`SettingsView` write path:**
  - Discovery, manual writes and retired writes share one serialized queue. Provider records are still merged at execution time (1436-1441).
  - Provider-removal fallback, explicit-selection chat overrides and `patch.provider` handling are unchanged from 1623d47.
  - No stale `removedModel` references remain (`rg`).
- **Native AI touched files in my seam:**
  - `ai/mod.rs`: rustfmt of the lifecycle-mutex block, which is still taken for delete, clear and delete-turn. The redaction regexes are now `LazyLock` with identical patterns. S3/S10 belong to the other reviewer, and I saw no regression in the seam.
  - `runtime/agent.rs`: rustfmt, plus `validate_images_budget(..., route.request_budget)`. That is N-W1, owned by the wire reviewer. Both cancel checks around OCR and validation are still in place before the user save.
  - Deletion fence, per-owner chat, no effect replay and external chat-selection ownership: no change in `App.tsx` or `AgentChatView.tsx`, and the N-R1 and N-R6 paths are intact.
- **`AiExtensionsSection.tsx` and `src/shared/aiExtensions.ts`:** copy changes and a `missingCredentials` flag (S4, other reviewer). The UI seam is display-only.

## Probes and evidence

- **Snapshot:** `/private/tmp/tezbar-runtime-probe-847ea62`, a `git archive 847ea62` with `node_modules` symlinked. Reviewer probes were added only there:
  - PROBE847-1 to PROBE847-4, in its `src/renderer/SettingsView.knowledge.test.ts`
  - PROBE847-5 and PROBE847-6, in `src/renderer/probe847.bridge.test.ts`
- **Baseline snapshot:** `/private/tmp/tezbar-runtime-probe-847ea62-base1623`, a `git archive 1623d47` with probes 1-3.
- **Logs:**
  - `/private/tmp/tezbar-runtime-probe-847ea62-settings.log`: PROBE847-1 and -2 FAIL (T-R1); -3 and -4 pass.
  - `/private/tmp/tezbar-runtime-probe-847ea62-base1623.log`: PROBE847-1 and -2 FAIL identically at 1623d47; -3 passes.
  - `/private/tmp/tezbar-runtime-probe-847ea62-bridge.log`: PROBE847-5 and -6 pass.
  - `/private/tmp/tezbar-runtime-probe-847ea62-existing.log`: `settingsLifecycle.bridge.test.ts`, `SettingsView.knowledge.test.ts` and `AiExtensionsSection.test.ts`. 118 passed and 2 failed, the two failures being exactly my T-R1 probes. Every repo test for C-R1–C-R3, Permissions, early close, pending writes and manual errors passed.
- **No cargo build.** The native claims are source-verified, and the parent's exact-source macOS, Windows, MSRV and lifecycle gates were accepted as reported. I reran no full suite.

## Coverage and limitations

- No desktop launch. Native window visibility, close ordering, the 3 s fallback, and WKWebView/WebView2 event delivery and crash behaviour come from reading the pinned Tauri 2.11.2, wry-runtime 2.11.2, tao 0.35.3 and tauri-macros 2.6.2 sources, not from runtime observation.
- The renderer fixtures mock `invoke` and `listen`. They prove our side of the contract, not live interop.
- The other providers' removal interleaving was checked in source only.
- Not reviewed here, because other reviewers own them:
  - the S8 MCP clients
  - the wire image budgets, apart from the call-site seam
  - S1–S10 internals
- Not in scope: pre-existing Windows OCR and semantic UI Automation.

## Final counts

- **C-R1:** closed (source-proven).
- **C-R2:** closed (fixture-proven; the residual tradeoffs above are judgments).
- **C-R3:** closed on the reported trigger and the removal interleavings. Not closed for explicit-selection interleavings: **T-R1, P3, actionable, probe-confirmed, pre-existing since N-R6.**
- **New regressions from the 18-repair diff in my scope:** 0.
- **Outstanding actionable:** 1 (T-R1). The gate is not passed until T-R1 is fixed and targeted re-check passes.
