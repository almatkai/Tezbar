# Native V2 first batch — 2026-10-21

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
- `pnpm typecheck` — passed. `pnpm test` — 618 passed, 9 skipped. Form reconciliation regression tests cover refresh, authoritative server changes and controlled-field rejection.
- Release-policy checks — 8 passed. No release feed or production release was published.
- macOS ad-hoc `.app` build succeeded and was installed under `/Applications/Tezbar.app`; the previous application and installation metadata were backed up. No notarization is claimed.
- Native CLI listing and `--extensions-v2-install --all`: 11/11 installed in an isolated macOS profile and in the active profile. Legacy packages and preferences remain available. After fixture QA the installed application was restarted in the normal profile; the actual installed executable lists all eleven canonical packages as installed.
- Live neutral GIF provider request (`cat`) returned HTTP 200 with results.

## Windows evidence and limit

A disposable source snapshot was compiled on Windows with Rust 1.85/MSVC. `cargo check --lib --locked`, test compilation (`--no-run`) and `cargo build --bin tezbar --locked` passed. The actual compiled CLI listed 74/84 Rust commands and installed 11/11 packages in an empty temporary profile. The SHA-256-pinned audio helper enumerated 2 input and 4 output devices.

The Windows test executable exits **before running any test** with `0xC0000139` (`STATUS_ENTRYPOINT_NOT_FOUND`). A clean system PATH did not resolve it; a minimal Rust 1.85 test executable ran. Checking named imports did not establish the cause. Therefore Windows unit tests are **not passed**. The snapshot used temporary placeholders for excluded `dist-backend` resources: these checks establish Rust/CLI compilation, not a Windows installer build or GUI pass. No Windows GUI pass is claimed. The final registry, form and bundled-timer-resource changes were copied into the same disposable snapshot; a fresh Windows `cargo check --lib --locked` passed.

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

- Color extraction no longer upscales tiny images and invents interpolated colors.
- Native refresh runs the adapter and search cancellation prevents obsolete HTTP results from committing state.
- Port termination validates a fresh process identity; search filters remain effective after refresh.
- Canonical installation routes to compiled adapters; explicit own V1 repository URLs still route to the compatibility installer.
- Curated catalog entries retain live download counts/artwork and own repositories.
- Native descriptions no longer promise unimplemented periodic audio enforcement.
- Launcher command IDs are deduplicated with Native V2 taking priority; uncovered legacy commands remain available.
- Timer sounds are bundled with the application. A clean-profile regression catches the former dependency on a legacy JS package; all six selectable WAVs are checked.
