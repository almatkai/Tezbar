# Beta.17 performance work

Target branch: `fix/beta17-runtime-search-memory`, based on
`9ff3663342b48ab01be2e56ab30f6d7123b0ced8`. The user approved this beta base and
the separate `Tezbar-performance` worktree. The original checkout belongs to
another task and must not be changed.

The target is five times faster runtime/search and one fifth of app RAM, with
simple search, deep search, background activity, and functionality preserved.
Opening the app is already satisfactory and is a regression constraint.

## Current evidence

`beta17-baseline.json` contains the initial measurements and complete ordered
result IDs/scores. No production optimization has been applied yet. The probes
are test-only and excluded from the installed application.

These are native subsystem measurements, **not** end-to-end UI latency or total
desktop app RAM. They exclude model inference and WebView processes. They cannot
establish a fivefold improvement for the whole app. A desktop run with an active
semantic model and indexing workload remains to be measured.

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

Original logs and baseline executables are retained locally at
`/tmp/tezbar-beta17-perf.e1qzXU`. Generated extension-worker bundles were restored
after setup; they are not part of the source change.

## Next experiments

1. Repeated per-candidate tokenization, Unicode conversion, and edit-distance
   work: preparing query state once and reusing bounded scratch storage should
   reduce typo/no-hit search time without changing scores.
2. FTS/source selection work: reducing repeated scans and sorting should improve
   broad file and deep lexical queries, while still choosing the same best chunk
   per source and preserving all ranking/tie behavior.
3. Idle maintenance and retained state: profile the complete desktop process
   family and representative indexing workload before changing lifecycle or
   memory policy. The empty-profile headless baseline is already low CPU and
   does not explain the user's desktop RAM use.

These are hypotheses, not established causes or improvement claims.
