# Independent Brew source review

Reviewed 2026-11-20. Read-only review of `C:/Users/alikh/Tezbar-rust-ports/brew`, its pinned `reference/` source, current Tezbar GPUI/process contracts and supplied evidence. No build, executable, Homebrew operation, user profile or desktop action was run. This report is the only file written by the reviewer.

## Findings

### P2 — an in-flight services refresh can recreate the menu after Hide

`src/tray.rs:35-39` takes the refresh mutex, then waits for the services CLI. `:75-76` publishes the tray and replaces callback IDs without checking the menu generation. The polling task checks generation only **before** calling this asynchronous function (`:106-110`). Hide increments the generation and clears callbacks, releases the durable background lease and removes the tray (`:129-135`), without taking the refresh/publication mutex.

Concrete order: polling enters `set`, waits for a slow `services list`; the user invokes Hide, which releases/removes the menu; the old CLI completes, and `set` publishes the menu and fresh service callbacks again. Its next polling iteration eventually exits, but the recreated tray has no durable registration. The error fallback at `:108-110` can similarly publish `Brew !` after Hide.

Fence successful and failed publications against the captured generation, and serialize the final check/publication with Hide/removal. Check before and after waiting on locks as appropriate; a generation check followed by an unlocked awaited host call leaves another publication/removal race. Add a deterministic delayed fake-executor/recording-host test: pause a refresh, hide, then complete the refresh and verify that no later `host.tray.set` or callbacks survive. No real Homebrew is needed.

### P2 — “Select All Displayed Upgrades” also selects hidden packages

The global action is titled “Select All Displayed Upgrades” in `src/lib.rs:350-354`. Rendering applies the search filter and the current 100-row page (`:389-404`). However, `Intent::SelectAll` copies **all** `state.rows` into the selection (`:660-665`), without applying either filter or offset.

With 205 outdated rows, display page 2 and/or filter to one package, then invoke Select All Displayed Upgrades. The selection contains all 205 rows, including packages the user never saw in that page/filter. `UpgradeSelected` intersects those identities against the fresh outdated result (`:700-706`) and offers upgrades for the hidden packages too. Separate per-package confirmations remain; this is not a demonstrated silent upgrade bypass.

Reuse the same filtered/page slice for the selection action, or explicitly change the action contract to select all observed upgrades. A fixture test should assert exact selected formula/cask identities with both a search filter and an off-first-page offset, then verify that subsequent upgrade candidates cannot exceed that intended selection.

## Evidence checked

- All **146** files in `evidence/source-inventory.json` matched current bytes at review time.
- Independently rehashed the Windows development archive: `8c99dad0567547973a24d1ca4d57fbaa6ff039b1e3a1f55f36e1c9cbfb40df8e`. Its 75 payload members plus `extension.json` match the exact complete inventory; every payload digest matched. It retains the macOS-only declaration and empty artifact pins.
- `evidence/gates.json` records successful format, Rust test and binary build exits. The reported 22 Rust tests comprise the three adoption tests and the behavior/process tests. These were inspected, not rerun by this reviewer.
- The 22 protocol check groups in `evidence/protocol-results.json` are supported by a real JSONL harness and owned fake CLI. They cover launch dispatch, selected mutations, replay refusal and durable menu callbacks after view disposal. They do not cover the Hide/refresh interleaving or filtered bulk-selection scope above.
- Functional Windows fixtures explicitly initialize a synthetic macOS target. The separate Windows-target loop proves rejection before CLI access. Neither establishes native macOS Homebrew or GPUI acceptance.
- SDK commit `e955911f0bf5b558f9d3eb5183df0adc95c22af2` resolves through the public [pinned SDK Cargo.toml](https://raw.githubusercontent.com/almatkai/tezbar-extension-sdk-rust/e955911f0bf5b558f9d3eb5183df0adc95c22af2/Cargo.toml). It identifies SDK 0.1.1 with Rust 1.85 minimum, consistent with the immutable lock pin. This read does not replace compilation on the target toolchain.

## Contracts inspected without a confirmed defect

The twelve command IDs/modes and source preference objects are retained. Rendered List/Detail nodes and action IDs fit the current generic host shape; the host admits callback IDs regardless of the descriptive action `kind`. This is source-contract inspection, not visual/keyboard acceptance.

Subprocess launch uses a literal executable path and argument arrays, null stdin, bounded parallel stdout/stderr reads, timeouts, cancellation selection and kill-on-drop. Unix process groups are owned by a drop guard. Package/service/tap identifiers reject leading options, traversal components and shell/control characters. No demonstrated command-string injection was found.

Mutating callback IDs are consumed in process-owned state before awaited host effects, separately from SDK staged session state; a cancelled action cannot simply replay its previous ID. Mutations serialize process-wide. Package-scoped trust confirmation uses `trust --formula/--cask <full-name>`; explicit tap trust uses `--tap`. Tests inspect accepted/declined trust scopes. Adoption checks exact published bundle identity and version, rejects symlinks, and revalidates after confirmation; the documented filesystem TOCTOU window remains.

External terminal execution, broad Doctor remediations, richer metadata/analytics and upstream adoption heuristics are explicitly unfinished parity areas. The Services Menu entry currently first opens a list and requires its Enable Services Menu action to create the tray; that extra activation step should be described as a UX difference from upstream's immediately mounted MenuBarExtra. No automatic upstream menu launch parity is established by the all-command dispatcher count.

Native Mac compilation, real Homebrew behavior, actual process-group descendants, GPUI/tray lifecycle, permissions and production importer verification remain separate gates. The frozen development port is not release accepted by this review. Rebuild and refresh inventories/evidence after repairs.
