# Beta.17 visible search responsiveness

Worktree `Tezbar-performance`, branch `fix/beta17-runtime-search-memory`.
Immediate A/B baseline: **6b11da3**, not the original beta.17. Native search and
runtime production code are unchanged this round. Historical reports/JSON/binaries
remain intact. Full new evidence: [beta17-responsiveness-results.json](beta17-responsiveness-results.json).
[Final handoff verification](beta17-responsiveness-final-verification.json) records
the last test-only assertion refinement, unchanged production hashes and process cleanup.

## Implemented and regression-tested

- Replace the unconditional 80 ms simple / 180 ms deep typing debounce with an
  **8 ms bounded coalescing window**. Its deadline is not extended by each input.
  Each normal/deep lane owns at most one running IPC and one replaceable pending
  input. A slow obsolete request cannot build an unbounded native queue; the
  final pending value is dispatched after it settles. Separate lanes let a mode
  switch bypass obsolete work, and backend generation changes get fresh lanes.
- Preserve response/request-ID guards, optimistic matches, the single-character
  local-only path, empty/deep prompt behavior, existing TTL/result/candidate
  limits, ranking/actions and extension argument handling. No native recall,
  result budget, index scope or model settings changed. There is no new IPC abort:
  obsolete **running** replies are ignored; pending jobs are replaced/cancelled.
  A long running request can still delay its lane's latest input.
- Route hide-triggered home refresh through that same guarded effect. The old
  native-hide + document-hidden path started **three** home requests in the
  component repro; it now starts one, with no separate 60 ms warmup closure.
- Suppress intermediate IME queries and submission keys during composition.
  Native hide clears interrupted composition state, including when the input
  receives no `compositionend`; otherwise search could remain suppressed on reopen.
  Deep Search's existing hide/query/result/selection preservation remains intact.

The actual component suite grew from 16 to **27 tests**. New tests first caught
both fixed dispatch delays, duplicate warmups and interrupted-IME suppression.
Coverage includes rapid pending bursts, a slow in-flight request, the final value,
non-extending deadlines, hide/reopen, invalidation, stale replies, backend restart,
mode changes, errors, one character and composition. Existing dynamic extension
rows/arguments and Deep Search selection tests pass. No subagents were started;
implementation and boundary review were performed locally by this executor.

## Release desktop protocol and latency

Apple M5 Pro, macOS 26.3, same existing isolated release socket harness and real
Q4 Gemma model. Complete responsible-PID + descendant family includes WebKit,
GPU, model, thumbnail and observed Metal compiler helpers. No builds/tests ran
concurrently with these desktop measurements. No real profile/model/clipboard,
installed app, global configuration or other task's checkout was changed.

Final completed order: `input-baseline-3`, `input-final-candidate-1`,
`input-final-candidate-2`, `input-baseline-7` (**AB/BA**, with failed baseline
attempts 4/5/6 retained between the last two). There are only two successful
processes per final version; this is not a reliability/tail guarantee.

Each process repeats the **unchanged** earlier 12 native hide/show/input cycles,
then eight simple bursts, four deep lexical bursts, eight warm-model deep bursts
and sixteen repeated warm-model bursts. Bursts alternate 2 ms and 20 ms input
spacing; simple bursts contain seven inputs, deep bursts four. Completion requires
a **fresh final-query native reply**, not a cached/optimistic row. Both versions
then wait two animation frames. Ordered DOM titles are checked afterwards:
`inputToCorrectDomObservedMs` is **not** a first-commit or pixel-paint timestamp.

Values below are medians of per-process p50s, not pooled percentiles.

| Input workload | Response + two frames, before → after | Ratio | Fresh response, before → after |
|---|---:|---:|---:|
| Preserved 12 open/close cycles | 118.50 → 48.00 ms | 2.47× | Not separately recorded by the old helper |
| Simple bursts | 125.25 → 42.25 ms | 2.96× | 87.50 → 14.25 ms |
| Deep, local/lexical fallback | 213.00 → 43.75 ms | 4.87× | 184.75 → 12.75 ms |
| Deep, warm real model | 234.00 → 52.00 ms | 4.50× | 206.00 → 23.25 ms |
| Repeated warm-model bursts | 235.00 → 49.75 ms | 4.72× | 208.00 → 23.25 ms |

Simple and warm-deep response-only ratios exceed 5×, but the comparable two-frame
metric does **not** uniformly reach 5×. Do not relabel response time as painted
latency. Median process p95s for that proxy are simple 134→45.5 ms, deep lexical
222.5→46 ms and warm-model 242.5→61.5 ms; sample counts remain small.

