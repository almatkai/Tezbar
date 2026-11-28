# Parent repair of Opus 290e414 review

Original review: [OPUS_290e414.md](reviews/OPUS_290e414.md), SHA-256
`ca8ea25857f1690c3c9c5321b0b1e5bb371bd7bd7b20b608a9f59000629c5e03`.
The original report and all failed runs are preserved. These are implementation
dispositions, not independent acceptance. Fresh Opus closure remains required.

| ID | Implementation and evidence |
| --- | --- |
| N-1 | `WriteQueue::reject` settles a definitive `ok:false` without stamping a successful write. It prevents older failed choices from replaying. Store rejection feedback belongs to the originating owner/revision and is consumed there; close/quit do not retain it. Finish/Skip drain their own owner, capture their barrier before spawning, and emit Finished once. Permanent real-Onboarding conflict probe plus concurrent rejection/Finish regression. |
| N-2 | Failed operational tickets complete with their own error and leave admission. They are never replayed by Retry saved changes and never enter the durable close barrier. Admitted operations still finish before close. Both original probes retained, including fifteen failures followed by a successful unrelated action. |
| N-3 | Settings flush captures an owner-scoped drain at admission. Its follow-up action and pending check use that owner, rather than all-owner operational activity. Original sign-in/toggle probes plus peer **config** failure and held unrelated operation regressions. All-owner `drain_with` remains on close/quit. |
| N-4 | Pump liveness/generation and final settlement use the same State mutex. A newer intent can start its pump immediately after the old exit decision. No try_lock/is_finished liveness check remains. The owned task outlives cancelled waiters; a generation-fenced lease reports panic/drop. Original 20,000-cycle stress passes; existing cancelled-waiter and close-drain regressions remain. |
| N-5 | Bare shortcuts accept exact F1–F24 only. Modifier combinations retain the existing path. Original F probe plus F0/F25/F01/non-function and valid boundary cases. |
| N-6 | Durable failure line derives from the store/parent persistence state and survives transient errors, refreshes and tab switches. Local validation/operation feedback has a separate line. Retry appears only on the durable line. Original probe asserts the persistent production error source. |
| N-7 | Shared native choice builder reads hydrated Theme, channel and Knowledge depth/speed, exposes RadioButton/Toggled state and selected fill. Hydrated-value fixture exercises the production selector. AX behaviour still needs physical testing. |
| N-8 | The tick task exists only while visible and Knowledge, sign-in or pending account polling needs it. Hide and idle AI dispose it; request completion and visible activation restart it when required. Retained onboarding can no longer keep hidden 4 Hz work alive. Actual entity hide/AI fixture. |
| N-9 | Parent State coalesces queued layout fields and supersedes old queued barrier tickets; a current successful save clears earlier failures. Reset plus later overrides retains default dimensions correctly. Production scheduling/barrier fixture exercises rapid width/scale and stale-failure settlement. The scale stepper advances its own displayed value before disk acknowledgement; a two-click GPUI fixture checks this. Actual OS resizing remains a manual gate. |
| N-10 | A failed core start marks the boot gate as decided/fail-open. Successful Retry loads channel data without hiding or re-running onboarding. Production fail-open state fixture plus source trace through start_core/m7_ready; no live OS core-start-failure probe is claimed. |
| D-1 | Settings uses product labels for folder, channel, speed and actions; voice downloads use formatted sizes and handle unknown totals. Details are readable labelled values rather than raw JSON. Execution-affecting environment/header names and values retain exact spelling. Target IDs remain in control identity, separate from labels. |
| D-2 | The 26-button zoom grid is replaced by a bounded scale stepper and current value. |
| D-3 | Focused modal sheet names the provider/extension/reset target, explains the effect and uses a danger action. Cancel retains default focus; Tab is trapped; the overlay occludes underlying controls. |
| D-4 | Transient validation/read/operation errors have no misleading Retry saved changes action. The durable line owns that action. |
| D-5 | Content is centred at max 610 px, or 850 px for AI. |
| D-6 | Shared native SVG pin marker ports the existing product path; the text glyph is removed. |
| Standards | Installed extension fields/subscriptions are prepared on Installed/Schema/Settings completion and removed when their command disappears. Render only reads them. Headers JSON is a masked single-line field. |
| M6 S-2 | Original handoff hashes/counts explicitly labelled historical; original values retained. M6 functional acceptance is the independent 290e414 review, not an upgraded historical receipt. |

## Test and experiment integrity

One prior assertion in `failed_setter_retry_cannot_replace_a_newer_successful_value_or_replay_auth`
expected an operational failure to block close. N-2 intentionally changes that
contract. Its own operation-result error assertion remains, closure now succeeds,
and a stronger assertion checks Retry sends no extra failed setter. Its newer
successful setter and no-auth-replay assertions remain. No test was removed,
skipped or ignored.

The first parent pump rewrite had a wait-loop mistake: every clean wake started a
new pump before checking settlement. A hung synthetic test process was terminated,
and the failed log is preserved (`review-tests-3.log`). The check now precedes
starting a pump. Earlier compile/test-visibility failures and the one intentional
old-contract failure remain in `/tmp/tezbar-native-m7-review-repairs/`.

Final results and exact build/source hashes belong in the companion verification
receipt. Tests use disposable files and synthetic/loopback transports. No real
clipboard, credentials, provider, extension server or desktop application was used.
No Windows, GUI, IME, VoiceOver/Narrator, GPU or RAM acceptance is inferred.
M8 composition/assets and M9 remain unexported drafts at this repair checkpoint;
M10–M11 and the full migration remain unfinished.
