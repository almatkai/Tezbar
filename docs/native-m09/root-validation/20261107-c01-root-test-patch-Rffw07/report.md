# M9 root validation: safe boundary

Validated worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-gpui-app`. HEAD remains `c01fa313fb04b5b53a97780c03ce2cb1838bb352`; no index/history/branch mutation. **Final results apply to that commit plus the exact root-authored test patch, dirty. They are not clean-PIN or clean-build results.**

The original full c01 library run is preserved as RED: 405 passed, one failed, zero ignored. `retained_store_runtime_ack_source_fence_and_settings_route` still assumed `drain_extensions` destroyed its active session. At a verified boundary with no Cargo/compiler, only root's authorized one-file patch was applied. Patch SHA256: `d3b390164b70563cfc9cab0a789eedfef4b51522480168f4c05d0a5987ebfff0`. It asserts two prior disposals and retained session-104 after quiescence, invokes actual final `dispose_extensions`, drains it and retains the original three-session disposal assertion. Source before/after hashes and exact patch are included. No other implementation, expectation, skip, manifest, lock or format change was made.

| Gate | Actual result | Evidence |
|---|---|---|
| Patched full native normal, runtime-shaders | 406 passed, 0 failed/ignored | patched-native-normal.json/.log |
| Patched full native bench | 408 passed, 0 failed/ignored | patched-native-bench.json/.log |
| Root regressions / original real SDK scenarios | 15 / 9 pass in **each** full suite | test-counts.json; full raw logs |
| UI | 57 passed | ui.json/.log |
| Models, all features, Rust 1.98.1 / 1.85.0 | 381 / 381 passed | models-198.json; models-185.json |
| Core worker framing/startup units / typed Host routes | 3 / 12 passed | core-worker.json; core-host.json |
| Core status boundary / explicit launch validation | 1 / 1 passed | core-status.json; core-launch.json |
| Explicit real Rust↔Bun worker lifecycle | 1 passed, run explicitly despite its pre-existing ignored annotation | core-worker-integration.json/.log |
| Host | 45 passed | host.json/.log |
| Typecheck / full TypeScript | passed / 746 passed, 7 existing skips | typecheck.json; typescript.json |
| Actual production worker smoke | passed with **new private** prepared bundle and mocked desktop/AI boundary | worker-smoke.json/.log |
| Date cross-realm/local calendar | Asia/Almaty covered by full suites; America/Los_Angeles targeted test passed | patched-date-los-angeles.json/.log |
| Native normal check / debug build | passed, bench-control=false | patched-native-check.json; patched-native-build.json |
| Rust 1.85.0 core/host/update/model all-features check | passed | msrv-check.json/.log |
| Tauri compatibility check | passed, locked/offline; sandbox denied all checkout writes and network | tauri-compatibility.json/.log; tauri-readonly.sb |
| Scoped Rustfmt / diff | 26 owned Rust files pass / source diff passes | patched-format-results.json; patched-owned-diff-check.json |
| Unchanged M11 packaging / updater | reused 7 packaging passes / 40 updater passes, one existing live-network ignore | m11-preservation-and-reuse.json; unchanged-components.json |

The UI/model/core/Host/TS/worker executions completed on clean c01 before applying root's test-only correction. Their complete component sources are unchanged by that one test-file patch; `patched-component-reuse.json` records this explicitly. They are not relabelled as post-patch reruns. Native normal/bench/check/build and MSRV/compatibility receipts are from the patched dirty freeze. Core was never run unfiltered: no login shell, credential or actual PTY/ConPTY exercise. Unchanged asset/history/PTY/protocol component evidence is scoped in the reuse manifests; the worker/core boundaries changed by root were actually tested again.

## Source and artifact provenance

All 608 root frozen input hashes and all 39 root-owned hashes matched initially. The original review matched SHA256 `f7206c9e0e270758c40589481eab1bc0ed13d1e30575bcda3fc13e01c04296f4`; all 43 root repair logs and 105 existing M11 integration evidence files were hash verified and preserved. Original root aggregate `c44dfaa97ae5e846dd20142d5ae15e4a5d06267905c20548488b142c1c53dadd` is recorded as supplied; individual hashes were verified. A distinct sorted-JSON aggregation was used for this run's manifests.

The patched freeze covers 608 inputs, SHA256 `4aaab379936783b63057dd566881b10a8518c098f6007e985c4fc0ba4925896d`. The broader before/after inventory hashes 2691 tracked and source/resource/test files, including detection of untracked source additions; final aggregate is `7489620ceef8fcf88e34c1e4e7143a27178f6158ac5b5e2de0f3db829230f84a`. Every command receipt records verified before/after source identity. `compiled-firstparty-source-manifest.json` records 365 actual first-party dep-info inputs across 43 freshly compiled workspace artifacts, including the Tauri check. Warm third-party dependency objects are not claimed rebuilt. The complete input manifests and worker's entire bundled input graph are retained.

Cargo used only owned APFS-cloned private targets, locked/offline, jobs=2 and test threads=2, with explicit Rust 1.98.1 or 1.85.0 and Bun `/opt/homebrew/bin/bun`. No incremental directory or writeback symlink was copied. Workspace fingerprints/build outputs were invalidated before first Cargo; the App's were invalidated again after the exact patch. Redundant stale copied workspace objects were never executed and cannot satisfy the removed fingerprints. Root cache size/mtime/inode inventory of 312893 nonincremental entries is unchanged; large raw inventories remain in the owned temporary directory with exact hashes in `external-artifacts.json`. Git index bytes, main/secondary Cargo locks, pnpm lock, toolchain and manifests are unchanged. Dependency/license deltas remain those already audited at 9230bcc; `lock-license.json` pins the inherited receipt and locks.

Preserved debug executable: `/tmp/tezbar-m9-c01-validation-Rffw07/tezbar-debug-preview`. SHA256 `5e57d2c5432674b2731d5e798e2367d6a31b6c09eb54ccf928a4332cd9bafd5e`. Its sole execution was pre-App `--version`. Metadata reports source c01, dirty=true, rustc commit `48a229ceaefd4985c50990b14116b6d856af0985`, GPUI `c84af5b99cbc366d675064e52b23664a015b6e35`, runtime shaders=true, bench-control=false. `otool -L` and the normal Cargo dependency tree contain no WebKit/JavaScriptCore/Tauri/Wry/Tao. The successful **separate** compatibility check does compile legacy Tauri dependencies and does not change the normal executable's graph. Binary size is not RAM evidence.

Fresh worker bundle: `/var/folders/7v/_tm0w_59693180wdf89zpwmh0000gn/T/tezbar-m09-fixtures-d06CfD`. All 23 bundled source inputs, 22 first-party graph inputs and all 40 generated/resource file hashes were captured; resources remain unchanged. No shared dist-backend was generated or edited. The rejecting-host no-view test and actual positive in-flight Preferences event passed, as did effect sequencing/no replay. Workers were launched only by explicit private fixtures, never App startup. Lifecycle and final process receipts verify owned workers are gone.

## Preserved failures and limits

Original c01 RED log, historical root failures, and raw whitespace are retained. The broad diff check reports whitespace only in preserved raw logs; scoped owned source diff passes. An initial aggregate-serialization assertion and a private format harness `FileExistsError` are identified as harness failures; the former preceded Cargo, and the latter was corrected only in the private harness and rerun. No source change was used to hide either. All gate/log SHA256 values are indexed. The historical global Rustfmt baseline is 49 files; one owned file (`app/m8_tests.rs`) appears in that historical list, and all 26 current scoped Rust checks pass. Global formatting was not mass-fixed or rerun.

M11 migration/state admission, updater actor, quiescence/resume, eight-second quit budget and root extension admission/teardown remain integrated. Default legacy Host controls remain no-op and SDK tray capability false; the legacy app compiles with those defaults. MenuBarExtra's intentional background owner remains; manual source-zero/background-control acceptance is open.

**This is a headless verification safe boundary, not M9/M11/full-migration acceptance.** Windows remains blocked before these gates; no provider/account/CLI/native or remote fallback was attempted. Physical GUI/IME/AX/GPU/RAM, actual old-profile discovery, native cutover, signed updater/installer/relaunch and signing acceptance remain open. New M810 R1–R7 root drafts are outside this tree and were not included; fresh M11 Opus review was rate limited and is not claimed. Tauri remains the default product. No subagent, push/PR/merge/release/Actions/install/signing, user profile/data, live provider, credential, clipboard or Keychain action occurred. Only fake test effects were exercised.

All builds/gates finished before these documents were copied into the repository. No Cargo/compiler or owned worker/App process remains. Root owns subsequent implementation and any later M810 integration.
