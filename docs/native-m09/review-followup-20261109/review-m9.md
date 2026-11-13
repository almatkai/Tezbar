# Tezbar GPUI M9 closure review at 0e6e455 (F1–F8 and the new F4 status icons)

Reviewer: Opus 5.5, acting as the review sub-agent. I did all the work myself, with no subagents, delegation, repairs, staging or commits.
Pin: `0e6e4558993ada1fbb49973ceb1d23e74009c623` on `feat/native-gpui-app`, worktree `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`. Review date: 2026-11-09.
Scope:
* M9 only.
* The residual findings F1–F8 from `tezbar-gpui-m9-d1-d15-opus-fec3257.md` (SHA256 `ab7c879e…d5477e6f`, verified).
* The original D1–D15 from `tezbar-gpui-m9-opus-0513321.md` (`f7206c9e…04296f4`, verified).
* The new F4 native status-icon implementation.

Evidence labels:
* **[S]** source trace in the verified archive.
* **[P]** a probe I wrote and ran.
* **[R]** a registered root test I re-ran in my own build.
* **[E]** recorded evidence I hash-verified but did not re-run.

## 0. Verdict

**No High or Medium functional defect remains in M9 scope. M9 is still not accepted, F4 is not closed, and no E row changes status.**

* **Closed with executed headless evidence:** F1/D13, F2/D12, F3, F7 and F8.
  * F1 now includes my real-worker break attempts: a 16-run bound with disposal after hide, and disposal of a rejected real view by the core.
  * F7 now has my end-to-end real-worker chain.
* **Closed by source only:** F5 (neutral notice slot). No screenshot was taken.
* **F6 is partially closed.** The launcher copy, the command lease and the non-destructive resume all work. However, a busy-extension quit refusal still **opens the Settings window** with "Couldn't save changes; quit cancelled. Retry: …" (Low; my probe is RED on the real App, normal and bench builds).
* **F4 is substantially implemented but not closed.** The architecture checks out: weak core binding, one shared scope/load/decode capability, four core permits, straight RGBA, revision fencing, A→B→A, invalidation, revocation, teardown, Windows marker policy, no duplicate glyph title, and explicit refusals. Three residuals have real triggers:
  * **(a) Low-Medium, parity.** Raycast asset names (`icon="my-icon.png"`, which the SDK resolves from `assets/`) are refused.
  * **(b) Low.** `Color.Magenta`, and any CSS colour other than 8 names or `#rrggbb`, refuses the whole supported icon.
  * **(c) Low, observation.** The foreground run reply waits inline for status-icon admission, fetch and decode. I measured 10 050 ms when the 4 decode slots were saturated.
* **Not claimed:** HTTPS icon success, Windows painting, macOS tray appearance, IME/AX/GPU/RAM, signing, updater, a clean-PIN binary, or migration. Tauri remains the default.

**/tmp was wiped by a reboot around 2026-11-08 14:16 +05.** The prior Nov 7 artifacts are gone:
* the root clean build `/tmp/tezbar-m911-root-final-20261107/…` (`01414172…`) and its receipt;
* the 211 external proofs under `/tmp/tezbar-native-m911-validation-20261107-2sefx6og/`;
* the prepared seed `…/tezbar-m09-fixtures-kND6GX` and its pointer file;
* the root target `/tmp/tezbar-m9-c01-validation-Rffw07`;
* user preview PID 79164.

Consequences:
* No committed copy of the clean receipt exists (`git log --all -S01414172…` is empty), so **the clean-PIN binary hash and receipt cannot be re-verified now**. That claim is reused.
* I rebuilt my own prepared seed from the PIN with the repository's `prepare.mjs`.
* The current user preview is PID 12297 (started Nov 8 23:26:10). It was untouched before and after.

## 1. Method, isolation and integrity

