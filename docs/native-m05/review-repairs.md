# Parent repairs of the 686d98e Opus review

The original [report](../reviews/GPUI_M45_OPUS_686d98e.md) is preserved verbatim,
SHA-256 `1be0dcfda29ba4754cefab8e56612c51207daf98e379ae0d3eeb38dae3cc0cd8`.
It closed M4's residuals and found eight M5 defects. The parent repaired them;
fresh independent review is required before accepting M5.

| ID | Repair | Permanent regression |
| --- | --- | --- |
| F-01 | Path-to-terminal conversion checks the actual input mode and double-space AI boundary, with no argument/pending form. | A real field types `/fixture  why is a > b`; prompt and AI mode remain intact. |
| F-02 | The command submits `Request::ConfirmQuit`; core owns the dialog and quit decision. The activation remains owned while it waits. | Existing note/quit fixture now checks the confirmation request and absence of a direct Quit event; a cancelled-quit regression retains usable query/rows. |
| F-03 | New model `ActivationHandled` releases only the matching local ticket while preserving the live session. Updates and confirmation completion use it; normal navigation/message completion keeps its existing reset contract. | Cancelled quit and update preserve input, update status refreshes, and a saved pin executes afterward. A model regression checks wrong-ticket rejection and preserved query/rows. |
| F-04 | Successful view results share one classification predicate; no-view notices are presented and failures become errors. Worker effects are never replayed. | Three real hotkey fixtures cover no-view, failed view-shaped result, and successful view handoff. |
| F-05 | The next pin mouse-down retires the previous drag's click suppression. | Actual reorder mouse gesture followed by a single click executes once. |
| F-06 | Each coalesced icon frame clears its flag and reads the latest viewport/rows. | Invalidation between scheduling and painting still requests the replacement generation's icons. |
| F-07 | Optional-only definitions execute immediately; inline or required definitions retain the form/validation path. | A real activation with two optional fields sends exactly one extension run and opens no form. |
| F-08 | Navigation uses an ordered, bounded inbox with per-surface consumption and explicit acknowledgement; updater actions use a separate typed inbox with monotonic nonzero nonces. Full inboxes report backpressure and preserve admitted work. | Three contracts cover multiple destinations, ordered same-surface consumption, capacity/acknowledgement and updater isolation/order. |

The inboxes admit at most 16 pending intents. Destination owners consume all
matching queued work on mount and acknowledge live-delivered work, preserving
other destinations. M6 composition and M7/M11 consumers must follow this
contract. An update action cannot appear as a Settings payload.

The case map now names key bindings explicitly, adds the eleven new native or
handoff fixtures, and marks source-only rows with no named fixture as partial.
Earlier v3 build/gate metadata is retained as historical evidence. Current
source hashes are taken from the indexed M5 repair snapshot; uncommitted M6
bridge and UI edits are excluded, including their changes in shared files.

Verification: the entire command-bar namespace passed **64 tests**, zero
failures/ignores (`/tmp/tezbar-m5-review-parent-commandbar.log`). That comprises
61 M5/contract cases and the three existing M4 durable-state fixtures. The
initial targeted review run passed 14 (the filter also selects existing preview
tests). Combined models pass **370** on Rust 1.85 offline and 1.98.1; that count
also includes the independent M6 note-selection seam tests. These runs used the
current integration tree while M6 remained active; they are not clean archive
builds. No baseline test or skip was weakened. The original review's probes
printed defect states; they were not failing assertion tests.

Non-gating observations remain assigned to their acceptance work: physical
Windows keycaps/AX/focus and icon blink to visual/platform QA; effective updater
status to M11; recent-extension ID import to M11; raw HTML ordered-list markers
and partial-stream list continuity to rich-content compatibility review. Other
formatting/duplication observations are recorded in the original report. No
fresh screenshot, live OS/provider effect, Windows, GPU or RAM claim is made.
