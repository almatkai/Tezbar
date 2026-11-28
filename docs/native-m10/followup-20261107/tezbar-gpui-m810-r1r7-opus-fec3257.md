# M8/M10 root repair recheck, R1–R7 @ fec3257

Reviewer: Claude Opus 5.5, working alone (no subagents, no repair implementation). Date: 2026-11-07.
Pin: `fec32571c4046f83823e1ca0fcee9548a85e65f2` on `feat/native-gpui-app` (worktree `Tezbar-native-rust-agent`). Source = root M810 implementation `37164d5` + docs-only `fec3257`.
Scope: R1–R7 of `/tmp/tezbar-gpui-m810-opus-0513321.md` (sha256 `a44f9595…5a8`, also preserved in `docs/native-m10/root-m810-repairs/`). M9, M11 and the broader matrix belong to other reviewers.

Evidence labels: **[RUN]** = executed by me at the exact pin; **[SRC]** = source trace only; **[REC]** = root-recorded, not rerun by me.

## Verdict

**NOT ACCEPTED as R1–R7 closure. This review does not accept the full migration or authorize a Tauri cutover.**

| ID | Verdict at fec3257 | Remaining issue |
|---|---|---|
| R1 SVG layers | Memory bound **CLOSED** for the documented preview subset (macOS, standalone release probe; not app RAM) | **N-R1F** LOW–MEDIUM functional regression: ordinary clip+mask icons refused |
| R2 CSS work | **NOT CLOSED** | **N-R2X** MEDIUM–HIGH: one-rule descendant selector, 374 B → 7.25 s; 430/606 B still running at the 30 s CPU cap |
| R3 dashes | **CLOSED** (macOS, standalone release probe + source proof against tiny-skia) | — |
| R4 failed starts | **CLOSED** for every claimed path | **N-R4W** LOW, pre-existing, outside the claimed paths |
| R5 pointer starvation | **CLOSED** for pointer reports | **N-R5A** LOW (documented ordering choice); observation O-R5P |
| R6 stale own Create | **CLOSED at the logic layer** | Real ConPTY **UNRUN**: platform hypothesis remains open |
| R7 keyboard docs | **CLOSED, docs-only** | Superseded after the pin by `8fc0f67`, which this review did not cover |

New actionable findings: **N-R2X** (MEDIUM–HIGH), **N-R1F** (LOW–MEDIUM), **N-R4W** (LOW), **N-R5A** (LOW). Nits: **D-1** (docs) and **S-1** (standards). The observations are not counted.

Still open and not claimed: Windows (scoped reads, OneDrive, ConPTY, NSIS; blocked by remote OAuth/quota), M78 N3/N4 on Windows, physical GUI/IME/AX/GPU, matched RAM, signing, real profile discovery and updater rehearsal.

## 0. Integrity and method

- **Pin archive.** `git archive fec3257` → `/tmp/tezbar-m810-r1r7-opus-fec3257/src`. `git hash-object --no-filters` matched all 2921 blobs before and after the review: 0 mismatches, 0 extra files.
  - ls-tree sha256 `3726ce92…`, tar `a2ef1971…`.
  - All execution used private copies.
- **Live worktree.** I never wrote to it, built in it, staged or committed.
  - At the start it was `fec3257`, clean.
  - **During the review it moved** to `9832852` (11:08) and `8fc0f67` (13:57), and gained 13 uncommitted entries. These changes are not mine.
  - `8fc0f67` edits `terminal/input.rs`, the spec and the ledger: it marks O-TERM-KEYS as "Resolved (user decision)". All verdicts here apply to `fec3257` only.
