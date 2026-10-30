# Beta.17 synchronous-layout experiments (R10)

**No production fix qualified. All prototypes are reverted.** Actual WebContent
stacks now identify synchronous result-row layout, not input font measurement,
as a substantial CPU callsite. Avoiding the input-width parent commit saves CPU,
but the repeated full cohort does not clear the RAM constraint. Uniform 5×
runtime/search and one-fifth family RAM remain unmet.
[Process summaries and receipt pointers](beta17-layout-results.json).

## Scope and CPU attribution

Only `/Users/almat/Desktop/code/Tezbar APP/Tezbar-performance`, branch
`fix/beta17-runtime-search-memory`, starting at
`1803f3d8a36be91ba727e3c6051b7520306ba025`, was used. No branch/worktree switch,
subagent, service reconfiguration, release action or real-profile mutation occurred.
XERJ again returned no match; exact `rg` was used. R7/R8/R9 and the unrelated
untracked HTML are untouched.

`layout-probe.py` repeats the actual lean seven-input burst 128 times in one
synthetic fresh Tauri process, with the existing native visibility, unlocked-display,
physical-input, fresh-final-reply, ordered DOM and two-frame guards. `/usr/bin/sample`
attaches only to PID/start-verified fixture host/WebContent processes for ten seconds
at 1 ms intervals. **Both attaches succeeded**, without permissions/security changes.
Native sample logs, full call trees and target identities are retained. No Xcode
installation or retry of the known-unavailable `xctrace` was needed.

The baseline main-thread tree has **7,495 samples**, including **1,623** in the
`jsHTMLElement_offsetTop → Element::offsetTop → Document::updateLayout` ancestry.
`Document::updateLayout` appears in 1,768 inclusive samples overall. The native path
continues through `performLayout`, nested `RenderFlexibleBox` layout, inline text
truncation and font shaping. These overlapping counts are **not additive CPU
percentages**; idle waits remain in the sample, and JIT JavaScript frames are
unsymbolized. This is a real native call tree, not a symbolized React profiler.

Bounded JS timers wrap the real DOM getters, including **lazy computed-style font
property reads**, not just `getComputedStyle()` creation. Across the entire baseline
128-burst diagnostic, 3,201 row `offsetTop` reads take **3,766 ms**; `offsetHeight`
takes 2 ms. Input computed-style/font-property reads plus canvas text measurement
sum to about **39 ms**. Diagnostic family/WebContent CPU is **15.209/8.884 CPU-s**.
Timers are quantized wall time and add overhead; sample attaches cover only ten
seconds of this longer workload. None of these instrumented totals is acceptance
CPU/latency/RAM evidence. No GC or retainer conclusion follows from them.

## One-variable experiments

1. **Equal GlideList geometry state:** retain the current state object for equal
   top/height/visibility and already-hidden states. Dependencies, geometry reads,
   scrolling and visuals stay unchanged. A real-component React Profiler regression
   goes red at **two commits instead of one** for rebuilt equal children and repeated
   empty selection; the guard makes it green. Changed/multiline geometry and
   keyboard/follow/wheel-suppression tests pass. However, short actual-app CPU
   differences overlap. **Rejected and reverted**, not combined into the next fix.
2. **Wrapper layout containment:** diagnostic injected `contain: layout` only on
   wrappers owning the highlight. It does not reduce synchronous `offsetTop` time:
   **3,766 → 3,946 ms**; WebContent CPU **8.884 → 9.057 CPU-s**. No production CSS
   change or equivalence claim was made; no broad matrix followed this negative probe.
3. **Input-width layout-only mutation:** remove width from CommandBar React state
   and apply the same flex-basis to its dedicated wrapper in the same layout effect.
   Continue measuring the actual input and current font on every commit; no stale
   dependencies/cache, scheduler, result/action, icon or visual treatment change.
   A real-CommandBar test records **three → two** measurements for input plus its
   optimistic-results update, without the third width-only parent commit. The exact
   two-call assertion is red again after rollback. The diagnostic reduces parent
   measurements **3,197 → 2,300** and row geometry reads **3,201 → 2,303**. Row
   `offsetTop` time is still **3,588 ms**, so most dirty-layout work remains.
   **Promising short CPU, unqualified full RAM; rejected and reverted.**

There was no icon/subtree memoization retry or canvas-cache experiment. No required
result was intentionally dropped; the 8 ms dispatcher, data limits and model/runtime
settings were unchanged. Backdrop blur/paint was not ablated: this round does not
establish its overall CPU share. Equal optimistic/native result-state comparison
was not attempted; stale-action risk was not hidden behind a visual-only comparison.

## Short fresh-process decisions

