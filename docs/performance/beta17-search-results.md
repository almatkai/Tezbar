# Beta.17 native search implementation results

Worktree: `Tezbar-performance`; branch: `fix/beta17-runtime-search-memory`.
Production base: `9ff3663342b48ab01be2e56ab30f6d7123b0ced8` (beta.17).
Instrumented baseline: `d874f1f`. The original baseline JSON is unchanged.

## Implemented

- **Launcher matching:** prepare tokens, query/keyboard variants and alias-prefix
  terms once per search; share candidate tokenization across variants. ASCII
  takes an equivalent non-NFKC path; Unicode retains NFKC/lowercase behavior.
  Banded, reusable UTF-16 edit-distance scratch computes exactly the distances
  relevant to fuzzy thresholds and title bonuses. No recall thresholds changed.
- **Launcher candidate handling:** typed internal rows instead of transient JSON
  maps; searchable metadata-only fuzzy/application pages; hydrate only their
  unchanged 160/20 winners. Serialize/decode output actions only at the final cap.
  The existing home extension-recommendation action parsing remains where ranking
  needs it. Reuse the recent-usage statement and note-query regex.
- **Launcher broad FTS:** materialize the unchanged best 160 weighted BM25 rowids
  before joining document payloads. Preserve the old ascending-rowid BM25 tie
  order. The bundled SQLite 3.46 query-plan regression confirms the materialized
  page precedes document-primary-key lookups. BM25 still examines all matches.
- **Deep lexical:** replace the materialized per-source window and second sort
  with a single global `(BM25, chunk ID)` ordering and source deduplication.
  The first chunk of each source is exactly its previous window winner; continue
  until the unchanged *source* budget, not a capped raw chunk count. Hydrate only
  selected rowids inside the existing search snapshot. Cache the word regex.
- **Deep exact semantic:** score borrowed SQLite F32 blobs without allocating an
  F64 vector per row; reuse the query norm. Keep legacy JSON/F64 and short/long
  vector zip semantics. Packed-cache scoring interleaves four independent F64
  accumulators without changing each vector's addition order. Select the exact
  top source representatives before sorting just that prefix, in both lanes.

No ANN, quantization, smaller candidate limits, result caching, disabled features,
background-lifecycle changes or frontend changes were used. Search cancellation,
source snapshots, dirty journals, model/root eligibility and memory fences remain.

## Repeated release A/B measurements

Host: Apple M5 Pro, 24 GiB, macOS 26.3, Rust 1.98; bundled SQLite 3.46.
Same baseline fixture and production entrypoints, 80 timed samples/case/process,
three rounds in **AB / BA / AB** order (six processes, 4,800 timed queries).
There is no concurrent build/test workload from this task during these runs.
Other desktop activity is uncontrolled; the paired ranges expose process/run noise.
An earlier exploratory A/B series included a 3% broad-file regression in one pair;
the final series below is not evidence that small gains are immune to desktop noise.

The table uses the median of the three process p50s (and separately p95s), **not**
a pooled percentile. Speedup is baseline median p50 / candidate median p50.
The range is the three paired process p50 ratios, not a confidence interval.

| Case | Baseline p50 ms | Candidate p50 ms | Speedup | Paired range | p95 ms, before → after |
|---|---:|---:|---:|---:|---:|
| launcher.home | 1.7627 | 0.8670 | 2.03× | 1.79–2.04× | 1.8274 → 0.8940 |
| launcher.exact | 4.7355 | 0.9017 | 5.25× | 5.09–5.30× | 4.8253 → 0.9355 |
| launcher.prefix | 2.6443 | 0.7945 | 3.33× | 3.25–3.40× | 2.8417 → 0.8189 |
| launcher.typo | 6.0777 | 1.4456 | 4.20× | 4.15–4.36× | 6.2498 → 1.4787 |
| launcher.keyboard_layout | 3.1796 | 0.7924 | 4.01× | 3.89–4.05× | 3.3099 → 0.8228 |
| launcher.file_heavy | 29.0388 | 24.5707 | 1.18× | 1.15–1.18× | 29.3862 → 24.8616 |
| launcher.no_hit | 5.0849 | 0.8515 | 5.97× | 5.87–6.13× | 5.2133 → 0.8797 |
| deep.semantic.cold | 8.4343 | 6.9078 | 1.22× | 1.21–1.22× | 8.6635 → 7.2260 |
| deep.semantic.warm | 1.3166 | 0.6165 | 2.14× | 2.12–2.18× | 1.3538 → 0.6409 |
| deep.lexical | 19.3982 | 10.5552 | 1.84× | 1.83–1.89× | 21.2450 → 10.9856 |

