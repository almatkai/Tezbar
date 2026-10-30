# Beta.17 R14 follow-up: independent review of the pass-1 qualification (Opus pass 2)

**Outcome: the R14 joined-BM25 production change (`cb3a74b` gate, macOS arm64 +
bundled SQLite 3.46.0 only) stays as it is. Pass 2 found no production regression
that needs a fix. It found overstatements in pass 1's causal and RAM conclusions
and a gap in the receipt coverage, and corrects them.** In a new matched 6+6
cohort the WebContent RSS separation that pass 1 saw (4/4 vs 0/4) does not
reproduce. The host transient shows up in **both** builds, only in windows with
physical input, and is owned by `main` + `NSEventThread`. This pass does not
show what caused R14's historical windows. **Uniform 5× speed / one-fifth RAM
remain unmet.** Compact data: [beta17-opus-pass2-results.json](beta17-opus-pass2-results.json).

Worktree `Tezbar-performance`, branch `fix/beta17-runtime-search-memory`, start
`467adce`. No product source change (the `cb3a74b` binaries are the current
production code: `git diff cb3a74b -- src-tauri src package.json pnpm-lock.yaml
vite.config.ts scripts index.html` is empty). No subagents. XERJ was not used
(supplied shared-breaker history); `rg` only. New raw evidence is only in
`build/performance-opus-pass2-20261030/` (71 files and 618 source/harness pins in
`artifact-manifest.json`). R5–R14 and pass-1 raw data are not modified.

## Parent fixes carried in (not redone)

- `3d73c58`: `summarize-quiet.contaminated` now fails closed on empty windows,
  missing, non-finite or negative HID values or timestamps, and unordered
  timestamps (4 tests).
- `467adce`: the post-search range wording was corrected to "overlap, slightly
  higher maximum". Idle cohorts are 4/3 and burst cohorts 4/4.
- The pass-1 manifest (253 raw files and 11 pins) predates those edits. Its
  summarize-quiet hash is the pre-`3d73c58` file. That delta is expected.

## Findings (severity ordered)

| # | Severity | Finding | Status |
|---|---|---|---|
| 1 | High (claim) | Pass 1 states as facts that the R14 transient "is WindowServer event delivery", that the deferred-host hypothesis is "falsified", and that RSS is "not retained allocation / not build-caused". None of these is established (A, C). | Corrected here and in the pass-1 report header. No raw data altered. |
| 2 | Medium (measurement) | The receipt used `t + 0.05 s` as the bound on when the HID value was read, and had no guard before the window. Measured delay from `t` to the HID read reaches **140 ms** under load, and input just before the window counted as quiet (B). | Fixed: timed in-process receipt, 2 s guard, callsite tests. Pass-1 classes are unchanged (margins ≥ 638 s). |
| 3 | Medium (provenance) | "Inventories differ only in `Contents/MacOS/tezbar`" cannot prove the renderer is identical, because Tauri embeds the frontend in that binary (D). | Closed: a runtime asset fingerprint, a rebuild from current source and binary SQL strings all match. |
| 4 | Low | The launcher's `open_reader`/`init` returns an error if `tezbar_bm25` registration fails on a 3.46.0 engine, where Knowledge falls back to built-in BM25. This was already true in R14 and is not new. Registration can only fail when FTS5 is absent, and the schema needs FTS5 anyway. | Documented. No change. |

## A. Causal claim strength

Reanalysis of existing raw data (`trajectory.py`, `pass1-trajectory.json`, `r14-trajectory.json`):

- In R14, the elevated candidate windows (`1b`: 295 ms/875 wakeups; `2c`:
  249 ms/992 wakeups) are sustained runs of 10–20 ms per 0.55 s step from about
  0.5–2 s to about 10–12 s. R14 recorded no thread or HID data, so these windows
  have **no known owner and no input receipt**.
- In pass 1's own windows *with* receipted input, input cost the host little.
  `desktop-candidate-4` had physical input in almost every step for 24 s
  (post-search), yet the host used 87 ms/540 wakeups versus 77–83 ms quiet.
  Across its four windows, input added about **+16 ms `main` and +8 ms
  `NSEventThread` per 30 s**. `hid-C1` (input in 89 of 113 quarter-seconds)
  showed only the periodic writer ticks. That is an order of magnitude below
  R14's +170–210 ms, so input explaining R14's magnitude is **not supported**.
- In pass 1's unreceipted probes, elevated windows occur in both binaries: R7
  baseline 2/14, R14 candidate 4/13. Adding R14: baseline 3/17, candidate
  6/17, Fisher p≈0.4. The rates cannot be told apart.
