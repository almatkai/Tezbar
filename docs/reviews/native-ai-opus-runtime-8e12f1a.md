# Native AI runtime: Opus 5.5 targeted T-R6 closure review at 8e12f1a

## Pin and scope

- **Worktree:** `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-rust-agent`. Read-only.
- **HEAD:** `8e12f1a80b6225943a1e8316baa0a7ed6dab135e`, clean (`git status --porcelain` empty) before and after the review.
- **Implementation:** `935233a0f47d88740c48d1d6f94eb1a8dd0f3179`, tree `49cd23e841e8bb2a112c132170c6e79dfd8583a4` (verified).
  - `935233a..8e12f1a` touches docs only (`docs/NATIVE_AI_VERIFICATION.md`, `docs/reviews/NATIVE_AI_REVIEW_FIXES.md`).
  - `git diff --quiet 935233a 8e12f1a -- src` holds.
- **Change under review:** `a0b3922..935233a`.
  - `src/renderer/SettingsView.tsx`: the merge gate changes from `if (replay)` to `if (!scope?.removedProvider)`, plus a comment. That is 6 lines.
  - Two `it.each` regressions in `SettingsView.knowledge.test.ts`.
  - The a0b3922 review doc.
- **Scope:**
  - T-R6 closure.
  - Regressions at the real callsites of `writeAiPatch`: first-write owned configs, definitions and models; Create; model, capability and protocol edits; activation; scoped removal; retained and retried drafts.
  - Sibling direct writers in `SettingsView` that are subject to the same owned-key contract.
- **Not reopened:**
  - The closed native scopes (wire 847ea62, standards fd8c356).
  - The runtime findings R/U/N-R/C-R and T-R1..T-R5.
  - Queue API, logical stamps and cross-owner ordering.
  - O-A and O-B, which stay as they were.
- **Tools:** I made a bounded XERJ attempt (`xerj def persistAiConfig`). It returned "No definition", and I continued with `rg` over known paths. I did no index repair.

## Verdict

| ID | Verdict | Evidence |
| --- | --- | --- |
| **T-R6** (live first write clobbers definitions and models a retired retry wrote) | **CLOSED** | PROBEA0B-N2 and N2a pass at 8e12f1a and fail at a0b3922 source (identical to f5f8cd0). The two new repo tests fail at a0b3922 source (2/2 RED, `expected false to be true`) and pass at HEAD |
| First write: owned configs, definitions and models (URL/key edit) | No regression | P1 passes. Only `ollama` is written. W's definition, config, models and selection, which landed on disk after mount, are kept. P1 fails at a0b3922 |
| Create (owns every snapshot key, O1 dacc2e0) | No regression | P2 passes. Zed's definition, config and models are written, and provider and chat are set to Zed. W, added on disk after mount, is kept (fails at a0b3922). P10 (retained Create, retried after W lands) passes |
| Per-model protocol, add model, capability toggle | No regression | P3 and P5 pass. X's models are updated (protocol, `x-b`, vision) and the selection is `x-b`. W and the newer Ollama model list are kept (both fail at a0b3922) |
| Provider-protocol edit (custom definition) | No regression | P4 passes. Only X's definition changes, and W is kept (fails at a0b3922) |
| Remove model (`removedModels` reselection) | No regression | P11 passes. `llama-o` is removed, the selection, model and chat model move to `llama-2`, and W is kept (fails at a0b3922) |
| Use this provider (activation) | No regression | P6 passes. Provider and chat are set to X, and W is kept |
| Scoped removal (`removedProvider` path, unchanged) | No regression | P7 passes at both versions. X is fully absent (definition, config, models, selection), provider and chat fall back to Ollama, and W is kept |
| No resurrection from a stale live snapshot | Improved | P8: X removed on disk by another owner, then a live Ollama URL edit. X stays absent at HEAD. At a0b3922 the snapshot re-adds X |
| Retained or retried drafts, queue/lifecycle API | No regression | All 13 PROBE509 probes and 8 PROBEA0B probes pass. P9 and P10 pass. The full suite is green |
| **T-R7 (NEW, actionable): Copilot "Sign in with GitHub" wipes every other provider's `providerConfigs`** | **OPEN, P1, regression from 8f70d2e** | P13 fails at HEAD and passes at 8f70d2e^ (5271fd7). P12 shows that the T-R6 class also applies at this callsite. A fix sketch passes the full suite with 0 failures |

