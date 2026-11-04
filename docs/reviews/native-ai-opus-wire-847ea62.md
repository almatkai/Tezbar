# Tezbar native AI targeted re-review: Scope A wire closure (N-W1..N-W5)

Reviewer: Claude Opus 5.5. I did the review myself, with no subagents or delegation.
Repository: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-rust-agent`.
Pinned HEAD: `847ea628155238424808a888afa6e9307b461354`. It was clean before and after the review (`git status --porcelain` empty, same HEAD, same branch). I created no worktree; `git worktree list` shows only the 9 pre-existing entries.
Tested source: `21d3e911a03bbea64ffb39324d6822ca343f9ad7`. `git diff --quiet 21d3e91 HEAD -- src-tauri src` returns 0, so HEAD differs only in docs (`847ea62` changes only `docs/NATIVE_AI_VERIFICATION.md` and the ledger).
Previous full audit: pinned `1623d47`. The reports `docs/reviews/native-ai-opus-{wire,runtime,standards}-1623d47.md` were added in `31bc3e2` and no later commit touches them. I left them unchanged.
`git log 1623d47..HEAD`: `31bc3e2` (docs, the audits), `5dddcb4`, `c4b8888`, `96a55cf`, `74c13ed`, `21d3e91` (implementations), `847ea62` (docs).
The Scope A production files changed only in `5dddcb4`. `ai/mod.rs` changed in `5dddcb4` and `21d3e91`.

## Verdict

**Scope A closes.** All five assigned IDs (N-W1..N-W5) are **closed** at their real call sites, and the closure is backed by the parent's regressions and by my own independent loopback probes.

- I found **no new regression** in the touched production paths.
- There are **0 actionable findings** (0 P0, 0 P1, 0 P2, 0 P3).
- Two non-functional judgment notes are recorded below (J1, J2). They are not bugs, and the scope does not depend on them.
- The interop uncertainties U1 (the exact wording of the include-error / `param` field) and U2 (consecutive assistant messages on a strict gateway) are still **unproven**. As the brief asked, I did not turn them into requirements.

| ID | Disposition | Evidence |
| --- | --- | --- |
| N-W1 | Closed | `agent.rs:175-180` uses `route.request_budget`; regression and probe Q1 |
| N-W2 | Closed | `chat.rs:121-142`; regression and probe Q2 |
| N-W3 | Closed | `gemini.rs:79-81`, `:117`, `:122-124`; regression and probe Q3 |
| N-W4 | Closed | `route.rs:266-269`; regression and probe Q4 |
| N-W5 | Closed | `wire/mod.rs:323-324` through `argument_text` (`:97-107`); regression and probe Q5 |

## Method and probe provenance

- **Diff read:** `git diff 1623d47..21d3e91` for `runtime/agent.rs`, `wire/{chat,gemini,mod}.rs`, `providers/route.rs`, `review_regressions.rs` and `ai/mod.rs`. I also read the surrounding production code at HEAD:
  - `limits.rs` in full
  - `chat.rs` in full
  - `gemini.rs` in full
  - `ollama.rs` in full
  - `wire/mod.rs:90-115`, `:285-380`, `:420-450`
  - `route.rs:175-330`
  - `agent.rs:90-260`
  - `mod.rs:470-500`, `:720-735`, `:960-1000`
- **Ledger read:** `docs/reviews/NATIVE_AI_REVIEW_FIXES.md:333-364`.
- **Spec read:** the Gemini auth row in `docs/NATIVE_AI_SPEC.md:719`.
- **Probes:**
  - Snapshot: an immutable `git archive 847ea62` at `/private/tmp/tezbar-wire-847ea62-snap`. It uses the same resource-stripped `tauri*.json`, `dist` and `build/embedding-runtime` as the earlier `1623d47` probe snapshot.
  - Probe module: `#[cfg(test)] mod opus_wire_probe`.
  - Build target: my own, `/private/tmp/tezbar-wire-118-target`.
  - Source: `/private/tmp/tezbar-wire-847ea62-probe.rs`.
  - Log: `/private/tmp/tezbar-wire-847ea62-probe.log`.
  - Result: 6 of 6 probe tests passed (`exit=0`). The probes print behaviour, so a pass is not a correctness claim; the verdicts below come from the printed output.
  - Safety: everything used loopback fixtures and fixture keys. I made no live or paid model call, used no third-party MCP, and did not launch the app. I wrote nothing into the repository.
