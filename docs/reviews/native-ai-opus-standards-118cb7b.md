# Native AI repair re-review: standards, security and extensions (Opus 5.5)

- **Worktree:** `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-rust-agent`. Read-only.
- **Pinned HEAD:** `118cb7b4c994cd766503b9078a7d836aad311958` (verified, clean). Initial reviewed source `32ba9c4e`; base `v0.3.0-beta.7` = `e2467521`.
- **Repair diff reviewed:** `git diff 32ba9c4..118cb7b` (commits `9b76b95`, `e672ecb`, `fcabc38`, `118cb7b`), limited to standards/security/extension scope.
- **Inputs:** original report `docs/reviews/native-ai-opus-standards-32ba9c4.md` (unchanged), ledger `docs/reviews/NATIVE_AI_REVIEW_FIXES.md`.
- **Method:** I traced the actual call sites in the current source. I did not take ledger claims or helper-only tests on trust.
  - No edits, builds, cargo/pnpm runs, app launches, network/AI calls, credential or profile reads, branches, worktrees or pushes.
  - I ran no cargo in the repo, to avoid contending with the parent's in-progress test runs on the shared target directory.
- **XERJ:** one bounded `autoindex` attempt (prefix `ax-65b00e69da24`). It stalled at 29% on `docs/performance/beta17-renderer-results.json` (4.1 MB). I stopped it after 75 s and used `rg` plus direct reads.
- **Test status:** I make no claim about the pending Mac full/MSRV/lifecycle or Windows runs.

## Verdict

**Not closed.** I found 4 actionable P2 issues and 4 P3 issues. Two of the P2s are regressions introduced by the repairs (OAuth private-DNS guard, cached-era startup error). Two P2s are original IDs that are only partly closed (P2-1, Windows P3-08 → P2 here because the claimed repair fails for the main case). There is no P0 or P1.

---

## Findings

### N1 (P2, regression from the P3-12 repair): OAuth sign-in can't work for any MCP server whose hostname resolves to a private address — CONFIRMED by trace

- **Where:**
  - `src-tauri/src/native_core/ai/oauth.rs:71-78`: `local` is true only when the resource host is the literal `localhost` or an IP literal.
  - `oauth.rs:84-100`: for every other host, all resolved addresses must be public, or it fails with "Remote sign-in metadata cannot access a private or local address".
  - Callers: `ext/mcp/oauth.rs:29` (the very first probe of the resource itself, `resource_http(&resource,&resource)`), plus `:63`, `:100`, `:162`, `:271` (refresh) and `oauth.rs:200` (token exchange).
- **Trigger:** An intranet, VPN or split-DNS MCP server with browser sign-in, for example `https://mcp.corp.example/mcp` resolving to `10.x`, `172.16/12` or `192.168.x`. This is the normal deployment for company-internal OAuth MCP servers.
- **Failure:**
  - Sign in fails immediately on the resource probe, before any metadata is fetched. A saved grant also can't refresh (`:271`).
  - The same server works with an access token. It also works by IP literal (`https://10.0.0.5/mcp`), because `local` is then true. The guard is inconsistent and blocks a legitimate primary flow. Before `118cb7b` this flow worked.
- **Fix:**
  - Resolve the **resource** host too, and pin it. If the resource itself resolves to private or loopback addresses, allow private metadata, token and registration endpoints (same trust zone, still DNS-pinned with no redirects).
  - Reject private destinations only when the resource is public.
  - Add a fixture that uses a resolver override, so a hostname resource maps to `127.0.0.1` / `10.x`.

### N2 (P2, P3-08 not closed): Windows `npx` (and every npm-installed bin) resolves to the extensionless shell shim and can't start — CONFIRMED by trace; the OS error itself is from Windows semantics

- **Where:** `src-tauri/src/native_core/ai/process.rs:494-514` (`windows_mcp_command`). The candidate extensions are `once(String::new()).chain(PATHEXT)`, so the bare name is tried **first** in each directory.
- **Trigger:** The canonical imported Claude-Desktop config `"command":"npx"` on Windows.
  - The Node.js Windows install dir (`C:\Program Files\nodejs`) ships `npx` (a bash script for Git Bash/Cygwin) next to `npx.cmd` and `npx.ps1`.
  - The same applies to `npm`, `corepack`, and every global `npm i -g` bin in `%APPDATA%\npm` (cmd-shim writes `foo`, `foo.cmd`, `foo.ps1`). `where npx` lists both.
