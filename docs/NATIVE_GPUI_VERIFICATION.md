# Native migration verification

M0–M2 checkpoint: runtime implementation `80a2aa1286ac8b4517452594a4779bc33c724b6b`;
M0–M1 checkpoint: `48fa44edee026db64b8f75276cda9bb669e7ba6b`.
The full migration is **not complete**. M3–M11 and all native UI acceptance remain
outstanding. No default entrypoint, installed app, production profile, signing
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

## M2 checkpoint

Runtime implementation source: `80a2aa1286ac8b4517452594a4779bc33c724b6b` (built from a
clean tree). The subsequent verification commit changes evidence/audit tooling and clarifies one
stale PTY comment; runtime code is unchanged.

The shared host crate now owns input/capture primitives and cancellation guards,
OS monitor lookup (frameworks supply only typed coordinate lookup), placement,
positions, snap acquisition/dwell policy, layout/resize math, native status/hotkey
drivers, timer and generic notifications, profile-path rules, bounded logging and
single instance. The legacy executable consumes the shared implementation and keeps
its framework-specific window operations, close handshake, event-loop adapters and
existing hotkey/tray/single-instance/log plugin lifecycle. NativeDesktop construction
is inert; its main-thread executor and event pump get wired/tested in M4. No hotkey,
tray, autostart, microphone or real capture was invoked by the new fixtures.

The update crate owns the channel and manual-release modules, temporary-cache policy,
status wire type, immutable asset URL validation and shared fallback/status helpers.
Its native transport is demand-driven, bounds manifests to 1 MiB, downloads to a
private file, throttles progress at 150 ms and verifies into an anonymous immutable
snapshot. Modern Minisign verification streams in 64 KiB chunks; legacy verification
maps the privately owned anonymous snapshot rather than allocating an artifact-sized
heap buffer. The signed synthetic archive fixture tests tamper rejection and staged
extraction. Archive entry paths are restricted to Tezbar.app and tar’s guarded unpacking is retained. The macOS install primitive requires
the existing designated requirement and verifies helper Team IDs before swapping;
the previous app is retained for relaunch/rollback. NSIS preparation preserves
`/P /UPDATE /ARGS`. **No installer, production bundle swap or production signature credential was
invoked.** Application-level install/relaunch coordination and full staged-install
rehearsal remain M11; the default Tauri updater still uses its existing plugin transport.

The 28 existing `src/shared/updater.test.ts` cases have Rust twins in
`tezbar-ui-model::update_policy`. The renderer and TS tests stay intact until cutover.
The exact [mapping](native-m01/m2-updater-test-map.json) prevents a duplicate port at M3.
This preserves the actual twelve-hour freshness/check boundary, including its strict
`>` versus `<` distinction; the spec's four-hour schedule prose differs from the
current TS source. Likewise, placement preserves the existing cursor-first selection
and its tests, despite the spec's frontmost-first prose. Review those discrepancies at
M4/M11 rather than changing current product behavior during extraction.

### Extracted module map

| Former owner | Single implementation / adapter |
| --- | --- |
| `src-tauri/src/native_core/**` | `crates/tezbar-core/src/**`; public `NativeCore` and its existing module tree |
| native terminal / bridge | `tezbar-core::terminal::pty`, `launcher::terminal::TerminalCore::metadata` and `terminal_bridge`; `src-tauri/src/native_terminal.rs` retains command macros |
| C rank helper | `crates/tezbar-core/build.rs` + `src/fts_rank_math.c`, compiled once |
| application coupling | `tezbar-core::host::{Host,SurfaceOp,DialogRequest,ShortcutRegistry,StatusItems,NotificationSpec,TestHost}`; `src-tauri/src/tauri_host.rs` implements the legacy GUI adapter |
| input, monitor and window policy | `tezbar-host::{input,monitors,placement,layout}`; Tauri retains window/event-loop type glue |
| notifications | `tezbar-host::{notifications,timer_notifications}`; typed callback supplied by the adapter |
| native desktop bootstrap primitives | `tezbar-host::{desktop,single_instance,paths,logging}`; event-loop/bootstrap integration remains M4 |
| channels, manual releases and temp files | `tezbar-update::{release_channels,manual_releases,updater_temp}`; legacy namespace reexports preserve imports |
| portable updater and wire status | `tezbar-update::{native,AppUpdateStatus}`; existing Tauri plugin transport/install stays in `src-tauri/src/updater.rs` |
| TS updater policy | `tezbar-ui-model::update_policy`, 28 mapped Rust cases |
| opt-in GPUI bootstrap | `apps/tezbar/src/main.rs`, package `tezbar-native`, binary `tezbar` |

### M2 test inventory and gates

[native-m01/m2-test-retention.json](native-m01/m2-test-retention.json) maps all 620 original
executions to their current package: **zero missing cases**, the same six consolidated
supervisor duplicate executions, all nine original ignore identities retained. The
feature-enabled inventory is now 675 cases (666 pass / 9 ignored):