Individual p50 readings (baseline → candidate, ms):

| Case | Round 1, AB | Round 2, BA | Round 3, AB |
|---|---:|---:|---:|
| launcher.home | 1.7645 → 0.9858 | 1.7587 → 0.8622 | 1.7627 → 0.8670 |
| launcher.exact | 4.7576 → 0.8982 | 4.6939 → 0.9017 | 4.7355 → 0.9302 |
| launcher.prefix | 2.6443 → 0.7903 | 2.7013 → 0.7945 | 2.6343 → 0.8108 |
| launcher.typo | 6.2989 → 1.4456 | 6.0313 → 1.4429 | 6.0777 → 1.4650 |
| launcher.keyboard_layout | 3.2045 → 0.7907 | 3.1796 → 0.7924 | 3.1463 → 0.8082 |
| launcher.file_heavy | 29.0388 → 24.5707 | 28.8550 → 24.4875 | 29.0789 → 25.3920 |
| launcher.no_hit | 5.2180 → 0.8515 | 5.0561 → 0.8615 | 5.0849 → 0.8433 |
| deep.semantic.cold | 8.3910 → 6.9078 | 8.4343 → 6.9028 | 8.4612 → 6.9323 |
| deep.semantic.warm | 1.3427 → 0.6165 | 1.3166 → 0.6197 | 1.3142 → 0.6138 |
| deep.lexical | 19.3982 → 10.6163 | 19.9793 → 10.5552 | 19.3702 → 10.5273 |

Full first/mean/p50/p95/p99 summaries, executable SHA-256s, signatures and per-run
statistics are in [`beta17-search-candidate.json`](beta17-search-candidate.json).
The JSON also records local raw-log paths. The helper emits percentile summaries,
not individual sample durations. First calls do not flush OS caches.

**The 5× target is not met across search.** Exact and no-hit exceed it in all three
final pairs. Broad-file improvement is small and noise-sensitive; do not present
it as a robust large gain. Deep search remains below target. No whole-app/runtime/RAM target is
established by these measurements.

### Experiments and limiting work

Single-variable exploratory runs informed the final implementation, not the
headline ratios: query preparation first reduced typo from 6.23 to 2.61 ms and
no-hit from 5.28 to 2.00 ms with exact matrix parity; later typed/ASCII/compact-page
changes removed additional work. Borrowed scoring/norm reuse and bounded selection
lowered cold semantic scans; interleaved accumulators subsequently reduced warm
semantic p50 from about 1.07 to 0.62 ms.

A Rust/SQLite per-source HashMap reducer measured lexical p50 11.71 ms; ordered
source streaming measured 11.09 ms on the same fixture, so the simpler bounded
streaming implementation was retained. An exploratory Python SQLite hidden-rank
plan was slower (roughly 37–43 ms versus 26–29 ms on 100k matching files), so it was
not adopted. That probe used the host Python SQLite, not bundled SQLite; it is
not included in the native A/B claims. Late hydration is verified and measured
on the bundled native implementation instead.

The remaining broad-file work is exact BM25 evaluation/doc-length lookup and
ranking of all 100k matches; late hydration cannot remove it. Deep lexical still
performs exact phrase and prefix/AND scans, eligibility joins and one ordering
per lane. Cold semantic still reads/joins and scores every authoritative eligible
vector. Warm semantic still scans all dense vectors and hydrates/fuses winners.
No larger gains for these stages were established in this round.

## Exact parity and regressions

All **60 per-case process signatures** (10 cases × 6 processes) equal the original
checked-in baseline's complete ordered IDs **and exact scores**. No results were
removed or reranked to gain speed. The existing timing helper also checks result
repeat equality after every timed query, and cold/warm semantic equality.

Five added regression tests cover:

1. Frozen beta.17 lexical/fuzzy/application scoring oracles over generated ASCII,
   mixed Unicode, NFKC, combining marks, non-BMP text, aliases and keyboard variants;
   reusable bounded distances are compared with full UTF-16 dynamic programming.
2. Weighted FTS candidate IDs/scores/ties across heterogeneous fields, updates and
   deletes; the late-hydration query plan is checked on bundled SQLite.
3. Deep lexical results/scores versus the old window SQL with repeated chunks,
   a source larger than the source budget, ties, multiple BM25 scores, pending and
   outside-root sources, phrase/prefix matches and no hits.
