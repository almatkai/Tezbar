# Beta.17 renderer regression follow-up (R8)

**Regression reproduced, not fixed. No production optimization is claimed.**
The full lean AB/BA cohort still has **+27.6% simple-burst family CPU** and
**2.70× post-repeat WebContent footprint** versus original beta.17. The
fivefold whole-runtime/search and one-fifth whole-family RAM goals remain unmet.
[Machine-readable results and receipts](beta17-renderer-results.json).

## Scope and identity

Authorized worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-performance`,
branch `fix/beta17-runtime-search-memory`. Start: **`b427ea1ad9ddeaa57836876e7bcba585714c9786`**.
Original: **`9ff3663342b48ab01be2e56ab30f6d7123b0ced8`**. All measurements below
compare those frozen production builds directly, not multiplied incremental ratios.

No renderer/native production source, scheduler setting, result/candidate limit,
ranking, action, cache policy, model, runtime package or lifecycle policy changed.
There is consequently **no third, optimized application candidate**. The change
is a focused reproducible harness, explicit lean measurement protocol, tests and
this negative-result report. No rebuild was needed: the six frozen R7 executables
and four actual `.app` payloads were rehashed; 573 accepted build inputs remain
identical. The R7 measurement-only original reference was neither edited nor rebuilt.
R7 reports, JSON, raw measurements and reference artifacts remain historical evidence.

The original benchmark SHA-256 is
`5a219f1bcff77ffb3dc70100ef21258795ccd7a7867b9e8333ba9f0b26139f74`;
accepted benchmark is
`1cbb16091ef42dc0fe72d009d0d84046663ead874e27b3cbb30ec6fc5923ffac`.
All ordinary/native-test and 160 packaged-file hashes are in the new JSON. The
ordinary-package activation and exhaustive model-boundary checks from R7 are
**reused**, not newly run or relabelled as new timing evidence.

## Tight symptom loop and protocols

`renderer-loop.py` launches a fresh complete frozen app, prepares the same 5,000
synthetic files under OS temp, waits past startup reconciliation, samples five
hidden seconds, then drives the real React input through eight seven-input bursts.
Gaps alternate **2/20 ms**, with identical queries, native visibility, fresh-final-
reply, ordered DOM, two-animation-frame and physical-input guards. It samples
before/after each burst and then eight or thirty hidden seconds. CPU, footprint,
RSS and every observed role/PID/start identity are retained. `vmmap` and `heap`
inspection occur **after** the timing/retention window.

The first original/accepted pair measured **1.265 → 1.489 CPU-s (+17.7%)**.
An assertion allowing 5% CPU regression went red. This was the small, simple-only
repro, before inspecting production code or proposing an optimization.

Two distinct wrapper protocols are retained:

- **old8:** unchanged R7 wrapper/full-final-row socket serialization. Two processes
  per build, AB/BA order. Outside-burst calls can retain full replies until the next
  burst. This is explicitly measurement ownership, not an application cache.
- **lean:** metadata-only completed-call records. Only the fresh latest reply is
  temporarily owned during an active burst's exact DOM check; it is released on
  success or error. No full rows or title arrays are returned through the socket.
  Home/reset/outside-burst calls retain no reply. Fresh reply and two-frame guards
  are unchanged. Ordered scores/actions are checked separately outside RAM timing.

The initial exploratory `lean8` pair preceded the final outside-burst ownership
restriction; it could hold one latest outside-burst reply. It is labelled exploratory,
not the final corrected protocol. The subsequent **lean30 AB/BA and all full R8 runs**
use the restricted owner. Eight Node-backed harness tests exercise the actual
injected scripts, including a deliberately red old-wrapper retention case, fresh
reply versus cached DOM, stale reply ownership, DOM/physical-input/frame guards,
and exception/disconnection cleanup. These test the harness, **not a production fix**.

| Simple-only cohort, two processes/build | Family CPU-s, original → accepted | Last hidden family footprint MiB, individual processes |
|---|---:|---|
| old8 | 1.290 → 1.496 (+15.9%) | 166.94/257.47 → 189.11/285.57 |
| exploratory lean8 | 1.323 → 1.365 (+3.1%) | 165.07/165.18 → 187.85/283.27 |
| final lean30 | 1.303 → 1.374 (+5.4%) | 153.54/158.46 → 179.39/179.74 |

These tiny cohorts do **not** isolate a statistically precise wrapper CPU effect.
They do show that serialization/inspection can perturb CPU and that roughly
90–100 MiB within-build retention variation is possible. Lean instrumentation
is necessary discipline, **not evidence that the regression is a pure artifact**.
The short loop includes eight extra per-burst family observations and omits R7's
opening cycles, so its absolute CPU is not interchangeable with the full cohort.

## Tested hypotheses and causal limits

1. **Extra intermediate work — supported.** Full simple bursts dispatch
   `[1,1,1,1,1,1,1,1]` originally versus `[3,7,3,7,3,7,3,7]` currently: 8 versus
   40 requests for equal inputs. Valid prefixes now complete before the next 20 ms
   input. Diagnostic string payload accounting (no contents retained) finds
   **880 → 3,280 returned rows**, **296,024 → 1,287,517 UTF-8 string bytes** across
   11/41 replies, including outside-burst/home work. This excludes JSON syntax and
   numeric fields; it is not a wire-byte counter. Counting overhead is excluded
   from acceptance timing. This establishes extra work, not a specific heap owner.
2. **UI-update allocation path — localized, owner unresolved.** Diagnostic replay
   of each build's recorded native request cadence **without React input/result
   updates** ends at **157.88 → 157.27 MiB** family footprint, with 0.085/0.238 CPU-s.
   Actual typing ends substantially higher. Removing both optimistic input rendering
   and native-result commits is not an equivalent user workload or a proposed fix;
   it shows that IPC payloads alone do not reproduce the large growth. The replay
   does not distinguish React, optimistic search, icons, DOM or WebKit collection.
3. **Large asset/cache payload — not established.** Diagnostic DOM mutation counters
   see 1,045 → 1,496 inserted and removed nodes, and 25 → 43 asset requests totaling
   74,114 → 127,478 data-URL characters. Observed row counts are approximately 80–90,
   not a 1,000-mounted-row explosion. Persisted candidate strings total ~88 KB.
   These counters do not account for ~100+ MiB by themselves. `heap --noContent`
   identifies mostly unclassified allocation bytes, plus detached/not-yet-collected
   DOM/SVG objects. It provides **no JavaScript retainer path** proving a live
   query/icon-cache leak. Mutation observers themselves perturb this diagnostic.
4. **WebKit allocation/collection variation — supported as a contributor, not an
   excuse.** Unchanged fresh processes have sharply different settled ledgers.
   Nevertheless both final full lean processes exceed both original renderers.
   `vmmap` post-repeat WebKit allocated bytes are original **74.7/71.3 MiB** versus
   current **181.4/201.2 MiB**; dirty bytes are also higher. This is not only RSS
   residency noise. Malloc totals do not distinguish reachable app objects from
   unreachable/uncollected JS/DOM blocks or allocator high-water state.

**Production decision:** do not restore 80/180 ms debounce, suppress intermediate
results, shrink caches/limits, force GC, destroy the WebView or unload the model to
make this pass. The extra dispatch work is causally explained by fast scheduling;
its exact avoidable allocation owner is **not** established. No measured safe
production fix emerged. This is an incomplete performance outcome, not a claim that
RAM cost is intrinsic or that optimization is impossible. The next concrete seam
is allocation/retainer attribution during input-result commits, distinguishing
reachable cache/closure/DOM owners from uncollected blocks. A stable-identity or
render-churn fix needs that evidence and an unchanged-output desktop comparison;
it is not justified merely by the allocation totals here.

## Full revised R7 workload: original / current / current / original

Same 96 documents, 192 actual learned vectors, model/settings, source/vector indexing
cadence, opening cycles, simple/deep bursts and thirty-second quiet windows as R7.
Only reply ownership/serialization changes. Actual frozen `.apps` supply their own
frontend/resources. Complete observed family accounting includes WebKit, GPU,
network/platform helpers, host, model, supervisor and observed compiler helpers.
Footprints sum physical ledgers, **not deduplicated RAM**; summed RSS can double-count.
Half-second sampling can miss brief helpers/peaks. No build/test overlapped timing.

Medians below are medians of **per-process nearest-rank** p50/p95 values, never pooled
percentiles. Small cohorts and uncontrolled OS/shader caches limit tail conclusions.

| Equal workload | Fresh reply p50 / p95 ms, original → current | Reply + two frames p50 / p95 ms, original → current |
|---|---|---|
| Simple bursts | 95 / 99.5 → 14 / 15.5 | 122 / 134 → 38 / 46 |
| Deep lexical | 195 / 205 → 13.5 / 14 | 216.5 / 232 → 42 / 45.5 |
| Warm deep model | 219.5 / 229.5 → 24.5 / 35.5 | 245 / 256 → 51 / 57.5 |
| Repeated warm deep | 224 / 227 → 24.5 / 36.5 | 250 / 256.5 → 52.5 / 63.5 |
| Typing during source indexing | 92.5 / 99.5 → 14 / 26 | 115.5 / 122 → 40 / 52.5 |
| Typing during vector indexing | 96 / 98.5 → 25 / 26.5 | 106 / 114 → 50 / 61 |

Simple fresh response remains **6.79× faster**, but its two-frame metric is only
**3.21×**. Warm-deep frame p50 is **4.80×**. Uniform fivefold response/frame performance
is still absent; cached/optimistic DOM alone never satisfied a timed completion.

| Work | Family CPU-s, original → current |
|---|---:|
| Eight simple bursts | **1.050 → 1.340 (+27.6%)** |
| Four deep lexical bursts | 0.527 → 0.489 |
| Eight warm deep bursts | 1.214 → 0.628 |
| Sixteen repeated deep bursts | 2.436 → 1.189 |
| Typing/source-indexing phase | 0.495 → 0.606 (+22.4%) |

Durable 192-vector completion is **23.208/14.473 → 18.457/10.353 seconds**;
CPU through durable completion is **25.570/9.198 → 18.989/2.465 CPU-s**.
The first original **and** first current process observed Metal compiler services;
they remain counted, not discarded. These uncontrolled first-use outliers explain
why this cohort cannot stand in for R7's warm indexing values or prove a stable
indexing speedup. All four processes completed the unchanged useful workload.

| Thirty-second quiet phase | Family footprint median MiB, original → current | Settled summed RSS MiB, original → current |
|---|---:|---:|
| Initial hidden | 144.00 → 143.68 | 328.07 → 324.97 |
| After simple work | **165.94 → 311.22** | 548.32 → 579.30 |
| Retained model after indexing | 553.19 → 469.91 | 1,314.95 → 1,133.95 |
| After repeated deep work | 548.00 → 460.60 | 1,317.12 → 1,160.94 |

Individual retained-model family footprints are original **611.44/494.93** versus
current **520.44/419.38 MiB** (phase medians). Compiler helpers remain in those
ledgers if present. One-fifth of this cohort's original retained family is
**110.64 MiB**; current **469.91 MiB** misses by **359.27 MiB**. There is no one-fifth
claim. Post-repeat exact last WebContent ledgers are original **79.45/82.02** versus
current **218.66/217.09 MiB**; the model still retains roughly 323 → 103 MiB.
No renderer RAM win is hidden inside the separate model-package saving.

Initial-hidden quiet CPU is 0.145 → 0.119 CPU-s per thirty seconds; post-simple
0.202 → 0.154; retained-model 0.295 → 0.279; post-repeat 0.271 → 0.255. None is 5×.

## Opening, retention and behavior

Full-workload first-UI readiness is original **1009/1014 ms**, current **1207/1074 ms**:
this small cohort is slower, not silently called an opening improvement. Six separate
opening checks give original **1201/868/870 ms**, current **845/848/857 ms**. The
200 ms-polling warm-cache proxy is noisy; these do not establish a deterministic
opening regression or a cold-start guarantee. Viewports remain 760×640 at 2×.

The new lean **90-second** quiet pair preserves the exact same model PID/start
identity under the unchanged 120-second policy. First fresh visible reply is
**234 → 33 ms**, plus two frames **267 → 65 ms**. The latter remains below 5×.
No keepalive, forced GC, pause/unload or page recreation was used. Full ordered
rows/scores/actions collected afterwards match on cloned authoritative indexes.
Sixteen further UI/native replies across simple/deep modes match exactly; synthetic
IME interruption plus native hide/reopen passes. These are not physical OS-IME or
cross-platform certification.

All four fresh full indexes retain 96 sources / 192 chunks / 192 vectors, identical
model/index identity, 256-F32 dimensions and vector digest
`27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c`.
Checkpointed post-exit DB copies are pinned under `closed-fixtures/`. CJK, exact
embedding outputs, 8192 support and unchanged 8193 rejection are explicitly reused
from R7 because production/model bytes are unchanged; the 8-GiB boundary was not rerun.

## Checks, artifacts and reproduction

- **747 TS tests passed, 9 skipped**; typecheck passed. This includes all 27 actual
  CommandBar lifecycle tests (slow backend, pending/stale replies, reopen, mode,
  invalidation, one-character and IME cases).
- **8 new harness tests passed**. The artifact regression check intentionally
  **fails**: CPU ratio **1.2763**, retained-WebContent ratio **2.6986**, versus a
  1.05 no-regression envelope. Do not count that red check as an acceptance pass.
- **450 Rust tests / 8 ignored reused from b427** with unchanged source fingerprints
  and executable/log pins. No new native production change or rebuild occurred.
- **553 observed PID/start identities** checked independently after shutdown;
  **zero survivors**. No owned app/model/compiler remains. Incidental clipboard
  stores were scrubbed only inside marked OS-temp fixtures after exit, without
  reading or retaining contents. The normal observer remained enabled.
- Both installed model weights and 122 existing manifest/runtime files still match
  their R7 pins. Only read-only APFS clones were used. The unrelated untracked HTML
  remains SHA-256 `1ee1b0742261dbfb1ad8071817778b5881cb82c084bda482684117279f45816d`.

New ignored evidence root: **`build/performance-renderer-20261030/`**. It retains
all short runs, diagnostics, full AB/BA JSON/vmmap/logs, `retention90-lean.json`,
`parity-opening.json`, source/app/test pins, closed synthetic DBs and cleanup audit.
`harness-used-full/` reconstructs the exact pre-UTF8-accounting/extra shutdown-check
revision used for full/lean30 timing; those later changes affect diagnostics/error
handling only. An initial report-assembly attempt failed opening a checkpointed
read-only WAL-mode copy; its log remains. The corrected assembly uses `immutable=1`
only for verified post-exit DB copies with absent/empty WAL, never live measurements.
No desktop timing attempt was excluded or failed in this cohort.

From the authorized worktree, choose **new** output directories/names:

```sh
python3 docs/performance/renderer-loop.py \
  --app build/performance-runtime-20261030/cumulative-20261030/accepted-bench.app \
  --protocol lean --idle-seconds 30 --vmmap \
  --output build/performance-renderer-20261030/NEW-focused
# Repeat original/current/current/original, then the full unchanged workload:
python3 docs/performance/renderer-full.py \
  --app build/performance-runtime-20261030/cumulative-20261030/accepted-bench.app \
  --model "$HOME/Library/Application Support/com.tezbar.app/knowledge/models/embeddinggemma-2-q4_k_m" \
  --output build/performance-renderer-20261030/NEW-full
python3 -m unittest discover -s docs/performance -p 'renderer_loop_test.py'
# Recheck this frozen cohort; expected RED while production remains unchanged:
python3 docs/performance/summarize-renderer.py \
  --artifacts build/performance-renderer-20261030 \
  --output build/performance-renderer-20261030/NEW-summary.json \
  --assert-no-regression
```

No push, PR, merge, remote build/workflow, signing-secret access, publication,
installed-app replacement, global service change, clipboard write or unowned
process termination occurred. The regression remains a release/acceptance limitation.
