# Native AI runtime: Opus 5.5 targeted closure of T-R7 at dd8e8cf

Reviewer: Claude Opus 5.5 (`claude-opus-5-5`). Did the work myself with no delegates. Read-only.

## Pin and scope

- Worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-rust-agent`.
- HEAD, checked before and after: `dd8e8cf750819a3bcd988cf00348bc5588538f85`. Working tree clean (0 porcelain lines) both times.
- Implementation commit: `08a50f8a8f3e622ee1ff45b3b2ad123a879c2d73`, tree `47ec0cb90f04d56cb40fda4a33a0ec6d4b20c7e3` (verified).
- `08a50f8..dd8e8cf` changes only 3 docs: `NATIVE_AGENT_RUNTIME.md`, `NATIVE_AI_VERIFICATION.md` and `reviews/NATIVE_AI_REVIEW_FIXES.md`. `git diff --quiet 08a50f8 HEAD -- src src-tauri native scripts` reports them identical.
- `8e12f1a..08a50f8` code diff:
  - `src/renderer/SettingsView.tsx`: only the body of `startDeviceSignIn`, +6/−5 lines.
  - `src/renderer/SettingsView.knowledge.test.ts`: +76 lines (4 cases).
  - The rest is the review doc.
  - The implementation matches the verified sketch `/private/tmp/tezbar-runtime-probe-8e12f1a-fixcheck.diff` byte for byte, apart from the comment wording.
- In scope: T-R7 (with P12 and P13), the changed paths, and regressions from the new pre-sign-in save.
- Not reopened: the native architecture, O-A, O-B and O-C, the closed wire and standards scopes, and the closed runtime IDs R/U/N-R/C-R and T-R1..6. Nothing I found gives new functional evidence against any of them.

## Search method

- XERJ first, as instructed: the server on `127.0.0.1:9200` was up, and `xerj def --prefix ax-65b00e69da24 persistAiSettings` answered.
- It pointed to `SettingsView.tsx:1404`, while the live line is 1389. So the index is stale, as expected. I did not repair it.
- After that I used `rg` and `sed` on the known paths. Every line cited below was checked against the current files.

## Trace of the changed flow (`src/renderer/SettingsView.tsx`)

1. **`startDeviceSignIn` (1103–1153)**
   - `persistAiSettings(true)` → `writeAiPatch(buildAiProviderPatch(pending), pending)` runs synchronously and sets `aiWrites.current` to *this* write's `promise.catch(...)` (1594).
   - The `await` therefore waits for this write, including its success or failure bookkeeping, and also for every earlier entry in the shared window queue (`settingsLifecycle.ts:19-37`).
2. **No stale pending state for another provider**
   - Every provider switch calls `flushAiSave()` first (985), and every immediate call clears `pendingAiOverrides`.
   - So the overrides still pending when the user clicks Sign in can only be debounced Copilot edits. They are folded into this one write (probe Q4).
3. **`buildAiProviderPatch()` with no overrides (1037–1101)**
   - `providerConfigs = {copilot}`. `providerModels` and `customProviders` are full UI snapshots, and there are no selection or activation keys.
   - `writeAiPatch` (1435–1607) re-reads `current` when the write runs:
     - `providerConfigs` becomes `{...current, copilot}`.
     - `providerModels` becomes `current` plus the owned `copilot` list only (1529–1533).
     - `customProviders` becomes `current` plus owned ids only. `copilot` is not custom, so the list is exactly `current` (1534–1544).
     - Without `model` or `provider` overrides, nothing touches `provider`, `model`, `providerSelectedModels` or `task*Overrides`.
   - This closes the T-R7 data loss and the P12 dangling active/chat provider. It also means sign-in cannot bring back a provider that was removed externally (Q3).
4. **Failure handling**
   - `providers = ['copilot']`. A throw, or `ok:false` (1578), puts `copilot` into `failedAiProviders` and sets `AI_SAVE_ERROR`.
   - `startDeviceSignIn` then returns before OAuth starts, and `finally` clears `deviceBusy`.
   - Clicking again creates a newer revision. When that write succeeds it removes `copilot` from the failed set (1584–1587), so the close drain no longer throws (Q5).
5. **Legacy client ID**
   - The direct `setLlmConfig({githubOAuthClientId: cid})` runs only after the provider write has settled.
   - Native `llm-config-set` (`src-tauri/src/native_core/ai/mod.rs:621-680`) ends in `ConfigStore::patch`, which is a shallow top-level insert under a transaction lock (`common.rs:166-191`).
   - A scalar-only patch has no `provider` key, so the legacy-migration branch (638–664) is skipped. It cannot change `providerConfigs`, `providerModels`, `customProviders` or the selection.
   - The fixture's `{...config, ...patch}` behaves the same way as native here.
   - If this write throws, the outer `catch` shows the error and OAuth does not start (Q6). That is the same as the old behaviour.
6. **Successful poll**
   - Native `persist_tokens` (`auth.rs:125+`) merges into `providerConfigs.copilot` through `config.update`.
   - The UI then reloads `providerConfigs` and the token. No later drain writes the stale token back over it (Q8).
7. **Other provider-data writers checked**
   - `buildAiProviderPatch` now reaches disk only through `writeAiPatch` (from 1422 and from `flushAiSave` at 1612). No direct caller is left.
   - The other direct `setLlmConfig` calls in SettingsView write scalars only: hotkey, thinking, header timeout, retention, memory, onboarding flag and mode timeouts.
   - `CommandBar`, `AgentChatView` and `OnboardingView` write full-snapshot spreads, not a narrowed owned map. So they are not the T-R7 class, and 08a50f8 does not touch them.
   - `ProvidersView.tsx` has no importer anywhere in `src`, so it is dead code. 08a50f8 does not touch it either.

## Per-ID verdict

| ID | Verdict | Evidence |
|---|---|---|
| **T-R7** (P1, the device sign-in write replaced `providerConfigs` with only `{copilot}`) | **CLOSED** | Trace steps 1–3. Permanent cases `existing` and `late-provider` pass. Probes Q1, Q2, Q4, Q7, Q8 and Q9 pass at HEAD and fail at 8e12f1a |
| **P13** (minimal repro: Ollama and custom X credentials survive) | **CLOSED** | Q1 checks deep equality of *all* non-Copilot state before and after, and finds exactly one provider write whose keys are `[copilot, custom:x, ollama]`. It fails at 8e12f1a with `providerConfigs: {}` |
| **P12** (T-R6 class at this callsite: a late W with active/chat must be kept and must not dangle) | **CLOSED** | The permanent `late-provider` case (with a client ID) and Q2 (without a client ID) both pass. Q2 asserts that the active and chat ids exist in `customProviders` |
| The new pre-sign-in save (regression check) | **No regression** | Q3: no resurrection. Q4: the debounce is flushed into one write, and that write happens before `start`. Q5: retry works and the drain returns `ok`. Q6: a failed client-ID write stops OAuth. Q7: the order is provider write → `{githubOAuthClientId}` → `start(cid-1)`, with the ID trimmed. Q9: the selection is untouched when Copilot is active |
| Changed test file | **OK** | The 4 permanent cases pass and assert the right invariants: peers kept, OAuth gated on both kinds of save failure, client ID passed and saved |

**Actionable findings: 0.**

## Non-gating observations (no fix requested)

- **NG-1:**
  - `persistAiSettings` increments `aiEditRevision`.
  - A Copilot model discovery that is still in flight when the user clicks Sign in therefore drops its result (`loadAiModels`, line 730 guard).
  - The old direct write did not increment it.
  - This guard is on purpose and applies to every AI edit. Discovery without a token usually returns nothing anyway, and the user can run discovery again.
- **NG-2 (pre-existing):**
  - The Copilot token `TextArea` (2496–2500) does not autosave. Its value is saved by Sign in or Save.
  - If the user edits Copilot fields *while polling*, the stale token in the UI could overwrite a token the poll just stored.
  - 08a50f8 did not change this, and old code had the same exposure. The narrower new write does not widen it.
- **NG-3 (pre-existing):** `ProvidersView.tsx` is unused dead code that still contains direct scalar writes.
- **O-A, O-B and O-C:** unchanged and not reopened.

## Probes

All probes run only in archive snapshots outside the repo, with `node_modules` symlinked. They use MockIPC through the file's own `draftFixture` and `mountDraftSettings` helpers. They do no native compile, launch no app, and use no live provider, MCP, sign-in or credentials.

| Artifact | Contents | Result |
|---|---|---|
| `/private/tmp/tezbar-runtime-probe-dd8e8cf-probes.ts` | 9 probes, Q1–Q9 | — |
| `/private/tmp/tezbar-runtime-probe-dd8e8cf` | `git archive dd8e8cf` plus the probes. `SettingsView.tsx` is byte-identical to HEAD | — |
| `/private/tmp/tezbar-runtime-probe-dd8e8cf-probes.log` | HEAD: the 9 probes plus the 4 permanent cases | **13/13 pass** |
| `/private/tmp/tezbar-runtime-probe-dd8e8cf-full.log` | HEAD plus probes, full `vitest run` | **960 passed, 9 skipped (existing), 0 failed**. 125 files pass, 2 skipped. 960 = the parent's 951 + 9 probes |
| `/private/tmp/tezbar-runtime-probe-dd8e8cf-red` and `-red.stderr.log` | `git archive 8e12f1a` plus the same probes | **9/9 RED** |

Why each probe fails at 8e12f1a:

- **Q1, Q2:** `providerConfigs: {}`.
- **Q3:** `custom:x` is brought back.
- **Q4, Q8, Q9:** `ollama.baseURL` is undefined.
- **Q5:** no `Could not save` message, because the old flow had no save gate.
- **Q6:** `start` is called.
- **Q7:** the log shows `set:provider[copilot]`.

## Limits

- I relied on the parent's results for typecheck, `build:frontend` and native/Mac/Win tests, and did not rerun them. They apply because the patch is UI-only and native code is byte-identical.
- The fixture's `setLlmConfig` is a shallow merge. I checked that it matches native `ConfigStore::patch` for the patch shapes used here. I did not exercise real IPC or concurrency between processes.
- XERJ results were stale (index not repaired). The final line references come from `rg` and `sed` on the current files.
- No repo source or docs were edited. No worktree, branch, push, PR, Actions run or release was created.

## Result

**T-R7, including P12 and P13, and all changed paths in 8e12f1a..dd8e8cf: CLOSED, 0 actionable.** The parent can combine this with the earlier closed wire, standards and runtime scopes.
