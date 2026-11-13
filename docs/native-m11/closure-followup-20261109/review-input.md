# Tezbar GPUI M11: closure review of 0e6e455 (Claude Opus 5.5)

- **Tree / branch / PIN:** `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, `feat/native-gpui-app`, PIN `0e6e4558993ada1fbb49973ceb1d23e74009c623` (tree `b9119273e24b5eb0cb8e24c8ff29624addf223e2`, parent `6ffa4e5`).
- **Date:** 2026-11-09.
  - The machine rebooted at 2026-11-08 14:16, which cleared `/tmp`.
  - This review was redone from scratch. No earlier partial results of mine survived.
- **Live root state:**
  - HEAD is now `0228b4f`: two commits after the PIN (onboarding and launcher-path fixes), plus many uncommitted edits.
  - None of that was reviewed or touched.
  - `Cargo.lock` and every manifest are byte-identical between the PIN, HEAD and the working tree.
- **Review object:**
  - `git archive 0e6e455`, extracted into the private directory `/tmp/tezbar-m11-opus-closure-0e6e455/src`. It has no Git, and no checkout or worktree was created.
  - Blob-verified 3243/3243 at the start and again at the end.
  - A fresh `git archive` at the end produced the same tar SHA-256, `6815312248fde7ead17ead30831c5b6668d081a35d56d3658d30d47c6465ec5a`.
- **Scope inputs:**
  - The `/tmp` originals of both earlier reports were deleted by the reboot.
  - Their committed copies match the requested hashes byte-for-byte:
    - `docs/native-m11/recheck-9832852-root/tezbar-gpui-m11-recheck-opus-9832852.md` is `f30b9cce…6cb2` (27479 bytes).
    - `docs/native-m11/root-review-repairs/tezbar-gpui-m11-opus-c01fa31.md` is `d28bc57b…83c5` (23523 bytes).
  - I read both, plus every `recheck-followup`, `validation/20261107-m911` and `parent-completion-20261107` document cited below.
- **Skills and policy read before any build:**
  - PIN `AGENTS.md` and `docs/MACOS_SIGNING.md`.
  - `…/Tezbar APP/.agents/skills/release-guidelines/SKILL.md` and `~/.agents/skills/xerj-search/SKILL.md`.
  - XERJ was not used: the prefix is stale and index loops are not allowed. Git, `rg` and `grep` against the pinned objects were the authority.

## Verdict

- **R0–R8 are all CLOSED at implementation level** for the scope stated in the brief.
  - Independent exact reruns on PIN bytes pass.
  - The R2 RED reproduces independently with the pre-fix `build.rs`.
  - The R6 successor regenerates byte-for-byte from PIN bytes.
- **This recheck follow-up can be closed.** This is **not** acceptance of M11, a platform, signing, the updater, the cutover or the migration.
- **New findings. None of them reopens R0–R8:**
  - **N1 (Medium, design/UX, CONFIRMED by source):** a staged-payload refusal is discovered only after the irreversible quit, is never shown to the user, and repeats on every "Restart to update".
  - **N2 (Low, CONFIRMED by probe):** the record's `rustVersion` and `gpuiRev` are source literals, so admission checks labels rather than the actual compiler or GPUI.
  - **N3–N7:** Low or latent items.
  - **N8:** documentation notes.
- **Not re-verifiable now (deleted by the reboot, never committed):**
  - the root's clean PIN binary `/tmp/tezbar-m911-root-final-20261107/tezbar-clean-0e6e455` (SHA-256 `01414172…7722`) and its `clean-native-receipt.json`;
  - the 211 external proofs under `/tmp/tezbar-native-m911-validation-20261107-2sefx6og/`;
  - the two preserved dirty binaries (`063f6178…`, `6d14eeed…`);
  - the prepared Bun seed (`/var/folders/…/T/tezbar-m09-fixtures-kND6GX`) and its `/tmp` pointer.

  No commit in the last 50 on any ref mentions the clean SHA. The clean-build claim stands only as parent-reported. I made no clean claim myself: my archive builds are honestly `dirty=true`.

## Integrity (all computed programmatically)

| Check | Result |
| --- | --- |
| Archive vs PIN | 3243/3243 Git blob hashes match, modes match, 0 extra files. Start and end identical. |
| `validation/20261107-m911/evidence-sha256.json` | 96/96. These are the 96 copied proofs; the directory holds 97 files, the extra one being the manifest. |
| `parent-completion-20261107/evidence-sha256.json` | 5/5 |
| `recheck-followup/evidence-sha256.json` | 22/22. Both subdirectories are covered by their own manifests. |
| `recheck-9832852-root/evidence-sha256.json` | 24/24 |
| `root-review-repairs/evidence-sha256.json` | 69/69, size-checked |
| `compiled-inputs.json` | **427/427 equal PIN bytes** |
| 28 gate records | 28/28 log SHA-256 match. All `stable`, before equals after, 0 survivors. Receipt `gates` vs records: 0 mismatches. |
| Freeze aggregates (recomputed) | approved/initial `dc3220ad…`, successor-01 `df06d8b2…`, successor-02/validated/final-before-documentation `afcafe60…` |
| Validated tree (3121 files) vs PIN | 3118 identical. The 3 that differ are all docs (`docs/native-m09/status-icons-followup/{handoff.md,receipt.json,source-hashes.json}`). The 122 PIN-only files are all docs. **Every non-doc byte the gates compiled equals the PIN.** |
| M11 actual test lines | 29/29 present in both the normal and bench gate logs |
| Updater reuse | Each of `updater-{185,198}-final.json` lists 15 compiled inputs, 15/15 equal to PIN bytes. Both logs show 43 passed / 1 ignored. |
| Parent provenance completion | Exit 0, before equals after (`d966d10d…`), log hash matches |
| Cutover held originals | patch `159dc2fb…94a2` and inventory `6e96fedf…d233` unchanged |

Gate exit codes:
- 101 for `native-check-01`, a compile error recorded as a failed attempt.
- 101 for `red-01-registered-name`, a compile failure that is not a RED.
- 101 for `red-02-exact` and `red-03-exact`, each 0 passed / 1 failed. These are the real REDs.
- 0 for every other gate.

Counts:
- normal 460, bench 462, UI 57;
- SDK 9 in America/Los_Angeles (the full libraries ran in Asia/Almaty);
- status 6, plus core status 4 and status-port 3 (the "core7");
- pnpm 748 passed / 7 skipped (81 + 1 files);
- Node 13, Python 1.

The only compile error inside the Node log is the deliberate `missing_function()` case of the cargo-artifact toy (`/var/folders/…/tezbar-artifact-cargo-2pkaES`). It is not the product.

The ignored updater test is `manual_releases::tests::live_beta_installer_discovery_from_stable_028` ("Requires GitHub network access and a published Beta installer"). That is network discovery, not signing (see N8).

Whitespace:
- The recorded gate was `git diff --check` in a tree where the 13 new files were still untracked, so it never saw them.
- I ran `git diff --check 6ffa4e5 0e6e455 -- . ':(exclude)docs'`: exit 0.
- All 13 new files have 0 trailing-whitespace, CR and tab lines and end in LF.
- The 59 flagged lines are all in byte-exact raw logs and patches under `docs/`, as intended.

## Exact reruns (mine, PIN archive bytes) vs reused

Every Cargo command used `cargo +1.98.1 … --locked --offline -j2` with `CARGO_INCREMENTAL=0`, `RUST_TEST_THREADS=2` and a private `CARGO_TARGET_DIR`. `TEZBAR_NATIVE_SOURCE_SHA=<PIN>` was set only for product archive builds, never for the toy Git fixtures. The archive was blob-verified before and after every gate (`logs/*.archive-{before,after}.txt`).

| Rerun | Result |
| --- | --- |
| `tezbar-native --lib --features runtime-shaders`, filters `app::production_launch::`, `app::quit_budget::`, `app::m11_deadline_tests::`, `legacy_state::tests::` (TZ Asia/Almaty) | **22 passed**, 0 failed, 438 filtered (460 in total, matching the recorded count) |
| The same library, `--exact` on 6 more M11-listed tests: `host_adapter::…preview_cannot_register_production_launcher_accelerators`, `terminal_integration::…quit_refusal_retains_exact_entity_and_cleanup_can_retry`, `terminal::…both_listeners_before_create_with_metadata_once`, plus three `ai_chat` tests | **6 passed**, 454 filtered |
| `tezbar-update --lib` on 1.98.1: the R1 0555 test, the successful-relaunch cleanup test, atomic swap/rollback, and team | **4 passed**, 40 filtered (44 = 43 + 1 ignored) |
| Node: `admission-fixture`, `build`, `cargo-artifact`, `nsis-template`, `package-macos`, `theme-fixture` | **13/13** |
| Node: `provenance.test.mjs` (fixture-owned mkdtemp Git; its own SHAs; `TEZBAR_NATIVE_SOURCE_SHA` unset) | **1/1** |
| Python: `scripts/native/cutover.test.py` | **1/1** |
| Real normal and bench debug binaries, built through the archive's own `runCargo` on the product manifest | Both selected `target/debug/tezbar` |
| `--version` (the only execution) and `otool -L`, on both binaries | Fields and types unchanged. 23 lines, 0 WebKit/JavaScriptCore. |

- That is 28 of the 29 recorded M11 test lines rerun on PIN bytes.
- `core_client::command_tests::review_m9_followup_command_quit_refusal_is_specific_and_non_destructive` needs the wiped prepared Bun seed and was **reused**. I did not rebuild a seed, because that would require `node_modules` or installs.

Reused from verified receipts, not rerun:
- native normal 460 (432 not rerun by me) and bench 462;
- UI 57, SDK 9, core 7, TS 748/7, typecheck;
- Rust 1.85 core check;
- updater 43/1 on 1.85 (all) and on 1.98.1 (39 not rerun);
- Tauri compatibility, rustfmt, dependency tree, whitespace;
- the clean PIN build and metadata gate.

## Per-ID status

| ID | Status | My evidence | Residual |
| --- | --- | --- | --- |
| R0 / M3 | **CLOSED** | `legacy_state` 12/12, including `review_m11_fresh_install_defaults_but_missing_upgrade_profile_still_refuses` | The fix sits inside `bootstrap`, so it no longer depends on who creates `~/.openray`. |
| R1 / M2 | **CLOSED** for the current-user boundary | Updater 4/4, plus the exact-function probe (below) | Foreign-owner sticky parent, ACL deny-delete and real `/Applications`: UNRUN |
| R2 / H2 | **CLOSED** | Provenance GREEN 1/1. RED reproduced twice with the pre-fix `build.rs`. The archive `build.mjs` refuses before Cargo. | — |
| R3 / H1 | **CLOSED** for its stated criteria | `bundle_validation` 4/4 and `production_launch` 1/1. Rust, Node and Python capture agree on the real binaries. `production()` refuses dirty and bench. | N1, N2, N3, N4 |
| R4 | **CLOSED** | Source | N5 |
| R5 / M4 | **CLOSED** | `quit_budget` 2/2, `m11_deadline` 3/3, 6 composition tests, and a review of the real caller | N6 (test gap) |
| R6 / M1 | **CLOSED**; the held successor is NOT_APPLIED / NOT_RELEASE_READY | Python 1/1, and the successor regenerates byte-identically | N7 (latent) |
| R7 / L4 | **CLOSED** for code plus the recording adapter | Theme fixture (inside Node 13) | GPUI/AppKit appearance: UNRUN |
| R8 / L7 | **CLOSED** | Node toy 2/2, plus real product-manifest selection twice | — |
| M5 | Primary-source closure preserved | `leveldb.rs` and `leveldb_tests.rs` are blob-identical to 9832852 | — |
| M6, L5, L9, L10 | **OPEN** (unchanged) | `webkit.rs` is blob-identical. Cost of the new capture step measured (below). | — |

## Findings, ranked

### N1 (Medium, design/UX, CONFIRMED by source; not an R3 regression): staged refusal arrives only after the irreversible quit

Order today:
- `Effect::Install` (`m11.rs:144-168`) runs only `preflight_macos_install`, then calls `quit`.
- The quit then:
  - commits: chat is disposed and the chat and terminal drains run (`app.rs:574-586`);
  - shuts core down (`app.rs:661`);
  - only then calls `finish_update` (`app.rs:671`).
- `finish_update` → `install` stages into `/Applications/tezbar-stage-*` and calls `verify_staged` (`m11.rs:243-244`).

On refusal:
- `finish_update` logs `Update install/relaunch failed: {error}`.
- It sends `InstallFailed` to the **exiting** process's actor. That actor marks its status failed (`update_integration.rs:272-278`), but nobody ever sees that status.
- It relaunches the old executable (`m11.rs:215-227`).
- The relaunched process has no record of the failure, and the same release is offered again after the next check and download. Every "Restart to update" repeats the cycle: quit, run disposal, relaunch, with no explanation. The verified download is also consumed each time.

Trigger: any staged refusal. The likeliest are the transitions the handoff and `scripts/native/README.md:28-30` already say need an "explicit compatibility and validator-upgrade plan":
- schema 2;
- the GPUI rev;
- Rust policy 1.98.1;
- the planned Developer ID pin migration from `MACOS_SIGNING.md`.

Installed validators refuse all of these by design. Any packaging mistake triggers it too.

Impact: no brick and no data loss. Durable saves happen before the commit and the old bundle is left untouched. The failure is destructive in that runs are stopped, and it is repeated and silent.

Fix, either of:
- (a) Stage and run `verify_staged` before `quit` in the Install handler. Keep the `StagedMac` for the post-quit swap, and discard it on `Refused`. A refusal then reuses R1's pre-quit copy.
- (b) Persist a small marker (version, archive hash, error) that the relaunched process reads, so it can show a specific message and stop re-offering that exact artifact.

Also make the validator-compatibility (bridge release) plan an explicit gate. Today it is only prose. It is not in the successor inventory's `gates` list or in the parent receipt's `remaining` list.

### N2 (Low, CONFIRMED by probe): `rustVersion` and `gpuiRev` are literals, so the record and admission compare labels

The literals:
- `build_metadata.rs:18` hard-codes `"gpuiRev":"c84af5b…","rustVersion":"1.98.1"` into the record.
- `production()` (`:47-60`), `verify_resources` (`bundle_validation.rs:71-72`), Node `captureBuildMetadata` (`build-metadata.mjs:34`) and `planMacos` (`package-macos.mjs:97`) all compare against the same constants.

What is actually observed:
- `build.rs` never reads the `release:` line from `rustc -vV`, and never reads the gpui rev from `Cargo.lock`.
- The only observed compiler identity is `TEZBAR_NATIVE_RUSTC` (the commit). It appears only in `--version`, not in the record or in admission.

Probe:
- A toy crate includes the verbatim `build_metadata.rs` and is compiled by **rustc 1.98.0** (`88d9e12ae`).
- Its own embedded record says `rustVersion "1.98.1"` and `gpuiRev c84af5b…`, even though the toy has no GPUI.
- `production("0.3.0-beta.7")` returns `Ok(())` (`logs/rust-version-literal-probe.txt`).

Why today's builds are still truthful:
- `rust-toolchain.toml` pins 1.98.1, `build.mjs` forces `+1.98.1`, and the manifests and lock pin c84af5b.

Where it breaks:
- A toolchain override (`RUSTUP_TOOLCHAIN`, `cargo +x`), or a GPUI bump that updates the manifests but not the four GPUI literals, produces an admitted binary that claims the pinned policy.
- So "the record agrees with `--version`" proves nothing for these two fields: `--version` prints them from the record itself.

Fix:
- Have `build.rs` derive both values (the rustc `release:` line, and the `Cargo.lock` gpui `source = git+…#<rev>`) and emit them as env vars for the record macro.
- Fail the build, or force dirty, when an observed value differs from policy.
- Use one shared GPUI constant instead of four literals.

### N3 (Low, latent, fail-closed): scanner constraints in `capture()` (`build_metadata.rs:70-121`)

- Any second complete record is refused, so a universal (multi-arch) binary, which carries two identical records, would always be "Ambiguous". Today's targets are arm64 only.
- The scanner's own START constant is linked into the binary. In my build:
  - It sits at `0x403a4b7`, directly followed by the END constant (NUL), so it is skipped.
  - The real record is at `0x403aba1`.
  - There are 2 START and 2 END occurrences in total.
- A future layout that puts `{` after that constant would refuse that build. The Node scanner at assembly is identical, so assembly would refuse first and such a binary cannot ship unnoticed.
- Record both as release-builder constraints.

### N4 (Low, design): validator-version skew remains with delete-at-spawn

- `install` relies on the old app's validator. The new binary's stricter startup admission (possibly new required resources, helper pins or a new DR) runs only after `confirm_relaunch` has removed the backup (`m11.rs:257`, `native.rs:658-660`).
- The packager enforces a superset of today's Rust-required list (`package-macos.mjs` `RESOURCE_ROOTS` and helpers), which mitigates resource skew.
- But no gate runs the new binary's own admission predicate against the final signed bundle before publication.
- Add that to the native release-builder gate, as a non-GUI verify entry point or harness, or keep the backup until the new process passes admission.

### N5 (Low, R4 residuals and cleanup)

- On a failed spawn, the OS error is dropped. `.spawn().is_err()` only produces "Update relaunch failed; previous version restored", or just the rollback error if rollback also fails (`m11.rs:247-253`).
- There is still no startup sweep for crash leftovers:
  - `/Applications/tezbar-stage-*` holds a complete signed bundle during the multi-second verification.
  - `/Applications/tezbar-previous-*` is left behind when the `confirm_relaunch` cleanup fails, and it holds a launchable signed old `Tezbar.app`. That failure only produces a `log::warn!`.
- This is pre-existing, rare and bounded.

### N6 (Low, test gap): the R5 tests drive helpers, not `Owner::quit`

- The deadline tests call `quit_budget::commit` and `quit_preparation` directly (`app.rs:1925-1963`).
- My review of the real caller found the composition correct (next section). Still, no GPUI-level fixture drives `Owner::quit` to `Refused` or `ForcedAfterCommit`.

### N7 (Low, latent): cutover generator blind spots (`prepare-cutover.py:10-37`)

- An empty-to-empty change emits no hunk, and the roundtrip treats a missing file as `b''`. A zero-byte relocation would therefore be silently skipped and go undetected. The moved roots contain no zero-byte files at the PIN.
- Unified diffs carry no file modes. `prototypes/native-launcher/scripts/bundle-macos.sh` (100755) would land as 0644 in the legacy archive. The roundtrip compares bytes only.

### N8 (documentation)

- `recheck-9832852-root/README.md:16` says "one existing ignored signing test". That is historical and inaccurate: the test is network installer discovery. The parent completion receipt names it correctly.
- The recorded whitespace gate did not cover the 13 files that were untracked at the time. The committed range is clean.

## Verified, no defect found

### R3 trust boundary

- `verify_bundle` (`production_launch.rs:250-268`) always runs `verify_macos_bundle_identity` (`native.rs:490-571`) before any payload read:
  - `--verify --deep --strict`;
  - `--verify --strict -R=<pinned DR>`;
  - exactly one `TeamIdentifier` and no ad-hoc signature;
  - all four pinned helpers, each verified with its team checked;
  - exact `designated =>` equality.
- That authenticates the executable pages, `Info.plist` (special slot) and every Resources file, including `.tezbar-native-production.json` and `.tezbar-native-source`.
- `binaryMetadataSha256` therefore provides **consistency**, not authenticity. That is correct and honestly documented: a whole-file digest inside the sealed resources would be circular.
- What it does not bind is everything outside the 227-byte record. That is acceptable only because the record is truthful, which N2 qualifies.
- Bytes outside a code signature can only add a second record, which leads to refusal.
- The stage is a user-owned 0700 TempDir, so TOCTOU is limited to same-user or root writers, who could already replace the app.
- Nothing staged is ever executed: `capture` is a read-only stream and PlistBuddy only reads the plist.
- At startup, `Admission::current` compares the on-disk record with its own compiled one (`bundle_validation.rs:34`).
- `plan()` rejects any path that is not `<bundle>/Contents/MacOS/tezbar`.

### R3 behaviour

- The staged check uses the release version, `running=None` and the staged binary's own source.
- Dirty, bench, missing, ambiguous, truncated, tampered, sidecar and manifest-hash cases are refused before the swap, with the old bundle and backup preserved.
- Compiled Metal is accepted when it matches, and a mismatched mode is refused.
- `--version` keeps its fields and types: `dirty` is the string `"true"`, and `runtime_shaders` and `bench_control` are bools. It is reached before admission, so the `#[used]` and `black_box` reference keeps the record linked.

My archive binaries:

| Variant | SHA-256 | Size | Record SHA-256 |
| --- | --- | --- | --- |
| Normal | `372afb39…b895` | 222,527,832 bytes | `d4bba8d8…c551` (227 bytes) |
| Bench | `4d343978…94f9` | — | `b2d3d66c…7ef9` |

- Source is the PIN in both.
- `dirty=true` in both, and `production()` refuses them.
- The same normal record with `dirty=false` would pass. The bench record is refused even when clean.

### R5 real caller

- The settings-pending check and `commit` run inside one `this.update` (`app.rs:574-584`), so nothing can interleave between them.
- The reserve uses the same `quit_started` clock as the 8 s timer (`app.rs:526-528`).
- After a timeout, `Refused` vs `ForcedAfterCommit` is decided from the same `committed` cell (`app.rs:1914-1919`).
- Every `Refused` path resumes extensions, defers the update and resumes the updater (`app.rs:596-601`).
- `gpui_tokio::Tokio::spawn` aborts its tokio task when the GPUI task drops (c84af5b, `gpui_tokio.rs:54-71`). When the timer wins, the extension drain's `acquire_many(16)` waiter is therefore removed and cannot stall later requests on tokio's fair semaphore. I checked this specifically.
- A post-commit error still maps to `Refused` and resumes with runs already disposed. That is pre-existing M9 design and unchanged.

### R0

- `create_dir_all(parent)` was added in the FirstInstall branch (`legacy_state/mod.rs:214-226`).
- The snapshot is still taken before `create_dir_all(instance_dir)` (`app.rs:1600-1627`).
- The test calls the real `for_install` and `bootstrap`.

### R1

- `access(W_OK)` (which honours ACLs), the sticky-parent owner check, the directory/symlink check and the parent probe all run before quit (`m11.rs:153-166`, `native.rs:583-607`).
- A refusal keeps the download (`defer_update` → `InstallDeferred`).

Exact-function probe (the 1389 bytes of `preflight_macos_install`, extracted verbatim):

| Case | Result |
| --- | --- |
| Symlinked bundle | refused |
| 0555 bundle | refused |
| 0555 parent | refused |
| Wrong bundle name | refused |
| Own bundle under a sticky parent | accepted |

No probe directories were left behind and the old contents are intact.

### R2

- `build.rs:68-104`:
  - canonical own Git root;
  - tracked `apps/tezbar/build.rs`;
  - tracked in-workspace notes;
  - `--no-optional-locks` on every Git call;
  - the source SHA must be 40 hex characters (`:79-86`), so nothing can be injected into the record.
- `build.mjs:26-44` adds a root guard.

Independent RED with the pre-fix `build.rs` from 6ffa4e5 (`4dcaf609…`; the PIN's is `82f08466…`):
- Line 32: external untracked notes give `false:2`, where `true:2` is expected.
- Line 41: with the notes block removed, a workspace nested in an ignored folder of a clean outer repo also gives `false:2`.

The PIN `build.rs` passes the whole test. The test is not a mirror.

### R4

- `m11.rs:215-221` logs `{error}`.
- The swap and rollback messages carry the retained paths (`native.rs:616-639, 661-673`).

### R6

Regenerated in a disposable toy repo: an APFS clone of the archive, then `git init`, `add` and `commit` with a fixture identity (1825 tracked files; the ignored entries were all logs under `docs/`).
- The patch `63376fbb…` (5,552,910 bytes) and the inventory `b37090f4…` are **byte-identical** to `cutover-successor-post-review/`.
- The generator's own `git apply --check`, apply and 481-file byte comparison passed again.
- The relocations (282) and binary copies (52) equal the old inventory, and every source hash equals PIN bytes. There are 23 consumer edits in each.

The gates now list:
- the native signed release builder;
- the signing/feed pipeline;
- post-apply `pnpm typecheck` and `pnpm test`;
- a release-builder dry run;
- resolution of the `dev`, `tauri`, `tauri:dev` and `icons:generate` scripts.

Unchanged at the PIN:
- `package.json` still runs `pnpm build:native && tauri build`.
- `scripts/build-release.mjs:177,295` still spawn the Tauri CLI.

This listing is not an implementation of that pipeline.

### R7

- There is one helper (`app.rs:38-48`). It is called at:
  - preview init (`:1655`);
  - cold start, under `preview.theme == "system"` (`:1791-1795`);
  - recovery (`:1182`, `:1199`);
  - Settings (`m7.rs:274`).
- Production passes `"system"` (`app.rs:1574`).
- Recovery exists only with production migration, so it cannot override an explicit preview theme.

### R8

- The collector filters on all of the following, requires exactly one executable, and returns `null` in check mode:
  - a `compiler-artifact` message;
  - bin `tezbar`;
  - `profile.test === false`;
  - the realpath of the manifest.
- On the real product manifest it selected `target/debug/tezbar` for both feature sets.
- The source snapshot and provenance logic are unchanged.

### M6 input (no acceptance claim)

- Startup admission now also streams the whole executable through `capture()`.
- On a 222 MB binary that takes 0.11 s warm with a release-built scanner (0.52 s on the first run), 2.43 s with a debug-built scanner, and 0.18 s for the Node scanner.
- This adds to the existing codesign and PlistBuddy spawns and resource hashing.

## Commands and probes (exact, all private)

1. Read-only Git in the root with `GIT_OPTIONAL_LOCKS=0`: `rev-parse`, `branch`, `worktree list`, `cat-file`, `log`, `ls-tree`, `archive`, `show`, `diff --stat/--check` and `grep`.
   - One initial `git status --porcelain=v2` ran without that setting and may have refreshed the index stat cache. No content or staging changed.
   - No checkout, worktree, stash, index operation or commit in the product repository.
2. Archive: `meta/pin.tar` (`68153122…`), extracted to `src/`, verified with `probes/verify_archive.py` (`python3 -I`).
3. Hash verification: `probes/verify_evidence.py` and `probes/verify_gates.py` (output in `logs/evidence-verify.log` and `logs/gates-verify.log`).
4. Cache:
   - An APFS clone (`cp -c -R -p`) of the root `target/debug`'s non-workspace entries only: 4800 of 36689 deps, 140 of 148 build entries and 1262 of 1313 fingerprints. No `tezbar*` entries, no symlinks and no incremental data.
   - The root was only read: it was being modified at 05:03 by other activity, so it was neither touched nor rebuilt.
   - The clone was deleted afterwards.
5. Gates through `probes/gate.sh`. Records are in `logs/<name>.json`, each with a log SHA-256 and archive checks before and after:
   - `rust-native-targeted-01`
   - `rust-native-m11-extra-01`
   - `updater-r1-r4-198-01`
   - `native-normal-build-01` and `native-bench-build-01` (`probes/build_driver.mjs` around the archive's `runCargo` and `captureBuildMetadata`)
   - `node-nonprovenance-01`
   - `node-provenance-01`
   - `python-cutover-01`
6. Probes:
   - `probes/rust-capture`: the verbatim Rust `capture()` on both binaries, the 1.98.0 literal toy and the timings.
   - Python independent record extraction and a marker scan.
   - `probes/r2-red`: the pre-fix `build.rs`, plus a copy of the test with only the notes block removed.
   - `probes/r1-preflight`: the extracted function.
   - `probes/cutover-toy/out`: the regenerated successor.
   - `build.mjs --check` in the archive refused with "Cannot attest native source: Git unavailable" before Cargo, and no target was created.
7. Cleanup:
   - Removed the target clone, the probe targets and the toy repo.
   - Kept `src/`, `meta/`, `logs/`, the probe sources, `probes/cutover-toy/out/` and the two dirty evidence binaries in `bin/`. The binaries were never run except with `--version`.
   - None of the probe and test temp fixtures remain.
8. Processes:
   - None of my cargo, rustc, node or test processes remain.
   - A concurrent independent review (`/tmp/tezbar-m9c-opus-0e6e455-3343da54`) was seen in `ps` only. I did not poll, cancel or touch it.
   - PID 79164 no longer exists after the reboot, and nothing of mine interacted with any preview or profile.
   - No subagents were used.

## Limitations

- No GUI or app launch beyond `--version` of my two dirty archive binaries.
- None of the following: real profile, Keychain, clipboard, credentials, live providers, updater network, `/Applications`, certificates or signing, install, release, Windows host or remote work.
- Git init/add/commit happened only in disposable fixture repositories: the provenance test's own mkdtemp repos and my cutover toy.
- I could not verify the root's clean binary or receipt; see the verdict.
- The 432 untouched native tests, the bench suite, UI, SDK, core 7, TypeScript and Rust 1.85 results are reused receipts.
- Still UNRUN or OPEN:
  - cross-user/ACL R1 cases;
  - a real signed staged install, and old→native and native→native rehearsals;
  - O-LS, Windows/NSIS/storage;
  - manual GUI, IME, accessibility, GPU and RAM;
  - M6 timing.
- The native public release builder and feed, and the cutover, remain held. Tauri remains the product default.
- None of these findings were fixed. Repository documents and originals were not modified. The probes exist only under `/tmp/tezbar-m11-opus-closure-0e6e455/probes` and must not be copied into the repository as-is.
