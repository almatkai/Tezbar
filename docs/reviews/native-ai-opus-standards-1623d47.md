# Native AI closure audit, Scope C: extension security/lifecycle + standards (Opus 5.5)

- **Worktree:** `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-rust-agent`, read-only.
- **HEAD:** `1623d47245390b4b32f30c0bed2b44a1d1a721e9`, clean before and after the audit (`git status --porcelain` empty). Base `v0.3.0-beta.7` = `e2467521`.
- **Sources:** renderer final `ae2fe00`. Rust tree `8f70d2e` is byte-identical to HEAD (`git diff --quiet 8f70d2e 1623d47 -- src-tauri` → identical). `ae2fe00..1623d47` touches docs only (`NATIVE_AI_VERIFICATION.md`, ledger).
- **Commits:** `git log e2467521..HEAD --oneline` lists 26 commits, ending `… 118cb7b, 9fe0576, a28c564, c486614, 5271fd7, 8f70d2e, c282d3d, ae2fe00, 1623d47`.
- **Diff audited in scope:** `git diff 118cb7b..ae2fe00`. I covered `oauth.rs`, `ext/mod.rs`, `ext/secrets.rs`, `ext/mcp/{client,oauth}.rs`, `process.rs`, `ai/mod.rs` (dispatch/redaction/lifecycle parts), `runtime/agent.rs` (extension-adjacent parts), `platform.rs`, `AiExtensionsSection.tsx`, `shared/aiExtensions.ts` and the docs. `child_supervisor`, Windows Job code, `scripts/`, `tauri.conf.json` and `Cargo.toml` have no changes in this range.
- **Inputs read in full:** `native-ai-opus-standards-32ba9c4.md`, `native-ai-opus-standards-118cb7b.md` and the ledger `NATIVE_AI_REVIEW_FIXES.md`. Every disposition below was re-traced through the production call sites, not taken from the ledger or helper tests.
- **XERJ:** one bounded `search` with prefix `ax-65b00e69da24`. It returned an irrelevant hit (`visibleEventRefresh.ts`). I used `rg` and direct reads for everything else.
- **Probe:** `/private/tmp/tezbar-std-probe-1623d47/pa.rs` copies `oauth.rs::private_address` verbatim and runs it with `rustc` outside the repo. No cargo was run in the repo.
- **Not done:** no edits, branches, worktrees, pushes, app launch, live models, credential or profile reads, Windows host runs or interop with real servers.
- **Upstream fact checked this session:** the [IANA IPv6 Special-Purpose Address Registry](https://www.iana.org/assignments/iana-ipv6-special-registry) lists `64:ff9b::/96` (RFC 6052) as Globally Reachable = True, and `64:ff9b:1::/48` (RFC 8215) as False.

## Verdict

**NOT CLOSED.** Counts: 0 P0, 0 P1, 0 P2, **10 P3**. All ten are concrete functional issues: 7 CONFIRMED by trace or probe, 3 PLAUSIBLE because they depend on the platform.

- Every original P1 and P2 from 32ba9c4 stays closed on its reported trigger.
- N1, N5, N6 and N8 are closed.
- N2, N3, N4 and N7 are closed for their reported cases, but each has a remaining functional variant below.
- The user requires every actionable bug to be repaired, so this gate cannot be signed off.

## New findings

### S1 (P3) The public/private trust zone misclassifies globally reachable NAT64 and IPv4-mapped public addresses as private

Verdict: classification CONFIRMED by probe; network impact PLAUSIBLE.

- **Where:** `src-tauri/src/native_core/ai/oauth.rs:79`. The clause `segments()[0] & 0xe000 != 0x2000` marks everything outside `2000::/3` as private. That includes `64:ff9b::/96` and IPv4-mapped *public* addresses: the mapped check at `:77-78` only adds private results and never exempts public ones.
- **Probe output (verbatim copy):**
  - `64:ff9b::5db8:d822` (NAT64 form of 93.184.216.34) → `private=true`
  - `::ffff:93.184.216.34` → `private=true`
  - `93.184.216.34` → `private=false`
- **Trigger:** The user is on an IPv6-only network with DNS64/NAT64 (carrier hotspots, IPv6-only enterprise or conference Wi-Fi) and signs in to an IPv4-only public MCP server. The server's name resolves only to synthesized `64:ff9b::/96` addresses.
- **Impact, two ways:**
  1. **Guard bypass.** `ResourcePolicy::from_addresses` sees only "private" addresses and sets `private=true`. Metadata from that remote server may then send registration POSTs and metadata GETs to `http://127.0.0.1:…` (allowed by `secure_url`) or to private literals. That is the P3-12/N7 SSRF class, reopened under this network condition.
  2. **Wrong refusal.** A public resource with native AAAA records whose authorization server is IPv4-only is refused with "Remote sign-in metadata cannot access a private or local address".
- **Fix:**
  - Classify `64:ff9b::/96` and `::ffff:0:0/96` by their embedded IPv4 address.
  - Treat `64:ff9b:1::/48` as private.
  - Add the NAT64 and mapped-public cases to `private_hostname_resource_is_pinned_and_public_metadata_cannot_rebind`.

### S2 (P3) The first explicit Test after a server crash fails with "test it again" instead of reconnecting

Verdict: CONFIRMED by trace.

- **Where:** `ext/mod.rs:384-391`. This check runs before the `explicit_test` handling at `:398-404`.
- **Trigger:**
  1. A running stdio client crashes. The stdout reader sets `crashed` and cancels `stop`, but the entry stays in `clients`.
  2. No run touches the connection afterwards, for example because the crash happened during the run's last call or while idle.
  3. The user presses Test. `ai-extensions:test` clears failures (`:1510-1516`). Then `ext_connect` finds the closed, crashed client, removes it and returns "The connection process exited. It will restart with backoff; test it again to reset failures."
- **Impact:** The explicit Test, whose job is to restart, fails. That text is saved as the row error, and the second Test works. This conflicts with N3's "Explicit Test … reconnects". The fixture `cached_era_startup_crash_retains_stderr_and_explicit_test_reprobes` only covers a crash during startup, so it misses this.
- **Fix:** When `explicit_test` is set, remove the crashed client and continue to `Client::connect`. Add a test where a client crashes after startup and the next step is Test.

### S3 (P3) Startup-crash stderr is kept only as the first 500 characters, so Show log loses the actual error line

Verdict: CONFIRMED by trace.

- **Where:**
  - `ext/mcp/client.rs:293-300`: the crash error includes the whole log ring (up to 200 lines × 2000 chars).
  - `ext/mod.rs:341`: failures store `cut(e,500)`.
  - `ext/mod.rs:1520`: the Test arm also uses `cut(…,500)`.
  - `ext/mod.rs:1321-1328`: when no live client exists, `ai-extensions:logs` falls back to that 500-character failure text. The failed client and its ring were dropped.
  - `cut` keeps the head (`ai/mod.rs:147`).
- **Trigger:** A Python stdio server fails on import. A normal multi-frame traceback runs past 500 characters, and its final line (`ModuleNotFoundError: No module named 'git'`) is at the end. Long `uvx` or `npx` install preambles behave the same way.
- **Impact:**
  - Neither the row nor Show log shows the cause.
  - Spec §10.4 (`NATIVE_AI_SPEC.md:1032`) promises "the short reason plus … up to 20 scrubbed lines".
  - The row reason is also not short: it carries 500 characters of raw log.
  - The fixture's stderr is a single short line, so it can't catch this.
- **Fix:** Store a short reason ("The program exited during startup", with exit status) separately from the last 20 scrubbed stderr lines of the failed client, keyed by connection. Serve those lines from `ai-extensions:logs`.

### S4 (P3) After a reset, secret-based connections show "Sign in required" (with no way to sign in), and extension provider credentials are archived without any status

Verdict: CONFIRMED by trace.

- **Where:**
  - `ext/mod.rs:1250-1262`: sets `needsAuth=true` for every connection with env or header secret refs or token auth.
  - `AiExtensionsSection.tsx:746-751`: a failure with any `needsAuth` renders "Sign in required" and hides the saved error "Enter the secret again, then test this connection.". The Sign in button exists only for OAuth (`:726`).
  - Extension-contributed provider secrets (`provider_key` namespace, `contributions.rs:118-121`) are lost by the same archive, but nothing flags them. The extension stays enabled, and the next chat fails with "Missing provider credential: …".
  - The Settings copy says only "enter connection credentials again".
- **Trigger:** Press Reset saved connection secrets with a stdio connection using `env` secrets, or a token connection, and/or an extension provider with a secret header.
- **Fix:**
  - Set `needsAuth` only for OAuth, or render "Sign in required" only when `auth.type==='oauth'`. Otherwise show the saved "Enter the secret again" text.
  - Mark affected providers in the reset edit (or surface their missing-secret state), and mention providers in the copy.

### S5 (P3) Windows: a key protected by DPAPI that can never be decrypted again has no recovery path in the UI

N4/P2-1 is closed for this platform only by documentation. Verdict: CONFIRMED by trace.

- **Where:**
  - `ext/secrets.rs:262-324`: any `CryptUnprotectData` failure returns `Err("Windows couldn't access connection secrets")`.
  - `reset_available` and `reset_missing` (`:51-80`) accept only `Ok(None)`.
  - `key()` then fails on every `put(Some)`.
- **Trigger:** These are the original P2-1 triggers: a Windows profile or config folder copied to another machine or user, or an administrator password reset that invalidates the DPAPI master key. `ai-extension-key.bin` exists but can never be decrypted again.
- **Impact:**
  - No secret can be saved again.
  - Reset is not offered.
  - `AI_EXTENSIONS.md` says an inaccessible store "must be made accessible", which is impossible here.
  - The only way out is deleting files by hand.
  - macOS is fine: a missing Keychain item gives `-25300` → `Ok(None)`.
- **Fix:**
  - Distinguish permanent DPAPI failures (`GetLastError` `NTE_BAD_KEY_STATE` 0x8009000B, `ERROR_INVALID_DATA`) from transient ones, and allow the same archive-based reset for them, still with no plaintext.
  - At minimum, document the manual recovery for Windows.

### S6 (P3) Windows: relative stdio commands resolve against Tezbar's own process folder, not the connection's working folder

Verdict: CONFIRMED by trace.

- **Where:**
  - `process.rs:570-571`: a binary with more than one path component gets `dirs = [PathBuf::new()]`, and `is_file()` is then checked relative to the Tezbar process cwd.
  - `client.rs:122-128` sets `current_dir(cwd)` only on the returned command.
  - `manifest.rs` allows relative commands. Project manifests default `cwd` to the project folder (`project.rs:57-63`).
- **Trigger:** A project manifest with `"command": "node_modules\\.bin\\my-mcp"` or `".\\tools\\server.exe"` on Windows.
- **Impact:** "Program not found on PATH". On Unix, Rust spawns the same relative path after `chdir`, so the manifest works on macOS and fails on Windows.
- **Fix:** Resolve relative multi-component commands against the effective `cwd`, or refuse relative commands with a clear message on both platforms.

### S7 (P3) Windows: an argument passed to a `.cmd`/`.bat` shim that ends in a backslash is changed or merged by the target's argv parsing

Verdict: PLAUSIBLE (MSVC CRT rules; not run on Windows).

- **Where:** `process.rs:617-623` builds `"\"…\" \"{a}\""` with `raw_arg`, and only refuses `% ! ^ & | < > " CR LF NUL`.
- **Trigger:** `npx -y @modelcontextprotocol/server-filesystem "C:\Users\me\Projects\"`. The trailing backslash is common in pasted folder paths.
- **Impact:**
  - cmd passes `%*` through, and node's CRT parser reads `\"` as a literal quote. The path becomes `C:\Users\me\Projects"`, and any following arguments are merged into it.
  - No command injection, because cmd metacharacters stay refused. But the arguments differ from the reviewed argv.
  - Rust std's own batch quoting doubles trailing backslashes for this reason.
- **Fix:** Double any run of backslashes that comes right before the closing quote. Add a fixture with a trailing backslash.

### S8 (P3) Explicit Test kills an active client's in-flight calls, and a Test that joins a start already in progress skips the re-probe

Verdict: CONFIRMED by trace.

- **Where:**
  - `ext/mod.rs:398-403` calls `cached.shutdown()` unconditionally on Test.
  - `ext/mod.rs:325-326`: a concurrent start for the same key is shared, so an explicit Test waiting on a non-explicit start gets that start's cached-era result.
- **Trigger:** While an agent run is inside a long MCP tool call, the user presses Test for that extension in Settings.
- **Impact:**
  - The stdio reader drains waiters, and the run's call ends with "Connection lost. The outcome of this call is unknown." There is no replay, so it is safe, but it is an avoidable unknown outcome.
  - In the shared-start race, Test doesn't re-detect an upgraded era.
- **Fix:**
  - Connect the probe client first and swap it in when it succeeds. Retire the old client only when `active==0`, or after its calls finish.
  - Give explicit-Test starts their own `starting` key, or have them wait for the pending start and then start again.

### S9 (P3) Step-up `requestedScopes` survive a change to a different server URL

Verdict: CONFIRMED by trace.

- **Where:**
  - `ext/mod.rs:1654-1675` restores `requestedScopes` by connection ID before checking `changed`. The changed-pin branch (`:1677-1680`) clears `cachedEra`, `tested` and `allowances`, but not `requestedScopes`.
  - `ext/mcp/oauth.rs:179` then puts `requestedScopes` ahead of the challenge and `scopes_supported`.
- **Trigger:** A 403 `insufficient_scope` from server A, then the user edits the connection URL to server B and signs in.
- **Impact:** Server B receives server A's scopes. Authorization servers reject unknown scopes (`invalid_scope`), so sign-in fails until the user enters Advanced scopes by hand.
- **Fix:** Clear `requestedScopes` when the URL or auth pin changes.

### S10 (P3) The stderr reader can no longer be cancelled; a descendant that escaped the process group holds the task, and each such task adds 5 s to app shutdown

Verdict: PLAUSIBLE.

- **Where:**
  - `ext/mcp/client.rs:172-196`: the N3 change removed the `killed.cancelled()` branch, so the reader ends only at EOF.
  - On Unix, `terminate` signals the process group. A descendant that called `setsid` and inherited stderr keeps the pipe open.
  - `AiCore::shutdown` (`ai/mod.rs:211-219`) waits 5 s for each task, one after another.
- **Impact:** One leaked task and pipe per such connection start. App quit is slower by 5 s per stuck reader. The Windows Job Object closes all descendants, so this is Unix only.
- **Fix:** After the stdout task's `terminate`, bound the stderr drain: `select!` on `stop` plus a short grace period, keeping the N3 final-line flush.

## Per-ID closure

### Standards 118cb7b re-review (N1–N8)

| ID | Verdict | Evidence (production path) |
| --- | --- | --- |
| N1 | **Closed** | `ResourcePolicy::new` resolves the resource once per flow, and all endpoints are DNS-pinned with no proxy and no redirect (`oauth.rs:109-170`). The resource probe, protected-resource metadata, issuer, RFC 8414/OIDC metadata, registration and token exchange all use `policy.http` (`ext/mcp/oauth.rs:29-180`, `oauth.rs:269`). A private hostname resource allows private endpoints. A public one refuses any private address. Mixed answers fail. Refresh re-resolves and refuses a public→private change via `resourcePrivate` (`ext/mcp/oauth.rs:275-281`). Private-zone metadata derives from a TLS-validated resource host, or from loopback. Residual NAT64 issue: S1. |
| N2 | **Closed** for the npx/npx.cmd layout | Extensionless names try PATHEXT only, and explicit extensions are literal (`process.rs:575-592`). The Windows fixture includes the shell-script sibling. cmd.exe runs through `process::command` (suspended, Job Object, `kill_on_drop`), so the cmd → node tree is job-owned. New Windows variants: S6, S7. |
| N3 | **Not fully closed** | Closed: cached-era initialize failure maps to "exited during startup" after the stderr drain (`client.rs:314-337`); the final unterminated stderr line is flushed; explicit Test drops `cachedEra`. Open: logs keep only the first 500 characters (S3); the first Test after a post-start crash fails (S2); Test kills in-flight calls and can skip the re-probe in a race (S8). |
| N4 | **Closed for confirmed missing key (macOS/Ok(None))**; Windows variant S5, copy/provider gap S4 | `reset_available`/`reset_missing` act only on `Ok(None)` with ciphertext present, under the store lock. Ciphertext is renamed to `*.bin.unreadable-<ms>-<uuid>`. A new key is saved, and on save failure the rename is rolled back. `revoked.json` is removed. The reset edit invalidates `tested`/auth and stops starts/clients only for secret-bearing connections. Key-store access denied → refused, and the ciphertext bytes are unchanged (test). No plaintext path exists. |
| N5 | **Closed** (minor variant S9) | A 403 writes `requestedScopes` (`ext/mod.rs:1067`). It is excluded from `command_pin` (`:202`) and restored on update. Sign-in order: configured, requested, challenge, supported. No tool replay. |
| N6 | **Closed** | Review shows title, the full annotations JSON (`readOnlyHint`/`destructiveHint` labelled) and `outputSchema` for both Before and After (`AiExtensionsSection.tsx:847-862`). `accept-review` still requires the displayed `{pins, trustPin}`. Fields such as `_meta`/`icons` are hashed but not rendered; I don't consider them execution-relevant here. |
| N7 | **Closed for the reported ranges**; new variant S1 | Covered: CGNAT, 198.18/15, 192.0/16, 240/4, 0/8, TEST-NETs, 192.88.99, private IPv4-mapped, ULA, link-local, documentation and 6to4 IPv6 (probe). Over-classification of NAT64 and public mapped addresses: S1. |
| N8 | **Closed** | `needsReview` produces "Tools changed in X. Review them in AI settings." (`ext/mod.rs:803-810`). |

### Original standards report (32ba9c4)

| ID | Verdict at 1623d47 |
| --- | --- |
| P1-1 | **Closed.** `review-project` queues `(cwd, nonce)` in native state. `list` returns it without consuming it. Settings scrolls once per nonce and drops stale loads (`loadRevision`). Trust, Keep disabled and Dismiss clear only the matching nonce, atomically under the mutex (`finish_project_review`). A failed trust doesn't clear it. Concurrent reviews keep the newer request. |
| P1-2 | **Closed.** Previews show the exact argv, cwd (the default project folder is filled in), env and headers with masked secrets, and flag execution-altering names. Project review shows all contributions. |
| P1-3 | **Closed.** Revocation is idempotent, and `ext_edit` emits only when data changes. |
| P1-4 | **Closed.** `policy.rs` is unchanged in this range. |
| P1-5 | **Closed.** |
| P2-1 | Same as N4: closed on macOS; S4/S5 remain. |
| P2-2, P2-3, P2-5, P2-6, P2-9, P2-11, P2-12 | **Closed.** No regression in this range. The contribution filter, profile fail-closed path and canonical URL comparison are unchanged. |
| P2-4 | **Closed** (with N6). |
| P2-7 | **Closed**; N5 storage is fixed. |
| P2-8 | Same as N3: **not fully closed** (S2, S3). |
| P2-10 | **Closed.** Residual PLAUSIBLE limit: a server that exits on the unknown probe. |
| P3-01…P3-07, P3-09…P3-17 | **Closed.** No regression found. The only P3-12 SSRF variant is S1. |
| P3-08 Windows | Same as N2: closed for the npx case; S6/S7 are new Windows variants. |

### N-R4 (project review nonce, owned by the runtime reviewer; ownership re-checked here)

**Closed.** Native clearing is nonce-scoped and atomic. Trust clears only after `trust_project` succeeds. The renderer passes the nonce it displayed. Viewing grants no trust and starts nothing (trusted records are added disabled and untested).

## Rechecked without new findings

- **Project trust:**
  - Idempotent re-trust of the same pin.
  - A changed pin removes old same-file records through `ai-extensions:remove` (client stop, secret revocation).
  - The `needsProjectReview` flag is cleared.
  - The raw-bytes plus folder pin.
  - Refusal of a manifest that points outside the folder.
- **Snapshot and tools:**
  - Per-run tool snapshot gating (`tested`, `needsReview`, `trustPin`).
  - Disabled extension providers are refused at header resolution.
  - Persistent patches strip contributions.
- **MCP calls:**
  - No `tools/call` replay on 401/403/404/EOF/timeout.
  - One sanctioned OAuth refresh retry.
  - Redirects are disabled on credentialed clients.
- **Process lifecycle:**
  - The stdout reader terminates the owned group/job, even after the root has exited.
  - Idle stop after 10 min.
  - The disable sweep runs only on enabled→disabled transitions.
- **No Pi/Node/npm/Bun/OpenCode built-in fallback:** the only spawns in `native_core/ai` are the shell tool, user-configured MCP stdio, the voice/OCR helpers, the login-shell PATH probe and test fixtures. Everything goes through `process::command`.
- **Redaction split:** keyword redaction stays on errors and logs (`redact_text`). Model output uses `redact_output` (W1, wire scope).

## Standards / judgment (not closure requirements)

- **AGENTS.md:** requires typecheck and tests to pass. The parent's recorded results (Mac 598/9, Windows archive 527/8, TS 918/9, typecheck/frontend) are accepted; I did not rerun them. No Actions, release or signing was in scope.
- **Judgment:**
  - `ai-extensions:list` now performs an OS key-store read on every list while ciphertext exists (`reset_available`). That adds a Keychain prompt surface on ad-hoc builds. Consider caching the result until the next secret operation.
  - From the earlier reports and still present: `ext/mod.rs` keeps growing (Divergent Change), untyped `Value` records combined with a hand-kept `command_pin` exclusion list (Primitive Obsession/Shotgun Surgery; `requestedScopes` had to be added in three places), and duplicated "secret-bearing connection" predicates (reset vs. secretless start).
  - None of these is a hard rule violation. A typed `Connection` with explicit `TrustInputs`/`Bookkeeping` structs would prevent the S9 class.

## Remaining unproven interop limits

- No real NAT64 network, intranet OAuth server or authorization server was contacted. S1's network impact rests on DNS64 behavior.
- No Windows host run. S5–S7 rest on DPAPI and MSVC CRT semantics and on the Rust std behavior for relative programs.
- MCP step-up: replacing the requested scopes with only the challenge scope (rather than a union with granted scopes) is untested against real servers. The upstream guidance was not fetched this session (unverified).
- No real stdio servers (Python/Node/uvx/docker) were exercised for startup-crash log shape, or for servers that exit on the unknown discover probe.
- No visual QA in either theme.