- **Compiled-source identity (root gates).**
  - The root `run-gate.py` freeze `native-normal-sources.json` covers 2877 paths, compared to the pin:
    - 2876 identical;
    - 1 differs: `docs/NATIVE_GPUI_VERIFICATION.md`, changed by docs-only `fec3257`;
    - 44 evidence docs were added later;
    - **0 code mismatches** (`logs/01`).
  - The dep-info of the root's final normal (09:55) and bench (09:56) test binaries lists 121 and 122 first-party inputs. All exist at the pin with freeze hash equal to pin hash (`logs/02`).
  - The preserved binary was built at 09:58:30 from the same freeze aggregate `ce9482b2…`. Its sha `5ec21f89…` is unchanged. Its embedded metadata honestly says `151df16`, dirty.
  - I never executed that binary. `otool -L` (a static read) equals the recorded `binary-links.log`, with 0 WebKit/JavaScriptCore entries (`logs/42`).
- **Root evidence ledger.**
  - `evidence-sha256.json`: 43 of 43 entries match the pinned files and the `/tmp` originals. The only uncovered file is the ledger itself.
  - `owned-source.json` equals the pin for all 10 files. For `applied-files.json`, see D-1.
- **Dependencies.** My offline `cargo tree -e normal` (no features, and `runtime-shaders`) lists 591 crates, with 0 tauri/wry/tao/webkit/javascriptcore/webview2 (`logs/41`).
- **Upstream sources.** The resvg/usvg/tiny-skia/tiny-skia-path/simplecss `.crate` checksums equal `Cargo.lock`. I read their extracted archives under `upstream/`.
- **Native build.**
  - Rust 1.98.1, `--locked --offline -j2`, `CARGO_INCREMENTAL=0`, `--test-threads=2`, `TEZBAR_NATIVE_SOURCE_SHA=<PIN>`, `--features runtime-shaders`.
  - The private target was an APFS clone of `/tmp/tezbar-m9-c01-validation-Rffw07/target/debug/{.fingerprint,build,deps}`. The source target was only read.
  - **Caveat, corrected.** The first build reused a cloned `tezbar-update` artifact, because the clone's fingerprints were newer than my copy. I touched every file in my copy (content unchanged) and rebuilt. All 6 workspace crates were then compiled from my verified copy (`logs/31`, `logs/32`).
- **SVG probe.**
  - A standalone **release** crate containing the byte-exact `asset_transport.rs:343-649` (`probes/svgprobe/src/verbatim.rs`, sha `8c131260…`).
  - Its `Cargo.lock` is a subset of the pin lock: 68 of 68 match by name, version and checksum.
  - Every run had an `RLIMIT_CPU` cap (`ulimit -t`) and was measured with `/usr/bin/time -l`. The only cap kills were the two N-R2X cases.
  - No multi-GB unsafe old render was run. The 0513321 decoder (prior review binary `5c4ac39d…`) was used only on small ordinary icons and on the repo corpus.
- **Preserved failures.**
  - `logs/10-…FAILED-extract-off-by-one.log`: my first extraction missed the closing brace.
  - `logs/24-…FAILED-zsh-nosplit.log`: one argument was not split.
- **Not used:** GUI, OS automation, clipboard, Keychain, credentials, real profiles, providers, network, remote or Windows.
  - Core tests were not run. The core PTY code is unchanged, so prior proof is reused as [REC].
  - The only real-`CoreClient` tests I ran (inside the `asset_transport` filter) use tempdir `CorePaths` + `TestHost`.
- **Processes and cleanup.** None of my cargo, rustc, test or probe processes remain. I deleted my private targets and build copy, which freed about 2.0 GiB.
  - The prior review's 44 inventoried artifacts are unchanged.
  - Both copies of the original report are still `a44f9595…`.

### My runs versus recorded results

