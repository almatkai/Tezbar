# Tezbar native AI repair re-review: provider, wire and auth

Reviewer: Claude Opus 5.5, reviewing the code myself with no delegation.
Source: `feat/native-rust-agent` at pinned HEAD `118cb7b4c994cd766503b9078a7d836aad311958`. The worktree was clean and HEAD was unchanged before and after the audit.
Repair diff: `32ba9c4..118cb7b`. Original report: `docs/reviews/native-ai-opus-wire-32ba9c4.md`, unchanged.
No real credentials, profiles, paid or live AI calls, app launches, or source edits were used.

## Verdict

**Outstanding findings remain, so this scope cannot close.** There are 15 actionable findings: 2 P1, 6 P2 and 7 P3.

The most serious is a new regression. The F23 repair rejects any model turn whose text, reasoning or tool arguments look like credentials. Ordinary code such as `token: string` fails the whole turn on all four codecs tested.

| Original ID | State | Blocking finding |
| --- | --- | --- |
| F23 | Not closed; repair causes a P1 regression | W1 |
| F16 | Not closed; repair causes a P1 regression | W2 |
| F29 | Partly closed; new thinking control causes a P2 | W3 |
| F14 | Not closed | W4, W6 |
| F17 | Partly closed | W8 |
| F19 | Partly closed; repair weakens a fail-closed check | W8 |

All other original IDs are closed or validly dismissed. Provider-side reactions stay unverified by design. These are labelled below.

## How findings are labelled

| Label | Meaning |
| --- | --- |
| **PROVEN** | Reproduced at the real Tezbar call site with loopback fixtures and fixture credentials. |
| **PROVEN + DOC** | Reproduced, and a current primary source confirms the upstream behaviour. |
| **SOURCE** | Established by reading the code path. |
| **OPEN INTEROP** | Tezbar's request or decode behaviour is proven, but the upstream reaction wasn't tested live. |

Probe source: `/private/tmp/tezbar-wire-118-probe.rs`. Log for T01–T06: `/private/tmp/tezbar-wire-118-probe.log`. T07 was a later targeted run whose output is quoted under W1, and it isn't in that log.

The probes ran from a `git archive` snapshot of the pinned commit at `/private/tmp/tezbar-wire-118-snap`, with the probe module added under `#[cfg(test)]` and the bundle resources removed. No git worktree was created. The six T01–T06 probes passed; they print behaviour, so a pass doesn't mean the behaviour is correct.

## P1

### W1. F23 repair fails ordinary turns that resemble credentials (PROVEN; new regression)

- **Where:** `src-tauri/src/native_core/ai/wire/transport.rs:260-266` and `:424-438`, using the generic patterns at `mod.rs:921-946`, plus exact-match secrets from `providers/route.rs` and `wire/redact.rs:27-29`.
- **Trigger A:** a model writes text, reasoning or tool arguments that match `(?i)\b(password|passwd|api[_-]?key|secret|token)\s*[=:]\s*([^\s,;]+)`. Examples are `token: string`, `password: string`, `const apiKey = process.env.KEY` and "the secret: rotate it weekly".
- **Trigger B:** a local server uses a placeholder API key such as `none`, `lm-studio` or `EMPTY`, and the model's output contains that word.
- **Failure:** the turn ends with a non-retried error, and its text may already have been shown. No tools run and nothing is saved. A coding agent will hit this routinely, because typed interfaces, config examples and auth code all match.
- **Evidence (T01):** the text `interface Login { token: string; password: string }` returns the error below on Chat, Responses and Gemini. It also fires on Chat tool arguments containing `const apiKey = process.env.KEY`, and on Anthropic thinking containing `the secret: rotate it weekly`.
  ```
  Native validation: provider returned credentials in opaque history; the response was not saved or replayed
  ```
