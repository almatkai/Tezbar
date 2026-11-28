# Native launcher matched experiment — 2026-11-05

Artifact source: **`7434ba1701d60395bbae510a50c80c4046ebb205`**, clean when A/B were built.
Measurement harness: **`0c00d26ce3ffe6cb7348fad02b20d573f03c2c5c`**. Branch: `feat/native-launcher-prototype`.
Last native Rust implementation: `8ef180046d751a21a7d7a481036d1300a5fcdaab`.

**The original strict-idle gate FAILED. The separately authorized background-idle matrix completed 20/20 valid runs.** Every additional result explicitly records `protocol:background-idle`, `strict_cpu_gate_passed:false` and the reason for the protocol deviation. This is a completed prototype experiment with a qualified decision, not a completed frontend migration or a strict-idle pass. Parent review and independent recomputation are complete; this report retains the production and platform limits below.

## Decision

Proceed with a **launcher-only production-feasibility phase**, rather than approving a broad frontend migration. In this matched synthetic workload, every B footprint sample was below every A footprint sample in each state. The centres below are medians of three run medians, in MiB:

| State | A: Tauri | B: GPUI | Observed difference |
|---|---:|---:|---:|
| hidden | 141.27 | 58.86 | 82.41 |
| visible | 244.35 | 67.69 | 176.66 |
| retained | 1262.22 | 65.81 | 1196.41 |

Visible weighted CPU also separates: A run means 3.144–3.203% of one core, B 1.018–1.060%. Hidden means are lower in B here, but hidden A/B interval peaks remain substantial; retained CPU means overlap. B input echo is faster in these synthetic observations, while result latency is query-dependent. OS-observed reopening was slower in B (n=3). The data do not support a blanket “native is faster” claim.

The large retained-memory difference merits follow-up, but its cause is not isolated. Equalize or bound A's retained icon/thumbnail/cache policy in a follow-up variant before attributing the difference solely to renderer technology. The observed process-role breakdown is below; it does not identify the contents or causal ownership of individual heap allocations.

### Service and build differences accompanying the numbers

| Service/property | A | B | C |
|---|---|---|---|
| Same NativeCore source, app/file scans, watcher, refresh, clipboard observer, background Knowledge | enabled | enabled | enabled |
| Tray / fixture F19 global hotkey / updater startup | enabled | absent | absent |
| WebView | created | not created | not created |
| GPUI renderer | absent | Metal on Mac / DirectX on Windows | not constructed |
| Tauri/WebKit linked on Mac | yes | yes, without WebView | yes, without WebView |
| Hide lifecycle | core trim after 5 s; resolved icon cache retained | core trim after 5 s; icons and GPU textures released | never shown |
| Rust compiler | 1.98.1, commit `48a229ceaefd4985c50990b14116b6d856af0985` | same | same |
| Shared dependency graph | main lock | standalone lock; shared futures 0.3.32 → 0.3.34 | same as B |

The core dependency watchlist is unchanged, but the complete dependency graphs are not identical; see [measured-7434ba1-lock-drift.json](../../prototypes/native-launcher/evidence/measured-7434ba1-lock-drift.json). Mac B is a runtime-Metal-shader local-preview build. No Apple cache was reset. The first-ever M0 shader launch was never measured. Runtime compiler helpers can still occur after earlier launches and are included when responsibility-owned. These differences, UI event delivery and cache lifetime prevent a causal renderer-only or production-wide savings claim.

## Protocol and validity

The original 120 s warmup + 30 stable CPU intervals (mean <2%, p95 <5%) could not settle for headless C. The first attempt exhausted its 15-minute stabilization allowance. A second attempt retained raw data but was invalidated by **clamshell sleep at 19:37:49–20:01:05 local time**. An independently recomputed pre-sleep warmup (115 samples, 114 intervals, 119.16 s observed span) has weighted CPU 0.868%, interval p95 7.548%, peak 9.404%; periodic peaks occur near the core's normal 10 s refresh cadence. This explains why a low mean is not a strict-gate pass. The initial timeout predates the raw-on-failure repair and lacks its full raw stabilization series; this evidence gap is retained explicitly, not reconstructed.

The parent authorized the **separate background-idle protocol** after independently examining the periodic C activity. It keeps the real services enabled and uses 120 s warmup, 30 diagnostic CPU intervals and 60 s requested sampling, without a p95<5 prerequisite. The raw phase name `stabilize` is retained for the diagnostic intervals, but it does not assert stabilization in this protocol.

Valid run order is C-before, then **A B B A A B** independently for hidden, visible and retained, then C-after. Each run uses a fresh byte-identical marked fixture: seed 65001, 20,000 generated files; actual matched index counts are applications 235, files 14,997, notes 50, snippets 79 (including built-ins), quick links 20, commands 10 and native commands 41. The same 22 staged resource-file hashes and fixture-manifest bytes match across all 20 runs. A's fresh build receipt binds source, binary, compiler, frontend/dist and resources; B's signed copy is version/compiler-checked after staging. Input and actual measured executable hashes are separate.

All Mac processes launch via LaunchServices and the root must own its responsibility domain. Accounting sums physical footprint and Mach-timebase user+system CPU for the root, descendants and responsibility-owned XPC, including PPID 1 helpers. RSS is secondary and can double-count shared residency. Sampling is approximately 1 Hz; very short-lived helpers can escape observation. Startup discovery is sampled separately at 250 ms; absence in samples is not proof a service never existed. WindowServer/DWM are outside this app-owned accounting.