- **Failure:**
  - `d.join("npx").is_file()` matches the shell script, so `batch=false` and `command("…\\nodejs\\npx")` is called directly.
  - `CreateProcessW` on a non-PE file fails with os error 193 ("%1 is not a valid Win32 application").
  - cmd.exe never runs extensionless files. It tries the PATHEXT extensions only.
  - The repair test `pathext_cmd_programs_accept_literal_arguments_and_refuse_injection` creates only `npx.cmd`, so it can't catch this. The parent's Windows source validation will only see it if it exercises a real Node layout.
- **Fix:**
  - When `binary` has no extension, try only the PATHEXT extensions (cmd semantics), and do not accept the bare file.
  - When it has an extension, accept it as-is.
  - Add an extensionless `npx` sibling to the Windows fixture.

### N3 (P2, P2-8 not closed for the common case): startup crash of a previously tested legacy-era stdio server again reports "Connection lost. The outcome of this call is unknown" with no log — CONFIRMED by trace

- **Where:**
  - `ext/mcp/client.rs:211-216`: with `cachedEra` `2025-*`, the discover probe is skipped and `Err` is returned instantly.
  - `:267-280`: the new `crashed()` check runs **before** the legacy handshake, so the process hasn't died yet.
  - `:291`: `initialize` fails through `?` with the drained-waiter text "Connection lost. The outcome of this call is unknown.", or "Connection was disabled or cancelled" if the process is already gone when `call_prepared` runs.
  - The explicit Test passes the stored record (with `cachedEra`) through `ext_discover_mode` → `ext_connect`, so Test also skips the probe and the crash mapping.
  - Logs: `ext/mod.rs` `ai-extensions:logs` falls back to `record.error` only. That is the misleading text, not the stderr ring of the dropped client.
- **Trigger:**
  - Real stdio servers today negotiate 2025-era versions, so after the first successful Test nearly every one has `cachedEra="2025-11-25"` (`ext/mod.rs:679`).
  - The server's startup then breaks: package removed, Python env broken, missing env after an update.
- **Failure:** The row reads "Couldn't start: Connection lost. The outcome of this call is unknown." and "Show log" has no stderr. These are exactly the original P2-8 symptoms. The fixture `failed_start_preserves_reason_and_log_without_call_replay` only covers a fresh, uncached connection.
- **Fix:**
  - In `Client::connect`, map any handshake error (discover **or** initialize) to "The program exited during startup…" when `client.crashed()` is true, after briefly letting the stderr reader drain.
  - Keep the failed client's scrubbed log lines for `ai-extensions:logs`.
  - Ignore `cachedEra` on an explicit Test, so Test re-probes. This also lets a server that was upgraded to the modern era be re-detected.
  - Add a cached-era crash fixture.

### N4 (P2, P2-1 partly closed): after the connection key is lost, no secret can ever be saved again, and the new copy points at a non-fix — CONFIRMED by trace

- **Where:**
  - `ext/secrets.rs:40-50`: `key()` errors whenever the key is missing and `ai-extension-secrets.bin` exists.
  - `:92-117`: `put(Some)` calls `load()` and fails.
  - `put(None)` now only appends to `ai-extension-secrets.revoked.json`. It never retires the unreadable ciphertext.
- **What was fixed:** secretless start, Test and Remove now work. The secretless test proves it.
- **What remains:**
  - Adding a connection with a token or secret-like env, `set-secret`, provider "Save key", and OAuth `persist_mcp_oauth` all fail permanently.
  - The new message says "Restore the key … or remove the affected connection". But removing every connection leaves the `.bin` in place, so `key()` still fails.
  - The only recovery is deleting the file by hand. The original fix asked for an explicit reset action. No reset exists in native code or UI.
- **Fix:**
  - When the key store returns `Ok(None)` (permanently missing, as opposed to an access-denied error), offer "Reset saved connection secrets" in Settings.
  - The reset should rename the `.bin` to `*.unreadable-<ts>`, clear `revoked.json`, create a new key, and mark connections with secret references `tested=false` with a clear "Enter the secret again" status.
  - Change the copy to match. Add a native test that loses the key, then adds a token connection.

### N5 (P3, introduced by the P2-7 step-up repair): a 403 `insufficient_scope` writes server scopes into the trust-pinned `auth` block, which silently untrusts the connection — CONFIRMED by trace

