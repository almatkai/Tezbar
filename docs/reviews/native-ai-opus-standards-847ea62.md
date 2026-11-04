# Native AI targeted re-review, Scope C (S1–S10): extension security/lifecycle and standards (Opus 5.5)

- **Worktree:** `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-rust-agent`, read-only.
- **HEAD:** `847ea628155238424808a888afa6e9307b461354`. `git status --porcelain` was empty before and after this review.
- **Tested source:** `21d3e911a03bbea64ffb39324d6822ca343f9ad7`. `21d3e91..847ea62` touches docs only (`NATIVE_AI_VERIFICATION.md`, ledger).
- **Commits (`git log 1623d47..HEAD`):** `847ea62` docs, `21d3e91`, `74c13ed`, `96a55cf`, `c4b8888`, `5dddcb4`, `31bc3e2` (docs: the retained reports).
- **Audit base:** `1623d47`. The three `native-ai-opus-*-1623d47.md` reports are unchanged since they were added in `31bc3e2` (`git diff --quiet 31bc3e2 HEAD -- <reports>`).
- **Read in full:** `native-ai-opus-standards-1623d47.md`, the 18-repair section of `NATIVE_AI_REVIEW_FIXES.md`, and the S1–S10 production diff in `1623d47..21d3e91`:
  - `oauth.rs`, `process.rs`, `vault/keychain.rs`
  - `ext/{mod,secrets,tests}.rs`, `ext/mcp/{client.rs,fixture.rs.txt}`
  - `ai/mod.rs` (redaction cache, shutdown)
  - `AiExtensionsSection.tsx`, `shared/aiExtensions.ts`
  - `AI_EXTENSIONS.md` diff, and `NATIVE_AI_SPEC.md` §10.4 ("short reason plus up to 20 scrubbed lines").
- **AGENTS.md** was read.
- **XERJ:** one bounded `search` with prefix `ax-65b00e69da24` returned an irrelevant hit (`native_protocol.rs`). Everything else used `rg` and direct reads of known paths. No index repair.

## Probes

All probes ran outside the repo, in `/private/tmp/tezbar-std-847ea62/`.

- **Archive:** `git archive 847ea62` plus APFS clones of the ignored build inputs (`dist`, `build/embedding-runtime`, `dist-backend/{node_modules,vec0,libsqlite3.dylib}`, `src-tauri/gen`, `node_modules/{esbuild,@esbuild}` dereferenced). Separate `CARGO_TARGET_DIR` (a clone of the existing debug target). No repo writes, no git worktree.
- **P-S8a/P-S8b:** two probe tests appended only to the archive's `ext/tests.rs`. Run with `cargo test --lib --locked --offline reviewprobe_ -- --nocapture --test-threads=1`: 2 passed (they print observations rather than assert the claim). Log: `/private/tmp/tezbar-std-847ea62/probe.log`.
- **P-S1:** `oauth.rs::private_address` copied verbatim and run with `rustc` (`s1/pa.rs`).
- **P-S7:** a model of the UCRT argv rules applied to the exact batch-tail quoting in `windows_mcp_command` (`s7/crt.rs`). This is a model, not a Windows run. The real Windows compiled-recorder evidence is the parent's.
- **Not done:** no full-suite rerun, no Windows host, no app launch, no live models or MCP servers, no credentials or profiles, no real NAT64 network.

## Verdict

**NOT CLOSED.** Counts: 0 P0, 0 P1, 0 P2, **3 P3**.

- S1–S7, S9 and S10 are **closed** on their reported triggers.
- **S8 is not fully closed.** Two confirmed variants remain:
  - **S8-R1:** remove does not stop retired clients.
  - **S8-R2:** shutdown regression for in-flight explicit-Test probes.
- **S5-V1** is a narrow residual Windows variant of S5. It is not a regression.
- All three P3s are functional issues with minimal fixes. The ledger's S8 claim "disable/change/remove/shutdown stop retired clients too" is false for **remove**. Shutdown also misses explicit probes that are not yet committed.