Hidden preparation is show once → hide, with no explicit query workload. Visible is show → `doc`; A waits two frames and search settles during warmup, B waits for its generation's frame. Retained adds one instrumented reopening and the six fixed burst/70 ms character query workloads, then hide. Both wait for OS visibility before warmup. Visibility is checked at every sample; actual focus loss auto-hides the launchers. No continuous focus forcing was used. New timed samples record wall clock and reject sleep/clock discontinuities. The original OS visibility helper excludes tiny windows using at least 480×80 bounds; opening polling is requested every 2 ms, not guaranteed photon timing.

### Rejected attempts and repairs

No invalid or smoke values enter the tables/chart. All failed attempts remain inspectable:

- Original strict C timeout: [matched-7434ba1-20261105/00-C-hidden.json](../../prototypes/native-launcher/evidence/matched-7434ba1-20261105/00-C-hidden.json); interrupted strict repeat: [matched-7434ba1-harness-c9213ca-20261105/00-C-hidden.json](../../prototypes/native-launcher/evidence/matched-7434ba1-harness-c9213ca-20261105/00-C-hidden.json), plus [measured-7434ba1-power-events.log](../../prototypes/native-launcher/evidence/measured-7434ba1-power-events.log) and [measured-7434ba1-interruption.json](../../prototypes/native-launcher/evidence/measured-7434ba1-interruption.json).
- Initial background A05 became unexpectedly visible after about 6 s warmup. B06 was deliberately interrupted to repeat A05/B06 in the successful prescribed order. Neither is selected.
- Three B08 visible attempts lost visibility after about 66 s, 60 s and 1 s warmup. Cause was not established. A separate full diagnostic B-visible passed; a read-only AppKit observer with a running run loop confirmed the prototype foreground PID in its observed interval. The diagnostic is excluded. An earlier diagnostic failed from an overlong socket path; its first observer omitted a run loop and its cached foreground readings are not used to assign cause.
- Three A10 attempts failed **AF_UNIX path too long** before sampling: the orchestration retry prefix crossed the Unix limit at a two-digit index. The driver now uses short marked task prefixes and asserts a <104-byte control path before launch. This was a driver setup failure, not GUI interference.

The selected ledger is [matched-background-retry3-7434ba1-0c00d26-20261105/session.json](../../prototypes/native-launcher/evidence/matched-background-retry3-7434ba1-0c00d26-20261105/session.json). Resumers copy completed valid rows unchanged with origin hashes and run the unchanged committed measurement CLI for the remainder; their exact source and SHA are recorded in the ledger. Invalid attempts remain in the initial/background-retry1/retry2 directories. Selection uses validity and prescribed order, never a better performance value.

Local harness commits after artifact freeze: `c9213ca` preserves raw samples on failure and allows remaining attempts; `d59cf0f` rejects sleep/clock discontinuities; `0c00d26` adds the separately authorized protocol and CPU interval statistics. No native Rust, production UI, main Cargo.lock or core seam changed in this measurement task.

## macOS background-idle measurements

Physical footprint is primary; RSS is secondary. All values below use valid full background-idle runs. `strict_cpu_gate_passed:false` in every run. CPU is percent of one core, including all owned helpers.

| Run | Mode/state | Footprint min / median / max MiB | RSS min / median / max MiB | CPU weighted mean % | Interval CPU p95 / peak % |
|---|---|---:|---:|---:|---:|
| 00 | C hidden | 12.31 / 12.31 / 12.31 | 52.36 / 52.36 / 52.36 | 0.889 | 7.651 / 9.277 |
| 01 | A hidden | 133.13 / 133.57 / 133.85 | 330.69 / 331.20 / 331.52 | 1.573 | 14.313 / 15.117 |
| 02 | B hidden | 57.56 / 69.13 / 69.64 | 75.52 / 75.55 / 122.62 | 0.866 | 7.608 / 9.295 |
| 03 | B hidden | 43.45 / 43.45 / 43.45 | 85.53 / 85.53 / 85.53 | 0.868 | 7.611 / 8.951 |
| 04 | A hidden | 141.19 / 141.27 / 141.44 | 273.19 / 273.73 / 274.33 | 1.084 | 6.519 / 11.430 |
| 05 | A hidden | 146.10 / 146.17 / 150.94 | 257.38 / 258.08 / 263.23 | 0.942 | 6.164 / 7.744 |
| 06 | B hidden | 58.38 / 58.86 / 58.86 | 81.08 / 81.21 / 123.00 | 0.804 | 7.010 / 7.124 |
| 07 | A visible | 234.79 / 234.83 / 235.08 | 328.97 / 329.03 / 329.11 | 3.185 | 7.953 / 9.967 |
| 08 | B visible | 76.45 / 76.52 / 76.56 | 137.89 / 137.95 / 138.00 | 1.060 | 6.235 / 6.323 |
| 09 | B visible | 65.05 / 65.05 / 65.05 | 129.84 / 129.84 / 129.84 | 1.057 | 6.355 / 6.591 |
| 10 | A visible | 244.27 / 244.35 / 244.38 | 250.33 / 251.23 / 252.11 | 3.203 | 8.280 / 9.454 |
| 11 | A visible | 244.52 / 244.85 / 244.96 | 202.98 / 205.36 / 206.92 | 3.144 | 7.806 / 9.650 |
| 12 | B visible | 67.69 / 67.69 / 67.69 | 89.61 / 89.61 / 89.61 | 1.018 | 6.223 / 7.611 |
| 13 | A retained | 1261.96 / 1262.22 / 1346.74 | 810.00 / 810.03 / 810.11 | 0.830 | 6.500 / 7.102 |
| 14 | B retained | 65.81 / 65.81 / 65.81 | 145.00 / 145.00 / 145.00 | 0.872 | 7.840 / 8.719 |
| 15 | B retained | 65.33 / 70.25 / 70.25 | 145.52 / 145.52 / 145.52 | 0.782 | 7.079 / 8.360 |
| 16 | A retained | 1240.96 / 1240.96 / 1325.47 | 848.34 / 848.34 / 848.36 | 0.816 | 6.097 / 6.591 |
| 17 | A retained | 1263.49 / 1263.52 / 1348.04 | 895.06 / 895.09 / 895.12 | 0.849 | 6.359 / 7.701 |
| 18 | B retained | 65.70 / 65.70 / 65.70 | 145.88 / 145.88 / 145.88 | 0.848 | 7.441 / 8.905 |
| 19 | C hidden | 12.58 / 12.59 / 12.78 | 49.81 / 49.81 / 49.81 | 0.855 | 7.706 / 8.687 |

