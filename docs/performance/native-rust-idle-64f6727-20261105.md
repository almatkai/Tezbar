# Native Rust hidden-idle measurement — 64f6727

Latest result, 2026-11-05: **121.925 MiB median total physical footprint**, 121.785–122.300 MiB over **60.001515 seconds**, after 120.119 seconds of warmup. Average CPU: **0.3670% of one core**. RSS median: 210.328 MiB.

This is an actual optimized **instrumented release LOCAL PREVIEW** using the pre-existing `desktop-bench` feature and a fresh empty synthetic home. It is not an ordinary production distribution, and is not a matched comparison with installed beta.7's populated profile. No “5×” claim follows from this result.

Selected tree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`. Branch: `feat/native-rust-agent`. Measurement source SHA: `64f67278b2fc4b4861770c4a482ecfd6893efebd`.

## Attempt 1 — ordinary release blocked before launch

The original report below is retained verbatim as historical evidence, also saved at `build/release-local-preview-idle-64f6727-20261105/desktop-bench/original-blocked-report.md`. Its conclusion that a new identity exception was needed is superseded: the approved continuation identified an existing opt-in runtime test seam. No new exception or policy change was made. The original build log/config/result/checksums remain byte-for-byte unchanged.

<details>
<summary>Original blocked-build report (08:44 UTC)</summary>

# Native Rust idle benchmark — blocked before launch

Date: 2026-11-05. Mac: Apple Silicon arm64, macOS 26.3 (25D2125).
Selected tree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`.
Branch: `feat/native-rust-agent`; source HEAD: `64f67278b2fc4b4861770c4a482ecfd6893efebd`.

## Result

**No valid idle memory measurement was obtained.** A real local ordinary release build failed before producing an executable/app because the unchanged build script rejects a distinct compiled identifier. No preview process was launched. Footprint, RSS, CPU, per-process costs, warmup and the requested 60-second sample are **unavailable**, not zero.

Native Swift helpers, the isolated Extension Worker bundle and Vite production frontend built successfully. Cargo then exited through Tauri with exit code 1:

```text
thread 'main' (...) panicked at build.rs:28:10:
release version and app identity must agree: "The build's release channel does not match its application identity"
```

`src-tauri/build.rs:16–28` reads the generated `TAURI_CONFIG` identifier and calls `release_channels::default_channel`. `src-tauri/src/release_channels.rs:72–77` only accepts `com.tezbar.app`. `build.rs:29–33` additionally requires product name `Tezbar`. The generated preview override uses product name `Tezbar` to satisfy the name guard but a distinct identifier `com.tezbar.performance.idle.s64f6727.d20261105` for single-instance isolation; the identifier guard rejects it. There is no existing local-preview exception in these guards.

Launching a production-identifier executable beside the running installed beta.7 would not meet the required isolation: the ordinary single-instance plugin uses the compiled context identifier. Changing only Info.plist would not solve this. No build-script/config-codegen bypass, instrumentation feature, source modification or signing-policy change was used. A separately scoped local-preview identity exception is needed before this benchmark can proceed under the requested constraints.

## Build attempted

Run from the selected tree (no branch/worktree switch):

```sh
node scripts/build-native.mjs
CARGO_BUILD_JOBS=2 RUSTUP_TOOLCHAIN=1.98.0 TEZBAR_MACOS_SIGNING_IDENTITY=- \
  node node_modules/@tauri-apps/cli/tauri.js build --ci \
  --config build/release-local-preview-idle-64f6727-20261105/tauri-local-preview.json \
  --bundles app -- --locked
```

Ordinary optimized release was requested: no debug/dev server, no `desktop-bench`, no new optimization flags. Tauri's normal `custom-protocol` feature was enabled in its compiled dependency (see `result.json`). The override requests only `.app`, disables updater artifacts, selects ad-hoc identity from the existing explicit LOCAL PREVIEW policy, and includes the declared native helpers. The production config, certificate pin and updater public key are unchanged. `scripts/build-release.mjs` was not executed; no updater private key was read or requested. No DMG, updater signing, Windows build, GitHub build, push, PR, install or publication was attempted.

## Intended measurement conditions (not executed)

