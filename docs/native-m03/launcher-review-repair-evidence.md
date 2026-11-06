# M3A repair evidence and adapter migration

Selected tree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-gpui-app`, starting HEAD `33baae87be7df74cd198ed5df4e8ac9ae92826e5`. Changes are unstaged. No new branch/worktree, commits, install, manifest/lock changes, release actions, real profile, OS clipboard, live provider/MCP, installer or preview launch. M5 app paths and `tezbar-ui` were not edited by this lane. M3B controllers were not edited. XERJ was queried once with `ax-65b00e69da24`; its stale source results were not evidence. Exact current-source reads and `rg` drove implementation.

The immutable review's checksum is recorded in [dispositions](launcher-review-repairs.md). Original scratchpad review/probes were read without modification. The parent retains primary parity ledger ownership and fresh independent review responsibility. This work does not accept the full application or mark native-view parity.

## Actual validation

Raw log directory: `/tmp/tezbar-native-m03a-repair-evidence/`. Cargo commands used `CARGO_BUILD_JOBS=2`, `--locked --offline`; Rust 1.98.1 was pinned by the existing toolchain file, and 1.85.0 was explicitly selected for MSRV checks. No lock refresh or toolchain install.

| Gate | Actual result | Log |
| --- | --- | --- |
| Initial old-behavior core regressions | 3 failed: calculator JS edges, JSON/BOM, base64 trailing bits | `core-before.log` |
| Initial old-behavior notes regressions | 2 failed: structural legacy HTML, empty copy | `model-before.log` |
| Combined `launcher-models,agent-models`, Rust 1.98.1 | 361 passed, 0 ignored | `model-final.log` |
| Same combined suite, Rust 1.85.0 | 361 passed, 0 ignored | `model-msrv.log` |
| Core instant, Rust 1.98.1 | 93 passed, 0 ignored, 572 filtered | `core-instant-final.log` |
| Core instant, Rust 1.85.0 | 93 passed, 0 ignored, 572 filtered | `core-msrv-final.log` |
| Full core, once | **652 passed, 5 failed, 8 ignored** | `core-full.log` |
| Five failing full-core cases, each isolated with `--exact --test-threads=1` | All five passed without source changes | `core-recheck-1.log` … `core-recheck-5.log`, `core-rechecks.json` |
| Original mapped TS suite | 170 passed, 19 files, no skips | `ts-original.log` |
| TypeScript typecheck | passed (both tsconfig files) | `typecheck.log` |
| Original tracked TS test sources | all 82 byte-identical to starting HEAD | `unchanged-ts-test-hashes.json` |
| Detached Node/jsdom structural/number reference | 8 HTML fixtures and number/JSON probes completed | `node-jsdom-reference.log` |
| Scoped Rust formatting / whitespace / ownership hashes | see final audit record | `final-audit.json` |

The full-core failures were outside `instant`: native-next header deadline, packaged CLI fixture timeout, native extension idle shutdown, unselected download observation and aborted embedding-runtime shutdown. Their log assertions/timeouts are preserved. Serial isolated success supports scheduling sensitivity; it does **not** turn the failed parallel full run into a clean full-suite pass. No unrelated source was changed and no repeated full-core run was made. The parent can choose a coordinated common verification window.

The four parent regressions remain in the combined suite. Original session test bodies are retained; the earlier controller IME backend test now types two BMP characters (`雪山`) because one UTF-16 unit intentionally resolves locally under M4. This preserves its backend ownership assertions. None of the original 170 TS cases, six prototype cases, immutable data fixtures or licenses were weakened or replaced. The new HTML corpus lives separately in `notes/opus_html_fixtures.json` and uses detached jsdom nodes without script execution.

## Scaling observations

Debug builds on this Mac; timings are descriptive, never unit-test assertions. `opus_json_duplicate_order_large_objects_scaling` uses base-36 keys so the 100k-key case stays within the 1 MiB cap. The machine record captures the exact observations from the final instant log. Node v26.7.0 is a reference comparison, not a production dependency.

The final run was 7.36 ms / 25.46 ms / 45.43 ms / 108.89 ms for 5k / 20k / 40k / 100k keys. Node's same-input parse/stringify reference was approximately 0.94 / 3.74 / 6.92 / 22.80 ms. The original review's separate debug probe reported 179 ms / 2.5 s / 8 s for 5k / 20k / 40k decimal keys; those are historical observations from a different key corpus, not a controlled same-binary comparison. Source inspection establishes the removed quadratic duplicate scan; large regression round-trips establish completion without data loss.

The note regression measures 500 notes, 20 warm queries, and 1000 idle ticks with 32 open drafts. Its retained observation was 41.7 ms cold, 10.4 ms for all 20 warm queries and 1.53 ms for all 1000 ticks. This predates the final small revision-cache memory reduction and is labeled accordingly. Final suites rerun the assertions; no final wall-clock numbers are inferred. The original report's 10.9-second note scan was a separate historical debug observation. No app RSS/paint/platform performance claim follows from these microbenchmarks.

## Exact adapter migration

M5 received the interface changes before editing the controller and again after they landed. This section is authoritative for follow-on integration; it does not claim that another lane has completed these adapters.

### Launcher

`Launcher::update(Input, now_ms) -> Vec<Effect>` remains the entry point. New inputs:

- `Invalidate { text: Option<String> }`: forward the core event's text unchanged; do not invalidate caches in the view.
- `ArgumentValue(Option<String>)`: update on entering/editing/exiting an argument chip; this supplies the second valid invalidation text.
- `DirectoryResolved { ticket: u64, result: Result<String,String> }`: return the directory resolver's canonical directory or error. Do not open a chat directly from the resolver callback.
- `SelectTerminalSession(Option<String>)`: explicit user session selection; `None` means command/new terminal.

New effects:

- `ResolveDirectory { ticket, path }`: use the existing core directory resolver on Tokio; return exactly one owned `DirectoryResolved`, including transport/validation failure. Resolution may not navigate independently.
- `OpenAiChat { ticket, boot: Option<AiBoot> }`: open the AI surface and pass the boot to the existing AI controller; `None` means a blank new chat. `AiBoot::Submit { prompt, working_directory }` submits the prompt; `NewChat { working_directory }` opens the resolved directory chat. Boot serde uses existing `kind`, `newChat`/`submit`, `workingDirectory` wire names.
- `OpenTerminal { ticket, command: Option<String>, session_id: Option<String> }`: resume the session when `session_id` is present, otherwise open the terminal and run the command when present. Do not reinterpret this as generic search-row execution.
- `OpenNote { ticket, note_id: u64 }`: open Notes, select this `createdAt`, then acknowledge the ticket. Do not lose note identity by dispatching the row's generic open-notes command.

Every terminal open/AI boot/note navigation and ordinary Execute finishes with `ActivationComplete { ticket, result: Result<(),String> }`. Resolution feeds `DirectoryResolved` first and keeps the same activation ticket for the subsequent open. Host hide/focus loss feeds `Hide`; backend transport epoch feeds `BackendChanged`. Old reply tickets cannot dispatch or unlock newer work. No model code can cancel an OS effect already executed; the executor must consume each emitted effect once and obey view/task ownership when cancelling unexecuted work. Composition starts cancel queued Enter, not merely search text.

`list_selection_active` is false after editing and hides, true after a deliberate Move or successful Click, and cleared by refresh/invalidation/backend change so it cannot transfer to a replacement first row. Row movement from pointer hover needs a deliberate-selection input if used for history selection; do not write selection fields directly as a replacement for controller ownership. Pending cached previews remain non-authoritative. M4's completed one-unit Basic local list is explicitly authoritative for that generation. Use `display_rows()` and generation-bearing clicks.

`router::Effect::UpdateAction` remains the parent's single updater intent. Evaluate **current** status when executed: Ready → restart; Available → download; ManualAvailable → installer URL; busy/downloading → no-op; otherwise check. Do not dispatch based on which of the three command IDs produced it.

### Clipboard and snippets (M6)

`Clipboard::convert(ClipboardAction) -> Option<ClipboardEffect>` now emits `Transform { request:u64, id:String, action:ClipboardAction, text:String }`. Execute `instant::transform::apply_clipboard_action` on a blocking worker, then feed `conversion_complete(request, Result<String,String>)`. Do not run the pure transform in a GPUI event/render callback. This also keeps JSON/QR/hash work off the UI thread. A stale selection/request cannot receive the result.

Replace direct query/filter writes with `set_query(String)` / `set_filter(ClipboardFilter)` and execute their optional lazy-preview effect. Read via `query()` / `filter()`. `image_preview_loaded(request, Result<Option<String>,String>)` supports a null preview. `image_loaded(request, Result<String,String>)` remains a convenience wrapper. File entries use the same lazy-preview request. Retained budgets and negative caching are explicit; large source images may use a bounded host-generated thumbnail.

For snippet writes, use `Crud::write_completed(request, Result<String,String>)`. Successful create must carry core's actual new ID; successful update can return its existing ID. Owner and revision in `CrudEffect::Write` are opaque controller metadata. The generic `completed(request, Result<(),String>)` continues handling non-create operations; it rejects ID-less successful creates and retains their ownership for an ID-bearing completion. Do not fabricate an ID or call save a second time while the same owner is in flight. Core is still responsible for actual persistence and uniqueness.

### Notes and locale (M5/M6)

Handle `notes::Effect::Message(String)` as user feedback; empty copy sends “Note is empty” and does not touch the clipboard. Note cache revision is core's `updatedAt`; use normal loaded/saved completions, or advance that revision if intentionally replacing a public row directly.

For actual OS locale, implement core `instant::transform::PresentationFormatter`:

```rust
fn local_timestamp(&self, ms: i64) -> String;
fn relative(&self, relative: &RelativeTime) -> String;
fn count(&self, count: usize) -> String;
```

Use `parse_timestamp_data(input, now_ms) -> Result<Timestamp,String>`, `relative_time(target_ms, now_ms) -> RelativeTime`, and `clipboard_count(input) -> ClipboardCount`. `Timestamp` has `ms`, ISO text and structured relative unit/value; Count has code-point characters, words, lines and UTF-8 bytes. Format through `format_timestamp` / `format_clipboard_count` with the platform formatter. Unit selection is model-owned; local timezone and locale text/number shaping are formatter-owned. `parse_timestamp` and ordinary Count remain explicit English fallback conveniences, now using actual local timezone and corrected English numeric-auto wording/grouping. Currency retains its existing `AmountFormatter` seam. No JS engine is needed for locale.

## Native/manual gates

Actual GPUI effects and timing; focus/IME marked text and asynchronous hide; real terminal/AI/session/note selection; status-dependent updater executor; off-thread clipboard worker; OS locale number/relative/date output under non-English settings; real clipboard file/image preview/thumbnail and QR/save dialogs; Windows atomic replacement/sharing and ACL inheritance; rich selection/list/undo interaction; accessibility; full application parity and memory measurement; and fresh Opus closure. None was run or declared complete here. Every model repair and intentional bounded-language decision is recorded by original finding identity in the companion disposition table.
