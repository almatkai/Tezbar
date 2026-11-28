# Original beta.17 → final cumulative comparison

**Uniform 5× runtime/search and one-fifth app RAM: not achieved.** Some native searches and UI responses exceed 5×. Durable indexing, visible frame proxies, idle activity and total family memory do not improve uniformly by that amount. No ratios from previous rounds are multiplied.

Measured 2026-10-30 in the authorized `Tezbar-performance` worktree, branch `fix/beta17-runtime-search-memory`. [Machine-readable evidence](beta17-final-results.json) · [comparison chart](beta17-final-results.html).

## Reference and final identity

Original production source is **`9ff3663342b48ab01be2e56ab30f6d7123b0ced8`**. This is a fresh release rebuild under the same local toolchain, not a recovered signed shipping binary. `prepare-final-reference.py` archives that ref inside ignored build artifacts, then applies only native test probes from `d874f1f` and the same opt-in desktop harness as the candidate. Exactly five original files change and two files are added; the patch is retained as `measurement-only.patch`, SHA-256 `f9003918bd69fe4df18b738198f151022456f5474e78229b44b68648a7405325`. All 559 authored original source inputs were rechecked after building. No production search, dispatch, idle, allocation or runtime-package optimization enters this reference. Cargo/pnpm lockfiles match.

Candidate production code remains **`ac14c06bfe6435c83e5399f39ec8158227732d6c`**. This round adds one `cfg(test)` regression for the real source-root/dev resource seam, measurement receipts and reports; it adds no production optimization or UI change. Both latest ordinary and benchmark actual `.app` packages were rebuilt after that test. Full source hashes and build commands are retained. `c81288e` is an intermediate R6 baseline and is never called original beta.17 here.

| Frozen executable | SHA-256 |
|---|---|
| `original-ordinary` | `230a98986d9657b688fd44d9f6e78c641ceac6b91feba0f29699af47fee4e200` |
| `original-bench` | `5a219f1bcff77ffb3dc70100ef21258795ccd7a7867b9e8333ba9f0b26139f74` |
| `original-native-tests` | `5cc9ce69dcb765b5646f8c33906c299fe34f95ad6e594e83aa9b1c59946b64a8` |
| `accepted-ordinary` | `d8c789fb6266b8a5544ac89add5017a0900d64cd5433ace907b982722ef09727` |
| `accepted-bench` | `1cbb16091ef42dc0fe72d009d0d84046663ead874e27b3cbb30ec6fc5923ffac` |
| `accepted-native-tests` | `1062aa8299fd0127a8a540a7b09a7a4d6b87b5b008e425fac964b6229eedc7e4` |

The final ordinary executable contains neither `TEZBAR_BENCH_HOME` nor `__desktop_bench_reply`. Its actual packaged activation loaded the committed app-owned libllama and preserved old model manifests. ZIP SHA-256 is `75698aa84eb35398cee7b3afb6fee7b9bdb1db7a3aedc3cc43af25780100e475`; libllama is `b459731b73340e3ad888f1b5fec9a26644940976edfc950fbc1bdd11673fecaf`.

## Method and scope

Apple M5 Pro, 24 GiB, macOS 26.3 (25D2125), Rust 1.98.0, Node 26.7.0, pnpm 11.17.0, AppleClang 21. Release optimization; default Tauri custom protocol; explicit local ad-hoc signing, updater artifacts disabled, no notarization. Original and final use their own production frontend and complete packaged resources in isolated benchmark identity wrappers. The ordinary package has a separate non-harness activation check.

Native: 100,400 launcher rows and 10,000 deep sources/vectors; AB/BA, 80 samples/case/process, three warmups and first-call values retained. Desktop: original/final/final/original, two processes/build, equal APFS-cloned Q4 weights/settings, 5,000 files, 96 generated documents and 192 actual durable vectors. CJK-rich long chunks have one extra pair, with the same UTF-16 chunk budget. All timed profiles are under OS temp, never under a launcher-excluded `build` component.

Values below are medians of process summaries. Per-process p50/p95 use nearest rank; they are not pooled percentiles or confidence intervals. Small UI cohorts (4–16 bursts/process) make p95 particularly descriptive. OS/shader caches and unrelated desktop activity are uncontrolled. “Cold semantic” clears only the app vector cache. Opening is coarse first-input DOM readiness with 200 ms polling. Two frames are render opportunities, not pixel paint or first-commit timestamps.

