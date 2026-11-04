# Native AI review repair ledger

Source reviewed: `32ba9c4e5ff41b527ba5943f779a3f9ea090d135`, compared with
`v0.3.0-beta.7`. Claude Opus 5.5 completed three independent scope reviews.

- [Provider/wire spec axis](native-ai-opus-wire-32ba9c4.md): F1–F30.
- [Runtime/UI spec axis](native-ai-opus-runtime-32ba9c4.md): R1–R20 and the P3 list.
- [Standards/security axis](native-ai-opus-standards-32ba9c4.md): P1-1–P1-5,
  P2-1–P2-12 and the P3 list.

These are original review reports, not claims that the findings have been fixed.
Duplicate findings should map to one tested repair. Confirmed functional P3 issues
also need repair; judgment-only smells must be distinguished from bugs.

## Coordinator decisions for repairs

1. Remove optional `opencode models` discovery too. Static metadata and native HTTP
   discovery are sufficient; opening Settings should not launch a program from PATH.
2. Fix credential ownership first: Copilot must never borrow generic `apiKey`, and
   persisted provider patches must not retain extension-contributed headers/models.
3. Preserve original images and completed effects during manual Retry. Canonical
   stored data must not be downgraded by missing UI image bytes or an OCR projection.
   Prepare a capability-specific wire view without mutating canonical history.
4. Review never approves through a mode change or unrelated Enter press. Missing
   profiles fail closed. Always-allow is unavailable when the policy always asks.
5. Imports/review show exact command, cwd and every declared variable (secret values
   remain masked). Execution-altering project variables require explicit review.
6. Trust revocation must be idempotent. Tool review accepts the exact catalog hashes
   displayed, retains before/after, and cannot be bypassed by an unrelated edit.
7. Provider image limits and codec decisions must use current primary documentation,
   not a reviewer's remembered old limit. Optional fields should be omitted rather
   than sent as invalid nulls. Preserve signed payload fidelity.
8. Keep remote header timeout 30 s by default. Cold local model servers can use a
   bounded configurable longer header deadline (default 120 s for local/Ollama,
   maximum 300 s). This changes the earlier uniform deadline intentionally; hard
   turn/cancel limits remain. Test without real-time multi-minute waits.
9. Fix all concrete functional P3 cases, including redirect credential safety,
   stale auth/cache/status, quadratic framing, Windows argument interoperability,
   and UI data-loss/copy/accessibility issues. Refactoring smells alone do not
   justify unrelated rewrites; document the chosen disposition with evidence.
10. The task is not complete until fixes have passed fresh tests on Mac and Windows
    and Claude Opus 5.5 has re-reviewed the repaired paths with no outstanding
    actionable findings. No push, merge or release is authorized by this ledger.

## Dispositions

The historical dispositions below describe the first repair pass. The new re-review
found actionable gaps; the current repair table at the end supersedes closure claims
for those paths. Final closure still requires the parent's **fresh** Opus 5.5 review.
Original wire/runtime/standards reports are unchanged.

Historical commit keys: A = `9b76b95`; B = `e672ecb`; C = `fcabc385f6eeb189020881cb2c4c8425fba0cdf5`
(provider/wire); D = `118cb7b4c994cd766503b9078a7d836aad311958` (extension/lifecycle). Final-source validation is
recorded in NATIVE_AI_VERIFICATION.md. Every test below
uses fixture credentials and temporary profiles; none establishes live entitlements.

### Provider / wire report

