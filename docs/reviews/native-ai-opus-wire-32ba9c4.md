# Tezbar native AI review: Spec Axis Part A (wire, providers, auth)

Reviewer: Claude Opus 5.5, reviewing the code myself with no delegation.
Source: `feat/native-rust-agent` at `32ba9c4e5ff41b527ba5943f779a3f9ea090d135`. Base `v0.3.0-beta.7` = `e246752149dbbede5e44720a0e35a3eacf2d66a9`. Both refs verified.
No source was changed: the worktree is clean and HEAD is unchanged. No real credentials, profiles or paid API calls were used.

## Verdict

**The scope does not meet the completion gate.** There are 20 actionable P0/P1/P2 findings: 1 P0, 4 P1 and 15 P2. Another 10 are P3. The existing fixture suite passes because it never sends these inputs.

- **P0:** a provider API key is sent to GitHub with no user action.
- **P1:**
  - Every OpenCode Go model returns 404.
  - Anthropic-protocol runs fail on any tool call with empty input. `screen_capture` is one such tool.
  - DeepSeek tool loops replay `reasoning_content: null`. DeepSeek documents a 400 when reasoning isn't passed back.
  - Chat history breaks permanently when an image result is followed by another tool result in the same batch.

## How findings were classified

| Label | Meaning |
| --- | --- |
| **PROVEN** | Reproduced at the real Tezbar call site with loopback fixtures and fixture credentials. |
| **PROVEN + DOC** | The defect is reproduced, and a primary source confirms the upstream behaviour that triggers it or the failure it causes. |
| **SOURCE-LEVEL** | Established by reading the code path. No separate probe was needed, or none was run. |
| **OPEN INTEROP** | Tezbar's behaviour is proven or read from code, but the upstream reaction could not be confirmed without a paid or live call. |

Probe source: `/private/tmp/tezbar-wire-probe.rs`. Probe log: `/private/tmp/tezbar-wire-probe.log`.

To rerun: add the file as `native_core/ai/opus_probe.rs` under `#[cfg(test)]` in a disposable snapshot, then run `cargo test --lib opus_probe -- --nocapture --test-threads=1`. The snapshot I used has been deleted. Its `tauri*.json` needed `bundle.resources` and `externalBin` removed, plus a clone of the ignored `build/embedding-runtime` and `dist`.

The T06 lines in the log were recorded by hand from the run output, because that run was piped.

## P0

### F1. Status refresh sends another provider's API key to GitHub (PROVEN)

- **Where:** `src-tauri/src/native_core/ai/provider.rs:46-51`, `auth.rs:158-161`, `providers/copilot.rs:129-149`, reached from `mod.rs:591-599`.
- **Trigger:** the user has an OpenAI, Anthropic, DeepSeek or custom key saved. `SettingsView.tsx:1086-1103` mirrors the active key into top-level `apiKey`. No Copilot token exists. `ProvidersView.tsx:531` calls `llm-provider-statuses`.
- **Failure:** `available()` calls `copilot_models()` for Copilot with no check that a Copilot credential exists. `copilot_token()` falls back to `raw["apiKey"]`. That key goes as a Bearer token to `api.individual.githubcopilot.com/models`. After the expected 401, it also goes to `api.github.com/copilot_internal/v2/token`.
- **Evidence (T02):** with `{"provider":"openai","apiKey":"fixture-openai-key"}`:
  ```
  GET /models    authorization="Bearer fixture-openai-key"
  GET /exchange  authorization="Bearer fixture-openai-key"
  ```
- **Regression:** the fallback existed at the base commit, but `available()` there stopped when `copilotGithubToken` was empty. That check survives as dead code at `provider.rs:59-71`, behind the new early return. The fallback also fires when a user selects Copilot without signing in, through `route.rs:84-85`.
- **Fix:** remove the `raw["apiKey"]` fallback from `copilot_token()`. Return `false` from `available()` and a "Sign in to Copilot" error from routing when the token is empty. Never call `copilot_models` or `exchange_copilot_session` with an empty token. Add a regression test that asserts no request is made.

## P1

### F2. Every `opencode-go/*` model routes to a dead gateway (PROVEN + DOC)

