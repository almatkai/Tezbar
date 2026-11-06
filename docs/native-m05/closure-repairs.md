# Parent closure repairs after the 35dca9b Opus audit

The independent [audit](../reviews/GPUI_M5_OPUS_35dca9b.md) closed all eight
original defects and reproduced three repair regressions. Their new fixes still
require independent closure; M5 is not accepted merely because tests pass.

| ID | Fix | Permanent evidence |
| --- | --- | --- |
| R-1 | The core's `activate-deep-search` row finishes its local activation with `ActivationHandled`, retaining the visible model. | Real Enter on the row, then Alt+1, executes the persisted pin once. |
| R-2 | Quit completion always releases its matching ticket. Only error presentation is fenced by the old search generation. | A held confirmation overlaps invalidation; both Cancel and a retired error release the action, keep the current message, and permit the next pin. |
| R-3 | Delivery acknowledges each intent once the retained Launcher has it; deferred utility delivery keeps its own clone. Return-home and hide also acknowledge the current intent. Command returns never enter the admission queue. Identical outstanding updater actions share their original nonce; distinct admitted actions stay ordered. | 64 successive Settings/AI/Terminal/extension/Command deliveries do not exhaust admission. Another test repeats pending update actions 64 times and checks original order, nonzero identities and fresh admission after consumption. |

The inbox is a delivery queue, not a history store. Future Settings, chat, extension
and terminal child owners receive each live intent directly from the retained
parent; they must not depend on an already delivered intent remaining in the
global inbox. Update execution remains M11; coalescing an identical outstanding
request does not execute or discard any distinct admitted action.

The same integration read found that Launcher hide cleared the current surface,
contrary to the legacy `window-shown { resetUi:false }` contract. Hide now retains
launcher destinations while retiring the separate Settings launch intent. Showing
a retained utility resumes its existing owner and native focus, without starting
a command-bar search behind it. A fifth parent M6 fixture hides/reopens a selected
note and types into the same canonical editor. Its first failed run is retained
at `/tmp/tezbar-m5-m6-parent-final-native.log` (132 passed, one failed); the complete
corrected native run passes **133**, zero failures/ignored, in
`/tmp/tezbar-m5-m6-parent-final-native-2.log`.
The complete bench-control run passes **134**, zero failures/ignored, in
`/tmp/tezbar-m5-m6-parent-final-bench.log`. Package formatting and whitespace checks
also pass. Shared UI sources are unchanged from M6's 47-test final pass.

The M5 map now has 65 named cases. Current source and reference hashes are refreshed,
including the stale controller reference identified by Opus. Every tested row and
test cites the current full-native log; previous logs remain labelled historical.
This is an integration-tree run, not an independent clean-archive run. Physical
platform, visual, accessibility, GPU/RAM and full migration acceptance remain open.
