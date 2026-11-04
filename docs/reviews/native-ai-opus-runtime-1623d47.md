# Native AI runtime closure review, scope B, at 1623d47: runtime, history, tools, approvals and renderer

- Reviewer: Claude Opus 5.5 (`claude-opus-5-5`). I did the review myself, with no subagents and no delegates. I edited no source or docs, created no branch, checkout or worktree, and pushed nothing.
- Worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-rust-agent`.
- Pinned HEAD: `1623d47245390b4b32f30c0bed2b44a1d1a721e9`. `git status --short` was empty at the start and at the end, and HEAD did not change.
- Sources:
  - Final renderer source: `ae2fe00`.
  - Native implementation: `8f70d2e`. `git diff 8f70d2e..ae2fe00` touches no Rust; it changes `App.tsx`, `SettingsView.tsx`, one test and docs.
  - `ae2fe00..1623d47` is docs only (`NATIVE_AI_VERIFICATION.md`, `NATIVE_AI_REVIEW_FIXES.md`).
- Task base: `v0.3.0-beta.7` = `e246752149dbbede5e44720a0e35a3eacf2d66a9`.
- Commits after the last full runtime audit, `git log a28c564..HEAD --oneline`:
  - `1623d47` docs(ai): record Settings window close follow-up verification
  - `ae2fe00` fix(ai): keep Settings close available on Permissions
  - `c282d3d` docs(ai): record six runtime repairs and exact-source validation
  - `8f70d2e` fix(ai): fence session deletion and durable settings closure
  - `5271fd7` docs: retain full Opus runtime closure audit
  - `c486614` docs(ai): record re-review repairs and dual-platform verification
- Earlier commits: `a28c564`, `9fe0576`, `118cb7b`, `fcabc38`, `e672ecb`, `9b76b95`, `2e29cf1`, `32ba9c4`, plus the original series `42e2ccb` … `b5ba21c`, 26 commits in total from the base.
- Inputs read in full:
  - `docs/reviews/native-ai-opus-runtime-32ba9c4.md` (R1–R20, the P3 list and the coordinator decision)
  - `docs/reviews/native-ai-runtime-118cb7b-partial.md` (U1–U3)
  - `docs/reviews/native-ai-opus-runtime-a28c564.md` (N-R1–N-R6)
  - the runtime and N-R sections of `docs/reviews/NATIVE_AI_REVIEW_FIXES.md`, including the `ae2fe00` coordinator follow-up

  I treated ledger claims as claims. Every disposition below comes from the production call path at HEAD, plus my own probes where stated.
- Earlier rate-limit failures were not used as evidence of anything.

## Verdict

**Not ready. No-outstanding gate: FAILED.**

**New findings: 0 P0, 0 P1, 0 P2, 3 P3.** Two of the three are confirmed by probes I ran. The third is confirmed by reading the pinned Tauri 2.11.2 runtime source.

- **N-R1–N-R6.** Each is closed on its original trigger.
- **The repairs for N-R2 and N-R6 introduce three new functional P3 regressions.**
  - **C-R1.** Closing the Settings window no longer brings the launcher back.
  - **C-R2.** Settings close can be blocked silently and permanently.
  - **C-R3.** Removing two models within the autosave debounce leaves a removed model selected for chat.
- **R1–R20, U1–U3 and the runtime P3 list** keep their closed dispositions in the current source. Nothing in `a28c564..ae2fe00` regresses them.

## New findings

### C-R1 (P3, confirmed from source). Closing Settings leaves no window visible: the launcher is no longer restored

**Where:**
- `src-tauri/src/lib.rs:2611-2619`, `finish_settings_close`: `window.destroy()`, then `restore_main_window(&app)`.
- `lib.rs:2420-2423`, `restore_main_window`: first calls `focus_settings_window_if_visible`.
- `lib.rs:2364-2386`, `focus_settings_window_if_visible`: returns `true` and *hides* the launcher (`hide_main_window_for_settings`) while Settings is still visible.

**Trigger:** Any close of the Settings window. That includes the title-bar close, Cmd+W, the Escape/back path through `closeCurrentWindow`, and Browse Store. After `8f70d2e` every route ends in `finish_settings_close`.

**Why it fails:**
- Tauri 2.11.2 (`Cargo.lock`) and `tauri-runtime-wry` 2.11.2: `destroy()` does not tear the window down synchronously. It posts `WindowMessage::Destroy` through the event-loop proxy (`tauri-runtime-wry-2.11.2/src/lib.rs:2279-2286`; the window is removed in `on_window_close` when the loop processes it, `:4378-4380`). When `restore_main_window` runs, the Settings window still exists and is still visible. There are two cases:
  1. **The command runs on the main thread.** A synchronous `#[tauri::command]` executes inline in the IPC protocol handler (`tauri-macros-2.6.2/src/command/wrapper.rs:404-434`). Getters on the main thread are answered synchronously (`send_user_message`, `:235-248`). `is_visible()` therefore returns `true`, and `focus_settings_window_if_visible` hides the launcher and shows and focuses the window that is about to be destroyed. It returns `Ok(true)`, so the launcher is never shown.
  2. **The command runs on another thread.** `IsVisible` is queued after `Destroy`. The window is gone by then, so the sender is dropped (`:3372-3381`), the getter returns `FailedToReceiveMessage`, `?` propagates, and the launcher is again not restored.