- **Search:** I did not use XERJ. The prefix is known to be incomplete, and targeted `rg` plus reads on known paths were enough.

## Per-ID closure

### N-W1: current-image validation uses the route budget (CLOSED)

**Call flow.** `native_run_history` processes the current images in this order:

1. `legacy::normalize_images` (`agent.rs:124`)
2. Image blocks pushed into `user` (`:133-138`)
3. Optional restore from the store on retry, or when there are no new images (`:139-153`)
4. The cancellation check
5. For a vision model, `native_route` and then `limits::validate_images_budget(&route.origin.protocol, &[user.clone()], route.request_budget)` (`:174-181`)

Only after that does it check cancellation again, parse `_userGeneration` (`:191-192`), persist with `save_at`, and reach `canonical_turn` and the transport.

**Budget values.** `route.rs:316-321` gives 128,000,000 only when the protocol is Anthropic and the host is `api.anthropic.com`. Every other route keeps `request_bytes(protocol)`. For non-first-party routes the effective budget is therefore exactly what `validate_images` used before, so there is no change for other providers.

**Limits that still apply.**
- The per-image check inside `validate_images_budget` (`limits.rs:28-52`) is unchanged: 10 MB base64 for Anthropic, `IMAGE_BYTES` (8 MiB) base64-ceil for the others.
- History and transport still go through `transport.rs:18-26` (`project_images_budget` and then `validate_images_budget` with the same `route.request_budget`).
- `bound_images` in the run loop is untouched.

**Regression.** `review_regressions.rs` `closure_current_images_use_anthropic_route_budget_before_persistence` sends three real ~9 MB PNGs through `native_run_history`:
- First party: reaches the `sequence` persistence barrier.
- Gateway: gets the `32000000` limit error.

Both outcomes are asserted, so the test discriminates the fix. It is not vacuous.

**Independent probe Q1:**

| Case | Result |
| --- | --- |
| Image size | base64 length 9,002,780 |
| 3 or 4 images to `api.anthropic.com` | Reach the persistence barrier (`expected a sequence`), so validation passed and nothing was persisted |
| 3 images to `gateway.invalid` | `Native limit: Anthropic image request exceeds 32000000 bytes` |
| 2 images to the gateway | Pass |
| Loopback gateway with a request counter, 3 images | Limit error with **0 requests** reaching transport |
| One 10,761,500-byte image to first party | `image exceeds Anthropic's 10 MB base64 per-image limit` (per-image limit retained) |

Direct calls: `validate_images` (32 MB default) rejects the 3-image set and `validate_images_budget(128 MB)` accepts it.

**Upstream:** 128 MB is the first-party figure from the ledger's citation, carried over from the full audit. I did not recheck it live. Tezbar's side is PROVEN.

### N-W2: null or blank continuation id/type/name (CLOSED)

**Code (`chat.rs:121-142`):**
- A continuation `id` or `type` that is `null` or a blank string is skipped. It is never copied and never compared.
- A non-empty `id` or `type` that differs from the stored value fails with `changed call ID`.
- `function.name` is considered only when it is a non-blank string; a non-blank conflict fails with `changed tool name`.
- Arguments handling is unchanged (`:143-154`).
- Terminal: `wire/mod.rs:337` still requires `object_args`, and the raw replay (`:349-380`) rebuilds `tool_calls` from the canonical `wire_id` and name, keeping unknown source fields except `index`, `function` and `id`.

Because a skipped `type: null` is no longer stored, the null can no longer reach the replayed raw `tool_calls`. That was the earlier side effect.

**Regression.** `closure_chat_blank_continuations_and_ollama_missing_arguments` goes through real loopback SSE:
- null and blank variants keep `id=call_1`, `type=function` and `name=read`, with the right arguments;
- a conflicting id, type or name, and `[1]` arguments, are all rejected.

**Independent probe Q2.** Each case was also re-encoded through the same-origin Chat replay.

| Case | Result |
| --- | --- |
| null `id`/`type`/`name` | OK; replay `{"id":"call_1","type":"function","function":{"name":"read",...}}` with no nulls |
| blank `id`/`type`/`name` | OK, same replay |
| omitted id | OK |
| same id repeated | OK |
| empty first id, then `call_9` | OK; identity is `call_9`, set by the first non-empty value |
| conflicting id | Err `changed call ID` |
| conflicting name | Err `changed tool name` |
| conflicting type | Err (fail-closed; see J1) |
| `"[1]"` or `"3"` arguments | Err `tool arguments must be an object` |
| unrelated `extra:null` field | Still retained in raw, as before; unchanged behaviour |

