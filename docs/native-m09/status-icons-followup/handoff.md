# M9 F4 status icons — executed implementation validation

This is implementation evidence, not independent review or full migration acceptance.
The earlier UNRUN handoff, receipt and source hashes are preserved byte-for-byte
in `checkpoints/unrun-01/`. They describe the superseded source-only candidate.

## Results

- Real exact baseline RED: declared Checkmark became the generic 16×16 Gear;
  both authorized files were restored with durable backups and hash proofs.
- Targeted status/weak-binding tests: **6 passed**; normal full native: **460 passed**;
  bench-control full native: **462 passed**; UI: **57 passed**.
- Nine real SDK scenarios pass in **America/Los_Angeles** using the explicitly
  supplied original prepared bundle/seed, unchanged by the fixtures.
- Rust 1.85 core check and the **4 + 3** inspected status tests pass. The latter
  exercise both Windows/non-Windows fallback policies portably, not Windows OS.
- Typecheck passes; non-watching pnpm suite: **748 passed / 7 skipped**.
- Two trivial M9 compile corrections (SVG literal/Result inference and recursive
  future Send erasure) and their failures are preserved. No M11 code was changed.

The provenance Node fixture is **HELD / UNRUN** pending explicit permission for Git init/add/commit inside its disposable toy repos. Other native Node fixtures: **13 passed**; Python cutover: **1 passed**. This is a permission-boundary handoff, not all-gates-green acceptance.

## Source behavior and constraints

The Host capability resolves actual declared builtin/local PNG/SVG/data/HTTPS
representations through canonical registry authorization, the one core-owned
four-permit AssetService and shared app decoder/SVG budgets. The status path
creates no fresh CoreClient, lifecycle owner, runtime, HTTP/image service, font
database or WebView. Blocking decodes retain their permits inside the work.
SDK paths never enter the legacy native-V2 decoder; that existing relative-icon
contract now has bounded encoded reads and header/allocation limits.

Final publication rechecks session, scope and checked monotonic revision after
loading. Invalidation coalesces follow-up work under one owner/session. Tests
exercise late A→B→A, revocation, dispose/replacement, null root and invalidation.
The requested MenuBarExtra background lifetime through launcher hide/navigation
is preserved until disable/remove/app quit. Tauri's status capability remains off.

Windows title-only roots receive a bounded neutral circle, refused declarations
a distinct warning marker, and useful tooltips distinguishing missing from
unavailable icons. Good declared pixels are untouched. macOS icon-only success
does not duplicate the icon as text. This does not claim physical OS painting.

Supported builtin identities are exactly the finite `status_icons.rs` match set
(reusing legacy SymbolIcon paths and the prior declared Gear pixels), not all
Raycast builtins. Unknown identities, glyph/blob/application/fileIcon/mask and
unrecognized tint syntax are explicit refusals/fallbacks. Relative assets are
package-root relative, e.g. `assets/pixel.png`. Source wrappers/adaptive values
use legacy dark-first representation. No live HTTPS success was tested; current
HTTPS security/transport policy is reused. Optional menu-only icon caching stays
deferred; changing menu data may still fetch/decode an unchanged declaration.

## Provenance, shutdown and remaining acceptance

Exact commands, counts, stable full input manifests, raw failures/restoration
proofs, inherited updater proofs and actual normal/bench binary metadata are in
`../../native-m11/recheck-followup/validation/20261107-m911/`. Raw evidence/binaries
remain under `/tmp/tezbar-native-m911-validation-20261107-2sefx6og/`.

Both actual binaries truthfully report dirty=true and their normal/bench features;
only `--version` was executed. No WebKit/JavaScriptCore links or normal Tauri/Wry/Tao
dependencies were found. This is PIN plus WIP, **not clean-PIN evidence**. Parent
must perform the real later commit/clean-bin gate and independent review.

Every prepared-worker before/after audit, including failing RED, found no owned
survivors. Successful fixtures explicitly drained/shut down their core. User
preview PID 79164/profile was not touched. No product Git staging/commit, GUI/tray,
clipboard/profile/provider/updater network, install/sign/release/cutover operation
was performed. No Windows/manual/IME/AX/GPU/RAM or full migration acceptance.
