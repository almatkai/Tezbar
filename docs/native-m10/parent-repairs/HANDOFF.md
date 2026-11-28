# Root-authored M10 repair patch (not a passing gate)

Source remains in a patch while M9 owns the shared compiled inputs. Base db4d0df, selected branch feat/native-gpui-app. The 15-file before/after manifest and unified patch preserve the original input snapshots. Do not overwrite co-owned files with these snapshots; apply only unified hunks.

The report is /tmp/tezbar-gpui-m10-opus-b739b4e.md. F1-F8 have root-authored changes and regression cases in the patch. Also restored sessions panel cwd, active state, running count, real New Session shortcut and hints/window title, and finite mailto opening. The three old replay tests now deliver positioned events after attach and before replay, then verify duplicate suppression.

F6 adds an additive core stream origin offset, not a guessed watermark. Only the first attach to this view's own Create generation replies from origin; a reattach or another owner suppresses every pre-watermark reply. Actual Windows startup/ConPTY remains required.

F8 buffers at most 1 MiB in 60 chunks, keeps event order/atomic paste boundaries, clears on generation retirement, and refuses overload visibly. Protocol replies bypass this buffer to prevent replay/device-query deadlock.

Default Windows actions are Ctrl+Shift+K/X/P/D, preserving Ctrl bytes for shell. A user preference question is pending; root explicitly stated this recommended default while continuing independent work. Native physical keyboard acceptance is not claimed.

Validation so far: scoped rustfmt parse/format; patch dry-run against exact before snapshots. NO Cargo, native test or application build has run for this patch. Direct rustc metadata attempts failed because cached rlibs mixed Rust 1.85/1.98 or distinct serde_core graphs; logs preserved. Those failures are not product regressions or passing tests. No shared source was touched by root during M9 ownership.

M9 has the authorized patch application and combined test gate. It must report errors back for root correction, preserve its own changes in App/events/command_bar, and include focused terminal/controller/panel plus core stream filters before final full native/bench/UI/MSRV gates. Fresh Opus, Windows, real GUI/IME/AX/GPU/RAM and full migration remain pending.

## Compiler correction v2

M9's first compiled root-patch filter found a duplicate Launcher method, an i32-to-char cast in a regression, and an incorrect Alacritty constant. Original failure is preserved in root-compile-blockers.log and M9 gates/17-root-terminal.log. Root authored compile-corrections-v2.patch (SHA-256 3a5de7d82be74ed60393646914b3d42d41b9be1f37c5e096ec07c04d7fd6eff1), four-file before/after hashes in compile-corrections-v2.json. Only the new delta method becomes apply_terminal_pin_change; the existing selected-row terminal_pin(cx) stays intact. The byte array explicitly infers u8, and mouse mode uses the locked MOUSE_REPORT_CLICK constant. Dry-run passes; no root shared-source edit or Cargo run occurred. Correction was delivered through the pending technical request. Passing compilation/tests remain to be reported by the integration gate.

## Bench decoder correction v3

The frozen bench-control build exposed missing SessionRow Deserialize after the typed panel change. Root authored bench-wire-v3.patch (SHA-256 a5f277ddb0c88d2d51c79ec33419e0813b4e6b35a3008b5f13fea9dff0a52aba), two-file manifest bench-wire-v3.json. It preserves existing tuple-shaped fixture input and accepts typed row objects with defaults for optional metadata. A bench-only regression exercises the actual decoder and malformed input. Original failure copied to root-bench-wire-compile-failure.log; original M9 frozen-03-bench-native.log remains intact. Dry-run/rustfmt parse passed, no root shared edits or Cargo; compiled result is pending M9 gate.
