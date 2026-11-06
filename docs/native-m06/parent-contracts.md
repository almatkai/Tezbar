# Parent-owned M6 integration contracts

The utility implementation owns the new surface modules and rich-text editor.
The parent supplies these shared seams; this is not an M6 acceptance report.

`CoreClient` implements `UtilityTransport`. Its finite `Request::Utility` delegates
to `UtilityRequest::wire` and retains the existing 16-operation admission and core
task ownership. Read-only release HTTP shares that admission bound but awaits
inline, so cancelling the view drops the read instead of detaching it. The UI
crate explicitly enables the existing `launcher-models` dependency for its notes
editor, including standalone UI checks.

`tezbar-update::notes::http::fetch` accepts only the exact Tezbar tag URL and media
type produced by the existing release-note model. It uses the existing native
HTTP dependency, no redirects/retries, a 1 MiB response limit and a ten-second
bound covering headers and body. Its client is lazy and caches successful
initialization only. Nothing runs at startup. Five loopback/validation fixtures
plus the nine existing note-model cases pass on Rust 1.85 offline; logs are
`/tmp/tezbar-m6-parent-notes-http-msrv-final.log`. Live GitHub behavior is untested.

Notes selection metadata belongs to the canonical model history:

```rust
pub struct EditorSelection { pub start: usize, pub end: usize, pub reversed: bool }
pub struct UndoResult { pub changed: bool, pub selection: Option<EditorSelection> }
Notes::edit_with_selection(now, before, change) -> Result<(), String>
// change: FnOnce(&mut Document) -> Result<EditorSelection, String> (after selection)
Notes::undo_with_selection(redo, now) -> UndoResult
```

The offsets are opaque bytes in the native editor's visual projection. The input
adapter validates them against restored content. Before/after metadata lives on
the same bounded HTML `Patch`, so it is evicted and moved between undo/redo with
that patch. Failed and HTML-noop edits create no history; failed undo preserves
the pending patch. Existing `edit` and boolean `undo` remain compatibility
wrappers; legacy patches return `changed:true, selection:None`. The note's stored
HTML and autosave protocol are unchanged. No second view history or serializer
is introduced.

Three regressions cover exact styled/Unicode HTML and selection direction,
rollback/noop/legacy behavior and independent note owners. The combined model
suite passes 370 tests on Rust 1.85 offline, including the new M5 local-completion
test. Log: `/tmp/tezbar-m6-parent-selection-model-msrv.log`.

Actual Launcher navigation, visibility, host effects and durable-quit composition
are parent work still in progress. Locale uses the existing `NativeLocale`
through the supplied formatter setter. Visual/platform/IME/accessibility and
full-app RAM acceptance remain outstanding.