No task builds/tests overlapped timed phases. Locked-display, native visibility and physical-input guards stayed active. Accounting includes every observed responsible-PID/descendant identity: host, WebKit/GPU/network/platform helpers, QuickLook, model and supervisor, including Metal/compiler helpers if observed. Half-second sampling can miss short-lived helpers and transient peaks. Footprint is a **sum of physical-footprint ledgers, not deduplicated RAM**; summed RSS can double-count shared/clean pages. CPU units independently matched `getrusage` (0.349796 vs 0.349783 CPU-s). Normal clipboard observation remained enabled; no clipboard content was used in workloads or copied into evidence, and fixture-local clipboard stores were removed without inspection after shutdown. No clipboard writes occurred.

## Native search

All **40 ordered signature comparisons** match original beta.17, with every timed reply checked. These are native subsystem timings, not UI/model inference or whole-app RAM. CPU was not separately timed per native query.

| Workload | p50 ms, original → final | p95 ms, original → final | p50 ratio | 5× p50 / p95 |
|---|---:|---:|---:|---|
| deep.semantic.cold | 8.083 → 6.639 | 8.421 → 6.793 | 1.22× | MISS / MISS |
| deep.semantic.warm | 1.286 → 0.613 | 1.319 → 0.633 | 2.10× | MISS / MISS |
| deep.lexical | 18.999 → 10.544 | 19.372 → 10.691 | 1.80× | MISS / MISS |
| launcher.home | 1.788 → 0.870 | 1.889 → 0.888 | 2.05× | MISS / MISS |
| launcher.exact | 4.683 → 0.906 | 4.887 → 0.936 | 5.17× | PASS / PASS |
| launcher.prefix | 2.605 → 0.800 | 2.702 → 0.826 | 3.26× | MISS / MISS |
| launcher.typo | 6.091 → 1.465 | 6.320 → 1.510 | 4.16× | MISS / MISS |
| launcher.keyboard_layout | 3.136 → 0.797 | 3.208 → 0.828 | 3.93× | MISS / MISS |
| launcher.file_heavy | 28.342 → 24.118 | 28.780 → 24.352 | 1.18× | MISS / MISS |
| launcher.no_hit | 5.117 → 0.883 | 5.233 → 0.904 | 5.80× | PASS / PASS |

## Desktop input and work

Each response requires a fresh final-query native reply, followed by the same two-frame proxy and ordered DOM-title check. Full rows/scores/actions equal direct native replies and each other on a cloned authoritative index (16 replies). Synthetic IME interruption + native hide/reopen recovery passes.

| Workload | Response p50 / p95 ms, original → final | Two-frame p50 / p95 ms, original → final | Response / frame p50 ratio | 5× response / frame p50 |
|---|---:|---:|---:|---|
| Simple bursts | 94.5 → 14.0 / 98.5 → 16.0 | 136.5 → 43.5 / 144.5 → 47.5 | 6.75× / 3.14× | PASS / MISS |
| Deep lexical bursts | 194.5 → 13.0 / 206.5 → 14.0 | 220.5 → 42.5 / 232.5 → 46.5 | 14.96× / 5.19× | PASS / PASS |
| Warm deep bursts | 223.5 → 25.0 / 229.0 → 35.5 | 248.0 → 51.0 / 259.0 → 57.5 | 8.94× / 4.86× | PASS / MISS |
| Repeated warm deep | 224.0 → 24.5 / 226.5 → 38.0 | 250.5 → 49.5 / 258.0 → 63.0 | 9.14× / 5.06× | PASS / PASS |
| Typing during source indexing | 92.0 → 14.0 / 100.5 → 32.0 | 103.5 → 43.5 / 121.0 → 53.0 | 6.57× / 2.38× | PASS / MISS |
| Typing during real vector indexing | 96.5 → 28.0 / 97.0 → 30.5 | 112.0 → 54.5 / 115.5 → 59.5 | 3.45× / 2.06× | MISS / MISS |

Frame p95 still misses 5× for simple, warm/repeated deep and indexing. Deep lexical reaches 5× in this small cohort. Opening proxy is **871.5 → 860.2 ms** (original 856.3–1002.0; final 846.7–868.4), with the same 760×640, 2× viewport. No material opening regression is observed within this coarse warm-cache protocol; this is not a cold-start/tail guarantee. Twelve actual hide/show/search cycles give two-frame p50 **129.0 → 48.5 ms** (2.66×), p95 132.5 → 50.0 ms.

