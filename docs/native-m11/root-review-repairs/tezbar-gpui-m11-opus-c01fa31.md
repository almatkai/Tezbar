# Tezbar GPUI M11: fresh critical review (Claude Opus 5.5)

- **Tree / branch / PIN:** `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, `feat/native-gpui-app`, `c01fa313fb04b5b53a97780c03ce2cb1838bb352`. HEAD equals the PIN (`git rev-parse HEAD`).
- **Review object:** `git archive c01fa31 | tar -x` into a private scratch directory outside Git (`.../scratchpad/pin`). No live working-tree files were reviewed. Nothing was checked out, staged or committed. No worktree or branch was created and the index was not touched.
- **Scope:** M11 production launch, first-run migration, updater and packaging integration, plus shared M9-quit integration regressions. M9 UI/worker findings themselves are out of scope.
- **Verdict:** **NOT closable.** 2 High, 6 Medium (3 confirmed, 3 plausible) and several Low findings. Each concrete functional defect below gates M11 closure. Nothing here accepts native migration, a platform, signing or the updater.

## Findings

### M11-H1 (High, CONFIRMED by source): the macOS updater permanently leaves a full signed copy of the previous app next to the installed app
- **Trigger:** any successful native-to-native update on macOS.
- **Path and cause:**
  - `crates/tezbar-update/src/native.rs:581-598` (`swap_bundle`) creates `tempfile::Builder::new().prefix("tezbar-previous-").tempdir_in(installed.parent())` and calls `backup.keep()`.
  - `apps/tezbar/src/app/m11.rs:236` retains it on purpose ("process spawn is not proof").
  - No later owner removes it. `rg "tezbar-previous|InstalledMac|\.backup"` finds only the creator, the `rollback` and one test.
  - The test `atomic_swap_keeps_old_bundle_for_relaunch_and_rolls_back_failures` (native.rs:1028) pins the retention.
- **Consequences:**
  - Every update adds `/Applications/tezbar-previous-XXXXXX/Tezbar.app`, an older, validly signed bundle with the same `CFBundleIdentifier com.tezbar.app`.
  - LaunchServices and Spotlight register duplicate apps. Bundle-id resolution (`open -b`, login items, URL handling) can pick the old version.
  - Disk use grows without bound, about one bundle per update.
  - A signed older build stays on disk as an easy downgrade target.
- **Standard / parity:** the locked `tauri-plugin-updater 2.10.1` (`src/updater.rs:1223-1302`, local registry copy) puts the backup in a system-temp `TempDir` that is dropped, so it is deleted after install. It also `touch`es the bundle afterwards to refresh LaunchServices.
- **Fix:**
  - Give backup ownership a finite end: write a marker naming the backup. The relaunched process removes it after production admission and first-ready, or the next successful start removes `tezbar-previous-*` siblings carrying a matching marker.
  - Keep rollback for the failure path. Add a test that a confirmed relaunch deletes the backup.
  - Restore the `touch`.

### M11-H2 (High, CONFIRMED by probe): the "dirty / clean source" provenance gate is stale and fails open, so dirty builds can pass production admission's clean check
- **Path:** `apps/tezbar/build.rs:39-76`.
  - `TEZBAR_NATIVE_DIRTY` comes from `git status --porcelain` with `.unwrap_or(false)`.
  - Its `rerun-if-changed` set covers only `src`, `Cargo.toml`, `Cargo.lock`, `../../crates/tezbar-core/src` and the HEAD/branch ref.
- **Consumer:** `production_launch.rs:183` (`Admission::current`) and the resource manifest `dirty` field (`production_launch.rs:270`).
- **Trigger 1 (stale):** do a clean build, then edit any other workspace crate (`crates/tezbar-update`, `tezbar-host`, `tezbar-ui-model`, `tezbar-ui`, …) without committing, and rebuild. Cargo relinks the new code but does not re-run `build.rs`, so the binary says `dirty=false` and `source=<HEAD>`.
- **Trigger 2 (fail-open):** build without `.git`, for example from a `git archive` with `TEZBAR_NATIVE_SOURCE_SHA=<pin>` set as the brief prescribes. `git status` fails, so dirty becomes `false`. Any modified source then claims clean provenance at that SHA.
- **Probe:** toy workspace in the scratchpad (`probe-rerun/`), `cargo +1.98.1 run --offline` with a private `CARGO_TARGET_DIR`. A build-script env stamp was unchanged (`1794029845724180000`) while the dependency output changed (`1` → `2`) after editing only the dependency.
- **Consequence:** the "dirty/bench builds cannot be admitted" claim (ports-and-ownership.md, completion.md) is false for these two cases. Release provenance (release-guidelines: exact build SHA) can be misreported. The signature pin still limits who can produce an admitted bundle, so this is an integrity/provenance defect, not remote code execution.
- **Fix:**
  - Treat an unknown dirty state as dirty unless an explicit, CI-verifiable archive digest is supplied.
  - Have `build.rs` re-run on every workspace member's sources and `git/index`, or derive dirty from a content hash of the workspace inputs.
  - Add tests: an archive build without an attestation is refused, and a dependency edit flips dirty.

### M11-M1 (Medium, CONFIRMED by probe): the "usable concrete cutover patch" does not apply, and its default build would be debug-only
- **Path:** `docs/native-m11/integration/prepare-cutover.py` uses `difflib.unified_diff` with no `\ No newline at end of file` handling. When a file has no trailing newline, the next file's `--- /dev/null` header is glued onto that file's last `+` line.
- **Probes:**
  - `git apply --check --stat final-cutover-review.patch` gives `error: patch fragment without header at line 11059: @@ -0,0 +1 @@`.
  - `patch --dry-run -p1` exits 1 with 12 "No file found" skips and 1 failed hunk (`prototypes/native-launcher/assets/app.svg`).
  - A regex count finds **12** glued headers.
- **Second defect in the same patch:** the default `"build": "node scripts/native/build.mjs"` (patch lines 25830-25831) runs `cargo build` with **no `--release`** and produces no bundle unless `--local-preview --resources --output` are passed (`scripts/native/build.mjs:28-31`). After cutover, `pnpm build` would ship a debug binary and no app.
- **Fix:**
  - Generate the patch with `git diff --binary` from a scratch index, or emit `\ No newline at end of file`.
  - Verify it with `git apply --check` against the PIN and record that as a gate.
  - Point the cutover default at a release build plus the assembly pipeline.
  - Re-derive `patch_sha256` in the inventory.

### M11-M2 (Medium, CONFIRMED by source; regression vs Tauri parity): the macOS install has no elevation path and no in-place fallback
- **Path:**
  - `m11.rs:224-226` stages in `bundle.parent()`; `native.rs:416-421` (`stage_macos`) uses `tempdir_in(parent)`.
  - `swap_bundle` renames there. Any `PermissionDenied` (root- or admin-owned `/Applications`, standard user) fails.
- **Consequence:** every Restart for those users quits the app, the install fails, `finish_update` relaunches the old version (`m11.rs:200-209`), and the verified artifact is consumed. Each check prompts again. Tauri 2.10.1 handles this case with an `osascript … with administrator privileges` move (`updater.rs:1259-1291`).
- **Fix:** keep the same-volume swap when writable. Otherwise add a reviewed elevated-move path (or an explicit "move to a writable location" refusal) before quitting. Validate writability before quit so declining costs nothing.

### M11-M3 (Medium, CONFIRMED by source; needs a parent product decision): a brand-new user always lands on the "Recover saved preferences" screen
- **Path:** `legacy_state/mod.rs:130-133,178-180`.
  - With no `~/Library/WebKit/com.tezbar.app/WebsiteData/Default`, `discover_modern` returns `Missing` and `complete=false`.
  - That becomes `Prepared::Retry`, then `Err("Legacy preferences unavailable. Close the previous app, then Retry…")`.
  - Launcher stays `migration_pending` (`command_bar/mod.rs:226-238`). `start_core` refuses and shows the recovery UI (`app.rs:92-100`). The updater is not mounted (`m11.rs:17-23`).
- **Consequence:** first run (no Tauri ever installed) blocks on a misleading screen. "Retry" can never succeed. The user has to find the two-step "Start with defaults…", and core, tray readiness and the updater wait on that. Per the parent decision, Missing ≠ empty is correct for an *upgrade*. The fresh-install path, however, has no specified behaviour or copy.
- **Fix (parent decision):** separate "no legacy container for this bundle identifier and no legacy data dir" (first run) from "container exists but is unreadable or ambiguous". At minimum, show first-run wording, not "close the previous app".

### M11-M4 (Medium, PLAUSIBLE, unrun; shared M9/M11 quit composition): the 8-second timeout after the destructive stage "cancels" quit after chat and terminal runs were already cancelled
- **Path:** `app.rs:556-575`, then the poll/timer at `app.rs:577-583`, then the cancel branch at `app.rs:585-623`.
  - The timer races the whole `prepare` future, including `view.dispose_chat` and `drain_chat`/`drain_terminal` (the final drains).
  - If the timer wins after `dispose_chat`, the result is `Err("Quit save/drain exceeded eight seconds")`. The app then resumes (`resume_cancelled_quit`, `start_core`) and reports "quit cancelled", even though live runs were cancelled.
- **Consequence:** this violates the follow-up's intent that refusal leaves live chat running and no destructive step happens before the final decision. The user sees a cancelled quit with killed runs.
- **Fix:** split the budget. Once the final drains begin, quit is committed, so a timeout should proceed to core shutdown and exit, not resume. Add a regression with a final drain that stalls past the deadline.

### M11-M5 (Medium, PLAUSIBLE; behavior pinned by test): a crash-torn trailing LevelDB log record makes Windows legacy state permanently unimportable
- **Path:**
  - `legacy_state/leveldb.rs:176-206`: `take(&mut s, size)?` on a short tail gives `Corrupt`, and so does a truncated header with non-zero bytes.
  - Test `checksum_torn_log_missing_live_table_and_unknown_compression_fail_closed` (`leveldb_tests.rs:163`) pins this.
- **Consequence:** after an old-app crash or forced kill, the only way out is "Start with defaults", which loses data the old app itself would recover.
- **Standard:** from memory of LevelDB `log_reader.cc` (*not verified*; no local primary source was found), a truncated header or payload at EOF is reported as EOF, not corruption.
- **Fix:** verify against the pinned LevelDB source. If confirmed, accept EOF-truncation of the *last* physical record of the *newest* log only, with a diagnostic, and keep mid-file corruption fatal. Windows is BLOCKED, so this gates Windows enablement, not macOS.

### M11-M6 (Medium, PLAUSIBLE, unmeasured): full production admission runs on every launch, including second-instance activation
- **Path:** `main.rs:24-28` runs `Admission::current()` before `Instance::claim` (`app.rs:1593`).
  - It spawns about 12 `codesign` processes (deep/strict verify, requirement, `-dv`, 4 helpers × 2, `-d -r-`) and 3 `PlistBuddy` processes.
  - It stream-hashes every pinned resource (embedding ZIP, `node_modules/esbuild`, `@esbuild`, `dist-backend/node_modules`, …) on each start, including a hotkey or `open` that only needs to activate the running instance.
- **Consequence:** startup and activation latency regress, possibly by seconds on a cold cache.
- **Fix:** claim or activate the instance first. Production-only launch order already allows the owner to verify before paths (fine), and `Instance::claim` needs only `instance_dir` and the identity. Otherwise cache verification keyed on (inode, mtime, size, cdhash). Measure it.

### Low / non-gating observations
- **L1 (Low, Windows-blocked):** `m11.rs:244` uses `std::env::args()`, which panics on non-Unicode argv. The failure relaunch at `m11.rs:207` uses `args_os`. Use `args_os` with a lossless OsString→NSIS path. Escaping itself matches Tauri (see Verified).
- **L2 (Low, O-LS):** `mod.rs:134-143` returns `AmbiguousOrigin` whenever `<data>/EBWebView/Default/…` exists. From memory (not verified), WebView2 places its profile under `<UDF>\EBWebView\`, so actual old installs may *always* be ambiguous. Resolve this in the disposable O-LS rehearsal before Windows admission.
- **L3 (Low):** `app/m7.rs:274-283` sends `Command::AutoUpdate(v)` to the actor *before* the durable UiState save. If the save fails, the actor diverges from the persisted value until restart. Send it after a successful save, or revert on failure.
- **L4 (Low):** after a successful `RetryLegacy`/`ConfirmFreshState` (`app.rs:1156-1176`, `command_bar/mod.rs:660-679`), the imported theme is not applied. `tokens::set` and `set_window_appearance` run only at window creation (`app.rs:1757-1765`) and on the Settings Theme effect. Apply the theme in the retry-success path.
- **L5 (Low, privacy):** `webkit.rs:407-419` writes the WAL-recovery private copy (user localStorage) to `$TMPDIR/tezbar-legacy-<pid>-<n>`. A crash leaves it behind and nothing cleans it at startup.
- **L6 (Low):** a lone UTF-16 surrogate in a stored value makes `String::from_utf16` fail (`mod.rs:55`), which sends the user to permanent recovery. Chromium (`chromium-storage-area.cc:756-763`, `String::Adopt`) and WebKit accept it. Consider lossy decoding for that key only, with a key-only diagnostic.
- **L7 (Low):** `scripts/native/build.mjs:42` inventories `target/debug/tezbar` and ignores `CARGO_TARGET_DIR`. It also captures `git rev-parse HEAD`/dirty *after* the build. A stale binary can be packaged under a fresh `sourceCommit` receipt (local preview only).
- **L8 (Observation):** `package.json` `native:preview:package` (`--local-preview` without `--resources/--output`) builds but never packages.
- **L9 (Observation, parity):** the actor auto-downloads after the host +3s check (`update_integration.rs:404-412`). In Tauri, only the renderer's own conditional check triggered download (`App.tsx:426-433`); the host check (`lib.rs:2135-2138`) did not. The interval (12h), +3/+5 and single-flight otherwise match `src/shared/updater.ts:41` and `App.tsx:418-461`.
- **L10 (Observation):** the download stream has no byte cap (`native.rs:291-307`; 900s timeout only). Signature verification comes after the bytes are written, so a hostile or misconfigured feed can fill the disk. Tauri buffers unbounded too, so this is hardening only.
- **L11 (Observation):** the local-preview bundle cannot be launched by double-click. With no args, `main.rs:24` requests production admission, which fails on ad-hoc signing. That is intended, but undocumented for testers.

## Verified, no defect found (source and upstream checks)
- **Migration order:**
  - `bootstrap` runs inside `Launcher::new_with_migration` before `with_adapter_and_settings` hydrates, and recovery passes `path=None`, so persistence is disabled (`command_bar/mod.rs:217-246`, `1115-1120`).
  - Existing valid native state wins, including defaults and `importedLegacy=false` (`mod.rs:155-174`). Corrupt, symlinked or >4 MiB native state is preserved and never rewritten.
  - The commit is `write_atomic_if_absent` via `hard_link` no-replace, with the marker in the same JSON (`ui_state.rs:440-470`).
  - "Fresh state" also uses no-replace and then re-runs bootstrap (`command_bar/mod.rs:631-657`).
- **SQLite / WAL:**
  - The original is opened only as `immutable=1` and read-only after sidecar refusal (`webkit.rs:50-84`).
  - WAL recovery copies with ordinary reads into a 0700 private dir with a 64 MiB total budget, checkpoints only the copy, and re-hashes all 4 originals to require equality (`webkit.rs:394-523`).
  - VM, length, column and expression limits plus a progress budget are in place. Length is checked before the value is materialized.
- **WebKit origin:** the `origin` file is decoded as ClientOrigin = 2 × (String protocol, String host, optional<u16> port). The String has a u32 length, an is8Bit byte, then Latin1 or UTF-16 text. This matches `webkit-persistence-coders.cpp:53-110` and `webkit-core-persistent.cpp:625-669`. The check is stricter (rejects trailing bytes and null strings), so it fails closed.
- **LevelDB:**
  - CRC32C mask, 32 KiB fragmentation, VersionEdit tags 1-7/9 with deleted-before-added ordering, the live log rule `>= log || == prev`, SST footer magic, restart validation, and Snappy `decompress_len` checked before allocation.
  - Highest sequence wins and tombstones do not resurrect values.
  - The Chromium 0=UTF16 / 1=Latin1 tags apply to keys and values (`chromium-storage-area.cc:39,752-770,831-847`). The origin prefix `_http://tauri.localhost\0` matches `SerializeForLocalStorage` (`chromium-storage-key.cc:717-728`).