- This is a regression from baseline (`e246752` `close_current_window`, and the old `CloseRequested` handler). Baseline called `window.hide()` before `restore_main_window`, so the visibility check was false and the launcher came back.
- The ledger claims this behaviour ("Rust uses `destroy` … and restores the launcher"). The production order does not deliver it.

**Impact:**
- After closing Settings, no Tezbar window is visible. The person has to press the global hotkey to get the launcher back.
- Every Settings close is affected.

**Proof level:** Source reading of the pinned dependency only. I did not launch the app. The renderer test mocks `invoke('finish_settings_close')` and cannot see this.

**Fix:**
- Call `window.hide()` before `window.destroy()` in `finish_settings_close`, as baseline did.
- Alternatively, restore the launcher from the Settings window's `WindowEvent::Destroyed` handler.

### C-R2 (P3, probe-confirmed for trigger a). Settings close can become silently and permanently blocked

**Where:**
- `SettingsView.tsx:770-773`: model discovery appends `window.tezbar.setLlmConfig(patch)` to `aiWrites.current` without a terminal `.catch`.
- `SettingsView.tsx:1569-1578`, `settleAiSaves`: `await writes` rethrows that rejection.
- `tauri-bridge.ts:151-171`: on rejection, the bridge logs "Close was blocked" and never calls `finish_settings_close`.
- `lib.rs:2864-2869`: Rust always calls `prevent_close()`.

The same settle gates Escape/back (`SettingsView.tsx:1540-1545`) and Open Permissions (`:2781-2788`). Both swallow the rejection with `.catch(() => {})`.

**Trigger (a), confirmed:**
- Settings mounts, and model discovery runs automatically for the selected provider with no user edit.
- The discovery `llm-config-set` IPC rejects, for example on a native config write or extension-data read error.
- From then on, every close request, Escape and Open Permissions rejects with that same stale error.
- No message is shown: the discovery catch is silent, and `failedAiProviders` is not set.
- The state persists until the person happens to make another AI edit that replaces `aiWrites.current`.

**Trigger (b), plausible, from source:**
- `prevent_close` is unconditional. Close works only while a React consumer is registered: `SettingsView`, or `SettingsWindowApp`'s Permissions effect (`App.tsx:261-267`).
- `src/renderer` has no error boundary (`rg` finds no `componentDidCatch`, `getDerivedStateFromError` or `ErrorBoundary` outside tests).
- So any render-time exception in Settings or Permissions, or a failed lazy chunk load, unmounts the root. The close request then goes into the bridge backlog with no consumer.
- The window cannot be closed except by quitting the app. Before this branch, close always worked.
- A WebView content-process crash would probably behave the same way. Wry's behaviour on termination was not verified.

**Probe:**
- Snapshot: `/private/tmp/tezbar-runtime-probe-1623d47`, a `git archive 1623d47` with `node_modules` symlinked.
- Test: `PROBE2 a rejected discovery write must not silently block Settings close`, in the snapshot's `src/renderer/SettingsView.knowledge.test.ts`. It uses the real `SettingsView`, mocks IPC, captures the registered close callback, and makes `setLlmConfig` reject.
- Log: `/private/tmp/tezbar-runtime-probe-1623d47-settings.log`.
  ```
  PROBE2-SAVES 1 HANDLER true
  PROBE2-CLOSE rejected:Error: fixture IPC failure VISIBLE-ERROR false
  PROBE2-SECOND-CLOSE rejected:Error: fixture IPC failure
  ```