- **Where:** `providers/route.rs:151-158`.
- **Trigger:** select any `opencode-go/*` model with a default base.
- **Failure:** requests go to `https://opencode.ai/zen/go/...` and return 404.
- **Evidence:**
  - T03 produced `https://opencode.ai/zen/go/openai/v1/chat/completions`, plus the matching `anthropic` and `google` URLs.
  - The official docs at `opencode.ai/docs/go/` list `https://opencode.ai/inference/go/openai/v1/...`, `/anthropic/v1/messages` and `/google/v1beta/models/...`.
  - The repo's own `opencode_catalog.json` also says `inference/go`.
  - An unauthenticated empty POST returned **404** at `/zen/go/openai/v1/chat/completions` and **400** at `/inference/go/openai/v1/chat/completions`.
- **Note:** spec §8.8 itself says `/zen/go`, so the spec should be corrected too.
- **Fix:** change the default to `https://opencode.ai/inference/go`.

### F3. Anthropic tool calls with empty input fail the whole turn (PROVEN + DOC)

- **Where:** `wire/anthropic.rs:81-87`, `wire/mod.rs:328-332` and `:393-399`.
- **Trigger:** on any Anthropic-protocol route (`anthropic`, Copilot `claude-*`, OpenCode Claude or Qwen, custom), the model calls a tool with empty input. Shipped examples are `screen_capture` (`native_tools.rs`, `json!({})`) and `list_resources` (`ext/mod.rs:855`, `properties: {}`).
- **Failure:** the empty `input_json_delta` creates an empty `d.args[i]`, so `object_args("")` fails. The run ends with `Native protocol: invalid completed tool JSON`, and the failure isn't retried.
- **Evidence (P01):** the same stream with the empty delta returns that error. Without the delta it produces a valid call with arguments `{}`.
- **Upstream:** the Anthropic streaming docs show `partial_json:""` after `content_block_start`, and say the final `tool_use.input` is always an object. They don't describe a stream where that is the only delta, so that exact case rests on known client behaviour. pi-ai tolerates it through its streaming JSON parser.
- **Fix:** ignore empty fragments, or treat a whitespace-only final buffer as `{}`. Keep rejecting any other invalid JSON.

### F4. A DeepSeek `reasoning_content: null` chunk erases accumulated reasoning (PROVEN + DOC)

- **Where:** `wire/chat.rs:55-64`.
- **Trigger:** `deepseek-v4-*` emits reasoning chunks, then content or tool chunks with `reasoning_content: null`. The DeepSeek API reference marks the delta field as nullable.
- **Failure:** the `null` branch overwrites `d.raw[key]`. The next tool-loop request then replays `"reasoning_content": null`. DeepSeek's thinking guide says the field "must be fully passed back" and that the API "will return a 400 error" otherwise.
- **Evidence (P02):** the replayed value is `null` after the stream `"step one"`, `" step two"`, `null`.
- **Extra gap:** the replay branch at `chat.rs:5-8` never adds `reasoning_content: ""` to a same-origin message that lacks it, which the §5.2 "every assistant message" rule requires. pi-ai adds it (`openai-completions.js:1051-1052`).
- **Other effects:** `has_thinking()` also treats a stored `null` as thinking, so "Thinking…" can show on turns with no reasoning. A `reasoning: null` chunk erases `reasoning` in the same way for other providers.
- **Uncertainty:** the docs confirm nullability but don't show a sample chunk with `null`. The sequence used here is my understanding of what the live API sends, and it needs a live check.
- **Fix:** ignore null deltas for these keys. For `provider == "deepseek"`, set `reasoning_content` to `""` on raw assistant messages when it is absent or null.

### F5. An image tool result followed by another result breaks Chat history permanently (PROVEN; upstream rejection known but not live-tested)

- **Where:** `wire/chat.rs:17-21` and `:34`, with `runtime/agent.rs:233` writing one message per result.
- **Trigger:** on Chat protocol, a vision model calls several tools in one batch and any result except the last contains an image. `screen_capture` and MCP tools can return images.
- **Failure:** the encoded order is invalid for OpenAI-style Chat, which requires tool messages for all `tool_call_id`s directly after the assistant message. The server returns 400. The transcript is saved, so every later turn in that conversation sends the same order and fails too.
- **Evidence (P04):** `["user","assistant","tool(a)","user","tool(b)"]`.
- **Fix:** collect follow-up image messages and emit them only after the contiguous run of `tool` messages. Add a fixture with two calls where the first returns an image.

