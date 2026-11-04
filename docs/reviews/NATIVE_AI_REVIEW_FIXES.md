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

Pending implementation. Record each review ID, repair or evidence-backed dismissal,
regression coverage and source commit here; keep the original reports unchanged.