- **Where:**
  - `ext/mod.rs:1053` sets `c["auth"]["scopes"]` from the server challenge.
  - `command_pin` (`ext/mod.rs:177-205`) doesn't exclude `auth`, so `trustPin != command_pin`.
  - The snapshot (`ext/mod.rs:789-795`) then withholds the connection with "is unavailable; test it in AI settings".
- **Trigger:** A tool call gets 403 with `WWW-Authenticate: … scope="…"`, and the user clicks Sign in.
- **Failure:**
  - After a successful sign-in, every run still excludes that connection.
  - The row shows a healthy "N tools · M prompts" with no row-level Test button (`tested` stays true; `AiExtensionsSection.tsx:672,725`). Recovery is only Details › Test connection.
  - A server-supplied value has changed a user-reviewed trust input, and the next Test re-pins it without showing the scopes to the user.
- **Fix:** Store the challenge scopes in a separate bookkeeping field (for example `requestedScopes`, added to the `command_pin` exclusions and restored in `update`). Have `mcp_oauth_start` prefer `auth.scopes`, then `requestedScopes`, then the challenge, then `scopes_supported`.

### N6 (P3, P2-4/R17 display gap): tool-change review hides annotation and output-schema changes, including a `readOnlyHint` flip that changes auto-approval — CONFIRMED by trace

- **Where:**
  - `AiExtensionsSection.tsx:804-822` shows only `description` and `inputSchema` for Before and After.
  - The pin hash covers the whole descriptor.
  - `allowReadOnly` auto-runs a tool based on `annotations.readOnlyHint` (`ext/mod.rs:987`).
- **Trigger:** A server changes a tool's `readOnlyHint` from false to true (or its `outputSchema`/`title`) without touching the description or input fields.
- **Failure:** Review shows "Changed: X" with identical Before and After. The user accepts without seeing that the tool will now run without asking under "Allow read-only tools".
- **Fix:** Render the changed annotation keys, `outputSchema` and `title`, with the `readOnlyHint`/`destructiveHint` change called out explicitly.

### N7 (P3, P3-12 hardening gap): the private-address check misses CGNAT `100.64.0.0/10` (the Tailscale tailnet range) and other special ranges — CONFIRMED by code

- **Where:** `oauth.rs:51-69` (`private`). It doesn't cover `100.64/10`, `198.18/15`, `192.0.0/24`, `240/4`, or `0/8` apart from `0.0.0.0`.
- **Impact:** Limited, because remote endpoints must be https with a valid certificate. But tailnet services exposed via `tailscale serve` do have valid `*.ts.net` certificates, and this developer's machines live on `100.x` (per AGENTS.md). Remote sign-in metadata could therefore direct GETs and registration POSTs to tailnet services.
- **Fix:** Add `100.64.0.0/10` and the reserved/benchmark ranges. Treat a CGNAT resource like a private one under the N1 rule.

### N8 (P3, copy, introduced with the new `needsReview` gate): runs tell the user to "test it" when the connection is actually waiting for tool review — CONFIRMED by trace

- **Where:** `ext/mod.rs:789-795`. `needsReview == true` now shares the "is unavailable; test it in AI settings." warning with untested and pin-mismatch cases.
- **Failure:** Testing doesn't clear `needsReview`. Only Review › Accept changes does, so the instruction sends the user to an action that doesn't help.
- **Fix:** Emit "Tools changed in X. Review them in AI settings." when `needsReview` is the reason.

---

## Per-ID dispositions (original standards report)