- **Keys:** 14 actual `UiState::KEYS`. The two caches are excluded, and both readers report `CacheRebuilt`. No fifteenth key was invented.
- **Updater actor:**
  - One actor, bounded 16/16 queues, watch-coalesced frames, a generation fence for stale progress.
  - Quiesce drops the job and fences effects before `Quiesced`. FIFO ordering of the effect channel guarantees earlier `CheckedAt` is applied before Launcher's `drain_state`.
  - On a declined quit, `defer_update` returns the artifact by generation and then sends `Resume` (`app.rs:585-588`).
  - `CheckedAt` has one owner (`settings_integration.rs:213-219`). The Launcher `update-status` branch no longer writes timestamps (`command_bar/mod.rs:1798-1805`).
  - The channel nonce comes from the M7 barrier (`m7.rs:433-438`), and `finish_channel` matches the ticket. The installer URL must equal the ManualAvailable URL exactly.
  - The HTTP client is lazy and reused (`native.rs:115-126`). Per-request 900s overrides the 15s client timeout.
- **Windows argv:** `escape_nsis_argument` is identical in logic to `escape_nsis_current_exe_arg` (tauri-plugin-updater 2.10.1 `updater.rs:1501-1531`), including quoting a genuinely empty argument. `/P /R` equals `WindowsUpdateInstallMode::Passive.nsis_args()` (`config.rs:36-45`), followed by `/UPDATE /ARGS` and one entry per argument. A null byte is refused.
- **NSIS:** `diff upstream/installer.nsi packaging/windows/installer.nsi` shows only the header comment, the WebView2 defines and the WebView2 section. `utils.nsh` and `FileAssociation.nsh` are byte-identical to upstream. It has not been compiled (O-NSIS stays open).
- **Production identity:**
  - Admission is opaque. It is not constructible from env, IPC or a feature, and bench builds are refused (`production_launch.rs:177-227`).
  - Identity is checked before HOME, `TEZBAR_*_DIR` and paths (`228-253`): codesign deep/strict, exact DR string, outer and helper Team IDs, no ad-hoc, PlistBuddy identity, executable and version, and resource manifest hashes with no symlinks and no case-colliding names.
  - Windows production is refused.
