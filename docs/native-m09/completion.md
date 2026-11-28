# M9 integration receipt

Store and runtime are exported, connected to the retained native Launcher/App and the one CoreClient, and all nine distinct real SDK scenarios execute through the actual production Bun worker, initialized NativeCore, TestHost and RuntimeView. Changes remain **unstaged and uncommitted**, in `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-gpui-app`, HEAD/base `db4d0df6d0d2313bec8c82429b32dd9ef607a401`. No subagents, Git history/index mutations, installations, real desktop launch or production services were used.

## Implementation and authority

The [final API description](final-api.md) covers routing, exact-once navigation acknowledgement, visibility/session/source fences, retained weak subscriptions, shared footer, M7 Settings delivery, existing dialog leases, native controls and colour format rows. CoreClient admits finite requests before polling, keeps mutations alive after waiter cancellation and drains ordinary/disposal permits. The typed desktop-effect hook intercepts clipboard, selection, open and reveal at their existing validated OS execution boundaries. TestHost handles or denies every variant, with no native fallthrough and redacted Debug.

SDK Close, Preferences, ClearSearch and HUD use finite owned core/native channels. Diagnostics are observational and never replayed. MenuBarExtra uses the existing Native V2 status boundary with core-owned session identities, real callbacks, hidden updates and retirement on rerun/dispose/disable/uninstall/shutdown/EOF. Canonical owners now preserve action sections, form defaults, client pagination, inline preference arguments and view revisions. Actual viewport admission drives bounded images; registry authority, accepted asset transport/decode/path locking, Markdown image tickets and M8 capture/quit ownership remain intact.

The nine fixtures assert list/grid paging and selection, actual 1×1 image decode, Detail metadata, all form kinds/defaults and corrected submission, inline preferences with original arguments, native action controls, full 10,010-character clipboard payloads, 60 action waves/rolling 50 diagnostics without replay, confirms, HUD/close, real hidden status callbacks/updates, bounded reruns, disable/uninstall cleanup and coalesced invalidation. TestHost executes no actual clipboard, opener, selection, tray, capture or dialog operation.

## Actual gates

All Rust gates use locked/offline dependency resolution, `-j2`, and test threads 2. Rust 1.98.1 native gates use `runtime-shaders`; no desktop binary was executed. Commands, elapsed time and before/after input hashes are in the reports.

| Gate | Actual result | Evidence |
| --- | --- | --- |
| Full normal native | 360 passed, 0 failed/ignored; all 9 real SDK scenarios passed | [freeze 06 gates](gates/frozen-gates-06.json) |
| Full bench native | 362 passed, 0 failed/ignored | freeze 06 gates |
| Root bench panel decoder | 2 passed, including legacy/rich/minimal/malformed wire | [freeze 05 gates](gates/frozen-gates-05.json) |
| UI / model full | 57 / 380 passed, no failures or ignores | [freeze 06](gates/frozen-gates-06.json) / [freeze 04](gates/frozen-gates-04.json) |
| M3 extension model regression filter | 53 passed | [freeze 04 gates](gates/frozen-gates-04.json) |
| Core assets / registry authority | 10 / 2 passed | freeze 04 gates |
| Core typed desktop / worker units / status shapes / terminal stream | 2 / 3 / 5 / 5 passed | freeze 04 gates |
| Existing ignored Rust real-Bun smoke, explicitly selected | 1 passed | freeze 04 gates |
| Existing actual Bun worker smoke | passed with private production bundle | freeze 04 gates |
| TypeScript / typecheck | 745 passed, 7 existing skips across 82 files; typecheck passed | freeze 04 gates |
| Production event union/reopen audit | 25 distinct channels; 100 cycles return allocations to zero | [channel audit](gates/frozen-04-channel-audit.log) |
| Native Host platform/status ports / pure colour formatter | 12 / 1 / 1 passed | [Host ports](gates/frozen-gates-05-host.json) / [feature-enabled formatter](gates/frozen-gates-06-color.json) |
| Normal check / debug build / diff | passed | freeze 06 gates |
| Lock drift | passed | freeze 05 gates |
| Rust 1.85 core/model check and model tests | passed; model 380 passed | freeze 05 gates |
| Rust 1.85 core terminal stream | 5 passed, including root origin regression | freeze 05 gates |

The static specification's original worker inventory is retained. The current complete TS runner collects 97 `src/main` cases: 90 pass and 7 retain their existing OS-dependent skips; none were deleted, newly skipped or relabeled. The effects test mock was extended to assert the new real HUD hook exactly once. The existing worker smoke still covers its production roundtrip, boundary rejections and EOF cleanup, using an explicit private bundle rather than shared `dist-backend`.

