# Parent repair of the af1ccdd M4 closure

The independent [closure report](../reviews/GPUI_M4_OPUS_af1ccdd.md) found 21 of
23 original findings closed, with six local findings N-01–N-06 remaining. The
parent implemented the following repairs. Fresh independent closure is required;
this document does not accept M4 or the full migration.

| ID | Production change | Evidence and limits |
| --- | --- | --- |
| N-01 | Launcher state owns its own `state_save_error`. Every quit calls `drain_state(cx)`, which starts a fresh atomic save of current state before awaiting its tasks. Model saves/config reads cannot clear or set this durable owner's error. | Three real GPUI fixtures use the production state writer and disposable files: failed atomic replacement followed by recovery/current-state save; successful model save cannot mask failed state; model config read failure does not block a successful state drain. All three pass. No real profile or provider is used. |
| N-02 | Shared launcher footer consumes `bar_float`. | The footer regression inspects the actual `Div` background against the palette's float token. It also keeps the distinct `bar` assertion. |
| N-03 | Focused text-input wrapper now owns editable role, value, synthetic text runs, and selection; the painted `TextElement` has no editable accessibility role. Consecutive Markdown list items share one list container, with each row a list item. | Real field/password render fixtures check wrapper value/role and the painted child's absence of a role. Markdown grouping is shared with footnote scroll-row resolution, with a three-item-list/footnote regression. VoiceOver/Narrator behavior remains a manual gate; these are not screen-reader results. |
| N-04 | `tokens(window)` and the contrast test consume the same palette function. Tests composite real translucent fills over the real sheet/shell and check text1–3, semantic text, Settings links, and white button text in both themes. | The new test first failed on real existing colors, including secondary text on selected/hover/pill, light success/warning, and white on accent-hover. The corrected palette passes. The placeholder/disabled-only text4 exemption is explicit. |
| N-05 | Showing an already-visible launcher activates it without replanning position. Placement remains on hidden-to-visible transitions. | Source-level route check covers dialog close/restore and visible confirm. Physical multi-monitor focus remains unverified. |
| N-06 | Sessions panel should-close emits `PanelHide` and refuses destruction; a failed update clears its stale handle and recreates the panel. | Compiled native route. Actual Windows Alt+F4 remains a Windows GUI gate. |

## Intentional contrast corrections

The exact legacy colors and the specification's AA requirement conflicted. The
test now exposes that conflict instead of asserting unrelated literal pairs.
These are deliberate corrections to §5.2, retaining the existing hues:

| Token | Legacy | Corrected |
| --- | --- | --- |
| Light success text | `#059669` | `#047857` |
| Light warning text | `#d97706` | `#b45309` |
| Dark secondary text3 | `#868a96` | `#979ba7` |
| Accent-hover (both themes) | `#6a6df4` | `#6063e6` |

The checks include sheet, footer, panel, field, popover, selected, hover and pill
fills. They are unit checks of rendered colors, not visual or accessibility
acceptance of the desktop app.

## Verification at this checkpoint

- Actual compiled UI test binary: **39 passed, zero failed/ignored**.
- Three new launcher durable-save fixtures: **3 passed** in the integrated app
  test run. That non-frozen run had **80 passed and one M5 test failure**
  (`ai_history_does_not_replace_home_while_reopen_refresh_is_pending`), reported to
  the M5 owner. It is not an accepted full integration result.
- Scoped rustfmt and `git diff --check` passed.

Logs: `/tmp/tezbar-m4-closure-token-red-compiled.log` (expected contrast failures),
`/tmp/tezbar-m4-closure-current-tests2.log` (including the M5 failure), and
`/tmp/tezbar-m4-closure-ui-direct.log` (39 UI passes). The earlier failed command
using the nonexistent UI `runtime-shaders` feature and the macro-import compile
failure are retained, not counted as test runs.

The M5 owner is still editing its assigned command-bar files. A final integrated
run must freeze and verify hashes before and after. No new clean app build,
desktop fixture, Windows M4 run, GUI/IME/accessibility or RAM claim is made here.
No installed app, signing identity, production profile or release default changed.