## P2

### F6. OpenRouter-style `reasoning_details` chunks overwrite each other (PROVEN; upstream rejection not live-tested)

- **Where:** `wire/chat.rs:55-64`.
- **Trigger:** a Chat provider streams `reasoning_details` as array deltas, as OpenRouter does according to pi-ai's comment at `openai-completions.js:424`.
- **Failure:** only the last array survives. Thinking is truncated while the signature is kept, so replay is corrupted.
- **Evidence (P03):** only `"part two"` remains.
- **Fix:** append arrays, and merge consecutive text entries with the same index as `appendOpenAIReasoningDetail` does.

### F7. Antigravity's empty-response retry is dead code (PROVEN)

- **Where:** `wire/mod.rs:434-436` returns first, `wire/transport.rs:206-209` marks it non-retryable, so `:229-235` never runs.
- **Trigger:** Antigravity ends a turn with `finishReason: STOP` and no parts.
- **Failure:** the turn fails on the first attempt. Spec §8.9 requires up to 2 retries.
- **Evidence (T01):** `Err("Native model: empty final answer") requests=1`.
- **Fix:** mark that decoder error retryable when `ag && !visible`, or use a typed error, and delete the unreachable branch.

### F8. Antigravity model fallback can't run with the default endpoints (PROVEN)

- **Where:** `wire/transport.rs:245`, `:264-276`, `:277-310`.
- **Trigger:** a selected model such as `gemini-3.8-flash` isn't available on the account, and all three endpoints return 404.
- **Failure:** the endpoint walk uses all 3 attempts. The fallback branch comes after the walk and sits behind `attempt < 2`, so it never runs.
- **Evidence (T06):**
  - Three endpoints: all receive `gemini-3.8-flash-low`, and the result is `status 404: Model not found`.
  - One endpoint: the same request returns `Ok("fell back")`.
- **Fix:** check for a recognised model-unavailable error before walking endpoints, or give fallback its own bound below the turn seam.

### F9. Saving Settings pins Antigravity to one endpoint (PROVEN at config level)

- **Where:** `src/renderer/SettingsView.tsx:1057`, `config.rs:112`, `providers/antigravity.rs:705-706`.
- **Trigger:** save AI settings with Antigravity selected.
- **Failure:** the UI stores the default `https://cloudcode-pa.googleapis.com`, and `_explicitBaseURL` becomes true. The daily and sandbox endpoints are lost, along with failover and catalog merging. The reference's own catalog merge comment says some models exist only on daily or sandbox endpoints.
- **Evidence (T03):**
  - With a saved default: `explicit=true`, one base.
  - Without one: `explicit=false`, three bases in the reference order.
- **Fix:** don't store the default, and treat a value equal to the default as not explicit.

### F10. HTTP errors with `{"error":"..."}` lose their message (PROVEN)

- **Where:** `wire/transport.rs:107-110`.
- **Trigger:** Ollama or a local server returns a string `error`. This shape is common for "model not found" and "does not support tools".
- **Failure:** the user sees a bare status code with no explanation.
- **Evidence (T04a):** a 400 with `does not support tools` becomes `Err("Native HTTP: status 400")`.
- **Fix:** also read a string `error`, top-level `message` and `detail`, keeping the existing redaction and 500-character limit.

### F11. Stream errors lose the provider's message and aren't retried (PROVEN + DOC)

- **Where:** `wire/mod.rs:223-225`, `wire/anthropic.rs:101`, `wire/responses.rs:82-84`, `wire/transport.rs:180-182`.
- **Trigger:** Anthropic sends a mid-stream `overloaded_error` (documented as the streaming form of HTTP 529), or Responses sends `response.failed`, for example on quota.
- **Failure:** the message is lost, contrary to §6.2's "provider's error message is shown". Nothing is retried, even before any visible text.
- **Evidence (T04b, T04c):**
  - `Native model: provider returned a stream error`, with `requests=1`.
  - `Native protocol: Responses turn failed or incomplete`, with the quota text dropped.
- **Fix:** extract `error.message` or `response.error.message`, redact and bound it. Treat overloaded and server errors before visible text as retryable. Consider adding HTTP 529 to the retry list.

