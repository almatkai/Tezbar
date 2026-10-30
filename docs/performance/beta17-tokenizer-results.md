# Beta.17 native tokenizer allocation audit (R13)

**No runtime change qualified.** Two same-model BPE storage candidates preserve
exhaustive vocabulary/API outputs but increase retained physical footprint in
fresh P/C/C/P cohorts. Both remain rejected, archived experiments. Production
source, runtime ZIP/catalog and ordinary/benchmark apps are unchanged. The uniform
5× runtime/search and one-fifth whole-family RAM targets remain unmet.
[Compact results and hashed receipts](beta17-tokenizer-results.json).

Work ran only in `Tezbar-performance`, branch
`fix/beta17-runtime-search-memory`, starting at
`6dbb2875f5486c1ceeabdcbe19a604703f5c9f6a`. No subagents, worktree/branch changes,
renderer experiment, model change, context reduction or residency-policy change.
XERJ had no indexed match; `rg` fallback required no shared-service intervention.

## Actual native allocation owners

The actual Q4 GGUF uses **BPE / `gemma4`**, not SPM/UGM. It has 262,144 tokens,
514,906 unique BPE rules and no precompiled character map. A numeric-only diagnostic
counts allocator requests and `malloc_size` for live node/bucket allocations,
string heap storage and vector backing allocations in the pinned source. The
unchanged control and diagnostic use the same AppleClang 21/CMake 3.31.6 recipe.
Only libllama changes; server/common/ggml/Metal remain official bytes.

| Vocabulary owner | Allocator bytes, MiB | Additional finding |
|---|---:|---|
| BPE nodes + buckets | **51.08** | 514,907 live allocations; 41.64 MiB requested |
| BPE strings outside their objects | .070 | 4,617,565 logical bytes mostly fit inside owning string objects |
| Token→ID nodes + buckets | **15.22** | 262,145 allocations; 15.14 MiB requested |
| Token-data vector backing | **12.00** | 262,144 elements; malloc backing size, not a RAM charge |
| Token-text heap / lookup-key heap | .169 / .169 | Only 5,160 long token strings |
| Piece string objects + string heap | **6.00 + .143** | 2,039,001 logical piece bytes |
| Tokenizer constructor | 80 bytes | No substantial independent normalizer allocation |

Thus the dominant measured container is the **BPE rule table**, while separately
allocated duplicate text is small. The candidate choice follows that measurement.
These allocator sizes must **not be summed into RSS or footprint**. A before/after
load snapshot includes temporary GGUF data and allocator reuse; the server loads
and releases one model/context before constructing the retained one.

Numeric checkpoints cover before vocabulary loading, after BPE construction,
after ID lookup/tokenizer/piece construction, metadata release, context creation
and inference. For the retained load, releasing model-loader metadata reduces
malloc bytes in use by about **20.4 MiB**; it does not establish immediate physical
recovery. Context creation reserves about **70.4 MiB** more malloc storage, much
of which is untouched capacity. The diagnostic has no vocabulary/input text dump.
External samples and vmmap/footprint snapshots cover ready, inference and quiet
states. Diagnostic timings are excluded from qualification.

## Implemented candidates and fresh-process qualification

**A: flat immutable BPE table.** Own all merge bytes in one arena; use compact
entries and an open-addressed slot array, with full byte equality on collisions.
Keep first duplicate rank, malformed/empty-pair behavior, sparse-rank merge export
and independent ownership after GGUF release. Guard loading to
Gemma-embedding2 + `gemma4`; oversized compact offsets retain the upstream path.
Token-data strings, token lookup, piece cache and numerical kernels are unchanged.

**B: pooled standard BPE table.** Keep owning `std::string` keys, the original
hash/equality and rank behavior. Reserve buckets and allocate nodes in a
vocabulary-owned `std::pmr::monotonic_buffer_resource`; destroy keys before freeing
the arena. The same narrow architecture/tokenizer guard selects it. This tests
allocator layout without changing lookup semantics.

Each candidate has an independent **P/C/C/P** cohort: two fresh processes per
build, identical archived Q4 inputs, 4 threads, poll=0, 8192 context/batch/ubatch,
4/8/16-input batches and three 12-second quiet phases. No build, test or diagnostic
runs overlap timing. No forced GC, allocator purge, pressure generator, cache flush,
early unload or Metal/kernel change occurs. Every successful run has identical
768-value responses, normalized 256-F32 bytes, token counts and usage.

| Cohort | Model retained footprint per process, MiB | Final RSS median, MiB | Ready median, ms | Warm short query median, ms | 16-input batch median, ms |
|---|---|---:|---:|---:|---:|
| A control | 104.27 / 104.55 | 372.74 | 3058 | 6.65 | 272.05 |
| A flat table | **139.00 / 138.49** | 346.14 | 2798 | **8.45** | 272.56 |
| B control | 105.86 / 104.31 | 373.50 | 3022 | 7.00 | 272.70 |
| B pooled table | **160.83 / 161.84** | 368.70 | 3126 | 6.97 | 272.33 |

A's retained footprint rises **104.41→138.74 MiB (+32.9%)** and its warm short
query median rises **27.1%**. B rises **105.09→161.34 MiB (+53.5%)**. Neither is
qualified. First query medians remain about 15.6–15.7 ms; all individual timings
and startup observations, including the initial 13.18 s control outlier, remain
in raw receipts. These small warm-cache cohorts are not cold/tail guarantees.

