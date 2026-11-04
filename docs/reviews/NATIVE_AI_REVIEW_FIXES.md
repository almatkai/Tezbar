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

Repair in progress. Original reports remain unchanged. Closure still requires fresh
full validation and the parent's new Opus reviews.

| IDs | Disposition and regression evidence | Commit |
| --- | --- | --- |
| F1 | Removed generic-key fallback; catalog, exchange and inference refuse an empty Copilot token. `providers::tests::unsigned_copilot_never_borrows_generic_credentials_or_makes_requests` failed before the repair (status made requests), passes after repair at actual dispatch. | credential ownership repair |
| F28 | Shared provider client refuses redirects. `credentialed_catalog_and_inference_refuse_redirects` covers catalog and inference with a second loopback origin; no request reaches it. | credential ownership repair |
| R7, standards P2-2 | Native save filters contributed providers/config/model lists; merged view drops stale contributed entries; secret resolution rejects disabled ownership. Extended `declarative_provider_is_native_scoped_and_its_header_is_never_returned` saves the merged view, checks raw config and revokes the provider. | credential ownership repair |
| R19 | Settings uses the same legacy-only, same-active-provider fallback as native selection; unrelated top-level key/base no longer populates another provider. Typecheck passes; renderer regression remains scheduled. | credential ownership repair |
| Runtime P3 OpenCode CLI discovery decision | Removed optional `opencode models` execution; only static/native HTTP discovery remains. Existing native catalog regression remains scheduled in final suite. | credential ownership repair |

| R1 | Dedicated `ShellCapture` retains bounded stdout/stderr, exit code and timeout output; success-only `output()` callers unchanged. `shell_failure_stderr_success_and_timeout_keep_both_streams` tests real shell dispatch. | runtime ownership repair |
| R2, R3, R9, R11 | Explicit Retry reuses stored original user images and retains old tool calls/results and repaired unknown outcomes; canonical images survive OCR/model changes; recursive disposable wire projection and recursive text sizing; in-loop image bound. `retry_reuses_original_image_and_preserves_completed_effect_history`, `nested_images_are_excluded_from_text_context_and_projected_recursively`. | runtime ownership repair |
| R4, R5, R13, R20, standards P3 misleading Always | Mode changes do not resolve approvals; unrelated Enter has no global approval handler; permanent mode picker; native rejects Always for unrememberable tools; sensitive/complex/destructive/mutation shell commands cannot be remembered. `ownership.test.ts` pending/destructive/mode/keyboard tests; native review and policy tests. | runtime ownership repair |
| R6, standards P1-1 | Project Review queues canonical cwd in native state and emits a reload event; Settings lists queued project without browser storage and scrolls on receipt. `project_review_handoff_survives_an_independent_settings_request` covers initial and reused windows' independent requests. | runtime ownership repair |
| R8 | Empty tool output becomes `(no output)`; old Anthropic encoding regression still pending in wire repair. | runtime ownership repair |
| R10 | Store generations captured before launch reject saves after delete/clear; owned runs cancelled first. `active_delete_and_clear_cannot_resurrect_transcript_files` covers both through real dispatch. | runtime ownership repair |
| R12, standards P2-3 | Unresolved selected profiles fail closed before network. `missing_profile_fails_closed_before_provider_request`. | runtime ownership repair |
| R14, standards P2-11 | Custom-provider creation has persisted Protocol select; editing existing providers still pending. | runtime ownership repair |
| R15 | Removed Pi onboarding/runtime type comments. | runtime ownership repair |
| R16 | Composer Escape participates in App capture-phase escape consumer, closes menu/arguments first. Existing composer renderer tests pass; expanded capture integration coverage pending. | runtime ownership repair |
| standards P1-4 | Glob/tilde-user/option mutation guard precedes remembered/Turbo shortcuts; attached path values inspected. Policy regression matrix expanded. Windows backslash parser follow-up pending. | runtime ownership repair |
| Runtime P3 approval expiry, standards P3 linear Lines | Native expiry clears UI cards; `Lines` tracks scan cursor. Mode/expiry renderer regression passes. | runtime ownership repair |

Runtime subset: 86 Rust AI tests passed; 25 renderer tests passed; typecheck passed.
All other IDs and listed pending subparts remain pending; no completion claim is made.