- **New focused measurement** (`post-search-receipts.py`, matched frozen apps,
  order ABBA×3, `--model`, 12/12 completed). Quiet windows (no input from 2 s
  before the window to the end): control 4, candidate 3. **No transient in any
  of the 7 quiet windows.** Both transients (wakeups > 550, rule set before
  the run) are in input windows, one per build: `rx-control-3` 122 ms/579 and
  `rx-candidate-4` 113 ms/613. At 0.25 s resolution both are owned by `main` +
  `NSEventThread`. A third input window (`rx-candidate-2`) shows the same
  owners at a smaller size. Quiet-window `main` is 1.8–2.4 ms and
  `NSEventThread` ≈0.

**Bounded conclusion:** in this pass's data, `main`/`NSEventThread` work in a
hidden window appears together with physical input, in both builds, at
+25–50 ms per window. A build-specific deferred host regression after search is
**not reproduced** in 8 quiet candidate post-search windows (3 pass-1 desktop,
2 pass-1 HID probes, 3 here; controls: 4 matched pass-1, 3 R7-baseline HID probes, 4 here, also none). An
intermittent cause at the unreceipted rate (≈0.3) would still give zero hits in
8 windows with p≈0.06, so this absence is moderate, not conclusive, evidence.
The cause of R14's larger and longer historical transients **remains unknown**.
They are not attributed to input or to WindowServer.

Quiet post-search cohort values (pass 2):

- Host: 73.9 vs 75.0 ms, wakeups 506.5 vs 507, p=0.54.
- Family: 136.3 vs 148.3 ms (131.8–144.2 vs 147.9–151.7, p=0.057, n=4/3). The
  difference is WebContent idle CPU (33–46 vs 43–55 ms). It is reported as a
  small open descriptive difference, not a claim.

## B. Quiet-receipt validity

What the source guarantees: `HIDIdleTime` is IOHIDSystem's time since the last
HID event, and also since events posted at the HID tap. Tezbar's
`native_input.rs` posts there only for paste/typing actions, which are absent
in idle windows. Reading it is read-only and shows no event content. A HID-quiet
window does **not** mean WindowServer was quiet: other apps' rendering and
display or Space changes are not covered.

Coverage holes in the pass-1 rule, and how they are fixed (`hid_receipt.py`):

1. *Skew.* The sample's `t` was taken before `processes()`, the window queries
   and `ioreg`. The delay from `t` to the read was measured at up to **140 ms**
   under load (smoke run). `t + 0.05 s` is therefore an assumption, not a bound.
   New receipts take monotonic timestamps immediately before and after the read
   (`pre`/`post`). The latest input is bounded by `post − idle`, and `pre ≥ t`
   is enforced.
2. *Prior input.* Input just before the window counted as quiet. A 2 s guard
   (the settle period) is now required: `quiet` / `pre_window_input` /
   `in_window_input`.
3. *Tail.* Covered: the last sample's receipt is read after its CPU counters,
   and the latest-input bound covers the whole window.
4. *Sampler overhead.* `ioreg` was a separate ~6–13 ms process per sample,
   outside the app family. The IOKit read is in-process, takes 0.07 ms
   (max 0.24 ms), and returns the same property (agrees with `ioreg` within its
   own ~4 ms latency).

Tests (`hid_receipt_test.py`, 8 tests):

- A red-capable slow-read case and a guard-band case that the pass-1 rule
  accepts as quiet.
- Fail-closed cases: post before pre, read before the sample, unreadable value,
  NaN, unordered.
- A legacy-equivalence case and a live read.
- A sampler callsite test (`post-search-receipts.instrument`).

All 30 Python harness tests pass. Pass-1 windows reclassified with a 2 s guard
and a 0.15 s skew (`pass1-receipt-reclassification.json`): **no change**. Every
quiet window had no HID event for ≥ 638 s before and through the window.
candidate-4 stays as input in all four phases. No run or window was dropped.

## C. RAM

Reading the vmmap columns: `RESIDENT` counts mapped pages in memory; `DIRTY`
counts modified, non-reusable pages. In every receipt the WebKit Malloc zone
has **BYTES ALLOCATED ≈ DIRTY** (within a few MiB). So the dirty size tracks
live libpas allocations, and `resident − dirty` is non-dirty residency. That is
consistent with pages the libpas scavenger has decommitted (madvise reusable)
but the kernel has not taken back. vmmap does not label those pages, so it is
not proof that every such page was freed. They count in RSS but not in physical
footprint.

- **Where pass 1's RSS gap arose** (`phase-webcontent.json`): during
  `simple_broad_bursts`, WebContent RSS grew +56/+57/+59/+2 MiB in the candidate
  and +3/0/+2/+1 in the control. This happened while dispatched requests (14),
  obsolete replies (2) and rows (1120) were identical. Pass-1 unreceipted
  probes (R7 vs R14): growth ≥ 40 MiB in 1/10 vs 8/12. Taken alone, this
  looked build-caused, contrary to pass 1's "within-build variance".