### Repeated-run ranges

Each centre is the median of three run medians. Full ranges span every measured sample across all three runs; they are not confidence intervals. The run-median range shows between-run variation.

| State | Mode | Median MiB | All-sample min–max MiB | Run-median min–max MiB | CPU mean median (run range) % |
|---|---|---:|---:|---:|---:|
| hidden | A | 141.27 | 133.13–150.94 | 133.57–146.17 | 1.084 (0.942–1.573) |
| hidden | B | 58.86 | 43.45–69.64 | 43.45–69.13 | 0.866 (0.804–0.868) |
| visible | A | 244.35 | 234.79–244.96 | 234.83–244.85 | 3.185 (3.144–3.203) |
| visible | B | 67.69 | 65.05–76.56 | 65.05–76.52 | 1.057 (1.018–1.060) |
| retained | A | 1262.22 | 1240.96–1348.04 | 1240.96–1263.52 | 0.830 (0.816–0.849) |
| retained | B | 65.81 | 65.33–70.25 | 65.70–70.25 | 0.848 (0.782–0.872) |

### Latency, with repetition 0 discarded

Each mode/style has 162 observations (3 runs × 9 retained repetitions × 6 queries). Quantiles use the median and nearest-rank p95. A echo is inputToTwoFramesMs; A results is inputToCorrectDomObservedMs. B echo/results end at content plus the following frame callback. A IPC and B core timing have different boundaries and are not pure engine comparisons.

| Mode | Style | Echo p50 / p95 / max ms | Results p50 / p95 / max ms | IPC (A) or core (B) p50 / p95 / max ms |
|---|---|---:|---:|---:|
| A | burst | 53.50 / 88.00 / 105.00 | 54.00 / 88.00 / 105.00 | 6.00 / 9.00 / 13.00 |
| A | characters | 45.00 / 66.00 / 83.00 | 45.00 / 66.00 / 83.00 | 6.00 / 8.00 / 10.00 |
| B | burst | 20.80 / 23.43 / 26.39 | 46.99 / 59.29 / 62.18 | 3.42 / 4.07 / 4.41 |
| B | characters | 24.50 / 28.12 / 31.12 | 48.62 / 53.29 / 55.56 | 3.64 / 4.33 / 4.94 |

Per-query results p95 (27 observations per cell):

| Query | Style | A DOM-observed p95 ms | B post-render-frame p95 ms |
|---|---|---:|---:|
| `do` | burst | 105.00 | 59.29 |
| `do` | characters | 60.00 | 52.21 |
| `doc` | burst | 88.00 | 60.14 |
| `doc` | characters | 49.00 | 55.27 |
| `Calculator` | burst | 47.00 | 57.87 |
| `Calculator` | characters | 53.00 | 51.43 |
| `document-001` | burst | 74.00 | 59.34 |
| `document-001` | characters | 69.00 | 53.67 |
| `Orchard project meeting` | burst | 55.00 | 57.03 |
| `Orchard project meeting` | characters | 36.00 | 51.59 |
| `zz-no-match-65001` | burst | 49.00 | 48.51 |
| `zz-no-match-65001` | characters | 48.00 | 51.48 |

### Instrumented reopening observations

One reopening per retained run (n=3 per mode), after an earlier show. These include control/socket/frame or CoreGraphics polling overhead; they are not initial launch or photon time. ready_ms uses different readiness boundaries in A and B and is not compared.

| Mode | Control-frame receipt p50 / p95 / max ms | OS on-screen poll p50 / p95 / max ms |
|---|---:|---:|
| A | 43.97 / 58.93 / 58.93 | 9.84 / 19.91 / 19.91 |
| B | 33.83 / 33.90 / 33.90 | 30.39 / 31.00 / 31.00 |

### First non-empty workload observation after reopening (warmup diagnostic)

These are `do`/burst, repetition 0, one per retained run. They are excluded from the percentile tables above and are not a cold launch or first-ever search measurement.

| Run | Mode | Echo ms | Results ms | IPC (A) / core (B) ms |
|---|---|---:|---:|---:|
| 13 | A | 54.00 | 55.00 | 5.00 |
| 14 | B | 22.39 | 61.78 | 4.99 |
| 15 | B | 30.21 | 67.90 | 4.78 |
| 16 | A | 45.00 | 45.00 | 6.00 |
| 17 | A | 53.00 | 53.00 | 8.00 |
| 18 | B | 26.91 | 58.61 | 4.74 |

B stale-drop counters are cumulative; post-warmup deltas are run 14: 0, run 15: 0, run 18: 0. A’s `obsoleteCompleted` is a per-observation harness field; its raw values are retained and are not summed against B’s cumulative counter as an equivalent metric.

## Windows absolute prototype observations

One B visible run and one C headless run; PrivateUsage (commit) is primary, WorkingSet secondary. These are separate Windows absolute observations, with no A transport, A/B ratio or cross-platform memory comparison. Both use 120 s warmup and a full requested 60 s collector window; additional diagnostic CPU intervals do not assert the macOS strict gate.

