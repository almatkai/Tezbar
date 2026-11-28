# Tezbar GPUI M11: recheck of root review repairs (Claude Opus 5.5)

- **Tree / branch / PIN:** `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, `feat/native-gpui-app`, PIN `98328520812a5d0d9d950378c43019ee0bbdd77c` (tree `b60c63866501510f0bde521e6388dbad680c7956`, parent `fec3257`).
- **Live root state at review time:** HEAD had moved to `8fc0f67` (child of the PIN) with uncommitted edits in `command_bar/*`, `tezbar-ui/virtual_list.rs` and untracked files. None of that was reviewed or touched. Every file read came from pinned Git objects.
- **Review object:** `git archive 9832852 | tar -x` into the private dir `/tmp/tezbar-m11-recheck-opus-9832852.5xyn6Q/pin`. At the end it was still byte-identical to a fresh archive of the PIN (`diff -rq` clean). Probes ran only on separate APFS clones of it (`probe-src`, `apply`).
- **Original review:** `/tmp/tezbar-gpui-m11-opus-c01fa31.md`. I read the whole report (H1, H2, M1–M6, L1–L11). It is byte-identical to the preserved `docs/native-m11/root-review-repairs/tezbar-gpui-m11-opus-c01fa31.md` (sha256 `d28bc57b…83c5`, blob `246e2075…81dc`).
- **Verdict: NOT closable.**
  - One original Medium is still a **confirmed functional bug**: M3, the first-install defaults write fails in production order.
  - Two originals are only **partially** fixed, each with a residual confirmed by a probe: H2 (fail-open inside an ignored subfolder of another repository) and M2 (a bundle that is not writable passes the preflight).
  - H1 closes the accumulation, but the spawn boundary it chose adds a **Medium design gap**: the staged bundle is checked less strictly than the new app's own admission.
  - M1, M4, M5, L1–L4, L6–L8 and L11 are closed for their original defects. M6, L5, L9 and L10 stay open, as the root stated.
  - Nothing here accepts M11, a platform, signing, the updater, cutover or the full migration.

## Integrity checks (before and after)
- **20 owned files:** SHA-256 values match `owned-files.json`. The Git blob ids of the analysis copy equal `git rev-parse PIN:<path>` for all 20 (0 mismatches).
- **12 Rust inputs:** they match `compiled-source-identity.json`. The aggregate is `4e6788803e6fdb18a9f0327cfcec392e80236cf31c0c3b349ef0f51f9bb89eba`, computed with the documented `json.dumps(sort_keys=True)` algorithm.
- **Evidence files:** all 69 entries in `evidence-sha256.json` match on sha256 and size. The directory has 70 files: those 69 plus the manifest itself.
- **Gate snapshots:**
  - For `m11-stable-{normal,bench,check,build}` and `m11-updater{198,185}`, the recomputed aggregate equals `before_aggregate`, `before == after`, `stable` is true and `log_sha256` matches.
  - The stable snapshots differ from the PIN in **0 Rust/Cargo files**. Only 3 docs changed afterwards (`NATIVE_GPUI_VERIFICATION.md`, the cutover inventory, the cutover patch).
  - The updater snapshots differ from the PIN only in `legacy_state/leveldb{,_tests}.rs`, which are not tezbar-update inputs.
  - Counts: 427 normal and 429 bench, 0 failed, 0 ignored. Updater: 42 passed on 1.98.1 and 1.85.0, with 1 ignored.
- **Binary:** `/tmp/tezbar-m11-root-repair-20261107/tezbar-debug-preview`.
  - sha256 `1457899c5fdf93ad0b358c03e236070e560b51d15a5898ecfb4a52bbae1c9c54`, 220512680 bytes, unchanged.
  - The only execution was `--version`: `source fec3257…, dirty "true", preview, runtime_shaders, bench_control false, rustc 48a229ce…, gpui c84af5b`.
  - `otool -L` shows 23 lines and 0 WebKit/JavaScriptCore. It is not a clean-PIN binary and not RAM evidence.
- **Unchanged between fec3257 and the PIN:** all `Cargo.toml` files, `Cargo.lock`, `crates/tezbar-core`, `tezbar-ui`, `tezbar-ui-model` and `tezbar-host`.
- **Processes:** none of my cargo, rustc or test processes remain.

## Per-original-ID status

| ID | Status | Kind | One line |
| --- | --- | --- | --- |
| H1 | **CLOSED** (accumulation) + new R3 (Medium) and R4 (Low) | design gap; diagnostic bug | Backup removed after spawn; old bundle gone before the new app passes admission; rollback-failure path never logged |
| H2 | **PARTIAL** | functional fail-open, CONFIRMED by probe (R2) | Original two triggers fixed; an ignored subfolder of an enclosing clean repo gives `dirty=false` with the outer HEAD |
| M1 | **CLOSED** (both concrete defects) | residual docs/proof gap (R6) | Patch applies byte-exactly; cutover still not release-capable (`build-release.mjs` runs the Tauri CLI) |
| M2 | **PARTIAL** | functional gap, CONFIRMED by a real-function probe (R1) | Parent writability is probed, but a non-writable bundle dir passes the preflight and the swap then fails with EACCES at quit |
| M3 | **OPEN** | **functional bug, CONFIRMED by probe** (R0) | True first install: defaults write fails with ENOENT (`~/.openray` absent), so the user still gets the recovery screen |
| M4 | **CLOSED** (deadline-after-commit) | tradeoff objections (R5) | ForcedAfterCommit is correct; no budget reserve before commit; core grace about 0 s |
| M5 | **CLOSED** (stated scope) | upstream verified; 3 extra probes pass | Matches LevelDB 1.23 EOF semantics; stricter areas stay fail-closed; Windows UNRUN |
| M6 | **OPEN** (justified to keep open) | measurement gap | Activation trigger mostly does not apply on macOS; cold start can be measured headlessly now (see M6) |
| L1 | CLOSED (code) | Windows UNRUN | `args_os` gives an error, not a panic; NSIS wide path is lossless |
| L2 | CLOSED (code + synthetic) | O-LS UNRUN | Unique direct or EBWebView store accepted; a pair is ambiguous |
| L3 | CLOSED | — | Actor send only after the durable write; tickets monotonic; Retry re-sends |
| L4 | CLOSED; new R7 (Low) | pre-existing asymmetry | Recovery applies tokens and appearance; cold start applies tokens only |
| L5 | OPEN (as stated) | non-gating | `webkit.rs` WAL code unchanged |
| L6 | CLOSED | — | Values lossy with a key-only diagnostic; keys stay strict |
| L7 | CLOSED; Low residual R8 | not probed (documented Cargo layout) | `CARGO_BUILD_TARGET` triple directory not handled |
| L8 | CLOSED | — | `parseNative` throws before Cargo |
| L9 | OPEN observation (unchanged) | — | No actor/feed change in this diff |
| L10 | OPEN observation (unchanged) | — | Download stream unchanged |
| L11 | CLOSED | docs-only | `scripts/native/README.md` |

## Actionable findings, ranked

### R0 = M3 (Medium, CONFIRMED by probe, functional bug): the true first install still lands on the recovery screen
- **Cause:**
  - `legacy_state/mod.rs:214-221` calls `UiState::default().write_atomic_if_absent(path)` for `FirstInstall` + `Missing`.
  - `write_atomic_mode` → `temp_file(parent)` (`tezbar-ui-model/src/ui_state.rs:450,475-490`) uses `create_new` in the parent and **never creates the parent**.
  - In production, `path` is `~/.openray/ui-state-v1.json` (`command_bar/mod.rs:225-228`).
  - Before that call, production creates only `profile.data` (`app.rs:1629`) and `profile.logs` (`FileLog::open`, `app.rs:1646`). `CoreContext::new`, `NativeCore::new`, `CoreClient::new`, `SettingsStore::new` and `GpuiHost::production` have no filesystem side effects (checked). Nothing creates `~/.openray`.
- **Probe:** a private copy of the PIN with an added `#[tokio::test]` that follows production order.
  - Snapshot with `for_install`, then create data and logs, then construct `CoreContext`/`NativeCore`/`CoreClient`, then `bootstrap`.
  - Result: `RECHECK_M3 config_exists_before_bootstrap=false result=Err("No such file or directory (os error 2)")`, so the test **FAILED**.
  - Control with `~/.openray` pre-created: `Ok(false)`.
  - Log: `/tmp/tezbar-m11-recheck-opus-9832852.5xyn6Q/logs/probe-test.log`.
- **Why the root test missed it:** `legacy_state/tests.rs:291` pre-creates `config` ("this launch may now create its own directories"), which production does not do.
- **Consequences:**
  - Every true first install (macOS, and Windows with `~\.openray`) shows the recovery screen with a raw OS error.
  - After that launch, `profile.data` exists. Every later launch is classified as an upgrade (`for_install` → not `FirstInstall`) and shows the upgrade copy. The user must find "Start with defaults" (`confirm_fresh_state` creates the parent at `command_bar/mod.rs:638-647`).
  - "Retry" works only by accident, if a layout write happens to have created `~/.openray`.
- **Fix:**
  - In the `FirstInstall` branch, `create_dir_all(path.parent())` before the write. Use the existing "Could not create the native preferences directory" copy, mirroring `confirm_fresh_state`.
  - Alternatively, create `profile.config` in `run_app` right after the snapshot, next to `profile.data`.
- **Test:** the production-order case above, with no pre-created config. Expect `Ok`, file present and `imported_legacy == false`. Keep the existing missing-upgrade control.

### R1 = M2 residual (Medium, CONFIRMED by a real-function probe): a bundle dir that is not writable passes the preflight, then the swap fails after quit
- **Cause:**
  - `preflight_macos_install` (`crates/tezbar-update/src/native.rs:583-591`) probes only that the **parent** is writable.
  - `swap_bundle` renames the bundle directory itself (`native.rs:599`). On APFS, renaming a directory without write permission on it fails with **EACCES**, even within the same parent.
  - Python control in a private dir: mode 0555 → EACCES for both cross-parent and same-parent renames. Mode 0755 → OK. `access(W_OK)` = False.
- **Probe with the actual functions** (temporary test added to the private copy of `tezbar-update`):
  - On a 0555 `Tezbar.app`: `preflight=Ok(()) swap=Err("Permission denied (os error 13)")`. No leftover directories. Old contents intact.
  - Log: `…/logs/probe-update.log`.
- **Trigger:** the bundle is owned by root or another user (pkg or MDM install, or a different admin installed it), while the current user can still write `/Applications` (admin group). I could not run the cross-user case (no second account or root). It uses the same permission check as the probe.
- **Consequence:** the same as the original M2 for these users.
  - The preflight passes, the app quits, and `install` fails at `m11.rs:241`.
  - `VerifiedArtifact` is consumed, the old app is relaunched, and the only trace is a fixed log line.
  - This repeats on every update.
- **Fix:**
  - Also call `access(installed, W_OK)` (libc or rustix).
  - If the parent has `S_ISVTX`, require the user to own the bundle.
  - Return the same refusal copy before quit.
  - ACL deny-delete cases may still slip past `access`; the O-signing rehearsal stays the definitive check.
- **Test:** a 0555 fixture → preflight `Err`, then restore the mode.

### R2 = H2 residual (Medium, CONFIRMED by probe): provenance fails open inside an ignored subfolder of another repository
- **Cause:** `apps/tezbar/build.rs:52-84` runs `git rev-parse HEAD` and `git status --porcelain` in whatever repository Git finds above the workspace. It never checks that the repository's top level *is* this workspace.
- **Probe** (std-only, my private dir, the **exact PIN `build.rs`**, `cmp` identical):
  - Setup: a clean outer repo with `.gitignore: ignored/` and a workspace at `outer/ignored/archive/{Cargo.toml, apps/tezbar/{build.rs,Cargo.toml,src/main.rs}, docs/releases/0.0.1.md}`.
  - Command: `cargo +1.98.1 run -q --offline --locked -j2` with a private `CARGO_TARGET_DIR`.
  - Without `TEZBAR_NATIVE_SOURCE_SHA`: `dirty=false sha=<outer HEAD>`. With `TEZBAR_NATIVE_SOURCE_SHA=<outer HEAD>`: the same.
  - After editing the archived `main.rs`: still `EDITED dirty=false`.
- **Realism:** this repository ignores `/build/p/`, `/build/release-*/`, and `out` and `dist` at any depth. Extracting a PIN archive into such a folder inside a clean checkout yields a "clean" binary that claims the *checkout's* HEAD. That passes `Admission::current`'s dirty gate and the manifest `sourceCommit` equality if the packager uses the same HEAD. The signing pin still limits who can ship it.
- **Fix:**
  - Treat as dirty when `git rev-parse --show-toplevel` (canonicalized) is not `CARGO_MANIFEST_DIR/../..` (canonicalized).
  - Also treat as dirty when `git ls-files --error-unmatch -- apps/tezbar/build.rs` fails from the workspace root.
- **Test:** add a case to `provenance.test.mjs` with the fixture nested inside an ignored folder of a clean outer repo; expect `true`.
- **Related Low items:**
  - `TEZBAR_RELEASE_NOTES_PATH` (`build.rs:3-16`) embeds release notes from any path without affecting `dirty`. Require a tracked in-workspace path, or include a notes digest in the provenance.
  - `git status` runs without `--no-optional-locks` on every Cargo invocation (including rust-analyzer flycheck), so it can briefly take `.git/index.lock` and collide with the user's git commands.
  - The always-rerun cost shows in root logs: about 2.5 s `cargo check` (`m11-stable-check`) and about 11 s debug build (`m11-stable-build`) with unchanged sources. That is build time, not runtime RAM. Accepted tradeoff.

### R3 = H1 boundary (Medium, PLAUSIBLE design risk, UNRUN): the old bundle is deleted at spawn, but the staged bundle is checked less strictly than admission
- **Cause:**
  - `m11.rs:240-255` with `native.rs:473-487`: before the swap, the staged bundle gets only `verify_macos_bundle_identity` (codesign deep/strict, DR and team).
  - The relaunched process's `Admission::current` (`production_launch.rs:182-227`) also refuses on:
    - a dirty or bench binary;
    - a missing or mismatched `.tezbar-native-production.json` (sourceCommit equal to the binary SHA, `gpuiRev`, `rustVersion == "1.98.1"`, `dirty`);
    - required resources and their hashes;
    - Info.plist identifier, executable or version.
- **Consequence:**
  - A signed update that passes codesign but fails any admission check (a packaging or provenance mistake in a release) will not start after the update.
  - `confirm_relaunch` has already deleted the old bundle at spawn, so there is no rollback, and the updater cannot run.
  - The only way out is a manual reinstall.
  - Deleting at spawn matches Tauri, but Tauri had none of these extra startup refusals.
- **Fix (either):**
  - (a) Before the swap, run admission-equivalent checks on the staged bundle: `verify_resources(staged/Contents/Resources)` against the staged binary's embedded `--version` source, dirty and bench values, plus the PlistBuddy identity, executable and version (version equal to `release.version`).
  - (b) Keep the backup until the new process passes admission: a marker naming the backup, plus a startup sweep of matching `tezbar-previous-*` siblings.
  - (a) is cheap and keeps the legacy spawn boundary honest.
- **Test:** a staged fixture whose manifest says `dirty=true`. Install must be refused before the swap, with the old bundle intact.

### R4 = H1 diagnostics (Low, CONFIRMED by source)
- **The recovery path is built but never recorded:**
  - `native.rs:600-623,645-656` build the "backup retained at …" and "rollback files preserved at …" messages.
  - `finish_update` (`m11.rs:210-219`) drops `result`'s error and writes a fixed line.
  - Fix: log `{error}` (it contains paths only).
- **Residuals:**
  - A crash between the rename (`native.rs:599`) and `confirm_relaunch` (`m11.rs:253`) leaves `/Applications/tezbar-previous-*`.
  - A crash during staging leaves a partial `/Applications/tezbar-stage-*`.
  - There is no startup sweep. Both are rare and bounded.
  - A `set_modified` failure (`native.rs:610-623`) forces a full rollback, while Tauri ignores a failed `touch`. That is stricter but acceptable.
- **Verified OK:**
  - The staged `TempDir` is removed when `install_over` ends.
  - A successful rollback deletes the backup, including `failed.app`.
  - A logger is installed (`app.rs:1648`), so `log::warn!` reaches `FileLog`.
  - The instance lease is released before spawn.
  - Real `/Applications` acceptance remains UNRUN.

### R5 = M4 tradeoffs (Low, PLAUSIBLE)
- **No budget reserve before commit:**
  - The single 8 s timer starts before the non-destructive stages (`app.rs:510`), and the commit point (`app.rs:561`) does not check the remaining budget.
  - Slow preliminary drains, for example a chat settle at 7.9 s, therefore lead to dispose followed by an almost immediate ForcedAfterCommit with "final writes may be incomplete".
  - The original fix asked to split the budget.
  - Suggestion: refuse without side effects if less than a reserve (for example 3 s) remains before `committed.set(true)`, or give the final drains their own fixed budget after commit.
  - Test: a preliminary stage that runs to deadline−ε → `Refused`, with no dispose.
- **Core grace is about 0 s:** after a forced commit, core's grace is `8 s − elapsed` (`app.rs:643`). Core shutdown in that path is effectively an immediate supervisor kill. Not a bug, but "remaining core 8 s budget" means close to zero.
- **Resume after commit:** errors after commit, and the post-check "Settings changed while quitting" (`app.rs:574-577`), still resume via `revive_after_quit` with runs already stopped. That is consistent with the M9 retry-persistence design. The copy says "quit cancelled" without saying runs were stopped.
- **Unverified hypothesis:** the forced path still calls `finish_update` (install and spawn) while the old core may still be alive (zero grace). That could mean two cores on one profile for a short moment. Not probed.

### R6 = M1 residual (docs/proof gap)
- **Verified:**
  - `git apply --check` exits 0, and an actual apply in a private copy exits 0 (481 files).
  - Of the 282 relocations: 63 non-renderer text destinations are hash-identical to the inventory sha256; 167 renderer rewrites keep their CRLF and trailing-newline state; all sources are emptied; the 52 binary copies stay a separate step.
  - The `package.json` build becomes `node scripts/native/build.mjs --release --local-preview`. It fails before Cargo without `--resources/--output` and produces only an ad-hoc private assembly.
  - Tauri stays the default and the patch is NOT APPLIED.
- **Gap:**
  - After the patch is applied, `scripts/build-release.mjs:174-186` still runs `@tauri-apps/cli/tauri.js build` against a relocated `src-tauri`.
  - The `dev`, `tauri`, `tauri:dev` and `icons:generate` scripts still call Tauri.
  - The inventory's gate list does not name a native release builder or signing/feed pipeline as a gate.
  - Add that gate, plus a post-apply smoke test (`pnpm typecheck`/`pnpm test`, a release-builder dry run), not just `--check`.
- **Latent generator hazard:**
  - `str.splitlines(True)` also splits on `\x0b \x0c \x1c-\x1e \x85    `. One tracked file (`crates/tezbar-core/src/knowledge/fixtures/parity.json`) contains U+0085; it is not in the current patch.
  - Split on `\n` only, and gate on a round trip (apply in scratch, compare every changed file to the generator's output).

### R7 = L4 observation (Low, pre-existing, no GUI run)
- On cold start (`app.rs:1794-1802`), only `tokens::set` applies the saved theme. Window appearance stays at the system setting (`app.rs:1659`).
- Recovery (`app.rs:1160-1175,1188-1203`) and Settings (`m7.rs:273-286`) apply both.
- Fix: extract one `apply_theme` helper (it is currently duplicated three times) and call it on cold start too.

### R8 = L7 residual (Low; documented Cargo layout, not probed)
- `build.mjs:24-25,65` uses `<target_directory>/<profile>/tezbar`. With `CARGO_BUILD_TARGET` or `build.target` set, Cargo writes to `<target>/<triple>/<profile>/` instead, so a stale binary at the non-triple path can be attested.
- The embedded `(source, dirty)` cannot tell apart two dirty builds of the same HEAD.
- Fix: take the `executable` from `cargo build --message-format=json`, or refuse when a build target is configured.

## M5 verification (primary source)
- **Upstream:** I fetched `https://raw.githubusercontent.com/google/leveldb/1.23/db/log_reader.cc` (sha256 `8138df3a296a64e17138063b2b431834d615d58edf0196752077a57c9c082c53`, 274 lines) into an isolated directory and treated it as data.
  - `ReadPhysicalRecord`, lines 190-212: a truncated header at EOF returns `kEof`.
  - Lines 223-233: when `kHeaderSize + length > buffer_.size()`, it is "bad record length" corruption if `!eof_`, and `kEof` if `eof_`.
  - Lines 244-255: checksum mismatch is reported as corruption.
  - `ReadRecord`, lines 144-151: `kEof` inside a fragmented record clears it without reporting corruption.
