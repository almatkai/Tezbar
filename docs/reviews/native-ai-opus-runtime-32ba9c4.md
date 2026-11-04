# Native AI spec review, part B: runtime and interface

- Source: `feat/native-rust-agent` at `32ba9c4e5ff41b527ba5943f779a3f9ea090d135`. The worktree was clean and unchanged at the end of the review.
- Fixed point: `v0.3.0-beta.7` = `e246752149dbbede5e44720a0e35a3eacf2d66a9`. The 13 listed commits were verified.
- Scope: `runtime/*`, `mod.rs`, `legacy.rs`, `chat.rs`, `native_tools.rs` caller semantics, `AgentChatView`, `agentChat/*`, Settings, Providers, Onboarding, `renderer/ai/*`, shared types and config, against `docs/NATIVE_AI_SPEC.md` sections 1, 5.3, 6, 9, 10 and the M, F and L rows of section 11.
- Reviewer: Claude Opus 5.5. No code was edited. No app launch, live API, real credential or `preview_*` call was used.

## Verdict

**Not ready.** There are no P0 findings. There are 8 P1 and 12 P2 findings, plus a short P3 list.

Every finding comes from reading the current call sites end to end. I did not run a new probe, because each failure is visible in the control flow. Findings that depend on provider behaviour are labelled, and I did not confirm them against a live provider.

The passing test suites do not catch these. Name searches of the Rust and TypeScript tests found no cover for: retry after reopening a conversation with an image, history images on a non-vision model, approval-card keyboard and mode paths, the Settings window project review, or empty or failing shell output.

## P1

### R1. The shell tool discards output the model needs

- **Where:** `src-tauri/src/native_core/ai/process.rs:221-227`, `:229`; caller `native_tools.rs:438-443`.
- **Trigger:** The model runs a command that exits non-zero, for example `pytest`, `vitest`, `cargo test` with failing tests, `git diff --exit-code`, or a PowerShell command. Separately, a successful command that reports on stderr, for example `git checkout -b x` or `git clone`. Separately, a command that times out.
- **Failure:**
  - Non-zero exit returns `Err("Process exited …: <stderr>")` and drops stdout. Test runners put their failure detail on stdout, so the model cannot see which test failed.
  - Success returns stdout only and drops stderr.
  - Timeout returns `Subprocess timed out` and drops all partial output.
- **Evidence:** `status.success()` branches directly to the stderr-only error. `native_tools.rs` calls this helper for `bash` and `powershell` with no other capture. Since this branch makes native the only path, every provider and platform is affected.
- **Fix:** Add a shell-specific capture that returns combined stdout and stderr plus the exit code. Mark the result `is_error` on non-zero exit, and keep partial output on timeout. The existing `redact_text` and truncation in `agent.rs:228-230` then apply.

### R2. Retry on a reopened conversation permanently deletes the saved image

- **Where:** `src/renderer/agentChat/transport.ts:74-82`, `:141-147`, `:296`, `:320`; `chat.rs:72-75`; `runtime/agent.rs:147-160`; `runtime/store.rs:61-65`, `:118-168`.
- **Trigger:** Send a message with an image, restart or reopen the conversation, then click Retry or Try again on its last response.
- **Failure:** The model answers without the image and without any placeholder. The stored image is deleted.
- **Evidence:**
  1. SQLite keeps attachment metadata only (`chat.rs:72-75`).
  2. `chatTurnToAgentMessage` adds no file part when `data` is absent.
  3. The transport therefore sends `images: undefined` with the same `userTurnId`.
  4. `agent.rs:155-157` overwrites the user turn with text only.
  5. `gc` then removes the blob as unreferenced.
- **Related:** An in-session retry after switching to a non-vision model takes the OCR path. It overwrites the stored image with OCR text in the same way.
- **Fix:** Never downgrade a stored user turn. On regenerate, or when the request has no images and a stored turn exists, reuse the stored turn. Store original image blobs even when the current model gets OCR text.

### R3. History images are sent to non-vision models and can block the conversation