| Mode | PrivateUsage min / median / max MiB | WorkingSet min / median / max MiB | CPU weighted mean % | Interval CPU p95 / peak % |
|---|---:|---:|---:|---:|
| B | 84.41 / 84.49 / 148.32 | 70.87 / 72.91 / 163.79 | 3.077 | 13.638 / 51.382 |
| C | 9.39 / 9.48 / 74.11 | 24.45 / 24.54 / 116.64 | 2.435 | 12.045 / 70.713 |

Windows helper-role PrivateUsage (min / median / max MiB, including zero when absent):

| Mode | Role | MiB |
|---|---|---:|
| B | `conhost` | 0.00 / 0.00 / 1.22 |
| B | `powershell` | 0.00 / 0.00 / 59.27 |
| B | `tezbar-launcher-prototype-release` | 84.41 / 84.49 / 87.87 |
| C | `conhost` | 0.00 / 0.00 / 1.11 |
| C | `powershell` | 0.00 / 0.00 / 60.16 |
| C | `tezbar-launcher-prototype-release` | 9.39 / 9.48 / 12.88 |

## macOS role detail

Per-role physical footprint min / median / max MiB. A role contributes zero in samples where absent; totals above include all observed processes. Raw series retain PID, parent, responsibility, start identity, CPU and RSS for independent process-level recomputation.

| Run | Role | MiB |
|---|---|---:|
| 00 | `host` | 12.31 / 12.31 / 12.31 |
| 01 | `com.apple.SafariPlatformSupport.Helper` | 9.81 / 9.81 / 9.81 |
| 01 | `com.apple.WebKit.GPU` | 14.67 / 14.70 / 14.70 |
| 01 | `com.apple.WebKit.Networking` | 5.97 / 5.98 / 5.98 |
| 01 | `com.apple.WebKit.WebContent` | 64.64 / 64.67 / 64.94 |
| 01 | `com.apple.audio.SandboxHelper` | 2.95 / 2.95 / 2.95 |
| 01 | `host` | 35.06 / 35.45 / 35.45 |
| 02 | `MTLCompilerService` | 13.53 / 13.53 / 13.53 |
| 02 | `host` | 44.03 / 55.60 / 56.11 |
| 03 | `host` | 43.45 / 43.45 / 43.45 |
| 04 | `com.apple.SafariPlatformSupport.Helper` | 12.27 / 12.27 / 12.27 |
| 04 | `com.apple.WebKit.GPU` | 16.88 / 16.88 / 16.89 |
| 04 | `com.apple.WebKit.Networking` | 6.88 / 6.91 / 6.98 |
| 04 | `com.apple.WebKit.WebContent` | 66.99 / 66.99 / 67.02 |
| 04 | `com.apple.audio.SandboxHelper` | 2.94 / 2.94 / 2.94 |
| 04 | `host` | 35.24 / 35.28 / 35.38 |
| 05 | `com.apple.SafariPlatformSupport.Helper` | 12.47 / 12.47 / 12.53 |
| 05 | `com.apple.WebKit.GPU` | 16.80 / 16.80 / 16.83 |
| 05 | `com.apple.WebKit.Networking` | 6.98 / 6.98 / 7.00 |
| 05 | `com.apple.WebKit.WebContent` | 69.70 / 69.74 / 74.50 |
| 05 | `com.apple.audio.SandboxHelper` | 2.97 / 2.97 / 2.97 |
| 05 | `host` | 37.11 / 37.19 / 37.28 |
| 06 | `MTLCompilerService` | 13.67 / 13.67 / 13.67 |
| 06 | `host` | 44.70 / 45.19 / 45.19 |
| 07 | `com.apple.SafariPlatformSupport.Helper` | 9.72 / 9.72 / 9.72 |
| 07 | `com.apple.WebKit.GPU` | 88.67 / 88.70 / 88.95 |
| 07 | `com.apple.WebKit.Networking` | 5.81 / 5.81 / 5.81 |
| 07 | `com.apple.WebKit.WebContent` | 85.99 / 85.99 / 86.02 |
| 07 | `com.apple.audio.SandboxHelper` | 2.95 / 2.95 / 2.95 |
| 07 | `host` | 41.61 / 41.66 / 41.67 |
| 08 | `com.apple.SafariPlatformSupport.Helper` | 9.72 / 9.78 / 9.83 |
| 08 | `host` | 66.74 / 66.74 / 66.74 |
| 09 | `com.apple.SafariPlatformSupport.Helper` | 9.72 / 9.72 / 9.72 |
| 09 | `host` | 55.33 / 55.33 / 55.33 |
| 10 | `com.apple.SafariPlatformSupport.Helper` | 12.06 / 12.06 / 12.06 |
| 10 | `com.apple.WebKit.GPU` | 91.63 / 91.63 / 91.64 |
| 10 | `com.apple.WebKit.Networking` | 7.05 / 7.05 / 7.05 |
| 10 | `com.apple.WebKit.WebContent` | 91.39 / 91.45 / 91.49 |
| 10 | `com.apple.audio.SandboxHelper` | 2.95 / 2.95 / 2.95 |
| 10 | `host` | 39.19 / 39.20 / 39.20 |
| 11 | `com.apple.SafariPlatformSupport.Helper` | 11.98 / 11.98 / 11.98 |
| 11 | `com.apple.WebKit.GPU` | 92.27 / 92.30 / 92.38 |
| 11 | `com.apple.WebKit.Networking` | 6.91 / 6.91 / 6.91 |
| 11 | `com.apple.WebKit.WebContent` | 91.28 / 91.35 / 91.41 |
| 11 | `com.apple.audio.SandboxHelper` | 2.94 / 2.94 / 2.94 |
| 11 | `host` | 39.06 / 39.39 / 39.41 |
| 12 | `com.apple.SafariPlatformSupport.Helper` | 12.11 / 12.11 / 12.11 |
| 12 | `host` | 55.58 / 55.58 / 55.58 |
| 13 | `com.apple.SafariPlatformSupport.Helper` | 12.09 / 12.09 / 12.09 |
| 13 | `com.apple.WebKit.GPU` | 20.69 / 20.95 / 97.94 |
| 13 | `com.apple.WebKit.Networking` | 6.63 / 6.63 / 6.63 |
| 13 | `com.apple.WebKit.WebContent` | 1172.39 / 1172.39 / 1179.94 |
| 13 | `com.apple.audio.SandboxHelper` | 2.98 / 2.98 / 2.98 |
| 13 | `host` | 47.17 / 47.17 / 47.17 |
| 14 | `com.apple.SafariPlatformSupport.Helper` | 9.72 / 9.72 / 9.72 |
| 14 | `host` | 56.09 / 56.09 / 56.09 |
| 15 | `com.apple.SafariPlatformSupport.Helper` | 9.75 / 9.75 / 9.75 |
| 15 | `host` | 55.58 / 60.50 / 60.50 |
| 16 | `com.apple.SafariPlatformSupport.Helper` | 11.05 / 11.05 / 11.05 |
| 16 | `com.apple.WebKit.GPU` | 18.66 / 18.66 / 95.63 |
| 16 | `com.apple.WebKit.Networking` | 6.06 / 6.06 / 6.06 |
| 16 | `com.apple.WebKit.WebContent` | 1153.28 / 1153.28 / 1160.83 |
| 16 | `com.apple.audio.SandboxHelper` | 2.97 / 2.97 / 2.97 |
| 16 | `host` | 48.94 / 48.94 / 48.94 |
| 17 | `com.apple.SafariPlatformSupport.Helper` | 9.64 / 9.64 / 9.64 |
| 17 | `com.apple.WebKit.GPU` | 18.02 / 18.05 / 95.02 |
| 17 | `com.apple.WebKit.Networking` | 6.00 / 6.00 / 6.00 |
| 17 | `com.apple.WebKit.WebContent` | 1177.83 / 1177.83 / 1185.38 |
| 17 | `com.apple.audio.SandboxHelper` | 2.95 / 2.95 / 2.95 |
| 17 | `host` | 49.05 / 49.05 / 49.06 |
| 18 | `com.apple.SafariPlatformSupport.Helper` | 9.78 / 9.78 / 9.78 |
| 18 | `host` | 55.92 / 55.92 / 55.92 |
| 19 | `host` | 12.58 / 12.59 / 12.78 |

