# Scoped M9 / SVG follow-up, 2026-11-09

Selected tree: `feat/native-gpui-app`, HEAD `0228b4f`, with ongoing unstaged
migration work. These are working-tree fixes and scoped checks, not a new clean
build claim or acceptance of M9/M8/M10 or the full migration. The two review
inputs are preserved as `review-m9.md` and `review-svg.md`.

| Finding | Result / boundary |
| --- | --- |
| N-R2X-B1/B2 | The reviews cover `0e6e455`, predating `css-bound-followup-20261109`. The current implementation no longer re-tokenizes Display output and charges attribute scan bytes. Existing regressions pass in the Cargo run. Added comma/brace/slash delimiter variants also refuse in preflight, without executing the exponential matcher. |
| N-R2M | Added an independent 8 MiB declaration-copy ceiling before usvg. It charges all rule values across all expanded elements, plus the maximum inline style per element, with three copies for `marker`, twenty for font shorthand/reset and per-attribute overhead. Both the universal-marker and inline-`use` regressions fail before this repair and pass after it. |
| M9 F4 asset names | The shared image resolver maps SDK names into `<root>/assets`, retaining explicit `assets/...` paths. It handles nested asset names without filesystem work in the model. Parent traversal, canonical scope and symlink refusal remain enforced. All four image variants have a common contract regression; real native status resolution loads bare and object-form names. NativeV2's separate path contract was not changed. |
| M9 F4 tint | Reuses the already-locked `svgtypes 0.16.2` parser through a direct native dependency; no registry package, version or checksum changed. All nine SDK colours, CSS names, short/long hex and RGB/RGBA work. Alpha is applied to straight RGBA pixels. An unknown/oversized tint keeps the declared supported icon in its default colour. Unsupported icon identities remain explicit refusals. |
| M9 F6 refusal surface | The quit preparation carries a typed phase (`Durability` / `ExtensionWork`). A busy extension leaves the launcher message and does not open Settings or add the save-error prefix. The existing actual Owner quit fixture now checks both Settings handle and lifecycle state, in addition to preserving the live agent run. Real durability failures retain their Settings recovery path. |
| M9 F4 reply latency | **Open observation.** Icon resolution still precedes the run reply and can wait for a shared decode slot. This repair does not introduce an unowned spawned task or change the revision/lifecycle publication contract. |
| Provenance precision | `named-inputs-and-query.json` records all 22 first-party worker bundle inputs, the two named include_str inputs and a fresh 23-line worker-grants query with its source hash. It supplements historical evidence; it is explicitly not a complete compiled-input manifest. |

The copy ceiling is an SVG parser allocation budget, not a measured app RAM
result. Existing node, geometry, CSS source/matching, raster/layer and four-slot
admission limits remain in force. The expensive-valid-SVG CPU observation is
unchanged. No adversarial parser workload was executed to reproduce the memory
spike: the regression asserts the refusal from preflight itself.

## Executed checks

- Standalone SVG production-slice gate: **11 passed**. Initial RED: the two new
  copy-budget tests failed, while delimiter variants already passed the earlier
  repair. The standalone extraction predates final formatting; the final Cargo
  gate below compiles and exercises all of these tests from current source.
- Native lib test compile: Rust 1.98.1, locked/offline, two jobs, runtime-shaders.
  **57 targeted tests passed**, zero failed/ignored, **469 filtered**. This
  includes all **9 real SDK scenarios**, status icons, assets, App quit recovery
  and retained extension ownership, in **America/Los_Angeles**.
- Image model suite on Rust 1.85: **16 passed**, zero failed/ignored,
  **371 filtered**. This is the image slice, not the whole combined model suite.
- Typecheck passed; TypeScript **748 passed, 7 existing skips**.
- Scoped formatting and whitespace checks passed. The main App file's unrelated
  baseline formatting was preserved.
- The ten scoped input hashes stayed identical across native compilation and
  execution. A new private worker seed was prepared from the selected tree;
  shared dist-backend and the previously lost seed were not used or replaced.

All changes remain unstaged. No GUI, Windows, physical keyboard/IME/AX, live
provider, real tray appearance, HTTPS success, GPU/RAM, signing, updater or
cutover acceptance is inferred. The running user preview and installed app were
not changed. Fresh independent review and remaining platform/manual gates are
still required.
