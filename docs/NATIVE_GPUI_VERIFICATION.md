# Native migration verification

M0–M1 checkpoint: implementation `48fa44edee026db64b8f75276cda9bb669e7ba6b`.
The full migration is **not complete**. M2 follows; M3–M11 and all native UI acceptance
remain outstanding. No default entrypoint, installed app, production profile, signing
identity, updater feed, release guard or immutable performance report was changed.

## M0–M1 evidence

Raw logs and archive: `/tmp/tezbar-native-m01-evidence/` on the build Mac.

| Gate | Evidence / result |
| --- | --- |
| Before moves, Rust 1.85 Tauri library with facade | `baseline-run.log`: 611 passed, 9 existing ignored (620 executed inventory) |
| Default extracted core, explicit Rust 1.98.1, locked | `core-tests-default-final.log`: 562 passed, 8 existing ignored |
| Extracted core with compatibility facade | `core-tests-final.log`: 564 passed, 8 existing ignored |
| Retained Tauri library, Rust 1.85, locked | `tauri-tests-final.log`: 56 passed, 1 existing ignored |
| Real Rust → Bun integration, explicitly enabled | `real-bun-core-2.log`: 1 passed, 0 ignored; no live AI/services |
| Original prototype, `bench-control` | `prototype-tests.log`: 8 passed, no ignored |
| TypeScript | `typecheck.log`: passed; `pnpm-test.log`: 744 passed, 7 existing skipped |
| Actual extension-worker package script | `extension-worker.log`: Bun source compile, read-only SQL, host services, Unicode, effects, errors, IPC rejection and EOF cleanup passed |
| MSRV | `msrv-tauri.log`, `msrv-core-root.log`: explicit `cargo +1.85.0 check --locked ... --lib` passed in both lock domains |
| Legacy release library | `tauri-release-lib.log`: `cargo +1.85.0 build --locked --manifest-path src-tauri/Cargo.toml --lib --release -j 3` passed; embedding/identity guards retained |
| GPUI release | `m1-mac-final-build.log`: `cargo +1.98.1 build --locked --release -p tezbar-native --features runtime-shaders -j 3` passed |
| CLI and dependency isolation | `mac-version.json`, `mac-help.txt`, `mac-libraries.txt`, `core-tree.log`, `app-tree.log`: marked-profile opt-in, no-profile refusal, no Tauri/Wry/Tao dependency or WebKit linkage |
| Lifecycle fixture script | `lifecycle-script.log`: one actual persistence/restart/internal-channel-rejection test passed after updating its package/filter |

The full executed-test mapping is [native-m01/test-retention.json](native-m01/test-retention.json):
620 old executions map to 614 retained executions with **no missing case**, plus 15 new
Host regressions = 629. The old library compiled the same six child-supervisor tests
under both `extensions::process::supervisor` and `knowledge::runtime::child_supervisor`.
Both names map to the single shared supervisor implementation. All nine old ignore
identities persist: eight in core and one in the Tauri adapter. No ignores were added.
The spec's 620 source attributes are not a count of passing portable core tests.

The new tests cover paste hide/key ordering, dialog confirm/error/cancel mapping,
Knowledge folder replies, settings/tab events, visibility and cache operations,
headless denial, screenshot PNG dimensions, duplicate/empty shortcuts, registration
conflicts, unregister cleanup, persistence rollback, Vault grant/cancel/failure focus,
and scoped status-item caps/update/callback/cleanup. The 51 command registrations,
206-member bridge and `legacy_channels.json` fixture remain identical.

Initial failures are retained, not masked: `core-tests.log` had six relocated resource
lookup failures (vec0 and pinned embedding archive), fixed at their source paths before
the complete passing rerun. `host-tests.log` caught the new test expecting an `ok:false`
envelope where the existing persistence contract propagates `Err`; the assertion now
checks the unchanged error. `real-bun-core.log` matched zero tests due to an incorrect
exact module filter; `real-bun-core-2.log` ran the real integration case.

## Artifacts and Windows

Both binaries below use source `48fa44edee026db64b8f75276cda9bb669e7ba6b` and rustc
commit `48a229ceaefd4985c50990b14116b6d856af0985` (1.98.1).