## Core floor, mappings and host state

C-before/after physical-footprint medians are **12.31 / 12.59 MiB**, an absolute drift of about 0.28 MiB; weighted CPU is 0.889 / 0.855%. Subtracting the midpoint C floor from A/B is only an estimate of additional host/UI/services cost, not causal savings from a migration. The per-state estimates are:

| State | A minus C midpoint MiB | B minus C midpoint MiB |
|---|---:|---:|
| hidden | 128.82 | 46.41 |
| visible | 231.89 | 55.24 |
| retained | 1249.77 | 53.36 |

All 20 runs captured `footprint`, `vmmap -summary` and filtered WebKit mappings successfully for every PID still owned in the final sample (all recorded return codes 0). [webkit-image-mappings-7434ba1.json](../../prototypes/native-launcher/evidence/webkit-image-mappings-7434ba1.json) parses the rounded vmmap region columns. For C-before/after, the main WebKit image has approximately 15.34 / 15.45 MiB resident and 0 dirty MiB; the WebKit family has 46.14 / 45.97 MiB resident and about 0.0073 dirty MiB. Much of this is shared-cache residency, not extra private footprint; these numbers must not be subtracted as recoverable RAM. No WebView is constructed in B/C. The preserved region logs allow each other run to be inspected.

Host: macOS 26.3 (25D2125), arm64 Mac17,9, 24 GiB RAM, 15 logical CPUs. Battery power throughout the recorded endpoints: preflight 81%, postflight 48%. Swap used changes from 2740.56 to 2580.56 MiB. Per-run power/swap and 15-second top-process CPU/RSS/name observations are retained in raw results and [background-7434ba1-host-load.jsonl](../../prototypes/native-launcher/evidence/background-7434ba1-host-load.jsonl). These are ordinary host conditions, not a controlled AC-powered laboratory run.

Installed `/Applications/Tezbar.app` was **not running at initial preflight; running PID 61547 at final audit**. Its binary SHA-256 stayed `665d9733f72efdb4bf43f3e5b599617f7b6031586f3227b9a769cbe6a65b596e`. No installed-app start/stop/replacement/signing operation was issued by this task; PID continuity is not claimed. This change in ambient host state is a limitation and is visible in the host observations. All task-owned Mac/Windows prototype processes are gone and the Windows one-off task is removed. Only marked task-temp clipboard JSON/images were scrubbed after owned exit, without reading or hashing them; profile stores, tokens and clipboard contents are excluded from exported evidence.

## Windows evidence and limits

The final Windows source archive is `source-7434ba1.tar.gz`, SHA-256 `24aad4f4c9c70a16db88c5b66c586c813bca6b127d2cfb44950a2a58700a4ca6`; all **1,686 tracked file hashes** were verified before build and after measurement. A new revision folder was used, with the existing private Cargo/target caches and existing VS/SDK 10.0.22621 fxc/rc. `C:\Users\User\Desktop\Tezbar` was not used. No toolchain default, SDK or elevation change was requested or performed. Final read-only audit resolves `stable` to Rust **1.98.1**, contrary to the handoff's 1.90 statement; no before/after default-version continuity is asserted. Explicit 1.98.1 compiler paths were used for the actual builds.

