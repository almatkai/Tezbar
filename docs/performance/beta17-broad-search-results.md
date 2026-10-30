# Beta.17 exact native broad-match ranking (R14)

**Implemented: eliminate BM25's nested document-size lookup.** Native broad reader
p50 improves **26.326→13.103 ms (2.01×)**, CPU **26.244→13.005 ms**. Deep lexical
improves less. **Uniform 5× runtime/search and one-fifth RAM remain unmet.** Desktop
results include material RSS/idle-CPU regressions; no whole-app RAM win is claimed.
[Compact results and complete cohort ranges](beta17-broad-search-results.json).

Work ran only in `Tezbar-performance`, branch `fix/beta17-runtime-search-memory`,
starting at `b01f7deedad3fd43da12e330d83b937a1bfb8649`. No subagents, branch/tree
switch, renderer/tokenizer/model change or historical rebuild. XERJ's supplied
shared-breaker/no-match history was respected; discovery used `rg`, without any
restart, reconfiguration or indexing. The unrelated HTML remains untouched.

## Actual bundled engine, causal profile and implementation

Cargo locks `rusqlite 0.32.1` / `libsqlite3-sys 0.30.1`; the app archive contains
**SQLite 3.46.0**. Its source ID, compile options, archive/header/source hashes,
actual prepared SQL, query plans, counters and native CPU stacks are retained.
FTS uses the ordinary `unicode61` configuration and external launcher content;
Knowledge keeps its existing indexed text and unindexed ID columns.

The production-read-only/snapshot CPU sample places **1379 samples** in the
`fts5ApiColumnSize` branch, dominated by `sqlite3Fts5StorageDocsize` stepping and
resetting its per-row statement, B-tree seeks and cursor release. The candidate's
actual native call stack removes that branch and instead calls the exact rank
auxiliary plus a normal outer-query primary-key lookup. Profiling runs are
excluded from acceptance. The earlier standalone sample lacked the production
snapshot and is labelled diagnostic, not used for the acceptance ratios.

Ranked hypotheses were: (1) remove nested docsize VM/cursor work; (2) connection
mutex overhead contributes to it; (3) hidden-rank access saves some VM work.
The first produces the material improvement. A native diagnostic using the exact
app SQLite static archive found weighted `ORDER BY rank LIMIT 160` slower
(~34.8 vs 22.5 ms for compact selection in the snapshot). Explicit `(rank,rowid)`
ordering was only slightly faster. Rank-only also lacks a documented secondary
rowid guarantee. Neither is adopted. A no-mutex connection reduces some baseline
cost but adds little after the joined-length fix; connection flags stay unchanged.
No host-Python SQLite search timing is used as evidence.

The retained change:

- `LEFT JOIN` the matching authoritative `%_docsize` row in the existing search
  snapshot; supply its small blob to `tezbar_bm25`, a registered FTS5 auxiliary.
  Compute the same phrase counts/IDF, weighted frequencies, average length and
  BM25. No payload tokenization, document-length cache or answer cache is added.
- Launcher keeps **5.0/2.0/1.0** weights, the same normalized BM25 contribution,
  materialized **160** candidates, late hydration, `(score,rowid)` ties and final
  ranking/actions. Every matching row is still ranked.
- Knowledge keeps phrase and prefix/AND lanes, indexed/enabled/selected-root
  eligibility **before** selection, `(BM25,chunk ID)` ordering and the unchanged
  distinct-**source** budget. Both source and chunk APIs are preserved. There is
  no raw chunk cap, smaller root set, ANN or unseen-winner pruning.
- Read-only shadow access introduces **no schema migration, index, rebuild or
  write cost**. Updates, rollback, reopen and root/model generations read the
  same transactionally maintained authority. There is no new state to invalidate.

The exact prepared launcher diagnostic has **1,109,226→1,711,627 top-statement VM
steps**, two sorts in both, and 159 fullscan steps for the final selected-page
iteration. The larger outer VM count is expected: those counters do **not** count
FTS5's nested docsize statements. This is removal of nested work, not a claim that
fewer outer opcodes or fewer FTS matches were evaluated. The query plan shows a
primary-key size lookup before the materialized cap and document hydration after it.

### Arithmetic and engine/build risk

The first pure-Rust prototype was **rejected**: the existing weighted-OR oracle
found a one-ULP score difference even though the simple 10-case matrix matched.
Disassembly identifies C contraction of the denominator/scalar accumulation and
separate vector-prefix accumulation. The oracle was not weakened.

