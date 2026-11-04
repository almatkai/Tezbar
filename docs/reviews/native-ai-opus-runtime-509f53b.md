# Native AI runtime final targeted review at 509f53b: T-R3 and touched Settings save/retry/ownership paths

- Reviewer: Claude Opus 5.5 (`claude-opus-5-5`). I did the audit myself, with no subagents or delegates.
  - I made no repo edits and created no branches, checkouts or worktrees. I did not push, open a PR or run Actions.
  - I did not launch the app or use live providers, MCP, credentials or profiles. Only mock IPC fixtures were used.
- Worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-rust-agent`.

## Pin checks

| Check | Start | End |
| --- | --- | --- |
| `git rev-parse HEAD` | `509f53bf7a390de41195680349404957d2ee1c67` | same |
| `git status --porcelain` | 0 lines | 0 lines |
| Branch | `feat/native-rust-agent` | same |
| `30905e2^{tree}` | `25f8305a0c9593fcbb1e7e8f7f1c41fc29c5deae` (matches brief) | — |

- `git diff --stat 30905e2..509f53b` touches only `docs/NATIVE_AI_VERIFICATION.md` (+40/−1) and `docs/reviews/NATIVE_AI_REVIEW_FIXES.md` (+25).
- `git diff --stat dacc2e0..30905e2` touches only:
  - `src/renderer/SettingsView.tsx` (+44/−3)
  - `src/renderer/SettingsView.knowledge.test.ts` (+161)
  - `docs/reviews/native-ai-opus-runtime-dacc2e0.md` (the predecessor report)
- No other `src`, `native` or `scripts` file changed in `dacc2e0..HEAD`.

## What I read and traced

- Read in full: `docs/reviews/native-ai-opus-runtime-dacc2e0.md`.
- The full `dacc2e0..30905e2` source and test diff.
- In `SettingsView.tsx` at 30905e2:
  - `writeAiPatch` in full (1434–1569)
  - `flushAiSave`, `persistAiSettings`, `buildAiProviderPatch` (1037–1097), `selectAiProvider`, `addCustomProvider`, `removeCustomProvider`
  - the protocol/capability/activation `persistAiSettings` callers (2352, 2418, 2704)
  - `pruneSupersededAiFailures`, `settleAiSaves`, the owner retry (1676–1688) and the unmount flush
- `settingsLifecycle.ts` in full: `enqueueSettingsWrite`, `hasNewerSettingsSave`, `registerSettingsOwner`, `drainSettingsOwners`, `retryRetiredSettingsOwners`.
- Search: I did not attempt XERJ, because its index is known to be incomplete or locked for this tree. I used `rg` on the known paths only.

## Verdict

- **T-R3: CLOSED.**
  - Both triggers B and C are fixed: with no extra failed retries, with 1–2 extra failed retries, and with the Remove failing too.
  - Legitimate recreation is still allowed: same owner, new owner, and the repo's Remove-then-Create case.
  - I confirmed this with independent probes. The same probes fail at dacc2e0.
- **30905e2's changes are correct for what they target.**
  - `delete next.providerSelectedModels`, the `goneIds` filtering (it runs after the `removedModels` fallback), the cloned `providerModels`, the activation suppression and the `origin` carry-through all behave as described.
  - No `patch.*` or `scope.*` is mutated. Removal writes are unaffected.
- **The zero-actionable gate is NOT passed.** There are 2 new actionable findings in the touched retry/ownership path:
  - **T-R4 (P3) is a regression from b2b4f77 and still present at 30905e2.** A pending Remove is silently lost after one more failed retry round. fd8c356 passes this case.
  - **T-R5 (P3) is pre-existing and identical at fd8c356, dacc2e0 and 30905e2.** A replayed older draft overwrites newer successful state with its captured snapshot.
  - I verified a fix for both on a scratch copy. The full suite is green with every probe, and tsc passes.

## Per-ID verdicts

| ID | Status | Evidence |
| --- | --- | --- |
| T-R3 B: peer Ollama draft resurrects removed X | **CLOSED** | PROBE509-B ×3 (0/1/2 extra failed retries) pass at 30905e2; all 3 fail at dacc2e0 |
| T-R3 C: failed Create Zed, then Remove, then Retry restores and activates Zed | **CLOSED** | PROBE509-C ×4 pass at 30905e2. The variants without a Remove failure fail at dacc2e0 |
| Legitimate recreation of the same id | Allowed | PROBE509-L1 (new owner recreates `custom:fixture-x` while a peer draft is retained) passes; the repo's 2 controls pass |
| 30905e2 changes: `origin`, `gone`/`goneIds`, selection delete, ordering, activation suppression | Correct | Code trace plus the above |
| Removal scoped-delete path | Unchanged, correct | Diff shows no change; repo removal-retry cases pass |
| **T-R4** | **NEW, P3, actionable, regression from b2b4f77** | PROBE509-R1[extra=1]: fails at 30905e2 and dacc2e0, passes at fd8c356 |
| **T-R5** | **NEW, P3, actionable, pre-existing** | PROBE509-L2/L3/L4: fail identically at 30905e2, dacc2e0 and fd8c356 |
| Earlier closed IDs (R/U/N-R/C-R/T-R1/T-R2), wire, standards | Not reopened | No changes outside the reviewed Settings diff |

## T-R4 (NEW, P3, actionable): a failed retry round drops a pending provider removal

**Steps** (PROBE509-R1, extra=1; fixture has Ollama active plus existing custom X):
1. **+ Add custom provider** "Zed", then **Create**. The write fails, and every config key (`ollama`, `custom:x`, `custom:zed`) gets the Create retry.
2. Click the **Fixture X** tab, then **Remove provider**. The write fails, and `failedAiWrites['custom:x']` becomes the removal retry.
3. Retire the owner: drain rejects, then the view remounts.
4. **Retry saved changes**, and the writes fail again.
5. **Retry saved changes** again, and the writes succeed.

**Result at 30905e2:**
- The final round sends 3 identical Create writes and no removal.
- `custom:x` stays in `customProviders` and `providerConfigs`.
- Retry and drain both resolve, so close succeeds. Settings reports success, but the user's Remove has been discarded.
- fd8c356 passes, because its removal owned every remaining key. dacc2e0 (b2b4f77) fails the same way.

**Root cause**
- In `writeAiPatch`'s failure handler (`SettingsView.tsx:1556–1560`), the object literal and the `retry` arrow are created *inside* `for (const id of providers)`. So:
  1. **Duplicate writes:** each key gets a distinct closure. The owner retry's `new Set(...retry)` dedup (`:1680`) never collapses them, so a failed N-key Create is replayed N times per round.
  2. **The last failure wins:** each duplicate's failure handler overwrites *every* key it owns. A Create owns the full UI config map, including `custom:x`. A duplicate whose position comes after `custom:x` in map order fails after the removal and replaces the removal's entry.
- The b2b4f77 change that narrowed removal ownership to `[removedProvider]` left the removal holding a single key that the duplicate can overwrite.
- At extra=0 the stale duplicate still runs after the removal. It is harmless only because 30905e2's `gone()` filters it.

**Fix (verified):**
- Create **one** failure record per failed write: `const first = origin ?? revision; const failed = { revision, origin: first, retry: () => writeAiPatch(patch, scope, first) }`.
- Do **not** replace an existing entry whose origin is newer: `const prev = map.get(id); if (prev && prev.origin > first) continue; map.set(id, failed)`.
- Verified alone on `/private/tmp/tezbar-runtime-probe-509f53b-fixcheck-tr4`. Settings and lifecycle suites: 131/134 pass. The only 3 failures are T-R5's L2/L3/L4, and R1 is fixed. The final round becomes 2 writes: Create, then the removal.

## T-R5 (NEW, P3, actionable, pre-existing): a replayed older draft overwrites newer successful state

**Mechanism:**
- `buildAiProviderPatch` always captures the full UI `customProviders` list and the full `providerModels` map.
- A Create also captures every `providerConfigs` entry, plus `provider`/`model`.
- On a retired-owner retry, `writeAiPatch` spreads those over current.
- `hasNewerSettingsSave` supersedes only the failure *entry* of a key that was saved later. The 30905e2 `gone()` covers only custom ids that were deleted.
- Nothing stops a replay that is still alive through some other key from reapplying stale values for:
  - keys a newer write saved
  - custom providers it does not own
  - activation
- This contradicts the `settingsLifecycle.ts` contract: "A successful later edit of that provider supersedes the older failure, without replaying it".

**Reproductions** (each fails identically at 30905e2, dacc2e0 and fd8c356):

- **L2: a provider created by a newer owner disappears, and chat is left pointing at it.**
  1. Start from stored config with no `providerConfigs.ollama`, which is normal for Ollama defaults.
  2. Edit the Ollama URL, then switch to X. The write fails.
  3. Retire the owner.
  4. In the new owner, **Create** "Yak". It succeeds, but it owns only `custom:x` and `custom:yak`.
  5. **Retry saved changes**.
  - Result: `customProviders: [custom:x]`, and `providerModels` no longer has `custom:yak`. Meanwhile `provider` and chat are still `custom:yak`.
  - So the active chat provider refers to a definition that no longer exists. Retry and drain resolve, so this is silent.
- **L3: a newer peer edit is reverted.**
  1. **Create** Zed fails.
  2. On the Ollama tab, the URL edit to `:33333` succeeds.
  3. Retire, then **Retry saved changes**.
  - Result: the Ollama URL is back to `:11434` (from the Create's snapshot).
- **L4: a newer activation is reverted.**
  1. **Create** Zed fails.
  2. On Ollama, **Use this provider** succeeds.
  3. Retire, then **Retry saved changes**.
  - Result: `provider` and chat are back to `custom:zed`.

**Fix guidance (verified on the scratch copy):** for a non-removal *retry* (`origin !== undefined`), rebuild the write from `current`, and apply only what this draft still owns:
- **Configs:** owned keys are `Object.keys(patch.providerConfigs)`. A key is stale when `hasNewerSettingsSave(id, origin)` is true; for those, use current's value.
- **Model lists:** start from `current.providerModels` and copy only the patch entries for keys that are owned, not stale and not gone.
- **Custom definitions:** start from `current.customProviders` and upsert only the patch definitions that are owned, not stale and not gone. Do not replace the list wholesale. No non-removal caller ever intends to delete a definition; deletion always goes through the removal path.
- **Selections:** drop selections for stale ids.
- **Activation:** track it as a pseudo-key, so a newer activation supersedes it.
  - The sketch adds `'@active'` to `providers` when `patch.provider` is set, and skips activation when `hasNewerSettingsSave('@active', origin)` is true.
  - Trade-off: a failed activation can then be cleared only by a later successful activation. Other designs are fine if L4 passes.
- First executions are unchanged, because the sketch gates this on retry only.

**Combined sketch:**
- Diff: `/private/tmp/tezbar-runtime-probe-509f53b-fixcheck.diff` (91 lines, applied to 30905e2).
- Tree: `/private/tmp/tezbar-runtime-probe-509f53b-fixcheck`.
- Settings and lifecycle suites: **134/134**. That is all 121 repo cases, including the 6 new T-R3 regressions, plus all 13 probes.
- **Full `vitest run`: 125 files passed, 2 skipped; 952 passed, 9 skipped, 0 failed.** That is 939 plus 13 probes.
- `tsc --noEmit -p tsconfig.json`: exit 0.
- A first sketch without the T-R4 change failed PROBE509-C[2, remove fails]. Adding the `'@active'` key added a key after `custom:zed`, which exposed the T-R4 overwrite. That is how T-R4 was found, and it confirms the two fixes interact: land T-R4's change with or before T-R5's.

**Recommended permanent regressions:** R1 (extra 0/1), L2, L3 and L4 from `/private/tmp/tezbar-runtime-probe-509f53b-probes.ts`.
- They assert removal intent is kept, newer owner's provider definition and models are kept, newer peer URL is kept, and newer activation and chat are kept.
- Each one failed before the fix.

## Non-gating observations

- **O1:** a successful retry writes the captured `providerModels` snapshot, which includes UI default lists for every built-in provider (`antigravity`, `openai`, …), even when only Ollama was edited.
  - This is harmless data inflation and pre-existing. The T-R5 fix (ownership-filtered models) removes it as a side effect.
- **O2 / O1 from dacc2e0** (close is blocked until a failed peer edit is retried; a failed Add attributes every key): unchanged, still UX-only.

## Evidence (all outside the repo)

- **Snapshots** (`git archive` with `node_modules` symlinked; probes spliced into `SettingsView.knowledge.test.ts` only, and `SettingsView.tsx` verified byte-identical with `cmp`):
  - `/private/tmp/tezbar-runtime-probe-509f53b` (30905e2)
  - `-based` (dacc2e0, which has b2b4f77's source)
  - `-basefd8` (fd8c356)
  - `-fixcheck` (30905e2 plus the combined sketch)
  - `-fixcheck-tr4` (30905e2 plus only the T-R4 change)
- **Probe source:** `/private/tmp/tezbar-runtime-probe-509f53b-probes.ts`, with 13 cases: B×3, C×4, L1, L2, L3, L4, R1×2.
- **Logs:**

| Log | Tree | Result |
| --- | --- | --- |
| `-probes.log` | 30905e2 | B×3, C×4, L1, R1[0] pass; L2, L3, L4, R1[1] fail |
| `-existing.log` | 30905e2, both Settings files | 130/134, the only failures being those 4 probes. All 121 repo cases pass |
| `-based-probes.log` | dacc2e0 | 10 fail: B×3, C×2 (no Remove failure), L2–L4, R1×2. Passing: C×2 with Remove failing, and L1 |
| `-basefd8-probes.log` | fd8c356 | R1×2 pass. B, C and L1 fail there through different flows: removal owned every key, so retire drained OK, and C resurrects. L2–L4 fail |
| `-fixcheck.log` | combined sketch | 134/134 |
| `-fixcheck-full.log` | combined sketch, full suite | 952 passed, 9 skipped |
| `-fixcheck-tsc.log` | combined sketch | exit 0 |
| `-fixcheck-tr4.log` | T-R4 change only | 131/134; only L2–L4 fail |

All log paths are prefixed `/private/tmp/tezbar-runtime-probe-509f53b`.

- **Parent gates** were read as reported, not rerun on 30905e2. My full-suite run on the sketch copy shares the same baseline count of 939.

## Scope and limits

- These are renderer fixtures with mocked `getLlmConfig` and `setLlmConfig`, using `{...config, ...patch}` merge.
- Owner retirement is simulated with `root.unmount()`. In the app, it is the `key={settingsTab}` remount, and "Retry saved changes" calls `retryRetiredSettingsOwners`.
- Not done, as briefed: app launch, native build or Rust tests. Native sources are unchanged in `dacc2e0..HEAD`.
- Not re-reviewed: wire and standards/security/MCP (closed), and the native architecture.
- T-R5's exact trigger needs a failed write, then a remount, then a newer successful action, then Retry. L2 also needs stored config without an Ollama key, which is common for default Ollama users.

## Final counts

- **T-R3: CLOSED.** 30905e2 has no regressions in what it changed.
- **Outstanding actionable: 2.**
  - **T-R4, P3:** regression from b2b4f77. The fix is one shared failure record per write, plus never replacing an entry with a newer origin.
  - **T-R5, P3:** pre-existing. The fix is to filter retries by ownership and staleness, merge custom definitions and models by id, and supersede activation.
- Both are reproducible with the listed probes. The combined fix is verified green on a scratch copy (full suite 952/9 skipped, tsc 0).
- **Gate: NOT passed** until T-R4 and T-R5 are repaired and the R1, L2, L3 and L4 regressions are added.
