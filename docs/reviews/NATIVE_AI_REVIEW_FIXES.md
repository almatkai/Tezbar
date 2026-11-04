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

Implementation repairs are complete subject to final validation below and the parent's
mandatory **new** Opus review. This is not a release or review-closure approval.
Original wire/runtime/standards reports are unchanged.

Commit keys: A = `9b76b95`; B = `e672ecb`; C = provider/wire repair;
D = extension/lifecycle repair. Exact C/D hashes and final-source validation are
recorded in NATIVE_AI_VERIFICATION.md after the source commits. Every test below
uses fixture credentials and temporary profiles; none establishes live entitlements.

### Provider / wire report

| ID | Disposition | Exact regression / evidence | Commit |
| --- | --- | --- | --- |
| F1 | Fixed: no generic key fallback; empty Copilot credentials cannot reach catalog, exchange or inference. | `providers::tests::unsigned_copilot_never_borrows_generic_credentials_or_makes_requests` reproduced red before repair and passes green. | A |
| F2 | Fixed default Go root in code/spec to `/inference/go`. | `opencode_go_default_and_explicit_uncatalogued_protocol_are_native_routes` | C |
| F3 | Fixed empty/whitespace completed argument buffers to `{}`; malformed/incomplete blocks remain errors. | `wire::tests::empty_tool_json_null_reasoning_and_completed_empty_responses_are_valid`; `anthropic_terminal_does_not_complete_an_unclosed_tool_block_but_json_is_complete` | C |
| F4 | Fixed nullable reasoning deltas; DeepSeek same-origin replay supplies empty string for missing/null reasoning. | `empty_tool_json_null_reasoning_and_completed_empty_responses_are_valid` | C |
| F5 | Fixed image followups after the whole contiguous tool-result run, across canonical messages. | `chat_tool_results_stay_contiguous_before_image_followups_across_messages` | C |
| F6 | Fixed reasoning-details accumulation by indexed/identified fragment, retaining separate unindexed entries. | `empty_tool_json_null_reasoning_and_completed_empty_responses_are_valid` | C |
| F7 | Fixed decoder empty-final error classification to enable bounded AG retry before text. | `antigravity_empty_reply_retries_and_model_fallback_has_its_own_budget` | C |
| F8 | Fixed independent model/account/endpoint budgets; recognized missing-model errors try model fallback first. | `antigravity_model_fallback_is_reached_with_all_three_endpoints_configured`; `antigravity_quota_rotates_accounts_and_transport_errors_walk_endpoints` | C |
| F9 | Fixed default AG URL to remain implicit after Settings save, preserving all endpoints. | AG three-endpoint regression above also asserts a saved default URL stays implicit. | C |
| F10 | Fixed HTTP string error/message/detail extraction, bounded and redacted. | `provider_errors_keep_messages_retry_before_text_and_never_persist_opaque_secrets` | C |
| F11 | Fixed HTTP 529 and typed stream overload/server/rate-limit errors before visible text; provider messages retained and scrubbed. | `provider_errors_keep_messages_retry_before_text_and_never_persist_opaque_secrets`; existing visible-disconnect no-replay fixture. | C |
| F12 | Fixed official-host Chat token field to `max_completion_tokens`; other hosts retain compatibility defaults. | `official_openai_chat_uses_completion_token_limit_through_real_transport`; official Chat reference cited below. | C |
| F13 | Fixed custom status/catalog using the actual resolved protocol and credential headers. | `custom_catalog_status_uses_protocol_and_headers` covers Anthropic, Gemini, Ollama, Responses. | C |
| F14 | Fixed explicit per-model Protocol selector including uncatalogued OpenCode IDs; no name guessing. | Settings renderer test `does not borrow the previous provider key or URL when reloading AI settings`; `opencode_go_default_and_explicit_uncatalogued_protocol_are_native_routes`. | C |
| F15 | Evidence-backed dismissal of proposed header change: current gateway docs explicitly specify Bearer for all inference APIs including Gemini. Keep Bearer. | Official OpenCode Inference API Authentication + Gemini endpoint table below; explicit Gemini-route regression asserts Authorization. Live rejection was never proven. | C |
| F16 | Conservative fix: request encrypted reasoning only for enabled/capable reasoning routes; preserve all received encrypted items. | `responses_keeps_encrypted_reasoning_and_call_identity`; official Responses migration docs. Upstream rejection of unconditional include remains unproven. | C |
| F17 | Fixed empty completed output preserving already streamed done items/calls. | `empty_tool_json_null_reasoning_and_completed_empty_responses_are_valid` | C |
| F18 | Fixed family-specific call/result IDs, including identical parallel call names/args; Gemini does not gain synthetic IDs. | `gemini_family_ids_and_error_results_are_correlated_without_synthesized_gemini_ids` | C |
| F19 | Fixed missing/empty object arguments for Chat/Responses/Gemini without accepting non-object/malformed JSON. | `empty_tool_json_null_reasoning_and_completed_empty_responses_are_valid`; `duplicate_ids_and_non_object_arguments_fail_before_execution` | C |
| F20 | Fixed configurable bounded header wait: remote30/local120/max300; cancellation and single-turn hard deadline remain. | `wire::transport::repair_tests::local_header_wait_defaults_to_120_seconds_and_is_bounded` uses virtual time; existing actual header/cancel/effect fixtures. | C |
| F21 | Fixed `error:null` handling. | `empty_tool_json_null_reasoning_and_completed_empty_responses_are_valid` | C |
| F22 | Fixed recursive per-protocol image validation and request-size limits, from current docs rather than old 5 MB memory. | `anthropic_saved_empty_tool_text_is_omitted_and_base64_image_limit_is_current`; canonical history/image tests. | C |
| F23 | Fixed opaque credential retention by refusing unsafe opaque history, rather than rewriting signed/encrypted payloads. Generic scrubbing excludes cryptographic fields; exact known credentials still fail closed. | `provider_errors_keep_messages_retry_before_text_and_never_persist_opaque_secrets`; signed/encrypted replay fixtures. | C |
| F24 | Fixed Gemini fallback to the existing recommended model (`gemini-3.5-flash`), with vision metadata. | `config::selected`, provider fallback, current Google model docs below; final provider suite. | C |
| F25 | Fixed valid-token/missing-project path deriving the stable project before early return. | AG three-endpoint fallback fixture deliberately has no projectId and asserts nonempty request project. | C |
| F26 | Fixed Gemini error results, including empty failure text. | `gemini_family_ids_and_error_results_are_correlated_without_synthesized_gemini_ids` | C |
| F27 | Fixed image-only and assistant-first continuation bridges without altering canonical history. Deliberately retain Tezbar's own system identity; copying AG's identity prompt is not necessary for wire validity and could misrepresent the app. | `gemini_keeps_signed_part_boundaries_and_foreign_observations`; image encoder fixtures; base prompt explicitly identifies Tezbar. | C |
| F28 | Fixed credentialed provider redirects, including catalogs/custom headers. | `credentialed_catalog_and_inference_refuse_redirects` proves second origin receives no request. | A |
| F29 | Fixed empty custom Anthropic key header, thinking control, and eviction/re-exchange of rejected unexpired Copilot sessions. | `revoked_unexpired_copilot_session_is_exchanged_again_once`; custom protocol/catalog fixtures; actual Settings thinking checkbox. | C |
| F30 | Fixed incomplete stop reason validation before completed-tool JSON parsing. | `incomplete_tool_json_is_labelled_before_parsing` | C |

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
| R17 | Fixed true descriptor comparison, full before/after schema fields, removed tools and Keep disabled; native accepts only displayed pins/trust hash. | `pending_catalog_survives_refresh_edit_and_rejects_stale_review`; AiExtensionsSection dark/white review tests. | D |
| R18 | Fixed provider-aware image/request limits before new vision image persistence and on every history/tool request. | Same image-limit and model-switch regressions as F22/R3; `oversized_historical_tool_image_is_projected_without_poisoning_or_mutating_history` verifies canonical preservation and continuation. | C |
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
| P1-2 | Fixed exact command/argv/cwd, declared env/header names and masked secrets in preview/project/Details; execution-altering variables flagged before explicit Test. Default program cwd is explicit home/project folder rather than process cwd. Project contributions shown. | AiExtensionsSection preview/edit tests; project no-start-before-review fixture; argument/draft regression. | D |
| P1-3 | Fixed idempotent project revocation and change-only writes/events. | `changed_project_revocation_is_idempotent_and_has_one_event` repeats revocation five times after whitespace change. | D |
| P1-4 | Fixed globs, tilde-user, unsupported syntax, attached mutation options before remembered/Turbo; Windows literal backslashes supported. | `risky_shell_always_precedes_remembered_and_turbo`; Windows process and path suite. | B/D |
| P1-5 | Fixed raw multiline argument draft until save/Test. | Renderer `preserves typed multiline arguments, cleared timeout and an unsaved instructions draft across reloads`. | D |
| P2-1 | Fixed secretless connections avoiding key reads and Remove recording namespace revocation without decrypting/destroying unrelated ciphertext. No plaintext fallback. | `missing_secret_key_does_not_block_secretless_test_or_remove` compares unrelated ciphertext bytes; existing secret-store unavailable tests. | D |
| P2-2 | Fixed extension contributions persisting after autosave/model switch. | R7 regression. | A/C |
| P2-3 | Fixed unresolved profiles widening tools. | R12 regression. | B |
| P2-4 | Fixed approved versus pending catalog retention, edit bypass and stale-review acceptance; native bookkeeping cannot be overwritten by UI fields. | `pending_catalog_survives_refresh_edit_and_rejects_stale_review`; notification snapshot tests. | D |
| P2-5 | Fixed clearing connection error/needsAuth on successful discovery and OAuth persist; UI reads needsAuth. | `insufficient_scope_is_recorded_once_and_successful_discovery_resets_status`; OAuth full-flow fixture. | D |
| P2-6 | Fixed canonical URL storage/comparison, including root slash/case/default port/whitespace. | `metadata_cannot_redirect_remote_resources_to_private_services`; root native MCP catalog/status and OAuth fixture. | D |
| P2-7 | Fixed omit empty scope, challenge/metadata scope discovery and stored insufficient_scope step-up without auto-call replay. | `insufficient_scope_is_recorded_once_and_successful_discovery_resets_status`; OAuth PKCE cancel scenario asserts scope absent; challenge parser fixture. | D |
| P2-8 | Fixed startup-exit reason/scrubbed log retention and fresh HTTP404 versus expired session distinction. | `failed_start_preserves_reason_and_log_without_call_replay`; legacy expired-session fixture. | D |
| P2-9 | Fixed positive 1–1800 timeout validation and raw UI field text; cleared UI defaults at save, imported0 rejected. | `invalid_add_has_no_orphan_secrets_and_timeouts_are_positive_bounded`; multiline/timeout renderer regression. | D |
| P2-10 | Fixed trusted-identity legacy era cache; changed execution identity clears cache, silent probe skipped on cold reconnect. No tool replay. | `silent_legacy_era_is_cached_by_trusted_connection_for_cold_snapshot` asserts cold snapshot under4s; modern/legacy dispatch tests. | D |
| P2-11 | Fixed Protocol select. | R14/F14 tests. | B/C |
| P2-12 | Fixed optional MCP tool description to string/title fallback, never null. | Insufficient-scope fixture has descriptionless tool and asserts wire definition is string. | D |