* **Archive.**
  * `git archive 0e6e455` → `/tmp/tezbar-m9c-opus-0e6e455-3343da54/src`. The tar SHA256 is `68153122…c6465ec5a`.
  * An independent Python check of blob sha1 and mode against `git ls-tree -r` gives **3243 / 0 mismatch / 0 missing / 0 extra**, both before and after all work (`meta/archive-verify*.json`).
  * The PIN tree has no `.gitattributes`, symlinks or submodules.
  * No live source file was used as pin evidence. Git use was read-only (`GIT_OPTIONAL_LOCKS=0`; show/diff/log/archive only).
* **Probe copy.**
  * `work/` is an APFS clone of the archive.
  * Its only differences are 3 appended test-only `#[path] mod` lines (in `status_icons_tests.rs`, `m9_review_tests.rs` and `m9_followup_tests.rs`) plus 3 new probe files. There is no production change (`meta/probe-copy-diff.json`).
  * `node_modules` was a private APFS clone of the live tree. The PIN and live `pnpm-lock.yaml`, `package.json` and `Cargo.lock` are byte-identical. No install ran.
* **Build.**
  * Rust `1.98.1 (48a229cea)`, `--locked --offline -j2`, `CARGO_INCREMENTAL=0`, `env -i`, private `HOME`/`TMPDIR`, `TEZBAR_NATIVE_SOURCE_SHA=<pin>`, features `runtime-shaders` and separately `runtime-shaders,bench-control`.
  * The private target was an APFS clone of the live `target/debug/{deps,build,.fingerprint}`, with no `incremental` directory. I deleted 31 889 + 51 + 8 workspace outputs before the first build. All 6 workspace crates recompiled from the archive, and all 560 dependencies were fresh.
  * The MSRV target was a clone of the live `target/msrv-m09` with workspace outputs removed.
  * Both clones were deleted afterwards. The root cache was only read.
* **Compiled inputs (dep-info).**
  * Pristine build: 317 first-party inputs, **all byte-identical to the PIN**.
  * Final probe build: 346 identical, plus the 3 mod-line test files and the 3 probe files.
* **Dirty flag.** The archive has no Git, so the build is honestly `dirty=true`. My binary's `--version` (`source`=PIN, `dirty:"true"`, bench=false, runtime_shaders=true, GPUI `c84af5b…`, rustc `48a229ce…`) agrees with the single schema-2 record that the repo's `captureBuildMetadata` extracted:
  * record: `dirty:true`, `rustVersion:"1.98.1"`; record SHA `d4bba8d8…`; binary `5830db6d…`;
  * `otool -L` shows no WebKit/JSC;
  * only `--version` was executed.
* **Prepared seed.** I built it from the PIN with `prepare.mjs` into my private TMPDIR. All 22 first-party worker inputs are byte-identical to the PIN, and the 42-file seed manifest is unchanged after all runs.
* **Processes and fixtures.**
  * Every run had an owned-process audit before and after: **0 survivors**.
  * Bun caches and fixture profiles stayed in my private HOME/TMPDIR. One leaked fixture tempdir was mine and has been deleted.
  * Unrelated `cargo`/`rustc` processes building in the **live** repo target, and the other review sessions, were not touched.
  * The live worktree moved during the review (HEAD `0228b4f`, dirty entries 82→99) through other actors. None of it was mine.
* **XERJ.** I read the skill. I did not use XERJ, because the prefix is stale and the brief says exact Git and `rg` are authoritative.

### Evidence integrity (all recomputed programmatically)