- **Where:** `runtime/agent.rs:87-88`, `:123-146`; `wire/chat.rs:15`, `:19-20`; `wire/anthropic.rs:21`; `wire/gemini.rs:68`.
- **Trigger:** Attach an image on a vision model, pick a model with no `vision` capability such as `deepseek-v4-flash` or `opencode/deepseek-v4-pro`, and send a follow-up.
- **Failure:** The vision check at `agent.rs:123-146` covers only the current prompt's images. Stored `Block::Image` values, including tool-result images, go to the provider unchanged. If the provider rejects images, every later message in that conversation fails until the turn leaves the 16-turn window.
- **Definite vs upstream:** Sending the image to a model the app itself marks as non-vision is definite and contradicts section 5.4. The exact rejection is provider-specific and was not tested live.
- **Fix:** After loading history, when `vision` is false, replace each image with a labelled placeholder or OCR text. Do not change the stored transcript, so switching back to a vision model still has the image.

### R4. Choosing Auto or Turbo on the approval card approves the pending command

- **Where:** `src/renderer/AgentChatView.tsx:1198-1206`.
- **Trigger:** A card asks about a destructive, sensitive or complex command, for example `rm -rf ~/x`. The person changes the mode picker to Auto or Turbo, intending to change future behaviour.
- **Failure:** `resolveApproval('once')` runs immediately and the command executes. Section 6.3 requires Ask in every mode for destructive commands and sensitive paths, and M6 and M7 require the same under Turbo. The picker's own help text says these still require review.
- **Note:** This code is present at the baseline. It is now in direct conflict with the delivered policy and copy.
- **Fix:** Changing the mode must not resolve the pending approval. If auto-resolve is kept, it must go through native re-evaluation under the new mode, so `shell_asks` or `file_asks` can still require the prompt.

### R5. Enter approves from anywhere, including when Deny is focused

- **Where:** `src/renderer/AgentChatView.tsx:776-782`; `window` capture listener at `:803`.
- **Trigger:** While a card is showing:
  - Tab to Deny and press Enter.
  - Or type in the composer, which stays enabled during a run, and press Enter.
  - Or press Enter inside the `/` menu or a prompt argument field.
- **Failure:** The capture-phase handler has no focus check. It calls `preventDefault` and `stopImmediatePropagation`, then approves once. A keyboard Enter on Deny therefore allows the command, and a person sending a message can approve a command they have not read.
- **Note:** Present at the baseline. It fails the keyboard-usable requirement and is unsafe alongside section 10.7.
- **Fix:** Handle Enter only when focus is on the card's own Allow control. Let native button activation work for Deny and Always.

### R6. The project extension Review button is a dead end

- **Where:** `src/renderer/ai/ProjectExtensionNotice.tsx:33-34`; `AiExtensionsSection.tsx:47-56`; `App.tsx:701-703`; `src-tauri/src/lib.rs:2399-2405`; `ext/mod.rs:1150-1155`.
- **Trigger:** Open chat in a folder with a project manifest, then click Review in "This folder has AI extensions. Not running."
- **Failure:** The folder is written to `sessionStorage` in the launcher webview, and Settings opens as a separate `settings` webview window. `sessionStorage` is not shared between them, so Settings calls `list` with no folder, `data.project` is never set, and the review panel with "Add for this folder" never appears. No other control supplies the folder. Even with shared storage, an already open Settings window would not reload, because `load` runs only on mount and on `ai-extensions-changed`.
- **Impact:** The project trust flow in sections 7.3 and 7.4 and section 10.7 cannot be reached through the UI. The component test passes because it does not cross windows.
- **Fix:** Pass the folder through an IPC event or `settingsInitialTab`-style state, reload on receipt, and scroll to the section. Do not use `sessionStorage`.

### R7. Editing AI settings saves extension providers, so disabling an extension does not revoke them

