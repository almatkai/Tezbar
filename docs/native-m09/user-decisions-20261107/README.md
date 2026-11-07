# User decisions — 2026-11-07

These decisions belong to the native GPUI migration, in feat/native-gpui-app.
They do not change the installed beta or assert Windows/OS acceptance.

- Windows Ctrl+D/X/K/P reach the shell. Ctrl+Shift+D/X/K/P are the Tezbar actions.
  O-TERM-KEYS is resolved in spec §5.5/§5.7 and ledger K-16/K-29; physical keyboard
  and AltGr verification remain open. The actual production key handler is shared
  with a deterministic Windows-platform fixture, executable on macOS.
- A launched MenuBarExtra keeps its background worker owner through launcher hide
  and opening other commands, until disable/removal or app exit. Rerunning the same
  command replaces its owner, preventing duplicate menu items. Worker failure remains
  an interruption, not a foreground-cache eviction. The bounded budget is 64 background
  menus plus a separate 30-entry ordinary-session cache, leaving headroom under the
  core's 128-session authority bound. Capacity refuses a new menu without evicting old ones.

Implementation separates foreground eviction from retained menu sessions. Native
view retirement releases images/UI polling but does not dispose the background worker
session, including when it temporarily renders no menu. Core status fingerprints
retain that background ownership with an empty signature so a later timer invalidation
can restore an identical menu after its view has been replaced.

Verification: 4 worker runtime tests and typecheck pass. One production Windows-input
fixture and one GPUI retirement/root-transition fixture pass. The real SDK menubar
scenario passes through Bun → NativeCore/TestHost → RuntimeView: hidden updates,
transient null root, opening another command, later menu restoration, menu action,
130 same-command replacements, command disable and extension uninstall. The core
library checks on Rust 1.85. These are targeted gates, not a rerun of the entire
native suite, and they use fake OS boundaries, not real tray/clipboard windows.
No signing/install/release/live provider, real profile or RAM measurement was run.

Source/provenance receipts are adjacent. Prior review evidence remains historical.
