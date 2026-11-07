# N-B1 parent-authored repair

The fresh b626f28 review closes R-1–R-6 for their original callers and D-1, but finds one Onboarding re-recording regression. Original report preserved in [OPUS_b626f28_CLOSURE.md](reviews/OPUS_b626f28_CLOSURE.md).

The parent prepared an exact source patch, without editing compiled inputs during the M8/M10 build owner's gate. Artifacts: `/tmp/tezbar-native-m7-nb1-parent/{manifest.json,tests.patch,fix.patch}`. Original artifact status was formatted/apply-checked only. The compile owner subsequently applied the exact patches between gates: RED 3 failed/1 passed, then GREEN 4/4; the parent independently reran all four successfully. See ../native-m08/repair-parent-verification.json and immutable repair-logs/nb1-{red,green}.log. Independent Opus acceptance remains pending.

Rejected saves now retain their top-level field names. A later same-owner Global edit removes only that field from older feedback. This applies both when an already-recorded rejection exists and when an older transport result arrives after the replacement was admitted. Unrelated fields and other owners keep their feedback; existing waiter capture, dropped-waiter ownership, and durable queue settlement remain unchanged. No second persistence owner, model fork, or new dependency.

Regressions include the independent review's real Onboarding handler and dropped-waiter probe, plus an already-settled rejection and another-owner boundary. The original dropped-barrier test changes a **different** field and must remain unchanged and passing. The older exact-revision regression and in-flight/queued Copilot follow-ups must pass too.

The compile owner applied tests first and recorded the expected failures, then applied the fix and verified Settings/Onboarding plus the final complete native and bench suites. The parent verified the exact resulting hashes and reran the four new cases. Original dropped-barrier and revision-order regressions remain unchanged and passing. Root will commit this checked source and obtain fresh Opus closure; no independent M7/full-migration acceptance is declared.
