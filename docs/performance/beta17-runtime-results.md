# Beta.17 runtime, background work and memory

Measured on 2026-10-29 in `Tezbar-performance`, branch
`fix/beta17-runtime-search-memory`. Runtime comparisons start at `354689e`, which
already includes the accepted search changes. The original beta.17 baseline and
search results are unchanged. This round does **not** establish fivefold
whole-app speed or RAM improvements.

## Production changes

- Knowledge's prewarm observer now waits for actual settings/runtime events.
  Inactive background work sleeps until its existing startup, watcher debounce,
  reconciliation or vector deadline. Watcher events wake both observers.
  Selected Gemma indexing retains its original 250 ms cadence. Shutdown,
  cancellation, the 1.2 s foreground grace period, extraction concurrency and
  the five-minute reconciliation policy remain active.
- Model idle supervision waits for process changes and the existing 120 s idle
  deadline instead of ticking every second without a process. Inference owns
  the process mutex, so the supervisor cannot unload an active request.
  Successful warm requests no longer publish a false `stopped` transition before
  returning to `ready`; aborted/error requests still receive status cleanup.
- The database worker blocks for jobs when it has no requested search cache.
  Requested-cache pressure/idle eviction and mutation maintenance remain active.
- Local deterministic text embeddings reuse UTF-16 storage and incremental FNV
  seeds, eliminating per-ngram allocations. Page normalization reuses compiled
  regular expressions. Vector bits, normalized content, offsets and hashes are
  preserved by regressions against the old implementation.
- Extension images share a mounted-consumer lease. The last release revokes the
  owned Blob URL, including a conversion that finishes after unmount. Concurrent
  consumers share work; caller-owned/local URLs and the durable offline
  CacheStorage cache retain their behavior.
- Authoritative queue drain publishes a previously incomplete semantic count
  without waiting for the count budget, for the same scope/generation. Already-complete mini-batches keep
  the ten-second COUNT budget. This fixes a measured completion-status delay
  without adding a COUNT for every indexed chunk.

## Desktop protocol and accounting

The final protocol is `visibility-and-durable-completion-v2`, implemented in
[final-desktop.py](final-desktop.py), [desktop.py](desktop.py) and
[window_visibility.py](window_visibility.py). Four fresh release processes run in
AB/BA order: baseline, candidate, candidate, baseline. Each has isolated
`TEZBAR_DATA_DIR`, `TEZBAR_CONFIG_DIR`, synthetic home and a temporary `.app`
bundle. The baseline executable was frozen before production changes with the
same opt-in harness. The candidate executable was relinked after the final fix;
a separate ordinary release executable was also built.

Both versions enumerate 5,000 synthetic files and the same OS Applications
directories, extract 96 text documents into 192 chunks, and save 192 real Q4
Gemma embeddings. The installed model/runtime is copied with APFS copy-on-write;
the original profile/model and installed app are never modified. No model is
downloaded. `HOME` is not overridden: the compile-time harness overrides only
the core's home path, because data/config isolation alone would still scan the
actual home folders. Clipboard observation runs normally, but the benchmark
never writes the clipboard or reports its contents.

The harness is enabled only by `desktop-bench`; ordinary builds contain no
socket, JavaScript evaluator, test home override or observer counters. It uses a
separate `com.tezbar.performance` identity and a mode-0600 Unix socket beneath
the OS temporary directory. It does not compete with the installed app's
single-instance lock. Only benchmark-owned apps are started and gracefully
closed. Its identity causes updater channel checks to reject: updater network
and installation behavior are outside these measurements.

Ownership is the exact macOS responsible PID plus recursive PPID descendants,
not all WebKit processes on the machine. This includes launchd-parented WebKit
WebContent, GPU and networking XPC services, SafariPlatformSupport and audio
helpers, the native supervisor, model server and observed thumbnail children.
Unrelated apps are excluded. Samples preserve PID/start identity, names,
responsibility and per-process values, without command-line arguments/tokens.

