# Coordinated M9/M11 implementation validation — not migration acceptance

This is the single implementation gate owner's evidence, not an independent review.
Selected worktree is `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`,
`feat/native-gpui-app`, HEAD `6ffa4e50fc0a45927319f9d791677ed40f78a948` plus dirty WIP.
No clean-PIN or clean native binary claim is made.

## Freeze and source corrections

The parent-approved 3,121-file freeze was independently reproduced byte-for-byte:
`dc3220ad914ffc33c52c6273d36cc9d82b0ee0f16d0494d73a9e3dae164de40d`.
M11 compiled aggregate remains exactly
`4c0cd6d4aead5d86de824f735c7013f34f2d421e9d3ef4fb493325cbf387f938`.
The original approved manifest and M11 source freeze are preserved, not rewritten.

Two M9-only trivial compile corrections were needed and preserved as successor
freezes: the SVG raw-string delimiter/closure Result type in `status_icons.rs`,
and explicit boxed-Send erasure of `sync_sdk_status` at the recursive Tokio
request/callback boundary. No M11 or parent repair source changed. Final validated
3,121-file aggregate is
`afcafe604cce6011cbad4927db75dc7ee3b19a1b9d3721cb8361855192b454cf`.
The other 3,119 approved files stayed byte-identical throughout all gates.
M9 owned-source aggregate is
`88f64c2c1a5406532759f4b7497b43d72e259f85a8823a06379d3255f4f63067`.

All compiled gates recorded before/after full non-ignored worktree hashes and
prepared worker/seed hashes. Every recorded gate has stable inputs. Documentation
added after the compiled gates is not retroactively part of their validated tree.
All raw attempts, including failures, live under
`/tmp/tezbar-native-m911-validation-20261107-2sefx6og/` and the copied gate records.

## Regression RED and restoration

Only the two explicitly authorized files were temporarily replaced with exact PIN
bytes: `extension_worker.rs` and `extensions/process_host.rs`. Each attempt had
fsynced independent implementation/PIN backups plus a `try/finally` restoration
proof. Both implementation files and the whole expected tree were hash-proven
restored before any subsequent implementation gate. No Git checkout/reset was used.

- `red-01-registered-name`: compile failure in the new helper, **not RED**.
- `red-02`: actual registered exact test ran, **0 passed / 1 failed**: Checkmark
  was the old 16×16 bitmap instead of the expected 24×24 declared-symbol pixels.
- `native-check-01`: M9 opaque-future Send compile error, **not a passed gate**.
- After the Send correction and renewed freeze, `red-03` again reproduced the
  exact **0 passed / 1 failed** pixel regression and proved full restoration.
- That same real-Bun test passes in the six-test targeted green run and both full
  native suites. No zero-match or compilation mistake is counted as regression RED.

## Executed gates

| Gate | Result |
| --- | --- |
| Rust 1.98.1 native check | PASS |
| Normal native debug bin build | PASS, actual Cargo JSON executable selected and copied before bench build |
| Normal full native library (Asia/Almaty) | **460 passed**, no ignored |
| Bench-control full native library | **462 passed**, no ignored |
| Targeted status-icon/weak-binding tests | **6 passed** |
| Real SDK scenarios (America/Los_Angeles) | **9 passed**, actual registered SDK names; explicit original prepared seed |
| Shared `tezbar-ui` library | **57 passed** |
| Rust 1.85 core check | PASS |
| Inspected `extension_worker::status_icon_tests::` | **4 passed** |
| Inspected `extensions::process_host::status_port_tests::` | **3 passed** |
| Full TypeScript typecheck | PASS |
| Non-watching `pnpm test` | **748 passed, 7 skipped**; 81 files passed, 1 skipped |
| Native Node fixtures excluding provenance | **13 passed**, including real private toy Cargo/optimized metadata and Metal assembly fixtures |
| Provenance Node fixture | **HELD / UNRUN**: private toy Git init/add/commit permission is unresolved; the other six Node files pass. |
| Python cutover byte-roundtrip fixture | **1 passed** |
| Actual normal/bench metadata, version and links | PASS; details below |
| Normal dependency tree | no Tauri/Wry/Tao |
| Legacy Tauri compatibility | PASS using the exact prior read-only sandbox command |
| Owned rustfmt and whole-tree whitespace | PASS |

All Rust check/build/test commands use the requested explicit toolchain, locked,
offline, `-j2` configuration and existing targets. Private toy fixture internals
retain their own temporary targets; they are not product builds or native clean
attestations. Node's expected diagnostic-negative toy compile is part of a passing
fixture, not an unreported integration compiler error. No unfiltered core suite ran.