Fresh temporary profile, `hasCompletedOnboarding:true`, isolated `Control+Alt+Shift+F19` binding; no copied credentials, extensions, models or Knowledge roots. Hidden launcher, ordinary core/background services, actual system home and normal startup file scan; wait for readiness and scan stabilization, then collect 60 seconds of descendants plus responsibility-owned XPC processes. Primary metric would be summed physical footprint, with RSS and interval CPU secondary, plus footprint/vmmap verification. This would represent fresh-profile hidden-launcher idle, not the user's populated profile after indexing/chatting. No claims about Bun, models or actual runtime process costs can be made without a launch.

## Evidence and cleanup

Evidence directory (ignored generated output):
`build/release-local-preview-idle-64f6727-20261105/`

- `build.log`: actual failed build, including successful frontend build and guard panic.
- `native-build.log`: successful native-helper build.
- `tauri-local-preview.json`: exact generated override.
- `reproduce-build.sh`: commands; `result.json`: machine-readable blocker, source/config hashes and feature evidence.
- `SHA256SUMS`: hashes of the evidence files.

No app or executable exists at `src-tauri/target/release/bundle/macos/Tezbar.app` or `src-tauri/target/release/tezbar`; therefore no app/binary hash is available. Source/config hashes are recorded in `result.json` and verified byte-for-byte against source HEAD. Tracked files were clean after the build, before adding this report. Historical reports are untouched; no commit was made.

No benchmark profile was created, no clipboard contents were inspected/copied/logged, no application was launched, and no process was signaled. Consequently there are no owned helper survivors or temporary clipboard stores to scrub. Installed Tezbar PID 6376 and all supplied helper PIDs (6377, 6378, 6379, 19468, 6381) were verified still present at completion. No installed app, user profile, Keychain, Vault, TCC, clipboard or system setting was changed.

XERJ indexing was attempted but failed with its own RSS memory circuit breaker (`785 MB >= 768 MB`); the existing index was incomplete, so current source was verified with `rg`/direct reads.

</details>

## Attempt 2 — existing-feature release built; first readiness attempt failed

The unchanged compiled guards require `com.tezbar.app` and product name `Tezbar`. The build override was corrected to these supported values, with `--features desktop-bench`, ordinary Tauri `custom-protocol`, optimized release, `--locked`, `CARGO_BUILD_JOBS=2`, Rust `1.98.0`, `--bundles app`, `createUpdaterArtifacts:false`, declared native helpers, and explicit local-preview ad-hoc identity `-`. Build succeeded in **1m 38s**. The native helpers from the original successful native build were packaged; frontend/Extension Worker rebuilt successfully. No updater private key was read/requested; `build-release.mjs` was not run.

The first runtime attempt reached the socket before the main window existed. This measurement harness initially treated `main window missing` as fatal. That attempt produced **no valid idle sample**. It was corrected in the ignored local harness to retry this expected startup response, matching the existing `desktop.py` readiness behavior. No app source was changed. Archived evidence: `build/release-local-preview-idle-64f6727-20261105/desktop-bench/attempt-1-startup-race/`.

That failed runtime host, PID 90391, was terminated only after matching its exact staged executable and kernel start identity (5890463516331); its socket quit also arrived before a window existed. A subsequent responsibility audit returned no owned processes. Its private fixture clipboard stores were scrubbed. The original staged app/profile remains at `/private/var/folders/7v/_tm0w_59693180wdf89zpwmh0000gn/T/tezbar-idle-64f6727-eig5nnlp/`. The installed app's process identities and file hashes remained unchanged.

## Attempt 3 — valid isolated hidden-idle release measurement

### Build, identity and profile

`src-tauri/src/lib.rs` calls `desktop_bench::validate_isolation()` after `generate_context`, then sets the runtime identifier to `com.tezbar.performance` before registering the single-instance plugin. With this existing feature, `CorePaths.home_dir` uses `TEZBAR_BENCH_HOME`. Compiled config identity remains `com.tezbar.app`; `build.rs`, release-channel rules, signing policy and tracked config are unchanged.

The complete built app was copied to our own marked OS-temp fixture. Only this staged copy's plist was renamed and assigned `com.tezbar.performance`. All eight Mach-O files/native resources, any nested bundles, and the app were ad-hoc sealed; `codesign --verify --deep --strict` and each native-resource check passed. All four declared helpers exist in the staged resources. No distributed or installed app was re-signed.

