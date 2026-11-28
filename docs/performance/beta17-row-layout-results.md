# Beta.17 row-layout experiments (R11)

**No production fix qualified. Both prototypes are reverted.** Actual WKWebView
row skipping increases CPU. Selected-geometry batching saves CPU in the full
cohort, but a late-rAF selection commit leaves the old highlight target until the
next frame. Retained RAM also fails to demonstrate a safe joint win. Uniform 5×
runtime/search and one-fifth whole-family RAM remain unmet.
[Measurements and hashed receipts](beta17-row-layout-results.json).

Only the authorized `Tezbar-performance` worktree and
`fix/beta17-runtime-search-memory` branch were used, starting at
`e94d0e050a0013771faf1b9b1ce2a04164d7ef53`. XERJ returned no match; `rg` was used
without changing the shared service. R7–R10 and the unrelated HTML are untouched.

## Actual row skipping and causal layout probes

The frozen accepted R7 benchmark app supplies the pre-fix production frontend.
`row-layout-probe.py` preserves the lean fresh-final-native-reply, complete ordered
DOM and two-frame guards. Extended diagnostics repeat 128 actual seven-input
bursts, with bounded getter timers and ten-second `/usr/bin/sample` attaches only
to PID/start-verified owned host/WebContent processes. All six attaches succeed.
These instrumented runs are **diagnostics, not acceptance CPU/latency/RAM**.

WKWebView reports support for `content-visibility:auto` and
`contain-intrinsic-block-size:auto 36px`. CSS targets only the search-result `li`
elements; all logical rows remain in DOM. Actual auto-state events report 18,045
skips and 3,979 unskips. The unremembered 36px intrinsic estimate is limited to
this ordinary-row corpus; it is **not** a production height assumption. Real
calculator/color/currency rows measured later are 53px.

| Diagnostic | Row offsetTop reads / total ms | Family CPU-s | WebContent CPU-s |
|---|---:|---:|---:|
| Pre-fix | 3,199 / 3,412 | 15.074 | 8.924 |
| Row content-visibility | 3,206 / 3,916 | 16.400 | 10.194 |
| Batched selected geometry | 918 / 246 | 15.120 | 9.457 |

Row skipping is supported and observable, but does not reduce the forced-layout
callsite. It is rejected after this negative probe, without a production CSS
change or full CSS matrix. This is not R10's wrapper `contain:layout` experiment.

The separate batching prototype opts in only the main search-result GlideList.
It retains one latest measurement callback per animation frame, then writes the
same highlight transform/height/opacity directly. It removes synchronous child
geometry-state commits; input scheduling, parent width state, all result rows,
limits, icons, handlers, model/runtime policies and CSS transitions are preserved.
The initial short/profiler build and refined full build are separately pinned;
the latter preserves the unbatched hidden-rectangle behavior of other GlideLists.

Batching removes 71.3% of getter calls and 92.8% of measured offsetTop wall time.
Native inclusive `jsHTMLElement_offsetTop` samples fall 1,345→79, while
`Page::updateRendering` rises 242→518 and `Document::updateStyleIfNeeded` 8→734.
The instrumented total CPU does not improve. These overlapping native ancestry
counts are not additive CPU percentages, and unsymbolized JIT frames do not
identify a JS retainer. Fewer forced reads alone would not qualify this candidate.

## Uninstrumented short and full confirmation

Each cohort uses pre-fix/candidate/candidate/pre-fix fresh actual `.app` processes,
without builds/tests or getter timers overlapping measurements. Every observed
helper remains charged, including QuickLook. Medians below are medians of process
summaries, not pooled percentiles or confidence intervals.

| Short order | Family CPU-s | WebContent CPU-s | QuickLook CPU-s |
|---|---:|---:|---:|
| Pre-fix 1 | 1.602 | .899 | .262 |
| Batching 1 | 1.395 | .841 | .132 |
| Batching 2 | 1.385 | .849 | .099 |
| Pre-fix 2 | 1.484 | .887 | .155 |

Short median family/WebContent CPU changes are **−9.9%/−5.3%**. The full unchanged
R8 lean workload then includes opening, source indexing, real model inference,
96 documents, 192 durable 256-F32 vectors, simple/deep searches and quiet phases.

| Full order | Simple family CPU-s | WebContent CPU-s | Post-repeat WebContent MiB | Post-repeat family median MiB |
|---|---:|---:|---:|---:|
| Pre-fix 1 | 1.388 | .734 | 86.27 | 275.66 |
| Batching 1 | 1.187 | .665 | 79.17 | 268.55 |
| Batching 2 | 1.134 | .624 | 107.30 | 294.05 |
| Pre-fix 2 | 1.438 | .771 | 83.42 | 277.37 |

Full simple CPU improves **1.413→1.160 CPU-s (−17.9%)**, WebContent
**.752→.644 (−14.4%)**. Simple native dispatches remain `[3,7,3,7,3,7,3,7]`;
all 40 requests per process finish. Fresh-reply p50/p95 is **14.5/15.5→14/15 ms**,
two-frame opportunities **39/49→37.5/46 ms**. These are not pixel-paint times.

Other phase family CPU medians, pre-fix→rejected batching: deep lexical
**.470→.426**, warm deep **.810→.629**, repeated deep **1.177→1.176**,
typing/source indexing **.599→.624**, typing/vector indexing **2.519→2.471 CPU-s**.
No uniform runtime improvement follows. Complete dispatches, latency tails,
per-role CPU/RSS, helper outliers and quiet samples remain in JSON/raw receipts.
Four closed immutable DB copies retain identical 96/192/192 counts, model identity
and vector digest `27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c`.

