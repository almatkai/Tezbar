# Opus review — M3 Lane B agent/settings controllers (pin a52240a)

Reviewer: Claude Opus 5.5, single reviewer, no subagents.
Worktree: `Tezbar-native-rust-agent`, branch `feat/native-gpui-app`, HEAD `a52240a8b6589db15a4e04188e7ceebf9345e148`.
Range reviewed: `db05838..a52240a` (delivery `8d3d3f0` + parent Settings repair `a52240a`), plus a short recheck of the M0–M2 repairs `5b2766b..435e130` and `db05838` `StartupOptions`.
All 20 in-scope files are identical to the pin in the working tree (`git diff a52240a -- <scope>` is empty). I made no source edits, staged nothing, built nothing in the workspace, and launched no app.

## Verdict

**The controller foundation needs repairs before it can be accepted.** Two findings are high severity. The extensions one replays effects; the chat one silently loses model context. One medium finding is a regression introduced by the parent's Settings repair. The rest of the design is careful: tokens/generations, barriers, frozen payloads and redaction. Knowledge and Vault look close to acceptable.

This review covers controller correctness and parity only. It does not claim that the GPUI migration or any GPUI view is complete.

## Evidence method

- Source traces against the pinned Rust and the original TS/core sources. Line references below are from the pin.
- A standalone public-API probe at `/private/tmp/claude-501/-Users-almat-Desktop-code-Tezbar-APP/7ada7892-2ff5-40b0-be46-221973dc4a02/scratchpad/probe/probe.rs`. It was compiled with `rustc +1.85.0` against the existing `target/debug/deps/libtezbar_ui_model-c0fa8aea3ba3559b.rlib` (agent-models, built 03:52, two minutes before the pin commit). There was no cargo build and no workspace write. The probe output is consistent with a source trace of the pin, including the parent's repaired branch. Output:

```
run 1: prior_turn_ids=[]
run 2: prior_turn_ids=["u1", "a1"]
run 3: prior_turn_ids=["u1", "a1", "u2", "a2"]
run 4: prior_turn_ids=["a3"]                 <- F2
P1 visible turns now: ["u4", "a4"]
P2 effects on Done during delete: 0; unsaved after failed delete: true   <- F4
P3 field patch after failed create: {}                                   <- F3
P3 retry patch: {"customProviders":[{"id":"custom:a","name":"A"}]}
P3 failures left: 0, pending effect: false
```

## Findings by priority

### F1 — HIGH, confirmed by source, new migration bug: worker effects are replayed on every reply and re-execute side effects the worker already ran
- **Source (worker).** `src/main/extension-runner.ts`: `pushEffect` (1309–1313) appends to `session.effects`. That list is created empty once per session (4406), capped at 50, and **never cleared**. Every reply returns the cumulative `effects: [...session.effects]` (3628 view render, 4515 and 4637 no-view). In the default `effectMode: 'system'` (4407) the worker already performs the effect: `copyClipboardContent` (1465), `shell.openExternal` (3281), `shell.showItemInFolder` (3290). Recorded values are truncated to 2,000 characters (1462 and others).
- **Source (Rust).** `extensions/runtime.rs` `RuntimeSession::new` and `refresh_finished` call `self.effects.extend(r.effects.into_iter().filter_map(WireEffect::into_effect))`. `no_view` does the same. `into_effect` turns `clipboard/open/show-in-finder/toast/hud` into executable `Effect::Clipboard/Open/ShowInFinder/Toast/Hud`. Only scripts are filtered out. The module docs tell the adapter to "drain typed effects once".
- **Trigger.** In any session, run "Open in Browser" or "Copy to Clipboard" once. Every later reply in that session carries the effect again: legacy 500 ms polls, `refreshMs` intervals, invalidations, searches, selections that load more, and later actions.
- **Impact.** An adapter that follows the documented contract opens the URL again on every poll, every 500 ms. It also re-reveals Finder windows. It overwrites the user's clipboard with a **2,000-character truncated** copy, or with a file path as plain text, on every refresh. That corrupts clipboard data and spams clipboard history. Even an adapter limited to toast/hud shows old toasts again on every refresh. TS did not execute reply effects. It only read the latest toast/hud for the action notice (`ExtensionRuntimeView.tsx` `effectNotice`).
- **Test gap.** `adaptive_grid_colors_and_effects_are_emitted_once` builds a single reply and does not model cumulative wire semantics.
- **Fix.** Treat wire effects as an already-executed record, as the worker does in system mode. Never map clipboard/open/show-in-finder to executable effects. Derive only a notice from effects that are *new* in this reply. Track that either with a worker-side monotonic effect sequence (preferred; the 50-entry sliding window makes count-based diffs unreliable) or by having the worker clear `session.effects` after each reply. Add a regression with two consecutive replies that carry the same cumulative list.