**Impact:**
- The Settings window does not respond to its close button, Cmd+W or Escape, and shows no reason.
- This is the opposite of the intended N-R2 behaviour, which is "failed writes keep a *visible* error and a recoverable draft".

**Fix:**
1. Terminate the discovery chain: `aiWrites.current = aiWrites.current.catch(() => {}).then(() => setLlmConfig(patch)).catch(() => {})`. Discovery is optional; it must not gate close and owns no draft.
2. Make the native close recoverable when no renderer consumer acknowledges. For example:
   - In the bridge, register a default settings-window consumer that drains `configWrites` and acknowledges whenever no component handler is registered.
   - And/or, in Rust, destroy on a repeated `CloseRequested` after a bounded wait with no acknowledgement.

   Neither makes a *visible* failed AI save closable without the person's action, so neither is an unconditional bypass.

### C-R3 (P3, probe-confirmed). Removing the selected model and then another within 300 ms leaves the removed model selected for chat

**Where:**
- `SettingsView.tsx:1167-1179`, `removeAiModel`: debounced; the scope is `{ removedModel }`.
- `SettingsView.tsx:1403`: `pendingAiOverrides.current = { ...pending, ...overrides }`. A second removal replaces `removedModel`.
- `SettingsView.tsx:1428-1434`, `writeAiPatch`: falls back only when the persisted selection equals the *last* removed ID.

**Trigger:**
- Models are `[A (selected), B, C]`.
- Click Remove on A, then Remove on C, within the 300 ms debounce.

**Failure:**
- The saved config has `providerModels.openai = [B]`.
- The selection still points at the removed A: `providerSelectedModels.openai`, `model` and `taskModelOverrides.chat` are all A.
- Settings shows B as selected, so its UI and the persisted chat choice diverge.
- This is a regression from the N-R6 change. Before `8f70d2e`, every patch carried Settings' local selection.

**Probe:**
- Test: `PROBE removing the selected model then another within the debounce falls back`, in the same snapshot. It uses the real `SettingsView` and a config-merging `setLlmConfig` fake.
- Output:
  ```
  PROBE-CONFIG {"models":["fixture-b"],"selected":{"openai":"fixture-a"},"model":"fixture-a","chat":{"chat":"fixture-a"}}
  ```
  It expected `fixture-b`. The test FAILED.

**Impact:**
- Chat keeps requesting a model the person explicitly removed.
- The picker shows a model that is not in the list. The provider may still serve the ID, so this can be silent.

**Fix:**
- Accumulate removals as a list, `removedModels: [...]`, and apply the conditional fallback if the current selection is any removed ID.
- Alternatively, flush the pending debounce before enqueueing a new `removedModel` scope.

## Per-ID closure: N-R1–N-R6 (a28c564 findings)

| ID | Verdict | Production path and evidence |
| --- | --- | --- |
| **N-R1** (P2) delete during send/retry preparation | **Closed** | See the N-R1 detail below this table. |
| **N-R2** (P3) pending autosave lost on real close | **Closed on the original trigger. New regressions C-R1 and C-R2 are open.** | See the N-R2 detail below this table. |
| **N-R3** (P3) shell tail lost | **Closed** | See the N-R3 detail below this table. |
| **N-R4** (P3) review request never cleared, repeated scroll | **Closed** | See the N-R4 detail below this table. |
| **N-R5** (P3) reopened non-vision retry loses OCR | **Closed** | See the N-R5 detail below this table. |
| **N-R6** (P3) Settings reverts the chat picker's choice | **Closed on the original trigger. New regression C-R3 is open.** | See the N-R6 detail below this table. |
| **`ae2fe00` coordinator follow-up** (Permissions unmounts the Settings close hook) | **Closed on its trigger.** C-R2 trigger (b) is the general form of the same hazard. | See the follow-up detail below this table. |

