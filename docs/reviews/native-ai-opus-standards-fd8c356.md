# Native AI targeted closure review: S8-R1, S8-R2, S5-V1 (Opus 5.5)

- **Worktree:** `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-rust-agent`, reviewed read-only.
- **HEAD:** `fd8c35648c9b59e4c8e52d149eb911ba852fc355`. `git status` was empty before and after the review.
- **Tested source:** `756072ee682eae773626b3d1b78d807217014580`, tree `0147dc5caa1e3dfbe94ab2214c5f7e8b808994a0` (confirmed). `756072e..HEAD` changes two docs only.
- **Diff reviewed:** `847ea62..756072e` for `ext/{mod,secrets,tests}.rs`. `ext/mcp/*`, `process.rs` and `ai/mod.rs` have no changes in that range.
- **Read in full:** `docs/reviews/native-ai-opus-standards-847ea62.md`, the latest ledger section, `AGENTS.md`, the production diff and its call flow, the relevant parts of `AI_EXTENSIONS.md`, and `NATIVE_AI_SPEC.md` (stdio shutdown and lifetime).
- **Search:** no XERJ result was used. The one bounded attempt failed before running because the `timeout` command is not installed here. Everything else used `rg` and direct reads of known paths. No index repair.

## Verdict

**All three assigned IDs are closed. No new actionable regression was found.**

| ID | Verdict | Basis |
| --- | --- | --- |
| S8-R1 | **Closed** | Trace, repo regression, and new local probes |
| S8-R2 | **Closed** | Trace, repo regression, and new local probes (HTTP and stdio) |
| S5-V1 | **Closed** | Trace, a macOS fake-keystore probe, and the parent's real Windows DPAPI run (not re-run here) |

Counts: 0 P0, 0 P1, 0 P2, 0 P3 actionable. There are 3 non-gating observations, listed below.

## Probes

All probes ran outside the repo in `/private/tmp/tezbar-std-fd8c356/`: a `git archive` of `fd8c356` with its own `CARGO_TARGET_DIR` and no Git worktree. Probe tests and one fixture wait were added to that archive only. They used loopback HTTP and owned stdio fixture processes.

- Logs: `probe.log` (first run) and `probe2.log` (stdio rerun).
- **First run:** 5 passed, 1 failed. The failure was my own probe's `acted < 1s` limit applied to stdio shutdown, which took 2.0 s. The panic also stopped the `disable` case from running.
- **Why that is not a defect:** `NATIVE_AI_SPEC.md` requires stdio shutdown to close stdin, wait 2 s, then escalate. The fixture was deliberately blocked, so it used the whole grace period (`client.rs:180-185`). The repo's under-1-second regression uses HTTP only, which is what it should cover.
- **Rerun:** I changed that limit to 2.6 s and reran the stdio probe: 1 passed, all three modes ran.

| Probe | Observed result |
| --- | --- |
| Stdio, Remove during `tools/list` | Action 5 ms; Test returned `Err("AI extension was removed")`; registry empty; process dead after 2.02 s |
| Stdio, shutdown during `tools/list` | Shutdown 2.00 s (the prior finding was at least 5 s per stuck task); Test `ok:false`; registry empty; process dead; `tested=false` |
| Stdio, disable during `tools/list` | Action 0.2 ms; Test `ok:false`; registry empty; process dead after 2.01 s |
| HTTP, Test future dropped during catalog | Probe closed; registry empty |
| HTTP, Test future dropped during connect (undelivered watch result) | After connect finished: registry and starts empty |
| Two successful Tests | Keys are exactly the normal keys; old idle clients closed and their retired entries removed |
| Partial failure (A's catalog succeeds, B's fails) | `ok:false`; existing A and B clients stay open; no probe entries remain |
| Normal start | Exactly the normal key; no probe entry remains |
| After shutdown | Normal and explicit starts refused; starts and clients empty |
| Stress: 40 iterations on 4 worker threads (3 Tests, 2 normal starts and 1 discover, racing shutdown or Remove at varied delays) | No deadlock; no leftover clients or starts in any iteration |
| Fake keystore S5 matrix | All cases below passed |

The stress run is timing-based. It supports the lock-order trace but does not prove every interleaving.

## S8-R1: closed

**Path.** The Remove arm now calls `stop_ext_clients(&cid)` (`ext/mod.rs:1745`) inside the `ext_edit` closure, under the lifecycle lock.