CPU uses `proc_pid_rusage` v4 user+system Mach ticks converted with
`mach_timebase_info` (125/3 ns here), calibrated against `getrusage`. A 0.25 CPU-s
burn read approximately 0.250023 s. CPU percentages mean percent of **one core**.
Interrupt and package-idle wakeup counters are recorded. Memory is the sum of
physical footprint ledgers for the owned family; it is **not deduplicated unique
private RAM**. Summed RSS is reported separately and can double-count shared
clean/model pages. Neither cache allocation estimates nor host-only RSS stand
in for app RAM. Sampling every 0.5 s can miss brief peaks, short-lived children
and final CPU between the last sample and exit.

After startup reconciliation and equal settling delays, hidden/visible idle and
post-feature retention each last 30 s. CoreGraphics verifies the benchmark
window's actual on-screen bounds for every idle sample without screenshots or
reading other windows' titles. A temporary native blur guard holds the test
window visible; it changes no stored setting or backend service and is restored
on exit. Keyboard focus during idle is not locked or sampled, so on-screen
idle can include focused and unfocused rendering; cursor/compositor behavior is
an additional source of CPU variation. Twelve hide/show/input cycles await the real hidden/reset milestone;
input timing includes the production debounce and two animation frames after
the actual search response. This is a response/render opportunity proxy, not a
pixel-level paint measurement.

Simple searches continue during actual source indexing and semantic indexing.
Source probes are 1.5 s apart to preserve the normal foreground grace period;
semantic probes are 2 s apart. A read-only connection observes saved vector
count every 20 ms, separately from cached UI counts. That diagnostic polling and
the equal snapshot requests contribute to indexing-phase CPU. These are equal
periodic-demand schedules, not equal total query counts: a slower completion
receives more foreground probes and their ordinary grace periods. The result is
end-to-end queue throughput under that demand, not a standalone model-kernel
speedup. Source completion is observed at a coarse 1.5 s interval. Active-model RAM
is sampled **after bulk inference, before any model restart/unload**; image
retention is measured with that model still ready. Idle begins after reported
completion, so an old cached-count delay also adds time since actual inference
completion; this can affect transient retention comparisons. Only then does an ordinary
pause/resume exercise cold model startup and warm deep search.

## Results

Values are medians of the two per-process summaries, **not pooled percentiles**.
Both versions finished with 5,000 files, 238 apps, 96 sources and 192 chunks.
Every idle interval below is 30 seconds.

| Phase | Family CPU seconds, before → after | % of one core, before → after | Summed footprint MiB, before → after | Settled summed RSS MiB, before → after |
|---|---:|---:|---:|---:|
| Hidden idle | 0.154 → 0.147 | 0.512 → 0.489 | 126.69 → 147.74 | 303.37 → 333.62 |
| Visible idle | 0.863 → 0.770 | 2.873 → 2.564 | 206.67 → 160.45 | 278.05 → 372.70 |
| After feature visits, hidden | 0.183 → 0.213 | 0.608 → 0.711 | 164.59 → 189.96 | 418.68 → 522.19 |
| After bulk inference, hidden | 0.298 → 0.336 | 0.993 → 1.119 | 519.46 → 527.58 | 821.17 → 1054.52 |

Sampled footprint peaks across the two processes (each range is min–max, MiB):

| Phase | Baseline peaks | Candidate peaks |
|---|---:|---:|
| Hidden idle | 113.71–183.99 | 155.04–155.88 |
| Visible idle | 261.83–310.94 | 241.66–384.96 |
| After feature visits, hidden | 301.07–314.93 | 336.29–372.71 |
| After bulk inference, hidden | 473.91–571.41 | 510.40–553.91 |