Durable completion is **10.445/10.523→9.899/10.302 s**, durable family CPU
**2.482/2.554→2.473/2.468 CPU-s**. Opening proxies are pre-fix **1222/1200 ms**,
candidate **1201/995 ms**, at 760×640, 2×. Coarse warm-cache polling does not certify
cold-start tails. Ordinary and benchmark release builds pass ad-hoc verification;
no new ordinary activation or original/final acceptance comparison is claimed.

## RAM trajectory and the first-frame blocker

Full quiet family footprint medians, initial/post-simple/model-retained/post-repeat:
pre-fix **144.25/276.77/329.00/276.52 MiB**, batching
**143.16/196.93/282.55/281.30 MiB**. Corresponding thirty-second CPU is
**.121/.156/.283/.270→.127/.149/.293/.261 CPU-s**. Post-repeat WebContent median
is **84.84→93.23 MiB**. These small cohorts and R10's unchanged 74–195 MiB range
do not establish a stable causal RAM regression or a safe RAM improvement.

A separate, uninstrumented **diagnostic** fresh pair runs four identical eight-burst
blocks, each followed by 15 hidden seconds. Lean reply ownership remains bounded;
no GC, model, keepalive or lifecycle ablation is used. Hidden family medians:

| Diagnostic process | Block 1 | Block 2 | Block 3 | Block 4 | Final 30s |
|---|---:|---:|---:|---:|---:|
| Pre-fix | 187.43 | 216.72 | 169.79 | 166.40 | 161.15 |
| Batching | 186.52 | 222.66 | 227.33 | 240.15 | 230.02 |

Final WebContent is **78.58→150.88 MiB**. The pair exposes different
allocation/collection trajectories under sustained identical work; it does not
prove reachable leakage, distinguish allocator from GC ownership, or replace the
original acceptance workload. No favorable process/window was selected as final.
Footprints sum physical ledgers, not deduplicated RAM; summed RSS can double-count.
Sampling every .5s can miss brief helpers and peaks.

Actual WKWebView bounds checks cover ordinary/special rows, complete first/last-row
keyboard navigation, long titles, native window resize/zoom, focus, wheel suppression
and hide/reopen. Early observer callbacks sometimes see the previous target, but
callback-order tracing shows those ordinary updates still occur in the **same**
frame. Those observations are not falsely labelled painted flashes. A direct CSS
font-size change without a React commit leaves stale geometry in both builds;
the next query commit updates it. That pre-existing limitation is retained openly.

The discriminating late-rAF case differs: a real selection commit inside an
animation callback selects row 1 at top **44px**, but batching leaves target **6px**
until its next callback, timestamp **13793→13809 ms**. Pre-fix has target **44px**
at the current commit. Thus the scheduler cannot guarantee same-frame highlight
updates for every commit. This is actual WKWebView callback/DOM geometry evidence,
not a pixel screenshot claim. The actual CommandBar regression goes red
(`translateY(6px)` instead of `44px`) on batching and green on rollback.

## Rollback and evidence

The technical blocker is a batching boundary that preserves late-frame commits
and dynamic geometry while demonstrating a joint CPU/RAM win. The tested rAF
boundary fails behavior despite a credible CPU reduction; supported automatic row
skipping does not remove the hot work. Neither result justifies changing debounce,
limits, models, visual design or framework. Production remains R7/ac14c06.

- **751 TS tests / 9 skipped, typecheck and restored frontend build pass.** All
  31 real CommandBar lifecycle tests pass. Four prototype batching tests were
  exercised red/green and archived; the new same-frame CommandBar guard remains.
- **8 lean harness, 2 R10 probe and 4 new probe/order tests pass.** An initial new
  harness test had a strict-mode undeclared-variable error; its failed log and
  corrected rerun are retained separately. No desktop measurement failed/excluded.
- **573 existing build-input pins** match R10 except the expanded lifecycle test.
  Native/runtime production, all frozen R7 app payloads/executables, **450 Rust
  tests / 8 ignored**, **40 exact native signatures**, **84 exact model artifacts**,
  both installed model inventories and the pinned runtime ZIP are rehashed/reused.
  No numerical rewrite or 8-GiB/model-boundary rerun occurred.
- All observed PID/start identities are audited after exit, with zero survivors.
  Normal clipboard observation stayed enabled. Only marked OS-temp fixture stores
  were scrubbed after exit, without reading/hashing clipboard contents. The unrelated
  HTML remains SHA-256 `1ee1b0742261dbfb1ad8071817778b5881cb82c084bda482684117279f45816d`.

Evidence root: **`build/performance-row-layout-20261030/`**. It retains raw short/full
and diagnostic runs, actual sample trees, callback-order/geometry traces, three
**rejected** app payloads, patches/source pins, build/test commands, closed indexes,
and the hashed manifest/details linked from JSON. Final rejected benchmark SHA-256:
`5c0c2e49c972e3fdbf1113d60cbc56f02eb1af041448f53787efc323d8d3afe9`;
ordinary: `6f88abd008e4a6b5a8c3062484941c1cdef79d1522b1a001ad4ec06dce096d94`.
The unchanged accepted benchmark remains
`1cbb16091ef42dc0fe72d009d0d84046663ead874e27b3cbb30ec6fc5923ffac`.
Reproduce probes with new output names using `row-layout-probe.py` (`--sample`,
optional `--skip-rows`), `row-layout-frames.py` (`--late-raf` or `--extended`), and
`row-layout-trajectory.py`; ordinary timing remains `renderer-loop.py`/`renderer-full.py`.

No new exact UI action-parity/90-second-model-retention or physical drag/OS-IME/
accessibility/pixel-paint acceptance claim is made for this rejected candidate.
Prior accepted evidence remains historical. No subagents, push, PR, remote build,
workflow, merge, deploy, signing-secret access, installed-app replacement, shared
service/security change or publication occurred.
