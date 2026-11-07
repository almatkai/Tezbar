# M11 source-stable implementation handoff — integration NOT accepted

Target: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch
`feat/native-gpui-app`, unchanged HEAD `6ffa4e50fc0a45927319f9d791677ed40f78a948`.
The complete Rust/GPUI migration is **not complete**. This is an uncommitted source
candidate for parent review and one coordinated whole-tree validation owner.

## Freeze and ownership

M11 source changes have stopped after the parent's shader-mode/live-reference correction.
The current compiled source aggregate is `4c0cd6d4aead5d86de824f735c7013f34f2d421e9d3ef4fb493325cbf387f938`. Exact input pins, tooling pins,
coownership and the aggregate algorithm are in `source-hashes.json`. The earlier
`source-hashes-candidate-01.json` records the superseded first freeze; its logs remain intact.

| Compiled input | SHA-256 |
| --- | --- |
| `apps/tezbar/src/app.rs` | `c8e43c0cd2c02db58b0255320e82981014285ea49cc534a342f032e46edb1b09` |
| `apps/tezbar/src/app/m11.rs` | `f5a511c23dd418df581583e64551ff404f961e2bd70db4aa47cf3279028cb74e` |
| `apps/tezbar/src/app/m7.rs` | `e21a539e3fd60484efd814dfce79749dfa7ad15fa5ff88283e2042dddf774fb5` |
| `apps/tezbar/src/app/production_launch.rs` | `3b616964306cd50a340ddd5da1901b477b9c1e01c759b4710a75cfbf9d52787e` |
| `apps/tezbar/src/app/production_launch/build_metadata.rs` | `4115a37670a7e5d2ff22f60d5cda255b03ca0cceab0aac13a293173ff99ca4bd` |
| `apps/tezbar/src/app/production_launch/bundle_validation.rs` | `0e2330896f075bbc34b9df891292277854eb0cc6226313744108f33bed1dc1f5` |
| `apps/tezbar/src/app/quit_budget.rs` | `6486f709c4390c025cefbb26d6a9abda387836e44487f783df41c5cb80f6a25d` |
| `apps/tezbar/src/main.rs` | `cfc8f9010e1602446e770dda0436cc9a7f49db7d009b7766a5be9ab0282ab998` |

Owned: production launch and its two new pure submodules; `quit_budget.rs`; the quit/theme
hunks and deadline fixture in `app.rs`; the Settings theme hunk in `app/m7.rs`; staged
validation in co-owned `app/m11.rs`; the `--version` metadata-reference hunk in `main.rs`;
native build/packaging helpers and fixtures; the review-only cutover generator.
`app.rs` retains Pi's `host.bind_status_assets(&core.0)` exactly. No Pi-owned file was edited.
No manifests, dependencies or locks were changed.

Parent repairs R0/R1/R2 remain byte-identical in `build.rs`, legacy-state source/tests,
updater permission logic/tests and `provenance.test.mjs`. R2's exact Git-root/tracked-script
and `git --no-optional-locks` protections remain in co-owned `build.mjs`. R4's actual error
logging remains in co-owned `app/m11.rs`. Parent proofs are inherited evidence only;
they do not accept the integrated tree.

## API and behavior changes

**R3:** `production_launch::verify_staged(bundle: &Path, release_version: &str)` is
crate-private and macOS-only. The real `m11::install` reads `artifact.version()` before
consuming it, stages privately, calls this validator, then calls the existing
`StagedMac::install_over`. Pinned codesign verification is mandatory in the shared wrapper
and remains mandatory inside `install_over`. Failure occurs before swap, backup creation/
deletion and relaunch. The existing updater signature, feed, archive and spawn/rollback
contracts are unchanged; no updater runtime/client was added.

Startup and staged admission share `bundle_validation::verify_payload`. The staged path
uses the **new release's version and staged binary's source**, never the running old
process's SHA/version. It checks executable static provenance, signed manifest binding,
required resource inventory and streamed resource hashes, paths/symlinks and Info.plist
identity/executable/version. Startup additionally compares the actual binary record with
its own compile-time record. No staged executable is executed, and these checks do not
resolve a HOME/profile or open application state.

`build_metadata::capture` and Node `captureBuildMetadata(executable)` stream the actual
binary, recognize exactly one bounded static JSON record, and reject missing/duplicate/
truncated records. Feature values are compiled into this record. Public
`production_launch::current_build_metadata() -> serde_json::Value` is consumed by the real
`--version` path, giving a live record reference independent of dirty/bench admission
refusal and `#[used]`. Existing `--version` fields/types remain: version/source/dirty string,
rustc_commit string, preview bool, gpui_rev string, runtime_shaders/bench_control bool.
Actual native linking and output compatibility still require the post-freeze gate.

Manifest **schema 2** contains version, sourceCommit, dirty, benchControl, runtimeShaders,
gpuiRev, rustVersion, binaryMetadataSha256 and resource file pins. The hash binds the exact
metadata JSON bytes read from the copied executable to the sealed manifest. Pinned codesign
independently authenticates all executable bytes and seals the manifest/resources. This
is a binary **metadata** digest, not a whole-binary digest: a whole-file hash inside the
resource seal would create a cycle with the executable signature. Private assembly remains
unsealed/nonpublishable, and ad-hoc preview assembly cannot satisfy production identity.

Both shader modes are valid: runtime compilation (`true`) and compiled Metal (`false`).
The actual mode must match the manifest/assembly plan. `nativeBuild` still deliberately
builds runtime-shaders and checks that explicit choice. Shared admission/assembly accepts
a matching false mode. A Node assembly regression passed for false and for mode-mismatch
refusal; the new Rust admission regression is UNRUN under the hold.