Standards P3 bullets are numbered in original order:

| P3 item | Disposition / exact evidence |
| --- | --- |
| Standards P3-01 backoff | Successful starts clear failures. `unrelated_edit_does_not_cancel_a_disabled_connection_test_and_success_resets_backoff`. |
| Standards P3-02 Test cancellation | Change sweep only cancels actual enabled→disabled transitions; same regression interleaves an unrelated instruction save during a disabled connection's Test. Explicit Disable still cancels starts. |
| Standards P3-03 Lines | Scan cursor prevents rescanning a long partial line on every chunk; existing byte-framing tests plus complete MCP suites. |
| Standards P3-04 list_changed | Notifications no longer reject old immutable run calls/startup catalogs; next snapshot refreshes and withholds changed hashes. `catalog_notification_keeps_run_snapshot_immutable_and_requires_changed_tool_review`. |
| Standards P3-05 subscriptions/TTL | Subscription HTTP timeout uses its long listen lifetime; fallback uses bounded minimum returned ttlMs (default60s). Continuation fixture asserts ttlMs2000 persisted. |
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

- [Claude vision](https://platform.claude.com/docs/en/build-with-claude/vision): direct API10MB base64/image, standard endpoint32MB/request. Bedrock's old5MB value is not used for the direct adapter.
- [Gemini inline inputs](https://ai.google.dev/gemini-api/docs/file-input-methods): current inline payload100MB; [current models](https://aistudio.google.com/docs/models) confirms the selected recommended3.5Flash remains catalogued. The app retains its tighter8MiB/image cap.
- [OpenAI image requirements](https://developers.openai.com/api/docs/guides/images-vision?api-mode=chat): current aggregate payload512MB; app image budget is tighter.
- [OpenAI Chat reference](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create): max_completion_tokens replaces max_tokens for reasoning compatibility. [Responses migration](https://developers.openai.com/api/docs/guides/migrate-to-responses): stateless reasoning items are replayed; completed encrypted payloads are preserved.
- [OpenCode inference](https://dev.opencode.ai/v2/docs/console/api/inference) and [harnesses](https://dev.opencode.ai/v2/docs/console/harnesses/): Bearer authentication, native family paths, supported paid/Go access and explicit free-tier403 restrictions. No client impersonation or bypass introduced.

Provider-side rejection/acceptance, quota and entitlements remain external-only: no
credentialed live model request was used to validate them. Fixtures prove request
shaping, no-request/redirect ownership, bounded retry and effects/history behavior.
Fresh final macOS/Windows results are recorded separately. New Opus re-review is
**pending with the parent**, and is not marked complete here.