Simple IPC p50 is 4.5→4.0 ms; deep lexical 4.0→4.25 ms. Warm-model final-query
IPC p50 is 25.5→12.75 ms in this cadence. The native implementation is identical;
more prefix inference and shorter inter-query gaps change warm-up/scheduling
conditions. This is not evidence of a new model-kernel/native DB optimization.
The newly exposed native work does not justify a speculative search rewrite.

## More responsiveness means more work

| Equal-input phase, per process | Completed queries, before → after | Family CPU seconds, before → after |
|---|---:|---:|
| Eight simple bursts | 8 → 40 | 1.063 → 1.519 |
| Four deep lexical bursts | 4 → 12 | 0.498 → 0.443 |
| Eight warm-model bursts | 8 → 23 | 1.225 → 1.469 |
| Sixteen repeated model bursts | 16 → 43–44 | 2.429 → 2.372 |

Simple-burst CPU rises **43%**, initial warm-model burst CPU **20%**. Shorter wall
time also raises CPU utilization; it must not be sold as lower CPU. Full workload
bridge totals are 104 completed requests in each baseline versus 169/170 in the
candidates, including home/reset/background phases. All completed successfully,
none remained active at completion, and observed maximum concurrency was **one**
in every final process (the designed cross-mode bound is two per generation).
Candidates completed eight obsolete simple replies, two lexical, nine/eleven
warm-model and 22/23 repeated-model replies per process; guards prevented replay.
This trades extra bounded work for fresher visible results, not query-count savings.

Actual source/model indexing continues under three-input UI bursts at the same
1.5 s / 2 s periodic-demand schedule. All runs extracted 96 sources / 192 chunks
and durably saved 192 byte-identical learned vectors; SHA-256 remains
`27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c`.
Source completion wall time is 1.895–1.902→1.799–1.820 s, with coarse polling/grace
resolution; family CPU rises 0.493→0.693 s. Durable vector completion ranges are
8.875–10.567→10.252–10.505 s; family CPU through publication is 8.018→8.224 s.
There is no demonstrated indexing speedup or starvation in these finite workloads.
Foreground two-frame burst p50s during source indexing are 107.5/110.5→51/46.5 ms;
during semantic indexing 100.5/97→61/55.5 ms.

Six additional fresh-profile openings, AB/BA/AB, give first-UI medians
1.022→0.832 s (ranges 0.825–1.030→0.824–0.974 s), at 200 ms readiness resolution.
All viewports remain 760×640 at 2×. These warm-OS/test-WebKit-cache runs do not
establish a cold-install or fivefold opening gain; no opening regression appeared.

## Memory investigation: no demonstrated RAM fix

The same process-family ledger accounting is retained. Footprints are summed
physical-footprint ledgers, **not deduplicated private RAM**; RSS can double-count
shared pages and changes with residency/compression. Each idle below is 30 s.

| Phase | Footprint MiB, before → after | Settled summed RSS MiB, before → after | CPU % one core, before → after |
|---|---:|---:|---:|
| Initial hidden | 147.41 → 158.32 | 258.01 → 342.26 | 0.480 → 0.539 |
| After simple bursts, hidden | 190.54 → 199.78 | 166.60 → 484.80 | 0.724 → 0.705 |
| After bulk inference, model retained | 531.00 → 498.80 | 663.78 → 1133.70 | 1.160 → 1.459 |
| After repeated deep bursts, model retained | 554.72 → 499.71 | 750.66 → 1137.80 | 1.141 → 1.449 |

Repeated model work changes the candidate settled footprint by +2.43 / −0.63 MiB
within its two processes, not a consistent unbounded growth signal. This short
check is not proof of absence of leaks. Native host, WebContent, model and GPU
ledgers are preserved per sample. Initial candidate WebContent ends around
62–65 MiB; after repeat work it is around 77–88 MiB in these final processes.
Other runs show larger WebContent/residency variation without source changes.

Ownership inspection confirmed the already bounded 256-entry/4 MiB asset string
LRU, shared/released extension Blob leases, bounded candidate/query caches and
mutually exclusive lazy feature surfaces; these are **not new wins**. The query
cache's five-second validity is checked on access/insert, not forced heap expiry.
No heap/object-retainer evidence establishes it as the cause of family variation.
No speculative cache disabling, forced GC/reload, WebView destruction, early model
unloading or smaller limits were adopted. Only the duplicate home-work loss was
reproducibly removed; no whole-app RAM saving is attributed to it.