- **Matched replication (pass 2, 6+6):** broad growth ≥ 36 MiB in **2/6 control
  vs 2/6 candidate**.

  | WebContent metric, control vs candidate | Median | p |
  |---|---|---|
  | Post-search RSS | 307.6 vs 313.5 MiB | 0.94 |
  | Max footprint during broad bursts | 391.7 vs 386.4 MiB | 0.83 |
  | vmmap lifetime peak footprint | 417.5 vs 394.2 MiB | 0.65 |
  | WebKit Malloc resident | 263.8 vs 269.4 MiB | 0.95 |

  Live heap after search is bimodal in both builds: about 39–45 MiB or about
  135–250 MiB. The high mode appears in control 4/6 and candidate 2/6, so
  dirty and allocated medians are *lower* in the candidate (67 vs 145 MiB,
  p=0.18). Replies were the same size (rows and serialized lengths identical
  in all 12 runs). Ordered digests differ between runs even within a build,
  which is the known HashMap tie order, unchanged.
- **Qualification:**
  - No material retained-memory regression is established. Live allocations
    and footprint are not higher in the candidate in matched data.
  - The WebContent residency increase is not established as build-caused
    either. Matched pooled counts: candidate 5/10 vs control 2/10 high-growth,
    p≈0.35.
  - The R7→R14 probe imbalance is unexplained. Those builds also differ in the
    stale worker bundle and it is not a matched comparison.
  - No RAM win is claimed. No pressure, purge, GC or unloading was used.
  - Footprint sums are ledger sums and RSS sums double-count; 0.5 s sampling
    misses peaks.

## D. Provenance and gates

- Frozen pass-1 apps were re-hashed before use (`pin-check-before.json`): 3×41
  files equal to `apps-sha256.json`. The bench apps differ only in
  `Contents/MacOS/tezbar`. Executables:
  - candidate ordinary `162a627c…`
  - candidate bench `29d8c44d…`
  - control bench `09cbdd92…`
- **Frontend identity** (the frontend is embedded in the binary):
  - The WebView-loaded `index.html` and `main-D7GZZIQ8.js`/`main-FxlyANJc.css`
    have the same SHA-256 in all 12 runs of both apps.
  - They equal `dist/`, and a fresh `vite build` of the current source into a
    temporary directory is byte-identical to `dist/` (23 files).
- **Control patch:** `control-test-only.patch` changes only `QUALIFIED_TARGET`
  to `false`. It applies forward to HEAD and not in reverse, so it was restored.
  The control binary does **not contain** the joined launcher SQL string (const
  dead code removed). Candidate and ordinary contain both SQL strings. Only the
  bench apps contain `TEZBAR_BENCH_HOME`/`__desktop_bench_reply`.
- **Gate:**
  - `supported() = QUALIFIED_TARGET && version == 3.46.0`. Registration is
    gated on the engine version only.
  - Knowledge uses `register().is_ok() && supported()`. Launcher selection uses
    `supported()` (finding 4).
  - Future SQLite or non-arm64 targets use built-in BM25. The pinned-version
    test fails deliberately on an engine bump.
  - All 7 strict oracle and callsite tests pass on the existing Rust 1.85
    release binary `app_lib-febb4269…` (SHA `bfa0c4f2…`). The tests were not
    weakened. No recompile was needed.
- **Overlap:** pass-1 builds and tests ended (1793359969) before its first
  desktop run, and parity, retention, suite and typecheck came after the last
  one. The rust185 focused tests overlapped an app *build*, not a timed run.
  Pass-2 probes ran with no build or test running. A host-wide load of 3–6
  (other users' processes, not touched) and WindowServer CPU were recorded per
  run; values are similar across arms.
- Shipping-profile coverage remains local macOS arm64 only.

## Reproduce

```
python3 docs/performance/post-search-receipts.py --app <app> --output <new dir> \
  --label control|candidate --model "<installed>/embeddinggemma-2-q4_k_m"
python3 build/performance-opus-pass2-20261030/drive.py          # ABBA×3, label→app asserted
python3 docs/performance/summarize-receipts.py --root build/performance-opus-pass2-20261030 \
  --runs rx-control-1 … rx-control-6 rx-candidate-1 … --output receipts-summary.json
python3 -m unittest discover -s docs/performance -p '*_test.py'
build/performance-opus-pass2-20261030/{trajectory,ram,phase-wc,probe-wc}.py  # read-only reanalysis
```

## Identities and cleanup

- 12 runs plus 1 smoke run (diagnostic, excluded): **250 PID/start identities,
  zero alive, zero recorded survivors**.
- Fixture clipboard stores were scrubbed by the harness without being read.
- Installed model inventory (88 files: size, mtime, inode) is unchanged.
- The unrelated HTML is unchanged (SHA-256 `1ee1b074…`).
- No push, PR, remote action, signing, installed-app replacement or deploy.

## Remaining open

- Uniform 5× runtime/search and one-fifth RAM: unmet.
- The cause of R14's historical long host transients is unknown.
- The WebContent post-search residency is bimodal with no attributed trigger.
- The small quiet WebContent idle-CPU difference (n=4/3) is unresolved.
- Windows and Intel are not qualified (the joined rank is off there).