| Work / activity | Original → final | Ratio | 5× |
|---|---:|---:|---|
| 192-vector durable completion, s | 15.027 → 10.244 | 1.47× | MISS |
| Family CPU through actual durable completion, CPU-s | 9.358 → 2.490 | 3.76× | MISS |
| Published completion, s | 15.676 → 10.282 | 1.52× | MISS |
| Eight simple bursts, family CPU-s | 1.141 → 1.450 | 0.79× | MISS |
| Eight warm deep bursts, family CPU-s | 1.230 → 0.625 | 1.97× | MISS |
| Sixteen repeated deep bursts, family CPU-s | 2.430 → 1.181 | 2.06× | MISS |

Durable wall times are original **16.761/13.292 s**, final **10.114/10.374 s**; CPU is **9.501/9.216 → 2.474/2.505 CPU-s**. Actual DB rows, not progress UI alone, determine completion. The initial package verification/materialization cost is included on final. Faster input scheduling dispatches more intermediate work: simple-burst family CPU increases; source-indexing burst CPU also increases 0.500 → 0.629 CPU-s. Complete dispatched counts and outliers remain in JSON. These are costs to retain in the acceptance decision.

## Complete family memory and idle

| Matched phase | Footprint p50 / p95 MiB, original → final | Settled summed RSS MiB | Footprint ratio | One-fifth footprint / RSS |
|---|---:|---:|---:|---|
| Initial hidden, no model | 143.50 → 144.17 / 150.41 → 152.68 | 325.40 → 325.66 | 1.00× | MISS / MISS |
| After simple bursts | 168.72 → 269.38 / 335.52 → 338.20 | 589.09 → 605.77 | 0.63× | MISS / MISS |
| Model retained after bulk inference | 490.45 → 376.69 / 640.73 → 550.09 | 1199.59 → 1004.91 | 1.30× | MISS / MISS |
| After repeated deep queries | 488.77 → 369.19 / 682.19 → 623.81 | 1201.88 → 1034.23 | 1.32× | MISS / MISS |

Ordinary retained footprint is original **489.82/491.08 MiB**, final **450.69/302.69 MiB**. The final two-process range is substantial and is not hidden by the median. Its one-fifth target is **98.09 MiB**; final median **376.69 MiB** misses by **278.60 MiB**. After repeated queries, footprint is **488.77 → 369.19 MiB** (1.32×), also far from one fifth. After simple bursts footprint regresses. No universal RAM reduction is demonstrated.

| 30-second quiet phase | Family CPU-s | Interrupt wakeups | CPU ratio | 5× CPU |
|---|---:|---:|---:|---|
| Initial hidden | 0.146 → 0.125 | 1521.5 → 1210.0 | 1.17× | MISS |
| After simple bursts | 0.206 → 0.173 | 1544.0 → 1192.5 | 1.19× | MISS |
| Retained model | 0.292 → 0.281 | 7397.5 → 6098.0 | 1.04× | MISS |
| After repeated deep | 0.271 → 0.256 | 7227.0 → 6041.5 | 1.06× | MISS |

Package idle-wakeup counters are zero in both builds; interrupt wakeups above are a distinct counter. After repeated work, supervisor interrupt wakes are **1152.5 → 0** per 30 s, while llama-server remains **4655.5 → 4654**. This directly shows why the supervisor fix does not create a fivefold whole-family idle gain.

**CJK-rich single pair:** retained footprint **1169.29 → 314.80 MiB** (3.71×; one-fifth **MISS**), RSS **1770.30 → 1010.19 MiB**. Durable completion is **26.233 → 18.977 s** (1.38×). This pair is descriptive, not a repeated tail/reliability claim. No stock 8192-token/8-GiB allocation experiment was repeated.

After **90 quiet seconds**, the same model PID/start identity remains in both builds under the unchanged 120-second retention policy. First fresh visible response is **235 → 34 ms** (6.91×), plus two frames **267 → 66 ms** (4.05×, misses 5×). Ordered first/recovery replies match. This is one pair, separate from retained-RAM windows, with no keepalive or early unload.

## What remains and what passed

