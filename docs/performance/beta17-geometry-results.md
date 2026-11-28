# Beta.17 same-cycle geometry boundary (R12)

**No production fix qualified. The narrowed hybrid fixes R11's demonstrated late-rAF
failure, but is rejected after actual release CPU/RAM qualification. Production is
restored. Uniform 5× runtime/search and one-fifth whole-family RAM remain unmet.**
[Process summaries and hashed receipts](beta17-geometry-results.json).

Only the authorized `Tezbar-performance` worktree and
`fix/beta17-runtime-search-memory` branch were used, starting at
`3fea98fe4dd272844313b9a1a8cac24d9044cddc`. No subagents or branch/tree changes.
XERJ returned no match; `rg` fallback required no shared-service intervention.
R7–R11 and the unrelated HTML are unchanged.

## Implemented boundary and red/green correctness

First reapply **exactly** R11's archived `batch-final.patch`. The existing real
CommandBar test fails in 0.56 s: `translateY(6px)` instead of `44px` after selection
commits inside rAF, with the next frame held pending. This is the specific bug,
not an early observer observation incorrectly labelled a painted flash.

Ranked boundaries were (1) immediate transitions + stable-target microtask,
(2) synchronous geometry writes on every commit, and (3) after-layout
ResizeObserver delivery. **Only (1) was implemented and measured.** No row skipping,
row/icon memoization, canvas caching or width-only experiment was repeated.

The candidate opts in only the main search-results GlideList. It writes the same
highlight styles directly, without a geometry-state commit. Selection/index,
selected DOM identity, count, immediate-predecessor/reorder and empty/visibility
transitions measure immediately. One latest microtask measures current DOM after
the React/layout-effect stack, coalescing stable-target nested commits. Transitions
also receive that post-ancestor-layout recheck. There is **no next-rAF scheduler,
row-height assumption, observer, stored row array or rect reuse**. Position-only,
font and height changes on a commit read actual `offsetTop`/`offsetHeight`, before
writing highlight styles. Unmount/opt-out clears pending measurement and target
references. Other GlideLists retain their existing path.

- All **39 initial component/lifecycle tests** pass. Eight prototype component
  tests cover stable-target coalescing, 53/71px and position-only changes,
  ancestor effects, replacement/reorder, empty, unmount/opt-out and follow/wheel
  behavior. Six fail against old R11 batching. They are archived with the patch,
  not left as tests of an absent production API after rollback.
- Actual WKWebView `--late-raf --extended` has target **44px** in the selection
  callback at timestamp **13727**, before the next timestamp **13743**. Production
  reads occur at 13730 ms; the current callback's capture also occurs at 13730 ms.
  No extra frame is needed. The existing CSS highlight animation remains active;
  its interpolated pixel bounds are not expected to jump to the target instantly.
- Ordinary input, calculator/color/currency rows, first/last keyboard navigation,
  focus, long titles, native resize/zoom, wheel suppression and hide/reopen have
  matching target top/height in the extended trace. A font-only stylesheet change
  without a React commit retains R11's pre-existing stale-height limitation; the
  next query commit repairs it. This is not claimed as a new font-observer fix.
- A separate diagnostic forces bridge replies through actual CommandBar with
  labelled synthetic rows: equal-height new row, reorder, 71px height change,
  empty and repopulation. All final orders, six fresh native completions, target
  geometries and focus checks pass. This is not exact native-result parity or
  acceptance timing. Initial fixture attempts accidentally selected the normal
  Deep Search recommendation, then used the intentional one-character native
  search bypass. Those inadequate probes are preserved and **not** used as proof;
  the corrected fixture has explicit verdicts and zero physical-input events.
- A new real-CommandBar regression keeps an equal-height same-index fresh row
  replacement current without another frame. Both it and the original late-rAF
  guard are red on old R11 and green on the hybrid **and restored production**.

Bounds reads intentionally force layout. These are commit/callback/DOM target
observations, **not pixel-paint measurements**. Fresh action data is not replaced
with a visual memo. No production trace arrays enter the timed apps.

## Actual release CPU: short and unchanged full workload