- App preserved: `/private/var/folders/7v/_tm0w_59693180wdf89zpwmh0000gn/T/tezbar-idle-64f6727-bgt7baje/Tezbar Idle Preview.app`.
- Built binary SHA-256: `6517fab853f99310d993f1de64f4f1a3f6cd55288400f2adf2f178db268743ad`.
- Measured staged binary SHA-256: `4e1a0d0d595efe5709fc062d10ee540e47c925786b4d1ad9cc8a0d7d8f8c8d0e`. Re-sealing explains the different hash.
- Profile preserved: `/private/var/folders/7v/_tm0w_59693180wdf89zpwmh0000gn/T/tezbar-idle-64f6727-bgt7baje`; marker `.tezbar-desktop-benchmark`.
- Full staged-file SHA-256 manifest: `staged-app-sha256.json`.
- `TEZBAR_DATA_DIR`, `TEZBAR_CONFIG_DIR`, `TEZBAR_BENCH_HOME`, `TEZBAR_BENCH_SOCKET` are absolute canonical paths under `/private/var/folders/7v/_tm0w_59693180wdf89zpwmh0000gn/T/`; `TMPDIR` uses that same OS-temp prefix. Exact values are in `result.json`.
- Empty synthetic Desktop/Documents/Downloads/Applications; zero seeded documents. No credentials, provider config, user extensions, model files or Knowledge roots were copied. `hasCompletedOnboarding:true`, `raymesHotkey:Control+Alt+Shift+F19`.
- Normal core, startup app/file scan, background refresh and clipboard observer remained enabled. No provider/MCP/model requests, downloads, UI JavaScript, osascript, privacy settings or Vault/Keychain operations were invoked. Only socket `status`, `hide`, `quit` operations were used.
- Preflight checked running bundle IDs and executable-bundle identities; another actual `com.tezbar.performance` would have caused refusal, with no contact/termination.

Host: Apple Silicon arm64, macOS 26.3 (25D2125), 24 GiB RAM, 15 logical CPUs. Installed beta.7 remained running concurrently. At host-state capture, system swap was 4,416.50 MiB used; this was not a pristine unloaded machine. Physical footprint includes compressed accounting; RSS is reported separately and may overlap shared mappings.

### Readiness, stabilization and timing

Launch: `2026-11-05T08:53:25.825388+00:00`. Native core `phase:ready`, `rendererLoaded:true`, generation 1 reached in **1.100 seconds**, then `hide` succeeded. Readiness remained ready/loaded after sampling.

Startup warmup: **120.119 seconds**. Existing launcher code schedules its normal initial scan after five seconds. We waited at least 120 seconds and required the last 30 sampled CPU intervals to average below 2% with p95 below 5% of one core. Observed final stabilization window: 31.283 s, mean **0.3343%**, p95 **0.8691%**. This establishes measured CPU stabilization; no extra launcher-status/Knowledge request was sent through the test socket.

Timed sample: `2026-11-05T08:55:25.946255+00:00` through `2026-11-05T08:56:25.995062+00:00` (13:55:25.946–13:56:25.995 Asia/Almaty). **61 snapshots**, approximately once per second, covering **60.001515 s** between the first/last sample timestamps. No build or test runner was launched during this timed measurement.

### Actual process accounting

Primary metric: summed `proc_pid_rusage(..., RUSAGE_INFO_V4)` physical footprint, in bytes in JSON and MiB here (1 MiB = 1,048,576 bytes). Every sample includes the host, recursive descendants, and processes whose `responsibility_get_pid_responsible_for_pid` equals the host PID. The four XPC helpers all had PPID 1 and responsible PID **90933**; a PPID-only tree would miss them. The same five process/start identities were present in every timed snapshot. Each row is also the corresponding per-role total.

