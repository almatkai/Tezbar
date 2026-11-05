# Opus 5.5 correctness/standards review — GPUI migration M0–M2

Scope: worktree `Tezbar-native-rust-agent`, branch `feat/native-gpui-app`, frozen range
`5597113..7e40b4f` (runtime `80a2aa1`). Source read via `git show`; no source edits,
builds, fmt, worktrees, network, profile, Keychain or clipboard access. HEAD `5b2766b`
(M3 preparation) and concurrent M3 lanes were not reviewed.

## Verdict

**ACCEPTED as the foundation for M3/M4, conditional on the small repairs F1–F3**, which
should land before M2 is recorded as complete. F4–F6 are contracts that M4/M11 wiring
must satisfy. Nothing found changes the shipping Tauri app's behaviour, and nothing found
bypasses update or signing policy. The whole migration is **not** complete.

## Findings (most severe first)

### F1 — macOS install primitive can never pass its designated-requirement check (Medium, confirmed)
`crates/tezbar-update/src/native.rs` `StagedMac::install_over` runs
`codesign --verify --strict -R <requirement> <bundle>`. codesign treats a bare `-R`
argument as a **file path**. Inline requirement text needs the `-R=<text>` form, which
`scripts/lib/macos-signing.mjs:112` already uses.

Probe (system binary, read-only):
- `codesign --verify --strict -R 'anchor apple' /bin/ls` exits 1 with "anchor apple: No such file or directory / invalid requirement specification".
- `codesign --verify --strict '-R=anchor apple' /bin/ls` exits 0.
- `'-R=identifier "com.wrong"'` exits 3, so the `=` form does enforce the requirement.

Impact: every staged install fails closed. It is not a bypass, but the verification
doc's claim that the primitive "requires the existing designated requirement" is
unproven; no test reaches `install_over`.

Repair: pass `format!("-R={requirement}")`. Add a unit test that asserts the argv shape,
or a probe against `/bin/ls` with a requirement that passes and one that fails.

### F2 — Windows NSIS arguments do not match the locked updater contract (Medium, confirmed against `tauri-plugin-updater 2.10.1` source)
The legacy app uses the default `installMode` Passive (`tauri.conf.json` sets none).

The plugin (`updater.rs:797-815`, `config.rs:36-44`) builds:
`/P /R /UPDATE /ARGS <each current arg escaped by escape_nsis_current_exe_arg>`. It joins
these into one raw parameter string and launches with `ShellExecuteW("open")`.

`StagedWindows::arguments` returns `["/P","/UPDATE","/ARGS", "\"a\" \"b\""]`, and
`launch` uses `Command::args`. That has three problems:

1. **`/R` (restart) is dropped.** A passive update installs but does not relaunch Tezbar.
2. **All current args collapse into one argv element.** std then re-quotes it, so the
   relaunched app receives a single argument with literal quotes. With no args it passes
   `""`, which becomes an empty argument.