| ID | Disposition | Exact regression / evidence | Commit |
| --- | --- | --- | --- |
| F1 | Fixed: no generic key fallback; empty Copilot credentials cannot reach catalog, exchange or inference. | `providers::tests::unsigned_copilot_never_borrows_generic_credentials_or_makes_requests` reproduced red before repair and passes green. | A |
| F2 | Fixed default Go root in code/spec to `/inference/go`. | `opencode_go_default_and_explicit_uncatalogued_protocol_are_native_routes` | `fcabc38` |
| F3 | Fixed empty/whitespace completed argument buffers to `{}`; malformed/incomplete blocks remain errors. | `wire::tests::empty_tool_json_null_reasoning_and_completed_empty_responses_are_valid`; `anthropic_terminal_does_not_complete_an_unclosed_tool_block_but_json_is_complete` | `fcabc38` |
| F4 | Fixed nullable reasoning deltas; DeepSeek same-origin replay supplies empty string for missing/null reasoning. | `empty_tool_json_null_reasoning_and_completed_empty_responses_are_valid` | `fcabc38` |
| F5 | Fixed image followups after the whole contiguous tool-result run, across canonical messages. | `chat_tool_results_stay_contiguous_before_image_followups_across_messages` | `fcabc38` |
| F6 | Fixed reasoning-details accumulation by indexed/identified fragment, retaining separate unindexed entries. | `empty_tool_json_null_reasoning_and_completed_empty_responses_are_valid` | `fcabc38` |
| F7 | Fixed decoder empty-final error classification to enable bounded AG retry before text. | `antigravity_empty_reply_retries_and_model_fallback_has_its_own_budget` | `fcabc38` |
| F8 | Fixed independent model/account/endpoint budgets; recognized missing-model errors try model fallback first. | `antigravity_model_fallback_is_reached_with_all_three_endpoints_configured`; `antigravity_quota_rotates_accounts_and_transport_errors_walk_endpoints` | `fcabc38` |
| F9 | Fixed default AG URL to remain implicit after Settings save, preserving all endpoints. | AG three-endpoint regression above also asserts a saved default URL stays implicit. | `fcabc38` |
| F10 | Fixed HTTP string error/message/detail extraction, bounded and redacted. | `provider_errors_keep_messages_retry_before_text_and_never_persist_opaque_secrets` | `fcabc38` |
| F11 | Fixed HTTP 529 and typed stream overload/server/rate-limit errors before visible text; provider messages retained and scrubbed. | `provider_errors_keep_messages_retry_before_text_and_never_persist_opaque_secrets`; existing visible-disconnect no-replay fixture. | `fcabc38` |
| F12 | Fixed official-host Chat token field to `max_completion_tokens`; other hosts retain compatibility defaults. | `official_openai_chat_uses_completion_token_limit_through_real_transport`; official Chat reference cited below. | `fcabc38` |
| F13 | Fixed custom status/catalog using the actual resolved protocol and credential headers. | `custom_catalog_status_uses_protocol_and_headers` covers Anthropic, Gemini, Ollama, Responses. | `fcabc38` |
| F14 | Fixed explicit per-model Protocol selector including uncatalogued OpenCode IDs; no name guessing. | Settings renderer test `does not borrow the previous provider key or URL when reloading AI settings`; `opencode_go_default_and_explicit_uncatalogued_protocol_are_native_routes`. | `fcabc38` |
| F15 | Evidence-backed dismissal of proposed header change: current gateway docs explicitly specify Bearer for all inference APIs including Gemini. Keep Bearer. | Official OpenCode Inference API Authentication + Gemini endpoint table below; explicit Gemini-route regression asserts Authorization. Live rejection was never proven. | `fcabc38` |
| F16 | Conservative fix: request encrypted reasoning only for enabled/capable reasoning routes; preserve all received encrypted items. | `responses_keeps_encrypted_reasoning_and_call_identity`; official Responses migration docs. Upstream rejection of unconditional include remains unproven. | `fcabc38` |
| F17 | Fixed empty completed output preserving already streamed done items/calls. | `empty_tool_json_null_reasoning_and_completed_empty_responses_are_valid` | `fcabc38` |
| F18 | Fixed family-specific call/result IDs, including identical parallel call names/args; Gemini does not gain synthetic IDs. | `gemini_family_ids_and_error_results_are_correlated_without_synthesized_gemini_ids` | `fcabc38` |
| F19 | Fixed missing/empty object arguments for Chat/Responses/Gemini without accepting non-object/malformed JSON. | `empty_tool_json_null_reasoning_and_completed_empty_responses_are_valid`; `duplicate_ids_and_non_object_arguments_fail_before_execution` | `fcabc38` |
| F20 | Fixed configurable bounded header wait: remote30/local120/max300; cancellation and single-turn hard deadline remain. | `wire::transport::repair_tests::local_header_wait_defaults_to_120_seconds_and_is_bounded` uses virtual time; existing actual header/cancel/effect fixtures. | `fcabc38` |
| F21 | Fixed `error:null` handling. | `empty_tool_json_null_reasoning_and_completed_empty_responses_are_valid` | `fcabc38` |
| F22 | Fixed recursive per-protocol image validation and request-size limits, from current docs rather than old 5 MB memory. | `anthropic_saved_empty_tool_text_is_omitted_and_base64_image_limit_is_current`; canonical history/image tests. | `fcabc38` |
| F23 | Fixed opaque credential retention by refusing unsafe opaque history, rather than rewriting signed/encrypted payloads. Generic scrubbing excludes cryptographic fields; exact known credentials still fail closed. | `provider_errors_keep_messages_retry_before_text_and_never_persist_opaque_secrets`; signed/encrypted replay fixtures. | `fcabc38` |
| F24 | Fixed Gemini fallback to the existing recommended model (`gemini-3.5-flash`), with vision metadata. | `config::selected`, provider fallback, current Google model docs below; final provider suite. | `fcabc38` |
| F25 | Fixed valid-token/missing-project path deriving the stable project before early return. | AG three-endpoint fallback fixture deliberately has no projectId and asserts nonempty request project. | `fcabc38` |
| F26 | Fixed Gemini error results, including empty failure text. | `gemini_family_ids_and_error_results_are_correlated_without_synthesized_gemini_ids` | `fcabc38` |
| F27 | Fixed image-only and assistant-first continuation bridges without altering canonical history. Deliberately retain Tezbar's own system identity; copying AG's identity prompt is not necessary for wire validity and could misrepresent the app. | `gemini_keeps_signed_part_boundaries_and_foreign_observations`; image encoder fixtures; base prompt explicitly identifies Tezbar. | `fcabc38` |
| F28 | Fixed credentialed provider redirects, including catalogs/custom headers. | `credentialed_catalog_and_inference_refuse_redirects` proves second origin receives no request. | A |
| F29 | Fixed empty custom Anthropic key header, thinking control, and eviction/re-exchange of rejected unexpired Copilot sessions. | `revoked_unexpired_copilot_session_is_exchanged_again_once`; custom protocol/catalog fixtures; actual Settings thinking checkbox. | `fcabc38` |
| F30 | Fixed incomplete stop reason validation before completed-tool JSON parsing. | `incomplete_tool_json_is_labelled_before_parsing` | `fcabc38` |

### Runtime / renderer report

| ID | Disposition | Exact regression / evidence | Commit |
| --- | --- | --- | --- |
| R1 | Fixed dedicated structured shell capture preserving both streams, exit status and timeout partial output, bounded/redacted. Success-only `output()` callers unchanged. | `native_tests::shell_failure_stderr_success_and_timeout_keep_both_streams`; `native_output_is_unicode_bounded_and_reported_to_model_and_stage` | B |
| R2 | Fixed original canonical image ownership on reopened/manual retry and OCR. | `canonical_tests::retry_reuses_original_image_and_preserves_completed_effect_history` | B |
| R3 | Fixed disposable recursive nonvision history projection, preserving canonical image bytes for switching back. | Same canonical retry/model-switch test; `nested_images_are_excluded_from_text_context_and_projected_recursively` | B |
| R4 | Fixed mode picker never resolving pending approval. | ownership renderer test `mode changes leave destructive approval pending and expired cards disappear` | B |
| R5 | Removed global Enter approval; native focused buttons retain Allow/Deny intent; composer/menu Enter cannot approve. | ownership `honors approval persistence capability … for controls and shortcuts`, mode/composer regression | B |
| R6 | Fixed launcher→native queue→independent Settings request; reload/scroll on existing-window event. No sessionStorage. | `project_review_handoff_survives_an_independent_settings_request` | B |
| R7 | Fixed native filtering of contributed provider/model/config patches, enabled ownership checks, fresh manifest model merge. | `declarative_provider_is_native_scoped_and_its_header_is_never_returned` saves merged config before disabling; checks raw persistence/revocation. | A/C |
| R8 | Fixed new empty outputs to `(no output)`; historical empty Anthropic text blocks omitted. | Shell capture fixtures; `anthropic_saved_empty_tool_text_is_omitted_and_base64_image_limit_is_current` | B/C |
| R9 | Fixed recursive image-free text sizing and in-loop image budget. | `nested_images_are_excluded_from_text_context_and_projected_recursively` | B |
| R10 | Fixed session/turn/global generations plus run cancellation preventing delete/clear resurrection. | `active_delete_and_clear_cannot_resurrect_transcript_files` through real chat dispatch. | B |
| R11 | Fixed explicit manual retry intent, authoritative previous tool outcomes, unknown-call closure; no blind replay. | `manual_retry_keeps_actual_completed_shell_effect_once` has a real append effect; canonical reopened retry fixture. | B/D |
| R12 | Fixed missing profiles failing closed before provider access. | `missing_profile_fails_closed_before_provider_request` | B |
| R13 | Fixed unavailable Always controls and native rejection for operations that always require review. | Native review/Always test; expanded policy matrix; renderer approval capability tests. | B |
| R14 | Fixed custom-provider Protocol creation/editing plus per-model selector. | Real Settings renderer protocol persistence; `custom_catalog_status_uses_protocol_and_headers` | B/C |
| R15 | Fixed stale Pi onboarding wording and shared runtime comments. | Full frontend build/typecheck; source no longer makes those claims. | B/D |
| R16 | Fixed capture-phase escape consumer for composer menu/argument row before App navigation. | ownership test `repeats unnonced launcher commands through App while the real chat stays mounted` now drives both Escape paths through real App. | B/D |
| R17 | Fixed true descriptor comparison, full before/after schema fields, removed tools and Keep disabled; native accepts only displayed pins/trust hash. | `pending_catalog_survives_refresh_edit_and_rejects_stale_review`; AiExtensionsSection dark/white review tests. | `118cb7b` |
| R18 | Fixed provider-aware image/request limits before new vision image persistence and on every history/tool request. | Same image-limit and model-switch regressions as F22/R3; `oversized_historical_tool_image_is_projected_without_poisoning_or_mutating_history` verifies canonical preservation and continuation. | `fcabc38` |
| R19 | Fixed legacy-only UI fallback and native model-switch migration of the original credential owner. | `model_switch_migrates_legacy_credentials_to_their_original_owner`; real Settings reload test. | A/C |
| R20 | Fixed always-accessible approval mode picker outside approval cards while a run is active. | Renderer mode test and native live-mode test. | B |