## T-R7 (NEW, P1, actionable): direct device-sign-in write replaces `providerConfigs` with only `{copilot}`

### Callsite

`startDeviceSignIn`, at `src/renderer/SettingsView.tsx:1103–1114`:

```ts
const patch = buildAiProviderPatch()
await window.tezbar.setLlmConfig({ ...patch, ...(cid ? { githubOAuthClientId: cid } : {}) })
```

This is the only AI-provider write in `SettingsView` that bypasses `writeAiPatch`.

### Cause

- Commit 8f70d2e ("fence session deletion and durable settings closure") narrowed `buildAiProviderPatch`.
  - Before: `providerConfigs: { ...aiProviderConfigs, [provider]: providerConfig }`.
  - After: `{ [provider]: providerConfig }`.
- The narrowing relies on `writeAiPatch` merging `{ ...current.providerConfigs, ...patch.providerConfigs }` at execution time.
- The device-sign-in callsite still writes the patch directly.
- The native `llm-config-set` handler (`src-tauri/src/native_core/ai/mod.rs:621–681`) calls `ConfigStore::patch` (`src-tauri/src/native_core/common.rs:166–179`). That does a top-level `object.insert`.
- So `providerConfigs` on disk becomes `{ copilot }`.
- The same direct write also sends the UI's stale whole `customProviders` and `providerModels` snapshot. That is the T-R6 class at a callsite the fix does not cover.

### Repro: PROBE8E1-P13, minimal, no other writer

1. Fixture: Ollama is active, with `ollama.baseURL :11434`, and custom X has `apiKey 'fixture-x'`.
2. Open the AI tab, then the **GitHub Copilot** tab.
3. Click **Sign in with GitHub**.

| Version | Keys in the last write | Keys on disk |
| --- | --- | --- |
| HEAD | `["copilot"]` | `["copilot"]` |
| 8f70d2e^ (5271fd7) | `["ollama","custom:x","copilot"]` | `["ollama","custom:x","copilot"]` |

- At HEAD, Ollama's URL and X's API key are deleted.
- At 8f70d2e^, the probe passes.
- On disk, every non-Copilot credential and base URL is lost after one click. No error is shown.

### Repro: PROBE8E1-P12, the T-R6 class at this callsite

1. After mount, the disk gains custom W (definition, config, models), and provider and chat are set to `custom:w`. This simulates a retired retry or another owner.
2. Click Copilot **Sign in with GitHub**.

The result at both HEAD and a0b3922:

- `customProviders = [custom:x]`, so W's definition is gone.
- `providerConfigs = {copilot}`.
- W's models are gone.
- `provider` and `chat` are still `custom:w`, so they dangle.

### Fix sketch (verified on a scratch copy)

Diff: `/private/tmp/tezbar-runtime-probe-8e12f1a-fixcheck.diff`

```diff
-      const patch = buildAiProviderPatch()
-      await window.tezbar.setLlmConfig({
-        ...patch,
-        ...(cid ? { githubOAuthClientId: cid } : {}),
-      })
+      // Persist the Copilot draft through the ordered, owned-key merge. A direct
+      // write of buildAiProviderPatch() would replace every other provider.
+      persistAiSettings(true)
+      await aiWrites.current
+      if (failedAiProviders.current.has('copilot')) return
+      if (cid) await window.tezbar.setLlmConfig({ githubOAuthClientId: cid })
```

- **How it works:**
  - The Copilot token and client-ID draft now go through the queued owned-key merge, with the same ordering, failure and retry ownership as every other provider edit.
  - The top-level `githubOAuthClientId` scalar is still written directly.
  - When the queued write fails, the flow stops. `writeAiPatch` has already shown the AI save error and recorded the retryable failure.