- **Where:** `ai/mod.rs:505-510`; `ext/contributions.rs:28-49`; `SettingsView.tsx:812`, `:842`, `:1067-1071`; `common.rs:166-179`; `providers/route.rs:33-35`; `ext/contributions.rs:58-66`.
- **Trigger:** Enable an extension that contributes a provider. Change anything in Settings > AI, which autosaves. Then disable the extension.
- **Failure:**
  - `llm-config-get` returns the merged list, including `_extensionId` and manifest `headers`.
  - Settings writes the whole list back to `config.json` with no filter.
  - After disabling, the provider stays selectable. Its secret headers are still resolved and sent to the manifest's URL, as long as the secret still exists.
  - Later manifest changes are ignored, because a same-id entry wins at `contributions.rs:30-37`.
  - After removal the provider remains as a broken entry reporting a missing credential.
- **Related:** The chat `selectModel` patch saves merged `providerModels`. That permanently blocks manifest model updates because of `contributions.rs:43`.
- **Fix:** Keep merged data out of the persisted patch. Either return contributions separately or strip any `_extensionId` entries and their model lists server-side in `llm-config-set`.

### R8. A tool with empty output produces an empty text block, which Anthropic is reported to reject

- **Where:** `runtime/agent.rs:227-231`; `wire/anthropic.rs:23`.
- **Trigger:** On the Anthropic protocol, a tool returns an empty string. Examples: `mkdir x`, `touch a`, `git add .`, `ls` of an empty folder, `find` with no matches, `read` of an empty file.
- **Definite:** The request contains `tool_result.content: [{"type":"text","text":""}]`, and the transcript persists it.
- **Upstream, not verified:** Community reports describe a 400, `text content blocks must be non-empty`. I found no official Anthropic page stating this for blocks nested in `tool_result`, and no live call was made. If it applies, the turn is saved first, so the conversation repeats the 400 on every send until that turn leaves the window.
- **Fix:** Replace empty results with `(no output)` in the agent loop, which is also clearer for every provider. Omit empty text blocks in the Anthropic encoder for already saved transcripts.

## P2

### R9. Tool-result images count against the 4 MiB text budget

- **Where:** `runtime/transcript.rs:142-160`, `:165-173`; callers `agent.rs:107`, `:186`.
- **Trigger:** A few `screen_capture` or MCP image results in one conversation window, or a single large one.
- **Failure:** `Block::Image => 0` covers top-level images only. `ToolResult` is measured with `serde_json::to_vec`, which includes nested base64. Over 4 MiB, results are cleared oldest first, including screenshots and unrelated text output, and possibly the newest image before the model sees it. Section 6.5 says images are excluded.
- **Fix:** Measure tool results recursively, skipping images. Also call `bound_images` inside the turn loop. Today it runs only during history load, so once this is fixed, images in a run would otherwise be unbounded.

### R10. Deleting a running conversation leaves its transcript on disk

- **Where:** `AgentChatView.tsx:885-900`; `chat.rs:280-285`; `runtime/agent.rs:262`; `runtime/store.rs:20`.
- **Trigger:** Delete a conversation from history while its agent runs. `chat:clear` during a run has the same race.
- **Failure:** The store folder is removed first. The run is cancelled only afterwards, by `retained.chat.dispose()`, and `save_transcript` recreates the folder. Tool outputs and tool images remain under `llm/transcripts/<hash>/`. No session row references them, and only `chat:clear` can remove them. Section 5.3 requires deletion.
- **Fix:** Cancel runs owned by that session and wait for them to finish before deleting, or keep a per-session tombstone that `store::save` checks.

### R11. Retry forgets effects already performed

- **Where:** `transport.ts:135-140`; `runtime/agent.rs:191`, `:234`.
- **Trigger:** A run completes tool calls, such as creating an issue or writing files, then fails. The person clicks Try again.
- **Failure:** The retried assistant turn is removed from `priorTurnIds`, and the first save overwrites its transcript. The model has no record of the earlier effects and may repeat them. With Turbo or remembered allowances there is no prompt. Section 6.2 rejects whole-run replay after effects. A person-initiated retry is arguably allowed, but it should not be blind.
- **Fix:** When the old transcript contains completed tool results, keep them as prior context under a separate ID, or warn before retrying.

### R12. A missing agent profile silently removes the tool restriction

