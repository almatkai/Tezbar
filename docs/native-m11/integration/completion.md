# M11 local integration — safe boundary for the root M9 repair patch

Implementation is linked and headlessly exercised in `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-gpui-app`, unchanged HEAD/base `05133210b78f125b83ffdf2482e09e72533ff219`. Changes are unstaged/uncommitted. No Cargo task is running. This is **not M9/M11 acceptance, a release, or production/platform verification**. The root's forthcoming M9 review-repair patch remains pending; preserve these M11 migration/updater quit fences when composing it.

The machine-readable [report](report.json) contains every gate command, exit code, elapsed time, exact raw-log SHA256 and before/after input result. The final [source freeze](final-source-freeze.json) covers 597 first-party, build, resource, reference and release-note inputs. Aggregate SHA256 is `9c440cef4e9df817636d94d9c8cba03dbcc628f2987f7aad89ed0154c8a3b8eb`. [Owned file hashes](owned-files.json) cover 40 changed/new integration files. All 34 historical M11 owned documents/reference files still match the original source audit.

## Actual checks

| Gate | Result | Receipt |
| --- | --- | --- |
| Native normal / bench, Rust 1.98.1 locked/offline, jobs/test threads 2 | 391 / 393 passed, zero failed/ignored | [normal](matched-native.json), [bench](matched-bench.json) |
| Actual real Bun SDK scenarios | all nine passed through production worker, NativeCore, TestHost and RuntimeView | matched-native.log, `real_sdk::` cases |
| UI / models | 57 / 380 passed | [UI](matched-ui.json), [models](matched-model.json) |
| Portable updater | 40 passed, one existing live-GitHub test intentionally ignored | [updater](final-updater.json) |
| Host / selected core Host port / worker units | 45 / 12 / 3 passed | host.json, core-host-port.json, core-worker.json |
| Existing production worker smoke | passed using the existing private prepared bundle | worker-smoke.json |
| Typecheck / TypeScript | passed; 745 passed, seven existing skips | typecheck.json, typescript.json |
| Rust 1.85 core/host/update/model all-features check | passed | final-msrv-check.json |
| Rust 1.85 models / updater | 380 / 40 passed; updater live-network test remains ignored | msrv-model.json, final-msrv-updater.json |
| Native check / debug build | passed | matched-check.json, matched-build.json |
| Node packaging / locked NSIS contract | seven passed; includes actual dummy-byte unsealed assembly and changed-resource rejection | final-packaging.json |
| Existing release/signing policy suite | 20 passed; two real-certificate tests explicitly skipped | final-release-policy.json |
| Lock drift / diff whitespace / owned new Rust formatting | passed | final-lock-drift.json, matched-diff.json, owned-format.json |

Unchanged TS/worker/core component evidence is reconciled in [component reuse](component-reuse.json); it is not relabeled as execution at a different source. Earlier failed compile/packaging attempts remain in the raw logs and their RED receipts. Exploratory checks predating the gate runner are clearly separated under `exploratory/`; they are not frozen-gate proof.

[Case mapping](case-map.json) accounts for the original 25 draft fixtures: 24 executed successfully and the one Windows-only case remains unrun. That original inventory comprises nineteen Mac Rust cases, one Windows-only Rust case and five Node cases, rather than 25 executable Mac Rust tests. New fixtures additionally cover atomic no-replace bootstrap, existing native defaults/corruption, real SQLite/WAL immutability, actual first-launch commit, native resource provenance/tamper, signed artifact decline/cancel/stale generations, separate nonce domains, exact queue capacity, channel durability cancellation, updater quiescence and the real preview actor loop. The portable updater exercises a captured local HTTP signed download, lazy client reuse, Ready/reset and channel retry. No live release-network loop was run.

## Integrated behavior

[Ports and ownership](ports-and-ownership.md) documents the exact APIs and finite effects. Launcher remains the one UiState owner. Migration precedes hydration and admission of native writes; missing/damaged/unsupported transport enters recovery, existing valid native state always wins, and import marker plus data commit atomically without replacing a concurrent native file. WAL recovery touches only a bounded private copy and proves the originals' byte hashes unchanged. Recovery offers retry, quit and an explicit two-step fresh-state choice that cannot overwrite native data.

