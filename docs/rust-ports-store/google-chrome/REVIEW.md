# Independent Chrome development-port review — 2026-11-20

Scope: frozen standalone source in `C:/Users/alikh/Tezbar-rust-ports/google-chrome`, its pinned upstream snapshot, reports and current GPUI/SDK source contracts. Read-only review; no builds, tests, profile access, browser automation, app edits or catalog edits were run. This document is the only review write. No blanket release or parity acceptance is granted.

Source changed during handback: after the preliminary P1 was sent to the parent, `src/chrome.js` and `src/main.rs` changed to capture/pass `tabId` and select a tab by that ID. The finding below describes the initially reviewed frozen source, not a claim that the repair still uses ordinal identity. Repair testing and native dictionary acceptance remain parent-owned. The existing `FINAL-STATUS.json` hashes now differ for those two files; the other 201 inventoried source files and recorded development ZIP still match. Existing 10/9 evidence belongs to the earlier freeze until rebuilt.

## Findings

### P1 — duplicate-URL tab shifts can close the wrong tab

`src/chrome.js:15` records window ID, ordinal index and URL, without the Chrome tab's stable ID. `src/chrome.js:51` checks the current bounds, `:52` selects the current tab at that ordinal, `:53` compares only its URL, and `:54` closes it.

Concrete trigger: render tab A at index 1 and tab B at index 2, both with the same URL. Close A directly in Chrome before invoking A's old **Close Tab** action in Tezbar. B shifts to index 1, passes the URL check and is closed. A's action ID is still the latest session revision because no extension refresh occurred. Reload/focus can similarly target a different same-URL tab. This contradicts the source comment and report's claim that changed tab positions cannot act on another tab. The upstream index-based implementation also has this weakness; the port's added URL comparison does not eliminate it.

Fix: capture Chrome's stable tab ID and resolve/check that ID inside the recorded window immediately before each effect. Reject a disappeared identity. Keep URL validation as an additional navigation-change policy if desired. Add an offline mocked-JXA contract test with duplicate URLs and insertion/removal/reordering, covering close, reload and focus. Mac runtime acceptance is still needed to verify the actual dictionary behavior.

### P2 — browser-only commands fail on unrelated Local State corruption

`src/main.rs:40`–`:53` loads and validates disk profiles before command-specific work. `data::profiles` fails on malformed Local State JSON or missing `profile.info_cache` (`src/data.rs:55`–`:59`). Therefore **Search Tabs** and **Search Windows** fail before their JXA enumeration when Chrome is running but Local State is temporarily truncated/malformed, even though those commands need no profile database. The pinned upstream `search-tab.tsx` and `search-windows.tsx` depend on browser automation only, without the profile dropdown/Local State dependency.

Fix: load profiles only for commands that consume profile data or expose profile-dependent opens. Browser-only enumeration should remain usable and report profile errors separately if those optional actions need them. Cover malformed Local State with a mocked successful tab/window enumeration; do not require a real profile.

### P2 — pagination promises more results after its hard display cap

`src/main.rs:192`–`:193` emits `hasMore = count > limit`, while `:406`–`:409` caps every load-more result at 10,000. With 10,001 matching bookmarks, once the limit is 10,000, every subsequent load-more produces the same rows and another `hasMore: true`. The 10,000 display bound itself is documented; the continuing pagination promise is incorrect.

Fix: stop pagination at the cap and expose the documented narrow-your-search limitation in the view. Test the render/load-more boundary with 10,000 and 10,001 synthetic items without filesystem or browser access.

## Evidence assessment

- `reports/cargo-test.log` contains a successful 10-test run, matching the ten test functions in source. The PowerShell `NativeCommandError` wrapper surrounds Cargo stderr but does not negate the final `10 passed; 0 failed` result. I did not rerun the suite.
- `reports/protocol-smoke.json` records nine successful checks, and `scripts/protocol-smoke.py` actually drives the executable JSONL protocol using temporary HOME/browser fixtures and a recording clipboard responder. It checks history ordering, include/exclude, stale and cross-session action IDs, clipboard payload, bookmark rendering/reset, fixture byte preservation, and macOS-only rejection for all eleven Windows-initialized commands.
- These nine checks are nine assertions/groups, not successful macOS execution of the eleven commands. The harness runs history/bookmark paths on a Windows executable initialized with the synthetic `macOS` platform. It never successfully executes the JXA tab/window/creation/name paths. Its all-command Windows loop establishes unsupported-platform errors only.
- `tests::stale_actions_cannot_target_replaced_rows` checks that render revisions increase; it does not execute a tab action or simulate Chrome-side identity changes. The protocol stale/cross-session tests are stronger for RPC session ownership but still do not cover the P1 browser-state race.
- The existing fixture DB is rollback-journal SQLite without live WAL/SHM. Its preservation check does not establish real Chrome WAL/locking behavior or all sidecar effects. `SQLITE_OPEN_READ_ONLY` is clear intent for database access; a live-WAL synthetic fixture and eventual Mac acceptance remain required before strengthening the no-write claim.

## Contracts inspected without a confirmed defect

All eleven original command IDs/modes are represented; manifest command declarations remain macOS-only. The three original preference definitions are retained. Seven upstream AI tools remain unregistered, explicitly documented rather than counted as command coverage.

The produced `List`, `List.Item`, accessory `List.Dropdown` and nested dropdown items match the current GPUI node shapes. Row `actionIds` refer to global callback actions. Current `tezbar-ui-model/src/extensions/runtime.rs:430`–`:443` explicitly admits the rendered accessory callback; GPUI dropdown dispatch supplies `formValues.value`, which the extension reads. This is source-contract alignment, not a keyboard/rendering acceptance result. The existing unit render test merely searches serialized strings.

SQL search terms use rusqlite positional bindings, so quoted query text does not become SQL syntax. Dynamic JXA input uses JSON serialization rather than concatenated script fragments. Chrome launch arguments use `/usr/bin/open` directly, validate the leading switch, and reject option-shaped/NUL URL arguments. I found no demonstrated SQL or JXA interpolation injection in these paths.

Default profile IDs reject separators, control characters and dot traversal. An explicit absolute `profilePath` intentionally allows an arbitrary user-selected profile location and is not a sandbox. Files/ancestors may be symlinks; the source does not claim race-resistant/no-follow containment. No whole-profile copy or migration scan was found. Do not reframe this as a capability sandbox.

The pinned SDK stages session state, fences cancellation/search ownership and drops the handler future when cancellation wins. `browser.rs` owns helpers with `kill_on_drop`, bounded concurrent stdout/stderr readers and ten-second timeouts; no detached port poller was found. Direct synchronous JSON/filesystem/SQLite work in `load` does not yield while running, so hard cancellation latency for large/slow profile files remains unverified. No post-cancellation browser side-effect guarantee is made: a launched external action may already have committed, and timeout messages accurately say its outcome can be uncertain.

Real Mac JXA dictionary execution, Automation/Accessibility behavior, Chrome WAL/locks, favicon extraction and GPUI interaction remain unverified. Those gaps are distinct from the reproducible source defects above. Repair P1 before destructive-tab shipping, address the P2 items, rebuild/package from the repaired freeze, then run the relevant offline contracts and native acceptance gates.