**N-R1 detail:**
- Renderer, `AgentChatView.tsx`:
  - `send` refuses when `owner.deleting` (`:481`). It re-checks `deleting` and `submission` after `modelSaveRef` (`:499`), after `profileSaveRef` (`:501`), after `getLlmConfig` (`:503`) and after `persistTurn` (`:544`), before `sendMessage`.
  - `retry` does the same (`:574`, `:582`, `:584`, `:586`), and now also awaits `profileSaveRef`.
  - `deleteConversation` sets `deleting`, clears `submission` and `preparing`, and notifies before any await (`:900-906`). It waits for in-flight writes. On failure it clears `deleting` and re-persists unsaved turns (`:915-918`). The owner is then fresh for a new submission, because `submission` is null.
- Native:
  - `launch` holds the async `session_lifecycle` mutex (`mod.rs:393`) across:
    - the persistent tombstone lookup `chat::is_deleted` (`:394-398`, `chat.rs:432-447`), which survives restart because it is SQLite
    - the retry pre-run write, moved in from `agent:run` (`:399-417`)
    - generation capture (`:418-425`)
    - run and session registration (`:433-450`)
  - `chat:delete`, `chat:clear` and `chat:delete-turn` take the same mutex (`:845-847`) across cancellation, the SQLite tombstone and transcript deletion (`chat.rs:261-301`, `:313-330`).
  - So the two orders are:
    - **delete → launch:** refused, `{"ok":false,"error":"Conversation was deleted"}`, before any provider or file work.
    - **launch → delete:** the run is registered first. Delete cancels it and bumps the revisions. The spawned task checks `cancel` before canonicalize (`:461`). `agent.rs:155` and `:171` check it again before OCR and before the first user save. `save_at` compares generations under the global store `LOCK` (`store.rs:22-29`).
  - No `std` mutex is held across an await.
  - Other `launch` callers (`query`, `chat:run`) carry no `sessionId`.
- I re-derived this from source. The parent's logs show the named native and renderer regressions passing. I did not rerun my a28 probes, because the code they hit is replaced.

**N-R2 detail:**
- The original trigger is closed:
  - Rust `CloseRequested` on `settings` calls `prevent_close` and emits `settings-close-requested` (`lib.rs:2864-2869`).
  - `close_current_window` for `settings` now calls only `window.close()`, which re-enters `CloseRequested` (Tauri `Window::close` docs, `tauri-2.11.2/src/window/mod.rs:1781`).
  - The bridge subscribes eagerly for the `settings` label (`tauri-bridge.ts:140-141`). The channel is persistent, so an early request is backlogged and replayed when a consumer subscribes (`:101-126`). That covers lazy Settings load and the `key={settingsTab}` remount gap.
  - The handler loops: `beforeClose`, then the tracked `llm-config-set` promises (`:151-171`, `:259-267`), then invokes the label-checked `finish_settings_close`. On error the window stays open.
  - `settleAiSaves` flushes the debounce and waits for `aiWrites` until the queue is stable, then throws if `failedAiProviders` is non-empty. On that failure the draft and the error message stay.
  - Escape, Open Permissions (`ae2fe00`) and Browse Store all go through the same settle and close.
  - `window.emit` broadcasts to every webview (`tauri-2.11.2/src/lib.rs:934`). A main-window listener could only call `finish_settings_close` from the wrong label, which is rejected. The main window never mounts `SettingsView` in practice, because `App.tsx:545-554` routes `settings` to the native window.
- **New regressions:** C-R1 (the launcher is not restored after destroy) and C-R2 (close can be blocked silently and permanently).

**N-R3 detail:**
- `CaptureBuffer` (`process.rs:266-345`) keeps a head of (limit−128)/3 bytes plus a ring tail, counts the total, and lives outside the cancellable read future. The timeout drain (`:388-395`) therefore continues the same ring.
- Partial UTF-8 at the cut points is trimmed and counted as omitted. Invalid bytes become one `?` each, so the budget cannot grow.
- The budget is 48 KiB stdout plus 12 KiB stderr, below the whole-result cap `RESULT_BYTES−100` (`agent.rs:268`), so the tails survive.
- Exit-status and timeout semantics are unchanged.
- The stage UI preview still shows only the first 8000 characters (`agent.rs:266`). That is display only; the model gets the full bounded text. Not a defect.

