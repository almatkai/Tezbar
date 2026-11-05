# Native launcher prototype — Opus 5.5 targeted recheck of the repairs

**Pin:** worktree `Tezbar-native-rust-agent`, branch `feat/native-launcher-prototype`, HEAD `4da6ff8`
(clean before and after this review).

**Range reviewed:** `45278ff..4da6ff8`, the three repair commits `8ef1800`, `9fc73e4` and `4da6ff8`.
I confirmed that nothing outside `prototypes/native-launcher/` changed in this range: no NativeCore
or production code, and no main `Cargo.lock`.

**Prior report:** `…/22463d83-…/scratchpad/native-launcher-opus-review-45278ff.md`, left unchanged.

**Reviewer:** Claude Opus 5.5, working alone with no subagents. I made no source edits or commits,
and this report is outside the tree so that `build.rs` keeps stamping `dirty=false`.

**Scope:** an experimental launcher prototype, not a production replacement. I only rechecked the
seams these repairs touched.

## Verdict

- **All of F1–F9 are closed.** The three newest repairs hold up: the one-character query change,
  waiting for OS visibility, and launching through LaunchServices with a responsibility check. I
  found no regressions in the seams they touched.
- **No P0, P1 or P2 issues remain in this scope.** There are six P3 items (N1–N6). None of them
  invalidates the current 8ef artifacts or the v2 smoke series. N1–N4 should be fixed or labelled
  before the full matrix is reported. N5 and N6 are hygiene.
- **Readiness for measurement:** the macOS harness is ready to *start* the full matched matrix with
  the existing 8ef A and B artifacts. All five v2 smokes passed. These smokes are functional
  evidence only and carry no memory verdict. The full 1-hour matrix, Windows GUI, DirectX and IME
  testing, and a screen-reader check are all still pending. This review does not close the
  measurement gate or the visual-QA gate.

## Closed findings

| ID | Status | Evidence |
| --- | --- | --- |
| **F1** BGRA | **Closed** | `bgra()` swaps bytes 0 and 2 and keeps alpha (`launcher_view.rs:550-556`). The unit test checks the channels and a 128 alpha value. Parent CUA saw Safari with a blue dial and a red needle at 8ef. |
| **F2** atlas leak | **Closed (hide path verified live; eviction and replacement paths confirmed in code only)** | `hide` drains the cache and calls `cx.drop_image(img, Some(w))`. Replacement (`insert` returning `Some(Some(old))`) and LRU eviction do the same. The `parent-gpu4` probe ran 60 cycles of `doc` query, settle, 1.5 s, hide (the same shape that leaked before). `IOAccelerator (graphics)` stayed at 6448K virtual, and dirty went 6448K → 4160K → 4160K. Footprint went 144.9 → 63.8 → 64.0 MiB, where it previously grew linearly to 76.8 MiB. No owned process survived. The probe covers only the hide path. A retained run touches at most 6×18 = 108 distinct icon keys, which is below the 128-entry LRU, so eviction will rarely fire during measurement. The code mirrors GPUI's own `image_cache` eviction. Images decoded but thrown away (stale generation, or hidden) were never painted, so they never reached the atlas. |
| **F3** A/B preparation | **Closed** | Both apps start hidden; B no longer receives `--show`. The shared preparation (`measure.py:158-166`) is: hidden = show, hide. Visible = show, then commit `doc`. Retained = show, `open_timings` (hide, show, frame receipt), then the fixed workload, then hide. `pre_sample_steps` is recorded in every v2 result and matches between A and B. In smoke mode, B's extra work (latency pass plus add-note activation) is labelled `smoke_extra_work`. See N4 for one wording overclaim. |
| **F4** bundled binary | **Closed** | `bundle-macos.sh --binary` copies the binary it is given. `matched.py` re-runs `native_version` on the signed copy inside the bundle and requires it to equal the verified version, including `bench_control` and `rustc_commit`. C is bundled the same way. |
| **F5** A provenance and compiler | **Closed, with gap N1** | `build-a.py` refuses a dirty tree, checks HEAD and resources again after cargo finishes, and writes a receipt with the source, binary SHA, compiler hash from the binary, resource hashes and main lock hash. `baseline_version` re-checks the source, `dirty`, the SHA, the compiler hash scanned from the binary and current resources, in `matched.py`, `stage-a.py` and `measure.py`. `compiler()` requires the binary's set of `/rustc/<hash>/` strings to be exactly the 1.98.1 hash. The 7 Python negative tests pass (I re-ran them). **Checked live:** `target/tauri-a/release/tezbar` has sha256 `ced9c3e6…edcf`, equal to the receipt. The receipt says source `8ef18004…`, `dirty:false`, rustc `48a229ce…`, equal to `rustc +1.98.1 -vV`. B `--version` reports 8ef, `dirty "false"`, the same rustc and `bench_control:true`. |
| **F6** identity fields | **Closed** | Each result now has `binary_version`, `source_sha` taken from the binary under test, and a separate `harness_sha`. The v2 results show source `8ef1800…` and harness `4da6ff8…`. |
| **F7** scroll | **Closed** | `ScrollStrategy::Nearest`. Parent CUA: 18 Down presses left the selection near the bottom edge. |
| **F8** quit | **Closed** | Control `quit`, Cmd-Q and Alt-F4 all go through `quit()`. A `quitting` latch stops a second quit. `quit()` hides, then awaits `core.shutdown()` on Tokio with a 5 s timeout, then calls `cx.quit()`. While quitting, control ops other than `status`, `ready` and `quit` are refused. `NativeCore::shutdown` is idempotent (`closing.swap` under the `lifecycle` lock), so the `on_app_quit` fallback returns at once after a completed shutdown. If the first shutdown hit its timeout, its future was dropped, the lock was released and `closing` is already true, so the fallback cannot hang either. The quit happens after the control reply is sent, and `close()` waits up to 15 s, which covers the 5 s bound. Limit: a forced OS termination still skips the hook, and the README says so. |
| **F9** polish | **Closed** | Icons are processed in batches of 4 through a `JoinSet`. Dropping `icons_task` aborts the whole tree: the gpui_tokio `Tokio::spawn` at the pin aborts on drop, and so does `JoinSet`. A failed or panicked join only means that icon is fetched again later. "No results" uses text-2 and the empty-input hint keeps text-3. The difference in icon-cache lifetime between A and B is documented in the README. The 22/21 px inset nit is unchanged; I accept it as is. `settled()` now includes the last status in its TimeoutError. |