| Item | Result |
| --- | --- |
| M911 copied proofs (`evidence-sha256.json`) | **96/96** |
| M911 external proofs | 211 listed; **0 exist now** (reboot); 137 hash-equal a copied in-repo proof |
| M911 `compiled-inputs.json` | **427/427** identical at PIN |
| 28 gate records | all log hashes verify; the only non-zero exits are the declared ones: `native-check-01` (Send compile error), `red-01-registered-name` (compile), `red-02-exact` and `red-03-exact` (0 passed / 1 failed) |
| `validated-tree.json` (3121 entries, aggregate `afcafe60…`) | 3118 identical at PIN. The 3 others are the superseded UNRUN F4 docs, which are byte-equal to `checkpoints/unrun-01/`. The PIN adds only 122 docs files |
| approved (`dc3220ad…`) → validated | exactly 2 files differ: `status_icons.rs` and `extension_worker.rs`, the logged successors. The other 3119 are unchanged |
| F4 owned sources | 10/10 at PIN; aggregate `88f64c2c…` recomputed; `app.rs` binding line occurs exactly once |
| RED proofs | Both "pin" backups equal the 6ffa4e5 blobs of the two core files. red-02's implementation `extension_worker.rs` is the approved pre-Send `8867dddb…`; red-03 equals PIN. The red-03 log fails at `status_icons_tests.rs:65` with `(16,16)` ≠ `(24,24)` (Checkmark fell back to the old gear) |
| M10 follow-up evidence / parent completion / M11 recheck-followup | **54/54**, **5/5**, **22/22** (sha and bytes) |
| Root clean binary `01414172…` | **Unverifiable**: wiped and never committed |

## 2. Recorded results vs my runs