### F12. `max_tokens` is sent to OpenAI's own host on Chat (SOURCE-LEVEL, OPEN INTEROP)

- **Where:** `wire/mod.rs:38-40`, with `provider_defaults.json` defaulting `openai-compatible` to `api.openai.com/v1` and `gpt-5.4-mini`.
- **Trigger:** `openai-compatible` with its default values, or `openai` with an explicit Chat protocol, on a GPT-5 or o-series model.
- **Failure:** OpenAI rejects `max_tokens` for these models and asks for `max_completion_tokens`. That is my understanding of current API behaviour, not a live test. pi-ai's `maxTokensField` setting exists for this difference.
- **Fix:** send `max_completion_tokens` when the host is `api.openai.com`, or add a per-model setting.

### F13. Custom-provider status and catalog ignore custom headers and protocol (SOURCE-LEVEL)

- **Where:** `provider.rs:52-112` and `:227-283`, versus `providers/route.rs:285-297`.
- **Trigger:** a custom provider authenticates through a configured or secret-ref header, or uses `anthropic`, `gemini` or `ollama` protocol.
- **Failure:** the probe sends no custom headers and calls `{base}/models` with Bearer auth regardless of protocol. Chat works while status shows unavailable and model discovery returns nothing, which breaks §8's "truthful status".
- **Fix:** build probe requests from the same protocol and header logic as `native_route`.

### F14. OpenCode model IDs missing from the static catalog have no way forward (SOURCE-LEVEL, catalog comparison run)

- **Where:** `providers/route.rs:149`, `providers/opencode_catalog.json`.
- **Trigger:** the user selects a model from the live `/v1/models` list that the catalog lacks. Defaults `opencode/deepseek-v4-flash-free`, `opencode/mimo-v2.5-free` and `opencode/north-mini-code-free` are absent. So are the official-docs IDs `opencode/muse-wonder-1.0` and `opencode/beam-free`.
- **Failure:** the error says `choose a protocol for this model`, but the only protocol control in the renderer is on custom providers (`SettingsView.tsx:1292`). Each new upstream model stays unusable until the app updates.
- **Fix:** add a per-model protocol control and remove stale defaults. Alternatively, derive the route from the official docs table or an authoritative metadata field. The coordinator's no-guessing decision rules out routing by name.

### F15. OpenCode's Gemini route sends Bearer, not `x-goog-api-key` (PROVEN request shape, OPEN INTEROP)

- **Where:** `providers/route.rs:251-255`.
- **Evidence (T03):** `opencode/gemini-3-flash` has `authorization=true x-goog=false`.
- **Why it matters:** the docs route these models through `@ai-sdk/google`, which normally authenticates with `x-goog-api-key`. The gateway source couldn't be fetched, so the rejection isn't confirmed. Anthropic routes already use `x-api-key`.
- **Fix:** confirm with a real key, then send `x-goog-api-key` on OpenCode Gemini routes if required.

### F16. `reasoning.encrypted_content` is always requested on Responses (SOURCE-LEVEL, OPEN INTEROP)

- **Where:** `wire/responses.rs:27`.
- **Trigger:** a non-reasoning model on `/responses`. `provider.rs` lists every model from OpenAI's catalog, so an older model can be selected.
- **Failure:** I recall OpenAI answering `Encrypted content is not supported with this model` in this case, but my search couldn't confirm that string. pi-ai sets this include for reasoning requests and for xAI (`openai-responses.js:293`, `:301`).
- **Fix:** include it only for models with the thinking capability. Confirm with a live call.

### F17. `response.completed` with `output: []` silently drops tool calls (PROVEN decoder, OPEN INTEROP)

- **Where:** `wire/responses.rs:57-63`.
- **Trigger:** a Responses gateway streams `output_item.done` items, then completes with an empty `output`.
- **Failure:** the blocks are replaced with nothing. The turn looks like a successful final answer and the tool never runs.
- **Evidence (P05a):** `Ok((0, "text"))`, with zero calls.
- **Uncertainty:** the official API returns a full `output`. Whether Copilot, OpenCode or other gateways ever send an empty one is unconfirmed.
- **Fix:** replace only with a non-empty array, otherwise keep the streamed items. Keep the existing behaviour of taking encrypted content from `completed`.