- **M9 follow-up composition:**
  - `drain_extensions` is non-destructive: it sets `closing`, refuses while an install is running, and acquires all permits (`core_client.rs:586-616`).
  - Refusal triggers `resume_extensions`, `resume_cancelled_quit`, `defer_update` and `Resume`. Live chat is untouched until `dispose_chat` in the final stage (with M11-M4 as the exception noted above).
  - `dispose_extensions` runs only after a successful prepare (`app.rs:626-630`).
  - The M11 updater barriers are preserved, and the single-instance lease is released before `finish_update`.

## Test-map reconciliation
- **`case-map.json`:** 25 historical cases (19 Mac Rust, 1 Windows-only Rust, 5 Node) map to 24 PASS and 1 UNRUN_WINDOWS_BLOCKED, plus 31 native-suite cases. This matches completion.md's counts. I did not re-run them.
- **Gaps that are not covered:**
  - Backup cleanup: a test *pins* the retention (H1).
  - Dirty-flag staleness (H2).
  - `git apply --check` of the cutover patch (M1).
  - Non-writable install parent (M2).
  - Fresh-install first run (M3).
  - Timeout after the final drains (M4).
  - Torn-tail tolerance: a test pins fail-closed (M5).
  - Admission latency (M6).
- **Root M9 focused results** (`docs/native-m09/root-repairs/focused-verification.json`, base `9230bcc`): 15 regressions, 9 SDK, 1 date-LA, 1 admission, 381 models, 2 TS worker tests, plus typecheck. As the brief says, these are focused runs only, not a before/after frozen proof or a clean-PIN binary. `source-freeze.json` reports 608 files and aggregate `c44dfaa97ae5e846dd20142d5ae15e4a5d06267905c20548488b142c1c53dadd`. **I did not recompute the aggregate**, because the hashing algorithm and file list are not in that JSON. M11 before-root gates (391/393/57/380/40/9/MSRV/745 + 7 skips/7 Node) are recorded in `completion.md`. They predate the M9 follow-up and are not evidence for c01fa31.