- The helper (`:328-343`) cancels normal and `{id}:test:*` starts.
- It stops and removes normal, `{id}:retired:*` and `{id}:probe:*` clients.
- Project trust with a changed pin reaches the same arm (`project.rs`, "normal removal seam").

**Evidence.**

- `removing_an_extension_stops_retired_clients_and_their_pending_call` uses the compiled stdio fixture, a real in-flight call and a real Test swap. It asserts the old client is open before Remove, then asserts: closed after Remove, empty extension list and client map, the call errors within 1 s, marker count exactly 1.
- This is the exact trigger from the earlier report. The old probe showed the retired client surviving Remove and its result being delivered.
- An in-flight Test blocks on the lifecycle lock until Remove has been written, then fails with "AI extension was removed". Its failure-path `record_mut` therefore cannot recreate anything.

## S8-R2: closed

**Registration and ownership.**

- `ext_connect` takes the lifecycle lock after `Client::connect`, refuses if the runtime has stopped, and registers `{id}:probe:<uuid>` before validity checks (`:549-553`).
- There is no `await` from there to the return, so the outer `select!` cannot interleave with this section.
- Drop order is correct: `_guard` releases before `connected`, and `Probes::drop` takes only the client map lock.

**Handoff.**

- Explicit path: `committed=true, handed_off=true` leaves the registration in place (`:577-578`).
- The start task then either delivers the client or, if `tx.send` fails, stops and unregisters it (`:406-408`).
- Discovery's own guard takes over at `:603` and releases the entry when it drops (`:596-599`, `:57-68`).
- Normal path: the guard releases the probe entry right after adoption under the normal key (`:575-578`).

**Cleanup scope.**

- `release_ext_probe` (`:344-349`) matches both the probe prefix and `Arc::ptr_eq`, so it cannot remove adopted normal or retired entries, or another Test's probe.
- Probes in the "two Tests" and "partial failure" cases confirm this.

**Stop and refusal points.**

| Point | Location |
| --- | --- |
| Before start registration | `:359` |
| Inside the `starting` lock | `:381` |
| Start task prefers core stop | `:398` (biased) |
| Under the lifecycle lock before registering | `:550` |
| Catalog calls use core stop | `:627` |
| Commit closure refuses | `:739` |
| Swap refuses, under the lifecycle lock | `:792` |

**Shutdown ordering.**

- `AiCore::shutdown` cancels `stop` first (`ai/mod.rs:196-197`).
- `Extensions::shutdown` then drains under the same lifecycle lock (`:157-166`).
- Any registration therefore either happens before the drain, and is drained, or sees `stop` and is refused. The same applies to normal adoption and the swap.
- A swap-time `spawn` happens only before `stop` is observed under the lock.

**Lock order (traced, and supported by the stress probe).**

- Paths: lifecycle → `starting` → (released) → `clients`; `starting` → `tasks` in `ext_client_mode`.
- Edit closures take lifecycle → `secrets`.
- I found no path that holds `starting`, `clients`, `tasks` or `secrets` while waiting for the lifecycle lock, and no re-entrant call to `Extensions::shutdown`.

**Regression.**

- `stopping_during_a_test_catalog_cancels_the_probe_and_prevents_adoption` waits for the real `tools/list` request with a 30 s reply delay.
- It asserts the probe is registered, the action takes under 1 s, Test is unsuccessful, clients and starts are empty, and nothing was committed. It runs for both Remove and shutdown.
- This replaces the earlier failing observation (`elapsed=5.002s`, adopted after shutdown).
- The earlier stdio limitation is now covered by the stdio probe above.

**Ledger accuracy.** The S8-R1 and S8-R2 ledger rows match the code and tests.

## S5-V1: closed

**Preconditions (`secrets.rs:58`, `:65`).** Both are `resettable && (ciphertext exists || key load fails)`, and the refusal condition is the exact inverse.

| State | Offered | Reset |
| --- | --- | --- |
| Windows permanent code, no ciphertext | Yes | Allowed |
| Permanent code, ciphertext present | Yes | Allowed |
| Missing key, no ciphertext | No | Refused; the next `put` creates a key |
| Missing key, ciphertext present | Yes | Allowed |
| Healthy key, with or without ciphertext | No | Refused |
| Denied or transient, with or without ciphertext | No (`list` maps `Err` to `false`) | Refused, files untouched |

**Classification is unchanged.** Windows `resettable`, `DpapiError::permanent` and the macOS `load` code are not in the diff.

**Rollback (`:73-96`).**

