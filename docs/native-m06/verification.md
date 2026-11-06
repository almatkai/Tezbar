# M6 utility surfaces — verification record

Selected tree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`,
branch `feat/native-gpui-app`. Requested parent was `f3234cdb`; parent integrated
M5/repair/selection/transport commits in this same tree while this lane worked.
No branch/worktree change, staging, commit, push, PR, merge, release, Actions,
server, desktop app launch, installed-app control, production profile, real clipboard,
credentials, Keychain/DPAPI operation, paid service or external HTTP probe was performed
by this lane. Fixtures use only synthetic transports and GPUI's test platform.

The user-requested bounded XERJ lookup used the selected tree prefix
`ax-65b00e69da24` after reading xerj-search. It returned no match. No indexing or
reindex loop ran; current files, `rg` and Git supplied the facts.

## Final executed gates

All Cargo commands used existing Rust 1.98.1, `--locked --offline`,
`CARGO_BUILD_JOBS=2`, and `--test-threads=2`. App selection includes `runtime-shaders`.
Raw evidence, compiler failures and the original failed composed run remain under
`/tmp/tezbar-native-m06-evidence/`.

| Gate | Actual result | Log |
| --- | --- | --- |
| Full native library, including parent composition and unchanged integration fixtures | **128 passed, 0 failed, 0 ignored** | `native-final.log` |
| Shared native UI, including original 43 tests and four new hook regressions | **47 passed, 0 failed, 0 ignored**; 0 doc tests | `ui-final.log` |
| Owned utility cases in the composed native run | **28 passed** (26 GPUI fixtures, 2 wire/structural tests), included above | `native-final.log` |
| Scoped formatting and whitespace | passed | `audit.json` |
| Original TS sources and icon data references | source hashes and actual case locations recorded; source files not edited | `source-case-map.json` |

The parent's canonical Notes selection model and release HTTP helper were separately
validated by the parent on Rust 1.85 and 1.98 (370 model cases; 14 release/transport
cases reported). This lane does not re-label those as its own executions. Core/model
MSRV and manifests/locks remain parent-owned. No unnecessary full core or TS rerun was
performed for Rust-only native changes; inherited source algorithm cases remain linked
to their M3 map. Individual native tests are not invented for every preexisting pure
transform algorithm.

## Preserved failures and repairs

Initial native compile attempts exposed a glob-import test macro recursion, GPUI
listener/role signatures, a private view field, and missing new event selection
metadata; the original logs remain. The `app-expanded-8.log` also captured the parent's
in-progress `Vec<WireEffect>` inference issue; parent repaired its file.

The first composed run with the four new parent integration fixtures had **126 pass /
1 fail**, `Utility drain stalled` (`native-composed-final.log`). The unchanged parent
fixture drove the executor without advancing a fake polling clock. The repair removed
drain polling: a single bounded completion Notify is registered with `enable()` before
the state check, request completion updates its canonical owner and wakes waiters,
and failed dirty notes retry only once per explicit drain. Subsequent complete native
runs pass all 128 tests. The failed report was not overwritten or counted as a pass.

## What the native fixtures prove

Actions run through actual GPUI utility entities, shared native TextInput/RichText,
production M3 controllers, and the finite mocked core wire. They assert effect counts,
wire envelopes, ownership and visible model/editor state. Real keystrokes cover rich
formatting, list continuation, Backspace/Delete paragraph joins, undo/redo, paste,
historical release selection and composition. A retired note's native input handler
cannot commit into the new note. Delayed create/save completions update their canonical
note without selecting it; late clipboard restore cannot hide a different surface.

Image fixtures use a generated one-pixel PNG and the canonical core object
`{ dataUrl, width, height, byteSize }`, lazy selection decode and hide cleanup. QR fixtures
assert native preview, one PNG clipboard write and one typed PNG export. Secret clipboard
rows/previews are concealed, transforms cannot consume them, and a Vault draft's history
entry is removed only after save success. Vault copy has unlocked, auth, cancellation
and error variants, with ID-only copy wire. No secret-bearing request Debug is exposed.

The source map deliberately separates workflow-family coverage from exact per-case
assertions and inherited M3 algorithm coverage. No shared ledger row is declared complete
from only a controller or structural-helper test. Parent composition fixtures additionally
exercise real Launcher navigation, Back, paste routing and durable quit error ownership.

## Remaining acceptance

Physical-only acceptance remains with the parent: Windows interactive build/GUI;
macOS/Windows native dialogs, file reveal, clipboard/paste focus restoration and actual
Keychain/DPAPI authorization; OS IME candidate positioning; VoiceOver/Narrator focus,
selection and announcements; light/dark/zoom/reduced-motion screenshots and critical
visual review; and whole-application RAM/performance. Synthetic IME and accessibility
regressions do not substitute for those runs. Fresh architecture/design review and the
parent's final parity-ledger reconciliation remain distinct acceptance gates.

The migration continues through M7–M11. M6 does not implement a new updater, monitor
backend, quicklink CRUD store, WebView, GPL editor or another async runtime.