## Commands run (exact) and counts
1. `git rev-parse HEAD; git branch --show-current; git cat-file -t c01fa31…; git log --oneline -8 c01fa31; git status --short` (read-only).
2. `git archive c01fa31 | tar -x -C <scratch>/pin` (outside Git).
3. Read-only `sed`/`rg`/`diff`/`cat` over the archive. The XERJ skill was read. XERJ was not used: the brief says the index is stale, and the skill forbids reindex loops. Exact `rg` was the authority.
4. Upstream primary sources read: `docs/native-m11/upstream/{webkit-persistence-coders.cpp, webkit-core-persistent.cpp, webkit-storage-utilities.cpp, webkit-sqlite-statement.cpp, chromium-storage-area.cc, chromium-storage-key.cc, installer.nsi, utils.nsh, FileAssociation.nsh}`. Also the local registry `tauri-plugin-updater-2.10.1/src/{updater.rs,config.rs}`, the locked version per `Cargo.lock:4997-4998`.
5. `git apply --check --stat final-cutover-review.patch` (in the archive copy, not the repo) gave error line 11059. `patch --dry-run -p1` exited 1 (12 skipped, 1 failed). A Python `-I` regex counted 12 glued headers.
6. Cargo probe: toy two-crate workspace in `<scratch>/probe-rerun`, `CARGO_TARGET_DIR=<scratch>/probe-rerun/target`, `cargo +1.98.1 run -q --offline -p app`, run twice, with a dependency-only edit in between. That was 2 Cargo invocations. `--locked` was not applicable (the toy has no lockfile). The repository and root `target/` were never touched by Cargo.
7. One mistaken, overly broad `find / -name log_reader.cc` was started and stopped via TaskStop. It read nothing and found nothing.