- **Evidence (T07):** keys `none`, `lm-studio` and `EMPTY` each give the same error for `none of the lm-studio EMPTY values changed`.
- **Why tests missed it:** `provider_errors_keep_messages_retry_before_text_and_never_persist_opaque_secrets` only sends the exact fixture key.
- **Ledger mismatch:** the F23 entry says "exact known credentials still fail closed", but generic patterns also fail closed. The spec's scrubbing row at `docs/NATIVE_AI_SPEC.md:1283` also says "Exact-match only".
- **Related:** `Redactor::clean` applies the same generic regex to visible text and stored text blocks, so `token: string;` displays as `token=[REDACTED];`. That predates the repair, and the same fix should cover it.
- **Fix:**
  - Remove the `redact_text(text) != *text` branch from `opaque_has_secret`. Fail closed only on an exact configured credential.
  - Skip the exact check for values that can't be real credentials. Use a minimum length and exclude known placeholders, or treat keyless local servers as having no secret.
  - For model output, restrict scrubbing to high-confidence shapes: PEM, `sk-`, `gh*_`, `AKIA` and long Bearer tokens. Keep the keyword pattern for errors and logs.
  - Add regressions on all five codecs for `token: string` in text, thinking and tool arguments, and for placeholder keys.

### W2. F16 repair replays reasoning items that can't be resolved with `store: false` (PROVEN request shape, OPEN INTEROP; new regression)

- **Where:** `wire/mod.rs:42-44`, `providers/route.rs:323-327`, `wire/responses.rs:5-12`, and capability inference at `src/shared/aiProviders.ts:287-291` and `config.rs` `normalize_models`.
- **Trigger:** a Responses route whose model has no `thinking` capability in metadata but still reasons. The inference pattern matches `gpt-5` but not `gpt-6`, so discovered `gpt-6-sol` gets no `thinking` capability. Likewise, `opencode/gpt-6-sol` and other catalogued Responses IDs missing from the defaults have no metadata at all. Grok and Muse IDs are also affected if they return reasoning items.
- **Failure:** the first request now omits `include`, so reasoning items come back without `encrypted_content`. The tool-loop request replays them unchanged with `store: false`.
- **Evidence (T02):** for both routes, the first request has `include=null store=false`, and the second has:
  ```
  {"id":"rs_1","summary":[],"type":"reasoning"},{"arguments":"{}","call_id":"call_1","id":"fc_1",...}
  ```
- **Upstream:** community and project reports describe a 404 of the form `Item with id 'rs_...' not found. Items are not persisted when store is set to false`. I found no official page stating this. The trigger also depends on the model returning reasoning items under its default effort, which wasn't tested live. Before the repair, `include` was always sent, so this replay shape had encrypted content.
- **Fix:**
  - On Responses replay with `store: false`, never send a reasoning item without `encrypted_content`.
  - When dropping one, also remove server `id`s from the items that follow it. Reports describe a separate error when `fc_` items lose their paired reasoning item.
  - Treat unknown capabilities as possibly reasoning: keep requesting `include`, and retry once without it if the provider says encrypted content is unsupported.
  - Add `gpt-6` to the inference patterns in both TypeScript and Rust, and cover `vision` too.
  - Add a two-turn regression through `canonical_turn`.

## P2

### W3. The new thinking checkbox makes default models fail (PROVEN + DOC for Anthropic; PROVEN request shape, OPEN INTEROP for Ollama)

- **Where:** `src/renderer/SettingsView.tsx:2204-2212`, `providers/route.rs:323`, `wire/mod.rs:49-56`.
- **Trigger:** the user ticks the global "Enable model thinking" box.
- **Failure:** `thinking` is true for every route, regardless of model support.
- **Evidence (T03):**
  - Anthropic default `claude-haiku-4-5-20251001` is sent `thinking={"type":"adaptive"}`. Anthropic's thinking table lists a **400 error** for adaptive on Haiku 4.5, Sonnet 4.5 and Opus 4.5.
  - Ollama default `llama3.2`, whose capabilities are `["tools"]`, is sent `think=true`. I recall Ollama rejecting this with "does not support thinking", but the error in my fixture was fixture-supplied, so that reaction is unconfirmed.
