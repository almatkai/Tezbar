# LaTeX Math Symbols — beta15 host integration

## Source and scope

Vendored from `https://github.com/almatkai/latex-math-symbols`, branch
`feat/native-v2-rust`, commit **7f79829c9ba9662379ef29942f02ba9971faae3a**.
The upstream checkout was read-only. Host base: release/0.2.9-beta.15 at
`0f5147f`, integrated only in `fix/beta15-latex-search-cpu`.

Byte-exact upstream runtime inputs: `src/{lib,search}.rs`, `data/commands.json`,
`adapter/v2_latex_math_symbols.rs`, `assets/icon.png`. The adapter's historical
header identifies its original beta baseline, not the host it now links against.
It is imported by the actual host module and uses the current `common` and
`v2_sdk` types; there is no copied SDK, substitute registry or Tauri test stub.

`src/search.rs` SHA256:
`d0f910ed8ebefed5652fd0b26dc81c3d5821814e99f04569aaf910650090bf7f`.
The CPU fix caches duplicate chunks only after Bitap positions clamp to the
field length. All chunk score additions and the denominator remain unchanged.
The four bit-exact score-equivalence regressions are in that same source file.
`tests/engine.rs` is also byte-exact upstream.

The original 7,516-byte renderer icon is at
`src/renderer/public/extensions/icons/latex-math-symbols.png`, as well as
`assets/icon.png` here. SHA256:
`e245364b6c13c1c70abf5a6e9255442477639e18526f857d086b5d611ca24191`.
Vite's existing renderer root serves that public asset without new UI code.
The complete NOTICE, upstream MIT, fork MIT and Fuse Apache-2.0 texts also ship
through `src/renderer/public/extensions/licenses/latex-math-symbols/` as `.txt`
assets. Tests require byte parity with the source notices. This makes notices
available in compiled distributions without installer configuration changes.

## Deliberate adaptations

- `Cargo.toml`: trim package inclusion to the vendored files; relax the original
  exact serde 1.0.219, serde_json 1.0.140 and uuid 1.16.0 pins to compatible
  semver ranges. This retains host-locked serde **1.0.228**, serde_json
  **1.0.150**, uuid **1.23.3**; no existing host package changes or downgrades.
  The upstream standalone `Cargo.lock` is retained for standalone engine tests.
  Host builds use the host lock, which only gains this path package/dependency.
- `manifest.json`: only verification notes updated to describe remaining desktop
  QA, not imply an unperformed check passed. The host catalog appends this exact
  manifest. All original native/V1 catalog records remain intact.
- Adapt upstream `checks/adapter-tests.rs` into real host regression tests at
  `src-tauri/src/native_core/extensions/v2_latex_math_symbols_tests.rs` using
  `ExtensionsCore`, its actual `NativeSessions`, real catalog installation and
  headless CoreContext with recording effect routers. Add host CLI, slug aliases,
  dispatch/catalog parity, icon hash, V1 preservation and fallback assertions.
  No OS clipboard operation is performed in tests.
- `NOTICE` replaces references to unshipped historical host fixtures and records
  the optimized source commit. This document replaces upstream fixture-specific
  provenance/integration instructions rather than claiming those fixtures ship.

## Included licensing evidence / excluded tooling

Original data and icon derive from JeffersonDing's Raycast extension at
`raycast/extensions` commit `6734d85992290066fd31a7f5bee3cca3d3545a1a`,
`extensions/latex-math-symbols`; original MIT text is `LICENSE-MIT-UPSTREAM`.
The fork's original MIT notice (literal backslash-n bytes) is preserved as
`fixtures/own/LICENSE` (fork base `a7429d60fca9a8943c61372a73fbedade03c30c4`).
All 131 source records, including variants/escapes/examples, remain bundled.
There are no TeX fonts, rendering libraries or preview images to port.

The search algorithm is a modified translation of Fuse.js **6.6.2**, Apache-2.0,
Copyright 2022 Kiro Risk / 2017 Kirollos Risk. Retain the complete Apache license,
`NOTICE`, and byte-exact `libraries/fuse.js/{LICENSE,fuse.common.js,package.json}`
for algorithm/attribution evidence. That JavaScript is not loaded by the host.
Aggregate license: **MIT AND Apache-2.0**.

Excluded: `.work`, `target`, `node_modules`, npm tarball/oracle, old beta host
snapshots, renderer/SDK harness templates, fixture audit scripts and upstream
validation reports. They are not runtime inputs; do not ship stale harnesses
that pretend to validate the current host. Complete original provenance and
upstream parity evidence remain available at the pinned upstream commit.

## Validation

Executed on macOS arm64, rustc/cargo 1.98.0, Node 26.7.0, pnpm 11.17.0.
Rust commands used `nice -n 10`, `CARGO_BUILD_JOBS=2`, `RUST_TEST_THREADS=2`;
full builds were sequential. All checks below passed:

- `pnpm typecheck`.
- `pnpm test --maxWorkers=2 --no-file-parallelism`: **724 passed, 9 skipped**
  (110 test files passed, 2 skipped).
- `cargo check --manifest-path src-tauri/Cargo.toml --locked --offline --lib --bins`:
  actual host production types/registration, with existing dead-code warnings.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked --offline --lib native_core::extensions::`:
  **126 passed**, including 14 LaTeX adapter/host tests, new catalog regression,
  existing NativeSessions lifecycle tests and all three SDK tests. The first run
  exposed two test-fixture router assertions on the real host's harmless
  `core:launcher:reindex-extensions` request after uninstall. The recording router
  now explicitly accepts that empty request; no production workaround was added.
- Same real-host test build with filters `native_v2_cli::` and
  `native_core::common::`: **3 + 3 passed**.
- `cargo test --manifest-path src-tauri/native-extensions/latex-math-symbols/Cargo.toml --locked --offline`:
  **8 passed** (four CPU-fix bit-exact regressions, four engine integration tests).
  Standalone target artifacts stayed under the host's ignored `target/latex-engine`.
- `pnpm exec vite build --outDir /tmp/tezbar-cpu-review/beta15-frontend-dist --emptyOutDir`:
  renderer production build; original icon and all four notice files verified
  byte-for-byte in its output. Existing browserslist/third-party annotation warnings
  were not fixed as part of this task.
- Actual host draft 2020-12 manifest schema: new manifest passes; catalog entry
  matches it exactly. Validator used the approved upstream's existing read-only
  Ajv installation; no new project npm dependency was introduced.
- Byte audit: 13 selected source files remain exact, all **11** original native
  catalog entries and **98** V1 records are unchanged. Host lock changes only add
  the path crate and its reference. `git diff --check` passes.

Logs: `/tmp/tezbar-cpu-review/beta15-*` (local review artifacts, not product files).
XERJ indexing on canonical prefix `ax-865a7399102e` was stopped after the bounded
30-second timeout; current-source exploration/audits used `rg` and direct reads.
No shared service settings were modified.

Neither a standalone rebuilt CLI executable invocation, macOS desktop QA nor a
Windows build/desktop QA is claimed. The real CLI catalog functions and IPC
lifecycle are exercised inside the linked host test binary. Host MSRV 1.85 was
not rerun here; upstream's separate MSRV validation is not host certification.
Both manifest verification states remain `not-run`. No installed app, service,
release version, installer, commit, push or release operation was changed.