The first A cohort pair has **34 MiB dirty + 61 MiB reclaimable MALLOC_SMALL**
in control, versus **55 MiB dirty and no reclaimable MALLOC_SMALL** in A; large
malloc dirty also rises **59→72 MiB**. Both have about **5.48 MiB untagged VM**
and **.98 MiB stack**, with smaller library/graphics/allocator/page-table charges.
The roughly **184 MiB clean mapped model pages are not a RAM floor**.
For B, small/large dirty are about **54/95 MiB**. In the separate pilot snapshots,
A's vmmap allocation count drops
roughly **819k→303k**, and allocated bytes **185.6→156.2 MiB**, yet its footprint
increases. Container compaction does not prove reduced charged memory on this
allocation lifecycle. The snapshots establish the changed ledger categories;
they do not identify every retained allocator page's source object.

A fresh unchanged source-built control also returns identical vectors and settles
to **104.28 MiB**. Its Mach-O code/data match the current packaged library exactly;
only filename-derived ad-hoc signature bytes/LC_CODE_SIGNATURE differ. This rules
out a changed compiler/code generation as the reason for the candidate results.

## Exactness, ABI and limits

The new [C/C++ parity probe](tokenizer-parity.cpp) streams comparisons through
SHA-256 without writing model/token text. Current, A and B independently produce:
`601b02f2f29feebe34c90bef5903327d5970f931456710869ad6e060affb3650`.

- Exhaustive 262,144 token IDs: owned text, C text, text→ID lookup, float score,
  attributes, EOG/control status and cached piece references. Both special flags,
  three lstrip values, required lengths and undersized output buffers match.
- All 514,906 exported BPE ranks/merge strings and 10,000 deterministic random
  pair attempts match. All 256 byte-lookup outcomes include upstream exceptions.
- 441 text cases × four add-special/parse-special combinations cover multilingual,
  CJK, whitespace, Unicode composition, emoji, every byte, embedded NUL, random
  bytes, actual control tokens, BOS/EOS and 2048/4096/8192/8193 tokenization inputs.
  Detokenization and four concurrent readers match; public text/data/piece
  references retain their addresses. No C API symbol is removed.

The ABI audit keeps public headers/`llama_vocab` layout and all C/C++ method
signatures intact; only the private implementation gains storage. A owns its
arena bytes without GGUF/string views; B owns keys and destroys its map before
its arena. Token-text mutation and special-token iteration are unchanged. The
hybriddna/other tokenizer paths do not enter either new loader branch. This is
source/ABI audit plus actual Q4 parity, **not a certification of other models**.

Both candidates fail the early memory gate. Q8/all-speed/8192 **inference**,
new ordered-search/192-vector app parity, 90-second retention and complete-family
app acceptance were therefore **not run for them**, and no package catalog was
updated. The tokenization boundary checks above do not substitute for those gates.
No stock 8-GiB boundary experiment was repeated.

## Restoration, evidence and remaining gap

- **752 TS tests / 9 skipped**, typecheck, frontend build, **5 package-pin** and
  **16 release-policy** checks pass. No generated tracked worker bundle changes.
- **573 production source pins**, **84 model exactness artifacts**, 13 prior
  receipts, Q4/Q8 installed inventories, current ZIP and four R7 frozen app
  inventories match. **450 Rust / 8 ignored and 40 ordered native signatures**
  are explicitly reused by unchanged-source/binary hash proof, not rerun claims.
- **32 sampled runtime/supervisor identities audited; zero survivors.** Final
  parity subprocesses returned zero and were reaped. No clipboard access or
  writes, real profile/model edits, installed-app replacement or unowned kills.
- Early harness failures are labelled: an over-broad alias glob, one nonexistent
  build-library filename, byte-lookup exception handling and detokenization's
  upper-bound length assumption. Corrected probes pass on current and candidates.
  Provisional vocabulary dumps were removed from this task's artifacts; their
  hashes/failure states remain, and the final probe only emits a digest.

All new material is under **`build/performance-tokenizer-20261030/`**. The manifest
pins rejected patches/libraries, pinned source archive/toolchain receipts, exact
runtime file inventories, probes, allocation checkpoints, raw P/C/C/P samples,
vmmap/footprint/pressure context and cleanup/reuse receipts. Performance fixtures
replace only the loaded versioned libllama aliases; the unused bare alias stays
current. Dependency pins and loaded vmmap paths identify the executed library;
the C/C++ parity fixtures replace all aliases. No production override is added.

Reproduce with new output names using the archived `build-compact.py` /
`build-pooled.py`, `instrument-final.py`, `run-vocab-final.py` and
`run-native-cohort.py`; `freeze-audit.py` verifies the result. The current ZIP is
preserved separately as `pre-fix-runtime.zip`. Historical R5–R12 evidence and the
unrelated HTML are unchanged.

The concrete blocker is a **physical-footprint increase despite lower allocation
count/volume**, with lost reclaimable small regions; adopting either patch would
regress the requested RAM metric. This is not an architectural lower bound.
No new accepted source/runtime fix, package build or full-app comparison is claimed.
R7's retained family remains **490.45→376.69 MiB**, far above the **98.09 MiB**
one-fifth target; broad native search and several app workloads still miss 5×.
No push, PR, remote build, merge, signing-credential access, publication or deploy.