Current-source debug/release builds and all 7 Windows prototype tests passed (six session + BGRA; Unix authentication unit test is not built on Windows). Headless ready/query/auth-negative/quit passed, `doc` returned 80 results, files indexed 14,997, and B/C fixture manifests, resource hashes and category counts match. The raw Windows absolute measurements were independently recomputed, including PowerShell/conhost helpers, PID/start identities and CPU deltas; see [independent-validation-7434ba1.json](../../prototypes/native-launcher/evidence/independent-validation-7434ba1.json) and [windows-evidence-7434ba1.zip](../../prototypes/native-launcher/evidence/windows-evidence-7434ba1.zip).

B ran under a one-off current-user **interactive token, limited privilege, no startup/login trigger**, in console session 1. Its independent 60-second collector recorded 59 memory samples over 59.42 s and 60 visible/painted status observations. Four instrumented post-show/query frame receipts were observed. A single GPU Process Memory counter snapshot reported 19,124,224 bytes dedicated and 258,048 shared; it is not a GPU-time or DWM presentation benchmark. This is an instrumented DirectX-path run, **not human visual or IME QA**. C's 59 samples span 59.69 s after full warmup and diagnostic intervals.

The first new Windows wrapper falsely expected at least 119 samples from a 120-second ~1 Hz collector and rejected an otherwise complete warmup. It was corrected to check temporal coverage; that failed attempt remains preserved. The initially added strict CPU prerequisite also failed on Windows and is retained as diagnostic evidence. The requested Windows **absolute 120+60 protocol** was then completed separately without claiming a strict CPU pass. B's independently completed absolute collector quit the app; the original stricter wrapper subsequently rejected the missing root in its later diagnostics, and the task wrapper hit a final COM “no running instance” refresh race. Its `finally` removed the scheduled task. These wrapper errors are preserved, not called successes; the independently validated absolute series and final zero-survivor/task audit are separate evidence.

There is no Windows A transport because `desktop-bench` is Unix-only. Thus there is no Windows Tauri A/B ratio. Release HLSL uses existing fxc, debug uses runtime compilation; Mac runtime-Metal startup claims do not transfer to Windows.

## Build/test ledger

| Check | Result | Inspectable evidence |
|---|---|---|
| Mac B release, clean 7434ba1, 1.98.1, bench-control | PASS | [measured-7434ba1-B-build.log](../../prototypes/native-launcher/evidence/measured-7434ba1-B-build.log) |
| Mac A release, clean 7434ba1, same compiler, fresh Vite build/dist receipt | PASS | [measured-7434ba1-A-build.log](../../prototypes/native-launcher/evidence/measured-7434ba1-A-build.log) |
| Prototype Rust tests, cached release target | 8 passed, 0 ignored | [measured-7434ba1-rust-tests.log](../../prototypes/native-launcher/evidence/measured-7434ba1-rust-tests.log) |
| Python provenance tests | 13 passed; repeated after harness repairs | [background-protocol-python-tests.log](../../prototypes/native-launcher/evidence/background-protocol-python-tests.log) |
| Python syntax/CLI checks | PASS | [background-protocol-cli.log](../../prototypes/native-launcher/evidence/background-protocol-cli.log) |
| Frontend typecheck | PASS | [measured-7434ba1-typecheck.log](../../prototypes/native-launcher/evidence/measured-7434ba1-typecheck.log) |
| Frontend tests | 744 passed, 7 existing skipped | [measured-7434ba1-frontend-tests.log](../../prototypes/native-launcher/evidence/measured-7434ba1-frontend-tests.log) |
| Prior unchanged default / facade backend tests | 609 / 611 passed, 9 existing ignored each; inherited, not rerun here | [default-test-bounded.log](../../prototypes/native-launcher/evidence/default-test-bounded.log), [facade-test-handoff.log](../../prototypes/native-launcher/evidence/facade-test-handoff.log) |
| Prior main 1.85 default / facade checks | PASS on unchanged core/seam; inherited | [msrv-default-handoff.log](../../prototypes/native-launcher/evidence/msrv-default-handoff.log), [msrv-facade-handoff.log](../../prototypes/native-launcher/evidence/msrv-facade-handoff.log) |
| Prior Windows 1.90/1.98 backend checks + two facade tests | PASS on unchanged src-tauri; inherited | [windows-backend-repaired-host.log](../../prototypes/native-launcher/evidence/windows-backend-repaired-host.log) |
| Final Windows debug/release + prototype tests | PASS; 7 passed, 0 ignored | [measured-7434ba1-windows-build-host.log](../../prototypes/native-launcher/evidence/measured-7434ba1-windows-build-host.log), [windows-evidence-7434ba1/revision/prototype-test.log](../../prototypes/native-launcher/evidence/windows-evidence-7434ba1/revision/prototype-test.log) |
| Mac background matrix | 20/20 valid, exact count/resource parity | [background-independent-validation-7434ba1.log](../../prototypes/native-launcher/evidence/background-independent-validation-7434ba1.log) |
| Actual measured Mac bundles | 20/20 hash + deep/strict codesign checks | [measured-bundle-verification-7434ba1.json](../../prototypes/native-launcher/evidence/measured-bundle-verification-7434ba1.json) |
| Windows absolute series + cleanup | independently recomputed, zero owned survivors, task absent | [independent-validation-7434ba1.json](../../prototypes/native-launcher/evidence/independent-validation-7434ba1.json), [windows-evidence-7434ba1/revision/cleanup-audit.json](../../prototypes/native-launcher/evidence/windows-evidence-7434ba1/revision/cleanup-audit.json) |