## Rechecks of the newest harness repairs

1. **Fresh-reply query list `d` → `do` in both A and B.** This matches the production renderer:
   `src/renderer/CommandBar.tsx:2347-2350` returns without a native request when the normalized
   value has length 1 (outside deep mode). Changing both lists keeps the workload the same, and
   the README plus `latency_query_note` say outright that one-character comparisons are left out.
   That labelling is honest, and the workload is still useful (2 chars, 3 chars, app name, file
   prefix, a phrase, and a no-match). It does depart from the spec §6.5 script, which lists a
   one-character query; the README should call it a §6.5 deviation in those words. One remaining
   asymmetry is in N2.
2. **Waiting for OS visibility before warmup.** `wait(main_on_screen == (condition=='visible'), 5)`
   runs once before sampling, and every `sample()` still rejects the run if visibility flips. This
   does not hide ongoing focus interference. Retained runs in both apps end hidden, so expecting
   "hidden" there is correct.
3. **LaunchServices launch and the responsibility check.** Native processes now start with
   `open -n -g -o … --stderr … <app> --args …`, the same way A does. The process is found by its
   exact `ps comm` path, which is unique per fresh root; the parse handles the spaces in the
   bundle name. The run is refused unless `host.responsible == host.pid`, and C is included in
   that check. In every v2 result the host is responsible for itself and every owned helper
   reports `responsible == pid`. B now accounts for a `SafariPlatformSupport.Helper` (about
   9.8 MiB), which is exactly the class of reparented XPC helper the old direct spawn could miss.
   In the parent-gpu4 probe the host's `responsible` is 68012 (T3), so that probe is valid only
   as a single-process GPU-lifecycle check, not as accounting evidence. That is consistent with
   how the parent labelled it. `close()` waits for the exact PID to disappear, verifies the
   executable path before sending SIGTERM, and only then scrubs the fixture clipboard. See N5 for
   one false failure this can produce.
4. **A artifact after the failed 9fc rebuild.** `parent-final-A-build.log` shows cargo
   recompiling and linking at 18:59 before the guard raised `Source/resources changed during A
   build`, so no receipt was written. The binary on disk therefore has an 18:59 mtime, newer than
   the 18:46 receipt. Its sha256 still equals the receipt's `ced9c3e6…`. The rebuild was
   byte-identical, so the receipt binds by content and A is still valid 8ef evidence. Do not cite
   the 9fc attempt as a successful build.

## v2 smoke series (finished; read after the static review)

The log shows all five runs completed. Each result has `failure: null`, `owned_survivors: []`,
`installed_app_unchanged: true`, the same `indexed_counts` (files 14997, applications 235, …),
source 8ef and harness 4da. No `tezbar-native-*` process remains.

| Run | pid=responsible | `pre_sample_steps` | Notes |
| --- | --- | --- | --- |
| C hidden | yes | — | single host, 27.5 MiB |
| A visible | yes | show, query doc, results committed | 6 owned processes |
| B visible | yes | show, query doc, results committed | `smoke_extra_work` labelled; 1 click record |
| **A retained** | yes | show, hide, show, opening frame receipt, fixed query workload | **passed**; open receipt 41 ms / OS 12 ms; 120 Tauri observations |
| B retained | yes | (same) | passed; open receipt 38 ms / OS 14 ms; 12 latency rows |

A retained, which failed before, now passes. It failed for the reasons the parent identified: the
one-character fresh-reply assumption, and visibility acknowledged before WindowServer showed it.
No further harness change is needed for it to run.

