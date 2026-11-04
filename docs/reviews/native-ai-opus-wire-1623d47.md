# Tezbar native AI closure audit: provider, wire and auth (Scope A)

Reviewer: Claude Opus 5.5, reviewing the code myself with no delegation or subagents.
Source: `feat/native-rust-agent` at pinned HEAD `1623d47245390b4b32f30c0bed2b44a1d1a721e9`. HEAD was clean before and after the audit (`git status --short` empty). The final renderer source is `ae2fe00` and the native Rust source is byte-identical since `8f70d2e`.
Base: `v0.3.0-beta.7` = `e246752`. Repair diff audited in this scope: `118cb7b..ae2fe00`. The last successful wire review was `118cb7b`.
Original reports `native-ai-opus-wire-32ba9c4.md` and `native-ai-opus-wire-118cb7b.md` were read in full and left unchanged.
No source or doc edits, no branches or worktrees, no push, PR, release or Actions, no app launch, no profiles or credentials, and no paid or live model calls.

## Verdict

**The scope does not close yet.** There are 0 P0, 0 P1, 0 P2 and **5 actionable P3** findings.

- One P3 is fully proven and leaves an earlier repair (W12) only partly closed.
- The other four P3s are proven on Tezbar's side; the upstream reaction is not proven (OPEN INTEROP). Each is small and local.
- All of F1–F30 are closed or validly dismissed.
- W1–W11 and W13–W15 are closed. W12 is partly closed.
- No P0, P1 or P2 issue remains in the provider, wire or auth path. The prior W1 regression (credential-like coding output failing turns) and the W2 regression (unresolvable reasoning replay) are both repaired at the real call sites.

| Priority | ID | Summary | Label |
| --- | --- | --- | --- |
| P3 | N-W1 | New-image validation for first-party Anthropic uses the 32 MB gateway budget, not the 128 MB route budget (W12 partly open) | PROVEN |
| P3 | N-W2 | Chat decoder fails the turn when a later tool-call delta carries `id: null`, `id: ""` or `name: ""` | PROVEN decoder, OPEN INTEROP |
| P3 | N-W3 | Gemini canonical (cross-origin) encoding sends empty `{"text":""}` parts, including a model turn made only of an empty part | PROVEN request shape, OPEN INTEROP |
| P3 | N-W4 | A keyless custom Gemini-protocol provider sends an empty `x-goog-api-key` header | PROVEN request shape, OPEN INTEROP |
| P3 | N-W5 | An Ollama tool call with no `arguments` fails the whole turn | PROVEN decoder, OPEN INTEROP |

## Labels

| Label | Meaning |
| --- | --- |
| **PROVEN** | Reproduced at the real Tezbar call site or function with loopback fixtures and fixture keys. |
| **OPEN INTEROP** | Tezbar's request or decode behaviour is proven, but the provider's reaction was not tested live. |
| **SOURCE** | Established by reading the production path. |

Probe source: `/private/tmp/tezbar-wire-1623d47-probe.rs`. Log: `/private/tmp/tezbar-wire-1623d47-probe.log` (P1–P8; P8 was also logged on its own in `/private/tmp/tezbar-wire-1623d47-probe-p8.log`).

The probes ran from a `git archive` snapshot of `1623d47` at `/private/tmp/tezbar-wire-1623d47-snap`. The probe module was added there under `#[cfg(test)]`, with the earlier snapshot's resource-stripped `tauri*.json`, `dist` and `build/embedding-runtime`. None of those files changed between `118cb7b` and HEAD. The build used the separate target `/private/tmp/tezbar-wire-118-target`; nothing ran inside the repository and no git worktree was created. All 9 probe tests passed. They print behaviour, so a pass does not mean the behaviour is correct.

## Findings

### N-W1 (P3). New Anthropic images are capped at the gateway budget on api.anthropic.com (PROVEN; W12 partly open)