Each experiment uses **pre-fix / prototype / prototype / pre-fix**, independently
launched frozen `.app` processes. No original/history rebuild or unchanged original
matrix was repeated. Ordinary lean wrappers have no diagnostic timers/counters.
Family CPU includes every observed helper, including newly appearing QuickLook
processes; their lifetime charge is not replaced by a zero single-sample delta.

| Process order | Family CPU-s | WebContent CPU-s | Host CPU-s | Reply p50/p95 ms | Two frames p50/p95 ms |
|---|---:|---:|---:|---:|---:|
| Geometry pre-fix 1 | 1.433 | .849 | .259 | 15/15 | 42/47 |
| Geometry prototype 1 | 1.400 | .822 | .258 | 14/16 | 40/47 |
| Geometry prototype 2 | 1.470 | .851 | .263 | 14/15 | 42/46 |
| Geometry pre-fix 2 | 1.512 | .876 | .267 | 15/17 | 42/48 |
| Width pre-fix 1 | 1.623 | .873 | .260 | 15/16 | 44/48 |
| Width prototype 1 | 1.370 | .793 | .255 | 14/15 | 43/44 |
| Width prototype 2 | 1.381 | .775 | .256 | 14/15 | 42/46 |
| Width pre-fix 2 | 1.535 | .887 | .262 | 15/15 | 44/46 |

Geometry's median family/WebContent changes are **−2.5%/−3.0%**, with overlap.
Width's are **−12.9%/−10.9%**. All simple dispatch sequences remain
`[3,7,3,7,3,7,3,7]` (40 native requests per eight bursts). Short final WebContent
ledgers are geometry pre-fix **195.25/190.44**, prototype **96.92/96.03 MiB**;
width pre-fix **192.09/94.53**, prototype **92.44/90.80 MiB**. These plainly variable
pre-fix values are not a causal memory claim.

## Full width confirmation and discriminating reversed pair

After the short result, both ordinary and benchmark release apps were built.
The unchanged R8 lean full workload ran **P/C/C/P**, then an additional **C/P**
pair because the first cohort's RAM range conflicted with a stable attribution.
All six runs include opening cycles, simple/deep work, source and actual vector
indexing, retained-model and thirty-second quiet phases. No builds/tests overlapped.

| Full process, chronological | Simple family CPU-s | WebContent CPU-s | Host CPU-s | Post-repeat WebContent MiB | Post-repeat family median MiB |
|---|---:|---:|---:|---:|---:|
| Pre-fix 1 | 1.439 | .752 | .273 | 74.49 | 264.59 |
| Width 1 | 1.268 | .643 | .257 | 140.02 | 332.35 |
| Width 2 | 1.332 | .688 | .265 | 124.77 | 312.08 |
| Pre-fix 2 | 1.382 | .689 | .266 | 194.63 | 388.05 |
| Width 3 | 1.316 | .637 | .259 | 152.42 | 344.74 |
| Pre-fix 3 | 1.325 | .660 | .265 | 112.66 | 304.04 |

Across three fresh processes/build, median simple family CPU is
**1.382 → 1.316 CPU-s (−4.8%)**, less than the initial ABBA's ~7.8% gain.
Median WebContent CPU is **.689 → .643 CPU-s (−6.8%)**. Post-repeat WebContent
is **112.66 → 140.02 MiB (+24.3%)** and complete family **304.04 → 332.35 MiB
(+9.3%)**. The added reversed pair gives **112.66 → 152.42 MiB** WebContent.
The candidate fails a conservative CPU-without-RAM-regression gate. This does
**not** establish a reachable leak, a stable +24% causal retention effect or a
specific GC cause: unchanged pre-fix spans **74–195 MiB**, wider than the candidate
range. It establishes that the proposed fix has not demonstrated a safe joint win.
No favorable process/outlier was discarded, and no further acceptance matrix was
spent on this rejected candidate.

Other full-workload medians (pre-fix → rejected width prototype), computed from
per-process nearest-rank percentiles, not pooled samples:

| Phase | Reply p50/p95 ms | Two frames p50/p95 ms | Family CPU-s |
|---|---|---|---|
| Simple | 14/15 → 14/15 | 39/49 → 37/47 | 1.382 → 1.316 |
| Deep lexical | 12/14 → 6/13 | 40/47 → 31/34 | .443 → .376 |
| Warm deep | 25/39 → 21/24 | 54/60 → 46/56 | .770 → .592 |
| Repeated deep | 26/39 → 21/26 | 49/66 → 42/60 | 1.156 → 1.145 |
| Typing/source indexing | 14/28 → 14/26 | 39/55 → 42/52 | .621 → .553 |
| Typing/vector indexing | 25/27 → 25/27 | 50/58 → 52/57 | 2.480 → 2.449 |

