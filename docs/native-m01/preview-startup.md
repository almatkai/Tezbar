# Native preview startup support

The native application can call
`NativeCore::start_with_options(StartupOptions::ISOLATED_PREVIEW)` to start basic
launcher services without default hotkey registration, durable extension background
restore or background Knowledge startup. `start()` delegates to all-enabled defaults,
preserving the temporary Tauri application's behavior.

The preview Host must also return `false` from `desktop_services()`. This keeps real
clipboard observation, filesystem watching and OS app discovery off. It does not
install or grant any system permissions. The native preview may explicitly register
its separate test hotkey through its desktop adapter. Knowledge requests still start
that service on demand; passive `core:knowledge:background` status does not. Neither
this API nor its name promises a blanket deny policy for explicit tool requests.

The regression fixture uses a marked-off fake resource/home/data tree and deliberately
unreadable extension-restore data. Preview startup succeeds, appending/searching a
note works, no Knowledge directory is created, no restore data is consumed, shutdown
stays lazy and restart after shutdown is refused. The paired normal-start fixture
continues surfacing the malformed extension-restore error.

M4 dependencies now expose the exact AccessKit version already resolved by GPUI
(`0.24.1`, `enumn`) directly to UI code. GPUI `test-support` is enabled only in the
UI/app dev dependencies. The app also directly depends on the host's exact
`global-hotkey 0.8.0`, `tray-icon 0.23.1` and `muda 0.19.2` versions so its event-loop
adapter can own typed event forwarding and primary-tray lifetime. UI/model modules
do not gain dependencies on those native OS event types.

The pinned OS event libraries install handlers through a one-time cell. Install the
relay before any manager, tray or event is created. Dropping a subscription must
release its runtime/window captures in the relay; calling `set_event_handler(None)`
does not replace an already-installed callback. Dispatch extension menu actions on
the GUI thread through `NativeDesktop::dispatch_menu_event`.

The original M4 dependency preparation check passed, including the reduced AppKit
feature set (`/tmp/tezbar-native-m04-parent-prepare-final.log`). Focused core startup
regression logs are `/tmp/tezbar-native-m04-parent-preview-start.log` and
`/tmp/tezbar-native-m04-parent-normal-start.log`. The Rust 1.85 Tauri desktop-bench
check also passed (`/tmp/tezbar-native-m04-parent-start-msrv.log`). Lock auditing is
recorded in `/tmp/tezbar-native-m04-parent-support-lockcheck.json`; the watchlist and
Tauri registry versions are unchanged. Full M3/M4 acceptance and actual GPUI/RAM
checks remain separate gates.