Fail-closed semantics are preserved and no tool executes on any error path. A numeric `id` (a pre-existing case outside this scope) still falls back to a synthetic `tz_0_0`; that is unchanged from `1623d47`.

### N-W3: Gemini canonical blank text and empty turns (CLOSED)

**Code (`gemini.rs`):**
- `Block::Text { text } if !text.trim().is_empty()` (`:79-81`); blank text falls to the existing `_ => {}` (`:117`).
- An entry whose parts end up empty is skipped (`:122-124`, pre-existing).
- The raw exact-origin branch (`:56-74`) is untouched, so signature-only parts are replayed byte-for-byte.
- A canonical block carries no signature, so nothing is lost by the skip.

**Regression.** `closure_gemini_keyless_headers_and_signature_only_replay` decodes a real `{"text":"","thoughtSignature":"c2ln"}` part over loopback. It then asserts:
- the exact-origin replay keeps that part;
- encoding for a different model drops a blank-only assistant turn and yields `[{"text":"answer"}]`.

**Independent probe Q3:**

| Case | Result |
| --- | --- |
| Foreign Anthropic history (blank text plus a tool call), then a tool result | Model turn omitted entirely. One user turn: `u`, `[Observation from read]\nok`, `next`. Before, this was a model turn of only `{"text":""}` |
| Blank-only assistant first | Omitted; contents `[user u]` |
| Blank user text plus an image | The image-only bridge `"See the attached image."` now fires correctly (before, the blank part suppressed it) |
| Exact-origin signature-only answer | Unchanged: `[{"text":"","thoughtSignature":"c2lnbmF0dXJl"},{"text":"answer"}]` |
| Same history, other model or other provider (canonical) | `[{"text":"answer"}]` |
| Exact-origin signed function-call group | Unchanged: the signature-only text part and the signed `functionCall` are kept, followed by the `functionResponse` |
| That group sent to another model | Becomes observation text in the user turn, per the existing foreign-call design; no empty model turn |

A blank-only *user* turn now encodes to an empty `contents`. It is not reachable: the agent IPC rejects `task.trim().is_empty()` (`mod.rs:729-731`), tool-result turns always carry parts, and the retry message is non-blank. Before, it would have sent `[{"text":""}]`, which is also invalid. **No regression.**

**Upstream:** Gemini's reaction to empty parts was not verified (still OPEN INTEROP from the full audit). The fix removes the shape either way.

### N-W4: keyless custom Gemini omits `x-goog-api-key` (CLOSED)

**Code (`route.rs:266-269`):** the header is added only when `!key.is_empty()`.
- The branch condition (`protocol == Gemini && p != "opencode" && p != "antigravity"`) is unchanged, so OpenCode and Antigravity keep the Bearer `else if` branch.
- Custom provider headers are applied later with `HeaderMap::insert` (`:302-314`), so an explicit header still wins.
- First-party providers still require a key (`:189-193`).

**Regression:** the same loopback test asserts the header is absent when keyless and set to `fixture-explicit` when configured through `customProviders`.

**Independent probe Q4:**

| Case | `x-goog-api-key` | `authorization` |
| --- | --- | --- |
| Keyless custom | absent | absent |
| Keyed custom | the key | absent |
| Keyless + explicit header | `fixture-explicit` | absent |
| Keyed + explicit header | `fixture-explicit` | absent |
| OpenCode Gemini family | absent | `Bearer ...` |

A first-party keyless `gemini` still fails with `gemini requires an API key or sign-in`.

**Antigravity:** I checked it by reading the source (unchanged branch), not with a probe, because its route needs OAuth account state.

### N-W5: Ollama missing or null arguments (CLOSED)

**Code (`wire/mod.rs:323-324`):** Ollama now uses the shared `argument_text` (`:97-107`):
- `null`, a missing value, or a blank string become `{}`;
- an object is serialized;
- any other string passes through and is then checked by `object_args` (`:337`);
- numbers, booleans and arrays fail.

The raw replay converts back to an object (`:361-366`). `ollama.rs` decode is unchanged.

**Regression:** the same test runs missing, `null`, `3` and `[]` through real NDJSON transport and asserts `{}` for the first two and an `arguments` error for the last two.