### F2 — HIGH, confirmed by probe, new migration bug: run context comes from the byte-trimmed display cache
- **Source.** `chat.rs` `prepared()` builds `prior_turn_ids` from `c.turns` (last 16, excluding user and assistant). `settle()` and `history_result_conversation()` call `trim_history(.., 256, 2 MiB)`. That function pops front turns while text plus **stage output** exceeds 2 MiB. Stage outputs may be 32 KiB each, with up to 512 stages.
- **Trigger.** Run a tool-heavy agent turn with about 64 or more stages of 32 KiB output, which is common for file and grep tools. Then send a follow-up. The probe shows run 4 receiving `prior_turn_ids=["a3"]`. The user's own question `u3` and all earlier turns are gone. Resuming a stored conversation whose tail exceeds 2 MiB loses context the same way.
- **Impact.** The model silently loses the user's prompt and conversation history. Core can only load what the ids name (`runtime/agent.rs` 64–109). Visible history also collapses to the last pair. TS sent the last 16 message ids (`CHAT_CONTEXT_MAX_TURNS`) regardless of payload size.
- **Fix.** Keep a cheap, separately bounded turn-identity list per owner (ids, role, error flag and image flag; at most 256 entries) for context and `history_has_images`. Let only payloads be trimmed or reloaded on demand. At minimum, never trim the turn being settled or its user turn. Add a regression with large stage outputs.

### F3 — MEDIUM, confirmed by probe, new bug introduced by the parent repair: edits to a custom provider whose creation failed are dropped and reported as success
- **Source.** `settings.rs` `merge()` (pin lines 274–289) skips every non-Definition edit for a `custom:` provider that is missing from the durable read, unless the *same intent* carries its Definition. A skipped edit pushes no key and records no failure. `finish(true)` therefore succeeds.
- **Trigger.** Create custom provider A. The Definition save fails (disk or permission error). Enter its API key (separate intent). Then press Retry.
- **Impact.** The key write produces an empty patch `{}` and succeeds. Retry recreates A without the key, and `failure_count()==0`. The draft still shows the key, so the user believes it is saved. TS `gone()` (`SettingsView.tsx` around 1488) skips only when `hasNewerSettingsSave(id, origin)` holds. It would have written the config.
- **Fix.** Skip only when this queue knows of a successful newer removal (`removed[p]`), or a later save, for that id. Otherwise either apply the edit as TS did, or keep the intent as a pending/failed dependency that retries after the Definition. Never drop it silently. Also order `retry()` by intent revision rather than by `Key` order. Today a `Field` failure is replayed before its own `Definition`.
- **Related (low, unproven reachability).** `Edit::Definition` does not distinguish create from update. A stale owner that edits A's name or URL after A was removed elsewhere recreates A as a definition-only provider. That contradicts the repair's own stated invariant. Splitting the edit into `CreateProvider` and `UpdateDefinition` makes the invariant enforceable.

### F4 — MEDIUM, confirmed by probe, parity regression with a weakened mapping: a failed deletion does not persist the run that settled during deletion
- **Source.** `settle()` inserts the turn into `dirty` but emits no `PersistTurn` while `deleted.is_some()`. `delete_result()` returns `()`, so it cannot emit the deferred write.
- **Trigger.** TS `ownership.test.ts:563`: a running conversation receives `done` during deletion, then the deletion fails.
- **Impact.** The partial response stays in memory only. TS asserts it is in the store (`store.get('B')…includes('partial private text')`). The mapped Rust test `done_overtakes_delete…` only checks in-memory text.
- **Fix.** Return `Vec<Effect>` from `delete_result(false)` with `PersistTurn` for every dirty turn. Assert it in the test.
- **Checked and not an issue.** Ordering an in-flight append against delete (TS `:846`) is safe because core's durable `chat_deleted_sessions` tombstone rejects late appends (`ai/chat.rs` around 168).