- Ciphertext rename and rollback are conditional on `had_ciphertext`.
- Revocation rollback is intact, and the Windows key archive is unchanged.
- The probe confirmed that a failed replacement with no ciphertext leaves an identical file list and identical `revoked.json`, with no ciphertext created.
- After a successful reset: revocations are archived, the key is new, and a new secret reads back for the previously revoked ID.

**The original trigger is resolved.** `put(Some)` still fails with "Use Reset…" advice (`:330`), and `canResetSecrets` is now true, so the advice has a working action.

**Windows.**

- The extended `windows_permanent_dpapi_failure…` test (`:615-625`) reads correctly: permanent failure with no ciphertext, reset offered, `put` refused, reset succeeds, offer cleared, new secret round-trips.
- Its actual pass is the parent's Windows run. I did not re-run it, and my macOS probe uses a fake keystore.

## Regressions on touched paths

No confirmed functional regression and no hard `AGENTS.md` violation. The diff adds no new built-in execution path, and no Actions, release or signing activity. New spawns are test fixtures only.

## Non-gating observations

These are not closure requirements.

1. **Stale error text after quitting during a re-Test (proven locally; not a regression).**
   - Probe result: an enabled, previously tested record keeps `enabled=true`, `tested=true` and its catalog, but gains the persisted `error:"Connection was disabled"`. The row would then read "Couldn't start: Connection was disabled" after relaunch.
   - The Test failure path is identical at `1623d47`, and by trace that base gave the same text. The extension stays usable. A successful Test or background refresh clears the error (`:741`).
   - Optional fix: skip the failure-path error write when `inner.stop` is cancelled.
2. **Extra key-store reads on macOS (traced).**
   - `reset_available` now reads the key store even when no ciphertext exists: one read when a key is present, two when it is absent. Previously it made none in that state.
   - Errors stay contained by `unwrap_or(false)`. Any Keychain prompt effect is unproven.
3. **Maintainability (judgment).**
   - `Probes` uses two booleans for its state.
   - Redundant `starting.get(...).cancel()` calls precede `stop_ext_clients` at `:1433`, `:1848` and `:1958`.
   - Some new lines are not rustfmt-shaped.
   - `set-enabled` and `ext_edit` keep their own prefix sweeps, which are correct as written.
   - Client roles are still encoded in string keys. Connection IDs are only checked as non-empty in the validation I read, so a sibling connection literally named `a:probe:x` could match a stop for `a`. This is unproven and the same pattern already exists for `:retired:`.

## Unproven hypotheses (not counted)

- **Dropped future after a delivered result.** If an explicit Test future were dropped after `tx.send` succeeds but before it reads the value, the `:probe:` entry would stay registered. The idle stop would close the client in 10 minutes, and any stop sweep would clear it. I found no timeout or abort around the `ai-extensions:test` call in the AI handlers, but I did not trace the outer IPC dispatch to the end.
- **Secret reset between catalog end and commit.** `reset-secrets` or `set-secret` landing in that gap could still let Test write `tested=true` and adopt a closed client. Discovery's commit code is not new here, and the next start reconnects.
- **Exact-key sweeps.** The sweeps at `project.rs:159` and `oauth.rs:215` are outside this diff. A project revoke of an enabled record is covered by the `ext_edit` disable sweep. Sign-in completing during a Test was not explored.

## Coverage and limitations

- **Proven locally (macOS):** Remove, shutdown and disable during catalog (stdio and HTTP as listed), dropped futures, registry ownership, partial failure, post-shutdown refusal, and the concurrency stress run.
- **Parent-proven, not re-run:** real Windows DPAPI (S5-V1), Windows suite 538 passed / 8 ignored, macOS suite 608 passed / 9 ignored, TS 928 passed / 9 skipped, typecheck, frontend, MSRV and lifecycle. The Windows snapshot hashes were not re-verified.
- **Not proven here:** which DPAPI codes real profile-copy or password-reset cases produce, Windows Job timing on these stop paths, and real third-party MCP servers.
- **Not done:** no full suites, app launch, credentials or profiles, live providers, or visual QA. The renderer row text was read from the code, not viewed.
- **Out of scope:** T-R1, the other S findings and the wire scope. `ext/mcp/*`, `process.rs` and `ai/mod.rs` have no changes in `847ea62..756072e`.
- **Disk:** `/private/tmp/tezbar-std-fd8c356/` is left in place for audit. Its 74 GB `target` is an APFS clone and can be deleted.

**Closure counts:** 3 of 3 assigned IDs closed; 0 actionable findings; 3 non-gating observations.
