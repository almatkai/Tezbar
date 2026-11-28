# Beta.17: unused-logits fix packaged and integrated

Local macOS arm64 follow-up to **c81288eb2e3c59e8284402dfef13449b5603481a**,
2026-10-30, same authorized worktree/branch. Full receipts and hashes:
[beta17-runtime-package-results.json](beta17-runtime-package-results.json).
Historical reports are unchanged. **No publication, deployment, installed-app
replacement, model change, or real-profile migration was performed.**

**The experimental allocation fix now has a working, tested ordinary-app path.**
The final package is local/ad-hoc signed, not Developer ID/notarized. The release
wrapper rejects this catalog for signed distribution. Whole-app fivefold speed
and one-fifth RAM remain **unmet**; results are workload-specific.

## Implementation and trust

See [the build/install contract](../EMBEDDING_RUNTIME.md). The app bundles the
pinned runtime ZIP as a resource, with a source-controlled archive/file catalog.
`build:backend` verifies or reproducibly assembles it; direct Cargo builds fail
closed on missing/wrong staged bytes. The original server and numerical backends
are official b11474 bytes. Only pinned-source libllama has the narrowly guarded
Gemma-embedding2/mean-pooling/no-samplers allocation patch.

Actual prewarm/launch verifies and stages the resource in a content-addressed
profile runtime directory. Merely having installed weights does not extract it
at opening. A process-local hash/fingerprint cache preserves foreground checks.
Flat ZIP inventory, pinned hashes, cancellation cleanup, atomic publication,
failed-rename rollback and authenticated cache repair are covered by tests.
Q4 and legacy Q8 upgrade without rewriting their old manifests, weights, model
keys or databases. Old upstream runtime directories remain usable by an old app.
No arbitrary library/env override, fixture-manifest replacement, unpublished
runtime URL, model redownload or reindex is required for this runtime upgrade.

The actual ordinary Tauri `.app`, with benchmark markers absent, activated its
bundled runtime in an isolated existing-model profile. Its loaded-library mapping
and every installed runtime file matched the committed pins; old manifests stayed
unchanged. Both actual ordinary and benchmark `.app` packages were built locally.
The actual bundled Bun worker roundtrip/termination also passed.

The ZIP SHA-256 is
`75698aa84eb35398cee7b3afb6fee7b9bdb1db7a3aedc3cc43af25780100e475`;
patched libllama is
`b459731b73340e3ad888f1b5fec9a26644940976edfc950fbc1bdd11673fecaf`.
Independent builds, including a selected-worktree path containing spaces and the
fresh automatic build hook, reproduced the ZIP byte-for-byte. Source, source
archive, pinned CMake wheel, compiler/configuration, RPATH/dependency receipts and
MIT license are retained. No newly compiled ggml numerical backend is packaged.

## Startup and first use

A balanced interleaving ran **six fresh processes per treatment**, each treatment
occupying every ordinal position twice. OS/shader caches were not flushed. All
full 768-value payloads matched; no owned survivors remained.

| Direct same-model subsystem | Stock | Rebuilt unchanged control | Patched |
|---|---:|---:|---:|
| Ready median, ms | 3354.4 | 3382.3 | 3401.2 |
| First query median, ms | 14.25 | 14.34 | 13.47 |
| First 16-chunk batch median, ms | 287.25 | 291.79 | 272.80 |

Stock ready spans **3.245–14.216 s**, control **3.230–4.430 s**, patch
**3.204–12.486 s**. Phase capture separates process spawn, first server log,
health attempts and internal model-load timestamps. Both stock and patch have
~9-second internal-load outliers; pre-first-log delays vary too. No new owned
compiler helper was observed in this startup cohort. Pressure/page/swap context
is retained. These are warm/uncontrolled OS-cache runs, not certified cold shader
startup. They do **not** reproduce a persistent patch-specific startup penalty,
and do not erase the earlier slower prototype observations.

A real packaging regression was investigated rather than hidden: putting the
24 MB ZIP inside the executable enlarged it to 58 MB and the opening cohort was
about one 200 ms readiness interval slower. Deferring extraction alone did not
remove that difference. The final app uses a pinned external app resource and a
**33.7 MB** benchmark executable. Final six interleaved openings give first-input
DOM-readiness medians **881.6 → 868.2 ms**, with the same 760×640, 2× viewport.
This coarse, warm-cache proxy is not a cold-install or tail-latency guarantee.

## Complete desktop family

Final ordinary workload: **AB/BA**, two processes per build, 5,000 launcher files,
96 documents and 192 real Q4 vectors. Representative CJK-rich long chunks have
one additional complete pair, preserving the same UTF-16 chunk budget. Every
profile's actual launcher file count was checked. The final runs use OS-temp
profiles, not paths beneath the launcher's ignored `build` component.

Exact responsible PID plus descendants includes host, WebContent, GPU, network,
observed platform/Metal helpers, model and supervisor. Values below are medians
of process summaries, not pooled samples/confidence intervals. Footprint is a
**summed physical-footprint ledger, not deduplicated RAM**; RSS separately counts
shared/clean residency and must not substitute for it. No pressure generation,
cache flush, forced GC, model/feature disabling or WebView recreation occurred.

| Complete-family milestone | Stock footprint MiB | Patched footprint MiB | Stock → patched summed RSS MiB |
|---|---:|---:|---:|
| Initial hidden, ordinary | 143.98 | 145.40 | 325.52 → 326.77 |
| Ordinary bulk inference, model retained | 525.12 | 274.78 | 1227.02 → 906.85 |
| Ordinary repeated deep queries, retained | 524.05 | 272.49 | 1250.28 → 906.89 |
| CJK-rich bulk inference, retained | 1173.65 | 286.16 | 1889.27 → 901.66 |
| CJK-rich repeated deep queries, retained | 1172.43 | 285.33 | 1890.30 → 901.89 |

