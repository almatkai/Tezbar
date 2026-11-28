# M7 verification record — 2026-11-07 retry

M7 acceptance remains open. All M7 work is unstaged/uncommitted in `Tezbar-native-rust-agent`, branch `feat/native-gpui-app`, HEAD `ba3a227dd3dd4084c9dfaea11759bdd299d297ac`. Parent owns final integration/commit and a fresh mandatory Claude Opus 5.5 review. This run used the requested Codex model without subagents or provider/account fallback.

## Frozen automated gates

Rust is explicitly **1.98.1**, `CARGO_BUILD_JOBS=2`, `--locked --offline`, runtime-shaders for every native gate and `--test-threads=2` for tests. Normal native tests/builds do not enable bench-control. [source-freeze-final.json](source-freeze-final.json) records 309 conservative source/build/asset inputs, the external include assets, Cargo lock/toolchain, compiler identity and 25 M7-owned/coowned files. Aggregate SHA-256: `ce360d24fcd0a70a22928f4a6427effcce4b06d6c93dcede8edc1ff075afe581`. The 309 files were held after formatting and before these gates; M8/M9 remained unexported. Initial/visibility freezes remain historical snapshots, not passing receipts.

| Gate | Result | Log |
| --- | --- | --- |
| Settings namespace | 25 passed, 0 failed/ignored | `/tmp/tezbar-native-m07-final-settings-1.log` |
| Onboarding namespace | 5 passed, 0 failed/ignored | `/tmp/tezbar-native-m07-final-onboarding-1.log` |
| App M7 composition | 3 passed, 0 failed/ignored | `/tmp/tezbar-native-m07-final-app-1.log` |
| Launcher Settings integration | 4 passed, 0 failed/ignored | `/tmp/tezbar-native-m07-final-launcher-1.log` |
| Entire native library | 187 passed, 0 failed/ignored | `/tmp/tezbar-native-m07-final-native-1.log` |
| Entire bench-control library | 188 passed, 0 failed/ignored | `/tmp/tezbar-native-m07-final-bench-1.log` |
| Shared UI library, including parent TextInput/rich-text inputs | 51 passed, 0 failed/ignored | `/tmp/tezbar-native-m07-final-shared-ui-1.log` |
| Native cargo check | passed | `/tmp/tezbar-native-m07-final-check-1.log` |
| Normal debug build with runtime-shaders | passed, ordinary dev profile | `/tmp/tezbar-native-m07-final-build-1.log` |
| Owned formatting | passed, 22 files with skip_children; only three authorized test function excerpts in peer files | `/tmp/tezbar-native-m07-final-fmt-1.log` |
| Whitespace | passed | `/tmp/tezbar-native-m07-final-diff-check-1.log` |
| Normal dependency tree | no tauri/wry/tao package | `/tmp/tezbar-native-m07-final-normal-tree-1.log` |
| Normal debug binary linkage | no WebKit.framework | `/tmp/tezbar-native-m07-final-linkage-1.log` |
| Post-gate freeze/source audit | 309 inputs unchanged; 65 original sources unchanged; all 37 fixture names in passing full log; staging empty | `/tmp/tezbar-native-m07-final-audit-1.json` |

Normal debug executable: `target/debug/tezbar`, SHA-256 `13b764a11112c2d1897e1a4fe0826cfc13af26c31fd9f11c87070d095c6ed1da`. It was inspected with otool and never launched. Existing core warnings and the intentionally unused M8 project-review seam remain visible; no source was changed to silence them.

Raw commands and exit codes are retained in `/tmp/tezbar-native-m07-final-gates.json`. Test counts overlap: scoped tests are included in native and bench-control totals. This is not a claim of one native fixture per original parameterized TS case.

## Current-source coverage

The native inventory has 37 M7 fixtures. Actual production Settings, Onboarding, Launcher and Settings shell entities are mounted with finite synthetic transports and production model/WriteQueue calls. Disposable files exercise the actual atomic launcher UI-state writer; no replacement test state machine is used.

Current probes verify fresh provider maps, removal/recreation and discovery retry ownership, dirty TextInput hydration, save-before-Copilot-sign-in, task overrides sharing one queue with the launcher picker, picker settlement after a peer read failure, and picker settlement independent of a held download. Close and quit retain all-owner operations and parent persistence barriers. A config edit admitted during the parent UI-state drain is rechecked. Failed setter retries cannot replay an older superseded value or auth/test operation. Voice buttons are clicked through real native bounds: selecting an uninstalled model, deleting partial assets and cancelling one model produce exact existing payloads and never start a download implicitly.