| Gate | Root record | My run |
| --- | --- | --- |
| 43 registered M9 tests: status icons 6, F1/F7 launcher 7, F2 3, Store, quit command, `review_m9_*` 15, `extension_integration_tests`, and the 9 real-SDK scenarios | in native 460 [E] | **43/43 Asia/Almaty, 43/43 America/Los_Angeles** [R], pristine archive build |
| `sdk_form` + `review_m9_date_default_local` | LA 1 [E] | **2/2 in each of** Pago Pago −11, St John's −3:30, Kiritimati +14, UTC and Kathmandu +5:45 (7 TZ including the above) [R] |
| All registered tests in extension_runtime, extensions_store, command_bar (terminal excluded), asset_transport, host_adapter, core_client, app::m9 | in 460 [E] | **166/166** Almaty [R] (probe build, probes skipped) |
| The same 43 under `bench-control` | in bench 462 [E] | **43/43** [R]; same probe split as normal |
| Core `status_icon_tests` (4) + `status_port_tests` (3) | 1.85: 4 + 3 [E] | **7/7 on 1.98.1 and 7/7 on 1.85.0** [R] |
| TS worker focused (effects, contracts, extensionWorker/*) | in TS 748/7 [E] | **25/25**, 5 files, LA [R] |
| My 9 probes | — | **5 GREEN, 4 RED**, identical in Almaty, LA and bench |
| Native full 460 / bench 462, UI 57, TS 748/7 skips, native Node 14 + provenance, Python cutover 1, updater 43/1-ignore ×2, Tauri compatibility, typecheck/format/whitespace, dependency tree, root normal/bench binaries | [E] hash-verified | **reused, not re-run** |

My [R] runs execute the committed PIN bytes (the archive is verified). The root gates ran on WIP 6ffa4e5 plus a patch before the commit. The build is still dirty=true by construction, so this is not a clean-binary claim.

## 3. Findings (actionable, most severe first)

### A. F4 parity: Raycast asset names are refused — **Low-Medium (Raycast compatibility; explicit fallback, not wrong pixels)**
* **Trigger.** `<MenuBarExtra icon="my-icon.png">` or `{source:"my-icon.png"}`, with the file in the extension's `assets/` folder. This is the documented SDK form: `@raycast/api` `Image.Asset` is "a string denoting an asset from the `assets/` folder", and the docs say to "reference custom ones from the `assets` folder … by file name, e.g. `my-icon.png`" (`types/index.d.ts:5426,5455`).
* **Cause.** The shared resolver joins relative names to the package root (`crates/tezbar-ui-model/src/extensions/images.rs:106-116`), and the status resolver reuses it. The handoff documents the package-root rule ("e.g. `assets/pixel.png`"), which contradicts the SDK contract the worker emulates. The legacy builder resolves the manifest icon from `assets/` first (`src/main/extension-builder.ts:247`).
* **Probe.** `opus_m9c_f4_bare_asset_name_follows_raycast_assets_folder` → **RED**. `assets/pixel.png` resolves (1×1). `pixel.png` and `{source:"pixel.png"}` give `Err("Image could not be loaded")`.
* **Consequence.**
  * Most real custom menu-bar icons show "icon unavailable": the command title as text on macOS, the warning marker on Windows.
  * The same resolver serves native List and Grid images, so this is cross-scope (owner: the native image contract, M810 assets).
  * Not a regression vs legacy Tauri, which did not render bare names either.
* **Fix.** Resolve bare relative names under `<root>/assets` (optionally falling back to the root), keeping the canonical containment and symlink checks. Add bare-name tests for status and List/Grid.

### B. F6 residual: a busy-extension quit refusal opens Settings with a false save-error message — **Low (copy and surface; executed)**
* **Trigger.** Quit while a Store install is running (probe). A running extension command takes the same `QuitPreparation::Refused` branch [S].
* **Cause.** Every refusal runs `this.settings(cx)` (`apps/tezbar/src/app.rs:618`). That calls `settings_intent(None)`, which hides the launcher and opens or focuses Settings. It then sets `"Couldn't save changes; quit cancelled. Retry: {error}"` (`:622`). The launcher copy is correct (`:604,629`). The root test only checks `save_error.contains(…)`.
* **Probe.** `opus_m9c_f6_busy_extension_refusal_has_no_false_save_error_surface` → **RED** in normal and bench builds.
  * Settings window: none before, opened after (`lifecycle.settings=true`).
  * Settings error: "Couldn't save changes; quit cancelled. Retry: Extension install in progress. Wait for it to finish, then quit."
  * Launcher and `save_error`: "Quit cancelled: Extension install in progress. …" (correct).
* **Consequence.** The M10 disposition "no false save-error prefix" holds for the launcher only. The user gets an unexpected Settings window claiming a save failure.
* **Fix.** Use a typed refusal (busy vs durability failure). For busy refusals, keep the launcher message only: do not open Settings and do not use the save prefix. Extend the test to every surface.

### C. F4: `Color.Magenta` (and most CSS colours) refuses the whole supported icon — **Low (functional/visual; executed)**
* **Trigger.** `icon={{ source: Icon.Checkmark, tintColor: Color.Magenta }}`. The worker shim maps `Color.*` to `String(prop)` (`src/main/extension-runner.ts:301-306,1801-1802`), so the wire value is exactly `{"source":"Checkmark","tintColor":"Magenta"}`, verified through the real worker.
* **Cause.** `tint()` (`apps/tezbar/src/asset_transport/status_icons.rs:150-176`) knows 8 names plus `#rrggbb`. Any other value returns an error, and `tint(…)?` (`:29`) then refuses the icon. The legacy list renderer passes any tint string to CSS (`list-runtime.tsx:187-195`), where an invalid colour falls back to `currentColor`.
* **Probes.**
  * `opus_m9c_f4_every_sdk_color_tints_a_supported_builtin` → **RED**: of the 9 SDK colours only Magenta is refused. `#f00`, `#ff000080`, `rgb(255, 0, 0)`, `magenta` and `pink` are refused too.
  * `…_real_worker_color_magenta_reaches_the_status_boundary` → **RED**: the status shows title "probe status icons", tooltip "Opus tint probe (icon unavailable)" and no bitmap.
* **Fix.** Add magenta, plus 3-, 4- and 8-digit hex and `rgb()`. Better, degrade an unknown tint to the default colour while keeping the declared icon. Add an all-SDK-colours test.

### D. F4 observation: the run reply waits for status-icon resolution — **Low (latency/design; executed)**
* **Cause.** `request()` awaits `sync_sdk_status` → `resolve_sdk_status_icon` before replying (`crates/tezbar-core/src/extension_worker.rs:282,968`). A remote fetch has a 10 s deadline, and decode admission or decode another 10 s (`tezbar-core/src/assets.rs:14`).
* **Probe.** In `opus_m9c_f4_transient_icon_failure_is_retried_on_an_identical_render` (GREEN), I held the 4 shared decode slots:
  * `extension:run-command` for a MenuBarExtra with a local SVG icon was still pending at 2 s and replied after **10 050 ms**;
  * the icon was then degraded to "(icon unavailable)";
  * it recovered on the next action re-render, so the failure is not sticky across interaction.
* **Consequence.**
  * An HTTPS icon on a slow or offline network, or decode saturation (for example a large Grid), delays the launcher's run reply and signature-changing invoke replies by up to about 10–20 s.
  * During that time the run holds a CoreClient command lease, so quit refuses with "Extension command still running".
  * A static menu with no interaction stays degraded until its state changes, a re-run, disable/remove or quit [S]. Native has no scheduler for manifest `interval`.
* **Suggestion.** Publish the bitmap from a spawned task under the existing `(signature, revision)` fence and return the reply immediately. The fence already makes asynchronous publication safe.

### E. Docs and provenance precision — **docs only**
* M911 `compiled-inputs.json` (427 entries, all exact at PIN) is not the full compiled-input set. It omits:
  * 2 `include_str!` inputs of the normal native library: `src-tauri/tauri.conf.json` (`app/m11.rs:31`) and `src-tauri/runtime/embedding-macos-arm64.json` (`knowledge/runtime_bundle.rs:71,319`);
  * 15 of the 22 first-party worker-bundle inputs, including `src/main/extension-runner.ts`.

  All 17 are in `validated-tree.json` and identical at PIN, so there is no integrity gap, but the "427 current compiled inputs" label overstates its coverage.
* `docs/native-m10/followup-20261107/worker-grants-current-map.json` is exact for 50440ad and 6ffa4e5 (source `a76dad69…`, 23 lines). It is stale at PIN, where `extension_worker.rs` is `03d8bfc5…`: the query still finds 23 lines, including `RunGrant` and `set_sdk_status_item`, but at other line numbers. The map is still named "current".

**Non-gating notes.**
* The real-worker test `review_m9_followup_d13_real_worker_hidden_launcher_orphans_live_session` now asserts disposal. Its name is misleading.
* A deterministic SDK author error (duplicate Form ids) shows "Extension view is unavailable. Run the command again." (`command_bar/mod.rs:1635`). Re-running cannot help (D15-type copy).
* Status bitmaps are fixed-colour (default grey 128), not template images, and source/tint selection is dark-first regardless of menu-bar appearance. Whether this is correct on a light macOS menu bar is a platform/manual unknown.
* F2 keeps one chosen value per session, not per pushed view. It persists across push and pop while the value is still offered. This matches the legacy TS reference (`ListRuntime` is not keyed per view; `accessoryValue` persists if offered). Raycast's per-view semantics were not verified.

## 4. F4 implementation verification (what holds)

| Requirement | Verdict | Basis |
| --- | --- | --- |
| Native host resolver; weak existing core; no ownership cycle | **Holds** | `host_adapter.rs:53-58,88-98` uses `OnceLock<Weak<NativeCore>>`; the single construction site binds it (`app.rs:1746`, production and preview); [R] weak-binding test; an unbound or dropped core gives an explicit Err |
| One shared `installed_scope` / `load_asset` / `decode_admitted`; no per-icon CoreClient, runtime, HTTP client or font DB | **Holds** | `status_icons.rs` uses the shared registry scope and `load_asset(core, None, …)`; `application_icons: None` denies the app-icon capability; the SVG decoder loads no system fonts [S] |
| Core 4 permits; SVG and asset budgets | **Holds** | [R] `share_all_four_core_slots…`. Builtins hold `decode_permit()` inside `spawn_blocking` (`:35`) and use `decode_preview(…, 24, ASSET_BYTES, ASSET_BYTES)` |
| Straight RGBA at the OS boundary | **Holds** | `decode_svg` demultiplies tiny-skia output (`asset_transport.rs:378`); raster `into_rgba8` is straight; channels are swapped back. [P] a semi-transparent PNG gives exactly `[200,100,50,128]` and an SVG `[199,100,50,128]` (GREEN) |
| Finite copied legacy SymbolIcon paths plus Gear | **Holds** | All 15 element shapes are verbatim legacy `SymbolIcon` elements; `circlefilled` is an SVG approximation (legacy used a CSS span); Gear keeps the previous 16×16 geometry |
| Custom PNG/SVG/data/HTTPS | **Partial** | Package-root PNG/SVG and data work [R][P]. Bare asset names are refused (A). HTTPS success was not tested |
| Unknown or unsupported forms explicitly refused, never a silent gear | **Holds** | [R] unknown builtin, glyph, blob, fileIcon, mask, http, credentials, symlink escape. The over-broad tint refusal is finding C |
| Windows distinct absent/refused markers; good pixels intact; macOS icon-only success has no glyph title | **Holds (portable policy)** | `process_host.rs:1027`; [R] 1.98.1 and 1.85; [P] successful icons show title `""` |
| Registry read/revocation; scope re-authorization after async work | **Holds** | `status_icons.rs:109-113`; [R] removing `package.json` during a pending decode gives Err; symlink escape refused |
| Session, scope and monotonic revision publication; A→B→A; invalidation during load; teardown | **Holds** | Checked increment (`extension_worker.rs:950`); final recheck under both locks (`:995`); invalidation nulls the signature (`:571`); [R] A→B→A, pending dispose/replacement and invalidation tests. `shutdown` clears signatures; worker EOF drains sessions and removes items; `NativeCore::shutdown` does not await pending resolutions [S] |
| Coalesced whole-root signature; no unchanged-icon cache for menu-only updates | **As documented (optional)** | A menu-only change re-resolves the icon; see D for the latency side |
| Dark/light semantics vs actual legacy source | **Partly as described** | Dark-first matches legacy Grid themed sources (`grid-runtime.tsx:66-76`) and legacy adaptive tints (`list-runtime.tsx:187-195`). Legacy List rows do **not** resolve themed sources at all (`textValue` → plus fallback), and legacy never rendered MenuBarExtra (Tauri SDK status is off). So "legacy dark-first" is accurate for Grid and tint, not universal |
| NativeV2 relative-icon contract | **Preserved, with bounds** | Canonical containment now also refuses symlink escapes (stricter); 2 MiB encoded cap and 2 MiB decoder allocation; content-sniffed format; the ≤512 px rule is preserved (`process_host.rs:1118-1146`); [R] 1×1 ok, 513 refused, `../` refused |
| Tauri unchanged | **Holds** | `TauriHost` overrides neither `sdk_status_items` (default false) nor the resolver [S] |

## 5. Closure verdicts (F1–F8)

| ID | Root repair (verified) | Evidence | Verdict |
| --- | --- | --- | --- |
| F1 / D13 | Extension runs live in `extension_runs` (`command_bar/mod.rs:125,1519,1857`), bounded at 16 (`:1386,1850`), and not cleared by `hide` (`:739`). Stale, hidden or rejected views are disposed (`:1420,1629`; `extension_integration.rs:239,254`; Store `extensions_store/mod.rs:473,507-519`). MenuBarExtra is skipped | [R] 7 launcher tests, including the real-worker hide case (core no longer owns the session). [P] 17 hotkey runs: 16 start, the 17th is refused, after hide 15/15 foreground views are disposed and the MenuBarExtra is kept. [P] A real-worker duplicate-id Form is refused and the **real core emits `extension:session-disposed`** | **Closed (headless).** The user-requested MenuBarExtra background lifetime (hide/navigation until disable/remove/exit) is preserved: [R] `sdk_menubar`; RuntimeView `retained_background`; worker exempt-from-prune limit 64 |
| F2 / D12 | Precedence controlled `value` > chosen value still offered > `defaultValue` > first; reset on new session (`extension_runtime/view.rs:1359-1379`, `mod.rs:287`) | [R] 3 dropdown tests | **Closed (headless)**; matches the legacy reference |
| F3 | Local `new Date(2026,10,7)` in the form fixture | [R] 7 timezones GREEN | **Closed** |
| F4 | Native bounded status-icon resolver | Section 4 | **Not closed**: A (Low-Medium), C (Low), D (Low observation). No OS painting, Windows or HTTPS |
| F5 | `ExtensionEffect::Notice` → `v.message` (neutral notice slot) (`app/m9.rs:36`) | [S] | **Closed by source**; no screenshot |
| F6 | Command lease plus a specific refusal (`core_client.rs:637`), resume, plain launcher copy | [R] real CoreClient/Bun command test, busy-install test. [P] Settings surface RED | **Partially closed**: B |
| F7 | Authenticated in-flight run grant (`extension_worker.rs:462,512`) → reply intent (`:228`) → the caller's own extension id → typed `ExtensionPreferences` → the Settings Preferences action (`settings/mod.rs:423-433`) | [R] worker/core flag, spoofed id and hidden caller rejected. [P] **end-to-end real Bun → CoreClient → launcher → `ExtensionPreferences{"raycast.m09-native-fixtures"}`**, no launcher error | **Closed (headless)**; actual OS Settings focus is manual |
| F8 | The Store keeps the worker's message (2048-char bound) and the transport reason (`extensions_store/mod.rs:534-539`) | [R] Store follow-up test | **Closed**; the docs map is stale at PIN (E) |

**D1–D15 at the pin (unchanged otherwise).** D1 closed (F7). D2 holds (Tauri status capability off). D3–D5, D7–D9, D11 and D14 stay repaired, with D14's copy residual tracked as F6/B. D6 closed with F3. D10 closed on the duplicate-gear point; the icon resolution is F4. D12 and D13 closed. D15: copy notes only.

## 6. E rows

No row is accepted. Affected rows:
* E-35 and E-42: F4 A, C and D.
* E-15, E-18 and E-36 (cross-scope): the asset-name contract (A).
* E-03 and E-37: the quit refusal surface (B).
* E-16: F2 closed headless; visual review pending.
* E-28: F3 closed.
* E-10 and E-34: F7 closed headless.
* E-38: F8 closed.

Windows, physical GUI/IME/AX/GPU/RAM, light/dark screenshots, real tray appearance, HTTPS, signing/updater/profile and the native release builder/feed keep every row open. This narrow closure does not accept a full migration.

## 7. Paths and hashes

* **Work dir:** `/tmp/tezbar-m9c-opus-0e6e455-3343da54`.
  * `src/`: the verified archive.
  * `work/`: the probe copy.
  * `fixtures/tezbar-m09-fixtures-IbNePN/`: my prepared seed.
  * `probes/`: the archive `--version` binary `5830db6d…` and `capture-metadata.mjs`.
* **Probe sources:**
  * `work/apps/tezbar/src/asset_transport/opus_m9c_probe.rs`
  * `work/apps/tezbar/src/app/opus_m9c_quit_probe.rs`
  * `work/apps/tezbar/src/surfaces/command_bar/opus_m9c_launcher_probe.rs`
* **Logs:**
  * `logs/focused-m9-{almaty,la}.log` (pristine PIN), `tz-form-*.log`, `m9-surface-almaty.log`
  * `probes-{1,2,3}-*.log`, `final-{Asia_Almaty,America_Los_Angeles}.log`, `bench-focused-almaty.log`
  * `core-status-{198,185}.log`, `ts-worker-focused.log`, `archive-bin-{version,metadata}.json`, `build-*.log`, `prepare.log`, `audit-*.txt`
* **Verifiers:** `tools/verify_archive.py`, `verify_evidence.py`, `verify_hashmap.py`, `depinfo_verify.py`, `manifest.py`.
* **Manifests:** `meta/` (archive, evidence, dep-info, cache clone, probe diff, seed manifests, user-preview before/after).
* **Hashes:** `SHA256SUMS.txt` (64 evidence files plus `pin.tar`). Final normal probe test binary: `70e58715…` (`meta/probe-binary-normal-3.sha256`). The bench artifact `tezbar_native-fc5ea7ff12d409be` was not hashed. Both targets were deleted afterwards; the pristine-PIN test binary was overwritten by the first probe build, and its runs are preserved only as logs.
* **Environment:** rustc 1.98.1 (48a229cea) and 1.85.0 (4d91de4e4), Bun 1.4.0, Node v26.7.0, macOS 26.3 (Darwin 25.3.0).