- **Note:** this path existed in code, but F29's repair is what made it reachable from the UI.
- **Fix:** gate on the model's `thinking` capability. Use the right shape per model on Anthropic: `enabled` with `budget_tokens` for 4.5-generation models, `adaptive` for Opus 4.6 and later, or omit the field. Add route-level tests for both defaults.

### W4. Refreshing the model list erases the F14 per-model protocol (SOURCE; F14 not closed)

- **Where:** `src/renderer/SettingsView.tsx:736-743`. Refresh runs on load and provider change at `:900`, `:943`, `:1009`.
- **Trigger:** the user selects an uncatalogued OpenCode model that the live list returns, chooses a protocol, and reopens Settings or changes provider. That is F14's exact scenario.
- **Failure:** the merge rebuilds each discovered entry with only `id`, `capabilities`, `contextWindow` and `hiddenFromPicker`. `protocol` and `maxTokens` are dropped, and the later `existing` loop skips the entry because its ID is already seen. The next chat fails with `choose a protocol for this model`.
- **Why tests missed it:** `SettingsView.knowledge.test.ts:254-268` checks the first save, not a save followed by discovery.
- **Fix:** spread `prior` into the discovered entry and only fill in missing fields. Add a test that refreshes after choosing a protocol.

### W5. Anthropic same-origin replay sends empty text blocks (PROVEN request shape, OPEN INTEROP)

- **Where:** `wire/anthropic.rs:13-16`.
- **Trigger:** a stream produces a text block with no text, then a tool call.
- **Failure:** the canonical path filters blank text, but the raw path doesn't. The stored block is replayed on the next request and on every later request in that conversation. I recall Anthropic returning `text content blocks must be non-empty`, and pi-ai skips blank text blocks, but I didn't test this live.
- **Evidence (T05):** `[{"text":"","type":"text"},{"id":"a","input":{},"name":"read","type":"tool_use"}]`.
- **Why tests missed it:** `anthropic_saved_empty_tool_text_is_omitted...` only covers `tool_result` on the canonical path.
- **Fix:** drop blank `text` blocks from raw replay. Leave `thinking` and `redacted_thinking` untouched, because signed content must stay unmodified.

### W6. OpenCode's default model is free-tier, and three defaults remain uncatalogued (PROVEN route + DOC; F14 not closed)

- **Where:** `provider_defaults.json:12`, `:384`, `:426-449`.
- **Failure:**
  - After adding a paid key, the default route is `opencode/big-pickle` (T06). OpenCode's Zen pricing table lists Big Pickle as free. Its inference docs say free-tier chat models are only available from within OpenCode, and other clients receive `403` even with a key. So the first chat fails with a bare status 403 per those docs.
  - `opencode/deepseek-v4-flash-free`, `opencode/mimo-v2.5-free` and `opencode/north-mini-code-free` are still offered but absent from the catalog, so they return `choose a protocol`. F14's proposed fix included removing stale defaults. Those three IDs also carry a `-free` suffix, so they are likely restricted the same way; I didn't confirm them individually.
- **Spec:** `docs/NATIVE_AI_SPEC.md` requires native access to use supported models and report restrictions truthfully.
- **Fix:** make a supported paid model the default and remove free-tier IDs from the native defaults. Map a 403 on OpenCode to a message saying the model requires OpenCode or a paid model.

### W7. OpenCode Go-only credentials show as unavailable, and Go models can vanish from discovery (PROVEN)

- **Where:** `provider.rs:80-84`, `:195`, `:199-239`.
- **Evidence (T04):**
  - With only an `opencode-go` entry in `auth.json` and an `opencode-go/*` model, status is `false`, while `native_route` succeeds at `https://opencode.ai/inference/go/openai/v1/chat/completions`.
  - With both keys, the live Zen list replaces the static catalog, and the returned list is only `["opencode/gpt-6-sol"]`. No Go IDs remain.