Runtime P3 bullets are numbered below in their original order (including all subpoints).

| P3 item | Disposition and evidence |
| --- | --- |
| Runtime P3-01: pre-start retry failure | Retry marks saved prior attempt as historical context before configuration/cwd preflight, retaining effects and closing unknown calls. Generation-checked write cannot resurrect deletion. Manual retry regression exercises the same dispatch path. |
| Runtime P3-02: expired approval | `approval-expired` clears renderer state; mode/expiry renderer regression. |
| Runtime P3-03: assistant-first window | Anthropic and Gemini encode an explicit continuation user bridge; signed-block fixtures keep original assistant content intact. |
| Runtime P3-04: removed folder | Config-get and agent-run now return `Directory does not exist` rather than a raw canonicalize OS error. |
| Runtime P3-05: Gemini default | F24, recommended model and matching vision metadata. |
| Runtime P3-06: row actions/Details | Sign in, Try again, policy select, Cancel sign-in, program/web time limit, Show all tools implemented; section renderer tests and TS checks. Policy edits reopen the reviewed form and retain typed Allow-all confirmation. |
| Runtime P3-07: composer/a11y | Extension group titles, resolved profile name/unavailable label, selected-option scroll, combobox role/expanded state. Existing composer/profile tests and real App Escape regression. |
| Runtime P3-08: error colours | Shared `.ai-extension-error` uses dark Message error colour and high-contrast white theme override; role=alert. Tests cover both theme attributes; no claim of screenshot-based visual QA. |
| Runtime P3-09: import file reset | Clears old preview and file input before loading new file; exact preview renderer tests and source change. |
| Runtime P3-10: Pi extension alias | Deliberate scoped alias: legacy `pi-extensions:list` supplies read-only not-loaded Pi items to its Pi-only consumer; AI extensions use `ai-extensions:list`. Mixing enabled AI items into that not-loaded list would be false. Spec alias clarified, no runtime fallback restored. |
| Runtime P3-11: prompt ownership/dead code | Native base includes cwd, installed-app search and shorthand guidance; renderer prompt builder and obsolete tests removed. Canonical dispatch fixtures exercise native prompt. |
| Runtime P3-12: stale copy/comments | Removed implementation-word approval title, old shared Pi comments and Pi-only exit-message branch; `formatLlmErrorMessage` native error test, frontend checks. |
| Runtime P3 coordinator OpenCode decision | Removed optional models CLI execution completely. Source guard now rejects it; static/native HTTP catalog fixture passes. |

### Standards / security report

| ID | Disposition | Exact regression / evidence | Commit |
| --- | --- | --- | --- |
| P1-1 | Fixed cross-webview Project Review. | R6 native handoff regression. | B |
| P1-2 | Fixed exact command/argv/cwd, declared env/header names and masked secrets in preview/project/Details; execution-altering variables flagged before explicit Test. Default program cwd is explicit home/project folder rather than process cwd. Project contributions shown. | AiExtensionsSection preview/edit tests; project no-start-before-review fixture; argument/draft regression. | `118cb7b` |
| P1-3 | Fixed idempotent project revocation and change-only writes/events. | `changed_project_revocation_is_idempotent_and_has_one_event` repeats revocation five times after whitespace change. | `118cb7b` |
| P1-4 | Fixed globs, tilde-user, unsupported syntax, attached mutation options before remembered/Turbo; Windows literal backslashes supported. | `risky_shell_always_precedes_remembered_and_turbo`; Windows process and path suite. | B/D |
| P1-5 | Fixed raw multiline argument draft until save/Test. | Renderer `preserves typed multiline arguments, cleared timeout and an unsaved instructions draft across reloads`. | `118cb7b` |
| P2-1 | Fixed secretless connections avoiding key reads and Remove recording namespace revocation without decrypting/destroying unrelated ciphertext. No plaintext fallback. | `missing_secret_key_does_not_block_secretless_test_or_remove` compares unrelated ciphertext bytes; existing secret-store unavailable tests. | `118cb7b` |
| P2-2 | Fixed extension contributions persisting after autosave/model switch. | R7 regression. | A/C |
| P2-3 | Fixed unresolved profiles widening tools. | R12 regression. | B |
| P2-4 | Fixed approved versus pending catalog retention, edit bypass and stale-review acceptance; native bookkeeping cannot be overwritten by UI fields. | `pending_catalog_survives_refresh_edit_and_rejects_stale_review`; notification snapshot tests. | `118cb7b` |
| P2-5 | Fixed clearing connection error/needsAuth on successful discovery and OAuth persist; UI reads needsAuth. | `insufficient_scope_is_recorded_once_and_successful_discovery_resets_status`; OAuth full-flow fixture. | `118cb7b` |
| P2-6 | Fixed canonical URL storage/comparison, including root slash/case/default port/whitespace. | `metadata_cannot_redirect_remote_resources_to_private_services`; root native MCP catalog/status and OAuth fixture. | `118cb7b` |
| P2-7 | Fixed omit empty scope, challenge/metadata scope discovery and stored insufficient_scope step-up without auto-call replay. | `insufficient_scope_is_recorded_once_and_successful_discovery_resets_status`; OAuth PKCE cancel scenario asserts scope absent; challenge parser fixture. | `118cb7b` |
| P2-8 | Fixed startup-exit reason/scrubbed log retention and fresh HTTP404 versus expired session distinction. | `failed_start_preserves_reason_and_log_without_call_replay`; legacy expired-session fixture. | `118cb7b` |
| P2-9 | Fixed positive 1–1800 timeout validation and raw UI field text; cleared UI defaults at save, imported0 rejected. | `invalid_add_has_no_orphan_secrets_and_timeouts_are_positive_bounded`; multiline/timeout renderer regression. | `118cb7b` |
| P2-10 | Fixed trusted-identity legacy era cache; changed execution identity clears cache, silent probe skipped on cold reconnect. No tool replay. | `silent_legacy_era_is_cached_by_trusted_connection_for_cold_snapshot` asserts cold snapshot under 4s; modern/legacy dispatch tests. | `118cb7b` |
| P2-11 | Fixed Protocol select. | R14/F14 tests. | B/C |
| P2-12 | Fixed optional MCP tool description to string/title fallback, never null. | Insufficient-scope fixture has descriptionless tool and asserts wire definition is string. | `118cb7b` |