- **Native mapping:**
  - `offset == len && data.len() < 32768` matches upstream `eof_` (the last read returned less than `kBlockSize`).
  - A pending fragmented record at EOF is tolerated.
  - A payload that overruns a full block is `Corrupt` (same as "bad record length").
  - A CRC mismatch is `Corrupt`.
- **Stricter than upstream** (fail-closed, documented, acceptable): a non-zero trailer in a full block (upstream skips it, and the writer only writes zeros there), a torn tail in an older live log, a torn MANIFEST tail, and a CRC mismatch (upstream with `paranoid_checks=false` drops the record and continues).
- **Private probes, all PASS:**
  - Lone `kFirst` at EOF → complete, theme `dark`, `RecoveredLogTail`.
  - Final record with a CRC mismatch and intact length → incomplete, `[Corrupt]`.
  - Full first block, then a valid record plus a torn header in a short second block → complete, theme `white`, `RecoveredLogTail`.
- The root's existing torn-tail assertion change follows the upstream semantics. Windows remains UNRUN.

## M6 assessment (disposition justified, with objections)
- **Identity before environment:** keeping identity/resource admission before any environment or profile access is consistent with `production_launch.rs` and the spec. I agree it should not be weakened on unmeasured latency.
- **The original trigger mostly does not apply on macOS:**
  - LaunchServices activation of a running app (Dock, double-click, `open`) sends a reopen event to the running process (`application.on_reopen`). It does not run `main`. This is standard LaunchServices behavior; I did not probe it here.
  - Hotkeys run in-process.
  - `native_v2_cli::dispatch` runs before admission.
  - What remains is cold start (login item or boot), direct exec and `open -n`. Windows production admission is refused outright.