The hidden-idle CPU difference is small (about 0.023 percentage points). Visible
CPU/footprint ranges overlap and focus is uncontrolled. Post-feature and active-
model CPU increased in these runs. Hidden/post-feature footprints and summed RSS
also increased. **There is no consistent app-wide CPU or RAM reduction.** The
visible-footprint decrease does not establish a RAM gain when other phases and
RSS move adversely.

After bulk inference the model alone accounts for 322.77–381.47 MiB of footprint
before and 322.78–322.81 MiB after; its median RSS is 498.70 → 590.08 MiB.
The candidate whole-family footprint is 527.58 MiB versus 519.46 MiB before
(1.6% higher), with 1,054.52 versus 821.17 MiB summed settled RSS (28.4% higher).
The one-fifth footprint target would be 103.89 MiB for that baseline. The observed
model alone already exceeds it. This is a floor observed with the current model/
runtime configuration, not proof that future model/runtime changes cannot lower it.

| Workload | Before | After | Interpretation |
|---|---:|---:|---|
| Source indexing, 96 documents | 1.563 s | 1.568 s | 1.5 s observation/grace granularity; no measured throughput gain |
| Source indexing, family CPU | 0.332 s | 0.345 s | Actual service work plus equal foreground probes |
| 192 real vectors durably saved | 18.176 s | 10.233 s | 1.78× faster under the periodic foreground-demand schedule |
| Completion published | 22.956 s | 11.484 s | 2.00× faster; cached-count delay is separate from inference |
| Family CPU through published completion | 8.328 s | 7.718 s | 7.3% lower CPU seconds; CPU utilization rises because wall time shrinks |
| Simple bridge query, p50 | 4.00 ms | 3.75 ms | IPC included; 10 samples/process/query |
| Input → response + two frames, p50 | 118.00 ms | 117.75 ms | Includes existing 80 ms debounce; 12 cycles/process |
| Simple search during source indexing, p50 | 4.25 ms | 4.75 ms | No fivefold foreground gain |
| Simple search during model indexing, p50 | 14.50 ms | 21.75 ms | Higher median; small/noisy sample counts |
| Simple search during model indexing, p95 | 85.50 ms | 75.50 ms | Periodic probes; tails not a guarantee |
| Deep lexical bridge, p50 | 4.00 ms | 4.00 ms | Accepted search implementation exists in both versions |
| First deep query while model cold | 5.50 ms | 5.00 ms | Existing lexical fallback; **not model inference** |
| Remaining wait for ready after first query | 0.849 s | 0.844 s | 200 ms polling, no changed prewarm policy |
| Warm real-model deep search, p50 | 12.00 ms | 12.50 ms | 10 queries/process, no gain established |
| Warm real-model deep search, p95 | 28.00 ms | 30.50 ms | No fivefold tail-latency gain |

Actual vector completion ranges are 17.929–18.423 s before and 8.270–12.196 s
after. Publication follows actual completion by 0.000–9.560 s before and
1.225–1.276 s after. All four 192-vector payloads are byte-identical (196,608
bytes), with filename/start-offset/vector SHA-256
`27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c`.
The earlier narrower durable-completion AB/BA experiment also saved the same
vectors in 13.809–13.837 s before and 8.164–10.297 s after, and exposed a
5.5 s candidate publication lag before the terminal-count fix.

Simple bridge p95 is 10.5 → 13.0 ms. Input-to-two-frame p95 is 1,859 → 120.5 ms,
but the former is dominated by one 3,596 ms baseline sample (the other baseline
run has a 122 ms p95). Candidate p95s are 118–123 ms. These tiny tail samples
must not be generalized into a stable app-wide speedup.

Fresh-profile opening was also repeated six times after all desktop fixtures
exited, in AB/BA/AB order. First-UI p50 was 0.846 → 0.953 s; ranges were
0.820–2.665 s before and 0.856–0.964 s after, at 200 ms readiness resolution.
The four full-workload openings included a 3.854 s candidate outlier
(other candidate 1.325 s; baselines 1.313 and 1.720 s). All six opening-probe
viewports were 760×640 at 2× scale. Repeated warm openings show no clear
regression at this resolution; the outlier and cold-opening uncertainty remain.
No fivefold launch improvement is claimed.