Standards P3 bullets are numbered in original order:

| P3 item | Disposition / exact evidence |
| --- | --- |
| Standards P3-01 backoff | Successful starts clear failures. `unrelated_edit_does_not_cancel_a_disabled_connection_test_and_success_resets_backoff`. |
| Standards P3-02 Test cancellation | Change sweep only cancels actual enabled→disabled transitions; same regression interleaves an unrelated instruction save during a disabled connection's Test. Explicit Disable still cancels starts. |
| Standards P3-03 Lines | Scan cursor prevents rescanning a long partial line on every chunk; existing byte-framing tests plus complete MCP suites. |
| Standards P3-04 list_changed | Notifications no longer reject old immutable run calls/startup catalogs; next snapshot refreshes and withholds changed hashes. `catalog_notification_keeps_run_snapshot_immutable_and_requires_changed_tool_review`. |
| Standards P3-05 subscriptions/TTL | Subscription HTTP timeout uses its long listen lifetime; fallback uses bounded minimum returned ttlMs (default 60s). Continuation fixture asserts ttlMs2000 persisted. |
| Standards P3-06 input_required | Continuation reuses tool descriptor mirrors and configured timeout with new RPC ID. `continuations_preserve_mirrors_and_sse_eof_reports_unknown_outcome`. |
| Standards P3-07 SSE EOF | Missing correlated response reports unknown outcome, never auto-replays. Same continuation/SSE fixture counts exactly3 calls. |
| Standards P3-08 Windows | Literal backslashes supported; PATH/PATHEXT resolution and guarded cmd/bat invocation, with ComSpec/PATHEXT/ProgramData allowlist. `windows_repair_tests::pathext_cmd_programs_accept_literal_arguments_and_refuse_injection`, existing real Job Object cancellation test. Unsupported batch expansion/metacharacters fail closed. |
| Standards P3-09 failed-add slug | Uses native returned record and preserves server-owned slug on update/retry. Multiline failed-start renderer regression; native update preserves pinned bookkeeping by ID. |
| Standards P3-10 instruction budget | Bounded assembly names the truncated source instead of failing every run. `combined_instructions_are_bounded_and_name_the_source_that_was_truncated`. |
| Standards P3-11 disabled provider | Agent dispatch now validates selection after merging scoped contributions; R7/R12 no-network fixtures. |
| Standards P3-12 OAuth SSRF | No redirects; remote metadata/token/registration endpoints reject private/local resolutions and use pinned DNS addresses, including token exchange/refresh. `metadata_cannot_redirect_remote_resources_to_private_services`; OAuth full-flow fixtures. |
| Standards P3-13 instruction draft | Dirty draft survives background list events. Multiline/timeout/instructions renderer regression. |
| Standards P3-14 orphan secrets | Prevalidate all confirmations before secret writes; rollback guard spans durable record write and revokes only new namespaces on failure. `invalid_add_has_no_orphan_secrets_and_timeouts_are_positive_bounded`. |
| Standards P3-15 Always | R13 controls/native rejection; no fake permanent allowances. |
| Standards P3-16 copy/UI | Profile display name, theme-aware errors, Try again and policy select; R15–R17 and renderer fixtures. |
| Standards P3-17 project bookkeeping | Retrust replaces old same-file disabled record; needsProjectReview cleared for new trust. Existing project byte-change/no-execution regression and revocation event test. |

Judgment-only smells: no unrelated architectural rewrite. JSON record ownership is now
concentrated at native edit/review seams, immutable reviewed state is preserved, and
changed Rust paths are formatted for review. Module size, duplicate naming/migration
helpers and a future typed Connection model remain refactoring opportunities, not
functional dismissals or substituted fixes.

### Primary-source checks (2026-11-04)

