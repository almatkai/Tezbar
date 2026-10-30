# Beta.17 performance work

Target branch: `fix/beta17-runtime-search-memory`, based on
`9ff3663342b48ab01be2e56ab30f6d7123b0ced8`. The user approved this beta base and
the separate `Tezbar-performance` worktree. The original checkout belongs to
another task and must not be changed.

The target is five times faster runtime/search and one fifth of app RAM, with
simple search, deep search, background activity, and functionality preserved.
Opening the app is already satisfactory and is a regression constraint.

## Current evidence

The [UI allocation experiments (R9)](beta17-ui-allocation-results.md) test scoped
canvas, icon and row-presentation prototypes. No production candidate is accepted:
row memoization saves CPU but increases full-workload retained memory; icon-only
memoization removes 88.7% of icon render invocations without a credible CPU gain.
All production prototypes are reverted. Two behavior tests, diagnostic tooling and
[receipts](beta17-ui-allocation-results.json) remain. The fivefold/one-fifth goals
and the renderer CPU/RAM regression remain open. R8/R7 below are historical evidence.

The focused [renderer regression follow-up](beta17-renderer-results.md) adds a
fresh-process simple-only loop and a full lean-protocol AB/BA cohort, using the
unchanged frozen original/current apps. The [new JSON](beta17-renderer-results.json)
retains per-step/per-role accounting, diagnostics and receipts. The regression is
**not fixed**: simple-burst CPU is +27.6% and post-repeat WebContent footprint is
2.70× original even without retained full-reply measurement payloads. Extra prefix
work is established; its avoidable allocation/retainer owner is not. No speculative
production optimization or whole-family RAM win is claimed. R7 evidence below is
preserved under its original protocol, not rewritten by this follow-up.

The [final cumulative comparison](beta17-final-results.md) rebuilds original
beta.17 production source with measurement-only instrumentation and compares it
directly with the final candidate in AB/BA order. It records per-workload latency,
CPU, full observed process-family footprint/RSS, exact parity, rebuilt app/source
identities and acceptance. The [JSON](beta17-final-results.json) and
[chart](beta17-final-results.html) retain the distinct outcomes: some searches/UI
responses exceed 5×, but uniform 5× runtime and one-fifth RAM remain unachieved.
The earlier immediate-baseline rounds below are historical context.

`beta17-baseline.json` contains the unchanged initial measurements and complete
ordered result IDs/scores. The probes are test-only and excluded from the installed
application. Production search changes and repeated interleaved A/B results are
now recorded in [beta17-search-results.md](beta17-search-results.md), with full
measurement JSON in `beta17-search-candidate.json` and a reproducible
`compare-search.py` runner. Improvements vary by case; these do not establish a
fivefold whole-app speedup or a total RAM reduction. The subsequent runtime round
is recorded in [beta17-runtime-results.md](beta17-runtime-results.md), with full
desktop process accounting, real Q4 inference, repeated before/after workloads,
and a separate renderer image-retention fixture. The subsequent
[visible-search round](beta17-responsiveness-results.md) replaces fixed input
waits with bounded latest-input scheduling: the preserved two-frame proxy improves
118.5→48 ms, with more intermediate work and no demonstrated RAM reduction.
Its [new JSON](beta17-responsiveness-results.json) preserves final release A/B,
exact UI/native parity, opening checks and failed desktop attempts separately.

The subsequent [runtime-package round](beta17-runtime-package-results.md) turns
the unused-logits prototype into a reproducible pinned app resource with atomic
installation, offline old-model compatibility and ordinary-app validation. Final
complete-family retained footprint is 525→275 MiB for ordinary indexing and
1174→286 MiB for the CJK-rich pair; these are workload-specific, not a uniform
one-fifth result. The final thin-resource app preserves the opening/90-second
visible-response checks. Fresh-profile package materialization adds CPU cost;
no further general runtime/CPU speedup is claimed. Build/trust and parent-owned
distribution steps are in [the runtime contract](../EMBEDDING_RUNTIME.md).
The [independent package review](beta17-runtime-package-review.md) records
parent-run integrity/lifecycle checks, ordinary-app activation, and a follow-up
fix for unbundled development resource lookup.

