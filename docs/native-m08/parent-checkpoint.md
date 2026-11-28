# Parent M8 checkpoint

The parent verified all 327 final frozen inputs, all 26 owned/coowned hashes,
all 18 handoff-document hashes and the existence/success records of all 18 final
gates. Protected M7 files match 409dbf3. The lock diff adds only the app resvg edge;
existing package versions/checksums are unchanged. All 27 standalone M8 tests remain
in both full native logs. This is a source checkpoint, not independent acceptance.

The final normal executable is preserved at
`/tmp/tezbar-native-m08-parent-evidence/tezbar-frozen-preview`, SHA-256
`f86c946aa4d674410fbd1b84481a47e46ce510715a01c5e7b616c3fd908ef544`.
It embeds 409dbf3 with dirty=true; the frozen source hashes prove its additional
inputs. It was not installed or launched. Final native254/bench255/UI52/model379
and the core/MSRV checks are scoped to this checkpoint, not later terminal work.
Original raw compiler/test logs are committed byte-for-byte, including their stdout
trailing blank lines; source whitespace checks exclude immutable raw logs only.

Independent Opus, Windows and manual platform/IME/AX/GPU/RAM acceptance remain open.
The early fixture Keychain-availability path is disclosed in the original verification
and receipt; corrected final fixtures script auxiliary channels. No stronger privacy
claim is made for the early development run.

## Parent finding to close before Windows acceptance

`crates/tezbar-core/src/assets.rs::read_scoped` pins descriptor traversal on Unix.
Its non-Unix branch checks path components and then opens a path without a pinned
ancestor/file identity. A junction/reparse-point replacement between check and open
can therefore redirect a Windows read. This is a source-traced race, not a reproduced
Windows probe. It needs a Windows handle-based scoped-open implementation and real
race tests before this capability is accepted on Windows. Do not downgrade the issue
to a mere unperformed GUI check, weaken canonical-root policy or claim the Unix test
proves Windows safety. The forthcoming independent asset review must include it.
