# M4 foundation review repairs

All numbered original findings have a source repair disposition. **Fresh Opus acceptance remains pending**; this does not complete M5–M11 or establish lower RAM. Current dirty-source evidence and per-ID test filters/source hashes: [repair report](foundation-review-repair-evidence.md), [repair map](foundation-review-repair-evidence.json).

Original immutable [Opus report at 82688f7](../reviews/GPUI_M4_OPUS_82688f7.md), SHA-256 `546c2a20e9f9d37cc7bcddd910a470835d6f637c00db187e78dd2f741d6b1273`. Original `foundation.md`/`foundation-test-map.json` and their old source hashes/fixtures remain unchanged.

| ID | Current repair disposition and limits |
| --- | --- |
| M4-01 | Parent exactly-once GPUI button fix retained; full Down/repeat/Up/disabled regression passes. |
| M4-02 | Explicit channel owners, independent bounded queues, coalesced status/unowned drop; Tokio-safe bridge and two-worker flood/close tests pass. |
| M4-03 | Durable writes cancel quit on failure; core stop proceeds to exit at eight seconds; JoinError consumed; real native scenarios pass. |
| M4-04 | Visible Settings/launcher dialog anchor, prompt type/title mapping and lease focus restore repaired; physical sheet/focus gate pending. |
| M4-05 | Left-overhang geometry retained; transparent/no-shadow sessions composition repaired; visual gate pending. |
| M4-06 | Current display identity/scale/size/zoom snapshot repaired; cross-monitor fixture passes, physical DPI gate pending. |
| M4-07 | Settings-first launcher hide is window-only; native state/second-instance fixture passes, focus observation pending. |
| M4-08 | Owned release polling/manual Windows drag, blur suppression, deferred macOS snap and nonactivating work-area/AX-hidden overlay repaired; physical gate pending. |
| M4-09 | Global capture move/any-up M5 hooks inspected; native TextInput outside-hitbox selection/any-up regression passes. |
| M4-10 | One full settled parse restores reference links/footnotes/lists/details; original function red, repaired function green. |
| M4-11 | Visible-only bounded background syntax work, light/dark cache invalidation, lazy initialization repaired; visual/pressure gate pending. |
| M4-12 | Mac line/document/selection/delete bindings and forward word delete repaired; actual GPUI key regression passes; OS IME remains manual. |
| M4-13 | No global Alt+F4; per-window native close routing retained; Windows physical gate pending. |
| M4-14 | Explicit desktop opt-in defaults off, parsed identity guards retained, tray independent of hotkey failure; CLI/alias tests pass. |
| M4-15 | Windows nonactivating show repaired; Windows build/GUI gate pending. |
| M4-16 | Independent core start/stop/command ownership, guarded Retry, bounded request admissions and completion logs integrated; cancellation test passes. |
| M4-17 | Independent Settings fallback timer survives cancelled quit; existing close-handshake regressions pass. |
| M4-18 | 128 admitted leases, counted release path bypassing ordinary queue capacity, no unlimited reserve; release/admission regressions pass. |
| M4-19 | Bounded latest-snapshot writer survives/retries failures and reports layout errors; transient and real durable-failure fixtures pass. |
| M4-20 | Editable focused role, masked password value, grapheme AX offsets, disabled state, task/list/table semantics repaired; VoiceOver/Narrator unproven. |
| M4-21 | Markdown rem-equivalent typography/spacing/padding now scale with zoom; visual zoom gate pending. |
| M4-22 | Bar/float and Settings tokens, contrast, native reduced-motion read and macOS blur repaired; contrast/motion tests pass, visual gate pending. |
| M4-23 | All details paragraphs collapse together; native labelled footnote anchors render after settle; parser regressions pass. |

Final verification: **56 app + 37 UI passed**, **57 bench-control passed**; zero failed/ignored in final suites. Locked 1.98.1 package check and normal runtime-shaders build passed with `CARGO_BUILD_JOBS=2`; package-scoped format/diff checks passed. Only existing core warnings remain. Normal binary reports `dirty:true`, `bench_control:false` and has no WebKit or Tauri/Wry/Tao dependencies.

The actual isolated Cocoa termination fixture passes repeated native quit held for 0.4 seconds, second-instance activation, durable layout-save failure with visible recovery and retry, and held core cleanup exiting at the eight-second bound. Owned PIDs/endpoints are absent. Exact binary hash/timing/source metadata is in the raw receipt, never a clean-source publication claim.

Parent M4-01 red/green proof remains `/tmp/tezbar-native-m04-parent-button-before.log` and `/tmp/tezbar-native-m04-parent-ui-after.log`; the earlier fixture compile error remains separately preserved. This lane adds red/green Markdown/keymap and real two-worker bridge regressions under `/tmp/tezbar-native-m04-repair-evidence/`. Initial endpoint-cleanup and save-message failures, forced recovery where used, intermediate compile errors and concurrent M5 failures remain explicit failed logs. The original two failed native-termination experiments remain untouched.

Physical GUI/IME/focus/VoiceOver/Narrator, Windows, screenshots and real texture-pressure measurements remain manual/separate gates. No installed beta, production profile, clipboard, credential, privacy state, live provider or updater feed was changed. Parent owns exact clean-source verification and fresh Opus review.