| Gate | Root-recorded | My run at the exact pin |
|---|---|---|
| Native normal / bench | 420 / 422 [REC] | Not rerun in full. Focused run: **81 passed, 0 failed, 0 ignored** (339 filtered; 420 total) across `review_m810`, `surfaces::terminal::`, `command_bar::terminal_integration`, `asset_transport`, `app::m10`, `windows::terminal_sessions` (`logs/33`) |
| 14 `review_m810` regressions | 14/14 (focused-3) [REC] | **14/14** inside the run above |
| My independent probes | — | **8/8** (`logs/34`; module removed afterwards; copy re-verified 2921/0/0) |
| SVG cases | 10 (rustc -O over debug deps) [REC] | 35 cases (34 run; `r2_desc_d40_k14` generated only) plus a baseline (`logs/20`–`29`) |
| UI 57 / models 381 / TS 746+7 / Host 45 / core / MSRV | [REC] reused component evidence | Not rerun; app-only delta confirmed by the freeze comparison |
| rustfmt `--check` on owned Rust files | pass [REC] | **pass** (`logs/43`; see S-1) |

## 1. Findings, most severe first

### N-R2X — MEDIUM–HIGH (availability) — R2 not closed: CSS selector-matching work is unbounded — [RUN]+[SRC]

**Callsite.**
- `svg_preflight` bounds only the decoded style bytes (32 KiB) and the count of `{`/`,` delimiters (1024) (`asset_transport.rs:550-578`).
- usvg tests **every rule against every element** (`usvg-0.46.0 parser/svgtree/parse.rs:377-383`).
- simplecss resolves descendant combinators by **unmemoized recursive backtracking** over all ancestors (`simplecss-0.2.2 selector.rs:174-192`).
- Matching cost per element is about C(depth, k), where k is the number of descendant combinators. The only limit on depth is the preflight XML depth (≤ 63). One rule with zero commas passes both counters.

**Trigger.** `<style>x g g … g {fill:red}</style>` followed by D nested `<g>` elements. The `x` never matches, so every ancestor combination is explored.

**Measured** (release probe, verbatim pin code, 30 s CPU cap):

| Case | Bytes | Result |
|---|---|---|
| D24, k=8 / 10 / 12 | 310 / 314 / 318 | Admitted after 22.6 / 66.5 / 119.6 ms |
| **D32, k=12** | **374** | **Admitted after 7,252 ms** |
| **D40, k=12** | **430** | **Killed at the 30 s CPU cap** |
| **D60, k=30** | **606** | **Killed at the 30 s CPU cap** |

- Growth follows C(D, k). Extrapolated (not run): D40/k12 ≈ 3 minutes; D60/k30 is effectively unbounded.
- `sample` of the capped D40 process: 2253 of 2253 samples were `main → svg_tree → usvg Tree::from_xmltree → svgtree::parse… → simplecss Selector::matches_impl`, i.e. **before `budget()`** (`logs/28`).

**Reachability [SRC].**
- **Chat Markdown/reference images.** `DocumentEffect::Image` → `request_document_image` (`ai_chat/references.rs:420`) → `document_image` (`asset_transport.rs:216-225`). `data_uri` admits `data:image/svg+xml;base64` (`tezbar-core assets.rs:376-391`). Next: `decode_blocking` (`references.rs:609-622`, permit held inside `spawn_blocking`) on the **shared 4 AssetService slots** (`assets.rs:79,85-95`) → `decode_chat_preview` → `decode_svg` → `svg_tree`.
- **Extension assets.** `decode_asset` (`asset_transport.rs:202-214`). Its 10 s timeout drops only the waiter; the permit stays with the blocking job.

**Consequence.**
- Each such image pins one core and one shared permit until the process exits.
- Four of them in one model reply or prompt-injected tool output stall, until restart:
  - chat previews;
  - remote and extension images;
  - chosen attachments ("Image admission timed out").
- I observed no memory growth (peak footprint about 1.2 MB) and no crash.

**Fix.** Bound matching work in `svg_preflight`, before `Tree::from_data`:
- Parse the ≤ 32 KiB stylesheet; simplecss 0.2.2 is already locked.
- Estimate `elements × Σ_rules (max_depth+1)^(combinators in the rule)` and refuse above about 2²². Delimiter caps alone cannot bound this work.