- **Results:**
  - P12 and P13 pass.
  - All 34 probes pass.
  - Full `vitest run`: **981 passed, 9 skipped, 0 failed**, with 125 files passed and 2 skipped. That is 947 repo tests plus 34 probes.
  - `tsc --noEmit -p tsconfig.json`: exit 0.
- **Recommended permanent regression:** add P13 to the repo, using a real component and MockIPC. Assert that the Ollama and custom X configs survive. Optionally add P12, asserting that W's definition is kept and the provider does not dangle.

### Why the earlier audits missed it

- The a0b3922 safety argument ("Every non-removal caller … owns that provider's config key") enumerated only `writeAiPatch` callers.
- No repo test drives `startDeviceSignIn` while other providers are configured.

### Other `setLlmConfig` writers in `SettingsView`

I checked all of them. None writes `providerConfigs`, `customProviders` or `providerModels` wholesale:

- hotkey, retention, memory, timeouts, thinking, onboarding flag (scalars)
- `loadAiModels`, which merges against a fresh `getLlmConfig` and writes inside the queue

Other views (`ProvidersView`, `CommandBar`, `AgentChatView`) do not use `buildAiProviderPatch`. 8f70d2e did not change them, and they are outside this scope.

## Non-gating observations

- **O-A, O-B:** unchanged and not re-evaluated.
- **O-C (pre-existing, not changed by 935233a): same-provider model list follows the owner's snapshot when another writer was unstamped.**
  - Evidence (PROBE8E1-P9, strict variant):
    1. A retained, failed Ollama URL draft is retried.
    2. Before the retry, the Ollama model list on disk gained `llama-ext` from a writer that records no settings revision. In the app, that writer is optional discovery in `loadAiModels`.
    3. The retry writes its own Ollama list `['llama-o']`.
  - It fails identically at a0b3922, because the replay path is unchanged.
  - P1 shows the same owner-wins behavior for a live first write.
  - Impact: discovered models are dropped until the next refresh. Edits made through `writeAiPatch` are stamped and therefore kept (T-R5). There is no credential or definition loss.
  - In the stored probe, it is logged as an observation only.

## Evidence

All of this is outside the repo, using `git archive` snapshots with `node_modules` symlinked to the worktree's `node_modules`.

### Snapshots

| Path | Contents |
| --- | --- |
| `/private/tmp/tezbar-runtime-probe-8e12f1a` | HEAD. `SettingsView.tsx` is byte-identical (`cmp`) to `935233a:` |
| `/private/tmp/tezbar-runtime-probe-8e12f1a-base` | a0b3922. `SettingsView.tsx` is `cmp`-identical to the predecessor's f5f8cd0 snapshot |
| `/private/tmp/tezbar-runtime-probe-8e12f1a-redcheck` | a0b3922 source with HEAD's test file |
| `/private/tmp/tezbar-runtime-probe-8e12f1a-pre8f70d2e` | 5271fd7, the T-R7 boundary |
| `/private/tmp/tezbar-runtime-probe-8e12f1a-fixcheck` | HEAD plus the T-R7 sketch |

### Probes

- **Predecessor's 21:** the PROBE509 and PROBEA0B block, spliced verbatim from the a0b3922 fixcheck snapshot.
- **New:**
  - 11 PROBE8E1 cases (P1–P11): `/private/tmp/tezbar-runtime-probe-8e12f1a-new-cases.ts`. The strict P9 variant is in `…-new-cases-strict.ts`.
  - P12: `/private/tmp/tezbar-runtime-probe-8e12f1a-p12.ts`.
  - P13: `/private/tmp/tezbar-runtime-probe-8e12f1a-p13.ts`.

### Logs