### F5 — MEDIUM, confirmed by source, interface gap: persisted assistant turns cannot carry `responseMeta`
- `Turn` has no provider, providerTitle, model or tokenCount. The run's `Preparation.selection` is consumed and not attached to the assistant turn. `select()`/`set_mode()` may change the selection during a run, so the adapter cannot reconstruct which model produced the turn when `PersistTurn` arrives.
- TS persisted `responseMeta` (`transport.ts` 87–110) and showed it.
- **Fix.** Store the run's selection (or a `ResponseMeta`) on the assistant turn at `prepared()`, and fill tokenCount when it arrives.
- **Note.** `Turn`'s derived `Serialize`/`Deserialize` does not match core's chat wire shape (`references`/`stages` are required; core uses `attachments`, `responseMeta` and optional `stages`). Document it as non-wire, or add the core mapping.

### F6 — MEDIUM, confirmed by source, interface trap: `Contributions::update` writes view data built from redacted secrets
- `update(id, field, value)` replaces a whole manifest field (`connections`, `providers`, …) with a view-supplied value. Views only have `display`, where `redacted()` turns `{"secret": ref}` into `{"saved":true,"label":…}`.
- Editing any connection property (for example a URL) therefore sends placeholders in place of secret references. The secret is lost, or a placeholder object is stored. The "original opaque manifest" protection covers only untouched fields.
- **Fix.** Offer field-level edits (per connection index and non-secret key), or rehydrate `{"saved":true}` placeholders from `raw` before sending. Add a regression.

### F7 — MEDIUM, confirmed by source, not probed: discovery emits a stale captured model list
- `ProviderDraft::discovered` merges discovered ids into the draft's **load-time** `models` and emits `Edit::Models{full list}`. `merge()` then writes that list verbatim over the fresh durable read.
- **Trigger.** A model is added or removed by another owner or window between the draft's `load` and discovery completing.
- **Impact.** The other owner's change is lost or resurrected.
- **Parity gaps.** TS merges against the persisted config inside the write (`SettingsView.tsx` 718–783). It also infers capabilities, normalizes the list, and auto-selects the first model when none is selected. The Rust path does none of these.
- **Fix.** Add `Edit::Discovered{provider, ids}` that is merged inside `merge()` against `current`, and reuse `normalize_models`/`infer_capabilities` there.

### F8 — LOW–MEDIUM, confirmed by source, interface: approval focus is not owner-scoped
- `event()` returns `Effect::FocusApproval` (no session id) and sets `c.focus = AllowOnce` for **background** owners too.
- **Impact.** The adapter cannot tell which conversation to focus, and may steal focus from the active chat. After the user resumes that chat, the model's focus is AllowOnce while real focus is the composer. If the view routes Enter via `enter()`, it approves.
- **Fix.** Emit `FocusApproval{session_id}` only for the active, visible owner. Keep `focus = Composer` for background owners until they are resumed.

### F9 — LOW, interface allows it, reachability unproven: retrying a non-last failed turn sends later turns as "prior" context
- `retry()` accepts any failed assistant turn. `prepared()` takes the last 16 turns from the whole conversation, including turns that come after the retried one.
- **Fix.** Restrict context to turns before the retried user turn, or allow retry only for the last failed response, matching AI SDK regenerate truncation.

### F10 — LOW, confirmed by source, mapping overclaims: images have no failure or recovery path, and two other gaps
- `ImageCache` has no `failed(token)` API. A failed fetch, or an oversized result (released in `ready()`), leaves a permanently pending entry. Re-acquiring returns the same entry with no new effect, and it counts toward the 128-entry cap.
- TS `ExtensionsView.test.ts:429` (placeholder and recovery for failed icons) cannot be expressed. App-icon resolution by app path (`:272`, `:289`, `:319`, `:344`, `:365`) is not modelled at all. All 18 ExtensionsView rows map to one test, `late_image_cannot_replace_new_acquisition`.
- `resolve()` parses `C:\…` as a URL with scheme `c` before the absolute-path check, so authorized Windows legacy icons are rejected.
- **Fix.** Add a `failed(token)` state with a placeholder and retry on the next acquisition. Check for Windows absolute paths before `Url::parse`.