The candidate model alone retains roughly 322 MiB of ledger footprint in the
final runs, above one fifth of the 531 MiB active-family baseline (~106 MiB).
That remains an observed constraint of this runtime/model configuration, not a
universal floor theorem. **The one-fifth RAM and fivefold whole-runtime targets
remain unachieved.** Active CPU and summed RSS are adversely affected here.

## Exactness, checks and failures

- Final TS suite: **747 passed, 9 skipped**; typecheck and frontend build pass.
- All **441 release Rust tests pass, 7 ignored**, using the preserved accepted
  test executable. All 18 accepted source fingerprints remain unchanged; no Rust
  source changed. Forty original-beta.17 ordered matrix ID/score signatures match.
- Fresh final instrumented **desktop executable** and separate ordinary release
  executable both built. Benchmark markers are present only in the former.
- Sixteen supplemental UI final replies equal direct native ordered rows/scores/
  actions. Baseline/candidate also match exactly on separate APFS clones of one
  completed isolated index. Independent timed profiles have different path-derived
  IDs/tie ordering, so those profiles are not falsely claimed identical. Their
  final deep replies are stable within each process. Native WebView synthetic
  composition + actual hide/reopen recovery also passes on the final binary;
  this does not certify physical OS IME integration across platforms.
- Failed baseline 1 exposed a benchmark helper's hidden-prefix detection bug;
  fixed before headline runs. Baseline 4 failed correct-DOM confirmation, 5 timed
  out awaiting frames after show, 6 observed a truncated final query. Causes of
  4/5/6 are unestablished. All partial JSON/logs remain; baseline 7 passed unchanged.
  Later diagnostics count trusted physical-input events without retaining their
  text/keys; they do not retroactively explain earlier failures.
- Earlier exploratory baseline 2 incurred Metal compiler helpers: semantic phase
  **20.922 s / 26.049 CPU-s**, not suppressed or rewritten. Shader/OS caches were
  uncontrolled. The final warm cohort must not stand in for cold model first use.
- No new Windows, voice/provider or live compatibility-extension performance
  certification. Existing functionality regressions pass; previous full-runtime
  evidence retains its original scope. No push, PR, merge, deploy or app replacement.

## Reproduce

All commands run from this authorized worktree. Original artifacts remain under
`/tmp/tezbar-beta17-perf.e1qzXU`; always choose **new** output/binary names.

```sh
cd '/Users/almat/Desktop/code/Tezbar APP/Tezbar-performance'
pnpm typecheck && pnpm test && pnpm build:frontend
cargo build --release --locked --manifest-path src-tauri/Cargo.toml \
  --features desktop-bench,tauri/custom-protocol
# Freeze target/release/tezbar under a new name before the ordinary build.
python3 docs/performance/responsiveness-desktop.py \
  --binary /tmp/tezbar-beta17-perf.e1qzXU/desktop-responsiveness-final-app \
  --model "$HOME/Library/Application Support/com.tezbar.app/knowledge/models/embeddinggemma-2-q4_k_m" \
  --output /tmp/tezbar-beta17-perf.e1qzXU/NEW-input-run.json
# Repeat with desktop-final-app as immediate baseline, interleaved fresh processes.
python3 docs/performance/responsiveness-checks.py \
  --baseline /tmp/tezbar-beta17-perf.e1qzXU/desktop-final-app \
  --candidate /tmp/tezbar-beta17-perf.e1qzXU/desktop-responsiveness-final-app \
  --model "$HOME/Library/Application Support/com.tezbar.app/knowledge/models/embeddinggemma-2-q4_k_m" \
  --template-result /tmp/tezbar-beta17-perf.e1qzXU/input-baseline-7.json \
  --output /tmp/tezbar-beta17-perf.e1qzXU/NEW-input-checks.json
cargo build --release --locked --manifest-path src-tauri/Cargo.toml \
  --features tauri/custom-protocol
python3 docs/performance/summarize-responsiveness.py \
  --artifacts /tmp/tezbar-beta17-perf.e1qzXU \
  --output /tmp/tezbar-beta17-perf.e1qzXU/NEW-input-summary.json
```

The summarizer freezes this round's named cohort and verifies hashes, native
parity and ordinary-build marker exclusion. Raw timings, actions, process ledgers,
errors, logs and fingerprints are linked from the new JSON, not edited into older
reports. Future cohorts should use new filenames and an explicitly selected cohort.