The actual normal native log names the M11 staged-new-release/source/version,
dirty/bench/tamper refusal, compiled-Metal match/mismatch, ambiguous/truncated
metadata, three-second quit reserve/deadline and fresh-install-vs-upgrade cases.
`m11-actual-test-lines.json` records their successful execution; this is not just
synthetic Node or source-trace evidence. The real Metal build itself was not run
on this CLT Mac, and no staged executable was launched.

The unchanged updater gates are explicitly reused, not rerun: **43 passed / 1
existing network-dependent ignored** on each of Rust 1.85 and 1.98.1. Every recorded
compiled input and log hash in the parent's two final updater receipts was checked
against current bytes. Parent R0/R1/R2/R4 and M11 R3/R5/R6/R7/R8 source pins remain
unchanged; original failed root runs and held cutover artifacts remain intact.

## Actual native binary provenance

Only `--version` was executed; neither binary was launched as an application.
`captureBuildMetadata` streamed the actual linked executable and found exactly one
valid bounded schema-2 record. It agrees with existing `--version` keys/types,
actual Rust/compiler/GPUI/source and feature flags. Both honestly have `dirty=true`;
`--version.dirty` remains the string `"true"`.

| Preserved executable | SHA-256 | benchControl |
| --- | --- | --- |
| `/tmp/tezbar-native-m911-validation-20261107-2sefx6og/tezbar-normal-dirty` | `063f61789082d427e28f53eb1bfcc0cd7c3428e4487bba864c36e7474c63f489` | false |
| `/tmp/tezbar-native-m911-validation-20261107-2sefx6og/tezbar-bench-dirty` | `6d14eeed460625aa209fc73d6877ed3dd7decc9df398acd1fe3b47749f84e965` | true |

Both have source PIN 6ffa4e5, runtimeShaders=true, GPUI
`c84af5b99cbc366d675064e52b23664a015b6e35`, Rust 1.98.1 and preview=true.
Normal metadata JSON SHA is
`a06d53e126ff5730fb43e04d49deedd3d1414e36852b5cb816635e957a0aa06a`;
bench metadata JSON SHA is
`00f883be2b30a79b395825417b0d58efbfc052f4700107d5a0ec3722460b537b`.
`otool -L` shows no WebKit/JavaScriptCore on either. The metadata remains linked
and readable despite dirty/bench production-admission refusal; no admission or
clean flag was weakened to manufacture this evidence. A real clean-bin metadata
gate requires the later parent commit and is still the parent's responsibility.

## Status-icon behavior and limits

Actual decoded pixels are verified at the fake native status boundary through the
real Bun worker and core: Checkmark differs from Gear, local PNG is straight RGBA,
SVG/data preserve expected colors, path/symlink/unknown-identity refusals are honest,
shared four-slot admission blocks, and revocation/pending dispose/replacement/
invalidation cannot restore stale icons. Portable source tests exercise both
Windows/non-Windows fallback policies; Windows OS painting was not executed.

Native status resolution uses the existing core asset service and shared app
transport/SVG decoder without a new CoreClient/lifecycle owner, runtime, HTTP
client, font database or WebView. SDK paths never enter the legacy V2 decoder.
No-icon Windows roots get a neutral circle; refused declarations get an explicit
warning marker, distinct tooltip and useful identity. Valid declared pixels are
not replaced, and macOS icon-only success does not duplicate a glyph as text.
Tauri's SDK-status capability remains off. Hide/navigation preserves the user's
chosen background MenuBarExtra lifetime until disable/remove/app quit.

Finite builtin match coverage is documented in the M9 handoff; unsupported
identities/forms remain explicit. No live HTTPS success was tested. Menu-only
changes may still fetch/decode the same icon; optional icon caching is honestly
deferred rather than introducing new retention/revocation state.

## Shutdown, remaining gates and non-acceptance

Every gate's before/after prepared-worker process audit was empty, including the
expected failing RED run. Successful real SDK fixtures explicitly drain/shutdown
their core. Final audit records no owned survivors; only owned fixtures could be
stopped. User preview PID 79164/profile were not touched or replaced. No product
Git staging/commit/checkout/reset/push, real profile/clipboard/UI/tray/provider,
updater network, install, signing, release, deployment or cutover application ran.
The provenance fixture was not run: its private Git init/add/commit internals need explicit permission under the no-Git-mutation instruction. This is a concrete permission-boundary handoff, not a claim that every required gate is green. The other thirteen Node tests passed without those Git operations.

The UNRUN M9 handoff/receipt/source hashes are preserved as superseded checkpoints.
M11's original source freeze and handoff are unchanged. This validation does not
accept Windows, actual OS tray appearance, HTTPS/live providers, IME/AX, GPU/RAM,
platform install/update/signing, M6 timing, L5 WAL/L9/L10 observations, the held
cutover, or full migration. Parent independently verifies these receipts, commits
locally, performs the real clean-bin metadata gate and obtains fresh independent
Opus review. Product default remains Tauri.
