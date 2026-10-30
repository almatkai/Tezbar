# Beta.17 UI allocation experiments (R9)

**No production candidate accepted. All renderer prototypes were reverted.**
The work moved from component totals to causal callsite experiments. Unchanged
icon rendering is frequent but does not explain most WebContent CPU. Memoizing
ordinary-row content saves CPU but increases retained memory in the full paired
workload. Uniform 5× runtime/search and one-fifth RAM remain unmet.
[Measurements, decisions and receipts](beta17-ui-allocation-results.json).

## Scope and causal counters

Only the authorized `Tezbar-performance` worktree and
`fix/beta17-runtime-search-memory` branch were used. Start HEAD:
`56e94837d20c968a84a5c7f195e42c58d01fb24c`. Original beta.17 is the unchanged R7
measurement-only rebuild of `9ff3663342b48ab01be2e56ab30f6d7123b0ced8`.
Pre-fix production remains `ac14c06`. No original source archive, frozen app,
R7/R8 report or unrelated untracked HTML was changed. One XERJ prefix search
again returned no match; exact `rg` was used without reconfiguring its service.

R8's lean short loop was retained: eight seven-input bursts with alternating
2/20 ms gaps, actual React input, fresh final native reply, ordered DOM, two frames,
native visibility, physical-input and locked-display guards; five initial and
thirty final hidden seconds. Temporary source counters and browser probes are
**diagnostic only**, excluded from acceptance timing and absent from final
production source. Render counts include render attempts, not only commits.

| Diagnostic, one variable added at each step | Icon render invocations | Family CPU-s | Last WebContent MiB |
|---|---:|---:|---:|
| Instrumented pre-fix | 13,199 | 1.591 | 99.59 |
| Cache identical canvas TextMetrics only | 13,197 | 1.428 | 96.19 |
| Memoize icon only | 1,490 | 1.505 | 94.64 |
| Icon memo plus ordinary-row content memo | 1,489 | 1.402 | 90.80 |

The canvas experiment is separate from the cumulative icon/content experiment.
All dispatch `[3,7,3,7,3,7,3,7]`, with the same fresh final DOM guards.
CommandBar is invoked 364 times and its row map 23,637 times. Optimistic search
makes 20 calls, visits 7,982 candidates and takes about 13 ms total; the candidate
pool peaks at **488**. Of 200 text measurements, 144 repeat text/font and 199 font
assignments repeat the value, but measured text work totals about 3 ms. The canvas
cache saves ~1 ms of measured text work; its unpaired total-CPU difference does
not establish a safe production win. Prepared metadata/layout changes were not
added on the strength of counts alone.

Icon memoization causally removes **88.7%** of icon render invocations. The common
content component runs 1,315 times. This proves redundant React/JSX/SVG work,
not proportional CPU savings or a live JS retaining owner. Neither prototype
shrinks limits/caches, delays input, suppresses correct intermediate results,
changes fonts/icons/layout, forces collection, recreates WebView or unloads models.
The row prototype keeps current action/selection/drag callbacks outside memoized
presentation and uses primitive visual props. It was still rejected on measurements.

## Fresh-process short decisions

Each cohort runs original/pre-fix/prototype/prototype/pre-fix/original, two fresh
processes/build, without diagnostic counters or overlapping task builds/tests.
Values are medians of process summaries; p50/p95 are nearest rank within each
process, never pooled percentiles or confidence intervals.

| Prototype | Family CPU-s, pre-fix → prototype | Last WebContent MiB | Decision |
|---|---:|---:|---|
| Icon + ordinary content memo | 1.429 → 1.340 (**−6.2%**) | 95.21 → 91.24 (−4.2%) | Promising short loop; full workload rejects it |
| Icon only | 1.441 → 1.472 (**+2.1%**) | 96.44 → 94.18 (−2.3%) | No credible CPU improvement; rejected |

For content memo, both prototype CPU values **1.383/1.298** are below both pre-fix
**1.422/1.436**. For icon-only, CPU is **1.453/1.430 → 1.458/1.486**.
Its WebContent CPU is effectively flat: **0.856/0.878 → 0.870/0.860 CPU-s**
(medians 0.867 → 0.865). All observed helpers, including `qlmanage` and `sips`,
remain counted. Renderer-only accounting therefore does not rescue the icon CPU
claim. A large reduction in render invocation count is insufficient here.

