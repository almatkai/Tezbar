# Native review follow-up — 2026-11-07

This is an implementation/verification handoff, not independent acceptance of M9,
M10 or the migration. The input reports are copied here byte-for-byte; their hashes
are in `receipt.json`. Earlier review reports and evidence manifests are unchanged.

## SVG and terminal findings from the fec3257 recheck

| ID | Change | Evidence |
| --- | --- | --- |
| N-R2X / R2 | Before usvg matching, parse the same locked simplecss selector representation and cap conservative descendant backtracking work across the expanded element count. Existing stylesheet-byte and selector limits remain. | `review_followup_svg_descendant_selectors_are_bounded_before_matching`; simple child/descendant selectors and quoted attribute values render actual expected pixels. |
| N-R1F / R1 | Account for child RGBA layers while rendering children; add clip/mask scratch only at the parent's own peak, after child layers are freed. Retain simultaneous memory, geometry and expansion caps. | `review_followup_svg_clip_mask_and_opacity_keep_ordinary_icon_pixels` covers clip+mask, clip+two opacity layers and nested masks; original layer-bomb tests still pass. |
| N-R4W / R4 | A durable output gap after a failed/released replay acquires a fresh attachment/token and resumes at the retained cursor. Admission refusal clears loading and reports discarded input. | `review_followup_terminal_replay_error_then_gap_resumes_with_a_new_token`. |
| N-R5A / R5 | Coalesce consecutive alternate-screen wheel chunks, preserving directions and keyboard boundaries. Keep the byte ceiling and reserve 4 KiB for the next key. | `review_followup_terminal_wheel_during_replay_preserves_the_next_key` uses real GPUI wheel events and checks all 60 arrows followed by the key. |
| S1 | Split the four previously unformatted long-literal statements. | Scoped rustfmt and diff checks. |
| D1 | Historical `../root-m810-repairs/applied-files.json` describes the **first attempted application**, before its failing tests and subsequent corrections. It is not a final source manifest. `owned-source.json` there describes that round's final source; `changed-source.json` here describes this follow-up. | This erratum leaves both historical manifests byte-unchanged. |

`simplecss = 0.2.2` was already locked through usvg. The direct app dependency is
the only Cargo.lock change: no package version, source or checksum changed, and
the portable core did not gain the renderer dependency. The recorded maximum
cost of large valid SVGs remains an observation; these changes do not claim an
application-wide CPU or RAM result.

## M9 residual findings

The promoted independent probes first reproduced five failures (three dropped
foreground sessions and two uncontrolled dropdown selections). The fixed paths
pass both scripted native fixtures and the real CoreClient/Bun/TestHost boundary.

| ID | Implementation disposition |
| --- | --- |
| F1 / D13 | Retain bounded run-reply owners through hide. Dispose stale/rejected foreground views, including malformed Store replies. Retain MenuBarExtra in the background as explicitly requested by the user. |
| F2 / D12 | Retain an uncontrolled dropdown's chosen value across worker renders. A controlled `value` still wins; unavailable choices fall back to the current default/first option. |
| F3 | The registered form fixture uses a local calendar-date constructor. Its exact real-SDK scenario passes in America/Los_Angeles. |
| F4 | **PARTIAL / OPEN.** Icon-only status roots no longer duplicate their gear as text and keep a useful tooltip. Non-gear/custom declared SDK status icons still use the generic bitmap. Actual icon resolution needs a separate bounded native asset integration. |
| F5 | Native host notices use the neutral message path instead of the launcher's error slot. Source-traced; no physical colour/screenshot acceptance for this path. |
| F6 | Quit names the actual refusal without Debug wrappers or a false save-error prefix. Detached extension command jobs retain an explicit admission lease; quit refuses them promptly with a specific wait message and leaves the job alive. The real CoreClient/Bun fixture also verifies resume/completion and subsequent successful drain. |
| F7 | An authenticated in-flight no-view Preferences request adds an intent to its reply. The native caller checks its own request/generation/visibility and supplies its own extension ID, then delivers a typed extension-preferences payload to the existing Settings owner. Live runtime Preferences uses the same monotonic navigation path. |
| F8 | Store errors retain the worker's bounded message; transport failures retain their reason. |
| Docs | `worker-grants-current-map.json` supplies a matching current query and exact results, superseding only the stale historical query. |

The no-view preference fixture verifies the real worker/core flag; a separate
native fixture rejects a spoofed reply extension and rejects delivery after hide.
Actual OS Settings focus and controls are still manual acceptance, not inferred
from these fixtures.

## Gates and provenance

- Final normal native: **448 passed**, bench-control: **450 passed**.
- Shared UI: **57 passed**; exact west-of-UTC SDK form: **1 passed**.
- Rust 1.85 core check and four inspected worker framing/launch-path cases pass.
- Normal debug build, scoped formatting and diff checks pass. The binary receipt
  records `preview=true`, `runtime_shaders=true`, `bench_control=false`, dirty
  source based on 8fc0f67, and no WebKit/JavaScriptCore links.
- The source-freeze wrapper hashes all non-ignored tracked/untracked files before
  and after each gate; the final gate records say `stable=true`. Its aggregate is
  SHA-256 of Python `json.dumps(source_map, sort_keys=True).encode()`.
- `validated-tree.json` is the full normal gate input; `changed-source.json`
  contains the source/manifest changes, excluding documentation added afterwards.
- Raw failures remain in `gates/`: the initial red probes, two corrected fixture
  assertions (RGBA versus BGRA and a retained replay cursor), compile mistakes,
  a zero-match timezone invocation and two wrong UI feature invocations. The
  zero-match/feature errors are **not** successful acceptance gates.

The new binary is preserved at the path in the receipt. The running user preview
was not replaced or quit. Its earlier UI/shortcut evidence remains under
`docs/native-m05/visual-preview-20261107` and
`docs/native-m04/preview-launcher-shortcut-20261107`.

No full core/TS/model suite, Windows, live provider, physical GUI, IME, accessibility,
GPU/RAM, signing, migration or updater rehearsal was run in this follow-up. Tauri
remains the product default, and the cutover patch is held. Fresh independent
review remains required; F4 is explicitly open.