Freeze 04's UI/model/core/TS/worker gates remain valid for unchanged components. The only change from freeze 04 to freeze 05 is the three native RuntimeView colour files, followed by full native/bench and normal check/build reruns. Final freeze 06 changes only import ordering in the UI terminal grid and fixture harness to the Rust 2024 style, followed by full native/bench/UI and check/build reruns. [Final format boundary](gates/64-final-format-boundary.json) records those two files. [Input comparison](gates/58-reused-component-inputs.json) records that boundary. No Cargo task ran concurrently with source edits; each frozen gate checked all compiled inputs before and after.

## Root M10 composition

Root authored every M10 repair. Unified hunks preserved M9 integration; no whole-file replacement was used. Applied patches:

* `m10-root-repairs.patch`: `9a5e55541bb55db73ac56b22359b3270758eedac582a144121ed9cc8a795a9bd`.
* `compile-corrections-v2.patch`: `3a5de7d82be74ed60393646914b3d42d41b9be1f37c5e096ec07c04d7fd6eff1`.
* `bench-wire-v3.patch`: `a5f277ddb0c88d2d51c79ec33419e0813b4e6b35a3008b5f13fea9dff0a52aba`.

Root requested focused filters passed: terminal 49, command-bar terminal controller 4, app M10 1, session panel 1 before the added bench-only regression, and core stream 5. Raw logs are `23-root-terminal.log` through `27-root-core-stream.log`; [composition hashes](gates/16-root-patch-application.json) and [bench patch after-hashes](gates/52-root-bench-after.json) record application. The original [Opus report copy](../native-m10/reviews/tezbar-gpui-m10-opus-b739b4e.md) is byte-identical, SHA256 `3e358161ce8d033d4170a8e57378d47ca36f3ddd72af132b6a93af92b3f7fb5a`.

## Sources, artifacts and limits

Final compiled-input aggregate: `185165ded66563538834c524d72dbf0cf04bd966486997a6010004ec9b0dcfa4`, with all 473 file hashes in [final source freeze](final-source-freeze-06.json). [Source ownership](final-owned-source-files.json) enumerates 69 source/resource entries and distinguishes root/composed changes; held M11 sources remain unchanged. [Dependency audit](final-dependency-audit.json) records 543 reachable normal/build packages, exact pinned Zed licenses and no Tauri/Wry/Tao/WebKit/JavaScriptCore packages or GPL licenses. All manifests/locks and the accepted asset implementation are byte-identical to base. [Private worker bundle](final-worker-bundle.json) records 23 actual source inputs and 33 resource/fixture files, including the real extension runner and no prohibited first-party core imports.

The normal arm64 Mach-O debug binary is 217,320,936 bytes, SHA256 `e3bb9e913f80890497996c88bdf4132698c623bb91f861839646dda812043aa7`. [Binary metadata](final-binary-metadata.json) records UUID, dylibs and Bun metadata/hashes. No WebKit or JavaScriptCore dylib is linked. Binary size is not a RAM measurement; the first-party default launch/build flow remains unchanged until M11.

All original M9 drafts, source maps and review/evidence files remain unchanged. RED logs are retained in [gates](gates/): initial compiler/fixture failures, `17-root-terminal.log`, freeze 01 obsolete placeholder regression, freeze 02 preflight visibility compile error, freeze 03 bench Deserialize error and two actual TS regressions, freeze 04 Python 3.9 lock-audit error, and the edition-correct formatting findings in `61-final-owned-fmt.log`. The first optional core colour filter selected zero tests because `native-ui-instant` was omitted; its zero count is preserved and the feature-enabled filter is recorded separately. Repairs were followed by new source freezes or isolated non-source gate reruns. All 51 owned Rust files pass the edition-correct [format check](gates/65-final-owned-fmt.json). The [global fmt check](gates/68-final-global-fmt.json) exits 1 on 49 unrelated files, all byte-identical to base and with no owned overlap; they were preserved. No workspace-wide fmt pass is claimed.

The [final case map](final-source-case-map.json) retains 115 expanded TS cases: 97 have their original model filter in a passing log; 18 have no matching filter. The old case filters do not directly name the new native tests. Exact new native scenario assertions are listed separately; a one-to-one native replacement map is still partial. All 45 E rows remain unaccepted. Fresh Opus 5.5 and M3 final closure, manual Windows/ConPTY, physical IME/AX/GPU and RAM/performance measurements remain parent gates. Both remote Windows attempts failed before tests (OAuth refresh/account usage limit); this receipt claims **no Windows execution or pass**. No live store/provider/MCP or real extension installation was performed.

[Temporary cleanup](gates/63-temp-cleanup.json) removed nine exact task-owned worker output directories and retained one bounded final private bundle for reproduction. No owned worker process required killing. The final receipt/index records source verification and process state after all gates; all raw logs remain under `gates/`.

The [machine-readable gate summary](final-gate-summary.json), [final process/source state](gates/69-final-state.json) and [evidence file index](final-evidence-index.json) provide the complete gate commands, raw log paths and SHA256 inventory. No Cargo/rustc or owned worker process remains running.