A 20-line C arithmetic kernel now retains SQLite's expression/operation order,
built with the same `cc` defaults, target compiler and optimization profile as
bundled SQLite. `cc 1.2.64` was already locked/transitive; its sole lockfile change
is adding it to Tezbar's **build-only** dependencies. No package version, vendored
SQLite, CFLAGS/fast-math setting or runtime library is changed. The mechanism and
cross-platform risk were recorded here **before** this dependency change.

SQLite [documents the size vector](https://www.sqlite.org/fts5.html) but discourages
application shadow access. Accordingly, query selection is gated to **3.46.0**;
a future engine version falls back to the built-in query until requalified.
Unrecognized/missing/overflowing size blobs use the engine's `xColumnSize` and its
own error handling. The function never reads outside the blob. The callback's
query workspace is **16 + 16×phrase-count bytes**, freed by FTS5 on cursor reset/
close, versus the built-in ranker's same-order workspace. SQLite still bounds its
ordinary page cache/sort workspace; this change adds no corpus-sized retained map.

The C compiler, optimization profile and platform remain numerical dependencies:
strict bitwise oracle tests must pass on each shipping build. Local evidence is
**macOS arm64/AppleClang 21**, not Windows/Intel certification. The tiny C kernel
adds a normal target-C build step, not a new SQLite engine or external runtime.

## Native balanced release acceptance

P/C/C/P plus reversed C/P/P/C; four processes/build/case, **80 samples** and three
warmups per case. All runs are retained, including the noisier first cohort.
The 100,400-document launcher fixture and 10,000-source/vector deep fixture are
unchanged. A second frozen launcher fixture exercises the **actual read-only
connection and per-request transaction**; its broad query matches all 100,400
FTS rows. Baseline has current accepted production code plus test-only CPU/full
reply instrumentation, not the original unoptimized beta.17 code.

Numbers below are medians of process summaries; p95 is nearest-rank *within each
process*, never a pooled percentile/confidence interval. First is the first
per-case call, not an OS-cache-flushed/disk-cold result. Only `reader.broad` is
also the first query on that newly opened reader. Cold semantic clears the app's
vector cache, not OS pages. CPU uses `getrusage` seconds + microseconds/1e6;
calibration gives **.350015 CPU-s** versus `.350013` process-clock seconds and
`.350029` proc_pid_rusage seconds (125/3 ns ticks). No builds/tests/profile probes
overlap these acceptance runs. Signatures/serialization are outside timers.

All values are **before→after milliseconds**:

| Case | First wall / CPU | Steady wall p50 / p95 | Steady CPU p50 / p95 |
|---|---|---|---|
| deep.semantic.cold | 7.504→7.858 / 7.488→7.789 | 7.180→7.416 / 7.535→8.835 | 7.171→7.405 / 7.503→8.535 |
| deep.semantic.warm | .743→.763 / .745→.765 | .617→.638 / .684→.908 | .617→.638 / .666→.787 |
| deep.lexical | 12.623→11.826 / 12.593→11.501 | 10.986→9.492 / 11.380→11.060 | 10.956→9.451 / 11.352→10.980 |
| launcher.home | 1.288→1.256 / 1.283→1.248 | .864→.870 / .914→.949 | .865→.869 / .907→.943 |
| launcher.exact | 1.421→1.377 / 1.422→1.378 | .902→.862 / .987→.967 | .902→.861 / .973→.951 |
| launcher.prefix | .832→.821 / .832→.808 | .801→.748 / .863→.838 | .802→.749 / .857→.820 |
| launcher.typo | 1.514→1.535 / 1.514→1.536 | 1.467→1.468 / 1.568→1.594 | 1.465→1.465 / 1.563→1.569 |
| launcher.layout | .858→.798 / .859→.799 | .798→.745 / .850→.832 | .799→.746 / .842→.808 |
| launcher.file_heavy | 24.915→13.103 / 24.833→13.042 | 24.876→13.219 / 25.549→13.791 | 24.824→13.161 / 25.450→13.584 |
| launcher.no_hit | 1.003→1.103 / 1.004→1.105 | .870→.876 / .919→1.013 | .870→.877 / .911→.960 |
| reader.broad | 27.150→15.478 / 27.092→14.457 | 26.326→13.103 / 27.648→13.590 | 26.244→13.005 / 27.502→13.394 |
| reader.multi | 41.402→27.847 / 39.191→26.131 | 38.929→25.734 / 40.061→26.549 | 38.847→25.572 / 39.938→26.377 |
| reader.prefix | 26.342→13.405 / 26.249→13.370 | 26.311→13.361 / 27.060→14.067 | 26.234→13.209 / 26.960→13.753 |
| reader.no_hit | 1.903→2.007 / 1.904→1.936 | 1.603→1.649 / 1.746→1.879 | 1.600→1.648 / 1.731→1.800 |

Broad improvement is robust; **not every case improves**. Semantic p95 regresses
in the first cohort. In reversed order, cold CPU process p50s are candidate
7.450/7.360 versus baseline 7.417/7.396 ms; the first cohort's larger regression
is not reproduced. Both cohorts stay in JSON, not selectively replaced. No new
semantic speedup is claimed. No-hit gains are absent; its reader p50 rises ~3%.

Native process peak footprint is **68.25–69.44→69.28–70.63 MiB** for the complete
matrix; reader-process peaks are **12.23–14.06→13.05–18.36 MiB**. These are measured
ledgers, including allocator/page-cache/test-code effects, **not a RAM reduction**.
20 ms sampling and kernel lifetime maxima are retained; native diagnostics do not
stand in for whole-app memory. There is no cache-cold/warm/invalidated answer-cache
benefit hidden here: every search recomputes authoritative ranking.

## Exactness, lifecycle and cancellation

- **112 complete ordered replies** match, including IDs, exact scores, actions and
  payloads; **80 ordered ID/score comparisons** also match the preserved original
  beta.17 signatures. Every timed repetition checks equality after its timer.
- Strict BM25-bit tests cover 1–20 phrases (including vector-prefix/tail boundaries),
  differing term frequency/document length/column weights, positive/floored IDF,
  prefix/phrase/NEAR/NOT/column filters, Unicode, no hits, 17,000-token length,
  NULL/malformed size fallback, commit, delete, rollback and persistent reopen.
- Real launcher reader/snapshot fixtures compare complete old/new rows, ordering,
  scores and nested actions in normal/candidate APIs through action/query stats,
  update/delete, rollback and connection shutdown/reopen.
- Both Knowledge APIs compare complete old/new results through duplicate-heavy
  sources, source budgets 1/16/80, selected/disabled roots, source status changes,
  changed/deleted chunks, rollback and root re-enable. Existing frozen window-SQL
  starvation oracle still passes; no one-source chunk cap was reintroduced.
- Existing scoped SQLite progress/generation cancellation, subsequent-write,
  root/model switch and dirty-journal tests pass in the complete release suite.
  The new query still yields to the same 1000-VM-step handler, with *more* outer
  VM steps per match; the internal phrase-statistics traversal is unchanged.
  No cancellation fence is removed and no query/cache generation is retained.

### Mutation/stress timings and inherited final-sort tie limitation

An additional native supplement includes the actual launcher/rank modules and the
same bundled SQLite/C kernel. It adds **no production change**. A 100k-row isolated
clone runs interleaved old/new SQL through the same compiled ranking callsite,
with a fresh actual read-only connection/snapshot per block and fixed result
clock: **2400 timed queries**, 60 complete ordered block replies, and 15 exact
ordered FTS-page comparisons. Both multi-term queries are retained in JSON.

The first supplement caught a **pre-existing nondeterministic final tie**: equal
rounded scores *and* equal timestamps inherit `HashMap::new()`'s randomized order.
Repeating the **same unchanged baseline path** already changes those final IDs.
This is not a BM25 or candidate-page mismatch. Production's final comparator is
unchanged; adding an ID tiebreak here would change the requested baseline behavior.

For that adversarial case, the supplemental harness records exactly one test-only
substitution: use an identical cloned `RandomState` for the old/new map constructor.
It does not change sorting, scores, candidates or actions. **64 independent map
seeds × three queries = 192 additional complete ordered comparisons** pass,
including the colliding-score/timestamp rows; raw FTS pages also match bit-for-bit.
No comparison is weakened to a result multiset. The unseeded failure, source,
executable and stderr are archived. The 112 ordinary native comparisons above
remain unmodified/unseeded; supplemental timings are labelled separately.
**Literal independent-replay repeatability for those underdetermined final ties
is not established, because the baseline itself lacks it.** Conditional old/new
ordering under the same baseline randomness is established, not a new tie policy.

Broad `document` timings below are median block summaries, before→after ms:

| State before request | First wall / CPU | Steady wall p50 / p95 | Steady CPU p50 / p95 |
|---|---|---|---|
| Fresh reader | 30.158→14.160 / 28.771→13.877 | 27.707→13.366 / 29.746→14.989 | 27.124→13.111 / 28.436→13.929 |
| Updated/deleted documents | 33.806→13.942 / 29.595→13.746 | 27.058→13.225 / 28.357→14.322 | 26.900→13.095 / 27.695→13.719 |
| Action/query statistics changed | 27.080→13.396 / 26.945→13.365 | 26.509→13.159 / 27.226→13.599 | 26.364→13.073 / 26.800→13.450 |
| Transaction rolled back | 26.991→13.913 / 26.823→13.495 | 26.861→12.948 / 27.610→13.593 | 26.713→12.881 / 27.220→13.228 |
| Forty varied long/Unicode documents added | 26.955→13.400 / 26.845→13.391 | 26.748→13.296 / 28.291→13.844 | 26.628→13.262 / 27.592→13.580 |

The shared-process trace peaks at **17.91 MiB footprint / 23.84 MiB RSS**; this is
not a per-variant RAM comparison. Existing trigger/write costs are retained:
update/delete 60.59 ms wall / 16.46 ms CPU, stats .40/.15 ms, rollback 21.30/6.28 ms,
40 long-document inserts 9.37/4.25 ms. These writes are outside search timers and
common to both read-only variants, not an app indexing-throughput claim. No new
rebuild/migration/generation-cache cost is introduced.

## Actual desktop paired lean workload

Four **complete** processes in P/C/C/P order, with identical added broad-final
simple/lexical/model bursts in both builds. Same 5,000 files, **96 documents /
192 durable 256-F32 vectors** and exact stored-vector digest as R7. No repeated
8-GiB model matrix. Normal clipboard observation remains enabled; no clipboard
contents were read/hashed/reported/written. Latest-reply data is released before
RAM windows. All observed responsible/recursive descendant roles, including
QuickLook and compiler helpers when observed, contribute to family accounting.

Four invalidated attempts are retained separately: three physical-input guard
failures and one fresh-DOM/input-reset failure. Their incomplete rows never enter
these complete-cohort summaries. Visibility, unlocked-display, trusted-input,
fresh-reply and ordered-DOM guards were not bypassed. Two complete processes/build
are descriptive, particularly for tails; they are not reliability guarantees.

| Workload | Response p50 / p95 ms, before→after | Two-frame p50 / p95 ms | Family CPU-s |
|---|---|---|---|
| Simple | 15.0→13.5 / 15.5→14.5 | 38.5→37.5 / 46.0→45.0 | 1.373→1.338 |
| Simple broad | 10.0→9.0 / 13.0→12.0 | 27.0→27.0 / 40.0→37.0 | .251→.277 |
| Deep lexical | 13.0→12.0 / 14.5→13.5 | 38.5→38.5 / 45.0→46.0 | .460→.464 |
| Deep lexical broad | 9.5→7.0 / 16.5→13.5 | 30.0→26.5 / 40.5→36.5 | .395→.286 |
| Warm deep | 29.0→25.5 / 45.0→38.0 | 58.5→49.5 / 79.5→69.5 | .909→.794 |
| Warm deep broad | 21.5→22.0 / 25.0→24.5 | 46.0→44.0 / 54.0→55.5 | .298→.318 |
| Repeated deep | 26.5→24.5 / 39.0→33.5 | 48.5→52.0 / 71.0→56.5 | 1.278→1.267 |

Native improvement is **not** a 2× UI or 5× app claim. Simple/warm-broad CPU rises,
repeated-deep frame p50 worsens, and the small Knowledge desktop index exposes
less ranking work than the native 10k-source fixture. Durable 192-vector completion
is **10.395→10.472 s**, CPU through completion **2.518→2.515 CPU-s**; source-index
burst CPU is **.596→.605 CPU-s**. No indexing/background speedup is attributed here.

| Matched quiet phase | Footprint median / p95 MiB, before→after | Settled summed RSS MiB | 30 s family CPU-s |
|---|---|---|---|
| Hidden/no model | 143.38→139.84 / 151.54→141.92 | 329.66→305.16 | .157→.149 |
| After simple+broad | 180.23→173.80 / 190.84→188.90 | 448.63→610.53 | .155→.363 |
| Model retained | 311.13→286.64 / 412.84→395.65 | 760.96→945.82 | .302→.332 |
| After repeat | 332.30→279.41 / 562.33→536.16 | 779.84→951.62 | .285→.356 |

**Material costs are not hidden:** post-search RSS rises **36.1%**, retained RSS
**24.3%**, and post-search quiet CPU **2.35×**. Post-search WebContent median RSS is
208.73→363.21 MiB while its footprint is 97.73→88.54 MiB; host footprint is
44.44→48.06 MiB. The model's retained footprint is 101.92→102.33 MiB. Per-process,
per-role values and complete ranges remain in JSON. These observations do not
establish a new retained-allocation cause; renderer/model production bytes are
unchanged. They prevent a whole-family memory/idle-CPU success claim, even though
the measured footprint sums are lower. Footprint sums are **ledger sums**, not
deduplicated RAM; RSS double-counts shared/clean pages, and .5 s sampling can miss
short-lived helpers/peaks. Do not turn workspace estimates into physical RAM wins.

Supplemental actual-package opening p50 is **860.64→859.15 ms** (three/build,
200 ms polling, uncontrolled OS cache). All 48 complete-cohort hide/show/search
cycles succeed. **20 full final-query replies** equal direct native rows/actions
on a cloned authoritative index and across builds, including broad cases.
Synthetic IME interruption + native hide/reopen recovery passes. Both models
retain the same PID/start after **90 quiet seconds**, with fresh response
34→34 ms and two-frame proxy 64→65 ms; eight further full replies match. This is
one retention pair, not a new tail guarantee or an early-unload change.

## Checks, identities, evidence and boundary

- **455 release Rust tests passed, 9 ignored**; **752 TS passed, 9 skipped**;
  typecheck, frontend build, actual ordinary/benchmark `.app` release builds,
  **5 package-pin** and **16 release-policy** checks pass. Actual packaged Bun
  host/worker roundtrip/termination passes (its clipboard host is simulated).
- Ordinary non-harness app activation loads the unchanged pinned app-owned runtime
  from a fixture with no app-runtime cache; old model manifests remain unchanged.
  The first attempt correctly rejected an already-materialized template before
  launch. The successful retry uses a separate APFS clone with only that fixture's
  app-runtime cache removed. No installed model/profile is edited.
- **576 source pins**, app resource inventories and runtime ZIP/catalog are pinned.
  ZIP SHA-256 remains `75698aa84eb35398cee7b3afb6fee7b9bdb1db7a3aedc3cc43af25780100e475`.
  Installed Q4/Q8 inventories and the unrelated HTML match their initial hashes.
- **268 observed PID/start identities audited; zero survivors.** All build/test/
  profiler/worker/calibration subprocesses are reaped. Only fixture clipboard
  stores are scrubbed after shutdown, without inspection. No unowned kill occurs.

| Executable | SHA-256 |
|---|---|
| Current baseline native tests | `4b778450a654f522c6235dccf159afdcf5d51690179fd0c91e2d5b340054cc47` |
| Candidate native tests | `0d5497c01fd179f61edb7cf65d5c06aa472b5f3552adc74617cc9cf8898258ab` |
| Candidate ordinary | `94b683758a1bd45a742777d45a4c10a629abfb7c721ccc0f8c4379c931cd7cf4` |
| Candidate benchmark | `e94e322be141ae0f48e733050677c3edffd1dec2b709325c7499947267f50adb` |

The frozen current baseline app remains R7 accepted benchmark
`1cbb16091ef42dc0fe72d009d0d84046663ead874e27b3cbb30ec6fc5923ffac`.
These R14 values compare **current accepted→new candidate**, not a fresh original
beta.17 desktop rebuild. R7's historical 5×/one-fifth gaps are not rewritten or
multiplied with these ratios. The broad native path still ranks every match;
multi-term ranking, cold vector traversal, UI frames and total RAM remain below
the requested uniform target. R14's retained footprint is nowhere near one fifth
of its matched baseline, and its RSS/idle costs remain open.

New raw material is only under **`build/performance-broad-search-20261030/`**:
`artifact-manifest.json`, `source-pins.json`, `harness-pins.json`, frozen executables/apps/fixtures,
actual-stack/disassembly/counter probes, both native cohorts, all desktop attempts,
parity/opening/retention/ordinary receipts, rejected Rust arithmetic patch/source, the mutation/stress trace and seed-control
patch/unseeded baseline failure, package/build/test logs and cleanup identities. R5–R13 are not overwritten.
The initial native parser failure (test-runner prefix on the first JSON line)
and zero-test `--exact` invocation are labelled in raw logs; corrected runs pass.

Reproduce with new output paths using `broad-native.py`, `broad-full.py`,
`responsiveness-checks.py --apps --broad`, `renderer-retention.py` and
`summarize-broad.py`. Exact commands, toolchain options and complete records are
in the artifact root. Local commits only: **no push, PR, remote build/workflow,
merge, signing-credential access, installed-app replacement, publication or deploy**.