## New / remaining findings

### S8-R1 (P3) `ai-extensions:remove` doesn't stop retired clients or in-flight explicit-Test starts

CONFIRMED by probe. This is an incomplete S8 repair.

- **Where:** `ext/mod.rs:1712-1726` (remove arm). It does `starting.get(&cid)` and `clients.remove(&cid)` only. The S8 repair added `stop_ext_clients` (`:322-337`), which also covers `{cid}:test:*` starts and `{cid}:retired:*` clients, but used it only in update (`:1831`), reset (`:1414`) and secretless-start removal (`:1944`). `set-enabled false` (`:1673-1689`) and the disable sweep in `ext_edit` (`:294-315`) use a prefix sweep, so they are fine. A removed record is absent from `data`, so the `ext_edit` disable sweep never fires for remove. Project trust with a changed pin removes old records through this same arm.
- **Trigger:**
  1. A run is inside a long tool call.
  2. The user presses Test, which swaps in a probe and retires the old client with `active=1`.
  3. The user presses Remove.
- **Probe output (P-S8a):**
  - `old.is_closed=false remaining client keys=["6:retire:stdio:retired:21d9…"]`
  - After the fixture released: `in-flight call outcome after remove: Ok("{…fixture 🦀…}")`
- **Impact:**
  - The process of a removed extension keeps running with its env secrets until the call ends (up to `timeoutSeconds`, ≤1800 s), after its secrets were revoked.
  - Its result is delivered to the model.
  - Without a prior Test, the same remove gives "Connection lost". So behavior depends on whether Test was pressed, and contradicts the ledger.
  - An in-flight explicit-Test start for that connection also isn't cancelled. It stops itself after connect, via the post-connect "Connection was disabled or changed" check (`:531-546`), so it is not adopted.
- **Minimal fix:** replace the remove arm's `starting.get`/`clients.remove` pair with `core.stop_ext_clients(&cid)`. Add a variant of `closure_test_retires_client_after_long_call_and_disable_stops_retired` that removes instead of disabling.
- **Standards note:** the root cause is Shotgun Surgery. Four separate stop sweeps exist (remove, set-enabled, `ext_edit` sweep, `stop_ext_clients`). Route all of them through one prefix-aware helper.

### S8-R2 (P3) Shutdown neither stops nor refuses an explicit-Test probe between connect and swap; quit waits ≥5 s and the probe is adopted after shutdown

CONFIRMED by probe. This is a regression against `1623d47`.

- **Where:**
  - Since S8, the explicit probe is not inserted into `clients` in `ext_connect` (`ext/mod.rs:547-555`). It lives only in the local `Probes` guard of `ext_discover_mode` until the swap at `:785-813`.
  - `Extensions::shutdown` (`:153-160`) cancels `starting` and drains `clients` only.
  - Catalog calls use `Cancel::new()` (`:601`), not core stop.
  - The post-edit validity check (`:765-783`) checks extension state but not `inner.stop`.
  - The probe's idle-reaper, listen and stdio reader tasks are in `inner.tasks`, so `AiCore::shutdown` (`ai/mod.rs:211-219`) waits 5 s for each before aborting it.
  - In `1623d47`, the explicit client was inserted in `ext_connect` before the catalog, so `Extensions::shutdown` stopped it.
- **Trigger:** the user presses Test, and Tezbar quits while the catalog listing is running. Slow `tools/list` or paginated catalogs widen the window.
- **Probe output (P-S8b):** HTTP fixture with `tools/list` delayed 1.5 s, shutdown 0.7 s into Test: `shutdown elapsed=5.002s test outcome=Ok(Ok(true)) clients after shutdown=["4:quit:web"] closed=[false]`. Test completed and committed after `AiCore::shutdown`, and a live client was adopted into the drained map.
- **Impact:**
  - Quit is ≥5 s slower per stuck probe task. A stdio probe has up to about three such tasks.
  - A stdio probe is killed only when its reader task is aborted (`GroupChild::drop`).
  - No security or data loss. During shutdown, the settings write of the probe's catalog also lands after shutdown began.
  - Disable during the same window is safe: the validity check refuses the swap and `Probes::drop` stops the probe.