- **Where:** `src-tauri/src/native_core/ai/runtime/agent.rs:169` calls `limits::validate_images(...)`. That function uses `request_bytes(Anthropic)` = 32 MB (`runtime/limits.rs:16-21`, `:74-77`). The transport correctly uses `route.request_budget` (`wire/transport.rs:18-26`), which `providers/route.rs:314-319` sets to 128 MB for `api.anthropic.com`.
- **Trigger:** a vision Claude model on the first-party host, and a prompt with 3 or 4 attachments of about 9 MB base64 each. That is within the 4-image cap (`legacy.rs:294`) and Anthropic's 10 MB per-image limit, but about 27 MB in total.
- **Evidence (P8):**
  - `route host=api.anthropic.com request_budget=128000000`
  - `agent.rs:169 validate_images => Err("Native limit: Anthropic image request exceeds 32000000 bytes...")`
  - `validate_images_budget(route budget) => Ok(())`
- **Impact:** the prompt is refused before any request is sent, while the transport and the API (128 MB per the Claude API overview cited in the ledger) would accept it. The W12 ledger entry covers history projection and transport only.
- **Fix:** call `limits::validate_images_budget(&route.origin.protocol, &[user.clone()], route.request_budget)` at `agent.rs:169`. Add a three-image regression through `native_run_history` for the first-party host.

### N-W2 (P3). Chat decoder fails on null or empty IDs and names in later tool-call deltas (PROVEN decoder, OPEN INTEROP)