- **Where:** `ext/mod.rs:1809-1831`; `runtime/agent.rs:174-176`; `AgentChatView.tsx:956-965`.
- **Trigger:** A conversation uses a restricted profile. The extension is then disabled or removed, the profile deleted, or a project extension leaves scope.
- **Failure:** `find` returns nothing, `profile` stays `None`, and all tools are offered. The header chip still shows the profile name, so the person believes the restriction still applies.
- **Fix:** When `agentProfileId` is set but unresolved, fail the run with a clear message, or clear the chip and warn. Do not run unrestricted.

### R13. "Always allow this command" does nothing for commands that always ask

- **Where:** `legacy.rs:110`; `native_tools.rs:379`; `runtime/policy.rs:67-69`; `AgentChatView.tsx:1232-1248`.
- **Trigger:** Any command containing `|`, `$`, `;`, `(`, or any destructive command. Click Always allow.
- **Failure:** The rule is saved, but `shell_asks` checks complex and destructive first, so the same prompt returns every time. The safety rule is correct. The control misrepresents it, and pointless rules accumulate in config.
- **Fix:** Set `canRemember: false` when `complex` or `destructive` is true, and reject `always` server-side in that case.

### R14. Custom providers have no Protocol control

- **Where:** `SettingsView.tsx:1288-1293`; no other `protocol` reference in `SettingsView.tsx`.
- **Trigger:** Adding a custom Anthropic, Responses, Gemini or Ollama endpoint in Settings.
- **Failure:** Every new provider is fixed to `openai-chat`. The `SelectField` required by section 10.6 does not exist, so these endpoints work only through a manifest. The runtime already supports them.
- **Fix:** Add the select to the provider form and persist it.

### R15. Onboarding still says AI chat runs on Pi

- **Where:** `src/renderer/OnboardingView.tsx:470-477`, `:783`.
- **Failure:** First-run copy says "AI chat runs on Pi, an open-source coding agent", "Pi answers right here" and "Pi is listening." Sections 9 and 10.6 require Pi wording to be removed. `PiRow` was removed, but this copy was not.
- **Fix:** Replace it with neutral copy.

### R16. Esc in the composer menu also leaves AI chat

- **Where:** `src/renderer/ai/ComposerExtensions.tsx:97-101`, `:154-160`; `App.tsx:632-644`; `AgentChatView.tsx:808-815`.
- **Trigger:** Press Esc with the `/` menu open or in a prompt argument field.
- **Failure:** The React handler calls `preventDefault` only. The event reaches App's `window` listener, which does not check `defaultPrevented`. The AI chat escape consumer only knows about `historyOpen`, so the surface switches to the launcher. Section 10.7 specifies Esc as closing the menu.
- **Fix:** Call `stopPropagation` in those handlers, and make the escape consumer aware of the menu and argument states.

### R17. Tool review labels every existing tool as Changed

- **Where:** `src/renderer/ai/AiExtensionsSection.tsx:703-723`.
- **Trigger:** A server changes one tool out of many, which sets `needsReview`.
- **Failure:** Every tool present in both catalogs shows "Changed". Only description and top-level property names are compared on screen, so a type, enum or `required` change appears identical before and after. The person cannot see what they are approving, and there is no `Keep disabled` action. Section 10.5 requires Added, Changed and Removed lists and plain schema changes.
- **Fix:** Compare descriptors, list only real changes, show the changed schema fields, and add `Keep disabled`.

### R18. Per-provider image limits are missing

- **Where:** `runtime/limits.rs:10`; `legacy.rs:315-317`.
- **Trigger:** An image over a provider's limit but under 8 MiB, for example from an MCP resource.
- **Failure:** Section 5.4 requires per-provider limits in `limits.rs`, checked before sending, with a message naming the limit. Only the global 8 MiB check exists. Anthropic's documented limit is 5 MB per image, from memory and not rechecked in this review. A rejected image is already saved, so later sends can repeat the failure while it remains in the window.
- **Fix:** Add a limit per protocol and check it before saving the user turn. Also check it for history images.

### R19. Settings can bind another provider's key to OpenCode

