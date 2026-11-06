# M4 native foundation

This slice provides shared Rust/GPUI controls, the opt-in application composition,
window/host coordination and isolated preview infrastructure. It does **not** establish
M5–M10 surface parity, the M11 product cutover, or a RAM improvement. The main parity
ledger and verification document remain parent-owned and unchanged by this lane.

All changes belong to `Tezbar-native-rust-agent`, `feat/native-gpui-app`. No staging,
commits, pushes, PRs, installations or production-profile operations were performed by
this lane. Concurrent controller/worker changes belong to the parent and other lanes.
The official GPUI pin and async-task patch were not changed. No Zed GPL UI/editor,
Markdown, theme or terminal crate was used.

## Interfaces provided

- `tezbar_ui::text_input::TextInput` is promoted from the Apache GPUI prototype. It
  owns UTF-8/UTF-16 conversion, grapheme and word movement, pointer selection, caret,
  IME marked ranges and candidate geometry, composition cancellation, committed-change
  events, multiline editing, bounded undo/redo (100 snapshots and 1 MiB history), password
  painting and synthetic AccessKit text. Password values and text-selection children are
  omitted from accessibility. Clipboard operations emit intentional effects; the app executes
  them only from field user actions. OS IME and screen-reader testing remain manual gates.
- `tokens`, `controls` and `virtual_list` hold the final `--ln-*` cascade, text ramp,
  dimensions, system/light/dark choice, font fallback, zoom metrics, reduced-motion timing,
  shared controls, guarded pointer/keyboard activation, focus restoration/cycling,
  full-count accessibility positions and lazy list/grid rows. They introduce no decorative
  gradients, glass cards, accent stripes or new palette.
- `image_cache` owns entry/byte LRU accounting, four concurrent decode admissions,
  duplicate-request suppression, per-request serial/generation fencing and explicit texture
  release through `App::drop_image`. Limits are 128 entries, 32 MiB retained bytes,
  2 MiB PNG input and 2048×2048 decode dimensions. Hide, replace/remove, eviction and
  entity release have release paths. Later surface effects execute admitted decode requests;
  constructing the cache decodes nothing. Pure ownership/conversion tests are not a real GPU
  texture-pressure measurement.
- `markdown` parses with pulldown-cmark and retains a committed prefix plus an unclosed
  streaming tail. Unchanged revisions/history are not reparsed. Settled visible code lazily
  initializes syntect and skips highlighting blocks over 12,000 bytes. Native text layouts
  support pointer selection and Copy effects. Links are scheme checked and emit host effects;
  images emit bounded HTTPS or relative sandbox-asset requests. Raw HTML is tokenized with
  html5ever into the fixed native subset, without DOM, HTML execution or WebView. Heading,
  document, link, code, table and task semantics are supplied; details have native expansion.
- `core_client` is the typed request seam. The shell uses the existing core search channel
  and does not implement ranking, application discovery or another core store. A cancelled or
  stale owner cannot accept a late reply. Core shutdown retains its actual Tokio task across
  an eight-second cooperative timeout; a later quit awaits that same task rather than treating
  the core's already-set closing flag as proof of completed cleanup.
- `events::Relay` has 256 control slots, coalesced activation/layout/status signals, and
  count/byte-bounded core output with lossless producer backpressure. One oversized,
  already-core-owned event may occupy an otherwise empty queue. `CoreEvents` provides
  scoped weak-owner subscriptions. Unsubscribed reliable output remains owned; its budget
  pauses core dequeueing while control messages still run. Pending UI buffers are explicitly
  disposed when real core shutdown begins. Consumers subscribe before starting optional
  services. Visibility/navigation events are not cached for later action replay.

## Window and host behavior