- **Source:** `models()` never queries `https://opencode.ai/inference/go/v1/models`, which is listed in the official Go docs, and never uses the Go key.
- **Fix:** report available when either credential exists, or when the selected model's credential exists. Fetch Go's list with the Go key and merge it. Fall back to static IDs per family, not across the whole list.

### W8. Responses and Chat decoders can run tools with arguments silently emptied (PROVEN; F17 and F19 partly open)

- **Where:** `wire/responses.rs:56-72`, `wire/chat.rs:131`, `wire/mod.rs:303-314`, `:392-396`.
- **Evidence (T05):**
  - `output_item.added` with `arguments:""`, argument deltas `{"path":"a"}`, then `completed` with `output: []` gives a tool call with `{}`. The merge only fills `arguments` when null, not when empty.
  - Text deltas then `completed` with `output: []` give `text="hello"`, 0 canonical blocks and raw `[]`. The assistant's text is lost from replayed history.
  - Non-string `arguments:{"path":"a"}` on Chat or Responses also gives `{}`. Before the repair, Chat failed closed on this input, because the empty result hit `object_args`. Now the tool runs without its arguments.
- **Impact:** a write or shell tool gets empty arguments and may misbehave. Whether a gateway emits these shapes is unconfirmed.
- **Fix:**
  - Prefer the accumulated buffer over an empty `arguments`.
  - Rebuild text blocks from streamed text when no `message` item survives.
  - Treat only a missing, null or blank-string value as empty. Serialize an object value, and reject any other type before execution.

## P3

| # | Where | Problem and evidence | Fix |
| --- | --- | --- | --- |
| W9 | `wire/mod.rs:203-213`, `:215-234` | A 200 JSON body such as `{"error":{"message":"fixture model is loading"}}` loses its message and retries. T06: `stream ended before terminal signal`, `requests=3`. Anthropic JSON returns `missing content`, and Responses JSON never checks `response.error`, by source. | Run `provider_stream_error` at the start of `Decoder::json`. |
| W10 | `providers/route.rs:258-262` | A custom Gemini-protocol provider sends its key as Bearer. T06: `bearer=true goog=false` for `generativelanguage.googleapis.com`. Google's key auth uses `x-goog-api-key`; a custom header is the only workaround. OPEN INTEROP. | Use `x-goog-api-key` for non-OpenCode Gemini routes, or add a visible auth-header choice. |
| W11 | `providers/route.rs:59-66` vs `provider.rs` native-Gemini checks | A saved `.../v1beta` base counts as native for status and catalog, but chat routes to Chat. T06: `.../v1beta/chat/completions`, `OpenaiChat`, no `x-goog-api-key`. | Treat `/v1beta` as native Gemini in the route. |
| W12 | `runtime/limits.rs:71`, ledger source note | The Anthropic budget is 32 MB, but current docs list **128 MB** for Messages. Earlier images are omitted around 27 MB of image data when the API would accept more. | Use 128 MB for `api.anthropic.com`. Keep a tighter value for gateways, and correct the ledger. |
| W13 | `wire/chat.rs:11`, `:40` | `reasoning_content` is only added when `provider == "deepseek"`. A custom or `openai-compatible` route to `api.deepseek.com` doesn't get it on foreign history. pi-ai also matches `deepseek.com` in the base URL (`openai-completions.js:1238`). OPEN INTEROP. | Decide by provider or host, carrying host-derived capability into `encode`. |
| W14 | `wire/chat.rs:80-87` | `signature` fragments are concatenated. pi-ai keeps the first with `signature ||= detail.signature`. A gateway that repeats the full signature gets a corrupted value. No gateway is confirmed to do so. | Keep the first non-empty signature. |
| W15 | `docs/NATIVE_AI_SPEC.md:822` | The ledger says F2 fixed "code/spec", but the spec still says `/zen/go` and contains no `inference/go`. Docs may be updated later. | Correct the spec to the official `/inference/go` root. |

