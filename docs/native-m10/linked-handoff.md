# M10 current linked integration

Supersedes the source-only status in the original M10 draft documents. The old
source freeze and first attempts remain historical evidence. The current owner
continued existing WIP, retaining root's generation-fenced defer_hide_terminal fix
and protected App failed-start M7 integration.

One retained TerminalView uses the existing CoreClient/runtime/CoreEvents and one
core-managed PTY writer. App mounts/subscribes before activation, routes typed host
effects and sessions panel actions, follows layout/theme, preserves grids on hide,
starts core keepalive via generation-fenced detach, persists pins through existing
UiState, and separates terminal font zoom from whole-interface zoom. Construction
and render do not create a terminal process. Sessions panel virtualizes all rows;
shortcut1..9 selects the corresponding first nine without truncating the catalog.

Native attach uses `core:terminal:stream-attach`, raw generation/positions/watermark,
opaque max32 replay cursors with60s idle expiry and explicit release. Reads are at
most64KiB and parse from byte0 through one retained Alacritty processor, yielding
between requests. Live gaps recover by durable cursor, not text overlap or a
whole-log String. Historical device replies are suppressed; OSC52 is disabled.
Disk append/identity/truncation/deletion failures are explicit. Live bytes remain
separate from the durable flag. Input is one base64 generation-fenced all-or-refuse
batch<=1MiB including bracketed-paste wrappers; X10 non-UTF8 bytes stay raw. Legacy
Tauri terminal routes/results remain supported. New routes are not added to the
extension/generic channel list. No new provider or runtime owner exists.

DECSET1049 copies the primary cursor into the alternate screen in Alacritty0.26.
The repaired VT regression asserts ALT at inherited column4, then explicitly CUPs
home and retains the original starts_with("ALT") assertion. Production grid does
not invent a cursor-home operation.

Quit preparation fences terminal input without disposal, preserving grid/session/
configure/preedit state if any save cancels quit. Only final chat teardown can
Cancel chat runs, after all unrelated barriers. The single App budget includes
save/drain and remaining core shutdown. Core shutdown retains independent ownership
if its waiter times out. See ../native-m08/repair-integration.md for shared details.

Actual final commands/results/source+binary hashes are in
../native-m08/repair-gates.json and repair-receipt.json. Test layers remain distinct:
production-state synthetic transport; GPUI TestPlatform integration; private real
PTY core fixture; and unperformed Windows/manual/IME/AX/GPU/RAM acceptance.
No complete parity row or full migration is accepted from these local tests.
Fresh parent Opus review remains pending. All source remains unstaged/uncommitted.