Short response/two-frame p50/p95, pre-fix → content prototype:
**14/16 → 15/15.5 ms**, **41.5/45.5 → 41.5/46.5 ms**.
Icon-only response is **15/16 → 15/15.5 ms**, frames **42.5/45.5 → 43/46.5 ms**.
Original response is about 94 ms; original dispatches 8 queries versus 40 in both
fast prototypes. No latency gain is obtained by dropping intermediate results.

## Full workload rejects content memo

After the promising short result, both actual ordinary and benchmark apps were
rebuilt. The full R7 ordinary workload ran original/prototype/prototype/original
with the same R8 lean protocol: opening cycles, source indexing, 96 documents,
192 durable vectors, lexical/warm/repeated deep search and thirty-second quiet
phases. A subsequent fresh pre-fix/prototype pair checks the RAM attribution.
No full icon-only matrix was run after its short-loop rejection.

The decisive additional pair gives:

| Full phase | Pre-fix | Content prototype |
|---|---:|---:|
| Simple burst family CPU-s | 1.389 | 1.223 (−11.9%) |
| Post-repeat WebContent footprint MiB | 79.24 | 101.31 (**+27.9%**) |
| Post-repeat family footprint median MiB | 269.18 | 291.88 (**+8.4%**) |

The CPU/RAM tradeoff fails this task's acceptance gate. The subtree cache is
reverted. This does **not** prove those extra bytes are reachable React objects;
allocation/collection high-water effects remain possible. It is a measured
retention cost, not dismissed as RSS noise.

Fresh pre-fix WebContent **79.24 MiB** also means the large decrease from R8's
~218 MiB cannot be attributed to this source change. R8 remains valid historical
negative evidence; its high retention is not reproduced by this later pre-fix
process. No source fix for that intermittent high-water state is claimed.

The rejected prototype's full ABBA preserves the accepted response gains:

| Workload | Fresh reply p50/p95 ms, original → prototype | Reply + two frames p50/p95 ms |
|---|---|---|
| Simple | 96/100 → 14/15.5 | 122.5/134.5 → 37.5/47 |
| Deep lexical | 194.5/207.5 → 12.5/13.5 | 221/234 → 38/45.5 |
| Warm deep | 220.5/229.5 → 22.5/35 | 249.5/256.5 → 52/58 |
| Repeated deep | 223.5/227 → 26/34.5 | 249/258 → 50/57 |
| Typing/source indexing | 93/99.5 → 14/26 | 113.5/121.5 → 39.5/50.5 |
| Typing/vector indexing | 97/99 → 24/26 | 107/117.5 → 47/51 |

Simple CPU is **1.037 → 1.238 CPU-s (+19.4%)**, source-indexing CPU
**0.451 → 0.563 (+24.9%)**. These compare original with a **rejected** prototype;
they are not final accepted-code improvements. Durable completion is original
**16.701/12.500 s**, prototype **10.141/10.351 s**; CPU through completion is
**9.549/8.955 → 2.423/2.463 CPU-s**. Existing native/runtime changes explain the
cumulative package behavior; no new native indexing improvement is claimed.

| Full ABBA quiet phase | Family footprint median MiB, original → prototype | Settled summed RSS MiB | Family CPU-s |
|---|---:|---:|---:|
| Initial hidden | 143.75 → 140.30 | 325.51 → 319.91 | 0.149 → 0.122 |
| Post-simple | 164.87 → 191.72 | 549.01 → 539.48 | 0.208 → 0.149 |
| Model retained | 487.87 → 284.15 | 1158.00 → 934.95 | 0.300 → 0.299 |
| Post-repeat | 484.15 → 284.76 | 1159.95 → 948.29 | 0.279 → 0.256 |