**Tests.** `r2_desc_d60_k30` (606 B) refused in < 50 ms; `g > .a {…}` and `svg .cls {…}` still render.

### N-R1F — LOW–MEDIUM (functional, fail-closed regression introduced by the R1 repair) — ordinary clip+mask structures refused — [RUN]+[SRC]

**Callsite.** `budget()` charges `area × 3` for any group that has a clip or a mask, and passes that charge to every descendant (`asset_transport.rs:490-501,509`).

**resvg order [SRC].** `render_group` (`render.rs:62-104`) allocates the layer and renders the children, whose layers are freed on return. Only after that does it apply:
- the clip: `clip.rs:11` RGBA plus a 1 B/px `Mask`;
- the mask: `mask.rs:17` RGBA plus the 1 B/px alpha at `:22`.

So scratch is about 1.25 A and never coexists with child layers. The true peak for clip(A)→mask(A) is about 3.25 A, which is under the documented 4-canvas ceiling. The model charges 6 A.

**Measured [RUN].** Six shapes that decode at 0513321 are refused at the pin with "simultaneous layer memory budget" (`logs/20`, `22`, `23`):

| Synthetic case (pin result: refused) | Size |
|---|---|
| Figma-style outer `clip-path` frame + alpha `mask`, full-bleed 24 px (also at 64 px) | 472 B |
| Same structure, 20×20 content on 24 px | 472 B |
| Same structure, Material-style 18×18 rounded box on 24 px | 581 B |
| clip + 2 nested opacity groups, 24 px | 259 B |
| Nested masks, 24 px | 282 B |

- The 16×18 "home" glyph in the same structure is still admitted.
- The boundary flips on float noise: the 18×18 box gives `bounds.width() = 18.0000x` → ceil 19 → 6·23² = 3174 > 3136 (dump, `logs/21`).
- **Repo corpus:** the 12 tracked SVGs give the same results at old and new (10 admitted, 2 refused by both under the pre-existing paint-amplifier rule). None of them uses clip or mask.

**Consequence.** These icons and previews are missing in chat and in extension assets. How common these export shapes are in real Tezbar content is **unverified**; the fixtures are synthetic and Figma-style, not vendor assets.

**Fix.** Charge scratch only at the group's own peak:
- Children receive `live + area`.
- Check `live + area·(1 + s) ≤ limit`, with s = 1.25 (1.5 if you want margin) for clip/mask.
- Keep the 64/8192 caps and the auxiliary/chained rejections.

This remains a strict upper bound under resvg's allocation order.

**Tests.** The full-bleed Figma clip+mask and clip+opacity+opacity cases render; 5 nested full-canvas opacity groups and the 62-nested case are still refused.

### N-R4W — LOW (pre-existing; R4 family outside the claimed Create/Attach/Resume paths) — replay-read failure plus a live gap queues input silently — [RUN]

**Callsite.**
1. Replay `Err` (`state.rs:484-501`) releases the token and clears `replaying`, but leaves `running = true`.
2. The next durable live event with `start > position` sets `replaying = true` and calls `request_replay` (`:882-886`).
3. `request_replay` does nothing without a token (`:748`).

These lines are unchanged since 0513321 [SRC].

**Probe** `opus_fec3257_r4_family_characterizes_…`:
- After "Terminal history was truncated or modified" and one live chunk, typed `x` produces no Input and no error.
- 60 more keys then produce "Terminal history is loading; input buffer is full", while `running = true`.

**Consequence.** The view wedges in a misleading loading state until the session is reselected. It needs a non-expiry replay-read failure (history replaced, truncated or I/O) on a durable stream, which is rare.

**Fix.** When the gap branch has no replay token, re-attach as Resume (as the expiry path does), or end the stream visibly and discard pending input.

### N-R5A — LOW (documented ordering choice; non-gating) — alternate-screen wheel arrows can exhaust the 60-chunk FIFO during replay — [RUN]