`src-tauri` is byte-unchanged in `79821f2..7434ba1`; all post-7434 tracked code changes in this task are in the two prototype Python scripts. Main MSRV remains 1.85 and main lock SHA-256 remains `a5d44a7ff003aa70e8892768f5a9502179d6657bd7cc12215fa6ed48f9a3f728`.

## Reviews, dispositions and production gates

Immutable copies: [original Opus review](../reviews/native-launcher-opus-review-45278ff.md) and [repair recheck](../reviews/native-launcher-opus-recheck-4da6ff8.md). Their source/copy hashes are in [immutable-review-copies-7434ba1.json](../../prototypes/native-launcher/evidence/immutable-review-copies-7434ba1.json); originals were not changed. Recheck closes F1–F9 with no P0–P2 in its scope. It does not review this new measurement protocol or close production QA.

| Finding | Disposition |
|---|---|
| F1 BGRA | Closed in 8ef; native unit test passes on both platforms; parent's Safari-colour QA retained. |
| F2 GPU atlas lifetime | Closed in recheck; parent's 60-cycle hide probe is lifecycle evidence, not matched all-process memory. Replacement/LRU paths were code-reviewed, not independently stress-tested here. |
| F3 matched preparation | Equal show/hide, doc-visible and retained scripts; pre_sample_steps retained. |
| F4 explicit bundled binary | Signed copy/version/compiler rechecked; all measured hashes reverified. |
| F5 A provenance | New clean build receipt binds compiler, binary, resources and frontend. |
| F6 source identity | Artifact source 7434ba1 and harness source 0c00d26 remain separate in every selected row. |
| F7 scrolling | Nearest-edge scrolling repair retained; parent's 18-Down QA, not repeated here. |
| F8 quit | B/C control quit passed; zero owned survivors. Forced cleanup on rejected setup paths is recorded. |
| F9 concurrency/contrast | Batch-four icon lookup and text-2 empty-state repair retained; no new UI edits. |
| N1 frontend binding | Fresh build-a frontend/dist receipt verified on every A stage. |
| N2 first character | B searches first character; A filters locally. Included and labelled as latency confound. |
| N3 query labels | All 720 raw A/B latency rows carry query; 648 survive rep-0 discard. |
| N4 A visible preparation | Labelled two frames; search settles during 120 s warmup, not overclaimed as immediate fresh reply. |
| N5 exit race / hashes | Gone PID accepted safely; unsigned input and actual signed measured hashes recorded. |
| N6 negative provenance cases | 13 Python tests passed. |

Parent native QA on unchanged 8ef Rust code confirmed Safari colour, exact supplied Kazakh/Russian/emoji paste, nearest-edge Down scrolling and Escape hide. This does **not** establish Japanese/Pinyin IME, dead-key composition, full mouse selection, every light/dark state, VoiceOver or Narrator. GPUI AX exposed only the window in that QA; accessibility parity remains an important production gate. Undo/redo, Geist feature tags, active-display recentering and unsupported product surfaces remain documented prototype limits.

Before production: complete OS input/IME/accessibility/activation and monitor-placement QA; verify production signing and precompiled-shader behaviour; define the 1.85/1.98 toolchain strategy; equalize/bound cache and service variants for causal follow-up; validate the hybrid GPUI/on-demand WebView architecture for settings/extensions/Raycast surfaces. No broad Tauri port was added. No push, PR, merge, release, deployment or GitHub Actions run was performed.

## Artifact paths and reproduction

All local paths below are within the selected worktree. Original target outputs remain, and immutable input copies are retained at:

- `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent/prototypes/native-launcher/evidence/artifacts-7434ba1/mac/tezbar`
  SHA-256: `ced9c3e6a2350bf4f0c2f969991bf880080039fce3020f25fdc5f14e8a32edcf`
- `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent/prototypes/native-launcher/evidence/artifacts-7434ba1/mac/tezbar-launcher-prototype`
  SHA-256: `1fee2e183da18cdf5f2ee86d153d7dfedc0cebef2b5c7426e6747a732b644e76`
- `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent/prototypes/native-launcher/evidence/artifacts-7434ba1/mac/tezbar.build-receipt.json`
  SHA-256: `b4363a7b5efba4dfe005669aadcc58ce2243deb9f31ee2d41d089fd226c7945d`
- `/private/var/folders/7v/_tm0w_59693180wdf89zpwmh0000gn/T/tezbar-native-A17-v27aela4/Tezbar Performance.app/Contents/MacOS/tezbar`
  SHA-256: `b7d55d7981a84593d8cd0c77722ee4b9a172145450bba882693ec5974c4051f7`
- `/private/var/folders/7v/_tm0w_59693180wdf89zpwmh0000gn/T/tezbar-native-B18-5z163_68/Tezbar Launcher Prototype.app/Contents/MacOS/tezbar-launcher-prototype`
  SHA-256: `50562d648fadc0b4d2467ef79fb5454315d19914fdcaa25a6328d7808e19e306`
- `/private/var/folders/7v/_tm0w_59693180wdf89zpwmh0000gn/T/tezbar-native-C19-y0jyin4a/Tezbar Launcher Prototype.app/Contents/MacOS/tezbar-launcher-prototype`
  SHA-256: `50562d648fadc0b4d2467ef79fb5454315d19914fdcaa25a6328d7808e19e306`
- `C:\Users\User\AppData\Local\Temp\tezbar-native-launcher-91e690194efa440ca6c7a3f874856a32\revision-7434ba1\artifacts\tezbar-launcher-prototype-debug.exe`
  SHA-256: `4ba53bb4d4a27f46a09f0a2c9898cdce63a532829af401e8819ec0d12ed628af`