## Allocation and retention evidence

Three AB/BA/AB pairs, 40 samples per case/process; median of process p50s:

| Production text case | p50 ms, before → after | Speedup | Allocations, before → after | Requested bytes, before → after |
|---|---:|---:|---:|---:|
| embedding | 0.087959 → 0.019333 | 4.55× | 4,299 → 12 | 66,844 → 8,604 |
| pages | 1.200583 → 0.433916 | 2.77× | 23,110 → 2,438 | 3,445,376 → 1,413,952 |

These are direct text-subsystem time/allocation-request measurements, not live
RAM or model inference. The allocation probe is test-only and thread-local.

The separate WebView image fixture uses the exact before/after production image
modules with controlled CacheStorage responses. All 96 images decode in both
versions. Each payload is 262,144 bytes, but the SVGs contain compressible padding;
24 MiB of nominal payload is **not** a claim of 24 MiB resident savings. Baseline
retains all 96 Blob URLs after unmount; candidate retains zero. Full-family
physical footprint and RSS after this fixture are recorded independently.
The image-fixture settled family footprint was 567.87 → 570.62 MiB, and
summed RSS 942.71 → 1,129.51 MiB. Thus this fixture proves lifecycle cleanup,
not a measured whole-app RAM saving. Ordinary catalogue/network distributions
may differ.

## Verification and investigation limits

- `pnpm typecheck`: passed.
- `pnpm test`: 736 passed, 9 skipped; 112 files passed, 2 skipped.
- `pnpm build:frontend`: passed.
- `cargo test --release --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1`:
  440 passed, 7 ignored in the complete final run.
- Both `desktop-bench,tauri/custom-protocol` and ordinary `tauri/custom-protocol`
  release app builds completed. The ordinary build is distinct from test/lib
  binaries and was not installed over the user's app. Its bytes match the frozen
  production candidate; `TEZBAR_BENCH_HOME` and `__desktop_bench_reply` markers
  are absent from it and present in the instrumented executable.
- Search parity: four executable runs, ten cases each, every ordered ID/score
  signature matches the preserved original beta.17 probes. Broad-file/deep
  search still does not uniformly reach the fivefold target; see the
  [search report](beta17-search-results.md).
- New regressions cover exact text outputs/allocation bounds, quiet observers
  and shutdown, real runtime state notifications, successful warm-request
  status stability, semantic queue-drain count budgeting and image ownership
  through component cleanup and asynchronous completion.

An earlier complete Rust suite timed out in the existing
`native_core::extensions::launcher::tests::timed_out_launcher_query_settles_and_recovers`
event fixture; its isolated retry and subsequent complete suites passed. One
earlier GUI automation attempt timed out awaiting a search response; its retry
passed. A final-protocol candidate attempt failed its native on-screen
assertion partway through visible idle; its partial JSON/log are preserved as
`final-desktop-candidate-2-visibility-failed.*`. Its cause was not established.
The unchanged-code repeat must pass the assertion before inclusion. These
discarded attempts are retained in the local artifact directory, not silently
counted as successful runs.

An experiment lengthening selected-model polling to one second was rejected:
it delayed indexing completion. An interim candidate exposed unnecessary
successful-request state transitions, which were fixed and regression-tested.
Direct database observation then showed 192 vectors saved sooner than the UI
reported; the terminal-count fix addresses that lag. A hybrid frontend control
retaining the original image module did not establish that image ownership
caused observed whole-process footprint variation. No app-wide RAM gain is
inferred from Blob-URL counts or ordinary model unloading.