**Callsite.** `view.rs:136-151` sends non-mouse alternate-screen arrows through `write_bytes` → `admit_input` (`state.rs:975-987`), one chunk per wheel event.

**Probe** (real GPUI `ScrollEvent`, replay held): 60 one-line events refuse the next typed `q` with "input buffer is full… not sent". The 60 arrows are delivered after the replay.

**Fix (optional).** Coalesce consecutive identical wheel-arrow chunks into the trailing pending entry, bounded by bytes. Keyboard ordering is preserved.

### D-1 — docs-only nit
`applied-files.json` is exactly the focused-1 snapshot, the state with the preserved `replaying` private-field compile error:
- `m810_review_regressions.rs` `df6f6e38…` versus the final `c1b29673…` (focused-3);
- `m810_review_pin.rs` `e2437ab1…` versus the final `4da9d6fa…` (focused-2).

It is not labelled as such, while `owned-source.json` (final) is correct. Add one line to `dispositions.md`.

### S-1 — standards nit
`rustfmt --check` passes, but rustfmt's overlong-literal bailout leaves four statements unformatted:
- `state.rs:234` (`self.error=Some(…)`, production);
- `asset_transport_tests.rs:350, 392, 407`.

Split the literals so rustfmt can format them.

### Observations (not counted)
- **O-SVG-CPU** (pre-existing bound) [RUN]. The admitted worst cases:
  - 8190 full-canvas half-transparent strokes (630 KB, every budget passed) take **12.36 s** of release CPU, 14.8 MB;
  - 3200 translucent full-canvas rects take 4.97 s.

  Four such decodes would also hit the 10 s attachment admission timeout. Consider a coverage-area budget.
- **O-R5P** [RUN]. In mouse mode a press during replay is dropped, and the release after replay is sent alone (`ESC[<0;1;1m`). Programs usually ignore an orphan release.
- **O-MSG** [RUN]. An isolating group inside a clipPath is refused with "SVG exceeds mask layer budget"; the message says mask, not clip.
- **O-PR-ERR** [SRC, unprobed]. A current-generation ProtocolReply `Err` clears `replaying` while replay continues. This is the same handler Write had at 0513321.

## 2. Per-ID verdicts (caller / source / evidence)

