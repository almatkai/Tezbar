# Beta.17 same-model memory and background runtime investigation

Measured 2026-10-29–30 in `Tezbar-performance`, branch `fix/beta17-runtime-search-memory`.
Immediate production baseline: **bcef608**. Historical reports, binaries and JSON
are unchanged. Full new evidence: [beta17-memory-results.json](beta17-memory-results.json)
and [comparison chart](beta17-memory-chart.html).

**The one-fifth total RAM and fivefold whole-runtime targets remain unmet.**
Production indexing uses **3.99× less family CPU**, with essentially unchanged
wall time. Production retained footprint increases in this cohort. A separately
built, same-model runtime prototype removes a large unused buffer and reaches
**268.80 MiB for the complete family**;
it is not enabled in production and has unresolved first-use/packaging checks.
The earlier accepted 531 MiB active-family reference implies a **106.2 MiB** target.

## Production changes and measured behavior

- Pass `--poll 0 --poll-batch 0` to the pinned embedding runtime. Compute workers
  [park between jobs instead of busy-waiting](https://github.com/ggml-org/llama.cpp/blob/b9acf138a1e28ce1fc23b5a4fc4b12444b50f7ea/ggml/src/ggml-cpu/ggml-cpu.c#L3206). Keep the existing 8192 context,
  logical/physical batch bounds, parallelism one, 2/4/8 slow/balanced/fast threads,
  4/8/16 indexing batches, 768-output/256-stored dimensions and 120-second policy.
  This reduces CPU work, not model inference by a universal factor.
- Unix supervisors block on lease closure or `waitid(WEXITED | WNOWAIT)` instead
  of a 20 ms timer. The group leader stays unreaped until owned-tree cleanup;
  the exit waiter is joined before reaping. Lease EOF, cancellation, failure
  status and extension protocol cleanup remain intact. Windows supervision is
  unchanged. Actual supervisor regressions first observed **42/43 wakes per
  second**, then passed; desktop retained-idle supervisor wakes are **zero**.

The final release cohort is **AB/BA**: `memory-cpu-desktop-baseline-2`,
`candidate-1`, `candidate-2`, `baseline-3`. Each is a fresh isolated profile with
5,000 home files, 96 documents, 192 real Q4 vectors and the accepted release
frontend/input cadence. No builds/tests overlapped timed desktop workloads.
Accounting includes the exact responsible PID and descendants, including WebKit,
GPU, supervisors and observed Metal compiler services. Nothing selects processes
just by name. Each idle lasts 30 seconds; values are medians of two process
summaries, not pooled samples or confidence intervals.

| Phase | Family CPU seconds, before → final | Summed footprint MiB, before → final | Settled summed RSS MiB, before → final |
|---|---:|---:|---:|
| Initial hidden | 0.201 → 0.133 | 170.92 → 144.52 | 371.72 → 269.36 |
| After simple bursts | 0.218 → 0.165 | 178.89 → 255.77 | 573.40 → 562.50 |
| After bulk inference, model retained | 0.381 → 0.353 | 509.64 → 590.75 | 1188.11 → 1178.73 |
| After repeated deep bursts, model retained | 0.357 → 0.296 | 504.33 → 574.13 | 1202.20 → 1185.86 |

Footprints are **summed physical-footprint ledgers, not deduplicated private RAM**.
RSS can count shared/clean pages repeatedly; footprint includes compressed/swapped
charges. Sampled peaks, settled values, per-process ledgers, pressure, page and
swap context remain in JSON/raw files. No pressure generator, cache flush, forced
GC, model unloading, WebView recreation or feature/cache disabling was used.

Bulk inference takes **9.44 → 9.41 s**, using **8.262 → 2.070 CPU-s**.
Individual durable times are 10.459/8.348 s before and 8.248/10.500 s after: there
is **no demonstrated throughput speedup**. Periodic foreground demand is equal;
query counts and grace periods can differ with completion time. Warm deep-burst
CPU is 1.416 → 0.663 s;
repeated deep-burst CPU is 2.453 → 1.180 s.
Idle CPU/wakeup changes vary: one baseline host has 4,560 wakes versus 744 in the
other, while the model retains roughly 5,400 wakes/30 s. Captured stacks identify
the [stock Metal 5 ms residency heartbeat](https://github.com/ggml-org/llama.cpp/blob/b9acf138a1e28ce1fc23b5a4fc4b12444b50f7ea/ggml/src/ggml-metal/ggml-metal-device.m#L987); it remains enabled. Clipboard,
watcher recovery, downloads and progress cadences are unchanged.

Response-plus-two-frame p50s remain simple **45.00 → 44.00 ms**, deep
lexical **43.25 → 40.00 ms**, warm model **53.00 → 51.50 ms**,
repeated model **50.00 → 50.00 ms**. During semantic indexing:
**59.00 → 59.00 ms**. This is a response/render-opportunity
proxy, not a pixel-paint timestamp. Six fresh openings give first-UI medians
**835.8 → 836.6 ms**, at 200 ms readiness resolution;
all viewports remain 760×640 at 2×. Warm OS/test-WebKit caches and uncontrolled
focus/shader state prevent a cold-install or tail-latency guarantee.

After **90 seconds without inference**, both builds retain the same model PID.
The first visible fresh response is **34.0 → 34.0 ms**;
response plus two frames is **66.0 → 65.0 ms**.
Native ordered rows/scores/actions match on cloned indexes. Accepted frontend
scheduling, opening and hide/reopen composition behavior are preserved.

## Where memory actually goes

Settled per-role footprint after bulk inference, MiB; production columns are
medians of two process endpoints, prototype is one separate process. These endpoint
statistics should not be added to the table's interval medians.

| Owner | Baseline | Production final | Experimental runtime |
|---|---:|---:|---:|
| Native host | 51.46 | 50.27 | 46.67 |
| WebContent | 94.58 | 167.32 | 80.34 |
| Model | 322.33 | 321.79 | 102.14 |
| WebKit GPU | 18.27 | 19.06 | 18.05 |
| Supervisor | 2.48 | 2.53 | 2.52 |

The installed runtime reports **0.6.0-dev, b11474, b9acf138a**, AppleClang 21.
Actual Q4 GGUF metadata has vocabulary **262,144**, non-causal attention,
768 output dimensions, BOS **and EOS**, and a larger training context than the
app's unchanged 8192 limit. The 173.28 MiB file is not a universal RAM floor:
normal model `vmmap` shows about 184 MiB of clean mapped files, 222.5 MiB of
`VM_ALLOCATE`, a malloc zone with 185.3 MiB allocated/72.1 MiB dirty, and 21.3 MiB
of empty large allocator regions. Clean mappings, allocated bytes, resident
pages and physical-footprint charges are distinct.

The decisive source finding is [`output_reserve()` in the exact pinned commit](https://github.com/ggml-org/llama.cpp/blob/b9acf138a1e28ce1fc23b5a4fc4b12444b50f7ea/src/llama-context.cpp):
it always allocates/clears vocabulary logits for every output token and retains
its largest capacity, even for this dedicated pooled encoder. Here
`tokens × 262144 × sizeof(float)` is **1 MiB per token**. An ordinary 223-token
indexing chunk explains roughly 223 MiB of avoidable retained storage; an
8192-token sequence requests **8 GiB** of logits alone. This is principally a
CPU output-buffer allocation, not a model-file-size theorem or merely GPU charge
movement. The [patch](llama-b11474-embedding-output.patch) omits those unused
logits only for Gemma-embedding2 with mean pooling, embeddings enabled and no
samplers. Embedding storage, graphs and kernels remain unchanged.

Two local libraries were built from the same pinned source: unchanged control
and this one-condition patch. Their RPATHs use the cloned **official ggml/Metal
backends**, not CMake's newly compiled kernels. The unchanged control matches the
installed runtime's complete vector payloads. Q4/Q8, all three speed modes,
4/8/16 input batches, 2048/4096/8192-token inputs and the existing 8193 rejection
match raw 768-value and normalized F32 hashes. A realistic 1200-UTF16-unit CJK
chunk plus title uses 2252–2253 tokens; a 16-input request totals 36,038 tokens
and preserves all inputs. No supported sequence/result/index budget was reduced.

| Q4 model experiment, same requests | Stock footprint MiB | Patched footprint MiB |
|---|---:|---:|
| After long-chunk batches | 2428.85 | 175.22 |
| After 8192-token input | 8993.76 | 739.27 |
| Actual desktop after 192 ordinary vectors | 322.33 | 102.14 |

The large stock case includes about 3.4 GiB of compressed/swapped charges; it is
not a claim of 9 GiB uniquely resident DRAM. The prototype's complete desktop
family is **268.80 MiB** after indexing and **268.05 MiB** after repeated queries,
with **1002.06/1005.56 MiB summed RSS**. All 192 stored vectors remain byte-identical.
The prototype's first measured Q4 ready time is **5.319 s**, versus a stock
**3.445 s** process; its one desktop durable-index time is **12.474 s**, slower
than both production candidates. Those first-use differences are unresolved,
not hidden as a warm-cache win. Its native ordered query/action parity passes;
this is evidence for a runtime fix, **not approval to ship it**.

The next concrete step is a trusted, signed, hash-pinned runtime artifact with
this patch, matching original build options and validating first use/platform
compatibility before changing the runtime catalog. This task changes neither
catalog nor installed runtime. Even the measured prototype exceeds 106.2 MiB by
**162.6 MiB**: its model alone uses 102.1 MiB and the other owners about 166.7 MiB.
Further progress needs measured tokenizer/host/WebKit budgets and likely a more
compact tokenizer or UI/process architecture. A smaller model alone is not an
established solution. No automatic model, reindex or architecture switch occurred.

## Rejected candidates, cache scale and limits

| Experiment | Outcome |
|---|---|
| `ubatch=2048` | Lower memory but rejects valid 2252-token chunks and 4096/8192-token sequences. Rejected. |
| `--no-warmup` | Ready 3.306 vs 3.445 s in cached processes; retained footprint still about 2428 MiB. Rejected. |
| HTTP threads 1 | Negligible memory difference, no idle-wake benefit; workers already block. Rejected. |
| Explicit Flash Attention on | Same payloads and footprint as auto on this host. No useful change. |
| Disable Metal residency sets | Whole-family wakes 8299.5→2015/30 s, but first native request after 90 s slows 14.762→23.848 ms; hidden bridge 35→68 ms. Rejected; exact measured source and artifacts retained. |
| CPU polling off | Payloads unchanged, substantial active CPU reduction, visible 90 s guard passes. Adopted. |

An on-disk **100,000-vector / 6,250-source** fixture uses 110.19 MiB estimated
cache allocations within the unchanged 1 GiB allowance, with exact cold/warm
ordering. At **one million vectors / 62,500 sources** (1.70 GB database), the
cache rejects before allocation and the authoritative streaming lane remains
exact. Clearing the 100k cache leaves process RSS around 133 MiB: allocation
release is not immediate physical RAM recovery. This is synthetic, compressible
data, not whole-app or real-profile RAM evidence. The real database file is about
9 GiB; a read-only SQLite open failed, so only file/header metadata was read and
no private paths/content or unsupported row counts are reported.

First-use Metal costs remain material: the earlier recorded **20.922 s / 26.049
CPU-s** indexing run is unchanged. This round's rejected-residency cohort also
contains **16.95/18.47 s, 25.17/25.42 CPU-s** semantic phases with newly owned
compiler helpers. Shader/OS caches were not flushed or controlled. Current warm
cohorts cannot certify cold shader behavior. Windows, physical OS IME, voice/live
providers and large real-profile cache behavior are not newly performance-certified.

## Verification, failures and reproduction

- Final **747 TS passed, 9 skipped**, typecheck and frontend build pass.
- Final serialized release Rust: **444 passed, 8 ignored**. The opt-in real Bun
  worker roundtrip/termination also passes with simulated clipboard services.
- Forty original-beta.17 ordered native signatures match. Sixteen fresh UI/native
  final replies and cloned-index cross-build rows/scores/actions match. Synthetic
  composition plus native hide/reopen passes. Q4 prototype native parity also passes.
- Every desktop run saves the same 192-vector payload SHA-256:
  `27efcec49c95f8e1d1d0a96046b8efdb509e07219f1e070b06ab8e0e8d60399c`.
- Instrumented **and ordinary actual release executables** were relinked after the
  final native change. Benchmark markers occur only in the instrumented binary.
  Source/binary/evidence hashes are in JSON. All owned fixture processes closed;
  332 recorded PID/start identities were audited. Installed Q4/Q8 and runtime
  library hashes still match their originals.
- Failed initial model boundary construction counted only one special token;
  fixed before complete runs. Final CPU baseline attempt 1 caught physical input
  interference without recording text/keys. Prototype desktop attempt 1 lost
  on-screen visibility and timed out before model activation; cause unestablished.
  Unchanged-code retries passed. A prototype-build path guard initially rejected
  the explicitly authorized `/tmp` artifact root before downloading/building;
  corrected locally. All failed/partial artifacts remain in
  `/tmp/tezbar-beta17-perf.e1qzXU`; no measurements were silently replaced.

Use **new** output paths, from this worktree. Preserved baseline is
`desktop-responsiveness-final-app`; final instrumented/ordinary binaries are
`desktop-memory-cpu-app` / `production-memory-cpu-app`. `memory-desktop.py` adds
out-of-timer component inspection to the accepted workload. `model-memory.py`
APFS-clones resources and runs a production-owned supervisor. `retention-checks.py`
uses a consistent, completed isolated index clone. Example commands:

```sh
python3 docs/performance/model-memory.py --model "$MODEL" \
  --supervisor /tmp/tezbar-beta17-perf.e1qzXU/desktop-memory-cpu-app \
  --config poll0 --threads 8 --output /tmp/NEW-model-run
python3 docs/performance/memory-desktop.py --model "$MODEL" \
  --binary /tmp/tezbar-beta17-perf.e1qzXU/desktop-memory-cpu-app \
  --output /tmp/NEW-desktop-run
python3 docs/performance/build-llama-memory.py --output /tmp/NEW-llama-build
# Prototype only: pass its libllama-no-logits.dylib with --library to model-memory.py
# or --experimental-library to memory-desktop.py; only a temporary clone changes.
python3 docs/performance/summarize-memory.py \
  --artifacts /tmp/tezbar-beta17-perf.e1qzXU --output /tmp/NEW-memory-summary.json
```

`$MODEL` is the existing installed Q4 directory specified for this task (or the
existing Q8 directory for direct variant checks). The source builder downloads
only pinned public source and an isolated CMake tool, not model weights or global
packages. No real profile/model installation, clipboard, credentials, global
configuration, installed application, remote branch or deployment was changed.
