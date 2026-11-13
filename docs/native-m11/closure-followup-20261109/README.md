# M11 follow-up to the 0e6e455 closure review

Working tree: `feat/native-gpui-app` at `0228b4f` plus concurrent, unstaged
migration work. This is scoped working-tree verification, not a clean production
build or full migration acceptance. The original review is preserved alongside
this report as `review-input.md`.

| ID | Implementation / remaining work |
| --- | --- |
| N1 | macOS staging and `verify_staged` now finish on the existing runtime's blocking pool **before** calling `Owner::quit`. A refusal shows a specific error and returns the verified download to the updater. Successful preparation retains the owned stage until swap; declining quit drops the stage and returns the download. |
| N2 | `build.rs` observes the compiler's `release:` and the resolved GPUI source revision in `Cargo.lock`. Policy comes from `rust-toolchain.toml` and the native manifest. Missing/mismatched identity forces the production dirty flag; the record contains observed values. Rust/Node admission and packaging use the shared policy, instead of repeating literals. |
| N3 | The thin, single-architecture constraint is explicit in the native builder README. Universal binaries remain unsupported and fail closed. |
| N4 | The successor inventory explicitly gates validator compatibility/bridge releases and the new binary's own admission predicate on the final signed bundle. The public release builder and this acceptance gate remain unimplemented. |
| N5 | Relaunch and rollback messages now retain the original OS errors, including failure to relaunch the previous version. Crash-leftover sweeping remains open; no existing stage or backup folders were deleted. |
| N6 | A new GPUI fixture drives the real App/Owner Install handler and proves refusal never enters quit. Full real-`Owner::quit` deadline/commit acceptance remains open; the existing helper tests are not relabelled as that acceptance. |
| N7 | The generator distinguishes deletion (`None`) from empty contents, emits Git file-mode headers and checks existence, bytes and executable mode in its private roundtrip. Empty relocations and executable scripts have real `git apply` fixtures. |
| N8 | Erratum: the updater's existing ignored case is published-Beta network installer discovery, not signing. Historical evidence is unchanged. |

`VerifiedArtifact::prepare_macos(&mut self, ...)` preserves the anonymous verified
download and rewinds it before each extraction. The consuming `stage_macos`
entry point is retained. The normal post-quit path accepts only the stage that
already passed payload admission; `install_over` still enforces the existing
signature/identity checks. No staged executable is run to inspect it.

The provenance toy now checks `TEZBAR_NATIVE_SOURCE_DIRTY` for Git attestation.
It intentionally has no GPUI and cannot attest production identity. The final
`TEZBAR_NATIVE_DIRTY` combines source and observed-identity refusal. A separate
fixture executes the actual build script with controlled compiler reports and
lock entries, checking wrong Rust/GPUI observations instead of policy labels.

## Verification

- Final native test library compiled with Rust 1.98.1, locked/offline, two jobs.
  **25 targeted tests passed**, zero failed/ignored. This includes actual App
  staged refusal, retained-download retry, observed metadata, admission,
  updater ownership and quit-budget tests, plus the two SVG CSS regressions.
  **496 other native tests were filtered**, not claimed as run.
- **8 Node fixtures passed**: build identity, provenance, admission and packaging.
  The new identity fixture fails with the HEAD/pre-repair build script in a
  separate disposable copy (`identity-baseline-red.log`).
- **3 Python cutover fixtures passed**. Full generation produced 282 relocations
  and 52 binary-copy records and passed the private byte/mode roundtrip. It was
  not applied to the product tree. Previous held patches were not replaced.
- Updater on Rust 1.85: **43 passed, one existing network case ignored**; the
  subsequently added retained-download fixture passed separately (**1/1**).
- Scoped Rust formatting and whitespace checks passed. The updater's unrelated
  baseline formatting was preserved; only the new method/test were changed.
- `final-own-inputs.json` stayed identical across the final native compile/run.

The first attempts with an incomplete shell PATH and then the GPUI test-macro
and nested task-result compile mistakes are preserved as failed attempts. None
is counted as a passing gate or a product baseline RED.

No production binary, signing, `/Applications` installation, updater network,
real user profile, GUI/IME/AX/GPU/RAM or Windows acceptance was performed. The
test's codesign reads were confined to its owned temporary fixture bundle.
No existing preview was changed. All repairs remain unstaged; fresh independent
review, the remaining release/platform gates and cutover authorization remain
outstanding.