That is **1.91×** less ordinary retained footprint and **4.10×** less in this
long-chunk pair, not a uniform one-fifth result. The earlier 106.2 MiB reference
target is still missed by about **168.6 MiB** in the ordinary retained cohort;
small hidden-app RAM does not improve. WebKit/host charges vary independently of
the model, and all exploratory cohorts remain available.

Ordinary durable indexing takes **10.177 → 10.610 s**; individual stock runs are
10.383/9.971 s and patched 10.433/10.788 s. Family CPU is **2.150 → 2.808 CPU-s**
in these fresh-profile first-use phases, which include package verification and
materialization on the patched side. This is an upfront integration cost, not a
throughput/CPU win. Long-chunk indexing is **17.433 → 17.531 s**, **3.455 → 3.820
CPU-s**. No fivefold inference or whole-runtime speedup is claimed.

The accepted poll-zero policy, 8192 context/batch/ubatch, speed-mode threads,
batches and blocking supervisor are unchanged and their regressions pass.
Warm deep-burst family CPU is **0.799 → 0.609 CPU-s**; repeat bursts
**1.159 → 1.177 CPU-s**. Retained 30-second ordinary idle CPU is
**0.275 → 0.323 CPU-s**. Thus this round does not claim another general CPU win.
Foreground input-to-response-plus-two-frame p50 while indexing is
**55.0 → 55.75 ms**, simple **45.0 → 44.25 ms**, warm deep **54.0 → 52.25 ms**,
repeated deep **51.0 → 51.5 ms**. These are render-opportunity proxies, not pixels.

After 90 quiet seconds the same model PID remains in both final builds. First
visible fresh response is **34 → 34 ms**, response plus two frames **66 → 67 ms**.
The unchanged 120-second unload policy is not defeated with keepalives. Shutdown
closed all owned fixtures; **537 recorded PID/start identities** were audited
before the final receipt, with zero survivors.

## Exactness, failures and platform limits

- All 40 original ordered native signatures match in repeated interleaving.
  Sixteen final UI replies match direct native rows/scores/actions and each other
  on cloned indexes; synthetic IME composition plus hide/reopen passes.
- Q4/Q8, 2/4/8 threads and 4/8/16 inputs match every raw 768-value response and
  normalized 256-F32 payload. Fresh patched 2048/4096/8192-token and CJK batches
  match preserved stock hashes; 8193 is rejected with the same status. The
  2252/2253-token CJK inputs are not truncated. No needless stock 8 GiB allocation
  was rerun. Every ordinary desktop preserves stored-vector hash `27efcec4…399c`;
  every CJK-rich desktop preserves `5ac291cd…9ab2` (full hashes in JSON).
- Final typecheck, frontend build, **747 TS tests** (9 skipped), serialized release
  **448 Rust tests** (8 ignored), 5 package-pin tests and 16 release-policy tests
  pass. Opt-in actual packaged worker lifecycle and both actual app builds pass.
- Early worktree-local `build` profiles missed launcher files and are excluded
  from primary data. Long socket paths, locked-session frame/startup failures,
  a Desktop/TCC-denied ordinary fixture, source-path quoting and build-hook
  assertion failures are retained. Guards now reject locked visible timing and
  launcher-excluded profile paths. A native parity harness initially compared
  different temporary root strings; normalized paths then passed.
- `/tmp/tezbar-beta17-perf.e1qzXU` became absent between calls; cause is unknown.
  This executor did not delete it. Matching baseline binaries were recovered
  from Cargo outputs and all new primary evidence was regenerated into the
  ignored worktree-local directory below. Missing early raw results are not used
  for the final claims. Historical Git reports/JSON were not overwritten.
- macOS arm64 is executed/validated. macOS Intel, Windows x64/arm64 and Linux x64
  retain their upstream runtime defaults. Their patched builds/execution and the
  optional Developer ID signing/notarization path were **not executed**. Policy
  fixtures are not cross-platform execution evidence.

## Artifacts and external final step

All paths below are inside this worktree, under ignored
`build/performance-runtime-20261030/`:

- `binaries/final-ordinary-4`, `binaries/final-desktop-4`, `binaries/final-native-tests-4`
- `Tezbar-ordinary-final-4.app`, `Tezbar-benchmark-final-4.app`
- `thin-visible-{stock,patched}-{1,2}/`, `thin-visible-long-{stock,patched}/`
- `first-use-1/`, `exact-*`, `bounds-*`, `ordinary-upgrade-final-4/`
- `ui-parity-opening-final-4.json`, `ui-retention-final-4.json`, `native-search-final-4.json`

The reproducible staged resource is
`build/embedding-runtime/resources/embedding-runtime.zip`; source/build receipts
also remain in ignored `build/p/runtime-rebuild-3/` and the automatic assembly
folders. [The JSON receipt](beta17-runtime-package-results.json) pins binaries,
source, payloads and raw evidence. Reproduce measurements with the existing
memory/first-use/retention scripts, using **new output names and default OS temp
profiles**, then run `summarize-runtime-package.py`.

Only parent-approved distribution preparation remains: approve/use the real
Developer ID credentials with the provided signing recipe, review the newly
signed ZIP/catalog hashes, stage/rebuild/revalidate, then perform the reviewed
multi-platform app signing/notarization/publication procedure. No remote runtime
asset URL is needed. No push, PR, remote build, merge, signing-credential access,
workflow dispatch or publication was performed. The unmet fivefold/one-fifth goals
are not a reason to weaken input, integrity or platform guarantees.