- **Measurement does not need to wait for O-signing.**
  - The dominant costs, `codesign --verify --deep --strict` (12 codesign spawns: 2 bundle verifies, `-dv`, 4 helpers × 2, `-d -r-`; plus 3 PlistBuddy) and streamed hashing of every pinned resource, can be timed headlessly on an ad-hoc `--local-preview` bundle with real resources. The embedding ZIP alone is 23.85 MB.
  - Only the DR/team checks differ on an ad-hoc bundle.
  - Recommend recording warm-cache numbers and bundle bytes now. No numbers are claimed here.

## Verified, no defect found
- **M2 refusal path:** `defer_update` → `return_install` → actor `InstallDeferred` restores `ready` (`update_integration.rs:263-270`). The preflight runs before `quit`.
- **L3:**
  - The `AutoUpdate` send happens only after the `save_settings_ui` write returns `Ok` (`m7.rs:350-377`).
  - Tickets come from `barrier.begin` and are monotonic. A late, older ack cannot override a newer one.
  - A failed send marks the effect failed. Any later successful UI save, or `RetryPersistence`, re-sends the pending choice.
  - Minor copy issue: a failed actor send reports the save as failed even though it was durable.
- **L6:** WebKit queries by allowlisted keys and LevelDB decodes keys with the strict `chromium_string`. Only values use `utf16_value` (lossy, with a `RepairedEncoding` diagnostic naming the key only).
- **L1:** `escape_nsis_wide` uses the same quote/backslash rules as before. Surrogates are preserved, null bytes and >32767 code units are refused, and `main` returns an error instead of panicking.
- **M4 core semantics:** `committed` is set synchronously immediately before `dispose_chat`, inside the same update. The pre-commit timeout still gives `Refused`. The two deadline tests exercise the production helper (not the App composition).
- **Docs:** the `docs/NATIVE_GPUI_VERIFICATION.md` addition matches the receipt counts and keeps M6 and L5 open. The receipt says `independent_closure: false`.

