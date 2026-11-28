# Launcher visual repair after the first user preview

The visible GPUI preview exposed layout defects that controller tests had missed:
rows had content-sized selection backgrounds, unbounded descriptions, raw
`native-command` labels, generic command glyphs, and a permanent mode cheat sheet.

The repair follows the existing React launcher in `CommandBar.tsx`,
`launcherIcons.tsx`, `styles.css` and the native spec. It gives list rows the full
available width, puts title and subtitle in a bounded text lane, preserves a
trailing category tile, copies the existing static command glyphs and colours,
adds the existing header badge, and removes the extra cheat-sheet line. Native
app/file icons keep their existing bounded image owner. No WebView was added.

The new real-GPUI geometry test failed before the repair (278 px rather than
1902 px for the selected row) and passes after it. The command-bar namespace
passed 95 tests; shared UI passed 57. The command-bar run precedes the final
cosmetic selected-tag tint, removal of the now-unused glyph helper and formatting;
the normal debug build and live window observation cover the final source hashes.
An initial test compile and two incorrect UI feature invocations are preserved as
failures, not counted as passing gates. See `result.json` for exact commands.

The final private debug preview was launched with the same isolated profile and
verified matching worker resources. Its metadata is explicitly dirty; the binary
has no WebKit link. Computer Use confirmed the corrected live main window, then
Escape hid it and the separate `.command` restored it. Exactly one preview process
remains running. The installed beta and its profile were not changed.

This is a targeted visual repair. It does not accept the other native surfaces,
physical keyboard/IME/accessibility or Windows, and contains no RAM comparison.
The preview profile has no imported user history and disables desktop scanning;
its empty app/history state is not a migration result. Ordinary Alt+Space still
belongs to the installed beta. The temporary preview and reopen command paths are
recorded in `result.json`; they are local runtime artifacts, not release packages.
