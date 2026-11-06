# Parent repair of the 409dbf3 M7 closure review

This round implements R-1–R-6 and the URL/acronym copy correction. It is not independent acceptance, Windows acceptance, or full-migration completion. The original [Opus report](reviews/OPUS_409dbf3_CLOSURE.md) and earlier failed logs are preserved unchanged.

| ID | Implementation | Regression |
| --- | --- | --- |
| R-1 | Pending aliases prevent pump exit and global settlement; an owner waiter can restart an idle pump. | 50,000 two-owner alias/exit races, each checking durable bytes and both waits; existing 20,000 owner-drain races. |
| R-2 | Rejections are recorded per owner/revision. Captured waiter claims retain ordering even if newer waiters complete first, and dropping a waiter releases its claim. Rejection feedback triggers another owner flush and cannot clear queued follow-up actions. | In-flight shortcut rejection followed by sign-in; already-queued sign-in; dropped waiter; existing simultaneous onboarding shortcut/Finish regression. |
| R-3 | Apply the latest queued layout before refreshing Settings from the launcher. | Actual Owner + Settings composition checks the queued zoom and third-click input. TestPlatform rejects physical placement: this proves presentation ordering, not OS geometry or durable layout. |
| R-4 | Paint the confirmation overlay after errors and footer; reject underlying actions while it is open. | Real painted footer Close click emits no Close through the sheet. |
| R-5 | Durable failures exclude local validation; preserve its specific error, omit durable Retry, and notify on invalid early returns. Validation still blocks close until corrected. | Invalid numeric field, actual Retry visibility, unchanged error after Retry, existing invalid-draft close test. |
| R-6 | Both the failed-start branch and the already-decided ready branch deliver the retained Settings intent. | Actual Owner mounts Settings once across simulated start failure and ready recovery. The helper is driven directly; no real failed application startup or OS window is claimed. |
| D-1 copy | Known URL, API, ID, MCP, JSON and UI acronyms stay intact. | Production display-name cases. |

The first full run found two issues in this repair attempt: concurrent onboarding waiters claimed the wrong rejection, and a new fixture incorrectly required query retention when opening Settings intentionally hides/reset the launcher. The waiter ownership was fixed; the fixture now checks the requested Settings delivery and identity, without changing existing tests. The failed run remains `native-1.log`.

A pre-existing M10 WIP compile error used an unavailable `w` in `Launcher::navigate`. The parent changed only that call to the existing generation-fenced `defer_hide_terminal(cx)` helper. It remains M10 work in progress and is excluded from the M7 commit. M10 remains unaccepted.

Final gates: **305 native**, **306 bench-control**, check and normal debug build pass. Full UI: **56 passed, 1 failed**, the unrelated M10 alternate-screen test. The existing UI foundation subset passes **52/52**, with exactly five new M10 draft cases excluded. No full UI acceptance is claimed. Final commands, exact frozen inputs, failures and results are recorded in `closure-repair-receipt.json`. Validation uses the combined working source, including uncommitted M10 inputs; it must not be described as a clean-source build of the M7 commit. The UI and bench wrapper attempts terminated with SIGTERM and no test result; these are preserved and only successful reruns count. No unsupported tool timeout cause is asserted.

Settings-local RadioButton groups, pin tint, physical accessibility and hidden Copilot polling remain the prior non-blocking observations, not newly claimed fixes. Existing parity maps remain historical. No GUI, live provider, clipboard, production profile or Windows action was performed in this parent round.