- **Where:** `SettingsView.tsx:845-850`, `:1054-1058` (`buildAiProviderPatch`); `providers/route.rs:167`; `provider.rs:409-416`.
- **Trigger:**
  1. OpenAI is configured, so the key is in top-level `apiKey`.
  2. OpenCode is usable through `auth.json` or the environment, and `providerConfigs.opencode` is absent.
  3. Pick an OpenCode model in the chat picker.
  4. Open Settings > AI and change anything.
- **Failure:** `reload` uses `providerConfig.apiKey ?? c.apiKey`, so the OpenAI key fills the OpenCode field. Autosave stores it in `providerConfigs.opencode.apiKey`. It then overrides the `auth.json` key and is sent as a bearer token to opencode.ai.
- **Note:** The fallback exists at baseline for other providers. Making OpenCode use a configured key gives this path a new external destination. The provider-auth reviewer may want to verify the same chain for other providers.
- **Fix:** Use top-level keys only under the same `legacy` rule as `config::selected`.

### R20. The approval mode can be changed only while an approval card is showing

- **Where:** `AgentChatView.tsx:1194`, the only renderer use of `ApprovalModePicker`. There is no other `agentApprovalMode` control in `src/renderer`.
- **Trigger:** A person in Turbo wants to return to Review, or wants to choose a mode before the first prompt.
- **Failure:** Turbo prompts rarely, so the control is rarely available. M2, "Change to Review mid-run", cannot be done through the UI while tools are auto-running. The native side honours live changes correctly, but the UI gives no way to make one.
- **Note:** The placement is at the baseline. The requirement that mode behaviour be explicit in UI help is not met outside the card.
- **Fix:** Add the picker to the composer footer or Settings > AI.

## P3 and spec gaps

- A retry that fails before the run starts leaves the old transcript in place. History then replays a response the UI shows as failed (`agent.rs:51-52` runs before any save).
- An expired approval, after 300 s, leaves a stale card showing while the run continues. Clicking it reports "no longer active" (`legacy.rs:125`).
- A 16-turn window can start on an assistant turn after a deleted response or crashed run. Gemini or Anthropic may reject this. Upstream behaviour was not verified.
- Sending in a conversation whose folder was removed shows a raw OS error, because `getLlmConfig(cwd)` rejects at `mod.rs:498`, before the clear `Directory does not exist` check in `agent:run`.
- The default Gemini model `gemini-2.0-flash` (`config.rs:105`) is not in the default list, so it is treated as non-vision. This applies only when no model has been chosen.
- Section 10.3 and 10.5 gaps: no `Sign in` or `Try again` row action; `Asks` is read-only text; no `oauth-cancel` control, although the channel exists; Time limit appears only for web connections; `Show all N` is absent.
- Section 10.7 gaps: extension names are inline prefixes, not group titles; the profile chip shows the ID, not the name; the menu does not scroll the selected item into view; the textarea has `aria-activedescendant` without `role="combobox"`.
- `text-rose-300` (`AiExtensionsSection.tsx:612`) and `text-red-400` (`ComposerExtensions.tsx:185`) have no `data-theme='white'` override that I could find, and section 10.3 asks for the `Message` error colour. This comes from source only, with no visual check.
- Importing a file does not clear an earlier preview, and the file input is not reset (`AiExtensionsSection.tsx:216-225`).
- `pi-extensions:list` returns Pi items only, while section 9 also requires AI Extensions (`legacy.rs:18-28`). Its one consumer relies on the current behaviour, so either change is acceptable, but the spec and code should agree.
- Section 7.7 says the renderer's prompt lines move to the base prompt. `agent.rs:50` keeps most of them but drops the stay-in-directory, installed-apps and path-shorthand guidance. `buildAgentPromptFromChat` is left behind, unused by the transport.
- Stale copy and comments: `Allow native tool?` (`native_tools.rs:376`) uses an implementation word in main copy; `src/shared/agent.ts:3`, `:8` describe a Pi compatibility adapter; `src/shared/chat.ts:57` says Pi starts in this directory; `llmErrors.ts:39` handles a Pi exit message.

## Coordinator decision needed