### F18. Gemini `functionResponse.id` doesn't match the replayed `functionCall` (PROVEN request shape, OPEN INTEROP)

- **Where:** `wire/gemini.rs:80`, with raw replay at `:54-62`.
- **Evidence (P05e):**
  - Call: `{"functionCall":{"args":{},"name":"read"}}`, with no ID.
  - Response: `{"functionResponse":{"id":"tz_7_0",...}}`.
- **Why it matters:** `pi-antigravity` deliberately handles this the other way (`stream.ts:102-108`, `:376`, `:414`). It sends IDs on both sides for `claude-*` and `gpt-oss-*`, and on neither side for Gemini. Spec §8.9 says to port this. A Claude call on Cloud Code that arrives without an ID is the most likely case to be rejected, but this is unconfirmed.
- **Fix:** match the reference: send the same sanitized ID on both sides for Claude and GPT-OSS, and omit a synthesized ID on Gemini.

### F19. Non-Anthropic tool calls with empty arguments also fail (PROVEN decoder, OPEN INTEROP)

- **Where:** `wire/mod.rs:262-270` and `:374-376`.
- **Evidence (P05c, P05d):**
  - Chat with `arguments: ""` gives `invalid completed tool JSON`.
  - Gemini with no `args` gives `invalid functionCall args`.
- **Why it matters:** the reference tolerates both, using `parseStreamingJson` and `asToolCallArguments`. Which servers send these shapes is unconfirmed. `screen_capture` and `list_resources` would hit it.
- **Fix:** normalize an empty string or missing arguments to `{}`. Keep rejecting anything else that isn't an object.

### F20. The 30-second header limit can fail cold local models (SOURCE-LEVEL, spec-mandated)

- **Where:** `wire/transport.rs:69`, spec §6.5.
- **Trigger:** Ollama or another local server takes more than 30 s to send headers, for example while loading a large model.
- **Failure:** `Native network: header timeout`, retried up to the bound. This follows the spec but can make a large local model unusable on first use. Ollama's exact header timing wasn't measured.
- **Decision for coordinator:** exempt loopback endpoints or use a longer wait for them.

## P3

| # | Where | Problem | Fix |
| --- | --- | --- | --- |
| F21 | `wire/mod.rs:223` | `.get("error").is_some()` treats `"error": null` as a failure (P05b). Whether any provider sends it is unconfirmed. | Check for non-null. |
| F22 | `runtime/limits.rs`, `legacy.rs:315` | §5.4's per-provider image limits don't exist; only the generic 8 MiB check runs. Anthropic documents 10 MB base64 on its API and 5 MB on Bedrock, so an 8 MiB image (about 11.2 MB base64) is sent and rejected. A failed image may stay in history; that path wasn't traced. | Add per-protocol checks before sending. |
| F23 | `wire/transport.rs:223-228` | Text blocks are scrubbed, but `opaque.value` isn't, so an echoed credential remains in the stored message and same-origin replay. | Scrub string fields in the opaque value, except signatures. |
| F24 | `config.rs:105`, `provider.rs:347` | The fallback `gemini-2.0-flash` has a listed earliest shutdown of June 1, 2026, and it disagrees with the recommended `gemini-3.5-flash`. | Use the recommended model. |
| F25 | `providers/antigravity.rs:277-283` | An account with a valid access token and no `projectId` returns early, so the request is sent with `project: ""`. | Derive the stable ID before returning. |
| F26 | `wire/gemini.rs:80` | Failed results are sent as `{output}`, whereas the reference uses `{error}` and "Tool failed" for empty text. | Use the reference shape. |
| F27 | `wire/gemini.rs` | The §8.9 continuation bridge and the "function-call turn must follow a user turn" repair weren't ported. Only reachable with no user text, such as an image-only prompt, which I didn't trace. The reference's Antigravity system instruction (`stream.ts:86`, `:898`) is also absent, with unknown effect. | Port them, or record the decision to omit them. |
| F28 | `mod.rs:161` | `reqwest::Client::new()` follows redirects by default, and I don't expect its cross-host redirect handling to strip `x-api-key`, `x-goog-api-key` or custom secret headers (not probed). Other credentialed clients here use `Policy::none()`. | Disable redirects for provider requests. |
| F29 | `providers/route.rs:241`, `:315`; `providers/copilot.rs:35` | Three small auth and config issues. An empty `x-api-key` is sent on keyless custom Anthropic. `thinking` and `reasoningEffort` have no setter in `src/shared` or `src/renderer` (from a grep limited to those paths), so thinking can't be enabled from the UI. A revoked but unexpired Copilot session stays cached. | Omit the empty header, add a control or document the omission, and clear the session on a 401 after exchange. |
| F30 | `wire/mod.rs:262-270`, `:427-433` | `length` with unfinished tool JSON reports the invalid-JSON error instead of the labelled incomplete-turn error required by §6.1. No effects run. | Check the stop reason before parsing arguments. |