The candidate is a real release `.app`, with its own embedded frontend/resources.
Pre-fix is the frozen accepted R7 app, not a historical rebuild or rejected R11
app. Short and full cohorts independently run **P/C/C/P**, with no builds/tests
or diagnostics overlapping timing. Every helper/outlier remains charged.

| Short process | Family CPU-s | WebContent CPU-s | QuickLook CPU-s |
|---|---:|---:|---:|
| Pre-fix 1 | 1.516 | .884 | .186 |
| Hybrid 1 | 1.471 | .863 | .166 |
| Hybrid 2 | 1.587 | .816 | .350 |
| Pre-fix 2 | 1.614 | .875 | .282 |

Short family/WebContent medians improve **2.3%/4.6%**, but family ranges overlap.
This modest WebContent signal warranted the full unchanged R8 lean workload:
96 documents, 192 real durable 256-F32 vectors, source/vector indexing, deep/simple
searches, model-retained and thirty-second quiet phases.

| Full process | Simple family CPU-s | WebContent CPU-s | Post-repeat final WebContent MiB | Post-repeat family median MiB |
|---|---:|---:|---:|---:|
| Pre-fix 1 | 1.396 | .765 | 79.58 | 317.05 |
| Hybrid 1 | 1.306 | .645 | 79.30 | 268.63 |
| Hybrid 2 | 1.374 | .731 | 168.97 | 356.91 |
| Pre-fix 2 | 1.341 | .714 | 82.61 | 271.65 |

Full simple family CPU is **1.368→1.340 CPU-s (−2.1%)**, WebContent
**.740→.688 (−6.9%)**. Both ranges overlap. The corrected boundary does **not**
preserve R11's roughly 18% family CPU reduction: it coalesces within a React
stack/checkpoint rather than across all commits before a later rAF. Immediate
transitions also retain necessary synchronous reads. Counts are not acceptance.

Every simple process still completes the same 40 native requests, cadence
`[3,7,3,7,3,7,3,7]`. Fresh response p50/p95 medians are **14.5/16.5→14.5/15 ms**;
two render opportunities **39.5/47→37/47 ms**, not paint times.

| Full phase median | Family CPU-s, pre-fix → hybrid | WebContent CPU-s |
|---|---:|---:|
| Deep lexical | .458 → .463 | .193 → .199 |
| Warm deep | .780 → .656 | .241 → .241 |
| Repeated deep | 1.155 → 1.169 | .411 → .436 |
| Typing/source indexing | .561 → .625 | .277 → .304 |
| Typing/vector indexing | 2.592 → 2.573 | .517 → .505 |

Warm-deep family CPU improves 15.9% without a WebContent improvement; retain the
per-role/helper records rather than attributing that whole difference to geometry.
Source-index input family CPU rises **11.5%**. Repeated-deep dispatches differ by
one call in one candidate process; vector-index burst counts are 5/4/5/4 across
P/C/C/P because completion ends that phase. They are not fixed-dispatch causal
comparisons. Four closed immutable indexes have identical **96/192/192** row
counts, model identity and vector digest
`27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c`.
Durable completion is pre-fix **10.665/10.134 s**, hybrid **10.125/10.693 s**;
no indexing win. Opening proxies are **1220/1212→1014/1194 ms** at the same
760×640, 2×. These coarse warm-cache observations are not a cold-start guarantee.

## RAM and discriminating ownership/trajectory check

Full quiet family footprint medians, initial/post-simple/model-retained/post-repeat:
pre-fix **143.83/234.21/296.38/294.35 MiB**, hybrid
**143.85/212.78/317.67/312.77 MiB**. Post-repeat final WebContent is
**81.10→124.13 MiB**, but the hybrid pair spans **79–169 MiB**. This does not prove
a stable causal regression; unchanged R10 spans 74–195 MiB. Nor does that prior
variance automatically acquit this candidate. Post-repeat summed final RSS is
**895.43→954.06 MiB**; full per-role CPU/RSS and helper samples remain in evidence.

The smallest additional discriminating check is a reversed **C/P diagnostic pair**:
four identical eight-burst blocks, each followed by 15 genuinely hidden seconds,
then 30 hidden seconds. No GC, early model unload, favorable-window selection or
lifecycle ablation. It is not substituted for acceptance workload results.