`provider.rs:134-141` spawns `opencode models` when listing OpenCode models. The spec's implementation decisions permit optional read-only discovery, and a static catalog fallback exists, so this is not an execution fallback and I do not count it as a failure. It does still run a third-party program from the login `PATH` during a Settings or picker model load, and the `L4` guard bans only `opencode run`. Decide whether to keep it, given the stated completion gate.

## Checked and found correct

- No Pi or Node prerequisite in `agent:run`, `chat:run`, `query`, `ai:action` or legacy `pi:*` handlers. `agentRuntime: "pi"` is accepted and ignored.
- `CallId` is namespaced per run, so reused wire IDs across runs are safe. Foreign IDs are remapped consistently for calls and results.
- Opaque data replays only to the same provider, protocol and model. Thinking text is not sent to the renderer.
- Cancellation closes unanswered calls with the synthetic result and saves. `close_unanswered` repairs crash leftovers without re-executing.
- `native_tool` and `extension_call` read the mode live before each tool, and an explicit request mode pins it. `shell_asks` and `file_asks` match the section 6.3 table as read.
- `prepare` and `create_path` perform no filesystem effects before approval.
- Delete-turn and delete-conversation remove files and unreferenced blobs when no run is active. Image eviction falls back to the labelled placeholder.
- The transport subscribes before starting a run. Late events cannot reach another run, and each conversation persists to its own owner. Conversation profile storage and clearing work.
- `Button` defaults to `type="button"`, so prompt argument buttons inside the composer form do not send it.
- Every action the renderer sends has a matching channel in `CHANNELS` or `ext::CHANNELS`.

## Coverage

**Read in full:** `runtime/agent.rs`, `store.rs`, `transcript.rs`, `limits.rs`, `clock.rs`; `ai/mod.rs`, `chat.rs`, `legacy.rs`, `native_tools.rs`, `config.rs`, `provider.rs`, `wire/mod.rs`, `ext/contributions.rs`; `agentChat/transport.ts`, `model.ts`; `renderer/ai/AiAccountsPanel.tsx`, `AiContributionsEditor.tsx`, `AiExtensionsSection.tsx`, `ComposerExtensions.tsx`, `ProjectExtensionNotice.tsx`, `api.ts`.

**Read in part or by diff:**
- `runtime/policy.rs`: top of file and lines 40-237.
- `AgentChatView.tsx`: lines 60-1429 plus the diff.
- `wire/transport.rs`: lines 1-260.
- `wire/anthropic.rs`, `wire/chat.rs`, `wire/gemini.rs`: encoders only.
- `providers/route.rs`: lines 23-110, 145-180, 255-300.
- `ext/mod.rs`: seams only (`extension_call` head, `extension_instructions`, `list`, `composer-items`, `remember_extension_tool`).
- `process.rs::output`.
- `SettingsView.tsx`, `OnboardingView.tsx`, `ProvidersView.tsx`, `aiProviders.ts`, `tauri-bridge.ts`, `desktop-api.ts`: the diff plus cited regions. `ProvidersView`'s default export has no importer, so its code paths were not counted as reachable.

**Not done:**
- Live provider or MCP calls, Windows execution, and Rust or TypeScript test runs. I relied on the coordinator's recorded results for test status.
- Visual QA in either theme. Theme and layout statements come from source only.
- Wire protocol detail and deep extension trust, which belong to other reviewers.

**Search tool:** One bounded XERJ refresh was attempted. It hit the server's memory circuit breaker and I stopped it. Searches then used `rg`.

## Upstream behaviour not verified

- Anthropic rejection of empty text nested in `tool_result` (R8). Community reports only:
  - [block/sprout PR 559](https://github.com/block/sprout/pull/559)
  - [Claude Code issue 62312](https://claudeissues.com/issue/62312-bug-anthropic-api-error-empty-text-context-blocks-in-messages)
  - [Claude Code issue 23440](https://claudeissues.com/issue/23440-api-error-400-text-content-blocks-must-contain-non-whitespace-text)
- How each non-vision provider responds to image input (R3).
- The first-role rule for a history window that starts with an assistant turn (P3).
- Anthropic's 5 MB image limit (R18), which is from memory.