## Per-ID verdicts

| ID | Verdict | Basis |
| --- | --- | --- |
| F1 (P0) | **Closed** | `auth.rs` fallback removed. `unsigned_copilot_never_borrows...` uses the real `llm-provider-statuses` and `agent:run` handlers and asserts no request. Route and catalog both reject an empty token, and exchange rejects a blank one. |
| F2 | **Closed in code**; spec open (W15) | The default is `/inference/go`, matching current official docs. Route test goes through `config::selected`. |
| F3 | **Closed** for empty arguments | Empty and whitespace buffers become `{}`; malformed JSON still fails. Empty text is W5. |
| F4 | **Closed** for `provider == deepseek` | Null deltas are ignored, and replay adds `""`. Host-based gap is W13. |
| F5 | **Closed** | Follow-ups are flushed only after the tool run, across messages. `runtime/agent.rs:303` writes role `tool`. |
| F6 | **Closed** | Indexed fragments merge. Signature edge is W14. |
| F7 | **Closed** | The real `canonical_turn` retries twice and recovers. |
| F8 | **Closed** | Recognized model errors fall back before the endpoint walk, with separate budgets. Real three-endpoint fixture. |
| F9 | **Closed** | The default URL stays implicit in `config::selected`, and Settings' default save passes through that check. |
| F10 | **Closed** | HTTP errors read string `error`, `message` and `detail`. The 200 case is W9. |
| F11 | **Closed** | 529 and typed stream errors retry only before visible text, and messages are kept. Verified across the Anthropic, Responses and Gemini shapes by source. |
| F12 | **Closed** | Official host sends `max_completion_tokens`, proven through real transport. |
| F13 | **Closed** | Status and catalog use `native_route` headers and protocol, tested for four protocols. |
| F14 | **Not closed** | Control exists, but refresh erases it (W4), and stale defaults remain (W6). |
| F15 | **Dismissal upheld** | OpenCode's console inference docs show `Authorization: Bearer <token>` and list the Gemini path. The Go and Zen pages name no Gemini auth header. No doc supports `x-goog-api-key`. |
| F16 | **Not closed; regression** | W2. |
| F17 | **Partly closed** | Done items survive; the `added`-plus-delta and text-only cases don't (W8). |
| F18 | **Closed** on our side | Claude and GPT-OSS send IDs on both sides, including identical parallel calls. Gemini gets no synthetic ID. Upstream unverified. |
| F19 | **Partly closed** | Missing and empty arguments work, but non-strings now become `{}` (W8). |
| F20 | **Closed** | Default 30 s remote, 120 s local, 300 s maximum. The value reaches `req.send`, and the Settings field persists it. |
| F21 | **Closed** | `error: null` is ignored. |
| F22 | **Closed**; stale value is W12 | Per-image Anthropic limit matches docs at 10 MB base64, recursively including tool results. |
| F23 | **Not closed; regression** | Signed payloads are no longer rewritten, which is correct, but W1 fails valid turns. |
| F24 | **Closed** | Fallback uses recommended `gemini-3.5-flash`. |
| F25 | **Closed** | Stable project is derived before the early return. Fixture has no `projectId` and asserts a non-empty value. |
| F26 | **Closed** | Failures use `{error}`, with "Tool failed" for empty text. |
| F27 | **Closed**; identity retained intentionally | Image-only and assistant-first bridges exist, and function-call turns follow a user turn. Not copying the Antigravity identity prompt is accepted as the recorded decision. |
| F28 | **Closed** | Provider, Antigravity and token clients use `Policy::none`. Fixture proves the second origin gets nothing. `voice.rs`'s default client only does a model download with no provider credential, by call-site read. |
| F29 | **Partly closed** | Empty header and Copilot revocation are closed. Thinking control now exists, but W3 follows from it. Antigravity reads `thinking` through its own config path (`antigravity.rs:389-401`). |
| F30 | **Closed** | Stop reason is checked before parsing arguments. |