### F11 — LOW, confirmed by source: terminal `Session` is stricter than core's metadata
- `shell`, `cwd`, `name`, `saveFor` and `keepAliveFor` are required. Core treats `saveFor` and `keepAliveFor` as optional with `week`/`3h` defaults (`launcher/terminal.rs` 230–234) and retains legacy rows without them (core test row at line 393).
- **Impact.** One legacy row fails deserialization of the whole list.
- **Fix.** Add `#[serde(default)]` with core's defaults.

### F12 — LOW, confirmed by source
- **Knowledge Cancel accessible name.** The Cancel button is named `Cancel Standard (Q8_0)`; TS uses `Cancel ${name} download` (`KnowledgeEmbeddingSettings.tsx` 566/571).
- **Knowledge tab switch mid-mutation.** `activate(false)` clears a `Pending` mutation lock. Switching tabs during a mutation and returning unlocks the controls while core is still executing, and the original result or error is dropped. TS `knowledgeBusy` survives tab switches. Consider keeping the pending phase across deactivate, or reconciling with a barrier read on reactivate.

### Preserved behavior noted (not counted as new bugs)
- Device-flow `slow_down` adds 5 s once (TS 1143 does the same).
- Vault strings are not zeroized (TS cannot either).
- Settings `KeepOpen` keeps the close request, so a later successful retry closes the window. This is close to the TS alert's "Retry saved changes → closeSettings".

## Per-area coverage

| Area | Assessment |
| --- | --- |
| **Chat** | The event serde matches core and `shared/agent.ts`: kebab-case types, camelCase fields, flattened approval, `approval-expired`, extra `suggestedRule` ignored. Run ownership by run id, stage replace-in-place, canonical answer, the "Agent finished without a response" guard, the 30 s start timeout, stop with partial output and failed stages, nonces, capture generations, preparation and deletion fences, retry with canonical user and assistant id reuse, and per-turn persistence tokens with a cap of 16 all match TS. Gaps: F2, F4, F5, F8, F9. The adapter must still supply the core `priorTurns` fallback text for unsaved prior turns; RunRequest omits it, but `conversation().turns` makes that possible. |
| **Settings** | The serial read/merge/save, shallow top-level patch, per-key revision stamps, removal tombstones and failure pruning were traced end to end. They are correct for activation, removal, selection fallback and peer-credential isolation. The parent's two original triggers are fixed. Gaps: F3 (regression from the repair), F7, and the create/update ambiguity. Close semantics (ack waits, KeepOpen while failures remain, 3 s fallback only without ack) match. `DeviceSignIn` matches TS. Contributions: write-only secrets and review/test gating are fine. F6 remains. |
| **Knowledge** | Timeouts (10 s read, 15 s mutation), serial quiet 1.5 s polling, failure stopping polls, barrier/Reconciling/Unconfirmed, Cancel supersession, status events unable to unlock, structured and legacy failure attribution, selected-variant count fencing, authoritative weights, progress capped at 99, and exact IPC args all match TS (`tauri-bridge.ts` 437–450, "Retry start" maps to switch). Only the F12 items remain. All 81 controller rows are mapped. I did not re-execute every row; I sampled the logic. |
| **Extensions** | All 54 worker `makeToken` node types are covered by `NodeKind` (exact set diff). Session/generation/sequence tokens, the submit guard, client vs server search, near-end pagination, push/pop/back callbacks, controlled-vs-draft form reconcile (matches `reconcileFormValues`) and command-scoped preferences look right. Gaps: F1 is the major one; F10 for images. Detail and metadata nodes the worker places in `props.detail`/`props.metadata`/`props.searchBarAccessory` are not validated as nodes (limits only count their bytes). That is acceptable for now, but the renderer will need typed access. |
| **Terminal** | No spawn. Attach generations, the listener gate, the KMP replay overlap (including Unicode boundaries), restore using the same id and `lastCommand` with no `initialCommand` re-execution, and pins/defaults intents are fine. F11 remains. |
| **Vault** | Status → unlock → copy ordering, generation fence, lock clearing the draft/pending/save, frozen `SaveInput` with redacted Debug, and clipboard-entry removal only after durable success are all correct. No issues found. |
| **Architecture** | Constructors do no I/O. The catalog is an `include_str!` of core JSON (no fork), and there is no TS eval or process fallback. Bounds exist. `infer_capabilities` and `format_error` recompile regexes on every call; use `OnceLock`. |