| Log | Tree | Result |
| --- | --- | --- |
| `/private/tmp/tezbar-runtime-probe-8e12f1a-probes.log` | HEAD, 21 old + 11 new (strict P9) | 31/32 pass. Only strict P9 fails (O-C) |
| `/private/tmp/tezbar-runtime-probe-8e12f1a-base-probes.log` | a0b3922 | 21/32 pass. Fail: N2, N2a, P1–P6, P8, P9 (same O-C assertion), P11 |
| `/private/tmp/tezbar-runtime-probe-8e12f1a-redcheck.log` | a0b3922 source + HEAD tests | 2/2 new repo tests RED |
| `/private/tmp/tezbar-runtime-probe-8e12f1a-knowledge.log` | HEAD, knowledge file + 32 probes (P9 as observation) | 160/160 |
| `/private/tmp/tezbar-runtime-probe-8e12f1a.p12.log`, `/private/tmp/tezbar-runtime-probe-8e12f1a-base.p12.log` | HEAD, a0b3922 | P12 fails at both (pre-existing T-R6 class at this callsite) |
| `/private/tmp/tezbar-runtime-probe-8e12f1a.p13.log` | HEAD | P12 and P13 fail |
| `/private/tmp/tezbar-runtime-probe-8e12f1a-pre8f70d2e.p13.log` | 5271fd7 | P13 passes |
| `/private/tmp/tezbar-runtime-probe-8e12f1a-full.log` | HEAD + 34 probes, full suite | **979 passed, 2 failed (P12, P13 only), 9 skipped.** All 947 repo tests pass. The run before P12 and P13 were added gave 979 passed and 9 skipped, with 0 failed |
| `/private/tmp/tezbar-runtime-probe-8e12f1a-tsc.log` | HEAD + probes | exit 0 |
| `/private/tmp/tezbar-runtime-probe-8e12f1a-fixcheck.probes.log` | sketch | 34/34 probes pass |
| `/private/tmp/tezbar-runtime-probe-8e12f1a-fixcheck-full.log` | sketch, full suite | **981 passed, 9 skipped, 0 failed** |
| `/private/tmp/tezbar-runtime-probe-8e12f1a-fixcheck-tsc.log` | sketch | exit 0 |

The parent's reported gates (947/9 tests, typecheck, frontend build, diffcheck, native byte-identical since 756072e) were read as reported. My HEAD snapshot independently confirms that all 947 repo tests pass and that tsc exits 0.

## Limits

- The renderer fixtures mock `getLlmConfig` and `setLlmConfig` with a shallow `{...config, ...patch}` merge.
  - I verified by reading the code that this matches the native `ConfigStore::patch` top-level insert.
  - I did not run native code.
- Owner retirement is simulated with `root.unmount()`.
- For "another owner / retired retry wrote after mount", P1–P12 mutate the fixture's disk state directly. Those writes carry no settings revision stamps.
- I did not launch the app, build native code, use a live model or MCP, or read credentials or profiles.
- I made no repo edits, checkouts, worktrees or branches. HEAD stayed pinned at 8e12f1a and the tree stayed clean before and after.

## Final

- **T-R6: CLOSED.**
  - The merge-gate change is correct at every `writeAiPatch` callsite: URL and key edits, Create, per-model and provider protocol, add and remove model, capability toggle, activation, scoped removal, and retained and retried drafts.
  - It also stops stale live snapshots from resurrecting removed providers.
  - None of the touched paths regressed.
- **The zero-actionable gate is NOT passed. 1 actionable finding remains:**
  - **T-R7 (P1, a branch regression since 8f70d2e):** Copilot's "Sign in with GitHub" writes `buildAiProviderPatch()` directly. Because 8f70d2e narrowed `providerConfigs` to the owned key, this deletes every other provider's credentials and base URLs on disk.
  - It also carries the T-R6 class: a stale whole `customProviders` and `providerModels` snapshot, which can leave `provider`/chat dangling.
  - **Fix:** route the write through `persistAiSettings(true)` and `aiWrites`, as in the sketch above. That is verified with 981 passed, 9 skipped, 0 failed, and tsc 0.
  - Add P13 (and optionally P12) as a permanent regression.
- **O-C (new, pre-existing):** non-gating. O-A and O-B are unchanged.