## Checked, no finding

- **Routes:** OpenCode paths, including Go, match current docs. Copilot's `supported_endpoints` routing and its proxy-host validation are intact.
- **Credential scoping:** `selected` limits legacy top-level values to the active provider. Contributed providers never inherit them, and disabled extensions can't resolve headers. Settings' Refresh persists before fetching, so a draft URL isn't paired with an unsaved key.
- **Copilot revocation:** a 401 or 403 clears the session, exchanges once, then surfaces the error. The base stays pinned when explicit.
- **Antigravity budgets:** account rotation derives a project for the next account. Rotation, fallback and retry have separate counters under a hard limit of 24 requests. Exhausting accounts returns a clear quota message.
- **Gemini signatures:** first-call validation and observation fallback are unchanged, and text-only history is kept.
- **Lifecycle helpers in `118cb7b`:** read only where they touch this scope (`oauth.rs` and `ext/contributions.rs`). Other reviewers cover the rest.

## Coverage

**Read in full:** `wire/{mod,chat,anthropic,responses,gemini,ollama,transport,redact}.rs`, `providers/route.rs`, `providers/copilot.rs`, `provider.rs`, `runtime/limits.rs`.

**Read in part:** `config.rs`, `auth.rs`, `providers/antigravity.rs` (accounts, bases, catalog), `runtime/transcript.rs`, `ext/contributions.rs` (header scope), `SettingsView.tsx` (save, discovery, thinking, protocol), `shared/aiProviders.ts`. Repair diff for `oauth.rs`. Key regressions in `providers/tests.rs` and `wire/tests.rs`. Ledger F1–F30 and its primary-source notes.

**Not reviewed:** runtime, UI and extension behaviour outside the call sites above, which other reviewers cover. I didn't trace whether non-agent chat can carry an oversized image into `project_images`; agent runs validate new images at `runtime/agent.rs:150`.

**Primary sources:**
- OpenCode: [Go](https://opencode.ai/docs/go/), [Zen](https://opencode.ai/docs/zen/), [console inference](https://opencode.ai/v2/docs/console/inference/)
- Anthropic: [thinking](https://platform.claude.com/docs/en/build-with-claude/thinking), [vision](https://platform.claude.com/docs/en/build-with-claude/vision), [API overview](https://platform.claude.com/docs/en/api/overview)
- Local `@earendil-works/pi-ai` code only. No credential files were read.

**Secondary sources for W2:** [Microsoft Agent Framework #4357](https://gh.hhj520.top/microsoft/agent-framework/issues/4357), [#4562](https://gh.hhj520.top/microsoft/agent-framework/issues/4562), [coder/mux #59](https://github.com/coder/mux/pull/59), [OpenAI community thread](https://community.openai.com/t/how-to-use-reasoning-encrypted-content-with-store-false-stateless/1286934/2). The search was thin on official wording, so treat W2's upstream error as reported, not documented.

**Search:** XERJ wasn't needed; targeted reads and `grep` were used on known paths.

**Verification not claimed:** this report makes no statement about the parent's macOS, MSRV, lifecycle or Windows runs. My probes establish Tezbar's behaviour only.

## Remaining uncertainty

- W2, W3 (Ollama), W5, W10 and W13 depend on upstream reactions that weren't tested live.
- W8's stream shapes and W14's repeated signatures aren't confirmed from any real gateway.
- With no system message, Gemini gets an empty `systemInstruction` text part and Anthropic gets `"system": ""`. Upstream acceptance is unconfirmed, so I'm not reporting these as defects.
- Any repair of these findings needs a new targeted review. W1 and W2 especially touch the core turn path on every provider.