All smoke footprints come from 2-second windows. A retained's WebContent was about 1.18 GB
immediately after its workload, which is transient. **None of these numbers is a memory
verdict.**

## Open findings (all P3)

### N1 — A's receipt does not cover the renderer `dist/` it embeds (confidence: high; current impact: low)
- `tauri.conf.json` sets `frontendDist: ../dist`. `build-a.py` runs a plain `cargo build` with
  `tauri/custom-protocol`, which embeds `dist/` but does **not** run `beforeBuildCommand` (vite
  plus build-backend).
- `dist/` is gitignored and missing from `resources()`. A `dist/` built from another branch
  (for example `dev`) would therefore pass every check and still get a receipt saying
  "source 8ef, clean". The README's `pnpm build:frontend` step comes before `build-a.py`, but
  nothing enforces it.
- **Current artifacts:** `dist/index.html` was written at 16:10, after the last renderer-side
  commit (`86fc4ad`, 10:33). The A binary is byte-identical across rebuilds, so the risk is low,
  but the receipt does not attest it.
- **Fix:** in `build-a.py`, run `pnpm exec vite build` (and the backend build if
  `dist-backend` should be bound the same way) after the clean check, then hash `dist/` into
  `resource_files` before and after cargo.

### N2 — "characters" style still includes a one-character step that only B sends to the core (confidence: high)
- In character-by-character input (70 ms per character), B commits `d`, starts a native search,
  then commits `do`. A filters `d` locally and searches only `do`.
- B's final-reply timing for `do`, `doc`, `document-001`, and so on can therefore include an
  extra search in flight. This biases the comparison against B, not for it.
- **Fix:** add one sentence to `latency_query_note` and the README. Optionally, report
  one-character echo or local visible-list time as a separate, unmatched UX metric.

### N3 — A's retained latency rows carry no `query` field (confidence: high)
- `observations.append(dict(rep, warmup, style, …))` in `measure.py` omits `query`. B's rows
  include it.
- Per-query comparison means rebuilding the query from loop order. Also, in smoke mode A always
  runs 10 repetitions while B runs 1; this only affects smoke runs.
- **Fix:** add `query=query`.

### N4 — A's visible preparation label "results committed" claims more than A waits for (confidence: medium)
- A sets the value through JS and waits two animation frames. It does not wait for the
  `searchAll` reply. B does wait, through `settled`.
- The 120 s warmup makes this irrelevant for the measurement itself, but the label claims an
  equivalence that was not checked.
- **Fix:** either wait on the existing responsiveness hook or DOM until the `doc` results are
  shown, or relabel the step for A as "query doc dispatched".

### N5 — `close()` can raise a false "identity changed" error (confidence: medium; fails safe)
- If the process exits between the end of the 15 s wait and the `ps -p` check, `actual == ''`
  and `close()` raises `native process identity changed` (the same applies to
  `Baseline.close`). Treat an empty result as "already gone".
- Related: `binary_sha256` means different things per mode. For A it is the unsigned target
  binary; for B and C it is the signed copy in the bundle. The A copy inside its bundle is
  re-signed and never hashed.
- The chain still holds, because `stage-a.py` verifies the receipt immediately before copying.
  Record the launched-executable hash under a separate field.

### N6 — `native_version` failure cases have no tests (confidence: high; hygiene)
- The 7 tests cover only `baseline_version` and `compiler`. `native_version` has none for
  `dirty "true"`, `bench_control:false`, a source mismatch, or a mismatch between
  `rustc_commit` and the compiler hash in the binary.
- **Fix:** about four small tests that patch `subprocess.check_output`.

## Limits and notes

- **`ready_ms` is not a matched metric** (an existing issue, outside this range, but it affects
  reporting). A's `ready_ms` stops at renderer readiness, before it waits for
  `core:launcher:status`. B and C's `ready_ms` include the launcher core becoming ready (about
  5.85 s against A's about 1.0 s in v2). Do not compare startup from this field.
- **No CUA was available to me** (ToolSearch found no desktop-automation tool). UI behaviour
  comes from the parent's CUA evidence (`parent-native-qa.json`): Safari colours, exact paste of
  Kazakh, Russian and emoji text, Down-key scrolling, and Escape hiding the window. The parent
  made no IME, Latin dead-key or screen-reader claim, and neither do I.
- **I ran no native GUI, probe or build.** I re-ran only the Python provenance unit tests
  (7 passed). The 8 Rust prototype tests are taken from the parent's run; this range changed only
  the prototype crate.
- **XERJ was not used.** The diff is small and fully read, so I used `rg` and direct reads.
- **Not checked or changed:** real profiles, clipboard, secrets, the installed app, approvals,
  push, PRs, releases or Actions.

## Before the full matrix (optional but recommended)

1. N1: bind `dist/` in `build-a.py`. Rebuilding A then needs a new receipt, so if you want to
   measure the current A unchanged, document N1 as a known limit instead.
2. N3: add the `query` field.
3. N4: either wait for A's results or relabel the step.
4. N2: add the note for the character-by-character asymmetry.

These four changes are Python and README only, so no native rebuild is needed for B.
