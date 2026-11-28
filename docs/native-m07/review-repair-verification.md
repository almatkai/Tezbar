# M7 parent repair validation

Evidence directory: `/tmp/tezbar-native-m7-review-repairs`.
Source changes are confined to M7, its shared write queue, the shared pin marker,
and the M6 historical-evidence labels. Original receipts are historical, unchanged
unless explicitly labelled in M6 documentation; they are not relabelled as current.

- Final normal native: **232 passed**, zero failures/ignores (`native-final-v4.log`).
- Final bench: **233 passed**, zero failures/ignores (`bench-final-v3.log`).
- Shared UI: **51 passed**; combined models: **379 passed** (`ui-models-final.log`).
- Rust 1.85 models: **379 passed** (`msrv-models-final.log`).
- Final check/build passed (`check-final-v3.log`, `build-final-v3.log`).
- Scoped fmt and diff check passed. All **281 tracked Rust/build-input hashes**
  match the final freeze; no manifest or lock changed.
- Normal dependency tree has no Tauri/Wry/Tao; executable links no WebKit.

Exact paths, log and binary hashes, frozen inputs and build metadata are in
[review-repair-receipt.json](review-repair-receipt.json). The preserved executable
is a `9d0c910` dirty preview containing these frozen source changes; it is not
relabelled as a clean-source build.

No new skips. Baseline operational-close assertion changed only to reflect the
N-2 repair, with an additional no-replay assertion; see the repair ledger.

Full Rust core, TypeScript and Windows suites were not repeated for this M7-only
repair. Existing core/TS evidence remains scoped to its earlier source. The shared
queue changed here is exercised by both toolchain model suites and native callers.
No app was installed/launched. No live service, GUI, OS IME/AX or memory result.
Fresh independent Opus acceptance remains pending.