- **Minimal fix:**
  - Register uncommitted probes where `Extensions::shutdown`, `stop_ext_clients` and the prefix sweeps see them (for example a `{id}:probe:<uuid>` entry, or a probes set).
  - Refuse the swap when `inner.stop.is_cancelled()`.
  - Give catalog calls a cancel tied to core stop.
  - Add a test: shutdown during an explicit-Test catalog finishes in under 1 s and leaves no client.

### S5-V1 (P3, Windows, narrow) A permanently undecryptable key file with no ciphertext file leaves "Use Reset…" advice without a Reset action

CONFIRMED by trace. Not a regression. Windows only, so not run.

- **Where:**
  - `secrets.rs:51-56`: `reset_available` requires `self.path.exists()`, where `path` is the ciphertext.
  - `secrets.rs:39-49`: `key()` propagates `load()`'s permanent `DpapiError`. Its text says "Use Reset saved connection secrets to archive them…" (`:318-329`).
- **Trigger:**
  1. `ai-extension-key.bin` exists but `ai-extension-secrets.bin` does not. This happens after a successful reset with no secret re-entered yet (reset archives the ciphertext and writes a new key), or after a failed ciphertext write following the first key save.
  2. DPAPI then fails permanently (profile copy, admin password reset).
- **Impact:** every `put(Some)` fails with Reset advice, but `canResetSecrets` is false, so no Reset button appears. Nothing is protected by the old key, so nothing would be lost.
- **Minimal fix:** offer the reset (or let `key()` call `self.key.reset(&new)`) when the ciphertext is absent and `self.key.resettable()? == true`. Keep the denied and transient codes refused. The old key file is still archived, never deleted.

## Per-ID closure (S1–S10)