| Package / gate | Result | Raw evidence in `/tmp/tezbar-native-m01-evidence/` |
| --- | --- | --- |
| tezbar-core, facade enabled, locked Rust 1.98.1 | 564 pass / 8 existing ignored | `m2-core-feature-final.log` |
| tezbar-host, locked Rust 1.98.1 | 39 pass / 0 ignored | `m2-win-log-final-tests.log` |
| tezbar-update, locked Rust 1.98.1 | 22 pass / 1 existing ignored | `m2-win-log-final-tests.log` |
| tezbar-ui-model, locked Rust 1.98.1 | 28 pass / 0 ignored | `m2-win-log-final-tests.log` |
| retained Tauri library, locked Rust 1.85 | 13 pass / 0 ignored | `m2-tauri-80a2aa1-tests.log` |
| prototype, locked Rust 1.98.1, bench-control | 8 pass / 0 ignored | `m2-prototype-locked-test.log` |
| explicit Rust 1.85 portable checks and Tauri check | passed | `m2-win-log-msrv.log`, `m2-msrv-repaired.log` |
| locked Tauri release-library build | passed, embedding/identity guard retained | `m2-tauri-80a2aa1-release.log` |
| pnpm typecheck / pnpm test | passed / 744 pass, 7 existing skipped | `m2-typecheck.log`, `m2-pnpm-test.log` |
| actual Bun worker fixture | passed | `m2-extension-worker.log` |
| release-policy/signing fixtures | 20 pass, 2 existing opt-in skips | `m2-release-policy.log` |

The final single-instance helper also waits at most two seconds for a newly elected
owner to bind its endpoint, retries only connection establishment and never replays
an activation. Its deterministic transient/permission-error test runs on both platforms.

The Windows 175ed71 run found the immediate drop/reacquire pipe-drain race, and its
failure is retained in `windows-m2-final-host.log`. The repair waits at most two seconds
for the retired pipe endpoint to close after obtaining the replacement owner mutex;
it never activates the retired endpoint. The same regression and real second-process
fixture are run on the repaired archive. This is a lifecycle repair, not a new skip or
a blind flaky retry. An initially stale prototype lock was updated offline with the
new path-package edges, then its locked eight-test suite passed. No registry versions
changed in that lock. The adba233 Windows run also caught append-handle truncation
being denied by Windows (`windows-adba233-portable-failed.log`). Rotation now uses a
short-lived write handle under the log mutex, preserving the pinned KeepOne content
policy and append semantics; the same regression passes on the final archive.
The final Rust timestamp parser also preserves JS Number hexadecimal/binary/octal
forms in the mapped timestamp round-trip case.

[Final lock evidence](native-m01/m2-lock-drift.json) and
[native dependency licenses](native-m01/m2-native-licenses.json) accompany the original
GPUI license evidence. Both direct menu/tray dependencies disable libxdo defaults;
that unnecessary Linux dependency was caught and removed by the lock audit.
The Tauri lock has only path-package/edge additions, with zero registry-set changes;
all ten core watchlist versions stay identical to the accepted seed. The final audit
also enumerates every registry package addition/removal and path package in all three
lock domains, and asserts zero prototype registry movement against its own accepted lock.
The core, host,
update and ui-model dependency trees contain no Tauri, Wry, Tao or GPUI framework.
[Source audit](native-m01/m2-source-audit.json) records the byte/hash checks and source
reference scan. The only Tauri configuration edit is the Info.plist path following
the moved core file; its contents, signing identity and all other values are unchanged.
All 51 registered command names, the bridge, its 206-member desktop contract and the
legacy channel fixture remain unchanged. No old performance evidence was edited.

Bounded ownership deviation: the private child supervisor, Windows Job helper and
resource resolver remain single definitions in core from M1, with host reexports.
Moving them upward into a crate that implements core's Host interface would introduce
a core↔host cycle. No implementation was forked, no child ownership changed, and both
entrypoints still dispatch supervisors/CLI before starting an application runtime.

### M2 dual-platform build artifacts

[Machine-readable artifacts](native-m01/m2-artifacts.json) records paths, metadata and
hashes. Both binaries report clean runtime source `80a2aa1286ac8b4517452594a4779bc33c724b6b`, Rust 1.98.1
compiler commit `48a229ceaefd4985c50990b14116b6d856af0985`, the required official GPUI pin,
and runtime shaders enabled. These are the opt-in empty M0 preview, not product UI.

- Mac arm64 release: `/tmp/tezbar-native-m01-evidence/artifacts/80a2aa1/mac/tezbar`; SHA-256 `3115a0a7d5a60a1af60ed7eecebd6f22568f9b572490d880dc6a691deb57de0a`.
  `m2-mac-80a2aa1-release.log` passed. CLI help/version and no-profile refusal passed;
  `m2-80a2aa1-otool.log` contains no WebKit. The M1 isolated preview observation above
  remains the only native-window launch; no final UI or accessibility parity is claimed.
