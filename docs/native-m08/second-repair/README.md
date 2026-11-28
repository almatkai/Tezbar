# M8 second repair: SVG work, quit preparation and Windows file authority

The original [Opus report](../reviews/OPUS_b739b4e.md) is preserved byte-for-byte. This round implements N1–N4 against `b739b4e`; it does not declare review closure, Windows acceptance or full migration completion.

| ID | Change | Evidence and remaining limit |
| --- | --- | --- |
| N1 | Reject SVG markers before usvg expands them; reject parsed group filters regardless of CSS source. Bound isolated layers to 64, path-source bytes to 256 KiB, parsed path segments to 16,384, in addition to the existing node/depth/use bounds. | Three permanent `review_n1_*` tests cover marker expansion, filter attribute/style/stylesheet, layers and geometry. The marker regression calls preflight before any expanded allocation. Existing valid SVG/use tests remain passing. |
| N2 | Retain the canonical prepared user turn and frozen selection while quit is preparing. A cancelled quit replays through the original controller once, rechecking token and quit state; final disposal drops the held preparation. A failed append cancels quit and restores the draft. | Four permanent `review_n2_*` GPUI tests cover cancelled quit, completed quit, re-entered quit and failed user append. None starts a provider run during quit preparation. |
| N3 | User-picked Windows images use a separate ordinary disk-file open, permitting aliases/cloud paths and `~` names. Validate rooted disk/UNC authority, final file type and size. Strict no-reparse, ancestor-locked traversal remains exclusive to extension-scoped reads. | Windows-only chosen-file/symlink/namespace regression added, **unrun on macOS**. No real OneDrive folder was accessed. |
| N4 | Compare ordinary and verbatim disk spellings without accepting a filesystem alias. Register both exact spellings of the canonical legacy registry icon. | `review_n4_legacy_registry_icon_reaches_real_client_for_both_spellings` passes through actual CoreClient, registry validation, model source resolution and asset decoding on macOS. Its ordinary Windows spelling and existing scoped-read races still need real Windows execution. |

The new Windows-only `m10_windows_tests::private_conpty_raw_input_replay_and_owned_process_shutdown` runs the real private PTY with `cmd.exe /D /Q /V:ON /C` and builtins only. It checks raw input admission, replay and exact-process shutdown. It is **not compiled or run on macOS**. `/D` disables Command Processor AutoRun; no user shell configuration or external utility is needed.

## Validation

Frozen source has 322 hashes in [source-freeze.json](source-freeze.json), unchanged before and after the final gates. This inventory includes inactive M9/M11 drafts; it is a source stability record, not a claim that those drafts were linked or accepted. Twelve tracked files changed. The App change is formatting only. The only lock change adds the already locked `sha2` as an app dev dependency for the real registry fixture; all package versions and checksums are unchanged.

- Final native: **324 passed**, zero failed/ignored (`native-frozen-final.log`).
- Final bench-control: **325 passed**, zero failed/ignored (`bench-frozen-final.log`).
- Core asset tests: **10 passed**; registry asset-scope tests: **2 passed**.
- Rust 1.85 offline core library check passed. It precedes only app SVG changes and the Windows-only PTY fixture, which macOS does not compile.
- Final all-target native check and normal debug build passed, Rust 1.98.1, `--locked --offline -j2 --features runtime-shaders`.
- Scoped rustfmt and `git diff --check` passed. Normal linkage has no WebKit and normal dependency tree has no Tauri/Wry/Tao.

UI 57/model 379 results from the previous repair remain applicable; their source is unchanged and those suites were not repeated. Commands for native/bench are `cargo +1.98.1 test -p tezbar-native --lib --features runtime-shaders[,bench-control] --locked --offline -j2 -- --test-threads=2`. Core filters were `assets::` and `extensions::asset_scope::tests::`; MSRV used `cargo +1.85.0 check -p tezbar-core --lib --locked --offline -j2`.

The [receipt](receipt.json) records binary metadata, source/log hashes and limits. The preserved binary is a **dirty b739b4e debug preview**, bench-control off, not a clean commit artifact or memory measurement. Its only execution was `--version`, before app construction. No GUI, live provider, profile, clipboard, credential or installation operation occurred.

## Preserved failed attempts

`red.log` records 4 expected failures and 5 passes against the earlier production code. `green-1.log` has 9 passes. `green-2.log` has 11 passes and one registry-fixture failure: macOS `/var` versus `/private/var` failed the intentionally strict exact-path policy. The fixture now canonicalizes its Unix root, retains ordinary drive spelling on Windows, and the real registry test passes in `legacy-green.log`. No production Unix path check was relaxed. The subsequent final native/bench gates supersede the earlier 323/324 runs.

Windows security/ConPTY execution, fresh Opus closure, native GUI/IME/accessibility, GPU/RAM, extensions and packaging/cutover remain pending.