**Test-map honesty.** Of the 354 rows, 351 point to 132 filters. Several many-to-one mappings assert weaker invariants than the TS cases they cite: ownership `:563` (F4), `:398` (TS covers history-list lateness and context; Rust checks that a typed draft discards a resumed load), and ExtensionsView `:272–:429` (F10). These rows should be re-labelled "partial" until fixed.

## Parent M0–M2 repair verdicts (recheck)

| Repair | Verdict |
| --- | --- |
| F1 macOS requirement | **Accepted.** `-R=<req>` is a single argv entry, with real `/bin/ls` positive and negative tests. |
| F2 NSIS | **Accepted on Mac.** Argument order `/P /R /UPDATE /ARGS` and `escape_nsis_argument` match the locked `tauri-plugin-updater-2.10.1` (`config.rs:41`, `updater.rs:1501`) character for character. The caller must pass args **without** argv0, as the plugin uses `[1..]`; this should be documented on the call site. The plugin exits right after ShellExecute, so M11 must own exit/relaunch. Windows execution and installer rehearsal are still pending. |
| F3 desktop-bench | Accepted. I did not re-run it; I relied on the parent log. |
| F4 socket | **Accepted.** `/tmp/tezbar-instance-<uid>` is created 0700, verified by owner, mode and symlink metadata, and the socket is chmod 0600. Stale socket removal happens before bind. Drop removes the socket before unlocking, and contenders retry NotFound/ConnRefused for 10 s. |
| F5 retiring owner | **Accepted.** An explicit `no\n` retries ownership. Ambiguous acknowledgements are errors and are never replayed. Windows releases the named object before retrying. M4 must call `begin_shutdown` before draining core, as planned. |
| F6 main thread | **Accepted.** Inline execution on the constructing thread, CAS-based cancel of queued work, and observing the real result once started. A started operation that never finishes blocks the caller without bound. That is documented as intentional; M4 must construct `NativeDesktop` on the GUI thread. |
| F7 feed errors | Intentionally stricter. Acceptance is deferred to the M11 rehearsal, and it does not affect signing. |
| F8 detach alias | Accepted from the diff (launcher route removed). |
| F9 updater feature | Accepted from the manifest and the parent's dependency-tree evidence. |
| `StartupOptions::ISOLATED_PREVIEW` | **Interface safe.** `start()` keeps the old defaults. Notes: (a) skipping `extensions.start()` also skips `bootstrap_launcher`, not only background restore, which matters if preview should list launcher extension commands; (b) the required pairing with `desktop_services()==false` is documented but not enforced. Consider a single preview constructor that sets both. |

## Remaining uncertainty

- F1's real-world severity depends on whether the M4 adapter executes drained Clipboard/Open effects. The documented contract says to do so, and the replay of notices happens regardless.
- F7 and F6 were traced but not probed.
- F8 and F9 depend on how the views route Enter and expose retry.
- Knowledge rows were sampled, not re-executed individually.
- No Windows host, GUI, live provider, MCP or PTY checks were done, by design. The M3/M4 peer files (launcher, instant, UI and app) were not reviewed.

## Requested repairs before acceptance

1. Fix F1, F2 and F3, each with a regression that fails before the fix.
2. Fix F4–F7, or explicitly defer each one with an adapter contract change recorded in `agent-settings.md`.
3. Re-label the weakened mapping rows.
4. Then obtain a fresh review of the repaired slice.