## Limitations
- No Tezbar Cargo build or test, no GUI, no real profile, installer, codesign against a real certificate, network, Keychain or clipboard.
- Every platform claim stays **unproven**: O-LS (real old macOS/Windows profiles, WebKit variants, EBWebView), O-NSIS (compile and run on a disposable hive), O-signing (real certificate continuity, signed old→native and native→native update/relaunch). The same goes for GUI/IME/AX/GPU/RAM.
- The old macOS `app.tar.gz` → native path through Tauri 2.10.1 was reviewed only in that plugin's source. It has not been rehearsed.
- Windows remains BLOCKED (Pi OAuth 401, remote Codex quota until Nov 11). No provider fallback was used.
- `docs/MACOS_SIGNING.md` (PIN) was read in full. It pins `com.tezbar.app` at `/Applications/Tezbar.app`, which supports H1 (a duplicate signed bundle beside it) and M2 (writes to that parent). `.agents/skills/release-guidelines/SKILL.md` **is not in the PIN tree** (`git ls-tree -r c01fa31` has no match) and is absent from the checkout, so I could not read it. Only `AGENTS.md`'s summary of it was applied. No release, inventory, version or signing action was taken.
- M5 and L2 rest on recalled upstream behaviour, not a local primary source. Verify them before acting.
- No source edits, staging, commits, pushes or PRs. The other Opus review was not polled or touched.