## Commands and probes (exact, all private)
1. Read-only git in the root: `rev-parse`, `branch --show-current`, `worktree list`, `cat-file`, `log`, `diff`, `show`, `ls-tree`, `archive`, `hash-object` on analysis-copy files.
   - One `git status --porcelain=v2` at the start. Git may refresh the index's stat cache on its own; no content or staging changed.
   - No checkout, worktree, stash, commit or index operation.
2. `git archive` of the PIN and of `fec3257` into `/tmp/tezbar-m11-recheck-opus-9832852.5xyn6Q/{pin,base}`, plus APFS clones `probe-src` and `apply`.
3. Hash verification scripts (`python3 -I`) for owned files, evidence, gate aggregates and the snapshot-versus-PIN comparison.
4. `git apply --check --stat` and `git apply` of the cutover patch in `apply/` (not a repository), then byte checks of the relocations.
5. H2 toy probe: 1 `generate-lockfile` and 3 `cargo +1.98.1 run --offline --locked -j2` in `probe-h2/` with a private target.
6. Warm cache: `cp -c -R` of the non-workspace `deps/build/.fingerprint` entries (12318/349/3391) from the read-only `/tmp/tezbar-m9-c01-validation-Rffw07/target/debug`. No incremental data, no symlinks, and no workspace (`tezbar*`) output was copied, so all 6 workspace crates were rebuilt.
7. `TEZBAR_NATIVE_SOURCE_SHA=<PIN> CARGO_INCREMENTAL=0 cargo +1.98.1 test --locked --offline -j2 -p tezbar-native --lib --features runtime-shaders recheck_ -- --test-threads=2 --nocapture`: 4 passed, 1 FAILED (the M3 bug), 427 filtered.
8. The same toolchain and flags with `-p tezbar-update --lib recheck_`: 1 passed (the M2 probe), 43 filtered.
9. Python filesystem-permission control in `probe-m2/`.
10. One curl of the public LevelDB 1.23 `log_reader.cc` into `upstream-dl/`.
11. Process check and final `diff -rq` of the analysis copy against a fresh archive.
12. The skills `/Users/almat/.agents/skills/xerj-search/SKILL.md` and `…/Tezbar APP/.agents/skills/release-guidelines/SKILL.md`, PIN `AGENTS.md` and `docs/MACOS_SIGNING.md` were read in full. XERJ was not used (the index is known stale and re-index loops are not allowed); git and rg were the authority.

## Limitations
- No GUI, app launch (beyond the binary's `--version`), real profile, Keychain, clipboard, credentials, live provider, updater network, `/Applications` change, real certificate or signing, NSIS, Windows host or remote work.
- The UI57, models381, TS746 and Core/Host/worker results were reused from receipts as the brief instructs, not rerun. The full 427/429 matrix was not rerun; I verified the root's receipts, logs and aggregates instead.
- The M2 cross-user case and R3, R5 and R8 are inferred from source and probes, not reproduced end to end. Real `/Applications` permissions, standard-user install, signed old-to-native and native-to-native update rehearsals, O-LS (real WebKit/EBWebView layouts) and Windows ConPTY/NSIS stay BLOCKED or OPEN. Physical GUI/IME/AX/GPU/RAM was not tested.
- No source edits were made to the root, and none of these findings were fixed. The probe tests exist only in `/tmp/tezbar-m11-recheck-opus-9832852.5xyn6Q/probe-src` and must not be copied into the repository as-is. No push, PR, release, install or Actions run. Other M9/M810 reviews were not polled or touched.
