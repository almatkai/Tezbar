# Native AI runtime targeted review at a0b3922: T-R4, T-R5 and changed Settings/shared-queue ownership

- Reviewer: Claude Opus 5.5 (`claude-opus-5-5`). I did the audit myself, with no subagents or delegates.
- Worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-rust-agent`. It was read-only.
  - I made no repo edits and created no branches, checkouts or worktrees.
  - I did not push, open a PR, run Actions, launch the app, use live models or MCP, or read credentials or profiles.
  - Only mock IPC fixtures were used.

## Pin and scope

| Check | Start | End |
| --- | --- | --- |
| `git rev-parse HEAD` | `a0b3922c8654047ca6cb3ead9ddfb23de7878568` | same |
| `git status --porcelain` | 0 lines | 0 lines |
| Branch | `feat/native-rust-agent` | same |
| `f5f8cd0^{tree}` | `1792f00d77a88e03c59236b99caecd80cd849f30` (matches brief) | — |

- `f5f8cd0..a0b3922` touches only `docs/NATIVE_AI_VERIFICATION.md` and `docs/reviews/NATIVE_AI_REVIEW_FIXES.md`.
- `509f53b..a0b3922` changes only these source files: `SettingsView.tsx`, `settingsLifecycle.ts` and `SettingsView.knowledge.test.ts`.
- `native/` is unchanged since 756072e (`git diff --quiet`). `src/renderer/tauri-bridge.ts` (the close/Retry loop) is unchanged since 509f53b.
- `enqueueSettingsWrite` has only 2 production callers: `writeAiPatch`, and model discovery, which uses the default arguments.
- What I owned: T-R4, T-R5, and regressions in the changed Settings retry/ownership code and shared-queue revision code.
- Not reopened: wire, standards/security/MCP, the earlier runtime IDs, and the native architecture.
- Search: XERJ is known to be incomplete or locked for this tree, so I did not use it. I used `rg` on the known paths.

## What I read and traced

- `docs/reviews/native-ai-opus-runtime-509f53b.md`, in full.
- The full `509f53b..f5f8cd0` diff.
- `settingsLifecycle.ts`, in full.
- In `SettingsView.tsx`:
  - `writeAiPatch`, including the success and failure handlers
  - `buildAiProviderPatch`, `persistAiSettings`, `flushAiSave`, `selectAiProvider`, `addCustomProvider`, `removeCustomProvider`
  - the "Use this provider", protocol and model-protocol callers
  - `loadAiModels` (the discovery enqueue)
  - `pruneSupersededAiFailures`, `settleAiSaves`, the owner retry/drain registration and the close hook
- `tauri-bridge.ts`: `closeSettings` and the Retry button.

## How the new code works

- **Shared record (T-R4).** A failed write now creates one record `{revision: first, retry}`, and every key it owns points to that record.
  - A key is not overwritten when its existing record has a newer revision.
  - The owner retry's `Set` therefore collapses to one replay per failed write.
  - Failed retries keep their original logical revision.
- **Logical revision.**
  - A retry calls `enqueueSettingsWrite(work, providers, origin)`, which returns `revision = origin` and does not advance the counter.
  - Saved stamps use `max`, so a retry that finishes later cannot lower a newer stamp, and cannot raise a stamp above the retry's own intent order.
  - First writes still get `++counter`, which is above every stamp, so `gone()` and `stale()` are always false for them.
- **Success guard.** The handler skips clearing a key whose current record is newer than this write's revision. First writes always clear, as before.
- **Replay merge (T-R5).** It runs only when `origin !== undefined` and the write is not a removal.
  - Owned keys come from `patch.providerConfigs`. Stale owned configs take current's value.
  - Models and definitions start from current; only owned entries that are neither stale nor gone are upserted.
  - Selections for stale ids are dropped.
- **Activation.** It owns the pseudo-key `@active`. `activate` requires the target to be not gone and `@active` to be not stale.
- **Removal path:** unchanged.

## Per-ID verdicts

| ID | Verdict | Evidence |
| --- | --- | --- |
| **T-R4** (failed round drops pending Remove; duplicate per-key closures) | **CLOSED** | PROBE509-R1[0], R1[1] pass at f5f8cd0, R1[1] fails at 509f53b. Repo regression asserts exactly 2 writes. PROBEA0B-N9 (same owner, newer URL failure over an older Create, 1 failed round) passes with 2 writes; at 509f53b it uses 3 writes and loses the URL |
| **T-R5** L2 (newer Yak definition/models/active/chat) | **CLOSED** | PROBE509-L2 passes at f5f8cd0, fails at 509f53b |
| **T-R5** L3 (newer peer URL) | **CLOSED** | PROBE509-L3 passes at f5f8cd0, fails at 509f53b |
| **T-R5** L4 (newer activation) | **CLOSED** | PROBE509-L4 passes at f5f8cd0, fails at 509f53b |
| Cross-owner: an old retry must not prune a newer live failure | **CLOSED** | Repo regression passes. PROBEA0B-N4: the live owner's newer failed URL keeps blocking, then applies `:22222` on its own retry, and Zed is kept. At 509f53b the drain wrongly resolves and the URL is lost |
| T-R3 B/C, L1 non-regression | **Still CLOSED** | PROBE509-B×3, C×4 and L1 all pass at f5f8cd0 |
| Changed shared-queue API (`origin` argument, `max` stamps) | Correct | Trace above. Discovery is unchanged. Physical queue order is still FIFO |
| Close and Retry lifecycle (bridge, native close) | Unchanged | Byte-identical. The full suite's close/bridge fixtures are green |
| **T-R6 (NEW)**: a live first write clobbers definitions a retired retry just wrote | **OPEN, P3, actionable** | PROBEA0B-N2 and N2a fail at f5f8cd0. N2 also fails at 509f53b, so the class is pre-existing. N2a is a new path, opened by the cross-owner fix |

## T-R6 (NEW, P3, actionable): a live owner's first write deletes a definition that a retired retry just created, leaving the active provider dangling

### Steps (PROBEA0B-N2a)

Base fixture: Ollama is active and custom X exists.

1. Owner A: **Create** "Zed". The write fails.
2. Retire A (remount).
3. Owner B mounts, with a UI snapshot that has no Zed.
4. In B, edit the Ollama URL to `:22222`. The write fails.
5. Close is blocked. Click **Retry saved changes**:
   - A's retry succeeds, writing the Zed definition, its models, and `provider`/chat set to `custom:zed`.
   - B's newer failure correctly survives (`d = "Settings save failed"`). This is the f5f8cd0 cross-owner fix working as intended.
6. Following the alert's "Retry the edit", the user re-edits the Ollama URL in B to `:44444`. The write succeeds, and drain resolves, so the window can close.

### Result at f5f8cd0

- `customProviders` is now `[custom:x]`, and `providerModels` no longer has `custom:zed`.
- `provider`, `taskProviderOverrides.chat` and `providerConfigs['custom:zed']` all still point at Zed.
- So the active and chat provider point at a definition that was deleted. No error is shown.

N2 hits the same outcome through a different B failure: B's **Create** "Yak" fails, then B makes a successful Ollama edit. N2 fails identically at 509f53b, so the class is pre-existing. At 509f53b, the N2a trigger was hidden because A's retry wrongly pruned B's failure and closed the window.

### Root cause

- The T-R5 merge, which upserts definitions and models into current by owned id, is gated on `replay` (`SettingsView.tsx:1522–1524`).
- First (live) writes still send `patch.customProviders` and `patch.providerModels` as whole snapshots. Both are built from the live owner's UI state, captured at mount (`buildAiProviderPatch`, `SettingsView.tsx:1036–1097`).
- A retired owner's successful retry changes disk after that snapshot was taken. The live owner never reloads.
- So its next unrelated edit overwrites definitions and models it does not own. It does not touch activation, which is why `provider` is left dangling.

This is the same contract violation as T-R5, with the direction reversed: a newer write's stale, *unowned* fields replace newer disk state.

### Fix (verified on a scratch copy)

- Apply the owned-id merge for definitions and models to every non-removal write.
- Keep the stale-config and stale-selection logic for replays only.
  - For first writes, `stale()` is already false, so only the merge changes.
- One-line change, as in the sketch:

```diff
-        if (replay) {
+        // Live first writes also own only their provider keys: their UI snapshot
+        // may predate definitions a retired owner's retry has since written.
+        if (!scope?.removedProvider) {
```

- **Why this is safe:**
  - No non-removal caller intends to delete a definition. Removal uses its own scoped path.
  - Every non-removal caller that edits a definition or a model list owns that provider's config key: Create owns all keys, and protocol, model and capability edits own `aiProvider`.
  - `selectAiProvider` and `removeCustomProvider` both flush the debounce first, so a pending edit cannot be built under the wrong `aiProvider`.
- **Diff:** `/private/tmp/tezbar-runtime-probe-a0b3922-fixcheck.diff`
- **Results on `/private/tmp/tezbar-runtime-probe-a0b3922-fixcheck`:**
  - all 21 probes pass
  - full `vitest run`: **966 passed, 9 skipped, 0 failed**, with 125 files passed and 2 skipped. That is 945 repo tests plus 21 probes.
  - `tsc --noEmit -p tsconfig.json`: exit 0
- **Recommended permanent regression:** N2a, which is the natural recovery path. Optionally add N2 too. Each one asserts that a custom `provider` is still present in `customProviders`.
- An alternative fix is to reload live owners' AI state after a successful `retryRetiredSettingsOwners`. It is broader and does not cover N2 if B edits before its own retry, so the merge is preferable.

## Non-gating observations

- **O-A: logical intent order changes one cross-owner outcome, consistently** (PROBEA0B-N1, N1c).
  - Setup: an older retired **Remove X** fails, then a newer retired **Create Zed** fails. B's Create snapshot owns X, because X was still on disk when B loaded (O1 from dacc2e0: Create owns every key).
  - At f5f8cd0, Retry keeps X. At 509f53b it removed X, because A's retry took a fresh revision.
  - Control N1c: when the newer Create succeeds directly, X is kept at *both* versions.
  - So f5f8cd0 makes retry match direct success, as the "no newer intent superseded by an old retry finishing later" rule requires. The surprise traces back to O1's over-attribution, not to the new ordering.
  - PROBEA0B-N8 (an older URL edit, then a newer Create that owns Ollama) behaves the same way at both versions.
- **O-B: a failed activation now outlives the removal of its target** (PROBEA0B-N3, regression from the `@active` design).
  - Steps, in a live owner: **Use this provider** on X fails, then **Remove provider** X succeeds.
  - At f5f8cd0, drain still rejects, because `@active` holds the failure. The error message stays, and no "Use this provider" button is shown (Ollama is active in the UI).
  - "Retry saved changes" retries only retired owners, so the live owner stays blocked. It recovers after a Settings tab switch and Retry: `gone()` suppresses activation, and the result is `{ok, ok}` with X absent. There is no data loss.
  - At 509f53b, the removal cleared the failure.
  - This is UX-only, in the same class as O2 (a live owner blocked until its failed edit is retried). The predecessor accepted the `@active` trade-off.
  - Optional fix: when a removal succeeds, also delete failed records whose activation target is the removed id. For example, store `target: patch.provider` on the record and filter on it in the removal success branch.
- **O1 and O2 from dacc2e0**: unchanged.
- **O1 from 509f53b** (retry writes default model lists for every provider): fixed for replays as a side effect. Live first writes still send them until T-R6 is applied, and then they do not.

## Evidence (all outside the repo)

- **Snapshots:** `git archive` with `node_modules` symlinked, and probes spliced only into `SettingsView.knowledge.test.ts`.
  - `SettingsView.tsx` and `settingsLifecycle.ts` were checked byte-identical with `cmp` against the f5f8cd0 worktree and against `git show 509f53b:`.
  - `/private/tmp/tezbar-runtime-probe-a0b3922` (f5f8cd0)
  - `/private/tmp/tezbar-runtime-probe-a0b3922-base` (509f53b, whose source is 30905e2)
  - `/private/tmp/tezbar-runtime-probe-a0b3922-fixcheck` (f5f8cd0 plus the T-R6 one-line sketch)
- **Probes:** the predecessor's 13 (`/private/tmp/tezbar-runtime-probe-509f53b-probes.ts`), plus 8 new ones in `/private/tmp/tezbar-runtime-probe-a0b3922-work/new-cases.ts`: N1, N1c, N2, N2a, N3, N4, N8, N9.

| Log | Tree | Result |
| --- | --- | --- |
| `/private/tmp/tezbar-runtime-probe-a0b3922-probes.log` | f5f8cd0 | 19/21 pass. All 13 PROBE509 pass, and N1, N1c, N3, N4, N8, N9 pass. **Fail: N2, N2a** |
| `/private/tmp/tezbar-runtime-probe-a0b3922-base-probes.log` | 509f53b | 13/21 pass. Fail: L2, L3, L4, R1[1], N2, N2a, N4, N9 |
| `/private/tmp/tezbar-runtime-probe-a0b3922-full.log` | f5f8cd0 plus probes, full suite | 964 passed, 2 failed (N2, N2a only), 9 skipped. All 945 repo tests pass |
| `/private/tmp/tezbar-runtime-probe-a0b3922-fixcheck-probes.log` | sketch | 21/21 pass |
| `/private/tmp/tezbar-runtime-probe-a0b3922-fixcheck-full.log` | sketch, full suite | 966 passed, 9 skipped, 0 failed |
| `/private/tmp/tezbar-runtime-probe-a0b3922-fixcheck-tsc.log` | sketch | exit 0 |

- **Parent gates:** I read them as reported (945/9, typecheck, frontend build, diffcheck, native). My run of the repo tests on the f5f8cd0 snapshot independently confirms 945 passing.

## Limits

- The renderer fixtures mock `getLlmConfig` and `setLlmConfig` with a shallow `{...config, ...patch}` merge.
- Owner retirement is simulated with `root.unmount()`. In the app, it is the `key={settingsTab}` remount, and Retry goes through `retryRetiredSettingsOwners`.
- I did not launch the app, build native code or run Rust tests. Native is unchanged since 756072e.

## Final

- **T-R4: CLOSED.**
- **T-R5 (L2/L3/L4): CLOSED.**
- **Cross-owner ordering fix: CLOSED.**
- T-R3 is still closed. The queue API change is correct, and the close/bridge paths are unchanged.
- **The zero-actionable gate is NOT passed. 1 actionable finding remains:**
  - **T-R6 (P3):** a live first write wholesale-overwrites definitions and models, leaving `provider`/chat dangling after a retired retry.
  - The class is pre-existing (N2 fails at 509f53b). The cross-owner fix opened the natural N2a path.
  - The fix is the one-line gate change above, verified with 966/9 and tsc 0. Add N2a as a permanent regression.
- Optionally fold O-B's removal cleanup of `@active` into the same round.