| ID | Disposition | Production path / evidence |
| --- | --- | --- |
| **S1** | **Closed** | `oauth.rs:71-80`: IPv4-mapped addresses and `64:ff9b::/96` (exact 12-byte prefix match) return the embedded IPv4 classification. Everything else, including RFC 8215 `64:ff9b:1::/48`, deprecated `::a.b.c.d` and non-/96 `64:ff9b:0:0:1::`, still falls through to the fail-closed `!2000::/3` rule. P-S1 verbatim output: public NAT64 and mapped public → `false`; NAT64 of 10/8, 127/8, 192.168/16, 100.64/10 → `true`; `64:ff9b:1::…`, `::ffff:10.0.0.1`, `::ffff:127.0.0.1`, `::7f00:1` → `true`; `2606:4700::1111` → `false`. The regression test (`oauth.rs:554-588`) runs public translations through `ResourcePolicy::from_fixture`, then checks: loopback endpoint refused, private pinned endpoint refused, mixed DNS refused, private translations stay private. Pinning and refresh code (`ResourcePolicy`, `resourcePrivate` refresh check) is unchanged, so a public→private rebinding is still refused. The network impact on real NAT64 is not demonstrated (no NAT64 network), but the classification it depends on is now correct. Judgment: the later `to_ipv4_mapped().is_some_and(…)` clause at `:86-87` is now unreachable dead code. |
| **S2** | **Closed** | `ext/mod.rs:436-445`: a closed, crashed cached client is removed, and the "test it again" error is returned only when `!explicit_test`. Explicit Test also bypasses the failure backoff (`:348-358`). `closure_first_test_after_post_start_crash_reconnects` crashes a started stdio client on a real call, then the first Test returns `ok:true`. |
| **S3** | **Closed** | `client.rs:373-383`: `startup_error` waits ≤350 ms for `stderr_done` and returns only "The program exited during startup (<exit status>). Show log for details." `StopGuard::drop` (`:919-943`) saves the last 20 ring lines into `failed_logs`. Connect success clears them (`:364-369`). `ai-extensions:logs` (`:1444-1482`) appends live logs and failed tails, then keeps the last 20. Ring lines are `scrub`bed, which runs `redact_text` plus exact secret values (`client.rs:985-1000`). The stderr reader flushes the unterminated final line on EOF or the drain deadline. Tests: the fixture writes 40 long lines, then an unterminated cause containing `FIXTURE_TOKEN`; assertions check the cause is present in logs, ≤20 lines, token absent, and row error <160 chars, for both the failed-start and cached-era paths. Redaction cache (`ai/mod.rs:965-1000`): same patterns, same order, same replacement strings, now in `LazyLock` (stable since 1.80; MSRV 1.85 check passed per the parent), so the output is identical. Judgment: for a non-crash failure (initialize timeout or protocol error) the tail is captured before `StopGuard` cancels stop, so a final flush after that is missed. `failed_logs` is not cleared on remove or update, so an extension re-added with the same id can show an old tail until it connects successfully. Neither is a gate. |
| **S4** | **Closed** | The reset edit sets `needsAuth` only for `auth.type=="oauth"` (`ext/mod.rs:1400`). Token and env/header connections get `tested=false` plus "Enter the secret again, then test this connection." The row's `failure && some(needsAuth)` check (`AiExtensionsSection.tsx:749`) therefore no longer hides that text after a reset. The Sign in button is still OAuth-only (`:726`). The list computes `providers[].missingCredentials` from the provider's secret namespace (`:1351-1373`). Details shows "Enter the provider credentials below, then save each key." above the existing Save key controls (`:878-920`). The reset copy names both owners. Tests: real reset IPC (`needsAuth=false`, error text, provider `missingCredentials=true`, plain connection unchanged); component copy test. Judgment: the provider flag appears only in Details, not on the row. On a denied or transient key store, `values(..).unwrap_or_default()` reports `missingCredentials=true` and prompts re-entry, though Save would then fail with the DPAPI or Keychain error. That is misleading copy, not data loss. The list now decrypts once per provider per list call (Keychain read on macOS), in addition to `reset_available`. Out of scope and pre-existing: a 401 or 403 at tool-call time also sets `needsAuth=true` for non-OAuth HTTP connections (`ext/mod.rs:1174-1181`, `client.rs:553-565`), which shows "Sign in required" without a Sign in button. Same UX class, different trigger. Noted, not counted. |
| **S5** | **Closed on its trigger**; residual S5-V1 | `KeyStore` gains `resettable`/`reset` defaults with identical macOS semantics (`load().is_none()`, `save`). Windows `resettable` returns `true` only for `ERROR_INVALID_DATA` (13) and `NTE_BAD_KEY_STATE` (0x8009000B), read via `GetLastError` immediately after the failing call. Decryptable → `false`; any other code → `Err`, so it is refused. `reset_missing` steps: rename ciphertext; rename `revoked.json` to a unique `<archive>.revoked.json`; `OsKey::reset` re-checks, archives the key file under a unique name and saves the new key. On key failure it restores the key, then the ciphertext and revocations. Nothing is ever written in plaintext, and reset happens only through the explicit IPC. Windows test (parent-run): a real wrong-entropy blob is permanent, a truncated blob is not; codes 5 and 0x80090020 are not permanent; `put` is refused; a real sharing-denied rename leaves key, ciphertext and revocations byte-identical; then reset succeeds, the archived key equals the original, and the new secret round-trips. Docs match. Judgment: a nested-rollback failure returns early without restoring `revoked.json` (archives are kept, so it can be recovered by hand). |
| **S6** | **Closed** | `process.rs:569-577`: multi-component relative commands resolve against the effective connection cwd, and relative PATH entries are joined to the cwd. `client.rs:98-102` passes the same `c["cwd"]`/home default that `current_dir` uses (`:139-145`), so the check and the spawn cannot diverge. Absolute commands are unaffected (`join` of an absolute path). Bare names still search only PATH, with no implicit cwd search, which matches Unix. Windows test (parent-run): exe and cmd via relative paths with spaces, plus bare and extension PATHEXT lookups through a relative PATH entry, all checked for exact hex argv. |
| **S7** | **Closed** | `process.rs:620-628`: only the run of backslashes right before each closing quote is doubled. Since `"` is refused, no internal backslash can precede a quote. Metacharacter refusal is unchanged (`:610-616`). Non-batch programs still use std `args`. P-S7 model: every case round-trips exactly (trailing `\`, lone `\`, UNC-with-trailing, mixed runs, leading/trailing spaces, empty, following argument). The pre-repair tail reproduces the merge `["C:\\folder\" following"]`. Parent-run real Windows recorder: same cases through `.cmd`, exact hex. The injection-refusal test and the Job-owned tree are retained. |
| **S8** | **Not fully closed**: S8-R1, S8-R2 | Closed sub-claims: explicit starts use a unique `{id}:test:<uuid>` key, so they never join a normal start (`:361-380`; `closure_test_does_not_join_normal_cached_era_start` re-detects the modern era). The probe and its whole catalog must succeed, and a post-edit validity check runs under `extensions.lock`, before the swap (`:760-813`). The old client is retired: shut down at `active==0` or on the last `Activity` drop (`client.rs:393-399`, `:806-813`). `subscriptions/listen` is excluded from `active` (`:759`), so a modern client can still retire. Cancelled or dropped probes are stopped by the `Probes` guard and by the failed `tx.send` (`:50-63`, `:393-395`). A normal start yields to a newer live client (`:547-553`). `closure_test_retires_client_after_long_call_and_disable_stops_retired` covers: real result exactly once, disable cancels the retired call, no replay (marker count 2). Immutable tool pins and review gates are unchanged (`command_pin`/`trustPin`/`needsReview` path untouched). Open: remove (S8-R1) and shutdown during the probe window (S8-R2). Limitation (not a bug): a run's snapshot keeps `entry.client`. After a Test swap, the run's later calls fail before sending with "Connection was disabled or cancelled": no unknown outcome, and better than the pre-repair immediate kill. |
| **S9** | **Closed** | `ext/mod.rs:1821-1822`: the `changed` branch (`command_pin` differs, covering URL, auth, command, env and headers) removes `requestedScopes` along with `cachedEra`. Unrelated record edits keep it through the bookkeeping restore list (`:1793-1815`), and configured `auth.scopes` stays because it is part of the pin. `closure_changed_server_clears_requested_scopes_only` runs on the real update IPC: rename keeps it, URL change clears it, `auth.scopes` is retained. The existing step-up regression is retained. |
| **S10** | **Closed** | `client.rs:190-219`: the stderr read races a deadline of `stop.cancelled()` + 250 ms. On the deadline or EOF it flushes the partial line, then fires `stderr_done`. `stop` is cancelled by the stdout reader at EOF/crash, by `shutdown()`, and by `StopGuard`. Normal crash output is already in the pipe and is read well within 250 ms. Unix test (`closure_escaped_stderr_pipe_does_not_hold_shutdown_tasks`): a `setsid` descendant holds stderr; shutdown takes <2 s; RAII kills only the owned PID. Without the fix, the 5 s per-task wait would fail that bound. Windows Job behavior is unchanged. Test-quality judgment: the `tasks.is_empty()` assertion is vacuous (`shutdown` takes the vector with `mem::take`), and this test doesn't assert the final "escaped stderr final cause" tail. By code that tail is flushed on the deadline path (`Ok(0)` → `feed(&[], true)`), and S3's tests cover the unterminated-flush path. Residual (unproven, low): a server that closes stdout and only then logs its cause more than 250 ms later loses that line. |

## Regressions on touched paths (outside the S-IDs)

- **Redaction cache:** semantics are identical (see S3). No regression.
- **`windows_mcp_command` signature change:** it has one production caller (`client.rs:91`), and that caller passes the cwd.
- **Lock order:**
  - `stop_ext_clients` takes `starting`, releases it, then takes `clients`.
  - The swap takes `extensions.lock`, then `clients`.
  - Edit closures take `extensions.lock`, then `starting`/`clients`.
  - `retire`/`shutdown` only cancel.
  - No inversion found.
- **Narrow residual race (unproven, pre-existing in kind):** a normal start finishes `ext_read` (still enabled), then a disable sweep runs before the synchronous insert, then the client is inserted. The window is microseconds. Tools are still gated by `enabled`, and the idle reaper reclaims the client in 10 min. Not counted.
- **Builtin execution:** no new Pi/Node/npm/Bun/OpenCode built-in execution path in the diff. New spawns are only test fixtures (`rustc` in a Windows test, the `setsid` fixture).

## Standards and judgment (not closure gates)

- **AGENTS.md:** typecheck and tests must pass. I accept the parent's exact-source record (Mac 606/9, Windows 536/8, TS 926/9, typecheck, frontend, MSRV, lifecycle) and did not rerun it. No Actions, release or signing in scope.
- **Smells:**
  - **Shotgun Surgery / Duplicated Code:** four client-stop sweeps exist, and S8-R1 comes directly from that. Recommend one prefix-aware `stop_ext_clients` used by remove, set-enabled, the `ext_edit` disable sweep, reset, update and shutdown.
  - **Feature Envy / Divergent Change:** probe lifecycle (`Probes`, `:test:`/`:retired:` key suffixes, validity check) is spread across `ext_client_mode`, `ext_connect` and `ext_discover_mode` inside a still-growing `ext/mod.rs`. A small `ProbeSession` type owning register → commit/abort → retire would fix both S8 residuals.
  - **Primitive Obsession:** string-encoded client roles in map keys (`{id}:test:`, `{id}:retired:`) let prefix scans match roles by accident. A typed key `(ConnKey, Role)` would be safer.
  - **Blocking I/O:** blocking `extensions.load()` runs under a std `Mutex` on an async thread in the S8 validity check (`:766-767`).
  - **Per-list key-store reads:** one per provider (S4). Cache until the next secret operation.
  - **Dead code:** the mapped-IPv4 clause in `private_address`.

## Coverage and limitations

- **Proven locally:**
  - S1 classification (verbatim probe plus repo test).
  - S7 quoting against a UCRT model.
  - S8-R1 and S8-R2 via archive probes on the real `AiCore` IPC with real stdio and HTTP fixtures.
  - The remaining dispositions are traced through production call sites, with repo tests read for each.
- **Parent-proven, not re-run:** Windows DPAPI (S5), Windows relative and batch argv (S6/S7), Windows Job ownership.
- **Plausible upstream, not demonstrated:**
  - Real NAT64/DNS64 interop (S1).
  - Which DPAPI codes real profile-copy or password-reset cases produce. Only the two documented codes are treated as permanent, and archives keep every misclassification recoverable by hand.
  - Real Python/Node/uvx stderr shapes (S3/S10).
  - S8-R2 for a stdio probe (traced; the probe used HTTP).
- **Not done:** visual QA in either theme; live models; third-party MCP servers.

**Final counts:** 0 P0, 0 P1, 0 P2, 3 P3 (S8-R1 and S8-R2 CONFIRMED by probe; S5-V1 CONFIRMED by trace, narrow, Windows only). Of the assigned IDs, 9 of 10 are closed. S8 stays open until S8-R1 and S8-R2 are fixed and re-checked.