| PID | Process / role | Footprint min MiB | Median MiB | Max MiB | RSS min / median / max MiB | Mean CPU, one core |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 90933 | host | 33.579 | 33.610 | 33.610 | 98.719 / 98.922 / 98.953 | 0.1912% |
| 90937 | com.apple.WebKit.GPU | 16.032 | 16.032 | 16.048 | 26.688 / 26.703 / 26.719 | 0.0585% |
| 90938 | com.apple.WebKit.Networking | 7.126 | 7.126 | 7.141 | 12.203 / 12.203 / 12.219 | 0.0171% |
| 90939 | com.apple.WebKit.WebContent | 62.001 | 62.157 | 62.516 | 65.281 / 65.344 / 65.938 | 0.1003% |
| 90941 | com.apple.audio.SandboxHelper | 3.000 | 3.000 | 3.000 | 7.125 / 7.125 / 7.125 | 0.0000% |
| **All 5** | **Summed each sample** | **121.785** | **121.925** | **122.300** | **210.234 / 210.328 / 210.906** | **0.3670%** |

CPU is interval process user+system time converted with the machine's Mach timebase. Total CPU time: **0.220234750 s** / 60.001515 wall seconds. One-second total CPU min/median/max: **0.1534% / 0.2130% / 3.6296%**. Interrupt wakeup delta: 2487; idle wakeup delta: 0. No Bun, llama, extension worker or embedding-model process was observed at startup or during the sampled run. One-second enumeration can miss a very short-lived process; this is not tracing of every process spawn.

After the timed interval, `footprint -p PID -f bytes --noCategories` and `vmmap -summary PID` succeeded for **all five** identities. Summed auxiliary `phys_footprint`: **127,733,216 bytes = 121.816 MiB**, consistent with the timed range. Sequential tool snapshots are not simultaneous. Footprint's region-total headline differs slightly from its auxiliary `phys_footprint`; the latter is the primary cross-check. All individual outputs are retained without process arguments or memory-content dumps.

### Cleanup and validation

The successful run quit through its own socket after rechecking the exact host path/start identity. `quit` succeeded. **No signals** were required for this run. The host and all four owned XPC identities exited; both the tracked-identity check and subsequent responsibility scan found **zero survivors**. Only this marked fixture's clipboard store/images were scrubbed using the existing `scrub_fixture_clipboard`; the measurement harness did not inspect, copy, hash or retain clipboard contents in evidence. The app’s normal observer stayed enabled. Fixture logs were deleted without reading them. The app and numeric measurement evidence remain available.

Installed beta.7 PID 6376 and supplied helpers 6377/6378/6379/19468/6381 have identical executable paths and kernel start identities before/after. SHA-256 of `/Applications/Tezbar.app/Contents/Info.plist`, `MacOS/tezbar` and `_CodeSignature/CodeResources` also match before/after. No installed app, Tezbar profile, Vault/Keychain, TCC or privacy setting was modified. Installed-profile data was not inspected.

Source SHA is still `64f67278b2fc4b4861770c4a482ecfd6893efebd`; no branch/worktree switch or commit. All tracked source/config files remain unchanged; policy hashes match both the pre-build snapshot and source SHA. `git status --short` shows only this untracked report. Original failed-build evidence is unchanged. `pnpm typecheck` passed; `pnpm test` passed (**744 passed, 7 skipped; 80 files passed, 1 skipped**). These checks ran after shutdown, outside the measurement interval.

### Evidence and interpretation limits

Evidence directory: `build/release-local-preview-idle-64f6727-20261105/desktop-bench/`.

`build.log`, `reproduce-build.sh`, `tauri-local-preview.json`, `compiled-features.json`, `policy-before.json`, `policy-after-build.json`, `policy-after.json`, `baseline.json`, `verification.json`, `host.json`, `staging-sign.log`, `staged-app-sha256.json`, `measurement-progress.jsonl`, `warmup.json`, **`samples.json`**, **`result.json`**, `spotcheck-summary.json`, all `footprint-PID.txt`/`vmmap-PID.txt`, installed before/after identities/hashes, typecheck/test logs and `SHA256SUMS` are preserved. The ignored `measure.py` records the exact harness. The original blocked build is in the parent evidence directory; the first feature startup attempt is archived separately.

`desktop-bench` adds a small test socket listener, runtime identifier override, renderer-loaded flag/reply state and synthetic-home hook. No performance optimization or production policy change was introduced. These are real measurements of this instrumented release under this empty-profile hidden-launcher condition; they do not establish exact ordinary production RAM, populated-profile/indexing/chat/model RAM, a steady-state baseline under other system memory pressure, or a 5× comparison with installed beta.7. This is one run, not a repeated cross-build study.