The one updater Actor uses RuntimeOwner, bounded commands/effects, retained watch status and one main-thread receiver. Existing policy, parser, channel/signature and staging authority stay in tezbar-update. The portable crate now owns one reusable lazy HTTP client. Status reaches Launcher, task indicator and both open/new Settings; timestamp writes have one UiState effect owner. Channel saves use the M7 durability barrier, with a separate reply domain. Quit first quiesces the actor and fences completion effects, then runs the retained save/drain/quit path. Declined quit returns the verified artifact and resumes the actor. Cancel/failure generations cannot restore stale artifacts. macOS keeps backup/rollback ownership; Windows argv remains `/P /R /UPDATE /ARGS` followed by each escaped current argument, including a genuine empty argument only when present.

Production capability admission is linked to the same App. It verifies the original pin, outer/helper Team IDs, exact designated requirement, bundle metadata and hashed resources before production environment/path/updater access. Dirty or bench builds cannot be admitted. Existing data/config overrides are consulted afterward. Production tray uses the core-owned configured launcher hotkey; preview retains Alt+F19 opt-in and stays isolated. Windows production admission is deliberately blocked pending the platform gates.

The native build path is usable through `pnpm native:check`, `pnpm native:build` and explicit `scripts/build-release.mjs --app native --local-preview --resources <staged resource root> --output <fresh OS-temp child>`. No actual preview bundle was built or GUI-launched here: assembly tests used disposable dummy bytes. Native distributed release assembly remains gated, and the normal pnpm/Tauri product is unchanged. [Concrete cutover patch](final-cutover-review.patch) and [inventory](final-cutover-inventory.json) remain unapplied and pin 282 relocations plus 52 binary copy operations, retaining historical TS oracle sources/prototype evidence and exact signing/catalog/icon bytes.

[Lock/license audit](lock-license.json): the only registry addition is approved `snap = 1.1.1`, BSD-3-Clause, checksum `1b6b67fb9a61334225b5b790716f609cd58395f895b3fe8b328786812a40bc3b`. The cached archive was verified, no fetch was needed. App reuses rusqlite 0.32.1 with default features off and the existing bundled engine. Existing sha2 was moved from app dev to normal use for resource hashing. GPUI pin, toolchains, public versions, feeds and release history remain unchanged.

## Artifact and remaining acceptance

The private debug preview binary is `/tmp/tezbar-m11-integration/tezbar-preview`, SHA256 `fd0a0aacd8e000b0bdfd12e3640dc9a9626741d28f0f843a80f8f265425403d4`, 220,649,352 bytes. [Binary metadata](binary.json) records source `0513321`, dirty=true, compiler commit, exact GPUI revision, runtime shaders and bench-control=false. Its only execution was pre-App `--version`. macOS `otool -L` and the normal Cargo tree contain no WebKit/JavaScriptCore/Tauri/Wry/Tao. This is Mac evidence only; debug binary size is not a RAM measurement.

- **O-LS:** real-format headless fixtures pass; actual disposable old macOS/Windows OS-profile discovery, WebKit variants and Windows direct Default versus EBWebView acceptance remain open. Missing directories never prove empty state.
- **O-NSIS:** locked-template contract passes; actual NSIS compilation and disposable Windows hive/installer rehearsal remain blocked. No Windows pass or alternate-provider/remote workaround is asserted.
- **O-signing/updater:** real certificate continuity, signed old-to-native update/relaunch and platform acceptance were not run. No production certificate, updater secret, installed beta, real profile, credential, Keychain or clipboard was accessed.
- **M9/root repair:** root reported 15 actionable review defects, including two High. Root owns the focused patch and requested tests. This work preserves the relevant M9 worker/Host/RuntimeView/forms/transport sources and adds no acceptance claim.
- **Manual/cutover:** GUI, IME, AX, GPU, fresh matched Opus review and full matched RAM remain open. No source cutover, product removal, public version/channel selection, installation, signing/publication, push, commit, PR or Actions was performed.

Source/Cargo work stops at this handoff until the root delivers its exact repair patch. The later release inventory/signing/publication scope remains governed by the existing release-guidelines skill and macOS signing policy.