- **Where:** `wire/chat.rs:121-134`. Any key other than `function` or `index` is copied, and `id` is compared with `old != v`. `function.name` is compared whenever it is a string.
- **Evidence (P1):** the first chunk carries `id:"call_1"` and `name:"read"`. The later argument chunk then varies:

  | Later chunk carries | Result |
  | --- | --- |
  | `"id":null,"type":null,"function":{"name":null,...}` | `Native protocol: changed call ID` |
  | `"id":""` | `Native protocol: changed call ID` |
  | `"function":{"name":"",...}` | `Native protocol: changed tool name` |
  | No `id` (OpenAI's shape) | OK |

  With `type: null`, the null would also be copied into the replayed raw `tool_calls`.
- **Reference:** local `@earendil-works/pi-ai` 1.3.0 `openai-completions.js:296-345` and `:442-446` takes an ID only when it is truthy (`if (!block.id && toolCall.id)`), and a name only when the block has none.
- **Upstream:** I believe some OpenAI-compatible proxies and serializers emit explicit nulls on continuation chunks, but I did not confirm a specific gateway. That part is unverified.
- **Impact:** an otherwise valid tool turn fails with no retry. No effects run, so this is fail-closed.
- **Fix:** ignore `null` or empty `id`, `type` and `name` on continuation deltas. Keep rejecting a real non-empty conflicting ID or name.

### N-W3 (P3). Gemini canonical encoding emits empty text parts (PROVEN request shape, OPEN INTEROP)

- **Where:** `wire/gemini.rs:79` pushes every `Block::Text` without checking for blank text. Empty canonical text comes from two decoders:
  - The Gemini decoder records signature-only `{"text":"","thoughtSignature":...}` parts as `Text("")` (`wire/mod.rs:433-440`).
  - The Anthropic decoder keeps blank text blocks as canonical `Text` (`wire/mod.rs:387`). W5 filtered only the raw same-origin Anthropic replay.
- **Trigger:** history made on one model (Anthropic, or another Gemini model or provider) is sent to a Gemini-protocol model. Switching models, or Antigravity model fallback, takes the canonical path.
- **Evidence:**
  - **P2:** foreign Anthropic history encodes as `{"parts":[{"text":""}],"role":"model"}`. That is a model turn containing only an empty part.
  - **P2b:** a Gemini signature-only part then a model switch encodes as `{"parts":[{"text":""},{"text":"answer"}],"role":"model"}`.
- **Reference:** pi-ai `google-shared.js:194-199` explicitly skips empty text parts that carry no signature.
- **Upstream:** I believe Gemini/Vertex can reject empty text parameters, but I did not verify this with current documentation or a live call.
- **Fix:** skip blank `Text` parts in the Gemini canonical encoder. A canonical block never carries a signature, so nothing is lost. Do not emit a content entry whose parts end up empty.

### N-W4 (P3). Keyless custom Gemini sends an empty `x-goog-api-key` (PROVEN request shape, OPEN INTEROP)

- **Where:** `providers/route.rs:266-267` sets `x-goog-api-key` to the key even when it is empty. Custom providers may be keyless (`route.rs:190-195`).
- **Evidence (P3):** the headers include `"x-goog-api-key":""`. The equivalent Anthropic route omits `x-api-key` (the F29 repair).
- **Impact:** a keyless local proxy that treats a present but empty key as invalid rejects every request. That reaction was not confirmed. A configured custom header with the same name still replaces the empty one, because it is inserted later.
- **Fix:** add the header only when `!key.is_empty()`, matching the Anthropic branch.

### N-W5 (P3). Ollama tool call with no `arguments` fails the turn (PROVEN decoder, OPEN INTEROP)

- **Where:** `wire/mod.rs:323-324` does `c["function"]["arguments"].to_string()`. A missing value becomes `"null"`, and `object_args` then rejects it.
- **Evidence (P6):** an NDJSON stream with `tool_calls:[{"function":{"name":"screen_capture"}}]` returns `Err("Native protocol: tool arguments must be an object")`.
- **Context:** F19 normalized missing or empty arguments on Chat, Responses and Gemini, but not on Ollama. Whether current Ollama ever omits `arguments` is unconfirmed; its documented examples always carry an object.
- **Fix:** treat missing or `null` arguments as `{}` on Ollama, using the same `argument_text` rule. Keep rejecting non-object values.

## Original F1–F30 verdicts (checked at `1623d47`)

| ID | Verdict | Current evidence |
| --- | --- | --- |
| F1 | Closed | Copilot has no generic-key fallback. `available()` (`provider.rs:90-95`) and `native_route` (`route.rs:98-101`) require a Copilot token; the regression asserts no request is made. |
| F2 | Closed | Go default `https://opencode.ai/inference/go` (`route.rs:166-172`). The spec is corrected (`NATIVE_AI_SPEC.md:822`). |
| F3 | Closed | Blank Anthropic argument buffer becomes `{}` (`mod.rs:109`, `:395-412`). |
| F4 | Closed | Null deltas are ignored (`chat.rs:68`). DeepSeek replay adds `""` (`chat.rs:11-16`, `:40-42`, `transport.rs:34-40`). |
| F5 | Closed | Image follow-ups are flushed after the contiguous tool run (`chat.rs:4-8`, `:47`). |
| F6 | Closed | Indexed `reasoning_details` merge (`chat.rs:72-101`). |
| F7 | Closed | Antigravity empty final answer is retryable (`transport.rs:255-258`). |
| F8 | Closed | Model fallback is tried before the endpoint walk, with separate budgets (`transport.rs:337-391`). |
| F9 | Closed | The default Antigravity URL stays implicit (`config.rs:118-121`). |
| F10 | Closed | HTTP errors read `error.message`, string `error`, `message` and `detail`, redacted and bounded (`transport.rs:141-157`). |
| F11 | Closed | Typed stream errors keep their message and are retried before visible text (`mod.rs:116-143`). 529 is in the retry list. |
| F12 | Closed | Official host sends `max_completion_tokens` (`transport.rs:41-45`). |
| F13 | Closed | Custom status and catalog are built from `native_route` (`provider.rs:30-70`). |
| F14 | Closed | Protocol control exists; rediscovery preserves it (W4); stale free defaults removed (W6). All 5 OpenCode defaults are in the catalog (programmatic check). |
| F15 | Dismissal upheld | OpenCode keeps Bearer on all families (`route.rs:266-269`), per the inference docs cited in the 118 report. |
| F16 | Closed | `include` is always sent, with unresolvable reasoning stripped on replay (W2). See uncertainty U1 on the fallback trigger. |
| F17 | Closed | Empty `completed.output` keeps streamed items and rebuilds text (`responses.rs:82-122`). |
| F18 | Closed | Claude and GPT-OSS get IDs on both sides; Gemini gets no synthetic ID (`gemini.rs:65-71`, `:94-106`). |
| F19 | Closed for its original codecs (Chat, Responses, Gemini) | `argument_text` (`mod.rs:97-107`). Ollama is not covered (N-W5). |
| F20 | Closed | Header wait is 30 s remote, 120 s local, 300 s maximum (`transport.rs:463-473`). |
| F21 | Closed | `error: null` is ignored (`mod.rs:224`, `:277`). |
| F22 | Closed | Per-image Anthropic limit is 10 MB base64 and recursive. The budget value is N-W1. |
| F23 | Closed | Fails closed only on an exact plausible credential, with placeholders excluded (`redact.rs:9-25`, `transport.rs:452-461`). Signed and encrypted data is never rewritten. |
| F24 | Closed | Fallback list starts with `gemini-3.5-flash` (`provider.rs:407`); recommended default is `gemini-3.5-flash`. This also resolves runtime P3-05. |
| F25 | Closed | Unchanged since 118, where it was closed. |
| F26 | Closed | `{error}`, with "Tool failed" for empty text (`gemini.rs:93`). |
| F27 | Closed | Image-only and assistant-first bridges exist (`gemini.rs:128-138`). |
| F28 | Closed | Unchanged since 118 (`Policy::none` clients). |
| F29 | Closed | Empty Anthropic key header omitted; Copilot revocation repaired; thinking control is capability-gated (W3). The same defect class on Gemini is N-W4. |
| F30 | Closed | Stop reason is checked before parsing arguments (`mod.rs:307-313`). |

## W1–W15 verdicts (checked against `ae2fe00` and `1623d47` code)

| ID | Verdict | Evidence at the production path |
| --- | --- | --- |
| W1 | **Closed** | `opaque_has_secret` checks only exact secrets. `Redactor::new` drops secrets under 8 bytes and the placeholders `none`, `empty`, `lm-studio`, `null`, `not-needed`, `no-key` and `ollama`. Model output uses only the high-confidence shapes in `redact_output` (PEM, `sk-`, `gh*_`, `AKIA`, long Bearer); keyword scrubbing remains only in `redact_text` for errors and logs. The byte-split hold-back for exact secrets is retained (`redact.rs:43-65`). Signed and encrypted fields are never rewritten. Exact-key tool arguments still fail before execution (`transport.rs:261-267`). |
| W2 | **Closed** (U1 remains) | `include` is unconditional (`mod.rs:41-45`). Replay drops reasoning items without `encrypted_content` and strips server `id`s from that message onward while keeping `call_id` (`responses.rs:5-33`). One bounded fallback happens before visible output, below the effect loop (`transport.rs:296-312`). `gpt-[56]` is inferred in both Rust (`config.rs:157`) and TypeScript (`aiProviders.ts:274-279`). |
| W3 | **Closed** | Thinking is gated on the model's `thinking` capability (`route.rs:339-342`). Anthropic shapes are per generation (`mod.rs:48-60`). P7: Haiku, Sonnet and Opus 4/4.1/4.5 get `enabled` with a budget; Opus 4.8, Sonnet 5, Fable 5 and Sonnet 4.6 get `adaptive`. `llama3.2` metadata is `["tools"]`, so `think` is never sent. Only `claude-3-7-sonnet-*` naming misses the legacy pattern; per my understanding that model is retired, so this is not reported. |
| W4 | **Closed** | `SettingsView.tsx:736-744` spreads `prior` before filling capabilities, so `protocol` and `maxTokens` survive. A revision fence stops stale writes. Real Settings test noted in the ledger. |
| W5 | **Closed** for raw replay | `anthropic.rs:13-19` filters blank raw text only. Thinking, redacted and signed blocks are untouched. The canonical-side residue is N-W3. |
| W6 | **Closed** | Default is `opencode/gpt-5.4-mini` (Responses in the catalog), mirrored in TypeScript. Discovery and static fallback exclude `-free`, `big-pickle` and `jev-` (`provider.rs:237-266`). The 403 hint states a possible cause, not a certainty (`transport.rs:313-315`). |
| W7 | **Closed** | Status accepts either the Zen or the Go key (`provider.rs:80-89`). Catalogs query `/v1/models` and `/go/v1/models` with their scoped keys and fall back per family (`provider.rs:197-273`). Route key selection is per family (`route.rs:182`). A configured Tezbar `apiKey` serves both families; whether OpenCode keys are account-wide is unverified, and a mismatch yields an access error, not a credential leak. |
| W8 | **Closed** | Streamed buffers win over empty completion fields (`responses.rs:92-101`). Object arguments serialize; scalars and arrays fail closed (`mod.rs:97-107`, `chat.rs:135-146`). Text is rebuilt into a message item when `output` is empty (`responses.rs:102-122`). |
| W9 | **Closed** | `Decoder::json` checks `error` and `response.error` before any codec (`mod.rs:223-226`). Ollama errors go through `payload` (`mod.rs:277-279`). |
| W10 | **Closed** | Custom native Gemini uses `x-goog-api-key` (`route.rs:266-267`). The empty-key variant is N-W4. |
| W11 | **Closed** | Route (`route.rs:61-67`), status (`provider.rs:130-148`) and catalog (`provider.rs:304-308`) agree on `/v1beta` and `/v1beta/openai`. |
| W12 | **Partly closed** | Transport and history projection use 128 MB at the first-party host, but new-image validation at `agent.rs:169` still uses 32 MB (N-W1). |
| W13 | **Closed** | `requires_reasoning_content` = provider is `deepseek` or host is `api.deepseek.com`/`deepseek.com` (`route.rs:125-126`), applied after encode (`transport.rs:34-40`). This matches pi-ai `openai-completions.js:1238`, `:1274`. |
| W14 | **Closed** | The first non-empty signature is kept (`chat.rs:89-92`). |
| W15 | **Closed** | `NATIVE_AI_SPEC.md:822` names `/inference/go`. |

## Cross-scope items from the full-runtime a28c564 review

- **Runtime P3-03, assistant-first history:** traced in all five encoders (P5).
  - Anthropic inserts a user placeholder (`anthropic.rs:45-47`), giving `user, assistant, user`.
  - Gemini inserts a bridge user turn (`gemini.rs:128-134`), giving `user, model, user`.
  - Chat, Responses and Ollama send `system, assistant, user`. OpenAI's Chat and Responses formats accept this, and no encoder produces a final assistant message.
  - **No Tezbar-side failure found.**
- **Runtime P3-05, Gemini fallback / F24:** closed (see F24).
- **Consecutive assistant messages after manual retry:** `mod.rs:411` appends the assistant retry marker. `runtime/agent.rs:186-198` re-adds the saved transcript only when it contains calls, then appends a user "Continue..." message. With a partial-error assistant message (`agent.rs:229`), the sequence `... tool, assistant(partial+error), assistant(marker), user` follows (P5).
  - Anthropic and Gemini merge consecutive same-role turns: `user, assistant, user, assistant, user` and `user, model, user` respectively.
  - Chat, Responses and Ollama send two consecutive assistant messages. OpenAI's formats accept them.
  - Strict server-side chat templates that enforce alternation are an **unverified** risk. That is U2, not a proven failure.
  - In all paths the request ends with a user message, so there is no Anthropic prefill. Claude 4.6+ models reject prefill per Anthropic's docs, so that matters here, and it does not occur.

## Remaining interop uncertainty (not counted as findings)

- **U1, include fallback trigger:** the fallback requires the error message to contain both `include` and an "unsupported"-style word (`transport.rs:296-307`). The fixture uses `"include is unsupported"`. If OpenAI rejects `include` with a message that names the field only in `param` (for example a hypothetical `"Encrypted content is not supported with this model."`, `param:"include"`), the fallback won't fire. P4 shows one request and a hard error for that hypothetical body. I couldn't confirm the real wording or that any current model rejects the include, so this is not a finding. Cheap hardening: also match `error.param == "include"` or "encrypted content".
- **U2:** whether strict OpenAI-compatible templates reject consecutive assistant messages (Chat and Ollama retry path).
- **U3:** whether any gateway emits the N-W2 null or empty continuation fields, Gemini's reaction to N-W3 empty parts, keyless Gemini proxies' reaction to N-W4, and Ollama omitting arguments (N-W5).
- **Carried over and still unverified live:** empty Gemini `systemInstruction` and Anthropic `"system": ""` when no system message exists (agent runs always have one); the W2 reasoning-item 404 wording; OpenCode key scope across Zen and Go; Copilot and Antigravity entitlements.
- **Not claimed:** the parent's macOS, Windows, MSRV and lifecycle results are recorded in `docs/NATIVE_AI_VERIFICATION.md`. I did not rerun the suites. My probes establish Tezbar behaviour only, not real UI or platform event delivery or live provider acceptance.

## Coverage

- **Read in full at HEAD:**
  - `wire/{mod,chat,anthropic,responses,gemini,ollama,redact,transport}.rs`
  - `providers/route.rs`, `provider.rs`, `config.rs`, `runtime/limits.rs`
  - `runtime/agent.rs`, plus `runtime/transcript.rs` lines 1–240
  - `mod.rs` launch/retry (`:385-470`) and redaction (`:962-990`)
  - Both prior wire reports and the ledger's W/N-R/close-path sections
- **Read in part:**
  - `SettingsView.tsx` model discovery (`:715-770`) and `shared/aiProviders.ts` inference
  - `legacy.rs::normalize_images`
  - `review_regressions.rs` (W2 and W8 tests, include-fallback fixture), `fixture_http.rs`
- **Diffs:** `118cb7b..ae2fe00` for all Scope A files, plus the stat of the whole diff. `git log e246752..1623d47 --oneline` was captured (26 commits; the last implementation commits are `a28c564`, `8f70d2e` and `ae2fe00`). Static data was checked programmatically: `provider_defaults.json`, and `opencode_catalog.json` (113 entries, all 5 OpenCode defaults catalogued, no free IDs among the defaults).
- **Reference:** local pi-ai 1.3.0 (`openai-completions.js`, `google-shared.js`), code only.
- **Primary and secondary web sources:** Anthropic docs on prefill removal for Claude 4.6 models, via search results. Searches for the OpenAI include-rejection wording found no primary source.
- **Not reviewed** (other reviewers' scope): broad renderer lifecycle, MCP internals, OAuth for extensions.
- **Search:** XERJ wasn't used. The prefix is known to be incomplete, and targeted `rg` and reads on known paths were sufficient.

## Closure verdict

**Not closed.** Scope A must stay open until N-W1 through N-W5 are repaired, each with a regression at its real call site, and a fresh targeted re-review confirms them. All of them are local, small fixes:

| ID | Location |
| --- | --- |
| N-W1 | `agent.rs:169` |
| N-W2 | `chat.rs:121-134` |
| N-W3 | `gemini.rs:79` |
| N-W4 | `route.rs:266` |
| N-W5 | `mod.rs:323` |

Every original F1–F30 and W1–W15 item is otherwise closed or validly dismissed, except W12, which is partly closed.