- Mac: `target/release/tezbar`, SHA-256
  `16b909461fb8077daa0c0d34e667f82a5ea90ed655e7b23b03beb3b10d14c991`.
  The local preview bundle is under the OS-temp root
  `tezbar-native-m01-preview-ftd72r9v/Tezbar Native Preview.app`, identity
  `com.tezbar.preview.native`, ad-hoc local preview only. CUA observed that identity
  running after launch; its hidden-window AX request timed out. No visible UI or a11y
  parity is claimed. The owned preview process was terminated.
- Windows: `C:\Users\User\AppData\Local\Temp\tezbar-native-m01-48fa44e\target-1-98-1\debug\tezbar.exe`,
  SHA-256 `4a6408ff855acfe984654e39eacfdfc952937b76498abf0b45785199af2ce7c9`.
  `windows-resume-host.log`: locked debug build passed with the existing SDK 10.0.22621
  fxc/rc and explicit Rust 1.98.1. `--version` agrees with the Mac. No Windows GUI,
  DirectX interaction, IME or permission grant is claimed.
- The Windows archive `source-48fa44e.tar.gz` has SHA-256
  `c1588c86b63c0726e6dffbad47bf4b167a99f15fd00faf908aa060ff83702efc`.
  The remote verifier checked the archive hash and all 1,713 tracked-file hashes
  against `manifest-48fa44e.json` before extraction/build. New task folder only;
  `C:\Users\User\Desktop\Tezbar` and earlier evidence were untouched.
  Initial PowerShell native-stderr handling aborted a first attempt; the resumed
  script uses cmd redirection, preserving both logs. T3 link tools were attempted
  once and unavailable; SSH was the explicitly authorized fallback.

## Locks, licenses and bounded deviations

[native-m01/lock-drift.json](native-m01/lock-drift.json) records every shared registry
name/version-set difference. `scripts/native/lock-drift.py` fails on movement of any
of the ten core watchlist packages or any registry change in the Tauri lock.
Tauri's registry set is unchanged; its only changes are the core path package and
its dependency edge. The prototype lock adds the same path package.

Root watchlist: arboard 3.6.1, image 0.25.9, notify 8.2.0, regex 1.12.4,
reqwest 0.13.4, rusqlite 0.32.1, serde_json 1.0.150, tokio 1.52.3,
unicode-normalization 0.1.26, walkdir 2.5.0. All match the Tauri seed.
GPUI's separate root resolution moves futures-{channel,core,executor,io,macro,sink,task,util}
0.3.32 → 0.3.34 and the wasm/TOML family; platform additions/removals and all precise
versions are in the JSON. No blanket cargo update or lock deletion was used. The root
lock was reseeded from the original Tauri lock with the actual core dependencies present.

[native-m01/gpui-licenses.json](native-m01/gpui-licenses.json) records all 21 locked Zed
workspace packages: Apache-2.0 at the official `c84af5b` pin. No GPL Zed UI/editor/theme,
markdown or terminal crate is linked. The prototype's async-task patch, font-kit,
Linux proc-macro-crate 2.0.0 resolution and opt-in runtime shaders are reused.

Authorized compatibility deviation: the old launcher facade was moved once into the
portable core behind `native-launcher-facade`, and the Tauri crate reexports it. This
preserves the documented prototype and its tests; remove the facade and prototype at
M11. There is no fork or AppHandle backdependency. Core owns the one PTY state and
synchronous metadata transaction; legacy commands are thin async/blocking adapters,
with write/resize kept synchronous in IPC order. Both startup binaries dispatch the
private child supervisor and native CLI before application/runtime creation.

The Host port lives in core; real GUI objects live in `src-tauri/src/tauri_host.rs`.
Headless TestHost defaults deny desktop interaction and do not start desktop services.
The isolated prototype explicitly enables its existing service set without registering
hotkeys or trays. Runtime development mode and legacy terminal history directory are
explicit CoreContext values supplied by Tauri, keeping CorePaths literals compatible.
No heavy feature startup or cache optimization is included in this extraction.