| ID | Verdict | Real caller and source | Evidence | Remaining |
|---|---|---|---|---|
| **R1** | Memory CLOSED (subset, macOS probe); **N-R1F** | `decode_chat_preview` (canvas ≤ 1024) / `decode_asset` (≤ 724 effective) → `decode_preview` → `decode_svg` → `svg_tree` (`asset_transport.rs:267-367`). Scale before budget `:380-384`. The area formula equals resvg's `ibbox` (`ceil+4`, clamp ≥ `fit_to_rect` 5W×5H). `Group::subroots` covers clip, sub-clip, mask, sub-mask and filter images (`usvg tree/mod.rs:1219-1245`). | [RUN] 62-nested, 5 nested full, chained mask, isolate-in-mask and clip-child-with-clip are all refused in < 0.1 ms. 4 nested full @1024 admitted: peak footprint 22.5 MB versus a 5.4 MB baseline (≈ +17 MB, consistent with the documented ≈ 16 MiB). 64 siblings × 4-canvas layers: 211 ms, 22.5 MB. Root 64-sibling and mask tests pass. | N-R1F; O-SVG-CPU; app RAM not measured |
| **R2** | **NOT CLOSED** | `svg_preflight` `:550-578` before `Tree::from_data` `:379` | [RUN] css60000 refused 0.73 ms; root comma, inline and ordinary-CSS tests pass. **N-R2X** reproduced, plus `sample`. | N-R2X |
| **R3** | **CLOSED** | `path_budget` `:397-451`. tiny-skia `dash_impl` iterates every interval with `dash_count += len·(n/2)/period` (`dash.rs:184-232`). ContourMeasure length ≤ control-polygon length, and the `+contours` term covers the phase and closed-contour extension. The dash is applied before transform (`painter.rs:342-351`). | [RUN] dash128 refused 0.23 ms; a 65,641-dash path refused; a 64,000-dash path renders in 5.07 ms; inherited dasharray over 3200 paths 5.26 ms; dashed circle renders. | — |
| **R4** | **CLOSED** for claimed paths; N-R4W outside them | `complete` Err `:472-478`, Create decode `:545-556`, Attach null/mismatch `:559-576`, Resume `:579-603`, missing identity `:693-699`, admission `create`/`attach`/`set_visible` `:288-299,384-405`, replay admission `:749-760`, tail `:652-654`, `admit_input` `:971-974` | [RUN] root 3×4 matrix passes. My probes: an Attach refused at admission after Create gives "…draining… Pending input was not sent.", `running = false`, no later queue. A Resume refused at admission gives a visible "draining" error, `running = false`, no later queue. | N-R4W |
| **R5** | **CLOSED** for pointer reports; N-R5A by design | `write_pointer_bytes` `state.rs:952-963`. The view calls it at `view.rs:45,81,103-104,146-147`. Dedup and accumulator reset on drop. Alt-screen arrows `:150` keep the FIFO. | [RUN] both root R5 tests pass (state, and real GPUI motion). My mouse-mode probe: press and wheel dropped during replay, the accumulator reset (no stale fraction later), key order kept. | N-R5A, O-R5P |
| **R6** | **CLOSED (logic)**; ConPTY UNRUN | `remember_startup` runs before the stale cleanup (`:443-447`). Watermark `origin.max(through).min(watermark)` (`:721-733`). Bounded 16 KiB batch with the reserved slot (`:805-839`; limits `:207-216`). Generation-agnostic dispatch (`:240-244`), retained by `cancel_queued` (`:188-192`), drained before quiescence (`mod.rs:245-247`). Retirement on consumption (`:848-850`), exit (`:900-906`), List (`:660-665`), mismatch (`:731`). Core returns `origin` on every open (`stream.rs:112`, test `again["origin"]==7`). No core or wire change. | [RUN] root 2 tests pass. My probes with **distinct per-session generations**: the stale Create keeps `(fresh, gen-fresh)` and queues Release+Detach; a's restored `ESC[6n` stays silent; fresh's reply carries `(fresh, gen-fresh)`, is dispatched **first** after navigation (before cleanup and a's Attach), and is never repeated. A split `ESC[6`/`n` is answered exactly once. The 65th own Create is refused visibly; an authoritative List retires stopped sessions. | Windows/ConPTY PLAUSIBLE, not run. Unselected new sessions stay unanswered by the side-effect-free policy. |
| **R7** | **CLOSED, docs-only** | Spec §5.5 O-TERM-KEYS, §5.7, ledger K-16/K-29 (still `todo`) match `input.rs:40-41` and the footer `view.rs:345-348` | [SRC] exact reads | Post-pin `8fc0f67` resolves O-TERM-KEYS and edits `input.rs`; not reviewed here |

The test-map claims hold [RUN]:
- P1/P2/F4 (macOS only), P3 and R5-motion use real GPUI dispatch.
- P4 exact 60 / 1 MiB; P9 flushed failure.
- P5/P7/P8 were rewritten as the R4/R5/R6 tests.
- The focused-1 error is preserved. `replaying` is still private (`state.rs:100`).
- M78 N2 is untouched: no `ai_chat` file is in `151df16..37164d5`.
- The Windows N3/N4 sources are untouched and remain UNRUN.

## 3. Standards, design and testing judgement

- **Design.**
  - The R4–R6 repair is coherent and small. There is one owner (the State actor), a bounded map (64), one reserved reply slot, and refusals instead of eviction. No wire or core change.
  - The magic `WORK_LIMIT - 4/-5` reserve deserves named constants.
  - The SVG budget keeps the right ordering: scale, then tree, then budget, then pixmap.
  - Its scratch model is too coarse (N-R1F), and its CSS gate measures the wrong quantity (N-R2X).
  - This is a bounded preview decoder, not a browser-grade SVG implementation, and that is honestly documented.
- **Tests.**
  - Good real-caller coverage for keys, wheel, pin and motion.
  - Gaps that my probes filled:
    - all synthetic sessions share `gen-a`, and no R6 test asserts the reply's `id`/`generation`;
    - no clip+mask nesting case;
    - no selector-complexity case;
    - no alt-screen wheel-during-replay case.
  - The R1 mask test asserts only `is_ok`, not pixels.
  - Windows Ctrl+Shift dispatch has no test, because Windows is uncompiled.
- **Standards.**
  - rustfmt passes but leaves S-1. `diff --check` is recorded clean [REC].
  - No new dependency.
  - The evidence is well hashed and honestly labelled, apart from D-1.

## 4. Recommended order for root

1. **N-R2X:** a CSS matching-work estimate in `svg_preflight`, with the 606-byte regression test.
2. **N-R1F:** scratch charged at the group's own peak, with Figma-style clip+mask render tests.
3. **N-R4W:** handle the tokenless gap with a re-attach or a visible end.
4. Optional:
   - N-R5A wheel coalescing;
   - D-1 one line;
   - S-1 literal splits;
   - O-SVG-CPU coverage budget.
5. Then a fresh SVG-lane recheck, plus real Windows ConPTY for R6.

## 5. Paths and integrity

Work root: `/tmp/tezbar-m810-r1r7-opus-fec3257/`, 641 MB (archive, tar, upstream crates, probes, logs).

| Path | sha256 prefix |
|---|---|
| `integrity/evidence-sha256.txt` (91 files: logs, probes, tools, integrity) | `c91ba07b2b30b5af` |
| `integrity/pin-ls-tree.txt`; `verify-before.txt` = `verify-after.txt` (2921/0/0) | `3726ce9226c93578`; `b8ebb272850efbea` |
| `integrity/test-bin-pristine.sha256` (pristine test binary `2c2278d0…`) / `test-bin-probe.sha256` (`88032885…`) | `1893f16885f0d45f` / `5a915c260b1eaa72` |
| `logs/33-native-pristine-focused.log` (81/81) / `logs/34-probe-run-1.log` (8/8) | `97703d61043ffff9` / `33ecd41e8b681ccf` |
| `logs/20` admission / `23` ordinary new / `24` repo corpus / `25` bounded renders | `7bf9a10d1ace59c0` / `aaa5e997b627a883` / `92ee5017c5807f49` / `52e5c263deb512df` |
| `logs/26`, `27` selector series / `28-svg-r2-sample.txt` / `29` CPU strokes | `afddfb5ee68eccae`, `7a9407df27539f4a` / `f068fc345fcc3adf` / `5f995bbea31b1e80` |
| `logs/40` evidence ledger / `42` otool / `43` rustfmt (empty) | `ab09a237a20a87f7` / `f33f21266223f6df` / `e3b0c44298fc1c14` |
| `probes/svgprobe/src/verbatim.rs` (pin 343-649), `main.rs`, `Cargo.lock`, release binary | `8c13126092411078`, `863a2e2831821422`, `9ad66d7c099bdd07`, `6744784cdb42bdc0` |
| `probes/terminal/opus_fec3257_probe.rs` (8 probes), `tools/gen_cases.py` (regenerates `probes/svgcases/`) | `79fa4e89b43a6505`, `1e17f3994e66ef31` |

Final state:
- The original reports (`a44f9595…`) and the preserved binary (`5ec21f89…`) are unchanged.
- The root evidence (43/43) and the prior review artifacts (44/44) are unchanged.
- No processes of mine remain. This report was not copied into the repository.