- [Claude vision](https://platform.claude.com/docs/en/build-with-claude/vision): direct API 10 MB base64/image, first-party Messages 128 MB/request (32 MB gateway fallback). Bedrock's old 5 MB value is not used for the direct adapter.
- [Gemini inline inputs](https://ai.google.dev/gemini-api/docs/file-input-methods): current inline payload 100 MB; [current models](https://aistudio.google.com/docs/models) confirms the selected recommended3.5Flash remains catalogued. The app retains its tighter 8MiB/image cap.
- [OpenAI image requirements](https://developers.openai.com/api/docs/guides/images-vision?api-mode=chat): current aggregate payload 512 MB; app image budget is tighter.
- [OpenAI Chat reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create): max_completion_tokens replaces max_tokens for reasoning compatibility. [Responses migration](https://developers.openai.com/api/docs/guides/migrate-to-responses): stateless reasoning items are replayed; completed encrypted payloads are preserved.
- [OpenCode inference](https://dev.opencode.ai/v2/docs/console/api/inference) and [harnesses](https://dev.opencode.ai/v2/docs/console/harnesses/): Bearer authentication, native family paths, supported paid/Go access and explicit free-tier403 restrictions. No client impersonation or bypass introduced.

Provider-side rejection/acceptance, quota and entitlements remain external-only: no
credentialed live model request was used to validate them. Fixtures prove request
shaping, no-request/redirect ownership, bounded retry and effects/history behavior.
Fresh final macOS/Windows results are recorded separately. New Opus re-review is
**pending with the parent**, and is not marked complete here.


## Re-review repairs at a28c564 (2026-11-04)

Source commit: `a28c5647dc589a33b60b2c80ca082246c1f0bf75`.
All 26 W/N/U items below have implementation changes and regression evidence.
These are implementation dispositions, **not reviewer closure**. The parent must
obtain fresh Claude Opus 5.5 wire, standards and full runtime closure; the partial
runtime recovery notes do not satisfy that gate. Original reviewer files remain
unchanged. Exact final suite/platform results and the archive hash are in
[the verification record](../NATIVE_AI_VERIFICATION.md).

Tests in `review_regressions.rs` run real `canonical_turn` HTTP/stream transport,
route/config and encode/decode seams, using loopback providers and fixture keys.
The old T01–T07 probe prints were replaced by assertions. Renderer regressions use
the real Settings and Chat components and SDK with mocked native IPC. Nothing here
establishes live provider entitlement, desktop visual quality, or upstream behavior.

| ID | Implemented change | Permanent regression / boundary evidence |
| --- | --- | --- |
| W1 | Opaque history fails closed only on exact configured credentials of plausible length, excluding local placeholders. Model output uses high-confidence shapes; errors/logs retain keyword scrubbing. Signed/encrypted data is never rewritten. | `coding_output_thinking_arguments_and_placeholder_keys_survive_all_codecs` covers text, thinking, arguments, all five codecs, real fixture-secret refusal and none/EMPTY/lm-studio. Existing byte-split exact-secret test retained. |
| W2 | Stateless Responses always requests encrypted reasoning, including unknown capabilities. Replay omits unresolvable reasoning and subsequent server item IDs while preserving call IDs and encrypted payloads. One unsupported-include HTTP400 fallback is allowed before visible output, below the effect loop. GPT6 inference includes thinking and vision in TS/Rust. | `regression_w_t02_responses_reasoning_without_encrypted_replay`; `unsupported_responses_include_falls_back_once_before_output_and_keeps_encrypted_replay`; `anthropic_thinking_shape_matches_model_generation_and_gpt6_infers_vision_and_reasoning`; shared TS inference test; existing exact encrypted replay fixture. |
| W3 | Thinking opt-in is gated by model capability. Anthropic generation-specific manual budget versus adaptive shape; unknown unsupported generations omit it. Default llama3.2 never sends think=true. | `regression_w_t03_thinking_checkbox_routes`; generation shape table test covers Haiku/Sonnet/Opus4.5, Opus/Sonnet4.6, Opus5.5 and unsupported Haiku3.5. |
| W4 | Model rediscovery spreads existing metadata before filling missing capability fields, preserving protocol/maxTokens. Pending edits fence stale discovery writes. | Real Settings test `retains a chosen protocol and token limit after model rediscovery`. |
| W5 | Raw Anthropic replay omits only blank text blocks; thinking/redacted/signed blocks remain intact. | `regression_w_t05_anthropic_raw_empty_text_and_responses_shapes`; existing exact signed-block replay test. |
| W6 | OpenCode default is supported paid `opencode/gpt-5.4-mini`; native defaults/discovery omit Big Pickle and free-suffix IDs. A403 gives supported-paid/Go/access guidance, without asserting every403 is a free-tier rejection. | `regression_w_t06_routes_and_json_errors` asserts actual default route; defaults mirrors and native discovery fixtures. |
| W7 | Status and credential-availability IPC recognize Go-only keys. Zen/Go catalogs query separately with their scoped keys and merge, with independent static fallback per family. | `regression_w_t04_opencode_go_only_key` asserts actual status/config IPC, Go route, both Authorization headers and both live families. |
| W8 | Streamed argument buffers survive empty completion fields; object-valued arguments serialize faithfully and scalar/array types fail closed. Streamed Responses text is rebuilt into replayable message blocks when completed output is empty. Empty Chat deltas remain fragments until terminal validation. | `regression_w_t05_anthropic_raw_empty_text_and_responses_shapes`; `repeated_signature_keeps_first_nonempty_and_invalid_argument_types_fail_closed`; `streamed_empty_argument_fragments_never_replace_accumulated_arguments` through real transport. |
| W9 | JSON error bodies are checked before codec wrapping; message and typed retry classification are retained. | `json_provider_errors_preserve_message_and_typed_retry_policy_on_every_codec` asserts message and single nonretryable request; existing typed overload retry and visible-output no-replay fixtures retained. |
| W10 | Custom native Gemini uses x-goog-api-key. OpenCode and Antigravity retain their Bearer contracts. | `regression_w_t06_routes_and_json_errors`; existing custom protocol catalog/header and gateway fixtures. |
| W11 | Official Gemini /v1beta and /v1beta/openai saved roots both select native Gemini. | `regression_w_t06_routes_and_json_errors` checks protocol, endpoint and header. |
| W12 | First-party api.anthropic.com Messages budget is 128 MB, including history image projection; gateways keep 32 MB. Per-image base64 limits remain separate. | `host_derived_deepseek_replay_and_first_party_anthropic_budget` checks route budgets and 32 MB image-history projection at both hosts. |
| W13 | Provider or official DeepSeek host requires assistant reasoning_content, including custom/openai-compatible routes. Original origin identity stays unchanged. | `host_derived_deepseek_replay_and_first_party_anthropic_budget` asserts actual request with foreign assistant history. |
| W14 | Indexed reasoning details retain the first nonempty opaque signature, while text/data fragments still accumulate. | `repeated_signature_keeps_first_nonempty_and_invalid_argument_types_fail_closed` checks repeated signature and text accumulation. |
| W15 | Spec Go root now matches official /inference/go. | Exact spec diff; existing route regression retained. |
| N1 | One OAuth flow resolves/pins the resource's addresses and trust zone. Private hostname resources permit private OAuth endpoints; public resources reject private destinations. Same-host requests reuse pinned resource addresses. Refresh retains public/private grant provenance. | `private_hostname_resource_is_pinned_and_public_metadata_cannot_rebind` uses a resolver-seam address override and real HTTP to our loopback fixture under a hostname. Existing full OAuth/DCR/PKCE/refresh fixture retained. No real intranet service contacted. |
| N2 | Windows extensionless command names try PATHEXT only; explicit extensions remain literal. | Windows `pathext_cmd_programs_accept_literal_arguments_and_refuse_injection` now includes the real npm sibling layout npx+npx.cmd and must execute the cmd shim. |
| N3 | Both discover and cached-era initialize startup failures wait briefly for drained stderr and preserve the crash reason/log. Stderr drains through EOF, including unterminated final lines. Explicit Test discards cachedEra and reconnects; failed-start logs also use cached failure reasons. | `cached_era_startup_crash_retains_stderr_and_explicit_test_reprobes`; `failed_start_preserves_reason_and_log_without_call_replay`; existing silent-era reconnect budget fixture. |
| N4 | Settings exposes Reset saved connection secrets only for confirmed missing keys. Native reset archives ciphertext under a timestamp/UUID unreadable name, saves a fresh protected key, clears revocations and invalidates affected connection tests/auth/clients. Unrelated connection records stay unchanged; denied key-store access cannot reset or write plaintext. | `reset_missing_connection_key_archives_ciphertext_and_allows_new_secrets_only_in_fixture` calls real add/reset/set-secret IPC; key-store denied test compares unchanged ciphertext; real component explicit-reset test. Only temporary fixture profiles were reset. |
| N5 | Step-up requestedScopes is bookkeeping excluded from command_pin, restored on ordinary updates. Configured auth.scopes precedes requestedScopes, challenge and advertised scopes. | `insufficient_scope_is_recorded_once_and_successful_discovery_resets_status` now asserts requestedScopes, unchanged trust pin, no configured-scope mutation and no automatic effect replay. |
| N6 | Tool review shows title, full annotations including readOnlyHint/destructiveHint, and outputSchema on both sides. | Real AiExtensionsSection dark/white review test asserts both titles, changed hints and output schema. |
| N7 | Public-resource OAuth blocks CGNAT 100.64/10, benchmark/reserved/documentation/mapped/IPv6 special ranges as well as RFC1918/loopback/link-local. Private-resource policy still permits its trust zone. | Resolver-seam/loopback policy test includes tailnet, benchmark, reserved and IPv6 destinations, plus mixed public/private answers. |
| N8 | Changed-tools gating emits Review them in AI settings, separately from missing Test/pin failures. | Extension snapshot regression in the insufficient-scope/discovery fixture asserts the actual warning. |
| U1 | Renderer marks the owner deleting before any await, drains pending writes, suppresses completion appends and recovers unsaved turns if deletion fails. Native transactional tombstones reject late appends after delete/clear, including running sessions absent from SQLite. | Real SDK/component delete-done-overtakes-reply and delete-failure recovery tests; `active_delete_and_clear_cannot_resurrect_transcript_files` now asserts late chat:append refusal for both operations. |
| U2 | Extension-owned provider shows its owner and directs edits to AI Extensions. Credentials/provider protocol/removal are absent; model mutations disabled. Selection remains available. Owned model discovery does not persist fake edits. Regular custom providers retain editing. | Real Settings contributed-provider ownership test; existing native contribution filtering/disabled-owner tests; user-owned settings tests retained. |
| U3 | Debounce builds the latest rendered patch at flush, serializes writes, flushes the previous provider before switching and flushes pending edits on unmount. Save shortcuts capture current state; model discovery waits for writes and fences intervening edits. Initial hydration does not autosave. | Real Settings paste, latest typing, key/URL, provider switching, model selection, pending-close, no-hydration-write and rediscovery tests. |

### Primary documentation checked for this pass

- [OpenAI reasoning](https://developers.openai.com/api/docs/guides/reasoning): current stateless responses return encrypted reasoning by default and accept the legacy include. We retain include for gateway compatibility, with bounded unsupported-field fallback and replay validation.
- [Claude thinking](https://platform.claude.com/docs/en/build-with-claude/thinking): 4.5 manual budgets and4.6+ adaptive support differ; the global opt-in cannot ignore model capabilities.
- [Claude API overview](https://platform.claude.com/docs/en/api/overview): first-party Messages/Token Counting 128 MB request ceiling; earlier 32 MB claim is superseded.
- [OpenCode models](https://opencode.ai/v2/docs/console/models/), [Go](https://opencode.ai/v2/docs/console/go/), [inference authentication](https://opencode.ai/v2/docs/console/api/inference/): paid model default, separate Go catalog endpoint, Bearer gateways and free-tier403 restrictions.
- [Gemini API keys](https://aistudio.google.com/docs/api-key): x-goog-api-key for native Gemini keys.
- [IANA IPv4](https://www.iana.org/assignments/iana-ipv4-special-registry) and [IPv6](https://www.iana.org/assignments/iana-ipv6-special-registry): special-use address classification for the public-resource boundary.

No live credentials/model requests or private-service probes were used to confirm
these upstream facts. Full runtime closure remains explicitly pending.

## Runtime re-review at a28c564 — original findings

Claude Opus 5.5 completed the full runtime audit; the unchanged report is
`native-ai-opus-runtime-a28c564.md`. Original R1–R20 are closed on their reported
triggers; runtime closure is still blocked by these six newly found variants:

| ID | Priority | Outstanding behavior |
| --- | --- | --- |
| N-R1 | P2 | Send/retry preparation can launch a deleted conversation; native run dispatch does not check the persistent deletion marker. |
| N-R2 | P3 | A real native Settings window close bypasses the React-unmount autosave flush. |
| N-R3 | P3 | Bounded shell capture drops the tail and test failure summary. |
| N-R4 | P3 | A completed project-review request stays queued and repeated reloads scroll Settings. |
| N-R5 | P3 | Reopened non-vision image retries omit the previously available OCR text. |
| N-R6 | P3 | An open Settings view can overwrite a model choice made from the chat picker. |

The wire and standards a28c564 review attempts stopped at the Claude account's
five-hour limit before auditing. They provide no closure verdict. The same Opus
5.5 model on the second configured Claude instance completed the runtime audit;
that instance may be used for the remaining same-model reviews after repairs.


## Full runtime follow-up N-R1–N-R6 — implementation `8f70d2e`

The unchanged report [native-ai-opus-runtime-a28c564.md](native-ai-opus-runtime-a28c564.md)
contains 0 P0 / 0 P1 / 1 P2 / 5 P3. All six findings have implementation repairs
and permanent regressions in source `8f70d2e7cf7beac2129155402587b82fded9c35b`.
**These are repaired dispositions pending the parent's fresh Opus 5.5 closure;
they are not reviewer closure or release approval.** Starting HEAD was clean
`5271fd7` (documentation only over implementation `a28c564`). No original review
report was edited. The prior R1–R20 regressions remain in the full suites.

| ID | Repair | Permanent seam-level evidence |
| --- | --- | --- |
| N-R1 (P2) | Delete immediately invalidates `submission` and `preparing`; send/retry fence entry and each preparation await, including persisted user-turn completion. Failed deletion restores the owner for a fresh submission. Native launch holds one async session-lifecycle gate across persistent tombstone lookup, retry history writes, generation capture and registration; delete/clear/turn-delete hold that same gate across cancellation and SQLite/transcript deletion. No standard mutex crosses an await. Other launch callers carry no persistent session ID. | `agentChat/ownership.test.ts`: both converted send/retry preparation probes, failed-delete preparation recovery, in-flight append deletion, and the existing running-delete/done/failure cases. Native `deleted_sessions_refuse_send_and_retry_before_any_work_even_after_restart` refuses both send/retry after delete and clear with a newly created core; no run registration, provider request or transcript. `delete_or_clear_between_registration_and_first_image_save_cannot_resurrect` blocks the real parent OCR route before first image save, then deletes/clears; cancellation leaves no user/assistant files and clear also tombstones the registered session absent from SQLite. Existing `active_delete_and_clear_cannot_resurrect_transcript_files` still runs. |
| N-R2 (P3) | Rust prevents native Settings `CloseRequested` and emits `settings-close-requested`. All `close_current_window` / platform close routes request this same close. The real bridge awaits the component's debounce/serialized queue and immediate config IPC writes, including edits arriving while close waits, then invokes the label-checked `finish_settings_close`. Rust uses `destroy`, avoiding recursive close, and restores the launcher. Failed writes retain the renderer, draft and visible save error. Escape/back also drains provider saves. Closing invalidates pending discovery writes. | `SettingsView.knowledge.test.ts` mounts real Settings and uses the actual `initTauriBridge` event adapter: deferred write must finish before acknowledgement, a new edit during that wait must save too, and `{ok:false}` must prevent destruction and retain the key/error. Existing unmount/debounce/paste/provider-switch tests remain. Installed `@tauri-apps/api/window.js` `close`, `destroy`, `onCloseRequested` implementations were inspected: destroy bypasses close request; the JS handler awaits before default destruction. Existing `core:default` event permissions are sufficient; this narrow native acknowledgement adds no renderer window-destroy capability. No desktop/webview launch was used to infer durability. |
| N-R3 (P3) | Each stream keeps bounded head and tail state outside cancellable read futures, continues the same ring during timeout drain, and reports exact omitted bytes. UTF-8 cut boundaries are trimmed; invalid bytes cannot inflate decoding beyond the budget. 48 KiB stdout + 12 KiB stderr, including marker reserve, preserve the whole-result 64 KiB cap, status and timeout semantics. | `process::capture_tests::verbose_failed_runner_preserves_both_stream_heads_and_summary_tails`, `cancelled_read_future_retains_ring_for_timeout_drain_and_unicode`, `invalid_bytes_cannot_expand_past_the_capture_budget`; `native_tests::verbose_failed_shell_summary_reaches_actual_model_tool_result` runs through agent IPC, real shell approval/execution and captures the actual second provider body, including both heads, failure/error tails, exit 7 and omitted marker. Existing shell timeout and process-tree tests remain. |
| N-R4 (P3) | Every review invocation queues a fresh nonce. Lists retain the request for cross-window handoff and overlapping readers; they do not consume it. UI scrolls once per nonce and rejects stale reload completions. Trust, Keep disabled and removed-folder Dismiss clear only the matching request, so an old acknowledgement cannot consume a newer request. Viewing grants no trust or execution. | Expanded `project_review_handoff_survives_an_independent_settings_request`: repeated list keeps one nonce; dismiss clears; same-folder review gets a new nonce; stale dismiss preserves the newer request; successful trust clears it. `AiExtensionsSection.test.ts` repeatedly emits list-change events, confirms one scroll, resolves the panel, emits again without a scroll, reviews the same folder again, and trusts with the new nonce. Existing removed-folder error path remains. |
| N-R5 (P3) | Canonical current-user reuse happens before OCR and image-route validation. Disposable OCR is derived from that effective user's images, including image-less renderer requests on reopened retry. The OCR parent route is in process; the local platform helper remains macOS-only and cancellation drops its future. Neither OCR text nor a non-vision projection is persisted into canonical image history. | `nonvision_ocr_uses_canonical_images_on_send_and_reopened_retry_only_in_wire_view` drives real agent dispatch and loopback provider bodies with a valid PNG and fixture parent OCR route: first send, in-session retry and reopened retry contain the expected OCR; image-less new task and missing-blob retry do not. Every stored original image remains byte-identical before the explicit missing-blob fixture and no stored turn contains OCR. N-R1's blocked OCR test covers cancellation before image save. Existing canonical image/retry/effect-ledger tests remain. Windows fixtures inject OCR text and do not claim new Windows OCR support. |
| N-R6 (P3) | Ordinary Settings provider-data edits omit provider/model/selected/task selection fields and top-level legacy credential mirrors. Serialized writes merge provider records with current config and update only the edited provider. Only explicit model selection/activation changes selection, merging current task overrides; removing a model/provider applies a conditional current-selection fallback. No self-write reload discards local drafts. | `SettingsView.knowledge.test.ts`: external chat provider/model selection before an edit and during a pending API-key draft both survive the later key/URL write, which lands in the original provider and preserves the other provider's key. Conditional model-removal cases preserve an external model choice or choose a valid fallback. Existing U3 latest typing, ordered writes, paste, provider switch, model choice and close tests, extension-owned provider protection, and W4 rediscovery protocol/maxTokens retention all run in the full suite. |

Exact final platform counts, archive SHA-256, commands and retained logs are in
[the verification record](../NATIVE_AI_VERIFICATION.md). The required next gate is
the parent's fresh full Opus 5.5 runtime closure plus its pending wire/standards
audits against this stable source. No subagents, push, PR, Actions, release,
application install/launch, real credentials/profiles or paid/live model call was
used in this repair.

## Coordinator close-path follow-up — `ae2fe00`

While verifying N-R2, the coordinator traced `SettingsWindowApp` switching to
Permissions, which unmounts `SettingsView` and its native-close callback. The
prevented native close then had no consumer. Renderer-only source
`ae2fe004e61538a33d6b661b4c4712d186a14640` drains Settings saves before that
transition and registers the window owner's close acknowledgement while
Permissions is displayed. A failed save leaves Settings mounted, displays the
error on its Permissions tab and retains the provider draft. No Rust source or
native lifecycle script changed after the dual-platform-tested `8f70d2e`.

The real-App/real-bridge test `drains Settings drafts before Permissions and
closes through the real window bridge there` runs successful and failed deferred
saves. Full TypeScript suite: 918 passed, 9 skipped; typecheck and frontend build
passed. Exact native/renderer sources and logs are in the verification record.
This follow-up also remains pending fresh Opus 5.5 closure.

## Full Opus closure audits at `1623d47` — follow-up repairs pending

All three Claude Opus 5.5 audits completed after the account-limit reset. The
unchanged reports are `native-ai-opus-{wire,runtime,standards}-1623d47.md`.
They close the original P0/P1/P2 findings and previous repairs on their reported
triggers. Eighteen concrete P3 variants remain; the migration is not declared
complete. S1, S7 and S10 include unproven platform effects and require regression
probes rather than assumptions. Every other finding has call-path or fixture
evidence in its report.

| Scope | Pending IDs | Behavior |
| --- | --- | --- |
| Wire | N-W1 | Current-image validation must use the route's first-party Anthropic budget. |
| Wire | N-W2 | Null/blank continuation call fields must not overwrite established identities. |
| Wire | N-W3 | Gemini canonical replay must omit blank text and resulting empty turns. |
| Wire | N-W4 | Keyless custom Gemini must omit the key header. |
| Wire | N-W5 | Missing Ollama tool arguments must normalize consistently. |
| Extensions | S1 | Classify public NAT64 and mapped addresses by their embedded IPv4 trust zone. |
| Extensions | S2, S3 | Explicit Test must reconnect after a crash and retain the useful tail of failed-start logs. |
| Extensions | S4, S5 | Secret reset must explain all affected credentials and support recoverable Windows reset states. |
| Extensions | S6, S7 | Resolve Windows relative commands against connection cwd and preserve trailing-backslash arguments. |
| Extensions | S8 | Explicit Test must not interrupt an active call or silently reuse the wrong probe mode. |
| Extensions | S9 | A changed server identity must clear old step-up scopes. |
| Extensions | S10 | Bound stderr draining after process stop, including escaped descendants. |
| Runtime | C-R1 | Hide Settings before queued destruction so the launcher can be restored. |
| Runtime | C-R2 | Optional discovery failures must not poison closing; recover unacknowledged native close requests. |
| Runtime | C-R3 | Accumulate rapid model removals while preserving conditional selection ownership. |


## Repairs for the eighteen 1623d47 closure findings

Implementation dispositions below are **pending the parent's fresh targeted Claude
Opus 5.5 review of all eighteen repairs and touched-path regressions**. They do not
close the overall migration or authorize release. The three original completed
reports remain unchanged. Final committed-source/platform evidence is recorded in
`docs/NATIVE_AI_VERIFICATION.md` after validation.

| ID | Implemented repair | Permanent regression / evidence |
| --- | --- | --- |
| N-W1 | Current-user image validation uses `route.request_budget`. | `closure_current_images_use_anthropic_route_budget_before_persistence`: three actual PNG attachments, first-party 128 MB versus gateway 32 MB through `native_run_history`; a deliberately invalid generation stops at persistence, before any network call. |
| N-W2 | Blank/null continuation id/type/name leave established raw identities intact; conflicting nonempty fields and malformed terminal arguments still fail. | `closure_chat_blank_continuations_and_ollama_missing_arguments`, real loopback transport, accepted raw identities and rejected conflicts/array args. |
| N-W3 | Canonical Gemini omits blank text and empty content turns; raw exact-origin signed parts remain untouched. | `closure_gemini_keyless_headers_and_signature_only_replay`, real decoded signed part, exact-origin replay and foreign-model encoding. |
| N-W4 | Keyless custom Gemini omits x-goog-api-key; explicit headers still replace defaults. | Same loopback header regression; prior OpenCode/Antigravity Bearer regressions retained. |
| N-W5 | Ollama missing/null args use shared `argument_text`; scalar/array args fail. | Missing, null, scalar and array responses through actual NDJSON transport in the N-W2 regression. No tools execute. |
| S1 | Well-known public NAT64 /96 and mapped IPv4 inherit embedded IPv4 policy; RFC8215 local NAT64 /48 remains private. | Expanded `private_hostname_resource_is_pinned_and_public_metadata_cannot_rebind`: public translations reject loopback/private endpoints, private translations stay private, mixed DNS refused; original pinning/refresh coverage retained. |
| S2 | Explicit Test removes a crashed cached client and connects on its first attempt. | `closure_first_test_after_post_start_crash_reconnects`, compiled stdio server crashes on one call, next Test succeeds. |
| S3 | Short startup reason/status is separate from last 20 scrubbed failed-start stderr lines. | Expanded failed-start and cached-era regressions: >500-character traceback/preamble, final unterminated cause, secret scrubbing, bounded log response; stdout/stop drain has bounded completion. |
| S4 | Only OAuth resets set needsAuth; token/env/header reset explains re-entry. List surfaces missing extension-provider credentials and reset copy names both owners. | Expanded real reset IPC regression checks token re-entry and provider status; component reset copy regression; provider credentials remain in their own namespace. |
| S5 | Only DPAPI ERROR_INVALID_DATA / NTE_BAD_KEY_STATE are resettable permanent failures; ciphertext and existing protected key are archived explicitly, with rollback if replacement fails. Access-denied/transient failures stay refused. | `windows_permanent_dpapi_failure_archives_both_files_and_denied_replacement_rolls_back`: real wrong-entropy protected blob (with truncated-blob invalid-parameter refusal) and real sharing-denied replacement in a temp fixture. Original unavailable-key-store unchanged-ciphertext regression retained. Real Windows result required in verification record. |
| S6 | Relative multi-component commands and relative PATH entries resolve against effective connection cwd. | `relative_windows_exe_and_cmd_preserve_exact_reviewed_argv`: compiled owned argv recorder under a temp connection directory; explicit exe/cmd and PATHEXT bare commands. |
| S7 | Batch argv doubles trailing backslashes before closing CRT quotes while retaining metacharacter rejection. | Same real Windows compiled recorder through .cmd: spaces, empty arg, trailing backslashes, following arg; exact hex argv equality. Prior injection refusal and owned Job tree tests retained. |
| S8 | Explicit starts cannot join a cached-era normal start. New probe/catalog must succeed before swap; retired clients stop after active calls finish. Failed/cancelled probes are stopped; disable/change/remove/shutdown stop retired clients too. | `closure_test_does_not_join_normal_cached_era_start`, `closure_test_retires_client_after_long_call_and_disable_stops_retired`: independent re-probe, actual result exactly once, cancellation on disable, no call replay. Original immutable snapshot/pin review gates retained. |
| S9 | Execution/auth/server trust-pin changes clear requestedScopes; unrelated edits preserve it and configured auth.scopes stays intentional. | `closure_changed_server_clears_requested_scopes_only`, actual update IPC; existing step-up regression retained. |
| S10 | Stderr drains to EOF or 250 ms after stop, flushing the final partial line. | Unix `closure_escaped_stderr_pipe_does_not_hold_shutdown_tasks`: compiled fixture escapes the group with setsid, keeps stderr open, shutdown finishes <2 s with task list empty; RAII kills only the owned escaped PID. Windows Job behavior unchanged. |
| C-R1 | Hide Settings and restore launcher before queued destroy; invalidate stale native fallback generations. | Direct production ordering derived from pinned Tauri 2.11.2 queue semantics; no desktop launch permitted. Native compile/lifecycle gate required; real window visibility remains source-verified. |
| C-R2 | Optional discovery terminates its rejection chain. Window-owned drains retain retired queues/failures before set IPC starts. Default bridge close consumer acknowledges immediately and drains without React; native fallback only closes unacknowledged requests after 3 s, never acknowledged busy/error states. | Converted discovery probe; real bridge default-consumer/repeated-close test; real Settings remount tests cover queued get, save success/failure, visible retry, shared ordering and newer drafts superseding older failed saves without replay. Original Permissions, early-close, pending-write and manual-error tests retained. |
| C-R3 | Accumulate removals and derive conditional fallback from final model list, preserving external selection ownership. | Converted double-removal probe tests selected A then C and removal of interim fallback B; chat override asserted. Existing external-choice/provider-switch regressions retained. |

DPAPI codes were checked against current primary Microsoft documentation:
[CryptUnprotectData](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptunprotectdata)
and [Winerror security codes](https://learn.microsoft.com/en-us/windows/win32/com/com-error-codes-4).
Only the two explicitly classified permanent codes permit reset; corruption can
produce other codes, which remain refused rather than being guessed permanent.
