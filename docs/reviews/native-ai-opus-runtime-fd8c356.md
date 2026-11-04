# Native AI runtime targeted closure review at fd8c356: T-R1 (residual C-R3) and touched Settings selection paths

- Reviewer: Claude Opus 5.5 (`claude-opus-5-5`). I did the review myself, with no subagents or delegates.
  - I made no repo edits, docs, branches, checkouts or worktrees.
  - I did not push, open a PR, launch the app, or use live providers, credentials or MCP.
- Worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-rust-agent`.
- Pinned HEAD: `fd8c35648c9b59e4c8e52d149eb911ba852fc355`. HEAD was the same at the start and at the end, and `git status --porcelain` printed 0 lines both times.
- Tested source: `756072ee682eae773626b3d1b78d807217014580`.
  - Its tree is `0147dc5caa1e3dfbe94ab2214c5f7e8b808994a0`, which matches the brief.
  - HEAD's tree is `200d14ee…`. `git diff --stat 756072e fd8c356` touches only `docs/NATIVE_AI_VERIFICATION.md` and `docs/reviews/NATIVE_AI_REVIEW_FIXES.md`.
- Diff reviewed: `847ea62..756072e` under `src/renderer/`, which is `SettingsView.tsx` (+11/−12) and `SettingsView.knowledge.test.ts` (+61). The Rust ext changes belong to the standards reviewer.
- What I read:
  - In full: `docs/reviews/native-ai-opus-runtime-847ea62.md`.
  - The `847ea62` follow-up section of `NATIVE_AI_REVIEW_FIXES.md`.
  - In current source: `writeAiPatch`, `buildAiProviderPatch`, `persistAiSettings`, `flushAiSave`, `addAiModel`, `removeAiModel`, the model-row click, `selectAiProvider`, `addCustomProvider`, `removeCustomProvider`, activation (`persistAiSettings(true, { activeProvider })`), the retired-owner retry and `settleAiSaves`.
- Search: I made no XERJ attempt, because the index is known to be incomplete. I used `rg` and direct paths only.

## Verdict

- **T-R1: CLOSED.** It is probe-confirmed on both of its original triggers, and on the captured-retry smell that was noted alongside it.
- **New regressions from `756072e` in this scope: 0.**
- **New confirmed finding: 1 × P3, actionable (T-R2).**
  - It is **pre-existing**: it fails identically at `847ea62`, so `756072e` did not introduce it.
  - It is in the same class as T-R1 (an explicit model choice or model removal combined in one debounce with another removal). Here the other removal is a custom-provider removal, and the selected-provider map is wrong.
  - It is on the touched path: in `writeAiPatch`, the changed `selections` block runs after the `removedProvider` branch and overrides it.
- **No-outstanding gate: NOT passed** until T-R2 is fixed. A one-line fix exists and I verified it on a scratch copy (see below).

## T-R1: closed

**What changed** (`SettingsView.tsx`, `writeAiPatch`, around lines 1442-1494):
1. `selections = { ...patch.providerSelectedModels }` is a fresh map.
2. For each provider with removed models, the effective choice is `selections[p] ?? current.providerSelectedModels[p] ?? (current.provider === p ? current.model : undefined)`.
3. It falls back only if that effective choice is one of the removed IDs. The fallback is the first non-removed entry in `patch.providerModels[p]`, else the recommended model.
4. The fallback is written into `selections`, never into `patch`.
5. `next.providerSelectedModels`, `next.model` and the chat override come from `selections`.
6. On activation, `next.model = selections[patch.provider] ?? next.model`, and the chat model is set to `next.model`.

**Call flow I checked:**
- **Explicit choice:**
  - A model-row click is an immediate `persistAiSettings(true, { model })`.
  - Add model is debounced: `persistAiSettings(false, { model })`.
  - `pendingAiOverrides` merges `model` together with the accumulated `removedModels`.
  - `buildAiProviderPatch` emits `providerSelectedModels = { [provider]: model }` only when `overrides.model !== undefined`.
- **Explicit choice wins unless it was removed:**
  - Remove A, then click D: the explicit D survives.
  - Add Z, then remove Z: Z is removed, so the code falls back to the first remaining model. That is A in the fixture, and it equals the UI fallback in `removeAiModel` (`normalized[0]`), so the UI and the persisted config agree.
  - Removal only: the code still uses the persisted selection, so the external-choice test (persisted C survives removing A) still passes.
- **Ordering and idempotence:**
  - The loop runs once per `removedModels` entry, so the same provider can be visited more than once.
  - After the first fallback, `selections[p]` is no longer a removed ID, so later visits change nothing.
  - Providers whose models were not removed are never touched.
- **Activation:**
  - `addCustomProvider` passes `model: ''`, which becomes `selections[id] = recommended`. `patch.model` resolves to the same value.
  - Activation of the viewed provider while a removal is pending: the fallback in `selections` equals the UI's `aiModel` fallback, which equals `patch.model`.
  - The new `selections[patch.provider] ?? next.model` also stops an earlier `selections[current.provider]` from overriding `next.model` for the newly active provider.
- **Captured-retry immutability:**
  - `rg` over the body of `writeAiPatch` finds no assignment to `patch.*` or `scope.*`.
  - `next` is a shallow copy with its own `providerConfigs` object, so the `delete` only touches the copy.
  - The retry closure `() => writeAiPatch(patch, scope)` therefore recomputes the result from the current persisted state each time.
- **Original failures re-tested:**
  - Both of the new permanent repo cases pass: choose-after-remove gives D, and remove-after-add gives A. Each asserts the selected model, `model`, chat, and the remaining list.
  - The existing rapid-removal and conditional-fallback cases also pass.

**Independent probes** (snapshot `/private/tmp/tezbar-runtime-probe-fd8c356`, which is a `git archive 756072e`):

The PROBEFD8-R cases make the first `setLlmConfig` fail, unmount Settings (retiring its owner), optionally change the persisted state, then call `retryRetiredSettingsOwners()`.

| Case | 756072e | 847ea62 base |
| --- | --- | --- |
| explicit-retry (remove A, click D, first save fails, retry) | **D/D/D** (selected/model/chat) | B/B/B (T-R1 revert, and the retry replays the reverted value) |
| removal-retry-external (remove A, first save fails, external C, retry) | **C/C/C**: recomputed; the retry's patch carries no selection | B/B/B: the captured fallback overwrote C |
| removal-retry (control) | B/B/B | B/B/B |

**Conclusion:** T-R1 and the captured-retry smell are both closed, and their regressions are permanent.

## T-R2 (new, P3, probe-confirmed, actionable, pre-existing): custom-provider removal combined in one debounce with a pending model add or removal persists the deleted provider's model as the active chat model

**Where:**
- `SettingsView.tsx` `removeCustomProvider` (around line 1321) does not flush the pending debounced AI save before it calls `persistAiSettings(true, { provider: nextCurrent, removedProvider, … })`.
- Two things in the pending overrides are then merged into the removal write:
  1. **A stale `model` override from Add model.** `buildAiProviderPatch` re-targets it to `overrides.provider`, which is the fallback provider (`ollama`). That produces `selections = { ollama: <deleted provider's model> }`.
  2. **Pending `removedModels` for the deleted provider.** The fallback loop sets `selections['custom:x']`.
- In `writeAiPatch`, the `selections` block runs **after** the `removedProvider` branch. It does three things:
  - It overwrites `next.providerSelectedModels` with `{ ...current.providerSelectedModels, ...selections }`, which re-adds the deleted provider's key.
  - It sets `next.model` from `selections[current.provider]`.
  - It sets the chat override from `selections[current chat provider]`.

  Together these undo the safe `ollama` values that the removal branch had just written.
- **Contrast with `selectAiProvider`:** it already calls `flushAiSave()` first, so a provider switch is not affected.

**Triggers** (two clicks within 300 ms, the same timing class as T-R1):
1. Active provider is `ollama` (`llama-o`). On the custom provider X's tab, add model `x-z`, then click **Remove provider**.
   - Persisted: `{provider: ollama, model: x-z, selected: {ollama: x-z, custom:x: x-a}, chat: ollama/x-z}`.
2. Custom provider X is active, with `x-a` selected. Remove `x-a`, then click **Remove provider**.
   - Persisted: `{provider: ollama, model: x-b, selected: {ollama: llama-o, custom:x: x-b}, chat: ollama/x-b}`.

**Control:** with no pending edit, Remove provider persists `{ollama, llama-o, selected: {ollama: llama-o}, chat: ollama/llama-o}`, which is correct.

**Impact:**
- Settings shows Ollama with `llama-o`, but the active model and the chat override are a model ID from the deleted provider, and those are sent to Ollama.
- In trigger 1, the persisted `providerSelectedModels.ollama` is also wrong, and a stale key for the deleted provider is left behind.
- This is the C-R3/T-R1 kind of mismatch. It lasts until the next model selection.

**Provenance:**
- `/private/tmp/tezbar-runtime-probe-fd8c356-base847.log` shows identical failures at `847ea62`. So this is not a `756072e` regression.
- The new `selections[patch.provider]` line does not affect it, because the removal path sets no `patch.provider`.
- I did not probe earlier bases.

**Minimal fix (verified):**
- Add `flushAiSave()` as the first statement of `removeCustomProvider`, which mirrors `selectAiProvider`.
- The pending X edit is then queued and written for X first, with its correct target. The removal write that follows carries no stale `model` and no `removedModels`.
- Result on the scratch copy `/private/tmp/tezbar-runtime-probe-fd8c356-fixcheck`, which is the snapshot plus that one line:
  - All three PROBEFD8-CP cases persist `{ollama, llama-o, selected: {ollama: llama-o}, chat: ollama/llama-o}`.
  - The whole `SettingsView.knowledge.test.ts` file passes: 115/115, including every repo case and every PROBEFD8 case.
  - Log: `/private/tmp/tezbar-runtime-probe-fd8c356-fixcheck.log`.

**Recommended permanent regressions:** add the PROBEFD8-CP add-then-remove-provider and remove-selected-then-remove-active-provider cases. They assert that `provider`, `model`, `providerSelectedModels.ollama`, chat provider and chat model all equal the fallback, and that the deleted provider's key is gone.

**Optional hardening (judgment, not required):** in `writeAiPatch`, drop `selections[scope.removedProvider]`, and let the `removedProvider` branch win over `selections`.

## Touched-path regression sweep (no new regressions)

- **Provider switch** (`selectAiProvider`) flushes before switching, so pending `model` and `removedModels` never cross providers on that path.
- **Other providers' selections** are untouched. `selections` holds only the explicit provider and the providers with removed models. The merge onto `current.providerSelectedModels` keeps external entries, as before.
- **Activation without an explicit model** while a removal is pending: the fallback equals the UI's `aiModel`, which equals `patch.model`, so the result is unchanged from `847ea62`.
- **External chat-selection ownership:** removing a model that is not the persisted selection never writes `model` or chat, because `selections` is empty. The repo's external-choice test passes.
- **Retired Settings, Retry and native close:** `failedAiWrites` bookkeeping, `settleAiSaves` and the bridge are unchanged.
  - The retry now recomputes its result from the current persisted state (probe above).
  - `SettingsView.knowledge.test.ts` and `settingsLifecycle.bridge.test.ts` at `756072e`: 114 passed. The only 2 failures are my T-R2 probes. Log: `/private/tmp/tezbar-runtime-probe-fd8c356-existing.log`.
- **C-R1, C-R2 and window lifecycle:** `lib.rs`, `tauri-bridge.ts`, `settingsLifecycle.ts`, `App.tsx` and `AgentChatView.tsx` are unchanged in `847ea62..756072e`. I found no new evidence to reopen them.

## Evidence files

- Snapshots, each a `git archive` with `node_modules` symlinked and with probes added only there:
  - `/private/tmp/tezbar-runtime-probe-fd8c356` (756072e)
  - `/private/tmp/tezbar-runtime-probe-fd8c356-base847` (847ea62)
  - `/private/tmp/tezbar-runtime-probe-fd8c356-fixcheck` (756072e plus the one-line fix)
- The probe source is also at `/private/tmp/claude-501/-Users-almat-Desktop-code-Tezbar-APP/06883919-73c4-4ff3-9bd3-0eccb5f16cac/scratchpad/probes.ts`.
- Logs:
  - `/private/tmp/tezbar-runtime-probe-fd8c356-probes.log`: on 756072e, the repo T-R1 cases, the R probes and the CP control pass; CP 1 and CP 2 fail (T-R2).
  - `/private/tmp/tezbar-runtime-probe-fd8c356-base847.log`: on 847ea62, R explicit and R external fail (T-R1); CP 1 and CP 2 fail identically.
  - `/private/tmp/tezbar-runtime-probe-fd8c356-existing.log`: 114 passed, 2 failed (the T-R2 probes only).
  - `/private/tmp/tezbar-runtime-probe-fd8c356-fixcheck.log`: 115/115 with the fix.

## Coverage and limitations

- These are renderer fixtures with mocked `getLlmConfig` and `setLlmConfig`. They do not observe the live backend or OS windows. No app launch, no cargo build, and no full suite rerun; I accepted the parent's exact-source gates as reported.
- Out of scope: the Rust ext and S8/S5 changes (standards reviewer), wire (already closed and unchanged), and whole-architecture questions.
- External changes to the persisted config while Settings is open remain an unreachable-in-practice hypothesis, as stated in the 847 report. Probes that use them only exercise retry recomputation.

## Final counts

| Item | Status |
| --- | --- |
| T-R1 | CLOSED |
| New regressions from 756072e | 0 |
| T-R2 | NEW, P3, actionable, pre-existing (identical at 847ea62) |

- **Outstanding actionable: 1 (T-R2).**
- **Gate: not passed** until `removeCustomProvider` flushes the pending AI save first and the two CP regressions are added.
