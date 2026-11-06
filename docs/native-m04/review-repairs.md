# M4 foundation review repairs

The foundation is not accepted yet. Original review:
[Opus at 82688f7](../reviews/GPUI_M4_OPUS_82688f7.md), copied verbatim with SHA-256
`546c2a20e9f9d37cc7bcddd910a470835d6f637c00db187e78dd2f741d6b1273`.

| ID | Parent repair and current verification |
| --- | --- |
| M4-01 | Removed duplicate key-down activation; GPUI's focused keyboard click owns key-up. The actual GPUI regression sends down, held repeats and up for Enter and Space, then a disabled press. It failed before the fix (`/tmp/tezbar-native-m04-parent-button-before.log`) and passes in the full 30-test UI suite (`/tmp/tezbar-native-m04-parent-ui-after.log`). An earlier fixture compile error is preserved separately in `parent-button-fixture-compile.log`. |
| M4-05 | Sessions-panel geometry now uses the left edge minus 266 logical px; the existing geometry expectation is corrected. Transparent/no-shadow panel composition and final app tests remain for the foundation repair lane. |
| M4-13 | Parent removed the global Windows Alt+F4 quit binding from the live app; native per-window close routing should handle it. This edit shares the in-progress M5 app file and is not claimed as a tested frozen app yet. |
| M4-14 | Preview exclusion compares parsed shortcut identities, rejecting aliases and invalid syntax. Alias tests added. Explicit desktop-integration opt-in and final app validation remain for the repair lane. |
| Remaining IDs | Substantial foundation repairs pending; fresh Opus acceptance and manual gates remain open. |

Parent UI tests compiled with the default model features disabled. They do not
claim the concurrent M5 app or M3A repairs compiled, and do not establish physical
keyboard, VoiceOver, Windows or visual acceptance. Earlier foundation hashes and
fixtures remain checkpoint evidence, not evidence of the repaired source.