The machine is an Apple M5 Pro (Mac17,9), 24 GiB RAM, macOS 26.3 (25D2125),
Rust 1.98.0, Node 26.7.0 and pnpm 11.17.0. There are only two processes per
version in the final desktop comparison: ranges show repeat variation, not
confidence intervals. Host/GPU scheduling and catalogue/network activity are
not globally controlled. Fresh core profiles share the test identity's normal
WebKit caches; launch milestones use 200 ms readiness polling and are not
cold-install/cold-OS-cache launch tests. This is macOS Q4 inference, not Windows,
voice/provider latency or an active compatibility-extension session benchmark.
Those features are preserved, not certified fivefold faster here.

## Reproduce and inspect

Machine-readable results are [beta17-runtime-candidate.json](beta17-runtime-candidate.json)
and [beta17-runtime-text.json](beta17-runtime-text.json). The
[chart](beta17-runtime-chart.html) is standalone HTML. Raw samples, process
families, profiles, frozen executables and complete logs remain under
`/tmp/tezbar-beta17-perf.e1qzXU`; their paths and hashes are in the JSON. Temporary
artifacts are local evidence, not a durable distribution of binaries.

From this worktree, with dependencies/resources already staged:

```sh
pnpm build:frontend
cargo build --release --locked --manifest-path src-tauri/Cargo.toml \
  --features desktop-bench,tauri/custom-protocol
# Copy src-tauri/target/release/tezbar to a unique temporary candidate executable.
node docs/performance/build-image-fixtures.mjs /tmp/tezbar-beta17-perf.e1qzXU
python3 docs/performance/final-desktop.py \
  --binary /tmp/tezbar-beta17-perf.e1qzXU/desktop-candidate-app \
  --model "$HOME/Library/Application Support/com.tezbar.app/knowledge/models/embeddinggemma-2-q4_k_m" \
  --image-fixture /tmp/tezbar-beta17-perf.e1qzXU/image-candidate.js \
  --output /tmp/tezbar-beta17-perf.e1qzXU/final-desktop-candidate-1.json
# Run baseline/candidate/candidate/baseline sequentially with fresh profiles,
# choosing the corresponding preserved binary and image module each time.
python3 docs/performance/desktop-opening.py \
  --artifacts /tmp/tezbar-beta17-perf.e1qzXU \
  --model "$HOME/Library/Application Support/com.tezbar.app/knowledge/models/embeddinggemma-2-q4_k_m" \
  --output /tmp/tezbar-beta17-perf.e1qzXU/runtime-opening.json
python3 docs/performance/summarize-runtime.py \
  --artifacts /tmp/tezbar-beta17-perf.e1qzXU \
  --output docs/performance/beta17-runtime-candidate.json
python3 docs/performance/render-runtime-chart.py \
  docs/performance/beta17-runtime-candidate.json docs/performance/beta17-runtime-chart.html
cargo build --release --locked --manifest-path src-tauri/Cargo.toml \
  --features tauri/custom-protocol
python3 docs/performance/compare-runtime-text.py \
  --baseline-executable /tmp/tezbar-beta17-perf.e1qzXU/runtime-baseline-tests \
  --candidate-executable src-tauri/target/release/deps/app_lib-42960ca2c7041bb3 \
  --output docs/performance/beta17-runtime-text.json
python3 docs/performance/compare-search.py \
  --baseline-executable /tmp/tezbar-beta17-perf.e1qzXU/baseline-instrumented-tests \
  --candidate-executable src-tauri/target/release/deps/app_lib-42960ca2c7041bb3 \
  --rounds 2 --samples 40 --output /tmp/tezbar-beta17-perf.e1qzXU/runtime-search-parity.json
```

The executable hash suffix is host/build dependent: choose the actual completed
release test executable. `semantic-throughput.py` retains the narrower real
model-completion experiment; `hybrid-vite.config.ts` retains the frontend
diagnostic control. Neither replaces the final complete desktop protocol.