**N-R4 detail:**
- `review-project` queues `(cwd, uuid nonce)` (`ext/mod.rs:1212-1218`). `list` returns `projectReviewId` without consuming it (`:1233-1237`).
- `trust-project` (after a successful trust) and the new `dismiss-project-review` clear the request only when the nonce matches (`finish_project_review`, `:2185-2195`). A stale acknowledgement therefore cannot clear a newer request.
- In the UI, `AiExtensionsSection.tsx:69-83` scrolls once per nonce, and the load revision drops stale reloads. Keep disabled or Dismiss passes the nonce, and trust passes the nonce as the third argument.
- Viewing grants no trust.
- Only one Settings window exists. A reopened window scrolls again only while the request is still pending, which is intended.

**N-R5 detail:**
- `agent.rs:139-153` reuses the stored canonical user turn before OCR and before image validation.
- `effective_images` comes from that effective user turn (`:155-158`). OCR runs on those images when the model has no vision (`:159-166`), and the text goes only into the disposable view (`:252`).
- The saved user turn is the canonical one (`:172-184`). OCR text is never persisted.
- A missing blob loads as a placeholder, so no OCR runs.
- OCR is now the in-process internal route `core:platform:ocr-images`:
  - `platform.rs:369-372` → `legacy::ocr_images_local`.
  - The `core:` prefix is rejected for renderer requests (`native_core/mod.rs:108-111`).
  - The router awaits inline (`native_core/mod.rs:61-69`), so `select!` on cancel drops the future. The helper uses `kill_on_drop` (`process.rs:50`), and the temporary directory is dropped with the future.
- Non-macOS still returns empty OCR text, so the run fails with "cannot read images". This is unchanged and documented.

**N-R6 detail:**
- `buildAiProviderPatch` (`SettingsView.tsx:1080-1092`) no longer emits `provider`, `model`, `providerSelectedModels` or task overrides unless the model or activation is explicit.
- `writeAiPatch` (`:1417-1482`) merges `providerConfigs` with the current config at execution time, so only the edited provider record changes.
- An explicit selection merges current task overrides.
- Provider removal falls back conditionally on the current config.
- Rediscovery is fenced by `aiEditRevision` and merges with the persisted config (`:713-773`).
- I checked the chat picker's `selectModel` (`AgentChatView.tsx:674-694`): it writes only selection fields plus a normalisation of the same model list. A later Settings write therefore does not revert it.
- **New regression:** C-R3.

**`ae2fe00` coordinator follow-up detail:**
- Open Permissions settles before navigating, and stays on Settings with a visible `Message` on failure (`SettingsView.tsx:2781-2795`).
- `SettingsWindowApp` registers a close consumer while Permissions is mounted (`App.tsx:261-267`). The bridge still drains the tracked config writes.
- The real-App, real-bridge test covers this with a mocked `invoke`.

## Per-ID closure: R1–R20 (32ba9c4 findings), re-checked at HEAD

No R item changed disposition. `a28c564..ae2fe00` touches only `send`, `retry` and `deleteConversation` in `AgentChatView`, the OCR helper in `legacy.rs`, project review in `ext/mod.rs`, shell capture, `launch`, the chat handlers, and Settings persistence and close.