4. Full-sort semantic source winners versus cold and packed lanes with generated
   nontrivial dense vectors, legacy F64/short JSON, repeated sources, zero-vector
   ties and budgets 0/1/16/80/1000; checkpoint cancellation during a packed scan.
5. Borrowed-blob cosine versus decoded F64 accumulation, bit-for-bit on generated
   data, short/long/empty/malformed blobs and legacy JSON; zero/nonfinite coverage.

The worktree's XERJ prefix returned no indexed matches. Per the shared-service
circuit-breaker constraint, no reindex/restart/reconfiguration was attempted;
implementation discovery and exhaustive verification used current source and `rg`.

Existing ranking, recency/learning, application reservation, reader/writer mutation,
cache delta/rollback, model/root switches, pressure eviction and cancellation tests
also pass. The cache allocation estimate remains **13,212,960 bytes** for 10k vectors
in every baseline/candidate process. This is **not RSS** or total RAM; the round
reduces transient work, not the existing packed-cache allowance or app memory.

## Reproduce and checks

Every command below runs from the authorized worktree. No dependency setup or
backend bundle regeneration is needed with the staged resources already present.

```sh
cd '/Users/almat/Desktop/code/Tezbar APP/Tezbar-performance'

cargo test --release --manifest-path src-tauri/Cargo.toml --lib --locked \
  search_matrix -- --ignored --test-threads=1 --nocapture

# Build/locate this worktree's release test executable, without guessing its hash:
cargo test --release --manifest-path src-tauri/Cargo.toml --lib --locked --no-run
# The measured executable here was src-tauri/target/release/deps/app_lib-42960ca2c7041bb3.
python3 docs/performance/compare-search.py \
  --baseline-executable /tmp/tezbar-beta17-perf.e1qzXU/baseline-instrumented-tests \
  --candidate-executable src-tauri/target/release/deps/app_lib-42960ca2c7041bb3 \
  --rounds 3 --samples 80 \
  --output docs/performance/beta17-search-candidate.json

cargo test --release --manifest-path src-tauri/Cargo.toml --lib --locked \
  native_core::launcher -- --nocapture
cargo test --release --manifest-path src-tauri/Cargo.toml --lib --locked \
  knowledge::db::search_cache::tests -- --nocapture
cargo test --release --manifest-path src-tauri/Cargo.toml --lib --locked \
  -- --test-threads=1
cargo build --release --manifest-path src-tauri/Cargo.toml --lib --locked
pnpm typecheck
pnpm test
git diff --check
```

Completed checks:

- Launcher targeted: **48 passed**, 2 opt-in benchmarks ignored.
- Deep cache/source targeted: **12 passed**, 1 matrix ignored; borrowed-cosine
  regression also passed in the complete library suite.
- Complete release Rust library: **434 passed, 6 ignored**, 0 failed (114.47 s,
  serialized); the two search matrices were run separately throughout and in A/B.
- Non-test release library build: passed (dead-code warnings, no errors).
- `pnpm typecheck`: passed. `pnpm test`: **734 passed, 9 skipped**.
- `git diff --check`: passed. No generated bundles, lockfiles or baseline artifacts
  changed. No frontend changes, so no new frontend build was needed.

An earlier parallel targeted Knowledge run had two process-startup fixture timing
failures: `launch_exit_and_readiness_timeout_publish_errors_cleanup_and_retry` and
`stop_remove_shutdown_abort_and_reap_owned_startup`. Both passed in the complete
serialized suite; this round does not modify those runtime fixtures. Do not hide
that sensitivity or label the initial parallel run as passing.

## Scope and next opportunities

These are native DB/search subsystem fixtures, not WebView/IPC/UI latency, app
opening, model inference, live provider tests or real-profile distributions. The
10k semantic fixture is deterministic and simple; the generated dense-vector
regression validates arithmetic/ranking, not real model quality. Windows codegen
and runtime were not measured locally. The original release/installed apps were
not replaced, and no desktop/RAM or background-activity result is claimed here.

Next: profile an isolated full desktop process family with a real semantic model
and representative indexing/search workload (the parent's separate round).
Within search, measure real broad-query distributions and SQLite VM/doc-length
costs before considering index/storage changes; retain exact BM25, source winner
and mutation parity. Investigate cold eligible-row traversal and warm winner
hydration/fusion costs separately. Avoid inferring a 5× overall gain from these
subsystem ratios or changing recall budgets to manufacture one.