3. **Escaping is naive.** Backslashes before quotes and a trailing `\` are not handled,
   and `/` inside an arg is not quoted, so NSIS `GetOptions` would split on it.

Also, `CreateProcess` (unlike `ShellExecute`) cannot trigger UAC if an install ever
needs elevation.

The test `constructor_is_inert_and_installer_arguments_preserve_required_flags` codifies
the wrong contract (`args[..3] == ["/P","/UPDATE","/ARGS"]`).

Repair: port the plugin's construction exactly (mode args, `/UPDATE`, `/ARGS`, each arg
escaped). Launch with `CommandExt::raw_arg`, or `ShellExecuteW` like the plugin. Test the
empty-args case and args with spaces, `/`, quotes and a trailing backslash. Unwired until
M11, but the primitive is an M2 deliverable.

### F3 — `desktop-bench` feature no longer compiles (Low–Medium, confirmed statically)
`src-tauri/src/lib.rs:1641` (still present at HEAD) calls `core.ctx.app.as_ref()`. M1
removed `CoreContext.app` (`crates/tezbar-core/src/common.rs` has no field or method
`app`). None of the M2 gates builds with `--features desktop-bench`. The matched-A
measurement build (`prototypes/native-launcher/scripts/build-a.py:13`) depends on it,
and so does §9.4.

Repair: add an injected `app: tauri::AppHandle` parameter to `call_backend` (IPC payload
unchanged) and pass `&app`. Then run `cargo +1.85.0 check --locked --manifest-path
src-tauri/Cargo.toml --lib --features desktop-bench`.

### F4 — Unix single-instance socket exceeds `sun_path` for OS-temp preview profiles (Medium for M4, confirmed by path arithmetic)
`single_instance.rs` binds
`<canonical dir>/.instance-<16 hex>.sock`. macOS `sun_path` is 104 bytes. The canonical
`$TMPDIR` here is `/private/var/folders/7v/_tm0w_59693180wdf89zpwmh0000gn/T/`. For the
M1 preview profile name, the socket path is 128 bytes under `…/data` and 123 bytes
under the profile root, so `bind` fails and `claim` returns Err.

The production data dir is 87 bytes and works. The test passes only because
`tempfile::tempdir()` gives 99 bytes. The spec's `--profile` dev loop (§6.4) lives
exactly in OS temp.

Repair: keep the lock in the profile, but bind the socket under a short per-user,
owner-only directory keyed by the same hash (verify owner and mode 0700). Alternatively,
bind through a short relative path. Add a test with a deep directory.

### F5 — Single-instance contender never re-attempts ownership (Low–Medium; matters for M4 bootstrap and M11 relaunch)
`connect_owner` retries only the connect, for up to 2 s. Owner `Drop` removes the socket
**before** releasing the flock. A launch that lands while the owner is shutting down
(the bounded shutdown is up to 8 s) sees NotFound until the deadline and returns Err. It
neither activates nor owns.

If the dying owner's listener is still alive, the activation goes to the instance that
is quitting. That is the update-relaunch race. Windows behaves the same, because the
mutex is held until drop.

Repair: inside the retry loop, re-try `flock(LOCK_EX|LOCK_NB)` (or the mutex) and become
owner when it succeeds. Make "owner is quitting" refuse activation so the contender
keeps waiting for ownership. Add a test for drop-during-contention.

### F6 — `NativeDesktop::call` contract (Low now; must be fixed when M4 wires it)
`desktop.rs` blocks the calling thread on `recv_timeout(5 s)`.
- Called on the main thread, it waits 5 s for its own queue and returns Err.
- On timeout the queued op is **not** cancelled. It later registers the hotkey or tray
  while `PlatformCore` has already recorded failure, leaking an OS registration that the
  next attempt reports as a conflict.
- `process_host` calls `status_items().set/remove` while holding its `resources`
  `std::sync::Mutex`, inside async handlers. That blocks a Tokio worker for a main-thread
  round-trip. TauriHost had the same pattern via Tauri's own main-thread wait, so this is
  not a regression.

Repair (M4): run inline when already on the main thread, and add a cancelled flag that
the late closure checks. Release the resources lock before the host call, or make the
port async.

### F7 — Portable `check()` diverges from the locked plugin's endpoint semantics (Low, unwired)
- The plugin ignores non-2xx statuses (logs and continues; at the end returns
  `ReleaseNotFound`, which maps to UpToDate or manual). `bounded_json` turns 403/5xx into
  a Transport error, which maps to `Error`.
- On 204 the plugin returns `None` immediately; the native code moves on to the next
  endpoint.
- Identity validation now runs before the platform lookup. On a feed with a bad identity
  but no matching target, the old app showed UpToDate and the new one shows Error.

All of this is stricter, with no policy bypass. Disposition at M11: either match the
plugin or record the change as an intentional deviation in the ledger.

### F8 — `core:terminal:detach` is now shadowed (Low)
`terminal_bridge.rs:30` maps `core:terminal:detach` to the PTY detach. It runs before
`launcher.handle`, which still advertises the same channel as metadata detach
(`launcher/mod.rs:69,410`, `launcher/terminal.rs:191`). There are no current callers, but
the M4 typed client gets an ambiguous name.

Repair: drop it from the launcher CHANNELS and the metadata alias, or rename the
metadata route.

### F9 — Standards: the "pure" UI-model crate pulls in the network stack (Low)
`tezbar-ui-model` depends on `tezbar-update` only for `AppUpdateStatus` and
`ReleaseChannel`. That brings in `reqwest`, `native-tls`, full `tokio`, `tar`, `flate2`
and `minisign` transitively. Gate `tezbar-update::native` and its dependencies behind a
feature, or move the wire types into a dependency-free module.

### Pre-existing, preserved (not M2 regressions; record in the ledger)
- **Tauri extension trays are never removed from the OS.** Tauri 2.11.2
  `TrayIcon::register` stores the icon in `resources_table` and appends a *global* menu
  listener. Neither the old nor the new `host.tray.remove`/cleanup/replace calls
  `app.remove_tray_by_id`. Stale trays stay visible and clickable, and dispatch
  `background_event` while the process is alive. TauriHost faithfully preserves this;
  `NativeDesktop` fixes it by dropping the `tray_icon::TrayIcon`.
- **Interrupted vault auth.** If an unlock or reset future is dropped mid-auth, blur
  suppression stays `true`. Unchanged from the old code.

## Verified without findings
- **Host port vs old call sites.** Paste does clipboard write, then `HideForPaste` (hide +
  macOS `app.hide()`), 80 ms, then Cmd/Ctrl+V. Confirm dialog keeps button order and
  suppress/restore of blur. Knowledge folder cancel returns `null`. Shortcut
  register-new, then release-old, with rollback and error strings, is byte-identical.
  `HotKey` id/Display round-trips through the same `global-hotkey 0.8.0`. Vault keeps
  suppress → auth → restore, with refocus on cancel/fail/after open. Settings ordering is
  unchanged.
- **Moved code.** An automated function-body comparison of 78 functions moved from
  `lib.rs`, `window_layout.rs` and `native_input.rs` into `tezbar-host` (whitespace,
  `pub` and paths normalised) found exactly one semantic change: `capture_protected`, now
  closure-based. Its protect → 120 ms → capture → restore order and the drop-restore on
  partial failure are preserved. Cursor-first monitor selection and Win32/CG cursor reads
  are identical.
  `release_channels`, `manual_releases` and `updater_temp` are byte-identical copies.
  The updater status and URL helpers are verbatim. Child supervisor, Windows Job,
  resource resolver and native-v2 CLI are pure path moves. Both binaries dispatch the
  supervisor, then the CLI, before any runtime or app is created.
- **PTY.** One `NativeTerminalState`, `Arc`-owned by `CoreContext`. The Tauri managed
  state was removed, and `shutdown_all` uses the core context. Exit ordering is
  preserved: drain output → persist exit under the session metadata gate → remove →
  notify → emit. The old `tauri::async_runtime::block_on` calls in `metadata()` and the
  bridge's `terminal:delete` are replaced by direct synchronous calls on blocking
  workers. The remaining `block_on` calls are a dedicated knowledge worker thread
  (pre-existing) and the sync CLI.
- **Channel exposure.** The new `core:terminal:*` aliases are unreachable by the
  renderer (`NativeCore::request` rejects `core:`), by Bun guests (`HOST_CHANNELS`
  allowlist) and by native-v2 guests (fixed method table).
- **Paths and resources.** Every `CARGO_MANIFEST_DIR` and `include_str!` was re-based
  correctly (worker `../../dist-backend`, screenocr, vec0, embedding catalog). The
  `infoPlist` path is the only config change. Config-path precedence is unchanged.
  Windows/macOS log directories match Tauri's `app_log_dir`.
- **Update policy.** The port matches `src/shared/updater.ts`: 12 h strict `<` / `>`,
  `Number()` parity for hex/bin/oct, empty string, whitespace, inf/NaN, and the search
  strings.
- **Locks and licenses (recomputed here).** Tauri and prototype locks differ only by
  three path packages, with zero registry change. All ten watchlist versions are equal.
  `global-hotkey`, `tray-icon`, `muda`, `minisign-verify`, `tar`, `flate2`, `url` and
  `tempfile` versions match the Tauri lock. Zed git packages are pinned at `c84af5b`,
  plus the async-task patch at `015845a`. All 20 linked Zed-repo crates found in the
  checkout carry Apache-2.0 only; there is no GPL editor/UI/terminal crate.
- **Signature verification and archive extraction.** Verification runs on an anonymous
  private copy before extraction: streaming for prehashed signatures, mmap for legacy,
  never artifact-sized heap. Extraction accepts only `Tezbar.app/<normal components>`
  plus tar `unpack_in`. Manifests are capped at 1 MiB, progress is throttled at 150 ms,
  and the download is cancelled through a guard. Channel changes are refused during
  Downloading/Ready.

## Evidence limits
- No cargo build or test was run; the parent's gates are relied on for pass counts. F3
  is from static field absence, not a compiler run.
- F2's `/R` semantics come from the plugin's own `nsis_args` comment ("/R: Restart") and
  its install code. The bundler's NSIS template text was not available locally (the CLI
  binary embeds it compressed).
- F4 is arithmetic plus the documented macOS `sun_path` limit. The bind failure was not
  executed.
- Test-retention JSON mappings, Windows artifacts and hashes were not re-verified
  (the parent did). The 620-case mapping shows retention, not behaviour.
- Unreviewed: M3 preparation at HEAD, GPUI runtime behaviour (no UI exists yet), and real
  installs, signing and relaunch (M11).