Deep fast-burst native cadence changes mostly **2/4 → 1/4** with the unchanged
8 ms scheduler: less synchronous input work lets pending bursts coalesce before
their deadline. Those phase CPU gains are **not** a fixed-dispatch causal comparison.
Simple dispatches stay identical and isolate the narrower CPU result above. All
fresh-final native-reply and ordered DOM guards pass; cached DOM cannot complete them.

Durable completion is pre-fix **10.648/8.600/8.621 s**, width
**9.924/10.322/10.567 s**; medians **8.621 → 10.322 s**, not an indexing win.
CPU through durable completion is **2.388 → 2.449 CPU-s**. Six post-exit immutable
DB copies retain 96 sources, 192 chunks, 192 learned 256-F32 vectors, identical model
identity and digest `27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c`.

Quiet-family footprint medians, initial/post-simple/model-retained/post-repeat:
pre-fix **143.60/198.00/304.28/304.04 MiB**, width
**141.79/220.07/331.72/332.35 MiB**. Their CPU is respectively
**.122/.151/.270/.264 → .123/.158/.273/.243 CPU-s per 30 s**.
Opening proxies are pre-fix **985/1183/1201 ms**, width **1431/988/1215 ms**;
760×640 at 2×. These small, coarse warm-cache observations do not certify opening
tails or equivalence. No new exact UI/native action-parity, 90-second-retention or
ordinary packaged activation claim is made for a candidate rejected before that gate.
Prior accepted R7 checks remain historical evidence, not newly executed results.

## Rollback, checks and evidence

- All **573 R7 build-input pins** match restored production except the intentionally
  expanded `CommandBar.lifecycle.test.ts`. CommandBar, GlideList, CSS, native code,
  model and runtime policies are restored/unchanged. **No new accepted app exists.**
  Frozen original/current binaries and all four R7 `.app` payload inventories were
  rehashed. `final-ordinary.app` and `final-bench.app` in this round's artifact root
  are **rejected width experiments**, not accepted release candidates.
- **750 TS tests / 9 skipped, typecheck and restored frontend build pass.** All
  **30** lifecycle tests pass, including the new current-font/deep-mode/clear width
  behavior test. Prototype-only commit-count tests are archived, not left failing.
  **8 lean harness tests and 2 new lazy-style/DOM-probe tests pass.** Both rejected
  ordinary/bench release builds and geometry bench build pass ad-hoc verification.
- **450 Rust tests / 8 ignored, 40 exact native signatures and 84 exact model/boundary
  artifacts are reused**, with native sources, test executable/log, signature receipt,
  runtime ZIP/catalog and installed model inventories rehashed. No numerical/runtime
  rewrite, new model-boundary matrix or 8-GiB experiment occurred.
- **308 observed PID/start identities audited; zero survivors.** Normal clipboard
  observation stayed enabled; no contents were read, hashed or retained in evidence.
  Only marked synthetic OS-temp clipboard stores were scrubbed after exit. The
  unrelated HTML remains SHA-256
  `1ee1b0742261dbfb1ad8071817778b5881cb82c084bda482684117279f45816d`.

Evidence root: **`build/performance-layout-20261030/`**. It retains all 18 raw runs,
per-role CPU and complete observed family samples, vmmap, actual native stack files,
source patches/build inputs, four rejected `.app` payload receipts, command timelines,
six closed indexes and test logs. The tracked JSON points by hash to the final
manifest and detailed process summaries instead of duplicating raw evidence. The
first four full receipts have default `production_label=current`; actual app path,
executable hash and build receipts identify P/C unambiguously. Assembly notes preserve
a non-authoritative draft/self-log pin and a harmless exclusive-name gate-wrapper
collision; all timed attempts completed, with none excluded.

Accounting uses `proc_pid_rusage` timebase **125/3 ns**, responsibility plus recursive
PPID ownership, all observed helper roles (including QuickLook; no Metal compiler
was observed here), and half-second sampling. Footprints sum physical ledgers, **not
uniquely owned/deduplicated RAM**; summed RSS can double-count. Complete per-role/RSS
records remain in the archived details/raw data. Small cohorts, OS/WebKit allocation
high-water state and shader/cache variation limit causal memory and tail claims.

The remaining blocker is **joint qualification**, not inability to sample CPU:
GlideList forces dirty style/layout synchronously, while removing a sibling
width-only parent commit saves modest CPU without demonstrating safe retained RAM.
A next candidate needs pre-paint geometry/commit batching that preserves dynamic
row/font/resize/selection behavior and then survives balanced CPU/RAM confirmation.
The native profile narrows that work; it does not provide a JS retaining owner or
justify disabling visuals, delaying input, dropping results, forcing GC or rewriting
the framework. Accepted R7 simple/warm-deep response gains (6.75×/8.94×) remain,
but its two-frame/indexing/family-RAM gaps and R8/R9 limitations are not resolved.