| ID | Verdict | Evidence at 118cb7b |
| --- | --- | --- |
| P1-1 | **Closed** | `review-project` queues a canonical cwd in native state and emits an event. `list` without a cwd uses the queue and returns a readable error for removed folders (`ext/mod.rs:1197-1216`). The notice calls it, then opens Settings (`ProjectExtensionNotice.tsx:33`). Settings reloads on the event and scrolls to the section. No sessionStorage. |
| P1-2 | **Closed** | `connectionPreview` (`AiExtensionsSection.tsx:18-32`) renders the JSON-quoted command and argv, the working folder (with the explicit home/project default from native, `ext/mod.rs:1269-1273,1362-1364`, `project.rs:57-63`), every env and header name with its literal value (secret-named values masked), and the auth header. Execution-altering names are flagged. Project review shows instructions, prompts, commands, agents and providers. Relative cwd is rejected (`manifest.rs:174-179`). The flag list is advisory; values are always visible. |
| P1-3 | **Closed** | `revoke_changed_projects` skips records that are already revoked and disabled (`project.rs:123-125`). `Extensions::edit` writes only on change, and `ext_edit` emits only on change (`ext/mod.rs:122-134,298-300`). The test repeats revocation five times and sees one event. |
| P1-4 | **Closed** | `complex` adds `* ? [ ]` and `~user` (`policy.rs:23-25`). `mutation_options` (`--output`, attached `-o`, `@file`, …) asks before remembered and Turbo approval (`policy.rs:74-91`). The probe cases are in the matrix test. On Windows, literal `\` is allowed and PowerShell `$`/backtick stay complex. |
| P1-5 | **Closed** | The raw `argsText` is split only when saving or testing (`AiExtensionsSection.tsx:144-145`). |
| P2-1 | **Not closed** (partial) | Secretless start, Test and Remove are fixed. Reset/recovery is missing. See **N4**. |
| P2-2 | **Closed** | `filter_extension_patch` strips contributed providers, models and configs from persistent patches. Reads drop legacy persisted `_extensionId` entries. `extension_provider_headers` requires an enabled owner (`contributions.rs:6-117`). |
| P2-3 | **Closed** | An unresolved profile fails before the provider request (`ext/mod.rs:2017`; `missing_profile_fails_closed_before_provider_request`). |
| P2-4 | **Closed** for bookkeeping, plus display gap **N6** | `pinnedCatalog` is fixed at approval and `previousCatalog` derives from it (`ext/mod.rs:657-670`). `update` restores all bookkeeping by connection ID and resets only connections whose pin changed (`:1514-1610`). `accept-review` requires the exact displayed `{pins, trustPin}` (`:1612-1640`). The UI diffs by full descriptor and lists removed tools. |
| P2-5 | **Closed** | Discovery and OAuth persist clear `error`/`needsAuth` (`ext/mod.rs:672-673`, `mcp/oauth.rs:186-187`). The row reads `needsAuth`. |
| P2-6 | **Closed** | URLs are canonicalized on add and update (`ext/mod.rs:1360,1539`) and compared canonically in `ext_connect` and refresh. The root path is equal to `/` through `Url` parsing. |
| P2-7 | **Closed** for empty scope, challenge and `scopes_supported` | Scope is omitted when empty (`oauth.rs:138-142`). Selection order is configured, then challenge, then `scopes_supported` (`mcp/oauth.rs:176`). There is no auto-replay after 403. The step-up storage side effect is **N5**. |
| P2-8 | **Not closed** for the cached-era path | The fresh-start path and the 404 mapping are fixed. See **N3**. |
| P2-9 | **Closed** | Validation is 1–1800 (`manifest.rs:165-169`), runtime clamps at `ext/mod.rs:1032` and `client.rs:442-449`, and the raw text field defaults when cleared. |
| P2-10 | **Closed** (silent-probe cold budget) | The legacy era is cached per trusted connection and cleared when the pin changes (`ext/mod.rs:1592`). Residual PLAUSIBLE risk, unchanged from the original report: a server that *exits* on the unknown probe never connects. Slow first `npx -y` starts can still miss 5 s (judgment). |
| P2-11 | **Closed** | There is a Protocol select for the provider and for each model (`SettingsView.tsx:2234-2245,2501-2510`). |
| P2-12 | **Closed** | The tool description falls back to `description`, then `title`, then `""` (`ext/mod.rs:1917`). |
| P3-01 backoff | **Closed** | A success removes the failure entry (`ext/mod.rs:339`). |
| P3-02 Test cancel | **Closed** | The sweep runs only on enabled→disabled transitions (`ext/mod.rs:277-296`). Explicit Disable and Remove still cancel. A narrow race remains (a start inserting its client just after a concurrent disable) and could leave an idle process until the 10-minute stop; I'm not filing it. |
| P3-03 Lines | **Closed** | The `scanned` cursor is reset correctly on drain and EOF (`process.rs:336-360`). |
| P3-04 list_changed | **Closed**, copy issue **N8** | Calls are no longer refused. A notified connection gets a bounded synchronous rediscovery, and changed tools gate the connection. |
| P3-05 subscriptions/TTL | **Closed** | Listen uses an 86 400 s timeout. TTL is the minimum `ttlMs`, clamped to 1 s–10 min. |
| P3-06 input_required | **Closed** | `call_tool_state` reuses the descriptor mirrors and the configured timeout with a fresh ID (`ext/mod.rs:1076-1086`). |
| P3-07 SSE EOF | **Closed** | Returns an unknown-outcome error (`client.rs:560-562`). No replay. |
| P3-08 Windows | **Not closed** | Literal backslashes, the env allowlist and the batch-injection refusal are fine. PATHEXT ordering breaks `npx`. See **N2**. |
| P3-09 slug | **Closed** | Native `update` restores `slug` and the rest of the bookkeeping by connection ID. The renderer adopts the connections returned by `add`. |
| P3-10 instruction budget | **Closed** | Per-source truncation with a notice (`ext/mod.rs:1933-1951`). Minor: when the budget is full, a later notice can be popped off. Not filed. |
| P3-11 disabled provider | **Closed** | `agent:run` validates the selection after merging scoped contributions (`mod.rs:433-435`). The header path refuses disabled owners. |
| P3-12 OAuth SSRF | **Closed** for localhost/RFC1918/redirect/DNS-rebinding | It introduced **N1** (regression) and has the hardening gap **N7**. |
| P3-13 instruction draft | **Closed** | The `instructionsDirty` ref blocks reload overwrites until the blur save completes. |
| P3-14 orphan secrets | **Closed** for `add` | All confirmations are prevalidated, and the guard revokes only the new namespaces if `add` fails before or during the durable write. |
| P3-15 Always | **Closed** (runtime reviewer's R13) | Not re-traced beyond the policy matrix. |
| P3-16 copy/UI | **Closed** | Profile name, `.ai-extension-error` with `role=alert`, Try again, policy select. No visual QA claim. |
| P3-17 project bookkeeping | **Closed** | Re-trust removes older records for the same file through the normal `ai-extensions:remove` seam (client stop and namespaced secret revocation) and clears `needsProjectReview` (`project.rs:186-232`). Retrusting the same pin is an idempotent flag clear. The retrust test asserts one record and no start. |

### Cross-checked runtime duplicates

- **R6:** closed. Same as P1-1.
- **R7:** closed. Same as P2-2.
- **R12:** closed. Same as P2-3.
- **R14:** closed. Same as P2-11.
- **R17:** bookkeeping closed. The display gap is **N6**.

## Hard standards

- **No-Pi / OpenCode:** no Pi, Node, npm, Bun or OpenCode execution fallback in the scope I reviewed. The only new spawn is the Windows MCP path through `process::command` (suspended, Job Object, `kill_on_drop`).
- **Redirects:** stay disabled on credentialed and OAuth clients.
- **Replay:** there is no `tools/call` auto-replay on 401, 403, 404, SSE EOF or timeout. The only re-send is the existing single OAuth refresh retry.
- **Secrets:**
  - Still no plaintext fallback.
  - `revoked.json` holds namespace IDs only.
  - Exact-value scrubbing is unchanged.
  - The import and project previews mask secret-like literals.

## Coverage

**Read:**

- The full repair diffs for `ext/{mod,project,secrets,manifest,contributions}.rs`, `ext/mcp/{client,oauth}.rs`, the fixture, `oauth.rs`, `process.rs` and `runtime/policy.rs`.
- Current source for:
  - `ext/mod.rs`: lifecycle, `ext_edit`, `ext_client`/`ext_connect`, discovery bookkeeping, snapshot gating, call 401/403 handling, the review-project/list/logs/parse-import/add/test/set-enabled/remove/update/accept-review/set-secret arms, and instruction assembly.
  - All of `project.rs`, `secrets.rs` (store logic), `mcp/oauth.rs`, `policy.rs` (to line 283), and `client.rs` connect/handshake/HTTP status handling.
  - Renderer: the `AiExtensionsSection.tsx` form, preview, row, Details, review and instructions; `ProjectExtensionNotice.tsx`; the `SettingsView` protocol select.
- Repair tests checked against what they assert: project trust/revocation, the missing-key secretless test, insufficient scope, failed start, silent legacy era, continuations, the policy matrix, the Windows PATHEXT test, and the OAuth PKCE fixture.

**Not done:**

- No test execution, no Windows host, no live MCP/OAuth/provider interop, no visual QA.
- N2's os-error-193 outcome rests on documented Windows `CreateProcessW` and cmd behavior plus the standard Node and npm Windows layout, not on a run.
- N1 and N3 are code-path proofs.