**Independent probe Q5:**

| Arguments | Result |
| --- | --- |
| missing, `null`, `{}`, blank string | Canonical `"{}"`, replayed as the object `{}` |
| `{"a":1}` | Accepted; round-trips as an object |
| `3`, `[1]`, `true` | Err `tool arguments must be an object or JSON object string` |
| `"[1]"` | Err `tool arguments must be an object` |

A JSON-object *string* is newly accepted on Ollama (see J2).

## Touched-path regression checks

- **Redaction caching (`ai/mod.rs:965-1000`):** `redact_text` and `redact_output` now use `LazyLock`-cached regexes with the identical pattern strings, order and replacement templates. Probe Q6 compared the new functions against a verbatim copy of the old implementation on 9 inputs (PEM, `sk-`, `ghp_`, `AKIA`, Bearer, keyword fields, mixed case, empty, Unicode): **9/9 identical** for both functions. `LazyLock` is stable since Rust 1.80, below the 1.85 MSRV. Lifecycle and log draining belong to another reviewer.
- **Opaque credential guard and signatures:** `transport.rs`'s exact-secret `opaque_has_secret` check and the policy of never rewriting signed or encrypted fields were not touched by this diff. Q3 shows signature-bearing parts replayed byte-for-byte.
- **Tool and effect replay:** in the N-W2 and N-W5 paths, every error happens in decode or finish, before any tool batch (`agent.rs` effect loop untouched). The raw `tool_calls` replay shape is unchanged apart from no longer copying a null `type`.
- **Transcript and limits seams:** `limits.rs` is unchanged, and the transport budget path is unchanged.
- **Formatting:** the `agent.rs` changes besides N-W1 are rustfmt-only (cancel checks and the `options` tuple), with no semantic change.
- **Prior closures:** F1–F30, W1–W15 (W12 now fully closed through N-W1), runtime assistant-first and the Gemini fallback were closed in the full `1623d47` audit. The diff gives no new source evidence to reopen any of them.

## Judgment notes (not functional failures, not counted)

- **J1, mislabeled error.** A non-empty `type` other than `"function"` on any chunk now fails with the text `changed call ID` (Q2 "conflicting type" and "first type custom"). Before, the value was silently copied into the raw replay while the call ran as a function. Tezbar declares only function tools, so failing closed is defensible and safe. The message names the wrong field.
  - Cosmetic fix, optional: use `changed call type` for the `type` key.
  - Whether any gateway emits a non-function type here is unproven.
- **J2, leniency on Ollama.** Ollama now accepts a JSON-object *string* for `arguments` and normalizes it to an object on replay. Native Ollama documents object arguments; this matches the Chat, Responses and Gemini rule, is harmless, and still rejects scalars and arrays.

## Remaining unproven interop (unchanged, not new requirements)

- **U1:** the include-fallback trigger wording and `param`.
- **U2:** consecutive assistant messages on strict OpenAI-compatible templates.
- **U3 residue:** how real gateways or Gemini react to the shapes that are now removed. Moot for N-W2..N-W5 on Tezbar's side.
- **Provenance:** all probe results are local Tezbar behaviour against loopback fixtures, not live provider acceptance or real OS or window events.

## Coverage and limitations

- **Covered:** the five assigned IDs, the Scope A production diff, the stderr/redaction regex change (semantics only), the parent's three new regressions (read, and their assertions checked as discriminating), and 6 independent probe tests.
- **Not covered:**
  - full-suite reruns (parent evidence in `docs/NATIVE_AI_VERIFICATION.md` was not re-executed by me);
  - the renderer, settings lifecycle, MCP, secrets, OAuth and keychain changes in `5dddcb4`..`21d3e91` (other reviewers' scope);
  - Windows;
  - live providers;
  - Antigravity header behaviour beyond reading the source.

## Final counts

| Item | Count |
| --- | --- |
| Assigned IDs closed | **5/5** (N-W1..N-W5) |
| New regressions | **0** |
| Actionable findings | **0** (P0 0, P1 0, P2 0, P3 0) |
| Judgment notes | 2 (J1, J2), non-functional |
| Unproven interop items carried forward | 2 (U1, U2), not requirements |

**Closure verdict for Scope A at `847ea62` (source `21d3e91`): CLOSED.** This is a scope-limited statement. It does not cover the runtime or standards scopes.