Post-repeat WebContent is **77.63/77.17 → 81.78/105.24 MiB**; retained model is
still roughly **323 → 103 MiB**, the existing runtime-package win. One-fifth of
the original retained family is **97.57 MiB**; even the rejected prototype's
**284.15 MiB** misses by **186.58 MiB**. Footprints sum physical ledgers rather
than deduplicated RAM; summed RSS can double-count shared/clean pages. Half-second
sampling can miss brief helpers/peaks. All observed roles remain included;
no Metal compiler helper was observed in these six full runs. Small cohorts and
uncontrolled OS/WebKit/shader caches limit precision and tail conclusions.

## Behavior, rollback and receipts

The rejected content candidate passes 16 exact UI/native ordered row/score/action
comparisons, synthetic IME interruption/native hide/reopen, and the same model
PID/start after 90 quiet seconds under the unchanged 120-second policy. First
fresh response is **236 → 33 ms**, two frames **265 → 66 ms**. Its actual ordinary
app activates the pinned app-owned runtime with unchanged old model manifests.
These are prototype checks, not acceptance of its RAM tradeoff.

Full opening proxies are original **999/1139 ms**, prototype **1190/1005 ms**;
separate openings **1016/855/861 → 1062/878/860 ms**, all 760×640 at 2×.
This coarse warm-cache check shows no material repeatable opening loss and does
not guarantee cold-start tails. Six closed synthetic DBs retain identical
96/192/192 counts, model/index identity, 256-F32 vectors and digest
`27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c`.

- **Final production source matches all R7 input pins** except the intentionally
  expanded test file. `CommandBar.tsx`, optimistic search, GlideList, native code,
  scheduler, runtime/model settings and limits are restored/unchanged. There is
  **no accepted third production application**; final runtime evidence is reused
  from the frozen pre-fix app. Rejected binaries and their source receipts remain
  clearly labelled in the ignored evidence root.
- **749 TS tests / 9 skipped and typecheck pass** on restored production. Two new
  real-CommandBar tests retain useful behavior coverage: changed text/icon/path,
  image-error recovery and fresh actions with unchanged visual props. Prototype
  render-count regressions were exercised red/green separately and are archived;
  no failing prototype-only assertion remains in the final suite.
- **8 harness tests pass. 450 Rust tests / 8 ignored and 40 native ordered
  signatures are reused** with unchanged native-source/test-executable hashes.
  **84 exact model/CJK/boundary artifacts**, ZIP/catalog and both installed model
  inventories were rehashed. No numerical Rust rewrite or 8-GiB experiment occurred.
- **565 observed PID/start identities audited; zero survivors.** Normal clipboard
  observation stayed enabled; no contents were collected, and only marked OS-temp
  fixture clipboard stores were scrubbed after exit. The unrelated HTML remains
  SHA-256 `1ee1b0742261dbfb1ad8071817778b5881cb82c084bda482684117279f45816d`.

Evidence root: **`build/performance-ui-allocation-20261030/`**. It retains diagnostic
source patches/builds, both rejected ordinary/benchmark apps, all short/full raw
samples and vmmap, separate parity/retention/ordinary checks, six closed synthetic
DBs, command timelines, test logs and source/binary/app receipts. The JSON also
records one non-authoritative summary assembly attempt; no timed run failed or
was excluded. `ui-allocation-probe.py` provides bounded diagnostic counters;
`summarize-ui-allocation.py` audits the evidence and rejects production drift.
`renderer-full.py` changes only production-label metadata, preserving its workload.
Use new output names for reproduction; exact diagnostic patches/build commands and
O/P/C/C/P/O / ABBA sequencing are retained in the evidence root.

The remaining technical gap is a way to reduce the measured WebContent/UI path
without the full-workload retained-memory cost of subtree memoization. Icon work
alone is too small to move CPU here; optimistic preparation/text measurement totals
also do not account for the ~0.87 CPU-s renderer cost. Parent row reconciliation,
font/style/layout work and the reachable-versus-uncollected WebKit allocation split
need narrower attribution. There is no demonstrated safe production fix in this
round, and no claim that the remaining cost is intrinsic or unfixable. Validation
is local macOS arm64/synthetic fixtures. No push, PR, merge, remote build/workflow,
signing credentials, installed-app replacement or publication occurred.