The baseline/search probes are native subsystem measurements, **not** end-to-end
UI latency or total desktop app RAM. They exclude model inference and WebView
processes and cannot establish a fivefold improvement for the whole app. The
desktop results linked above measure that broader scope separately.

The launcher fixture has 100,000 file records and 400 application records and
uses the production database search function. It measures home, exact, prefix,
typo, keyboard-layout, broad file-term, and no-hit queries. This is a deliberately
simple synthetic corpus; real profile distributions may differ substantially.

The deep-search fixture contains 10,000 indexed sources/chunks, their FTS rows,
and deterministic 256-dimensional learned vectors. It measures the production
source search with cache clearing before every cold query, a populated cache for
warm queries, and a lexical query matching every source. Cold and warm semantic
results must match. The vector values are fixtures, not timed model inference.
The cache memory value is its conservative internal allocation estimate, not RSS.

Each timed case records a first call, three warmups, and 40 samples with nearest-
rank percentiles. Result equality is checked after timing every sample. Result
signatures are generated outside the timer. One run is exploratory: repeat and
interleave baseline/candidate processes before making improvement claims. The
first-call measurement does not flush OS caches or represent cold application
startup. High percentiles from 40 samples are particularly noise-sensitive.

The idle probe starts the actual native core with an empty isolated profile and
no app handle. It awaits Knowledge initialization, settles for two seconds, and
measures process CPU over 30 seconds and RSS before/after. Native core background
loops run, but WebView, model inference, application enumeration and filesystem
watchers are absent. `ready_ms` measures native core readiness, not app opening.

## Reproduce

Use release builds, from this worktree. Dependency/resource setup is:

```sh
pnpm install --frozen-lockfile
pnpm build:backend
```

Run the matrix, saving the output containing `TEZBAR_PERF` JSON records:

```sh
cargo test --release --manifest-path src-tauri/Cargo.toml --lib --locked \
  search_matrix -- --ignored --test-threads=1 --nocapture

cargo test --release --manifest-path src-tauri/Cargo.toml --lib --locked \
  native_core::performance::core_idle -- --ignored --exact --nocapture
```

`TEZBAR_BENCH_SAMPLES` can increase the timed sample count (10–10,000). The idle
probe currently uses Unix process accounting and `ps`; it is not a Windows RAM
measurement. No real profile, clipboard, installed app or model is modified.

Before accepting changes, compare the ordered result signatures as well as
timings, run functional tests for ranking, cancellation, source/model updates,
and check the app process family separately. Do not improve measurements by
disabling features or shrinking result/candidate limits.

## Baseline checks

- `pnpm typecheck`: passed in the initial delegated setup.
- `pnpm test`: independently passed, 734 tests; 9 skipped.
- Release Rust library suite: independently passed, 429 tests; 3 opt-in tests
  skipped, before adding these three benchmark probes.
- `pnpm build:frontend` and `pnpm tauri build --no-bundle --ci`: passed.
- Both search matrix tests and the headless idle probe: passed.

Original logs and baseline executables were initially stored at
`/tmp/tezbar-beta17-perf.e1qzXU`; that directory later became absent for an
unestablished reason. The runtime-package report identifies recovered artifacts,
regenerated evidence and excluded missing results. Generated extension-worker bundles were restored
after setup; they are not part of the source change.

## Follow-up experiments

Query preparation, candidate hydration, exact semantic scoring and source
selection experiments have now produced the measured changes described in the
search report. Broad-match BM25, cold authoritative row traversal and warm winner
hydration still limit search; further index/storage changes need their own real-
profile measurements and exact parity checks.

Idle maintenance and retained state were investigated in the desktop round
linked above. Its event/deadline observers, indexing allocation reductions and
renderer Blob-URL ownership fix have their own regression tests and measurements.
The empty-profile headless baseline is already low CPU and must not substitute
for those desktop results.
