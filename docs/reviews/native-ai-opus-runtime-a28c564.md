# Native AI runtime closure review, scope B: runtime, history, tools, approvals and renderer

- Reviewer: Claude Opus 5.5. I did the review myself, with no subagents. I edited no source or docs.
- Worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-rust-agent`.
- Pinned HEAD: `c4866144f063846a935376010da331547007be0b`. It is docs-only on top of the tested source `a28c5647dc589a33b60b2c80ca082246c1f0bf75`; `git diff a28c564..c486614` touches only 4 docs files.
- Worktree state: clean (`git status --short` empty) at the start and at the end. HEAD was unchanged at the end.
- Task base: `v0.3.0-beta.7` = `e246752149dbbede5e44720a0e35a3eacf2d66a9`.
- Diffs reviewed:
  - Latest repair: `118cb7b..a28c564`, 37 files.
  - Full scope: `e246752...c486614`, read for the paths below.
- Commits, `git log e246752..HEAD --oneline`:
  - `c486614` docs(ai): record re-review repairs and dual-platform verification
  - `a28c564` fix(ai): repair native migration re-review seams
  - `9fe0576` docs: preserve native AI repair re-review findings
  - `118cb7b` fix(ai): close extension trust auth and lifecycle review findings
  - `fcabc38` fix(ai): repair provider wire shaping and bounded fallback
  - `e672ecb` fix(ai): preserve run history and require explicit tool approval
  - `9b76b95` fix(ai): isolate provider credentials and refuse redirects
  - `2e29cf1` docs(ai): retain complete Opus review and repair decisions
  - `32ba9c4`, `42e2ccb`, `df2f37d`, `ef66e03`, `7188ec2`, `36c376b`, `7a7c62e`, `677f534`, `c7271bc`, `3d975c0`, `910a2d3`, `6f32a99`, `b5ba21c`: the original implementation series.
- Inputs read:
  - In full: `docs/reviews/native-ai-opus-runtime-32ba9c4.md` (R1–R20, P3 list and coordinator decision), `docs/reviews/native-ai-runtime-118cb7b-partial.md` (U1–U3) and `docs/reviews/NATIVE_AI_REVIEW_FIXES.md` (the ledger).
  - In part: the coordinator decisions at the top of `docs/NATIVE_AI_SPEC.md`.
  - I treated ledger claims as claims, not evidence. Every disposition below comes from reading the production call path. Where I say so, it also comes from a probe I ran myself.

## Verdict

**Not ready. Findings: 0 P0, 0 P1, 1 P2, 5 P3.**

R1–R20 and the functional runtime P3 items are closed on their original triggers, and so are U2 and U3's in-component ordering. One new P2 remains, confirmed by probes on both sides of the IPC seam:

- **N-R1.** Deleting a conversation while a send or a retry in it is still preparing has two effects:
  - The renderer still starts a native run for that conversation.
  - Native accepts a run for a tombstoned session. That run calls the provider and writes the deleted conversation's user prompt and answer back to disk.

  This is a residual variant of R10 and U1. The U1 fix covers runs that were already active when deletion started. It does not cover runs that start during deletion.

## New and outstanding findings

### N-R1 (P2). Delete during send or retry preparation starts a run and recreates the deleted transcript

**Where (renderer):** `src/renderer/AgentChatView.tsx`.
- `send`:
  - `:499` `await getLlmConfig`
  - `:540` `await persistTurn(userTurn, owner)`
  - `:551` `owner.chat.sendMessage(...)`
- `retry`:
  - `:579` `await getLlmConfig`
  - `:589` `owner.chat.regenerate(...)`
- `deleteConversation`:
  - `:893-895` sets `deleting = true`
  - `:917` clears `submission` only after the native reply

**Where (native):**
- `src-tauri/src/native_core/ai/mod.rs`: `agent:run` at `:688` and following; `launch` at `:372`. The transcript generation is captured at launch, `:388-394`.
- `runtime/store.rs:11-29` and `:192-202`: the revisions are in memory and checked against the generation captured at launch.
- `chat.rs:168-179`: the `chat_deleted_sessions` tombstone is checked only by `chat:append`.

**Trigger:**
1. Press Enter, or click Try again, in conversation B.
2. While B is still preparing, open History and delete B. Preparation is the await on the model save, the profile save and `getLlmConfig(cwd)`. The Delete button is enabled during that time, `AgentChatView.tsx:1081-1092`.
3. Native deletion is still in flight when preparation finishes.

**Failure:**
- `send` and `retry` check only `owner.submission`, which deletion clears after the native reply. They never check `owner.deleting`.
  - `persistTurn` correctly skips the SQLite write, because the owner is deleting.
  - `send` and `retry` then call `sendMessage` or `regenerate` anyway.
  - `agent:run` is sent with the deleted session ID.
- `chat:delete` cancels only runs that are registered when it executes, `mod.rs:826-846`.
  - A run that launches afterwards captures the post-delete generation.
  - `save_at` therefore accepts its writes, `store.rs:22-29`.
  - No path checks the SQLite tombstone.
- The new run then does three things:
  - It persists the user turn, including image blobs, to `llm/transcripts/<hash(session)>/` before any provider call, `agent.rs:152-174`.
  - It calls the provider and may run tools.
  - It saves the assistant transcript.

  The renderer's later `chat.dispose()` cancels the run, but files written before the cancel stay on disk. No session row references them, so only `chat:clear` can remove them.
- This breaks the deletion requirement in spec §5.3. It also contradicts the ledger's U1 statement that native tombstones reject late work for deleted sessions; that is true only for `chat:append`.

**Evidence (renderer probe):**
- Snapshot: `/private/tmp/tezbar-runtime-probe-a28c564`, a `git archive` of `c486614`.
- Test: `src/renderer/agentChat/delete.probe.test.ts`, using the real `AgentChatView`, the real AI SDK and mocked IPC.
- Both tests fail:
  - Send: `PROBE-RUNS [{"sessionId":"B","task":"Private follow-up"}]`, while `store.has('B') === false`.
  - Retry: `PROBE-RETRY-RUNS [{"sessionId":"B","retry":true}]`.
- In both tests `agentCancel` is called only afterwards, by dispose.

**Evidence (native probe):**
- Snapshot: `/private/tmp/tezbar-runtime-native-a28c564`. Target: `/private/tmp/tezbar-runtime-a28c564-target`, a copy-on-write clone. Log: `/private/tmp/tezbar-runtime-native-a28c564-probe.log`.
- Test: `probe_a28c564_run_started_after_delete_must_not_write_transcript`, through the real `chat:append`, `chat:delete` and `agent:run` dispatch, with a loopback provider and a temporary profile.
- Output: `PROBE-START {"ok":true,...}` and `PROBE-PROVIDER-CALLED true USER-SAVED true ANSWER-SAVED true`. The test FAILED.

**Fix:** both layers are needed.
1. Renderer:
   - At the start of `deleteConversation`, set `retained.submission = null` and `preparing = false`. Alternatively, check `owner.deleting` after every await in `send` and `retry`, and before `persistTurn`, `sendMessage` or `regenerate`.
   - Do not start a run for an owner that is being deleted.
2. Native: reject `agent:run` with a clear error when `sessionId` is in `chat_deleted_sessions`.
   - Do this before the retry pre-run at `mod.rs:703`, and before the generation is captured.
   - Alternatively, keep a persistent store-level tombstone that `save_at` checks.
   - Add a regression for both orders: a run that starts after deletion, and deletion that lands between launch and the first save.

### N-R2 (P3). A pending Settings autosave is lost when the Settings window closes within 300 ms

**Where:**
- `SettingsView.tsx:1416-1470`: the debounce, `persistAiSettings` and `flushAiSave`.
- `:1545-1553`: the only flush on close, a React unmount cleanup.
- `src-tauri/src/lib.rs:2847-2861`: on `CloseRequested` for `settings`, the code calls `hide()`. It does not prevent the close, so the webview is destroyed ("Release the Settings renderer when closed").
- No `pagehide`, `beforeunload` or `onCloseRequested` flush exists in Settings. The only `beforeunload` is in `main.tsx:97` and does not flush.

**Trigger:** Paste or type an API key or base URL, then close the Settings window within the 300 ms debounce.

**Failure:** React does not unmount roots when the page is torn down. The flush in the unmount cleanup therefore never runs on a real window close, and the last edit is never written. The renderer test for "pending-close" unmounts the React root, which is not the production close path.

**Certainty:** That nothing flushes is certain from the source. Whether WKWebView or WebView2 fires unload events during destroy was not checked, because I did not launch the app, and no handler exists anyway.

**Fix:**
- Flush synchronously on `pagehide` and on `visibilitychange` to `hidden`, or from a Tauri `onCloseRequested` handler in the Settings window that awaits `aiWrites`.
- Alternatively, flush on blur of the key and URL fields.

### N-R3 (P3). Shell capture keeps only the head of large stdout, so test-runner summaries are lost

**Where:** `process.rs:262-279` (`capture_read` keeps the first `limit` bytes) and `:296-298` (48 KiB stdout, 12 KiB stderr).

**Trigger:** The model runs a verbose test runner whose stdout exceeds 48 KiB, for example `pnpm test`, `pytest -v` or a large `cargo test`. These runners print the failure list and summary at the end.

**Failure:** The tail is dropped, and only `[Output truncated]` is appended. The model sees the first 48 KiB of passing tests and not which tests failed. This partly undermines R1's purpose.

**Fix:** Keep the head plus a tail ring buffer, for example 16 KiB head and 32 KiB tail, with an explicit omitted-bytes marker.

### N-R4 (P3). The project-review request is never cleared, so Settings keeps jumping to the extensions section

**Where:**
- `ext/mod.rs:1210-1219`: `review-project` sets `review_cwd`, and nothing ever clears it (the field is defined at `:60` and set only at `:1213`).
- `ext/mod.rs:1232-1235`: every `list` call without an argument returns `project` for the queued folder.
- `AiExtensionsSection.tsx:74-77`: the section scrolls into view whenever `state.project` changes identity.
- `:114-119`: the section reloads on every `ai-extensions-changed` event.

**Trigger:**
1. Click Review once in the project notice.
2. Later in the same app session, make any extension change that emits `ai-extensions-changed` (`ext/mod.rs:300-302`, `oauth.rs:292`), such as saving a connection, testing it, toggling a tool or finishing a sign-in.

**Failure:** Each reload creates a new `project` object, and Settings scrolls back to the top of the AI Extensions section. The project review panel for that folder also stays shown until restart, even after it was handled.

**Fix:**
- Clear `review_cwd` once the project is trusted or kept disabled, or when the panel is dismissed.
- Make the scroll one-shot, keyed on a review nonce rather than on object identity.

### N-R5 (P3). Retrying a reopened conversation on a non-vision model drops the OCR text the original attempt had

**Where:** `runtime/agent.rs:141-147` runs OCR only on images in the current request. `:159-165` reuses the stored user turn on retry. `transcript.rs:142-152` (`image_view`) projects the stored images to "[Earlier image omitted…]".

**Trigger:**
1. On a non-vision model, send an image. The original attempt gets the OCR text.
2. Reopen the conversation, then click Try again.

**Failure:** The renderer has no image bytes after a reopen, so the request has no images and OCR is skipped. The stored image becomes a placeholder, so the model loses the content it had on the first attempt. An in-session retry still gets OCR, because the renderer still has the bytes. Behaviour therefore depends on whether the conversation was reopened.

**Fix:** When vision is false and the reused stored user turn contains images, run OCR on those stored images for the disposable view.

### N-R6 (P3, existed before this branch). Settings writes back stale provider and model selections made from the chat picker

**Where:**
- `SettingsView.tsx:826-912`: AI state hydrates only on mount.
- `buildAiProviderPatch`, `:1034-1100`: every write includes `provider`, `model`, `providerSelectedModels`, `taskProviderOverrides.chat` and `taskModelOverrides.chat` from the Settings state.

**Trigger:**
1. Settings is open.
2. The person picks a different model in the chat header's `ModelPicker`, which calls `selectModel` and writes the config.
3. The person edits any AI field in Settings.

**Failure:** The Settings patch restores the hydrated provider and model, and the chat selection silently reverts.

**Fix:** Reload the AI selection fields on `onLlmConfigChanged` when no edit is pending, or send only the fields the edit changed.

## Per-ID disposition: R1–R20

| ID | Disposition | Production path and evidence |
| --- | --- | --- |
| R1 shell output | **Closed** (residual P3: N-R3) | `native_tools.rs:422-445` calls `process::shell_capture`. `process.rs:236-327` keeps both streams, the exit code and partial output on timeout: the time-out path drains the queued pipes for 500 ms after `terminate`. Non-zero exit or timeout maps to `Err` and `is_error`. `agent.rs:254-261` bounds the result to `RESULT_BYTES` (64 KiB), which is larger than 48 + 12 KiB plus the header, so stderr survives. Cancellation still discards output, which is correct because the outcome is unknown. |
| R2 reopened retry deletes the image | **Closed** (residual P3: N-R5) | `transport.ts:139-140` and `:299-301` keep the same user turn ID with `retry: true`. `agent.rs:159-165` reuses the stored canonical user turn when there are no request images or on retry. Saving at `:166-173` never downgrades it, and blobs stay referenced, so `store.rs:139-190` gc keeps them. OCR text goes only into the view, `:219`. |
| R3 history images on non-vision | **Closed** | `agent.rs:219` builds `image_view(&messages, vision)` each turn. `transcript.rs:142-152` projects recursively, including tool-result images, without mutating the canonical messages. |
| R4 mode change approves | **Closed** | `AgentChatView.tsx:1207-1237`: both pickers call only `setLlmConfig({agentApprovalMode})`, and nothing calls `resolveApproval`. Native reads the mode live per tool, `native_tools.rs:315-319`. |
| R5 global Enter approves | **Closed** | The window keydown handler at `AgentChatView.tsx:787-812` handles only Cmd/Ctrl+`.` and Cmd/Ctrl+Shift+S. Allow, Deny and Always are plain `type="button"` elements (`:1246-1278`), so Enter acts only on the focused button. |
| R6 project Review dead end | **Closed** for the hand-off (residual P3: N-R4) | `ProjectExtensionNotice.tsx:33` sends `review-project` to native. `ext/mod.rs:1210-1219` queues the folder and emits an event. `list` with no folder returns the queued project. `AiExtensionsSection` reloads on the event and scrolls the section into view. No `sessionStorage` is involved, and native IPC crosses the windows. |
| R7 extension providers persisted | **Closed** | `mod.rs:589-601` runs `llm-config-set` through `filter_extension_patch` (`ext/contributions.rs:7-43`). It strips `_extensionId` and manifest-ID providers, and their `providerModels` and `providerConfigs`, whether the source extension is enabled or disabled. The Settings discovery write also skips extension-owned providers, `SettingsView.tsx:733`. |
| R8 empty tool output | **Closed** | `agent.rs:296-301` maps an empty or whitespace-only result to `(no output)`. Shell results always carry the exit-status header. Encoding of saved empty Anthropic blocks belongs to the wire reviewer. |
| R9 image bytes in the text budget | **Closed** | `transcript.rs:153-155` counts images as 0 and sizes tool results recursively. `agent.rs:216` bounds images inside the turn loop. |
| R10 delete during a run leaves the transcript | **Closed for the original trigger.** **Residual variant N-R1 is open.** | `mod.rs:826-846` cancels the session's runs. `store.rs` revision checks in `save_at` refuse saves from runs that started before the deletion. The repo regression `active_delete_and_clear_cannot_resurrect_transcript_files` covers this. A run that starts after the deletion is not covered (N-R1). |
| R11 retry forgets effects | **Closed** | `transport.ts:301` sends an explicit `retry`. `mod.rs:703-716` closes unanswered calls and appends a marker to the saved attempt. `agent.rs:177-192` re-includes a prior attempt that made calls, plus a "do not repeat effects" note, and subsequent saves keep it. Regression: `manual_retry_keeps_actual_completed_shell_effect_once`. |
| R12 missing profile widens tools | **Closed** | `ext/mod.rs:2072-2098` returns an error when `agentProfileId` is set and unresolved, before any provider call. Regression: `missing_profile_fails_closed_before_provider_request`. The chip shows "(unavailable)", `ComposerExtensions.tsx:122-126`. |
| R13 useless Always allow | **Closed** | `native_tools.rs:371-381` passes `remember=false` for complex, destructive, mutation-option or sensitive commands and for every non-bash tool. `legacy.rs:110` sets `canRemember`. `legacy.rs:144-147` rejects `always` on the native side. The renderer hides the button, `AgentChatView.tsx:1262`. |
| R14 no Protocol control | **Closed** | `SettingsView.tsx` shows a Protocol select for user-owned custom providers (`:2259` and following) and a per-model protocol inside the model fieldset. These are hidden or disabled for extension-owned providers (U2). |
| R15 Pi onboarding copy | **Closed** | No user-facing "Pi" copy remains in Onboarding or Chat. The only match is a code comment at `OnboardingView.tsx:647`. The "Pi extensions (…, not loaded)" summary in `AiExtensionsSection.tsx:1053` is the copy the spec requires. |
| R16 Esc in the menu leaves chat | **Closed** | `ComposerExtensions.tsx:102-107` and `:175-181` call `preventDefault` and `stopPropagation`. App's window listener skips events with `defaultPrevented` (`App.tsx:635`). The chat escape consumer calls `commands.consumeEscape()` first (`AgentChatView.tsx:815-826`). |
| R17 tool review | **Closed** as far as runtime reachability goes. The standards reviewer owns the hash and descriptor details. | `Keep disabled`, the Added, Changed and Removed lists and before/after fields exist in `AiExtensionsSection.tsx` (`Keep disabled` is at `:961`). |
| R18 per-provider image limits | **Closed** as far as runtime reachability goes. The wire reviewer owns the codecs and budgets. | `agent.rs:148-151` validates current vision images per protocol before the user turn is saved. `limits.rs:13` and following. |
| R19 key crossing in Settings | **Closed** | `SettingsView.tsx:607-611`: `readProviderConfig` falls back to top-level keys only for legacy configs, and only for the matching provider. `mod.rs:602-632` migrates legacy top-level credentials into their original owner when the provider changes. |
| R20 mode only on the card | **Closed** | `AgentChatView.tsx:1206-1217` adds an always-visible footer `ApprovalModePicker`, outside the card. A duplicate picker also stays inside the card, which is cosmetic. |

## Per-item disposition: runtime P3 list and the coordinator decision

| Item | Disposition and evidence |
| --- | --- |
| P3-01 pre-start retry failure | **Closed.** The saved attempt is marked as historical before preflight, `mod.rs:703-716`. If a retry fails before any output, `agent.rs:266-291` saves an empty slice. That is acceptable: the UI keeps the error, and history simply omits that attempt. |
| P3-02 expired approval card | **Closed.** `legacy.rs:122` always emits `approval-expired`, and `transport.ts:256-258` clears the card. |
| P3-03 assistant-first window | **Belongs to the wire reviewer.** Not re-verified here. |
| P3-04 removed folder | **Closed.** `mod.rs:514-518` returns `Directory does not exist`. `send` catches it and formats it, and no turn is persisted. |
| P3-05 Gemini default | **Belongs to the wire reviewer** (F24). Not re-verified. |
| P3-06 row actions | **Closed** as present in source: Sign in, Cancel sign-in, Try again, Show all, Keep disabled and the policy select (`AiExtensionsSection.tsx:431`, `:684`, `:720`, `:811`, `:961`, `:966`). |
| P3-07 composer accessibility | **Closed.** Group titles (`ComposerExtensions.tsx:153-157`), resolved profile name, `scrollIntoView` of the active option (`:62`), and `role="combobox"` with `aria-expanded` (`AgentChatView.tsx:1359-1360`). |
| P3-08 error colours | **Closed in source** (`styles.css:2797-2798`). No visual QA was done. |
| P3-09 import reset | **Closed.** `AiExtensionsSection.tsx:256` and `:270-271` clear the preview and the input value. |
| P3-10 `pi-extensions:list` alias | **Accepted.** The deliberate scoped alias is documented in the ledger, and the consumer behaviour is consistent. |
| P3-11 prompt ownership | **Closed.** The base prompt at `agent.rs:54` includes stay-in-folder, `launcher_search` for installed apps and path-shorthand guidance. `buildAgentPromptFromChat` no longer exists in `src`. |
| P3-12 stale copy | **Closed.** No "Allow native tool?" remains; the title is "Allow this tool?" (`native_tools.rs:373`). |
| Coordinator decision: `opencode models` | **Closed.** No `opencode` or Pi, Node, npm or Bun spawn exists in non-test code under `native_core/ai` (`rg`). The only `npx` match is a Windows PATHEXT fixture test, `process.rs:578`. |

## U1–U3 (recovered partial findings)

| ID | Disposition | Evidence |
| --- | --- | --- |
| U1 delete of a running conversation; completion persistence; failure recovery | **Original trigger closed. Not fully closed: N-R1 is open.** | `deleting` is set before any await (`AgentChatView.tsx:895`). Pending writes are drained (`:897`). Completion persistence is suppressed while deleting (`persistTurn`, `:243-248`). On failure, the owner recovers and saves its unsaved turns (`:905-909`). On success, the owner's session is cleared before dispose (`:912-920`). Native tombstones (`chat.rs:168-179`, `:261-301`) reject a late `chat:append` after delete or clear, including the extra running-session IDs passed to clear. The repo tests "prevents resurrection… done overtakes" and "recovers… when deletion fails" pass, per the parent's logs. There is no clear-all UI consumer (`chatClear` is only in the bridge). The gap is a send or retry that starts during deletion (N-R1). |
| U2 extension-owned provider contract | **Closed.** | `SettingsView.tsx:1710`: `extensionOwner` hides Remove, Protocol, the key field and the base URL. It disables adding models, Show all and Hide all, removing models and the per-model fieldset, and shows a "Managed by AI Extension" note. Selecting a model stays available. Discovery does not persist anything for owned providers (`:733`). User-owned custom providers keep every control, because the conditions apply only when `_extensionId` is set. Persistence filtering is native (R7). One cosmetic point: the note shows the extension ID, not its display name. |
| U3 autosave ordering | **Closed for in-component ordering** (residual P3 for a real window close: N-R2). | The debounce flush builds from `buildAiPatchRef.current`, the latest render, `:1458-1470`. Writes are serialized through `aiWrites`. `selectAiProvider` flushes the previous provider first (`:986`). Discovery is fenced by `aiEditRevision` and waits for queued writes (`:723`, `:730-732`). Discovery spreads `prior`, so protocol and `maxTokens` survive (`:745`). Hydration (`reload`) never calls `persistAiSettings`. The only automatic write is model discovery, which merges against the persisted config and does not cross keys. Every debounced call site passes no overrides, so dropping pending overrides loses nothing. A real window close does not run the unmount flush (N-R2). |

## Regression sweep of the originally repaired paths

- **Shell:**
  - stdout, stderr and exit status: correct.
  - Timeout keeps partial output.
  - A background grandchild holding stdout open still waits until the timeout and then reports a timeout. This existed before and is not new.
  - Cancellation is checked first, with `biased`.
- **Image store and GC:**
  - Content-addressed blobs, re-referenced on reload.
  - GC runs on every save and scans every JSON file in the session folder. That costs O(turns) per tool-result save. It is a performance note, not a functional defect.
  - Over-limit blobs load as an explicit placeholder.
- **Side-effect retry ledger:**
  - Correct for retries.
  - Each retry adds an assistant marker and a user "Continue…" message, and these stay in later history. The window bounds them.
- **Live permission modes:**
  - The mode is read from config per tool call, unless the request pins it.
  - Focus-local approval keyboard: no global Enter handler; native buttons.
- **Pending new chats, screens, attachments, initialization and session isolation:**
  - Owners are per conversation.
  - The transport closes over its owner's session (`AgentChatView.tsx:300-305`).
  - Capture and boot nonces have revision guards.
  - I found no regression by reading. The parent's ownership suite covers these cases.
- **Project review IPC between windows:** functional. Residual UX issue: N-R4.
- **Model protocol control:** present, and preserved across rediscovery.
- **Onboarding wording:** native.
- **Escape in the slash menu:** correct.
- **Profile selection and missing-profile error:** fail-closed, with an unavailable label.

## Interop and behaviour I could not prove

- **Consecutive assistant messages after a manual retry.** `mod.rs:713` adds an assistant marker after the saved attempt. That attempt usually ends with an assistant message carrying an error suffix, so two assistant messages become adjacent. Whether each provider codec merges or accepts this is for the wire reviewer to verify. I did not check current provider documentation or make a live request, so this is not a finding.
- **Window teardown in WKWebView and WebView2.** Whether either fires any unload event during destroy (relevant to N-R2) was not checked, because I did not launch the app.
- **Fixture limits.** The probes use loopback and mocked IPC. They do not establish live provider acceptance, desktop visual quality, or Windows behaviour for these renderer paths.

## Coverage

- **Read in full:**
  - `runtime/agent.rs`, `runtime/store.rs`, `chat.rs`
  - `agentChat/transport.ts`, `ProjectExtensionNotice.tsx`, `ComposerExtensions.tsx` (keyboard and menu part)
  - the original runtime report, the partial report and the ledger
- **Read by region or diff:**
  - `runtime/transcript.rs` (lines 120–298), `runtime/limits.rs` (head)
  - `process.rs` (lines 190–335 and the a28c564 diff)
  - `native_tools.rs` (lines 300–470)
  - `legacy.rs` (approval prompt and approve)
  - `mod.rs` (agent:run, launch, chat handlers, llm-config-get and -set)
  - `ext/mod.rs` (`review-project`, `list`, `extension_instructions`, change emit)
  - `ext/contributions.rs` (filtering)
  - `AgentChatView.tsx` (lines 55–1300 and 1350–1365)
  - `SettingsView.tsx` (the a28c564 diff, reload, `loadAiModels`, `selectAiProvider`, `buildAiProviderPatch`, persistence, the U2 controls)
  - `App.tsx` (Escape and `openNativeSettings`)
  - `AiExtensionsSection.tsx` (load, reload, scroll and the P3 controls)
  - `messages.tsx` (the retry control)
  - `lib.rs` (the Settings `CloseRequested` handler)
  - `ownership.test.ts` (the harness and the U1 tests)
  - `native_tests.rs` (the delete regression and its helpers)
- **Probes:**
  - Renderer: `/private/tmp/tezbar-runtime-probe-a28c564/src/renderer/agentChat/delete.probe.test.ts`. Both tests fail, which confirms N-R1 on the renderer side. `node_modules` is symlinked from the worktree, and the vitest cache is redirected to `/private/tmp`.
  - Native: `/private/tmp/tezbar-runtime-native-a28c564`, test `probe_a28c564_run_started_after_delete_must_not_write_transcript`. It compiled and ran, and FAILED, which confirms N-R1 on the native side. Log: `/private/tmp/tezbar-runtime-native-a28c564-probe.log`. The build used its own copy-on-write target clone, `/private/tmp/tezbar-runtime-a28c564-target`. The embedding payload and `node_modules` were copied or symlinked read-only from the worktree.
- **Search:** one bounded XERJ attempt with prefix `ax-65b00e69da24`, capped at 20 s, returned only low-relevance hits. After that I used `rg` and direct paths. I did not repair the index.
- **Not done:**
  - Full suite reruns. I relied on the parent's logs: macOS 591 passed and 9 ignored, Windows 520 passed and 8 ignored, TS 905 passed and 9 skipped.
  - App launch, live providers or MCP, credentials or profiles, visual QA, and Windows runs of the probes.
  - Wire codecs and MCP, trust and secrets internals, which belong to the other reviewers.