- Windows x64 debug: `C:\Users\User\AppData\Local\Temp\tezbar-native-m01-80a2aa1\artifacts\tezbar.exe`; SHA-256 `bf023e4fe25bccfaac3f7d4f35b9c83b8ef3009b5e4843c50fc5acb151a4c40f`.
  A verified local copy is `/tmp/tezbar-native-m01-evidence/artifacts/80a2aa1/windows/tezbar.exe`. `windows-80a2aa1-debug-full.log` passed;
  help/version/no-profile CLI checks passed with redirected subprocess handles.
  `windows-80a2aa1-portable-full.log`: host **38 pass / 0 ignored**, UI-model
  **28 pass / 0 ignored**, updater **16 pass / 1 original ignored**. Platform-specific
  macOS/Unix cases account for the smaller Windows inventory. No GUI/DirectX/IME claim.
- Archive: `/tmp/tezbar-native-m01-evidence/source-80a2aa1.tar.gz`, SHA-256 `ebaa99fd8ae5ffd6e251ccc9f88f4c77c33e611c624395f4c7233a0785c92656`. The remote verifier
  checked all **1738 tracked-file hashes** against
  `/tmp/tezbar-native-m01-evidence/manifest-80a2aa1.json` before compiling. New revision folder only; the Windows
  release checkout and earlier `source7434` evidence were not edited.

The final Windows build used the existing SDK 10.0.22621.0 fxc/rc and explicit user-level
Rust 1.98.1, with no SDK/toolchain installation or global/default change. Old dependency
cache entries had missing Cargo.toml files. `windows-locked-cache-seed.log` records the
repair in the new task-private cache: 730 locked public registry archives checked against
Cargo.lock SHA-256, 512 downloaded and 30,434 missing source files restored. No package
version changed. The final build and tests ran **offline + locked** against that cache.
T3 environment/link tools were attempted once and unavailable; SSH was the authorized fallback.

Both task process audits (`mac-final-process-audit.log`, `windows-final-process-audit.log`)
found no remaining owned preview/test executable. All build/test commands have exited.
No installer, autostart, real recording/capture, permission grant or production host launch
was invoked. No release/Actions/install/push/PR/merge operation was performed.

### M2 reproducible commands

Run from the selected worktree; every Cargo command uses its own existing lock.

```sh
cargo +1.98.1 test --locked -p tezbar-core -j 3
cargo +1.98.1 test --locked -p tezbar-core --features native-launcher-facade -j 3
cargo +1.98.1 test --locked -p tezbar-host -p tezbar-update -p tezbar-ui-model -j 3
cargo +1.85.0 test --locked --manifest-path src-tauri/Cargo.toml --lib -j 3
cargo +1.85.0 check --locked -p tezbar-core -p tezbar-host -p tezbar-update -p tezbar-ui-model -j 3
cargo +1.85.0 check --locked --manifest-path src-tauri/Cargo.toml --lib -j 3
cargo +1.85.0 build --locked --release --manifest-path src-tauri/Cargo.toml --lib -j 3
cargo +1.98.1 build --locked --release -p tezbar-native --features runtime-shaders -j 3
cargo +1.98.1 test --locked --manifest-path prototypes/native-launcher/Cargo.toml --features bench-control -j 3
pnpm typecheck
pnpm test
pnpm test:extension-worker
cargo +1.98.1 test --locked -p tezbar-core real_rust_host_bun_roundtrip_and_termination -- --ignored --test-threads=1
pnpm release:test
/opt/homebrew/bin/python3 scripts/native/lock-drift.py
```

The existing optional real-Bun case was explicitly run at M1 (1 pass); M2 reran the
real worker package fixture. No paid/live AI, MCP, production profile or actual clipboard
was used. Test/build concurrency stayed at three jobs; child ownership tests remain intact.

### Remaining M3–M11 handoff

- M3: port the remaining first-party controllers/business logic and durable UI state;
  reuse the 28 mapped update-policy cases already present.
- M4: full GpuiHost, launcher shell, native executor/event pumps, hotkey/tray/run-loop
  spikes, windows/monitor integration, single-instance/logging bootstrap, accessibility
  and full marked-profile isolation. Existing framework-specific window glue stays in
  the temporary adapter until it is replaced by this native host.
- M5–M10: complete command bar, utilities, Settings/onboarding, AI chat, extension node
  rendering and terminal, with parity tests and critical visual reviews.
- M11: updater orchestration, signed packaging, staged update rehearsal, UI-state import,
  default entrypoint cutover, relocation of the remaining immutable embedding catalog
  and development resources currently addressed under `src-tauri`, and removal of
  Tauri/first-party React and the compatibility facade/prototype. Bun remains only for third-party extension logic.

No full-app RAM or CPU saving is claimed. The ordinary product is still Tauri, and the
GPUI binary remains the intentionally empty M0 opt-in preview. Measure the full native
application only after UI parity; inactive heavy features must remain lazy and caches
bounded. No new speech/model/extension process is started by the M2 libraries.