- `C:\Users\User\AppData\Local\Temp\tezbar-native-launcher-91e690194efa440ca6c7a3f874856a32\revision-7434ba1\artifacts\tezbar-launcher-prototype-release.exe`
  SHA-256: `4562089f182f176212e6b48d5c485642029186bbe442e81280e580fbabf6aa4d`

All 20 measured bundle paths/hashes are in [measured-bundle-verification-7434ba1.json](../../prototypes/native-launcher/evidence/measured-bundle-verification-7434ba1.json). A's adjacent receipt contains full compiler/frontend/resource hashes; B `--version` records clean source/compiler/pin. Windows archive/file manifests and exact build scripts are in [windows-evidence-7434ba1.zip](../../prototypes/native-launcher/evidence/windows-evidence-7434ba1.zip). The source archive is also retained at `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent/prototypes/native-launcher/evidence/source-7434ba1.tar.gz` with the SHA above.

The exact original Mac build commands were:

```sh
CARGO_TARGET_DIR="$PWD/prototypes/native-launcher/target" CARGO_BUILD_JOBS=2 \
  cargo +1.98.1 build --manifest-path prototypes/native-launcher/Cargo.toml \
  --release --locked --features bench-control
/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/build-a.py
```

To repeat from the preserved 7434 inputs, create a fresh fixture and output directory; current receipt validation refuses changed frontend/resources:

```sh
prototype_template="$(/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/fixture.py \
  --source 7434ba1701d60395bbae510a50c80c4046ebb205)"
/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/matched.py \
  --protocol background-idle --continue-on-error --template "$prototype_template" \
  --a-binary "$PWD/prototypes/native-launcher/evidence/artifacts-7434ba1/mac/tezbar" \
  --b-binary "$PWD/prototypes/native-launcher/evidence/artifacts-7434ba1/mac/tezbar-launcher-prototype" \
  --output "$PWD/prototypes/native-launcher/evidence/matched-new-unique-output"
```

The Mac must remain open, unlocked and have uninterrupted foreground use reserved for visible/latency runs. Do not reset OS caches or force focus during sampling. Use a new output name. The original `strict-idle` protocol is still available and is expected to report its own result honestly; it is not silently replaced by background-idle.

For Windows, the exact build command/script is preserved in the archive; its initial setup intentionally refuses an already populated revision folder. Run the existing verified artifact without touching the release checkout:

```powershell
$prototypeStage = 'C:\Users\User\AppData\Local\Temp\tezbar-native-launcher-91e690194efa440ca6c7a3f874856a32'
$prototypeRevision = Join-Path $prototypeStage 'revision-7434ba1'
& "$prototypeRevision\artifacts\tezbar-launcher-prototype-release.exe" --version
```

For a headless run, use the archived `windows-measure-absolute-7434ba1.py C`, which creates a fresh marked fixture and performs auth/ready/doc/empty/warmup/diagnostic/measure/quit/clipboard-store cleanup. Preserve its named `C-absolute-result.json` before reusing that script; per-fixture series remain in unique roots. For manual functional launch, use a new marked fixture from `fixture.py --source <full SHA>`, a local random token file, the verified executable's `--headless --profile <fixture> --resources <matching resources> --control-token-file <file>`, then `scripts/control.py <fixture> status|query|quit`. B requires an available interactive session and purpose-built controls; the archived one-off scripts record this experiment's launch and PID, not a reusable hardcoded PID. No credential, elevation or global startup changes are required or authorized.

The standalone chart is [native-launcher-matched-7434ba1-20261105.html](../../prototypes/native-launcher/evidence/native-launcher-matched-7434ba1-20261105.html); HTML preview at width 728 passed with content height 1611 and no console messages. Parent presentation remains pending. [background-independent-validation-7434ba1.json](../../prototypes/native-launcher/evidence/background-independent-validation-7434ba1.json) retains derived per-run/per-role results and warmup-discarded latency rows; [run-details-index-7434ba1.json](../../prototypes/native-launcher/evidence/run-details-index-7434ba1.json) indexes safe fixture/resource manifests and VM logs. First-query warmup observations, startup process maps, all CPU stabilization/diagnostic series and failed attempts remain in raw evidence. No old smoke or empty-profile reference is used as A.

The final documentation commit, clean-branch audit, selected-run hashes, artifact hashes and build/test-log hashes are recorded in [MEASURED_HANDOFF_7434ba1_20261105.json](../../prototypes/native-launcher/evidence/MEASURED_HANDOFF_7434ba1_20261105.json).

## Parent acceptance

The parent independently recalculated all 20 selected macOS runs directly from their
raw per-process series, including footprint medians, identity-based CPU deltas,
clock continuity, warmup/sampling coverage, root responsibility, category/resource
parity and cleanup. All 36 local hashes in the measured handoff passed; copied-origin
hashes, immutable review copies and the installed binary were also checked. Both
Windows absolute series were recomputed from their raw 59-sample collectors, including
PowerShell/conhost; the exported cleanup audit has no survivors or one-off task.
Post-warmup latency grouping independently reproduced all four 162-observation cells.

Evidence: [parent-final-validation.json](../../prototypes/native-launcher/evidence/parent-final-validation.json),
[recomputation script](../../prototypes/native-launcher/evidence/parent-final-validation.py),
and [compact Russian chart](../../prototypes/native-launcher/evidence/parent-native-launcher-summary.html).
This accepts the qualified prototype experiment. It does not change the failed strict
CPU gate, prove a renderer-only cause for retained memory, or approve production migration.
