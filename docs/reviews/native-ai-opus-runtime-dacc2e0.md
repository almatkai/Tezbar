# Native AI runtime final review at dacc2e0: T-R2 and touched Settings provider save/removal/retry paths

- Reviewer: Claude Opus 5.5 (`claude-opus-5-5`). I did the audit myself, with no subagents or delegates.
  - I made no repo edits, docs, branches, checkouts or worktrees.
  - I did not push, open a PR, run Actions, launch the app, or use real providers, credentials, profiles or MCP.
- Worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-rust-agent`.
- Pinned HEAD: `dacc2e0818267683dc1443a40ded70d7c68d7292`. HEAD was the same at the start and at the end, and `git status --porcelain` printed 0 lines both times.
- Tested implementation: `b2b4f778a1e45e3c4b7cc7385212d391195f5ea0`. Its tree is `b0b43ceb0a874282e8558717cb0a321312c992a8`, which matches the brief.
  - `git diff --stat b2b4f77 dacc2e0` touches only `docs/NATIVE_AI_VERIFICATION.md` and `docs/reviews/NATIVE_AI_REVIEW_FIXES.md`.
- Diff reviewed: `fd8c356..b2b4f77`:
  - `src/renderer/SettingsView.tsx` (+83/−67 net in the hunks)
  - `src/renderer/SettingsView.knowledge.test.ts` (+154)
  - The other two files in that range are the predecessor review docs.
- Current call sites traced:
  - Removal and model edits: `removeCustomProvider`, `addCustomProvider`, `addAiModel`, `removeAiModel`.
  - Selection and save: `selectAiProvider` (the flush precedent), `persistAiSettings`, `pendingAiOverrides`, `flushAiSave`, `buildAiProviderPatch` (its `customProviders`, `providerModels` and `providerConfigs` defaults), and `writeAiPatch` (in full).
  - Failure handling: `failedAiProviders`, `failedAiWrites`, `pruneSupersededAiFailures`, `settleAiSaves`, and the unmount flush.
  - `settingsLifecycle.ts` in full: `enqueueSettingsWrite`, `hasNewerSettingsSave`, `registerSettingsOwner`, `drainSettingsOwners`, `retryRetiredSettingsOwners`.
  - `tauri-bridge.ts` `closeSettings`, which shows the "Retry saved changes" alert that calls `retryRetiredSettingsOwners`.
  - `App.tsx`: the native Settings window mounts `<SettingsView key={settingsTab}>`, so a tab re-target remounts the view and retires its owner.
  - The discovery write path (`loadAiModels`), which is gated by `aiEditRevision`.
- Read in full: `docs/reviews/native-ai-opus-runtime-fd8c356.md`.
- Search: I did not attempt XERJ, because the index is known to be incomplete. I used `rg` and direct paths on the known files only.

## Verdict

- **T-R2: CLOSED.** Both original triggers are fixed. That holds with and without a first pending-save failure, and also for an extra chat-override variant. All of this is probe-confirmed, and fd8c356 fails the same probes.
- **Touched-path ownership, error and retry changes (1–3): correct for what they target.** They also fix the two parent-found bugs.
- **New finding: 1 × P3, actionable (T-R3).**
  - A failed draft that was captured before a successful custom-provider removal brings the deleted provider back when the "Retry saved changes" path replays it.
  - It has two triggers:
    - Trigger B is a **regression introduced by b2b4f77**: fd8c356 passes it.
    - Trigger C is **pre-existing**: it fails identically at fd8c356.
  - Both come from the same root cause in the touched retry path.
- **Zero-actionable gate: NOT passed.** I verified a fix on a scratch copy (see below).

## T-R2: CLOSED

**What changed and why it closes:**
- `removeCustomProvider` now calls `flushAiSave()` first, which mirrors `selectAiProvider`.
- A pending debounced X edit is always for the viewed provider, and Remove provider is only rendered for the viewed `aiProvider`. So the flush writes that edit for X, with its own `providerSelectedModels: {X: …}` or `removedModels` scope, before the removal is queued.
- `pendingAiOverrides` is then `undefined`. The removal's overrides therefore carry no `model`, no `activeProvider` and no `removedModels`, and its patch has no selection fields.
- Hardening inside `writeAiPatch`:
  - It now applies `delete selections[removed]` before the `selections` merge. That merge was the T-R2 override.
  - It also recomputes `providerConfigs`, `providerModels` and `customProviders` from `current`, so the deleted key cannot reappear from this write.

**Independent probes** (snapshot `/private/tmp/tezbar-runtime-probe-dacc2e0` = `git archive b2b4f77`; base `/private/tmp/tezbar-runtime-probe-dacc2e0-basefd8` = `git archive fd8c356`):

| Probe | b2b4f77 | fd8c356 |
| --- | --- | --- |
| A1: Ollama active, chat override on X/x-a; on X add x-z and Remove provider in the same tick | PASS: ollama/llama-o everywhere, X keys gone. The X edit was written for X first, and no write targeted Ollama with x-z | FAIL: model `x-z` |
| A2: X active with x-a; remove x-a, then Remove provider in the same tick | PASS: ollama/llama-o, no ollama/x-b write | FAIL: model `x-b` |
| D: the pending X edit fails AND the removal fails; live drain rejects; retire, then `retryRetiredSettingsOwners`; drain then resolves | PASS: clean | FAIL |
| E: control, no pending edit | PASS (the Ollama base URL is untouched) | PASS |

The repo's 5 new real-component regressions also pass: the 4 T-R2 triggers with and without failure, and the peer credentials on a failed-removal retry.

## Touched-path changes: verdicts

1. **flush before removal:** correct. The ordering is preserved because `enqueueSettingsWrite` chains on `queue.catch(()=>{})`, so the removal runs even if the flushed write fails. There is no stale `model` or `removedModels` merge.
2. **Owner set narrowed to `[removedProvider]`:**
   - **What is correct:**
     - On success, `saved[X]` advances. Retired owners' X drafts are then pruned through `hasNewerSettingsSave`, and local X failures are cleared.
     - `AI_SAVE_ERROR` is cleared only when an X failure was discarded and no other failure remains.
     - The parent's bug "a pending edit fails, then the removal succeeds, and close stays blocked" is fixed: the repo cases pass, and D's live variant agrees.
   - **The side effect that leads to T-R3:**
     - Before this change, a removal patch owned and persisted every remaining provider's UI config, and its success cleared their failures.
     - Now a peer's failed draft (for example Ollama) survives a successful removal, together with its *captured* `customProviders` and `providerModels` snapshot, which still includes X.
3. **Removal retry recomputes from current:** correct and immutable.
   - `next.providerConfigs`, `next.providerModels`, `next.customProviders` and `next.providerSelectedModels` are fresh copies taken from `current`.
   - `delete selections[removed]` acts on a local copy.
   - There is still no assignment to `patch.*` or `scope.*` (`rg`).
   - The parent's peer-new versus captured-peer-old retry bug is fixed (the repo case passes).

Other touched paths that do not regress:
- `selectAiProvider` is unchanged.
- External selections are preserved through the `current` merge.
- `loadAiModels` stale writes are blocked, because `persistAiSettings` bumps `aiEditRevision`.
- The T-R1 cases still pass.
- C-R1 and C-R2 (`lib.rs`, `tauri-bridge.ts`, `settingsLifecycle.ts`, `App.tsx`) are unchanged in `fd8c356..b2b4f77`.

## T-R3 (new, P3, actionable): a retained failed draft resurrects a successfully removed custom provider on "Retry saved changes"

**Root cause:**
- `buildAiProviderPatch` always captures the full UI `customProviders` list and the full `providerModels` map. Add-provider also captures the full `providerConfigs`, plus `provider` and `model` for activation.
- `failedAiWrites` retries replay `writeAiPatch(patch, scope)`, and that spreads `...patch` over the current config.
- Nothing at execution time drops a custom provider that a later write has deleted. So a draft that failed before the removal re-adds X when it is retried after the removal.
- Change 2 makes this reachable for peer drafts, because the removal no longer supersedes them.

**Trigger B: regression from b2b4f77**

Steps:
1. Viewing Ollama (active), edit the Ollama Base URL. Then click the custom X tab. `selectAiProvider` flushes the edit, and that write fails.
2. Click **Remove provider**. It succeeds.
3. Retire the owner: the Settings view remounts, for example when the native Settings window is re-targeted to another tab (`key={settingsTab}`).
4. Close is blocked, and the user clicks **Retry saved changes**, which runs `retryRetiredSettingsOwners`.

Results:
- **b2b4f77:**
  - After removal: `customProviders: []`, Ollama URL still the old one, and live close is blocked.
  - After retry: `customProviders: [{id: 'custom:x', …}]`, and `providerModels` contains `custom:x` again.
  - So the deleted provider reappears in Settings and the picker, with no config.
- **fd8c356:**
  - The removal persisted the new Ollama URL and cleared the failure. Close is OK and the retry is a no-op. The probe passes.

**Trigger C: pre-existing, identical on both bases**

Steps:
1. **+ Add custom provider** "Zed" → **Create**. The immediate write fails, and every provider key is recorded as failed with the add retry.
2. **Remove provider** succeeds. Close is blocked.
3. Retire the owner, then **Retry saved changes**.

Result: `custom:zed` is re-added *and made the active provider and the chat override* (`provider: custom:zed`, chat `custom:zed`).

**Impact:**
- A provider the user deleted comes back, and in C it becomes the active chat provider.
- This is the same "captured snapshot overwrites newer intent on retired retry" class that the parent fixed for removal retries.
- It needs a write failure, a removal, a remount and an explicit retry, so P3.

**Fix (verified on a scratch copy):**
- Write a draft's *first* enqueue revision into its retry closure: `retry: () => writeAiPatch(patch, scope, origin ?? revision)`.
- In `writeAiPatch`, for non-removal writes, treat a custom id as gone when all of these hold:
  - it is not in `current.customProviders`, and
  - `hasNewerSettingsSave(id, origin ?? revision)` is true.
- For each gone id:
  - drop it from `customProviders`, `providerConfigs`, `providerModels` and `selections`;
  - if `patch.provider` is gone, drop `provider` and `model` and skip the activation and chat block.
- This keeps the peer's own draft (in B, the new Ollama URL is persisted on retry).
- It does not block a later legitimate re-add with the same id, because the re-add's revision is newer than the removal.
- Scratch copy: `/private/tmp/tezbar-runtime-probe-dacc2e0-fixcheck` (b2b4f77 plus a sketch of about 30 lines).
  - Diff: `/private/tmp/tezbar-runtime-probe-dacc2e0-fixcheck.diff`.
  - `SettingsView.knowledge.test.ts` plus `settingsLifecycle.bridge.test.ts`: **121/121 passed**. That covers every repo case and every PROBEDAC probe, with B and C now clean.
  - `tsc --noEmit -p tsconfig.json`: exit 0.
  - Logs: `/private/tmp/tezbar-runtime-probe-dacc2e0-fixcheck.log` and `/private/tmp/tezbar-runtime-probe-dacc2e0-fixcheck-tsc.log`.
  - Other designs are fine too, for example lifecycle-level removal tombstones, as long as they drop removed ids from replayed snapshots without discarding the peer's own draft.
- **Recommended permanent regressions:** B and C as written in `/private/tmp/claude-501/-Users-almat-Desktop-code-Tezbar-APP/a35f47ec-6b02-46b6-8da5-c02309555253/scratchpad/probes.ts`. They assert that X or Zed is absent from `customProviders`, `providerModels`, `providerConfigs` and `providerSelectedModels`, that provider and chat stay Ollama, and, for B, that the peer URL is persisted.

## Observations that do not gate

- **O1 (B, live owner).** Close is now blocked until the user re-enters the failed peer edit, where fd8c356 persisted it through the removal. This matches the documented "retry the edit before closing" contract, so it is not a bug by itself.
  - The synchronous `Removed X.` success message replaces the earlier `AI_SAVE_ERROR` banner. The user then learns of the peer failure only from the bridge's close-blocked alert. It is a UX note.
- **O2 (C, live owner).** After a failed Add followed by Remove, every key in the add's `providerConfigs` stays failed, so a live owner cannot clear it by editing just one provider. This is pre-existing. The T-R3 fix makes the retry path correct, but it does not reduce how many keys the add attributes.

## Evidence

- Snapshots: each is a `git archive` with `node_modules` symlinked, and probes were added only there. The predecessor's `/private/tmp/tezbar-runtime-probe-fd8c356*` files are untouched.
  - `/private/tmp/tezbar-runtime-probe-dacc2e0` (b2b4f77). Its `SettingsView.tsx` is byte-identical to `b2b4f77`.
  - `/private/tmp/tezbar-runtime-probe-dacc2e0-basefd8` (fd8c356)
  - `/private/tmp/tezbar-runtime-probe-dacc2e0-fixcheck` (b2b4f77 plus the T-R3 sketch)
- Logs:
  - `/private/tmp/tezbar-runtime-probe-dacc2e0-probes.log`: b2b4f77, PROBEDAC A1, A2, D and E pass; B and C fail.
  - `/private/tmp/tezbar-runtime-probe-dacc2e0-basefd8-probes.log`: fd8c356, A1, A2, D and C fail; B and E pass.
  - `/private/tmp/tezbar-runtime-probe-dacc2e0-existing.log`: b2b4f77, 2 files, 121 tests. 119 passed, which is all 115 repo cases plus 4 probes. The only 2 failures are B and C.
  - `/private/tmp/tezbar-runtime-probe-dacc2e0-fixcheck.log` and `-fixcheck-tsc.log`: 121/121, tsc exit 0.
- Parent gates, read but not rerun:
  - `/private/tmp/tezbar-provider-final-ts.log`: 933 passed, 9 skipped.
  - `/private/tmp/tezbar-provider-current-typecheck.log`: tsc ran with no errors in the log tail.
  - `/private/tmp/tezbar-provider-final-frontend.log`: built.

## Limitations

- These are renderer fixtures with mocked `getLlmConfig` and `setLlmConfig`, whose merge semantics are `{...config, ...patch}`.
- Owner retirement is simulated with `root.unmount()`. The real trigger is the `key={settingsTab}` remount.
- There was no app launch and no native build; the native source did not change in this range. I did not rerun the full suite.
- Out of scope, as briefed: the wire and standards scopes (closed), and the native architecture.

## Final counts

| ID | Status |
| --- | --- |
| T-R2 (both triggers, with and without failure) | CLOSED |
| Change 1: flush before removal | Correct |
| Change 2: removal owns only the removed id; supersede and clear the error | Correct for its own drafts. It exposes T-R3 trigger B |
| Change 3: removal retry recomputes from current | Correct and immutable |
| Parent-found bugs (failed draft blocks close; peer-old overwrite on retry) | Fixed |
| T-R3 | NEW, P3, actionable. Trigger B is a b2b4f77 regression; trigger C is pre-existing |
| C-R1, C-R2 and other closed IDs | Unchanged, not reopened |

- **Outstanding actionable: 1 (T-R3).** The gate is not passed until replayed failed drafts drop custom providers deleted after them, and the B and C regressions are added.