| Diagnostic process | Hidden block 1 family MiB | Block 2 | Block 3 | Block 4 | Final 30s |
|---|---:|---:|---:|---:|---:|
| Hybrid | 193.80 | 200.60 | 198.10 | 200.60 | 195.22 |
| Pre-fix | 188.46 | 233.02 | 209.71 | 170.30 | 163.65 |

Final WebContent is **112.69→80.74 MiB** in chronological C/P order. The candidate
plateaus rather than reproducing R11's steadily rising trajectory. All four quiet
checkpoints have **1,198 DOM nodes, 81 rows and one highlight** in both processes.
Candidate microtasks have **maximum one pending**, **zero pending at every quiet
checkpoint**, and all **590 scheduled callbacks finish** by the final checkpoint.
No final reply remains owned, and active bridge calls are zero. Unit cleanup tests
also prohibit reading detached DOM after unmount/opt-out.

Those bounds rule out an accumulating queued-callback backlog in this fixture,
not all reachable DOM retention or allocator/GC effects. Missing qualification is
specific: a robust total CPU benefit under the corrected boundary, and repeatable
joint non-regression of retained family RAM under the full workload. The weak
family CPU gain, source-index input cost and unresolved full/trajectory RAM spread
do not justify retaining extra production scheduling machinery. No generic
“unsafe RAM” verdict, stable +53% causal claim or retainer cause is asserted.

## Rollback, checks and reproducibility

- **752 TS tests / 9 skipped**, typecheck, restored frontend build and **18 harness
  tests** pass. Candidate typecheck and expanded **32** CommandBar tests pass.
  Production CommandBar/GlideList are restored byte-for-byte; of **573** prior
  input pins only the intentional lifecycle test differs.
- **450 Rust / 8 ignored**, **40 exact native signatures**, **84 model artifacts**,
  installed Q4/Q8 inventories and runtime ZIP are explicitly rehashed/reused.
  R11's **44 harness / 186 raw** pins, all three rejected app inventories, and all
  frozen R7 app inventories match. No native/model/8-GiB matrix is rerun.
- **248 PID/start identities audited, zero survivors.** Normal clipboard observation
  remained enabled; no contents were read/hashed/reported. Only marked OS-temp
  fixture stores were scrubbed after exit. The unrelated HTML remains SHA-256
  `1ee1b0742261dbfb1ad8071817778b5881cb82c084bda482684117279f45816d`.

Artifact root **`build/performance-geometry-20261030/`** retains the rejected patch,
prototype tests/source/build pins, 15 actual-app receipts (including the inadequate
fixture attempts), raw process/geometry evidence, four closed indexes and one
**rejected** release benchmark app. Executable SHA-256:
`33f350eeffd5c6eab367ba004081e04b55513fd46b022c94ee3793d8afdbb6b9`.
The manifest pins its 41-file payload and 111 raw/source/receipt artifacts. Existing
accepted benchmark remains
`1cbb16091ef42dc0fe72d009d0d84046663ead874e27b3cbb30ec6fc5923ffac`.

Reproduce with new output names: `row-layout-frames.py --late-raf --extended`,
`geometry-frames.py`, ordinary `renderer-loop.py --protocol lean` / `renderer-full.py`,
and diagnostic `geometry-trajectory.py --protocol lean --idle-seconds 30`.
`run-short.py`, `run-full.py` and build commands in the artifact root retain exact
paths/order. `summarize-geometry.py` verifies rollback, prior/new inventories,
durable rows/vectors and owned-process exits before freezing the compact report.

Accounting is `proc_pid_rusage` **125/3 ns**, responsibility plus recursive PPID
ownership, every observed QuickLook/WebKit/model/helper role, .5 s sampling.
Footprint sums physical ledgers, **not deduplicated RAM**; summed RSS can double-count.
Sampling can miss transient peaks/helpers; two-process cohorts and collection/cache
variation limit causal conclusions. No outlier or role is discarded.

No new accepted ordinary app, original/final acceptance matrix, exact action-parity,
OS-IME/drag/pixel-paint or 90-second model-retention claim is made for this rejected
candidate. Those final gates were not reached; prior evidence is historical.
No push, PR, remote build, workflow, merge, signing-secret access, publication,
deploy, installed-app replacement or global/security/service change occurred.
