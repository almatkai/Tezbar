# Beta.17 R14 follow-up: post-search idle regression and RSS (Opus pass 1)

**Outcome: R14's joined-BM25 production adoption is retained, now gated to the
only qualified target (macOS arm64 + bundled SQLite 3.46.0).** The R14 hidden
post-search regression (family CPU 2.35×, host wakeups +70–95 %) does **not**
reproduce against a matched current-source control when physical input is
verified absent. R14's RSS increase is clean, reclaimable WebKit allocator
residency whose size varies as much *within* each build as between them; it is
not a retained-allocation cause. **Uniform 5× speed / one-fifth RAM remain unmet.**
Compact numbers: [beta17-opus-pass1-results.json](beta17-opus-pass1-results.json).

Worktree `Tezbar-performance`, branch `fix/beta17-runtime-search-memory`, start
`7dfa40c`. Commits: `cb3a74b` (gate + callsite tests + probes) and the report
commit. No subagents, no other tree/branch, no renderer/model/tokenizer change.
XERJ was not used (supplied shared-breaker/no-match history); discovery used `rg`.
New evidence only under `build/performance-opus-pass1-20261030/`; R5–R14 raw
evidence, frozen apps and pins are untouched.

## Premises that were wrong

1. **"Most additional idle CPU is native host work left after search."** The
   R14 candidate's extra host CPU is a *transient* in the first ~3–15 s of the
   window, after which both builds idle identically (≈3 ms/3 s plus the same
   periodic ~35 ms tick present in baseline). The transient is intermittent and
   occurs in **both** builds (below). Its signature — `main` plus
   `com.apple.NSEventThread` CPU in a hidden LSUIElement app — is WindowServer
   event delivery. R14's hidden windows had no guard for physical input
   *elsewhere on the machine*: the WebView trusted-input counter only sees input
   delivered to Tezbar's own page.
2. **"R14 baseline differs from the candidate only by the rank change."** The
   frozen R7 baseline and R14 candidate also ship different
   `dist-backend/extension-worker.js` (tracked `aa02…` vs regenerated `027b…`;
   the tracked bundle is stale relative to `src`: a `launcher` field). The worker
   only runs on demand under an installed Bun and is absent from every observed
   family, so it is not the idle cause, but it is a packaging confound. This pass
   replaces that comparison with a **matched control**: same commit, same
   `build:backend`/`build:frontend` outputs, same resources; the two bench apps
   differ **only in `Contents/MacOS/tezbar`** (verified file-by-file).
3. **"RSS +36 % suggests new retained memory."** See RSS section: it is clean
   libpas residency; dirty/physical footprint is not higher.

## Hypotheses tested (ranked before testing)

| # | Hypothesis | Test | Result |
|---|---|---|---|
| 1 | Deferred host work after search scales with the rank change | Host per-0.5 s trajectory, per-thread CPU (`proc_pidinfo`, no suspension), `sample` stacks, short repeatable loop | **Falsified** under quiet input; transient appears in both builds and tracks physical input |
| 2 | Frozen-R7-baseline build/resource confound | Resource diff; matched current-source control | Confound exists (worker bundle) but is not causal; control removes it |
| 3 | Order/thermal/OS | P/C/C/P + reversed C/P/P/C | No order effect in matched cohorts |

## Feedback loop and diagnosis

`docs/performance/post-search-probe.py` replays broad-full's exact prefix before
`post_search_idle` (hidden idle, 12 open/close searches, 8 simple bursts, 4 broad
bursts, hide, 2 s settle) and samples the 30 s window, ~2 min per run. Options add
the model copy and R14's `vmmap` inspection, a full-window or burst-triggered
`sample`, per-thread host CPU and system HID idle time (read-only `ioreg`
`HIDIdleTime`; no event content). Stack/trigger runs are diagnostic only.