Knowledge mutation completion survives tab changes. Indexing→Knowledge Settings→close returns to the identical retained Indexing entity. The project-review native fixture coalesces its UI nonce and uses core-issued projectReviewId for trust/dismiss. `Owner::settings_project_review` remains unused until M8 dispatch wiring; no end-to-end chat project-notice/session acceptance is claimed. Late OAuth-start cannot open the browser after cancellation. Opaque saved-secret slots and multiline connection edits use existing production contribution APIs.

Onboarding tests execute practice path completion, the real calculator, platform step selection, finish failure/retry, late shortcut-load fencing, boot retry/fail-open, retained hide/show and Finished identity fencing. The App fixture exercises Launcher/Onboarding composition without invoking real Owner OS window effects. Parent host feedback and close errors keep exact entity/window/generation ownership. Native close-error regression covers both an earlier retry generation and a fresh shell.

## Preserved failed attempts and exact test-only assertion changes

- Historical Settings `/tmp/tezbar-native-m07-tests-13.log`: 18 passed, 1 failed (held-operation cancellation), zero ignored. `/tmp/tezbar-native-m07-settings-current-1.log` subsequently passed **19/19**; that previously missing record is retained here. Newer final scope is 25/25.
- Earlier onboarding `/tmp/tezbar-native-m07-onboarding-3.log`: 4 passed before practice/App composition; superseded by the executed current 5-case scope and 3 App fixtures.
- `/tmp/tezbar-native-m07-current-namespace-1.log`: compile failed at three newly authored Voice fixture calls using an unsupported string simulate_click API. Repaired to use real debug-selected native button bounds and coordinate clicks.
- `/tmp/tezbar-native-m07-current-namespace-2.log`: **183 passed, 4 failed, 0 ignored**. Three old launcher assertions conflicted with authorized M7 contracts; the fourth was an M7 fixture that supplied a directory during initial load and therefore triggered the intended corrupt-initial-file preservation guard.
- `/tmp/tezbar-native-m07-app-current-1.log`: compile failure from an attempted private state_path fixture injection. Corrected without widening field visibility. The constructor receives a valid absent path, then a directory is created there before the actual atomic save. Same owner reports the write failure, the directory is removed, and Retry saves Theme::Dark plus KZT durably. `/tmp/tezbar-native-m07-app-current-2.log`: 3/3 passed.
- `/tmp/tezbar-native-m07-current-namespace-3.log`: **187/187 passed** before final close-error fencing/format. `/tmp/tezbar-native-m07-settings-current-2.log`: **25/25 passed**, including the final close-error assertions. Frozen final full receipts above follow those changes.

The user/parent explicitly released only these three peer test functions; no production behavior was changed to satisfy them:

1. `command_bar/tests.rs::native_command_ids_route_and_record_usage_once`: replaced the Settings-in-Launcher.navigation expectation with exactly one actual Relay Event::Navigate to the requested surface, empty admission and unchanged command origin. Non-Settings routing assertions remain. Exactly-one search:record-usage and no search:execute assertions remain.
2. `command_bar/review_regressions.rs::review_delivered_settings_and_placeholder_routes_retire_admission`: for Settings, invokes actual navigate, requires exactly one matching Relay event, then consumes/acknowledges delivery and requires the previous retained navigation to survive. The initial origin is Indexing. All 64 admission cycles and non-Settings placeholder assertions remain. The separate actual retained Indexing entity/close fixture remains green.
3. `command_bar/review_tests.rs::model_config_read_failure_does_not_block_durable_launcher_drain`: replaces the raw synthetic transport error string with the exact safe visible model-save error. Still asserts no launcher-state write error and an actual durable UI-state file. It does not remove the error assertion or substitute in-memory durability.

All failed logs remain untouched. No test was skipped/ignored/deleted and no source test hash was rewritten to hide a failure.

## Acceptance limits

The immutable M3B and M6 Opus reports remain unchanged. Their controller/earlier-scope reviews do not accept M7. [source-case-map.json](source-case-map.json) retains the original 65 source hashes and 143 expanded reference cases; all 49 mapped ST/O/C rows remain partial. The original source hashes are rechecked in the final audit.

Pinned GPUI TestWindow lacks a11y_init. The failed real-tree probe remains in `accesskit-red-probe.rs.txt` and `/tmp/tezbar-native-m07-tests-7.log`. Roles, password options and native keyboard controls do not establish actual VoiceOver/Narrator acceptance. No app launch, manual OS/Windows/IME/AX/GPU/RAM check, live provider/model/MCP call, download/install, shipping profile, credential-store, clipboard, permission dialog, release, Actions or commit/stage/push/PR was performed. U3 stays startup read-only, PushToTalk descriptive and Last Screen saved-but-inert. M8/M9 stay unexported; migration and fresh independent review remain open.