| ID | Verdict | Current evidence |
| --- | --- | --- |
| R1 shell output | Closed. The N-R3 residual is closed too. | `native_tools.rs` → `process::shell_capture` with both streams, the exit status and the timeout drain. The result is bounded at `agent.rs:268`. |
| R2 reopened retry deletes the image | Closed | Stored canonical user reuse is at `agent.rs:146-153`, and a stored turn is never downgraded. The N-R5 residual is closed. |
| R3 history images on non-vision | Closed | `image_view(&messages, vision)` runs every turn (`agent.rs:252`), and canonical history is unchanged. |
| R4 mode change approves | Closed | Both `ApprovalModePicker` instances (`AgentChatView.tsx:1218-1227`, `:1238-1247`) only call `setLlmConfig({agentApprovalMode})`. |
| R5 global Enter approves | Closed | The window key handler handles only `.` and `S` (`:800-818`). Allow, Deny and Always are focus-local buttons (`:1261-1283`). |
| R6 project Review dead end | Closed. The N-R4 residual is closed. | IPC queue with a nonce (see N-R4). |
| R7 extension providers persisted | Closed | `llm-config-set` → `filter_extension_patch` (`mod.rs:633`, `ext/contributions.rs:7-43`). |
| R8 empty tool output | Closed | `(no output)` mapping in `agent.rs`. The wire part belongs to the wire reviewer. |
| R9 image bytes in the text budget | Closed | Recursive image-free sizing, and `bound_images` inside the loop (`agent.rs:249-250`). |
| R10 delete during a run | Closed. The N-R1 residual is closed. | Generations, cancellation, the lifecycle mutex and the persistent tombstone. |
| R11 retry forgets effects | Closed | The retry pre-run marker, now under the lifecycle mutex (`mod.rs:399-417`), plus re-inclusion of prior calls (`agent.rs:187-200`). |
| R12 missing profile widens tools | Closed | `ext/mod.rs:2107` fails closed. |
| R13 useless Always allow | Closed | `canRemember` (`legacy.rs:110`) and native rejection (`:144-147`). The renderer hides the button (`AgentChatView.tsx:1273`). |
| R14 no Protocol control | Closed | Unchanged since a28. Not in this diff. |
| R15 Pi onboarding copy | Closed | Unchanged. |
| R16 Esc in the menu leaves chat | Closed | `ComposerExtensions.tsx:103`, `:176` and `App.tsx:643` (`!e.defaultPrevented`). |
| R17 tool review | Closed for reachability. The standards reviewer owns the internals. | Unchanged. |
| R18 per-provider image limits | Closed for reachability. The wire reviewer owns the internals. | `limits::validate_images` now runs on the *effective* user turn (`agent.rs:167-170`). |
| R19 key crossing | Closed | The legacy-only fallback is unchanged. N-R6 no longer mirrors top-level credentials. |
| R20 mode only on the card | Closed | The footer picker is always shown (`AgentChatView.tsx:1217-1228`). |

**Runtime P3 list (P3-01 … P3-12) and the OpenCode coordinator decision:** dispositions unchanged from my a28 report, because the code is unchanged.
- P3-01 pre-start retry: the marker is written under the lifecycle mutex before preflight.
- P3-02 expired approval card: closed.
- P3-04 removed folder: closed.
- P3-06, P3-07 and P3-09: closed.
- P3-08: closed in source; no visual QA.
- P3-10: accepted alias.
- P3-11 and P3-12: closed.
- OpenCode: there is no Pi, Node, npm, Bun or OpenCode spawn in non-test `native_core/ai`.
- P3-03 and P3-05 belong to the wire reviewer and were not re-verified.

## U1–U3 (118cb7b partial notes)

| ID | Verdict | Evidence |
| --- | --- | --- |
| U1 delete of a running conversation | **Closed**, including the N-R1 preparation variant and clear-all. | `deleting` is set before any await. Writes are drained. `persistTurn` is suppressed while deleting (`AgentChatView.tsx:246-250`). Failed-delete recovery restores the owner. `chat:clear` tombstones every SQLite session plus the running session IDs under the lifecycle mutex. No clear-all UI consumer exists. |
| U2 extension-owned provider controls | **Closed** | Unchanged since a28. Discovery still skips extension-owned providers (`SettingsView.tsx:726`). |
| U3 autosave ordering | **Closed** for ordering and real close (N-R2). | The latest-render patch ref, the serialized `aiWrites` queue and the close settle loop. C-R2 and C-R3 are new and separate. |

## Regression sweep of a28c564..ae2fe00

- **No regressions found in:**
  - the transport
  - per-owner isolation
  - approval UI
  - capture and boot nonces
  - the effect ledger
  - image GC
- **Lifecycle mutex:**
  - It is held only across short SQLite and file work.
  - A duplicate `runId` returns an error after the retry marker is already written. The marker is harmless historical context.
- **`settleAiSaves` side effect:** it bumps `aiEditRevision`, so a discovery that is in flight when the person opens Permissions is dropped. `SettingsView` unmounts anyway. Not a defect.
- **Narrow residual note, not a finding:**
  - Trigger: a cross-window `storage` event (the launcher opening a different Settings tab while Settings is open) remounts `SettingsView` through `key={settingsTab}`.
  - The old instance's unmount flush runs on its own `aiWrites` chain. The bridge tracks that write only once `setLlmConfig` is actually invoked, after its `getLlmConfig` round-trip.
  - A close within that window of a few milliseconds could destroy the webview before the write starts. A failure there also has no visible owner.
  - Same class as C-R2. I did not count it separately.