| Ledger rows | Foundation behavior | Remaining acceptance |
| --- | --- | --- |
| W-01, W-04 | Hidden PopUp by default; 760×640, 480×360 minimum; 18 px sheet; show/focus/hide signals; generation-fenced five-second idle trim | Current light/dark screenshot comparison |
| W-02, W-05, W-06 | Host placement/layout math; cursor-first monitor choice (the actual M2 rule); work-area clamp; per-monitor positions/dimensions/zoom; serial latest-snapshot writer; edge/corner resize; independent UI zoom; Windows post-show correction | Physical multi-monitor/mixed-DPI and Windows GUI matrix |
| W-03, W-09 | 120 ms blur grace, dialog suppression, Settings-focus handling, panel-focus exception; lazy parented sessions panel with 300/266/96/86/260 geometry and 80 ms panel grace | Interactive native focus/blur and panel matrix; M10 session content |
| W-07 | Host dwell/speed acquisition, generation-fenced 130 ms timer, snap overlay with raw-handle click-through | Physical drag/pass-through on both OSes |
| W-08 | Native Settings shell and close generation/ack/finish protocol; only unacknowledged bootstrap has a three-second fallback | M7 installs the actual Settings write owners and full tabs; shell has no pending provider drafts |
| W-10, W-11 | Native template tray, left-button-up toggle, right menu, Alt+F19 preview shortcut; OS handlers installed before managers/items; extension menu dispatch stays on GUI thread | Actual OS hotkey/tray event-delivery matrix, especially Windows |
| W-12 | Existing single-instance lease and real restore callback: visible Settings first, otherwise launcher; retirement starts only after successful write drain, before core stop | Clean-source two-process interactive window-focus check |
| W-13 | Idempotent pre-quit coordinator; serial writes drain first; then retire instance, unregister preview shortcut, drain core, retain lease until done; native macOS termination deferred with `NSTerminateLater` | Final-source repetition, Settings M7 save-error UI, Windows interactive close/console matrix |
| W-14, W-15, W-20 | Native confirmation/folder/PNG-save prompts with cancel mapping and blur suppression; notifications forwarded to host; capture hides/restores owned windows; paste hides native windows/app before core synthesis | Permission-free automated checks do not prove dialogs, notification permissions, capture or real paste on user systems |
| W-16, W-17, W-18 | Regular activation/Dock restore; core ready/failure and retry seam; existing rotating FileLog registered at Info level inside preview profile | Native activation and log-path equality on installed product belong to final platform checks |

`GpuiHost` constructs `NativeDesktop` inside the actual GUI launch callback and verifies
macOS's main-thread marker. The wake callback enqueues operations; only the GPUI receiver
executes them. Native global-hotkey/menu/tray handlers capture a weak releasable mailbox.
Upstream OnceCell handlers are installed once and are never “replaced” with `None`.
Core extension trays retain their existing `NativeDesktop` resource/callback ownership.

The standalone executable requires a marked child of the OS temporary directory, rejects
profile symlinks, requires an explicit source-matched read-only resources directory and
never falls back to a production profile. Supervisor and native-v2 CLI dispatch precede
runtime/GUI construction. One Tokio runtime is installed through `init_from_handle`.
`StartupOptions::ISOLATED_PREVIEW` plus `desktop_services=false` keep production shortcuts,
clipboard observers, app/filesystem discovery, background extension restore and background
Knowledge startup off. No model, Bun, embedding or terminal request is made just to render
or hide the shell. Explicit preview shortcut registration rejects production Alt+Space and
Control+Space. `bench-control` is compile-time gated and absent from ordinary builds.

The twelve-hour update policy remains the existing shared UI-model policy; isolated preview
never initiates a live updater check. The M11 product composition must use that policy and
perform the updater rehearsal. No release identity, profile migration or default entrypoint
cutover is claimed here.

## Evidence and limits

Raw evidence is in `/tmp/tezbar-native-m04-evidence`. `foundation-test-map.json` names
individual source tests and the remaining manual gates. Final targeted validation passed:

- `CARGO_BUILD_JOBS=2 cargo +1.98.1 test --locked -p tezbar-ui -p tezbar-native --features runtime-shaders`: **22 app + 30 UI passed**, zero failures/ignored. App builds with both integrated model features; no model effect is executed by the shell. Earlier standalone UI-only validation also passed with model features off.
- `cargo +1.98.1 check --locked -p tezbar-native --features runtime-shaders`: passed.
- `cargo +1.98.1 build --locked -p tezbar-native --features runtime-shaders`: passed.
- `cargo +1.98.1 test --locked -p tezbar-native --lib --features runtime-shaders,bench-control`: **23 passed**.
- Package-scoped formatting and owned-path `git diff --check` passed. Only the existing core warnings remain; no UI/app warning was introduced.
- Normal dependency tree contains no Tauri/Wry/Tao. `otool -L` contains no WebKit framework. Normal `--version` reports `bench_control:false` and `dirty:true`; its compiler/source/hash receipt is `final-normal-build-receipt.json`.

The app preserves physical persisted position units and legacy monitor identities, while GPUI window sizes stay logical. Windows cursor/frontmost lookup resolves the HMONITOR-backed GPUI DisplayId instead of comparing physical cursor coordinates to logical display bounds. Placement uses target-display scale, including its post-show correction. A negative-origin/DPI table regression is included; interactive mixed-DPI acceptance is still pending.

A later integrated control fixture (`final-fixture.json`, `final-lifecycle-smoke.json`, `final-termination-smoke.json`) verified hide/show state, Settings state, a successful second-instance activation exit, and repeated actual native termination while held for 0.4 seconds. Release then exited with code zero and removed the endpoint. This is state/control evidence, not a screenshot or an assertion of observed OS focus. Final capture/save cancellation guards and resize/grid/focus refinements were subsequently validated by the final automated checks; the control receipt remains explicitly tied to its recorded binary hash.

Owned preview PIDs and the final endpoint were checked absent (`owned-cleanup.json`). Two earlier blocked experiments used forced recovery as described below; the successful fixtures exited cooperatively.

Real macOS termination smoke passed on the intermediate binary recorded in
`termination-smoke-receipt.json` (source `cc7823f2714ed7b13b17f5bbf5bc1cb8961edfe4`,
**dirty=true**). `termination-smoke.py` records the exact fixture sequence: hold shutdown,
initiate actual `NSApplication terminate:`, wait **0.4 s**, inspect responsive quitting state,
initiate a second native quit, release shutdown, wait **0.6 s**; PID then no longer existed
and the endpoint had been removed. The receipt includes binary, resource manifest, actual
tray-template and script SHA-256 hashes. This is intermediate-source evidence, not a claim
that later edits were exercised by that same process.

Direct termination inside a borrowed GPUI callback and then a GCD main-queue block failed
because of AppKit nested-loop reentry. Those failed fixtures were force-killed **only for
recovery**, and failure logs are preserved separately. They are not graceful-shutdown
passes. The successful fixture schedules the native termination via Cocoa's delayed selector,
outside the GPUI borrow and outside a non-reentrant main GCD block. The adapter adds only
an absent `applicationShouldTerminate:` method to the exact `GPUIApplicationDelegate`,
checks installation/ABI, leaves existing GPUI methods intact and replies YES/NO after drain.

CUA inventory worked, but selecting this isolated preview consistently returned
`Computer Use server error -10005: cgWindowNotFound`. No new visual, VoiceOver, clipboard,
IME, tray-click or hotkey observation is claimed. No installed Tezbar app was opened.
The prototype screenshot references in its README and the CSS/spec remain the reference
baseline; a complete old/new surface screenshot matrix is still required after M5–M10.

On macOS, native events use the AppKit/Carbon run loop; on Windows the dependencies use
the thread's Win32 message loop (source: locked `global-hotkey` 0.8.0, `tray-icon` 0.23.1,
`muda` 0.19.2 implementations). No polling timer pumps these channels. Linux has different
tray/GTK and display-backend requirements and is not a supported product acceptance target
in this Mac/Windows migration. No Linux or Windows GUI pass is inferred from this Mac build.

M5–M10 still own full command-bar behavior, utilities/rich notes, all Settings write queues,
onboarding, chat, extension runtime and terminal content. M11 owns exact-source dual-platform
packaging, updater/data/signing continuity, removal of the first-party TS/WebView path and
matched RAM measurements. No ledger surface is promoted to completed parity by this document.
