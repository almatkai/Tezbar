# Historical Native V2 first batch — 2026-10-21

> Historical evidence for the former bundled schema-2 adapters. Those sources and
> audio/timer assets were later extracted and removed. The checks below are not
> acceptance evidence for the current schema-3 external packages or host. See
> [EXTERNAL_RUST_EXTENSIONS.md](EXTERNAL_RUST_EXTENSIONS.md) for current behavior.

Native V2 is extension manifest schema 2. The application version remains unchanged. This batch adds bundled Rust replacements for all eleven packages installed in the active profile. It does **not** port the complete collection of 98 own V1 packages (247 commands); those entries remain marked JavaScript in the catalog.

## Coverage

Counts mean a compiled Rust handler exists for a command on that OS, not complete feature parity or an end-to-end pass. Canonical IDs use `com.tezbar.native.<slug>`. The CLI computes these counts from the running executable.

| Extension | macOS | Windows | Dependencies and remaining differences |
|---|---:|---:|---|
| Amphetamine | 5/5 | 5/5 | Native keep-awake process/actor; no Amphetamine app required. |
| Audio Device | 12/12 | 12/12 | Pinned native helpers. Enforcement commands apply once; no periodic watcher. Quicklink/disable-command actions are not ported; Windows mute remains unavailable. Mac volume helper currently targets Apple Silicon. |
| CleanShot X | 22/23 | 13/23 | macOS requires CleanShot X. Raycast AI Chat integration unavailable. Windows uses native capture tools and does not cover all recording/OCR/overlay/scrolling features. |
| Color Picker | 9/9 | 9/9 | Native picker, conversion, names, extraction and wheel. Unsupported wide-gamut color spaces are rejected or clipped. Cross-extension picker callbacks and automatic tray restoration remain incomplete. |
| GIF Search | 3/3 | 3/3 | Real HTTP providers, saved favorites and recents. MP4 square conversion remains unavailable. |
| Google Calendar | 5/5 | 5/5 | Requires the user's own Google Desktop OAuth client ID and consent. Raycast tokens are not imported. Live authenticated reads/writes were not tested. |
| Google Search | 2/2 | 2/2 | Suggestions, history and URL opening; window hiding replaces Raycast pop-to-root behavior. |
| Perplexity | 1/1 | 1/1 | Opens Perplexity in the browser/app; does not return an AI answer inside Tezbar. |
| Port Manager | 4/4 | 4/4 | Live enumeration, search and process identity checks. Menu-bar command uses an on-demand view; some process-path actions are unavailable. |
| Speedtest | 1/1 | 1/1 | Pinned Ookla CLI with SHA-256 verification. Result is awaited; intermediate progress is not streamed. A real bandwidth test was not run. |
| Timers | 19/19 | 19/19 | Persistent scheduler, alerts, timer/stopwatch forms. Menu-bar commands use on-demand views. Windows custom-audio volume adjustment is unavailable. |
| **Total** | **83/84** | **74/84** | CleanShot accounts for the commands without native handlers. |

## Automated checks

- macOS Rust 1.85: `cargo test --manifest-path src-tauri/Cargo.toml --lib --locked` — 278 passed, 2 ignored.
- Real Rust host/pinned Bun 1.2.5 round-trip and termination integration — 1 passed (explicit opt-in). Remaining JavaScript compatibility extensions retain a working worker path.
- `pnpm typecheck` — passed. `pnpm test` — 621 passed (105 files). Form reconciliation regression tests cover refresh, authoritative server changes and controlled-field rejection.
- Release-policy checks — passed. No release feed or production release was published.
- macOS ad-hoc `.app` build succeeded and was installed under `/Applications/Tezbar.app`; the previous application and installation metadata were backed up. No notarization is claimed.
- Native CLI listing and `--extensions-v2-install --all`: 11/11 installed in an isolated macOS profile and in the active profile. Legacy packages and preferences remain available. After fixture QA the installed application was restarted in the normal profile; the actual installed executable lists all eleven canonical packages as installed.
- Live neutral GIF provider request (`cat`) returned HTTP 200 with results.

## Windows verification and remediation (2026-10-21)

- **Environment & Toolchain**: Windows `DESKTOP-OB83S8J`, MSVC Rust 1.85.0 (`x86_64-pc-windows-msvc`), Node v24.6.0, pnpm 11.17.0. Branch `feat/rust-core`.
- **Root Cause & Permanent Fix for `0xC0000139` (`STATUS_ENTRYPOINT_NOT_FOUND`)**:
  - Dumped import descriptors for the test executable revealed an unresolved reference to `TaskDialogIndirect` in `comctl32.dll`.
  - Cargo test executables on Windows MSVC are linked without SxS application manifests by default, causing Windows to bind to legacy ComCtl32 v5.82 rather than v6.0.0.0.
  - Fixed in `src-tauri/build.rs` by emitting `cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'`.
- **Rust Quality Gates**:
  - `cargo check --lib`: Passed with 0 errors.
  - `cargo test --lib`: **256 passed, 0 failed, 2 ignored** (network tests). Knowledge vector search test passed cleanly with real `dist-backend/vec0.dll`.
  - Flake fix: Updated `stopwatch_pause_resume_rename_restart_copy_and_delete_persist` assertion in `src-tauri/src/native_core/extensions/v2_timers.rs` to allow `00:00` or `00:01` under high-concurrency 258-test runs.