## Interop and behaviour not proven

- **No desktop launch.**
  - Native window close and destroy ordering (C-R1), WKWebView and WebView2 event delivery, and content-process crash behaviour (C-R2 trigger b) come from reading the Tauri and wry source pinned in `Cargo.lock`. They were not observed at runtime.
  - The repo's renderer close tests mock `invoke` and `listen`, and do not establish desktop event delivery.
- **Thread of synchronous commands.** That synchronous Tauri commands run on the main thread is my reading of the pinned source. I did not re-fetch the Tauri docs page. C-R1 fails in either threading case, as shown above.
- **No live provider, MCP, credential or profile use.** Wire codecs, MCP, trust and secrets internals belong to the other reviewers.
- **Windows OCR remains unsupported:** the helper is macOS-only, and the Windows fixtures inject OCR text.

## Coverage

- **Read in full or by complete diff:**
  - `git diff a28c564..ae2fe00` for `lib.rs`, `chat.rs`, `mod.rs`, `agent.rs`, `legacy.rs`, `platform.rs`, `process.rs`, `ext/mod.rs`, `AgentChatView.tsx`, `App.tsx`, `tauri-bridge.ts`, `AiExtensionsSection.tsx`, the shared types, and the `8f70d2e..ae2fe00` part of `SettingsView`.
  - `launch` and `agent:run` in full (`mod.rs:375-520`, `:720-850`).
  - `native_run_history` (`agent.rs:100-275`).
  - `store.rs` revisions and save.
  - `chat.rs` delete, clear, append-tombstone and handle.
  - `SettingsView` persistence and close (`:713-790`, `:1016-1182`, `:1385-1590`, `:2775-2800`).
  - `SettingsWindowApp` and `LauncherApp` routing.
  - The bridge event and backlog code.
- **Read by region:**
  - Tauri 2.11.2 `Emitter::emit`, `Window::close` and `Window::destroy`.
  - `tauri-runtime-wry` 2.11.2 `destroy`, `send_user_message`, getters and `Message::Window` handling.
  - `tauri-macros` 2.6.2 blocking command body.
  - `common.rs` request and router, `native_core/mod.rs` request guard, `process.rs` `kill_on_drop`.
- **Probes:**
  - Snapshot: `/private/tmp/tezbar-runtime-probe-1623d47`, a `git archive 1623d47` with `node_modules` symlinked from the worktree. The repo was not touched.
  - Two probe tests are in its `SettingsView.knowledge.test.ts`. Both FAIL, confirming C-R3 and C-R2(a).
  - Log: `/private/tmp/tezbar-runtime-probe-1623d47-settings.log`.
  - Command: `vitest run src/renderer/SettingsView.knowledge.test.ts -t PROBE` (2 failed, 100 skipped).
  - No cargo build was needed; the native claims are source-verified.
- **Search:** I made no XERJ attempt. The brief allows at most one, and the index was known to be incomplete. I used `rg` and direct paths.
- **Not done:**
  - Full suite reruns. Per the parent: macOS 598 passed / 9 ignored, Windows archive 527 / 8, TS 918 / 9.
  - App launch, visual QA, live providers or MCP, credentials, and push, PR or Actions.

## Final counts

- **N-R1–N-R6:** 6 of 6 closed on their original triggers.
- **R1–R20:** 20 of 20 closed. R17 and R18 are closed for reachability, with internals owned by the other reviewers.
- **U1–U3:** 3 of 3 closed.
- **Runtime P3 list:** closed, accepted or owned by the wire reviewer, as listed.
- **New:** 3 P3, all actionable and all regressions from the N-R2 and N-R6 repairs:
  - **C-R1:** the launcher is not restored after Settings closes.
  - **C-R2:** Settings close can be blocked silently and permanently.
  - **C-R3:** a double model removal leaves a removed model selected for chat.
- **Gate:** not passed. Fix C-R1–C-R3, then run a re-check.