- 27 completed probe runs (14 frozen R7 baseline, 13 R14 candidate; 3 failed with
  the host's `deadline has elapsed` js-command error, retained, excluded).
  Elevated host wakeups (>550/30 s): baseline **2/14** (one at 646, CPU .129),
  candidate **4/13**. Host CPU ranges overlap: baseline .065–.129, candidate
  .053–.185 CPU-s. Not distinguishable.
- **Labelling error retained:** the first `loop-C-*`/`loop-P-*` series all ran the
  R7 baseline (zsh word-splitting in the driver loop). `probe-summary.json` carries
  the authoritative app/binary per run. One of those baseline runs shows a burst.
- Per-thread data: in the one mildly elevated run with thread capture, the extra
  CPU is `main` 18.1 ms + `NSEventThread` 7.0 ms (quiet runs: main 2.6–3.5 ms,
  NSEventThread ≈0). No SQLite/launcher/Knowledge thread differs. Full-window
  `sample` runs saw no bursts; the burst-trigger attached after the periodic
  tick, so no burst stack was captured — the owner is identified at thread level,
  not by a stack.
- HID series (C/P/C/P/C/P): five windows had **zero** physical input for the whole
  window (HID idle 7.7–354 s): host CPU .092/.092 (C) vs .094/.091/.088 (P),
  wakeups 506–507 vs 501–510. The single window with input (HID-active for 89 of
  113 quarter-seconds) is the only elevated one (537 wakeups, `main` 4.1 ms).

## Matched full desktop acceptance (R14 broad-full workload, unchanged)

`docs/performance/broad-full-quiet.py` = unchanged `broad-full.py` + per-sample HID
idle and host thread CPU. `summarize-quiet.py` marks an idle window contaminated
if any sample's HID idle is shorter than the time since the window began; such
windows are reported, never deleted, and excluded from quiet cohorts. Order
control/candidate/candidate/control/candidate/control/control/candidate; all 8
complete, zero survivors, vector digest `27efcec4…` everywhere. `candidate-4` had
physical input in all four idle windows → excluded (and is the only run with
elevated idle wakeups: 540/821/852). Quiet cohorts: **control 4, candidate 3**.

| Quiet 30 s window | Family CPU-s, control→candidate | Host CPU-s | Host wakeups | Footprint median MiB | Settled summed RSS MiB |
|---|---|---|---|---|---|
| Hidden/no model | .112→.117 | .054→.058 | 512→514 | 134.6→134.9 | 286.4→284.8 |
| After simple+broad | .109→.118 | .069→.078 | 504.5→505 | 239.7→183.6 | 513.6→657.4 |
| Model retained | .288→.294 | .151→.160 | 737.5→743 | 360.8→293.9 | 932.8→1029.9 |
| After repeat | .280→.249 | .138→.130 | 763→763 | 338.6→295.2 | 934.9→1030.2 |

Medians over runs; full ranges/values in JSON. The R14 2.35× post-search family
CPU becomes **1.08×** (+.009 CPU-s per 30 s, i.e. 0.03 % of one core; candidate
range .114–.151 overlaps control .096–.148, with a slightly higher maximum). Host post-search +.009 CPU-s with
identical wakeups is within control range (.065–.082) at its edge; it is reported,
not hidden, and is far below R14's +.17–.21.

| Bursts (median of runs) | Response p50/p95 ms | Two-frame p50/p95 ms | Family CPU-s |
|---|---|---|---|
| Simple | 16.0→14.5 / 17.0→15.5 | 42.5→39.5 / 48.5→47.0 | 1.633→1.402 |
| Simple broad | 10.5→9.0 / 14.0→13.0 | 23.5→25.5 / 37.5→43.5 | .349→.337 |
| Deep lexical | 14.5→13.5 / 15.0→14.5 | 37.5→40.5 / 43.0→46.5 | .466→.470 |
| Deep lexical broad | 8.0→7.5 / 15.0→14.0 | 36.0→34.0 / 48.5→42.5 | .454→.408 |
| Warm deep | 23.5→24.5 / 36.5→36.0 | 53.0→52.0 / 63.0→64.0 | .819→.831 |
| Warm deep broad | 18.5→18.5 / 24.5→24.0 | 49.0→47.5 / 52.5→53.5 | .290→.303 |
| Repeated deep | 23.0→23.0 / 29.0→28.0 | 54.5→53.5 / 59.5→57.0 | 1.455→1.456 |

Simple-broad two-frame p50/p95 and warm-deep response p50 are slightly worse;
these four-per-build burst medians are descriptive, not tail guarantees. The
input-contaminated idle windows are excluded only from idle cohorts; burst
cohorts retain all four completed runs per build. Durable
192-vector completion 12.34→12.26 s, CPU 3.11→2.98 CPU-s (slower absolute than
R14 on this machine state, both builds alike). No UI or app-wide speedup is
claimed from the native gain; the desktop Knowledge index is small.

## RSS versus physical footprint

- Within a run, WebContent footprint falls after hide (e.g. 325–518 → 88–360 MiB)
  while its RSS stays flat: freed allocator pages remain resident until the
  kernel takes them.
- From the harness's own `vmmap -summary` receipts, WebContent `WebKit Malloc`
  resident − dirty ("clean resident") after the model phase is **0–216 MiB in
  control** and **0–301 MiB in candidate**; the same R14 receipts give
  49/112 (baseline) vs 299/155 MiB (candidate) while candidate *dirty* is lower
  (50/55 vs 63/96 MiB). Dispatched request counts are identical.
- So the R14 RSS increase is clean, reclaimable libpas residency with large
  within-build variance. Quiet-cohort RSS medians are still higher for candidate
  (657 vs 514 MiB post-search) with lower footprint (184 vs 240 MiB); with n=3–4
  and 0–300 MiB within-build spread this is **not** shown to be build-caused, and
  **no RAM win is claimed** from the lower footprint either. Footprint sums are
  ledger sums; RSS sums double-count shared/clean pages; 0.5 s sampling misses
  peaks. Renderer bytes are unchanged by this work.

## Production change: qualified-target activation

R14's `supported()` gated only the SQLite version, while bit-exact arithmetic was
executed only for optimized macOS arm64 builds; CI runs `cargo +1.85.0 test --lib`
in the **debug** profile on its OS matrix and ships optimized builds.
`cb3a74b` keeps `tezbar_bm25` registered whenever the engine is 3.46.0 (so the
strict oracles still execute on every CI target) but selects the joined query for
production only when `cfg!(all(target_os="macos", target_arch="aarch64"))`.
Elsewhere launcher and Knowledge use the unchanged built-in BM25 SQL. Engine
upgrades still fall back. No ranking, cap, budget, tie or semantic change.

- New callsite tests: `production_search_selects_joined_rank_only_on_qualified_target`
  (asserts the 3.46.0 pin, the selected SQL and an actual `sz` query-plan join)
  and `store_selects_joined_rank_only_on_qualified_target`. Red check: forcing
  the gate false fails both while the joined-vs-builtin oracles still pass
  (function registered) — exactly the unqualified-target behaviour.
- Oracles pass in release (Rust 1.98), **debug** (CI profile) and **Rust 1.85.0
  release** on macOS arm64: 7/7 each. Local only; **not** Windows/Intel certified.
  No remote build/test was run.

Native broad-search evidence from R14 (reader broad p50 26.326→13.103 ms, 112
full replies, 192 coupled-seed comparisons) is **reused, not re-measured**: on
this target the production search code is unchanged (`QUALIFIED_TARGET` is a
compile-time `true`), confirmed by the callsite test.

## Checks and identities

- Release Rust **457 passed / 9 ignored** (R14's 455 + 2 new); TypeScript **752
  passed / 9 skipped**; `pnpm typecheck`, `build:backend`, `build:frontend`,
  ordinary and bench `.app` release builds with `codesign --verify --deep
  --strict`. Generated worker artifacts restored; tracked tree clean.
- Parity/opening (`responsiveness-checks.py --apps --broad`, control vs
  candidate): 20 fresh final replies equal direct native rows/actions; synthetic
  IME + native hide/reopen passes; opening medians control 1253 vs candidate
  1078 ms (three each, uncontrolled OS cache; slower than R14's ~860 on both).
- 90 s same-model retention (`renderer-retention.py`): both keep PID/start, exact
  ordered parity; fresh response 31→35 ms, two-frame 54→65 ms (one pair, not a
  tail claim).
- 767 observed PID/start identities, **zero alive** after the runs; 48 marked
  fixture clipboard stores scrubbed without inspection; installed Q4/Q8 model
  inventory (88 files, size/mtime/inode) unchanged; unrelated HTML SHA-256
  `1ee1b074…` unchanged.

| Executable (`cb3a74b`) | SHA-256 |
|---|---|
| Candidate ordinary | `162a627c112953d7f85d1f29924017b1133ccb0e92b99f8899cf7d3ebb6b859a` |
| Candidate bench | `29d8c44d421bfb5b94873012f5aeb07cb6699ee447fb72f18f616ff4bb25bedc` |
| Control bench (+ `control-test-only.patch`) | `09cbdd920e52dbdcfcc8c3ade5823ab3515c3777a66c4e4944020d5eb55fde7e` |

Reproduce (new output paths): `build-apps.py` in the artifact root;
`post-search-probe.py --app … --output … --label … [--model …] [--vmmap]
[--stack|--trigger]`; `broad-full-quiet.py --app … --model … --output …
--production-label current|candidate`; `summarize-quiet.py --root … --runs
control=… candidate=… --output …`; `responsiveness-checks.py --apps --broad` and
`renderer-retention.py` as in R14. `artifact-manifest.json` pins 253 raw files and
source/harness hashes.

## Remaining gaps

- Uniform 5× runtime/search and one-fifth RAM: unmet. Retained footprint is
  nowhere near one fifth of baseline.
- The desktop harness still cannot prevent physical input; it now detects and
  excludes it. Earlier R5–R14 idle windows had no such receipt and may contain
  similar contamination; they are not reinterpreted here beyond R14.
- No burst-time stack was captured (thread-level ownership only).
- Windows/Intel shipping arithmetic remains unqualified; the joined rank is off
  there by construction.
- Local commits only: no push, PR, remote build/workflow, merge, signing,
  installed-app replacement, publication or deploy.