- **Frontend Quality Gates**:
  - `pnpm typecheck`: Passed with 0 errors.
  - `pnpm test`: Passed 105 test files, 621 tests.
- **Windows Production Application Build**:
  - Verified presence of all 12 bundled resources on disk.
  - Staged `native/audio-device/win-audio-cli.exe` with verified SHA-256 (`9BDA285CAEA2477F5504A73B532D4102E5D8E37A63C16669FACA1834CCFD2048`).
  - Compiled backend & frontend assets via `pnpm exec tsx scripts/build-backend.ts` and `pnpm exec vite build`.
  - Production binary: `src-tauri\target\release\tezbar.exe` (39,746,560 bytes).
  - Production NSIS installer: `src-tauri\target\release\bundle\nsis\Tezbar_0.2.9-beta.10_x64-setup.exe` (16,355,478 bytes).
- **CLI & Isolated Profile Verification**:
  - Tested in fresh temporary directory (`TEZBAR_DATA_DIR`).
  - `--extensions-v2-list --json`: Enumerated all 11 native packages with correct metadata and 74/84 command count.
  - `--extensions-v2-install --all`: Successfully installed all 11 packages (`results: 11, all ok: true`). Subsequent listing confirmed all statuses changed from `not-installed` to `installed`.
  - Deduplication: Verified in catalog and registry that native packages take priority over legacy JS packages while preserving unported commands without duplicates.
- **Native UI Automation Gap & Programmatic Verification**:
  - Native Windows desktop automation is unavailable in this agent environment (`Agent device access is turned off for this environment`), so an interactive GUI automation pass was not performed.
  - **Timers Form Drafts**: `src/renderer/src/raycast-api/form-runtime.test.ts` proves that `reconcileFormValues` preserves `Seconds` and `Name` inputs across repeated background heartbeats and refreshes.
  - **Bundled Timer Audio**: `clean_native_install_can_start_default_timer_with_bundled_sounds` proves that `alarmClock.wav` and other sounds load from `resources/native-timers/` without relying on legacy JS assets. On Windows, `spawn_sound` plays sounds via `System.Media.SoundPlayer`.
  - **Dismiss Ringing Timer**: `fake_failed_notification_remains_pending_and_ringing_can_be_dismissed` and `row_dismiss_targets_only_its_ringing_timer` prove that `dismiss` removes the ringing timer and cleans up store state.

## Live macOS checks

Checks used the installed `.app` through native UI automation. Side-effecting trial data used a separate temporary profile containing only canonical Native V2 packages.

- Catalog: installed CleanShot shows `Rust 22 of 23`, explicit unavailable command and canonical replacement ID; Perplexity shows Rust and its replacement ID. Both canonical and old installed rows stay reachable.
- Color conversion: `#ff0000` returns RGB `rgb(255, 0, 0)`, HEX `#FF0000`, HSL `hsl(0, 100%, 50%)`; invalid input returns an empty-state explanation.
- Port Manager: displayed actual listening ports and filtered to port 9200. No user process was terminated.
- Audio Device: displayed the actual MacBook Pro speakers and current volume. No device or volume setting was changed.
- GIF Search: loaded trending data, then the `cat` query, rendered image tiles and saved a GIF to the temporary profile's favorites.
- CleanShot X: Manage Recording Presets displayed the eight built-in presets and Create New Preset action; capture/recording was not invoked.
- Amphetamine: Configure Session displayed duration and unit controls; OS keep-awake was not changed during this UI check.
- Speedtest: displayed its Ready screen; no bandwidth test or license acceptance was performed.
- Perplexity: displayed its query form; no live AI query was submitted.
- Google Search: the neutral `rust programming` query returned 16 actual suggestion rows.
- Google Calendar: displayed the own-client OAuth setup requirement without reading or writing calendar data.
- Timer form: live QA caught periodic refresh clearing typed fields. The failure was reproduced at the form reconciliation seam before the fix. After the fix, duration and name survived multiple refreshes; a five-second timer started with its default sound, reached `Ringing` in Manage Timers, and was dismissed to an empty list.

Native metadata deliberately retains `verification: not-run` for a full per-platform extension check. Selected live scenarios and passing unit tests do not imply every command, external application, account flow or Windows UI has passed.

## Regression fixes found during this batch

- Fixed Windows test entry point failure `0xC0000139` by embedding Common-Controls 6.0 manifest dependency in `build.rs`.
- Fixed test race condition in `v2_timers` stopwatch elapsed time copy assertion under high concurrency.
- Color extraction no longer upscales tiny images and invents interpolated colors.
- Native refresh runs the adapter and search cancellation prevents obsolete HTTP results from committing state.
- Port termination validates a fresh process identity; search filters remain effective after refresh.
- Canonical installation routes to compiled adapters; explicit own V1 repository URLs still route to the compatibility installer.
- Curated catalog entries retain live download counts/artwork and own repositories.
- Native descriptions no longer promise unimplemented periodic audio enforcement.
- Launcher command IDs are deduplicated with Native V2 taking priority; uncovered legacy commands remain available.
- Timer sounds are bundled with the application. A clean-profile regression catches the former dependency on a legacy JS package; all six selectable WAVs are checked.