## Hints from the earlier attempt

- **DeepSeek `reasoning_content: null`: confirmed** as F4.
- **Anthropic empty `partial_json`: confirmed** as F3.

## Checked and found correct

- **Anthropic version:** `2026-10-21` is valid on the official versioning page, and Tezbar sends none of the removed sampling parameters.
- **OpenCode non-Go routes:** Responses, Chat, Anthropic and Google paths match the official docs table. So does the `/inference/v1/models` catalog path.
- **Credential files:**
  - `write_json_atomic` writes through a `NamedTempFile`, which is 0600 on Unix by default, then renames.
  - Malformed stores are refused without being overwritten.
  - Unknown fields and other `auth.json` entries are kept.
  - Tokens stay out of the account listing returned over IPC.
- **Antigravity ports:** `thinking()`, `quota_wall`, the endpoint order, Gemini signature validation and the Gemini 3 observation rule match `pi-antigravity` 0.11.0.
- **Copilot:** session caching, `proxy-ep` host validation and the one-time exchange on 401/403 behave as intended, and device flow requests the `read:user` scope.
- **Replay fidelity:**
  - Anthropic signatures and `redacted_thinking`, Responses reasoning items, and Gemini parts stay in order for the same origin and are dropped for other origins.
  - `extra_content` on Chat tool calls is preserved.
  - Wire IDs stay the same for the same origin and are sanitized otherwise.
- **Effect safety:** the transport runs no tools, and the retry loop is bounded at 3 attempts. Visible text disables retry. Secret-bearing tool arguments are rejected. Cancellation wins the `select!`. Tool calls on an incomplete stream never run.
- **Stream parsing:** split UTF-8, CRLF, a missing final newline and multi-line `data:` handling are correct.
- **OAuth:** PKCE S256, separate state and verifier, issuer check, callback host and port checks, and `Policy::none` on token exchange are in place.

## Coverage

**Read in full:**
- `wire/*`: `mod`, `anthropic`, `chat`, `gemini`, `ollama`, `responses`, `redact`, `transport`
- `providers/*.rs`, plus both Antigravity JSON files and a programmatic check of the OpenCode catalog
- `config.rs`, `auth.rs`, `oauth.rs`, `provider.rs`, `runtime/transcript.rs`, `runtime/limits.rs`

**Read in part:**
- Tests: `wire/tests.rs` and `providers/tests.rs` were sampled, and all test files were grepped for the failing shapes.
- `canonical_tests.rs` and `native_tests.rs` were only grepped.
- `runtime/agent.rs`, `native_tools.rs`, `ext/mod.rs` and the Settings views were read only at the call sites cited here.

**Primary sources used:**
- Official pages for OpenCode, DeepSeek and Anthropic, and Google's deprecations page.
- `pi-antigravity` 0.11.0 and `@earendil-works/pi-ai` 1.3.0, code only, from the local install. No credential files were read.

## Remaining uncertainty

- No authenticated live call was made. F12 and F15–F19 need live confirmation. F4 and F5 are proven in Tezbar, but their upstream reactions weren't tested live.
- The XERJ refresh aborted because another process held its state lock, so `rg`, `grep` and direct reads were used.
- Parallel Chat tool calls that all use `index: 0` with different IDs fail with `changed call ID`. pi-ai also matches on index first, so I didn't report this as a defect. It is a possible live compatibility issue with some gateways.
- `SettingsView.tsx:850,860` can copy a stale top-level `apiKey` or `baseURL` into the active provider's form. If saved, that could send a credential to the wrong host, but I didn't prove a path that reaches this state. The UI reviewer should trace it.
- `/zen/go` was checked only for route status, without credentials.