Broad-file BM25 still evaluates many matching rows, and cold authoritative semantic traversal still reads the 10k-row corpus; their measured ratios are only 1.18×/1.22×. UI dispatch improvements exceed native improvements, while two-frame scheduling overhead remains visible. More intermediate simple queries consume additional CPU and correlate with larger WebContent retention; this cohort does not prove the retained allocation owner. No new speculative optimization was attempted.

After repeated work, median process-summary final model-server footprint is **102.75 MiB**, down from **322.96 MiB**. WebContent is **79.81 → 175.56 MiB** (final individual ledgers vary sharply); host is **44.62 → 47.17 MiB**, with other WebKit/platform charges still present. The runtime alone is around the ordinary one-fifth family budget before the host/UI. Removing unused logits therefore cannot by itself satisfy the total-family target under these settings. These are measured ledgers, not a proof of an immutable architectural lower bound.

- **450 Rust tests passed**, 8 ignored; **747 TS tests passed**, 9 skipped; typecheck, both frontend/app builds, 5 package-pin and 16 release-policy checks passed. Actual packaged Bun worker opening/roundtrip/termination passed.
- The added source-root/dev configuration test resolves the real staged ZIP and validates its committed pin; missing packaged resources fail closed in both dev/release path configurations. This is a real path/config seam check, not a full unbundled renderer launch.
- All six desktop processes retain 192 identical stored vectors per corpus and the same source/model/index key. Q4/Q8 original installed weights/manifests/runtimes are unchanged. **84 R6 exactness/boundary artifact hashes were reverified** and reused explicitly because server/ggml/libllama/catalog/weights bytes are identical: full 768-value outputs and 256-F32 values, 2/4/8 threads, 4/8/16 inputs, 2048/4096/8192 boundaries, CJK and unchanged 8193 rejection. These are parity evidence, not multiplied timing ratios.
- **199 observed PID/start identities audited; zero survivors.** Ordinary activation used the latest accepted `.app`, not an older cf8001b executable. No pending owned compiler, test, app or model process remains.

Only macOS arm64/synthetic fixtures were executed. There is no personal-corpus or other-platform acceptance claim. Missing historical `/tmp/tezbar-beta17-perf.e1qzXU` raw data is not used. All timed attempts in this new cohort completed; initial archived-source pnpm and post-exit SQLite report-assembly failures are retained and labelled separately. Historical reports and the unrelated untracked `beta17-results-report.html` are untouched.

One recovery-copy limitation remains: a missing/corrupt bundled ZIP can inherit the existing generic “Download again to repair” model hint, although the specific bundled-resource cause is included and execution fails closed. Redownloading weights cannot repair a missing app resource. No such package failure occurred in the successful ordinary activation; no automatic weight redownload was introduced.

## Retained artifacts and release boundary

All new artifacts are in **`build/performance-runtime-20261030/cumulative-20261030/`**:

- `original-source.tar`, `original-source/`, `measurement-only.patch`, `reference-receipt.json`, build/acceptance commands and logs.
- `accepted-ordinary.app`, `accepted-bench.app`, `original-ordinary.app`, `original-bench.app`, and frozen executable/test copies listed above.
- `original-1/`, `final-1/`, `final-2/`, `original-2/`, `long-original/`, `long-final/`; every result, sample, inspection and cleanup receipt.
- `native-comparison.json`, `native-raw/`, `ui-parity-opening.json`, `retention90.json`, `ordinary-activation/`, CPU calibration and fixture-data hygiene receipts.
- `closed-fixtures/*/knowledge.sqlite3`: immutable post-shutdown synthetic DB copies, pinned to the live fixture bytes so OS-temp cleanup cannot erase durable-vector evidence.

Reproduce reference preparation with `prepare-final-reference.py`; run `cumulative-desktop.py` with new output names/default OS-temp profiles; freeze evidence with `summarize-final.py` and render with `render-final.py`. Exact commands/environment/build flags and full hashes are in JSON/ignored receipts.

External release preparation remains separate: approve Developer ID runtime signing, review the newly signed archive/catalog pins, rebuild and revalidate, then perform the approved multi-platform signing/notarization/release process. The current catalog is local-ad-hoc and the signed-release wrapper rejects it. No push, PR, merge, remote build, workflow dispatch, signing-secret access, installed-app replacement or publication occurred.