This validator accepts **schema 2, GPUI c84af5b99cbc366d675064e52b23664a015b6e35 and Rust
policy 1.98.1 only**. Schema 1/record-less archives require rebuilding. Already installed
validators do not automatically admit arbitrary future schema/GPUI/compiler revisions;
those require an explicit compatibility/validator-upgrade path. The old Tauri archive and
minisign/feed transport contract is preserved, not a promise that arbitrary old payloads
pass new native admission.

**R5:** production `quit_budget::commit(elapsed, &committed, destructive_callback)` requires
at least three seconds of the original eight-second budget before commit/dispose/cancel.
The real coordinator calls it immediately before disposal. Insufficient time becomes
`QuitPreparation::Refused`; it does not invoke the callback. Post-commit deadline handling
remains `ForcedAfterCommit`, and core receives only the original remaining budget. No extra
eight-second clock and no claim that incomplete final writes were saved.

**R7:** the one `apply_theme(Theme, &mut App)` helper sets tokens, native appearance and
refreshes windows. Cold start, recovery, Settings and preview initialization call it.
Saved-theme hydration retains the existing `preview.theme == "system"` guard, so explicit
light/white/dark preview choices retain precedence. Private recording adapters exercised
all three theme outputs; actual GPUI/AppKit appearance is UNRUN.

**R8:** `runCargo` streams `--message-format=json`, prints rendered compiler diagnostics,
and selects only the actual native manifest's non-test `tezbar` bin executable. Missing/
ambiguous artifacts refuse; check mode requires no executable. No guessed conventional
`target/<profile>` path remains. A real private toy Cargo fixture proved configured
`build.target` + custom `CARGO_TARGET_DIR` selection with a stale non-triple executable
present and ignored. It also proved check mode and readable failure diagnostics.

## R6 successor proof and release limitations

The current successor is in `cutover-successor-post-review/`. The first local successor
under `cutover-successor/` and all original held artifacts are preserved. The original
`integration/final-cutover-review.patch` remains SHA-256
`159dc2fb402b008214e8241f869f69eb24a006d4413c099ceb0aeea4088c94a2`; its inventory remains
`6e96fedf7efaac2ab53ff3c851b45174ea35f451c604675bde4351899101d233`.

The generator requires a **fresh explicit `--output-dir`**, splits only LF and verifies
actual patch application in a private temporary directory without a Git checkout/worktree.
Every changed text file roundtrips byte-exactly. The Unicode regression includes U+0085,
U+2028/U+2029, other non-LF separators, CRLF and no-final-newline cases. All 282 original
relocation authorities and 52 separately pinned binary copies remain identical to the
old inventory; binary source hashes were verified. See `cutover-proof.json`.

**NOT_APPLIED / NOT_RELEASE_READY.** No product default switch, deletion or pipeline run.
`scripts/build-release.mjs` still invokes the Tauri CLI and cannot build a native release
after relocating `src-tauri`. Local-preview assembly is not a public release builder.
Required cutover gates now explicitly include:

1. Implement and review the actual native signed macOS/Windows release builder: executable
   artifact selection, complete resources/schema 2 provenance, signed helpers and app,
   installer/archive and truthful receipts, existing Tauri updater signatures.
2. Implement and review the native signing/feed pipeline: pinned certificate/requirement,
   helper Team IDs and resource seal, matching source SHA across platforms, complete
   existing feed platform entries, publication/receipt guards. No public execution here.
3. After applying in a separately authorized private copy, run `pnpm typecheck`, `pnpm test`
   and a real native release-builder dry run against relocated import/oracle/policy paths.
   A `--check`, dev command or old Tauri command is not this gate.
4. Resolve dev/tauri/tauri:dev/icons:generate after relocation, then independent platform,
   disposable profile/installer/updater, manual IME/AX/GPU/RAM and explicit cutover approval.

## Executed evidence versus pending native gates

Before the latest parent correction: 11 Node tests passed, including five tests from exact
pure Rust production sources in a private toy crate and one recording-adapter Rust theme
fixture. The optimized toy binary retained truthful metadata as dirty/bench features
changed. These inputs are pinned by `source-hashes-candidate-01.json`; they are **not**
proof of the final native executable or current full integration.

After that correction, **no Cargo was run**, including toy Cargo. Seven Node/recording-
adapter tests passed (including compiled-Metal assembly), plus two selected Node metadata/
artifact tests and the Python LF/roundtrip fixture. Selection excluded the toy Cargo cases.
Both successor generations passed private byte roundtrip. `git diff --check` passed.
Raw logs and expected diagnostic-negative toy compile attempts are preserved; `receipt.json`
also records the two initial failed path lookups and bounded stale XERJ result.

Parent must first review and coordinate the whole-tree freeze, then appoint **one gate
owner**. Pending gates include current native check/build/tests, the new compiled-Metal Rust
regression, existing and new async quit fixtures, and **captureBuildMetadata on the ACTUAL
final dirty native executable** plus clean/bench variants where authorized. Verify the
record agrees with existing `--version` fields and actual build features, remains linked
when production admission refuses dirty/bench builds, and accepts a matching compiled-Metal
production record. Synthetic markers/optimized toy output do not close that native gate.
Do not relax provenance, falsify dirty=false or alter release identity to manufacture a
clean variant. If authorized later: Rust 1.98.1 `--locked --offline -j2`; core/updater MSRV
1.85; filtered core tests only. This handoff does not release the hold.

M6 cold-start timing, L5 WAL and L9/L10 observations, Windows, signing/install/update,
manual GUI/IME/AX/GPU and matched RAM remain OPEN. No production `/Applications`, real
profile, Keychain, clipboard, GUI, remote host, updater network, release/signing/install,
Git stage/commit/checkout/push, or preview PID 79164 interaction occurred.
