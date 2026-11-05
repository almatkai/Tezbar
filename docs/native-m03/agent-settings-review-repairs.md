# M3B Opus findings and repair status

The controller foundation is **not accepted**. Original review, unchanged:
[GPUI_M3B_OPUS_a52240a.md](../reviews/GPUI_M3B_OPUS_a52240a.md).
The report covers controller correctness rather than GPUI view acceptance.

| ID | Finding | Current status |
| --- | --- | --- |
| F1 | Cumulative worker effect records would re-execute clipboard/open/reveal effects and repeat notices. | Repair required; no extension adapter may execute these records. |
| F2 | Display trimming removes identities required for AI context and image history. | Repair required; canonical context identities must survive display-payload trimming. |
| F3 | Parent removal fence silently drops fields after a failed provider creation; retries use key order rather than intent order. | Parent repaired with two failing-before regressions; fresh Opus confirmation pending. |
| F4 | Response settling during failed deletion is not persisted afterwards. | Repair required. |
| F5 | Assistant persistence cannot retain response provider/model/token metadata. | Repair required. |
| F6 | Connection edits can replace secret references with display placeholders. | Repair required. |
| F7 | Model discovery emits an outdated full model list. | Repair required. |
| F8 | Approval focus is not scoped to a visible conversation owner. | Repair required. |
| F9 | Retrying an older response can use later turns as prior context. | Repair required. |
| F10 | Image failures cannot recover; app-path resolution and Windows paths are incomplete; mappings overclaim. | Repair and honest mappings required. |
| F11 | Legacy terminal metadata fails strict deserialization. | Repair required. |
| F12 | Knowledge Cancel label and tab-switch mutation ownership diverge. | Repair required. |

The parent's F3 fix now distinguishes a known successful removal from simple
absence after a failed creation. Later fields can save while the definition remains
a recoverable failed intent. Retried intents run in original revision order.
Tests reproduce both lost fields and a definition-dependent writer rejecting the
wrong order before the fix. The earlier deleted-provider activation/recreation
regressions remain passing.

The full combined model suite passes **308/308** on Rust 1.98.1 after this repair.
Logs: `/tmp/tezbar-native-m03-parent-f3-before.log` and
`/tmp/tezbar-native-m03-parent-f3-after.log`. Rust 1.85, worker wire verification,
all remaining findings and fresh Opus closure remain for the repair wave.

The same Opus review accepted the parent M0–M2 repairs from source/tests/logs.
Windows installer execution and the intentionally stricter F7 update-feed
behavior remain explicit M11 rehearsal gates. No old review report was edited,
and no native UI parity row is closed here.
