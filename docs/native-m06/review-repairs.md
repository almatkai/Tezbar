# M6 parent repair work after the c2a36b5 Opus review

Status: **in progress, not accepted**. The immutable original is
[GPUI_M6_OPUS_c2a36b5.md](../reviews/GPUI_M6_OPUS_c2a36b5.md), SHA-256
`c413347f35c0b6ac4a0a094b0f7484315fd835c8c5efd5459467fdb1d0a4cb49`.
Its M5 R-1/R-2/R-3 closure is accepted for the reviewed source. M6 fixes still
need the complete native regressions, final freeze and fresh independent review.

| Finding | Current work / remaining evidence |
| --- | --- |
| F-1, High | Projection now distinguishes placeholder BRs and interns paragraph ownership for each visual position. Insert/Enter target the owning block. New pure tests and the prior shared UI suite pass (50 total). Actual Opus native probes plus empty-block IME, second-paragraph start, Backspace and paragraph-join cases pass in the integrated fifteen-test repair target. |
| F-2 | TextInput emits the exact replaced byte range and inserted text, including the anchored composition range. RichText no longer guesses edits using a prefix/suffix diff. Plain/password delegation remains off. Forward Delete probe and a Backspace/boundary counterpart are permanent; native target passes. |
| F-3 | Durable Vault Save completion retains the original clipboard source and token independently of the concealed view. Successful cleanup runs once; only presentation is session-fenced. Native hide-before-completion probe passes. |
| F-4 | Unsubmitted snippet drafts follow old unmount semantics on hide, and neither local validation nor an unsaved draft vetoes quit. Admitted jobs still drain. Native invalid-draft probe passes. |
| F-5 | Build script stages exact-version Markdown (or the explicit release-note path) into OUT_DIR. The native reader embeds that file. Nonempty/exact-version native build assertion passes. |
| F-6 | Pure model now formats bytes, rates, uptime, missing battery and named stats. Native view caches presentation on snapshot completion and renders percentages with meters. Model formatting test passes as part of 375; native cached row/meter assertions pass. |
| F-7 | One shared footer entity owns visibility, update status, identity-bearing actions, single-flight requests and 1s active/10s idle polls; hidden/unmounted indicators stop polling. Utility-sheet poll/banner removed. Both footer GPUI fixtures pass; M8 receives the same entity via its explicit injection seam. |
| F-8 | Fresh Release Notes navigation resets the selected version; hide/show retains it. Original probe permanent; native target passes. |
| F-9 | Native note rows now render cached title, summary and locale date. Portable date validation avoids invalid timestamps reaching the platform formatter. Native title/summary/date assertions pass. |
| F-10 | Assigned to the active M7 composition: pass tab/intent directly to Settings, preserve the originating launcher destination, and test Indexing → Knowledge Settings → return. |
| S-1 | Vault/snippet field subscriptions now use their disposable field collection. Twelve repeated native form mount/cancel cycles preserve the permanent subscription count and release field subscriptions. |
| S-2 | Closed: the original source-case map and parent-composition document explicitly label their hashes, fixture counts and logs as historical handoff evidence. Original values remain intact. Fresh M7 review independently accepted every M6 functional repair at 290e414; see `../native-m07/reviews/OPUS_290e414.md`. |
| Other design/standards notes | Standard base64 replaces the hand-coded codec; invalid/oversized previews show an error, duplicated admission checks removed, Vault kinds use product labels, pins trail row text, Clear uses specific destructive copy. The existing broad utility action rows remain a documented visual-quality limitation for the independent review; no screenshot/physical acceptance is claimed. |

Current parent logs: `/tmp/tezbar-m6-parent-editor-ui-2.log` (**50 UI passed**) and
`/tmp/tezbar-m6-parent-models-2.log` (**375 models passed**). The latter also
includes the independent M7 owner-failure accessor regression. These are not a
frozen whole-app pass or new Windows/visual/IME/AX/RAM evidence. No baseline tests
were removed or ignored. M7–M11 and the full migration remain active.


Latest parent targeted integrated logs: `/tmp/tezbar-m6-parent-native-probes-4.log` (**15 passed**), `/tmp/tezbar-m6-parent-task-footer-2.log` (**2 passed**) and `/tmp/tezbar-m6-m8-parent-shared-ui-1.log` (**51 passed**). Earlier failed fixture/debug-selector/compile attempts remain preserved. These targets are not a full frozen application acceptance; M7 now owns final shared-source native/bench/check/build validation before the next immutable review. Core and shared Markdown M8 seams are checkpointed separately in `ba3a227`.


The 290e414 Opus review accepted F-1–F-10 and S-1 at their real callers, with five additional passing editor edge probes. Its pristine reruns were 187 native and 51 shared UI tests. Those figures describe that review pin, not later M7 repairs. Current parent utility-composition source SHA-256: `f616ecaddf2f91bdd80c8de8a4bf3cc59246ad63a91768af9a1689f1a80e88ca` (five fixtures).
