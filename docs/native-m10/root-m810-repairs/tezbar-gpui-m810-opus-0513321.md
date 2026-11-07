# M10 F1–F8 and narrow M78 N1–N4 closure review @ 0513321

Reviewer: Claude Opus 5.5, working alone with no subagents. Date: 2026-11-07.
Pin: `05133210b78f125b83ffdf2482e09e72533ff219` on `feat/native-gpui-app`, worktree `Tezbar-native-rust-agent`.
Scope: M10 F1–F8, the panel/bench/mailto follow-ups and the three replay tests; M78 N1–N4 and the second repair. M11 is excluded. Settings and M7 are not reopened.

## Verdict

**NOT ACCEPTED, and this review does not accept the full migration.**

- **M10 terminal**
  - F1, F2, F3, F4, F5 and F7 are closed at their real callers on macOS.
  - F6 is closed for the logic layer. Real ConPTY was never run, so the platform remains blocking. One residual remains (R6).
  - F8 is partial: two new LOW gaps (R4, R5).
  - The panel, bench decoder, mailto and replay-test follow-ups are closed.
- **M78**
  - N2 is closed.
  - N3 and N4 are source-closed only. Nothing was compiled or run on Windows.
  - **N1 is not closed.** Markers and all three filter routes are now refused, but three other amplifiers bypass every new bound (R1–R3, all confirmed by probe):
    - nested isolated layers, about 6 GB from a 1.4 KB SVG;
    - a stylesheet with quadratic parse cost;
    - dash expansion.

New actionable findings: 7 in total.

| Severity | Count | IDs |
|---|---|---|
| MEDIUM–HIGH | 1 | R1 |
| MEDIUM | 1 | R2 |
| LOW–MEDIUM | 1 | R3 |
| LOW | 4 | R4, R5, R6 (Windows impact plausible), R7 (docs) |

No Windows, ConPTY, GUI, physical keyboard, IME, AX, GPU or RAM acceptance is claimed. I have no screenshots and made no manual runs.

## 0. Integrity and method

- **Archive.** `git archive 0513321` went to `/tmp/tezbar-m810-opus-0513321/src`, outside the live tree.
  - `git ls-tree -r` lists 2415 blobs, all mode 100644/100755, with no symlinks or submodules.
  - My `verify_blobs.sh` compares `git hash-object --no-filters` against every blob. Before: 2415 checked, 0 mismatches, 0 extra files. After the probes were removed: the same.
  - ls-tree sha256 `e2f3f7ea…1b8d`. Archive tar after cleanup `99f3a842…4a53`.
- **Live worktree.** I never edited, built in, staged, committed, branched, fetched or pushed it, or switched its HEAD.
  - HEAD is still `0513321` on `feat/native-gpui-app`.
  - **Note:** during this review the live tree gained M11 changes (32 status entries, 20 tracked files). An M11 `docs/native-m11/integration/run-gate.py` cargo job (PIDs 88552, 88553, 94892) is running there. These are not mine and I did not touch them. None of my evidence reads live files: PIN facts come from the archive or git objects.
  - Upstream references came from the cargo registry and the git checkout: resvg/usvg/tiny-skia/simplecss sources and gpui `c84af5b`. Bundled xterm.js 6.0.0 under `node_modules` was used as an upstream reference only.
- **Toolchain.** Rust 1.98.1, `--locked --offline -j2`, `CARGO_INCREMENTAL=0`. Native builds used `--features runtime-shaders[,bench-control]` with `TEZBAR_NATIVE_SOURCE_SHA=<PIN>`.
- **Private target.** An APFS `cp -c` clone of live `target/debug/{.fingerprint,build,deps}`. No live build directory was used. When I finished I deleted the private target and the probe target, which released about 3.3 GB of real space.
- **XERJ** was not attempted. It is known to be stale; exact `git`/`rg` reads were authoritative.
- **Processes.** None of mine remain: no svgprobe, cargo or test binary. The PTY fixture process lists before and after are identical (empty).
- **Fixtures I read before running:**
  - Core `m10_same_owner_private_pty_binary_input_replay_and_shutdown` runs explicit `/bin/sh -c "printf 'M10_READY\n'; exec /bin/cat"`, non-login, with tempdir data/profile/home/cwd. Cleanup is kill + `wait_stopped` + `shutdown_all` Drop + a check that the PID is gone.
  - N4 `review_n4_…` uses `CoreClient` over tempdir `CorePaths` and a `TestHost`. Its artifact URL is `example.invalid` and is never fetched.
- **Not used:** real profiles, clipboard, Keychain, credentials, providers, network stores, GUI, remote or Windows.

### Recorded results versus my reruns

| Gate | Parent-recorded | My rerun on the archive |
|---|---|---|
| native full | 360 | not rerun. 360 listed: 49 + 311 filtered |
| bench full | 362 | not rerun. 362 listed; the bench panel filter passed **2/2** (`70-…`) |
| `surfaces::terminal::` | 49 (`23-root-terminal.log`) | **49 passed** (`31-…`) |
| `command_bar::terminal_integration` | 4 | **4 passed** (`32-…`) |
| `app::m10` / `windows::terminal_sessions` | 1 / 1 | **1 / 1** (`33-`, `34-…`) |
| core `terminal::pty::stream` | 5 | **5 passed** (`61-…`) |
| core private PTY fixture | (M10 original) | **1 passed**, no leftover process (`62-…`) |
| `review_n2_` / `review_n1_` / `review_n4_` | 4 / 3 / 1 (second repair) | **4 / 3 / 1** (`35–37`) |
| `asset_transport` / `review_closure` / `repair_quit_preparation` | — | **12 / 7 / 1** (`38–40`) |
| Pristine recheck after probe removal | — | **55 passed** (`71-…`) |
| Scoped rustfmt `--check` | owned 51 pass | 12 app files (edition 2024) and 4 core files (edition 2021): **0 diffs** (`80-…`) |
| `git diff --check b739b4e..PIN -- apps crates` | pass | **0 lines** (`81-…`) |
| UI 57 / model 380 / TS 745 + 7 skips / MSRV / normal check/build / 9 SDK scenarios / dependency and linkage audit | recorded | **not rerun**; accepted as recorded only |

The original reports are unchanged:
- `docs/native-m10/reviews/tezbar-gpui-m10-opus-b739b4e.md` = `3e358161…f7fb5a`. It equals `/tmp/tezbar-gpui-m10-opus-b739b4e.md` and is touched only by 0513321.
- `docs/native-m08/reviews/OPUS_b739b4e.md` = `feca7735…f269`. It equals `/tmp/tezbar-gpui-m78-opus-b739b4e.md` and is touched only by 399c63e.

The root patch hashes match the M9 receipt:
- `m10-root-repairs.patch` `9a5e5554…` (15 files)
- `compile-corrections-v2` `3a5de7d8…`
- `bench-wire-v3` `a5f277dd…`

## 1. New actionable findings (most severe first)

### R1 — MEDIUM–HIGH — N1 not closed (decode path preexisting; repair bound does not cover it) — confirmed by probe
**Nested isolated layers allocate up to 25× the canvas area, all at once. A 1.4 KB SVG reaches about 6 GB.**

- **Code:**
  - `asset_transport.rs:369` `budget()` caps the *count* of `should_isolate()` groups at 64, and the preflight XML depth guard allows nesting up to 62.
  - resvg 0.46 `render.rs:64-104` allocates every isolated group's layer as `fit_to_rect(bbox, max_bbox)`, where `max_bbox` is 5W × 5H (`lib.rs:40-46`). The layer stays alive while its children render, so nested layers coexist. Mask and clip add further layer-sized pixmaps (`mask.rs:17`, `clip.rs:11`).
- **Trigger:** `<svg width="1024" height="1024">` + 62× `<g opacity=".99">` around one `1e6×1e6` rect. The input reaches `decode_chat_preview` (canvas ≤ 1024) through model or prompt-injected Markdown data:/HTTPS images, or `decode_asset` (≤ 724 canvas) through extension assets.
- **Probe:** release build, verbatim PIN `decode_svg` + `svg_preflight`, log `11-svgprobe-run.log`. Peak footprint scales exactly with nesting depth × canvas area:

  | Case | Peak footprint |
  |---|---|
  | 62 layers @128 px | 104 MB |
  | 16 @512 | 412 MB |
  | **4 @1024** | **383 MB, about 95 MB per layer** |
  | 63 layers | refused by preflight |

  - **Projection:** 62 layers @1024 ≈ **5.9–6.5 GB** for one image; extension assets ≈ 3 GB. Up to 4 such decodes can run concurrently, which can kill the app on 8–16 GB machines.
- **Fix:**
  - Compute `scale` from `tree.size()` *before* `budget()`.
  - Pass the canvas W×H into `budget()`.
  - For each isolated group, take its area as `layer_bounding_box()·scale` (+4 px), clamped to 5W × 5H. Double it if a mask or clip path is present.
  - Keep a running sum along the current isolation stack and reject when it exceeds about 4·W·H. At 1024² that is 64 MiB.
  - Keep the 64 total-layer cap for CPU.
- **Tests:** `nested_opacity_62_1024` rejected in under 50 ms; 64 sibling translucent icons still render.

### R2 — MEDIUM — N1 not closed (preexisting; outside the new bounds) — confirmed by probe
**`<style>` parsing in `Tree::from_data` has superlinear (about quadratic) CPU cost in the number of rules, before `budget()` runs.**

- **Code:** `svg_preflight` bounds `<` count, ids, `d`/`points` bytes, `<use>` expansion and amplifier element names. Nothing bounds stylesheet text.
  - usvg `parser/svgtree/parse.rs:627-650` parses every `<style>` element through simplecss `parse_more`.
  - Then usvg applies each rule to each element (`parse.rs:377`).
- **Measured** (rules dominate; node count barely matters):

  | Rules | Size | Time |
  |---|---|---|
  | 5k | 98 KB | 0.11 s |
  | 20k | 0.35 MB | 1.12 s |
  | 60k | 1.0 MB | 9–12 s |
  | **120k** | **2.05 MB**, inside the extension `ASSET_BYTES` of 2 MiB | **42 s** |

  - **Projection:** at the 8 MiB chat source limit, about 480k rules ≈ 11 minutes of release CPU per image.
- **Consequence:** each decode holds one of the **4 shared** `AssetService` permits for its whole duration. Four images starve chosen attachments (10 s admission gives "Image read timed out"), remote and extension images, and chat previews. This is the same class as the original M78 N1 CPU case, and it contradicts "no CSS amplification bypass".
- **Fix:** in `svg_preflight`, sum the text of `style` elements plus all `style` attribute values. Refuse above 32 KiB, or above about 1024 `{`, with "SVG stylesheet exceeds budget".
- **Tests:** a 60k-rule case is rejected in under 50 ms. The existing `<style>rect{filter:…}` case stays rejected.

### R3 — LOW–MEDIUM — N1 not closed (preexisting) — confirmed by probe
**`stroke-dasharray` expands up to 1,000,000 dashes per path at render time, after the parsed 16,384-segment budget.**

- **Code:**
  - tiny-skia-path 0.11.4 `dash.rs:199` (`MAX_DASH_COUNT = 1000000` per contour).
  - resvg `path.rs:113` `stroke_path` dashes the source path.
  - `budget()` counts only the source segments.
- **Measured:**

  | Case | Time | Peak |
  |---|---|---|
  | `M0 y H99999` with `stroke-dasharray=".1"`, 64 paths | 0.70 s | 515 MB |
  | 128 paths | 1.36 s (10.6 ms per path) | 523 MB |

  - **Projection:** at the 8,190-node limit, about 87 s of CPU per decode while it holds a permit.
- **Fix:** in `budget()`, for each `Path` whose stroke has a dasharray, estimate dashes as Σ control-polygon length × scale ÷ (period × scale) × (len/2). Cap the total at about 65,536. Alternatively, refuse periods under 1 device pixel.
- **Tests:** `dash_128` rejected quickly; an ordinary dashed icon still renders.

### R4 — LOW — new with F8 — confirmed by probe P5
**Input buffered while a Create or Attach is in flight is kept, and never reported as not sent, when that Create or Attach fails.**

- **Code:**
  - `state.rs:412-441`: the `Err` branch sets `replaying=false`, but only the Write and Replay purposes discard pending input.
  - The same gap exists for a null Attach (`:495-501`), a Create decode error (`:484-493`) and a missing stream identity (`:606-609`).
  - `admit_input` (`:821-834`) then keeps queueing behind the non-empty buffer.
  - `flush_pending_input` (`:869-871`) never runs, because `running` is false.
  - A later `detach_active` clears everything with no message.
- **Trigger:** type during Create/Attach of a session whose Attach or Create fails, for example "failed to start shell", a core error, or a session that disappeared.
- **Probe:** after the failure, the error is only "attach failed" / "failed to start shell". Later keys are queued silently. 0 inputs are ever sent.
- **Fix:**
  - In those failure paths, call `discard_pending_input()` when no stream remains.
  - In `admit_input`, if `!replaying && !running` and the buffer is non-empty, discard it with an error.

### R5 — LOW — new with F7 and F8 interplay — confirmed by probe P7
**Pointer reports (motion, press, wheel) share the 60-entry replay buffer. During replay of a mouse-mode session they exhaust it, and the next keystroke is refused.**

- **Code:** `view.rs:95-104` and `:136-138` call `state.write_bytes`, which goes to `admit_input` (`state.rs:821-833`).
- **Trigger:** select or restore a running session whose history enabled 1003/1002 mouse tracking, with a history long enough to take several round trips. Hover across 60 cells.
- **Probe:** 60 deduplicated motion reports are buffered. The next key gets "input buffer is full… This input was not sent." The stale coordinates are flushed later.
- **Fix:** do not buffer positional mouse reports while `replaying`; drop them, because the program has not seen the screen yet. Alternatively, coalesce a trailing motion entry in `pending_input`. Keystrokes and IME commits keep the FIFO.

### R6 — LOW (Windows impact PLAUSIBLE, not run) — F6 residual — confirmed at the logic layer by probe P8
**An own Create that completes after a generation switch is detached as stale and never recorded as fresh. A later attach to that new process suppresses its startup DSR.**

- **Code:**
  - `state.rs:393-410`: the stale completion path detaches but does not record `own_create`.
  - `:632-643`: origin applies only to the immediately following own attach.
- **Trigger:** New session, then ⌘1–9, a panel selection or Back+reselect before the Create reply. Later select the new session.
- **Probe:** replies to the `\x1b[6n` at origin 0 are **0**.
- **Consequence:** on ConPTY with `PSEUDOCONSOLE_INHERIT_CURSOR`, that session's startup query is answered by nobody, so it hangs or waits for conhost's timeout. macOS shells are unaffected.
- **Fix:** let core own "first attach since create". `ReplayRegistry::open` returns `origin` only on the first stream attach of a generation (a flag on `Stream`), and the UI replies from origin whenever `origin` is present. Alternatively, remember stale own-Create `(id, generation)` until its first attach.
- **Verify on real Windows.**

### R7 — LOW — documentation honesty
The implementation diverges from the spec, and the decision is recorded only in `docs/native-m10/parent-repairs/HANDOFF.md`:
- **Spec §5.5** says "Windows uses Ctrl". **Spec §5.7** and ledger **K-16/K-29** still list ⌘K/⌘X/⌘P/⌘D only.
- The code makes macOS Ctrl+letter shell bytes and makes Windows default to Ctrl+Shift+K/X/P/D. The footer says "Configure Ctrl+Shift+K".

No document claims the Windows choice is decided, so the policy was not silently rewritten. Still, spec readers see a contradiction.

**Fix:** add an open decision, for example O-TERM-KEYS, to §5.5 and to rows K-16/K-29. It should state the macOS ⌘-only rule (Ctrl goes to the shell), the Windows Ctrl+Shift default, and that the user preference is pending. Physical keyboard acceptance stays manual.

### Observations (not counted)
1. **Input during the boot `List` round trip is dropped silently** (probe P6). The old TS app did the same (`TerminalView.tsx onData: if (!sessionId) return`), so this is parity and not a defect.
2. **Error text when a write fails after flush.** A transport `Err` on a flushed Write cancels the remaining queued writes. The error shown is only the transport text, without the "remaining input was cancelled" wording that the `false` reply uses (P9). Cosmetic.
3. **Delayed cwd retry.** A command submitted from the replay buffer schedules its cwd retry at the next keystroke, because `track_cwd` runs before `pump` flushes.
4. **Orphan pin ids.** A command-bar `>` delete does not remove the id from `terminal_pinned_sessions`. Harmless: deltas cannot resurrect it. Preexisting.
5. **Dropped effects under a stale mount.** Effects are dropped if the mount changes before relay delivery, for example ⌘P then ⌘2 in the same tick. This is the preexisting effect design.
6. **⌘+arrow.** On macOS ⌘+arrow sends plain arrows, where TS sent nothing. Preexisting nit.
7. **No Option-as-Meta preference.** None exists. The default behaviour equals TS `macOptionIsMeta:false`. Optional feature.
8. **SVG text renders no glyphs** (empty usvg fontdb). Preexisting functional limit.
9. **Case-sensitive N4 compare on Windows.** It compares exact text case, so a differently-cased registry path fails closed and the icon is missing. Unproven.
10. **Queries emitted while hidden.** Device queries a process emits while hidden or detached are never answered on reattach. This is the stated "reattach side-effect-free" policy.
11. **Quit during a long replay.** Quit drain waits for an in-flight replay inside the 8 s budget. This was accepted in the earlier review and remains unmeasured for long histories.

## 2. M10 per-ID verdicts (caller / source / probe)

| ID | Verdict | Real caller and source | Evidence: recorded / my run / my probe | Remaining |
|---|---|---|---|---|
| **F1** Ctrl bytes on macOS; explicit Cmd actions | **CLOSED (macOS)** | `TerminalView::key` (`mod.rs:385-422`) → `input::key` (`input.rs:40-41`: macOS primary = ⌘ only; app actions = ⌘, or Ctrl+Shift off macOS) → control table `:136-146` | The test now encodes the correct expectation per platform (`key_sequences…`, `command_control_numbers…`, `review_m10_f1…`). **P1, real GPUI window dispatch:** ctrl-d/x/k/p send `[4],[24],[11],[16]`, with no kill/delete/configure/effect. ⌘P → `TogglePin("a")`. ⌘D → `terminal:delete`. | Windows Ctrl+Shift default is the root's recommended default, user preference pending (R7). Windows is uncompiled. No physical keyboard proof. |
| **F2** Option text, dead keys, IME | **CLOSED (macOS)** | `input.rs:79-81` returns `PlatformText`, so propagation is not stopped. gpui_macos `window.rs:2737-2778` then calls `inputContext handleEvent`, which leads to `insertText`. Dead keys (key_char None) take the marked-text branch at `:2704-2734`. | Recorded `review_m10_f2` calls the handlers by hand. **P2, real `Window::dispatch_keystroke`:** Option+l/1/8 with key_char `@`/`¡`/`[` commit exactly `@`, `¡`, `[` once each, with no ESC. xterm.js 6.0.0 sends `ESC[1;3D` for Option+Left, matching native. | Real layout, dead-key and IME matrix is manual. There is no Option-as-Meta preference; its absence equals the default false. |
| **F3** wheel | **CLOSED** | `view.rs:107-151`: pixel delta y → `WheelAccumulator` (`input.rs:246-263`, fractional, ±128 clamp) → `scroll()` `:270-293`. Mouse mode sends one report per line. ALT_SCREEN+ALTERNATE_SCROLL without mouse sends APP_CURSOR-aware arrows. Shift bypasses. Zero-y adds nothing. | Recorded encoder/accumulator test. **P3, real `ScrollEvent` through the cells hitbox:** three sub-line events (⅓h, ⅓h, 0.4h) produce exactly one `ESC[A` in the alternate screen. A horizontal 120 px swipe in SGR mouse mode sends nothing. −2.5h sends two `ESC[<65;1;1M`. | Trackpad feel and momentum are manual. |
| **F4** single ordered pin owner | **CLOSED** | Terminal `pin`/`delete` emit `TogglePin`/`Unpin` (`state.rs:1002-1015`) → relay → `Owner::m10_effect` (`app/m10.rs:14-20`, 66-76; accepted source, not quitting) → `apply_terminal_pin_change` (`terminal_integration.rs:227`, merged into the live ordered list) → `persist` → `sync_terminal_pins` (`mod.rs:1029`, `:219`). The command bar's ⌘P edits the same list and persists. | The recorded two-surface test bypasses both real callers. **My F4 probe:** real ⌘P in the mounted terminal produces exactly one `TogglePin("a")` with an **accepted** source; merging with the command-bar's live `["b"]` gives `["a","b"]`. Real ⌘D gives `Unpin("a")`, accepted, and leaves `["b"]` with no resurrection. | `Owner` itself is source-traced, not instantiated. |
| **F5** hide before the Create/Attach reply | **CLOSED** | `detach_hidden` (`state.rs:278-284`) sets the flag only when a detach was really enqueued. `install_stream` → `detach_hidden` (`:648`). | Recorded `review_m10_f5` covers Attach-first and Create-first, and asserts exactly one `DetachGeneration` that is not repeated on dispose. I reran it. | — |
| **F6** origin / ConPTY startup | **CLOSED at the logic layer; platform blocking** | Core `Stream::new` sets `origin = position` *after* the restore seed (`pty.rs:609-637`, `stream.rs:31,112`). The UI uses origin only for `!resume` with a matching own `(id, generation)` (`state.rs:632-643`). Restored prior bytes, other owners and every Resume stay suppressed. TerminalView is the only native session creator. | Recorded core and UI tests; I ran core stream 5/5. **P8** shows the residual R6. | **Real Windows/ConPTY UNRUN.** The path is sufficient for the normal own-Create flow, including Create while hidden, because replay is not gated by visibility. It is insufficient for R6. |
| **F7** motion dedupe | **CLOSED** | `MotionReporter` (`input.rs:294-323`) keys on generation, cell, button, modifiers and mode. It resets on down/up/hide and on refusal. | Recorded test. P7 shows the dedupe holds (60 distinct cells give 60 reports). | Interplay with F8: R5. |
| **F8** bounded replay input | **PARTIAL** | `admit_input` / `flush_pending_input` / `discard_pending_input` (`state.rs:817-886`). Protocol replies during replay bypass the buffer (`:707-721`). Generation retirement clears via `detach_active`. Write `false`/`Err` cancels the queue. Quit fences admission. | Recorded 4 tests. **P4:** exactly 60 chunks admitted and the 61st refused visibly; exactly 1 MiB admitted; a whole paste refused atomically; FIFO kept. **P9:** a flushed Write failure cancels the rest. | **R4**, **R5**. Long-history RAM/latency gate not measured. |
| Old 3 replay tests | **CLOSED** | `tests.rs:85-127` deliver a positioned event after Attach while replay is pending, then the same-position duplicate | My terminal run 49/49 | — |
| Panel: typed rows, active, running, ⌘N, title | **CLOSED (headless)** | `terminal_panel_rows` builds typed `SessionRow`s (`terminal_integration.rs:166-189`). The panel (`windows/terminal_sessions.rs:83-205`) shows the header count, "New session · ⌘N/Ctrl+N", ⌘/Ctrl+1–9 hints, a `ListBoxOption` row whose `aria_selected` equals active, and a cwd subtitle. The window title is "Terminal Sessions" (`app.rs` panel `set_window_title`). | Recorded and rerun `review_m10_panel…` (4/4 controller) | Parenting, blur, multi-monitor and VoiceOver are manual. |
| Bench decoder | **CLOSED** | `fixture_rows` is an untagged enum: legacy `[id,title]` tuples or objects with defaults | **Bench filter 2/2 rerun** (`70-…`); malformed input rejected | — |
| mailto | **CLOSED** | `allowed_url` accepts `mailto:` and refuses control characters (`app/m10.rs:79-82`). Spec §5.7 lists http/https/mailto. | app::m10 1/1 | — |

### Preserved protections (rechecked by source, unchanged since the original M10 review)
- **Single PTY and transport owner:** one `CoreClient` and no new runtime. Sessions are created only through TerminalView.
- **Internal routes:** `core:*` stays internal.
- **Stream bounds:** 64 KiB reads with positions. The UI validates `encoded ≤ 87384` and `bytes ≤ 65536`.
- **Exits and disk truth:** exit only after join. Durable, truthful disk state (the core stream tests pass).
- **Lock order and back-pressure:** unchanged.
- **Write budget:** 1 MiB (`WRITE_BUDGET`) with the 60/64 work limit.

The F8 buffer adds no new channel and no unbounded state. Its caps are 1 MiB and 60 entries.

## 3. M78 per-ID verdicts

| ID | Verdict | Caller and source | Evidence | Remaining |
|---|---|---|---|---|
| **N1** | **NOT CLOSED** | `decode_svg` / `svg_preflight` (`asset_transport.rs:343-499`). Shared by chat Markdown (`decode_chat_preview`, 1024) and extension `decode_asset` (≤ 724 effective). | **Closed parts, probe-confirmed:** `<marker>`, including namespace-prefixed forms, is refused before usvg allocates. Filter via attribute, `style` and `<style>` is refused after parse and before render. `<use>`, cycle, depth and duplicate-id bounds still work, and a valid `<use>` icon renders. Recorded and rerun `review_n1_` 3/3. | **R1, R2, R3.** |
| **N2** | **CLOSED** | `finish_prepared` (`ai_chat/runtime.rs:465-490`) defers `(session, token, canonical, selection)` while `quit_preparing`. `revive_after_quit` replays once, deferred, with token and quit rechecked (`ai_chat/mod.rs:995-1008`). Dispose drops it (`:1016`). A non-durable deferral refuses quit (`:959-964`). `PreparationConfig` performs only the store drain, a local `chat:config` and the durable `Append`: no provider, no tool. | **Rerun `review_n2_` 4/4.** The tests assert `agent:run` counts (0 during prepare, 1 after revive, still 1 after a double revive; re-entered quit 0 → 1; failed append gives 0 runs, quit `Err`, draft restored). | The App-level quit/cancel caller is source-traced only (as in the M78 H1 trace). |
| **N3** | **SOURCE-PLAUSIBLE; Windows UNRUN (platform-blocking)** | `read()` (its only caller is `chosen_image`, from the picked-file asset transport) → Windows `open_chosen` (`assets_windows.rs:76-94`). It requires a rooted Disk/VerbatimDisk/UNC prefix (not `pipe`), uses an ordinary `File::open` that follows aliases, cloud placeholders and `~`, then checks `is_file` and `GetFileType==DISK` on the handle. Strict `open_locked` remains for scoped reads. | The Windows-only test is not compiled on macOS. | OneDrive hydration latency, a UNC stall holding a permit (accepted M3 residual), READ-share ancestor contention for scoped reads. All need real Windows. |
| **N4** | **SOURCE-CLOSED for authority; Windows UNRUN** | `asset_scope.rs:38-50` grants only when `canonical_spelling(candidate) == canonicalize(candidate)` (text-only verbatim conversion, so aliases are not followed). `authorize` (`assets.rs:33-55`) re-checks exact spelling against the canonical path. | **Rerun the real-`CoreClient` fixture 1/1** (macOS: both "spellings" are the same path). | Ordinary drive-letter spelling and case behaviour (Obs. 9) need real Windows. |

## 4. Ledger rows (T/L/S/K/W)

No row is green. Every row stays `todo` in the ledger, which is honest.

| Row | Status | Change since the original M10 review |
|---|---|---|
| T-01, T-03 | PARTIAL | F6 logic is closed; ConPTY is unrun |
| T-02 | PARTIAL | F5 and F8 closure, residuals R4/R5 |
| T-05 | PARTIAL | The F1 destructive Ctrl+D/X is gone on macOS; keep-alive is fixed by F5 |
| T-06 | PARTIAL | Was FAIL; F4 closed |
| T-08 | PARTIAL | Was FAIL; F1/F2/F7 closed; R5; real AltGr/keypad/dead keys are manual |
| T-10 | PARTIAL | F3 closed; mailto aligned |
| T-11 | PARTIAL | Real IME matrix is manual |
| T-12 | PASS (local layer) | — |
| T-13 / S-18 | PARTIAL | §5.8 info restored; parenting, blur and AX are manual |
| T-14 | PARTIAL | — |
| L-17 | PARTIAL | — |
| S-15 | PARTIAL | — |
| K-16 / K-29 | PARTIAL (macOS) | Windows decision is pending (R7) |
| W-06 / W-09 | PARTIAL | — |

Standing blockers for every row:
- **Manual gates:** Windows, ConPTY, physical keyboard, IME, VoiceOver/Narrator, GPU and RAM/long-history.
- **Pending before full migration:** M11.

## 5. Test-map, spec, design and standards

### Test map
The recorded F1/F2/F3/F4 tests sit below the real caller:
- F1 is tested at `input::key` level.
- F2 calls `key()` and `replace_text_in_range` by hand.
- F3 is tested at encoder level.
- F4 bypasses both the terminal emission and the relay.

My P1/P2/P3/F4 probes close those gaps with real GPUI dispatch and the real relay, and they pass. Root should promote them as permanent regressions; the probe source is at the path below. The F5/F6/F8 state-level tests are appropriate for state logic. The case map's "assertion subset" wording remains accurate.

### Spec honesty
- The Windows terminal shortcut contradiction is R7.
- Spec §5.7 still says "app shortcuts ⌘K/⌘X/⌘P/⌘D", which is correct for macOS only.
- The ledger never claims green.
- `HANDOFF.md` states plainly that native physical keyboard acceptance is not claimed.

### Design
The panel and terminal use flat sheet, header and footer controls with existing tokens:
- The active row uses the shared `virtual_list::row` accent `#4f52d9`, which §5.6 permits, with white text. The panel subtitle hardcodes `rgb(0xffffff)` to match the shared row instead of using a token. That is a nit and consistent with existing code.
- The copy is specific: "Configure ⌘K   Switch sessions ⌘1–9   Back Esc / ⌘[", "Terminal sessions · N running", "New session · ⌘N" and "Starting shell…".
- There are no gradients, glass, side stripes, badges or icon grids.
- No visual acceptance was made, and there are no screenshots.

### Standards
- The scoped rustfmt check is clean, and `diff --check` is clean.
- No new dependency: the probe crate used the PIN lock versions resvg/usvg 0.46.0, tiny-skia 0.11.4 and simplecss 0.2.2.
- `alacritty_terminal` 0.26 is Apache-2.0.
- The no-WebKit/Tauri linkage, MSRV and dependency audit are recorded results only; I did not rerun them.

## 6. Recommended order for root (all small)

1. **R1:** area-bounded isolation in `budget()`, using scale before budget.
2. **R2:** a stylesheet byte or rule cap in `svg_preflight`.
3. **R3:** a dash estimate cap in `budget()`.
4. **R4:** discard pending input visibly when no stream remains.
5. **R5:** do not buffer positional mouse reports while replaying.
6. **R6:** core-owned "first attach since create" origin, then verify on real Windows/ConPTY.
7. **R7:** record the terminal keys decision as an open item in the spec and ledger.

Promote probes P1–P3 and F4 as tests, and add regression tests for R1–R5. Then get a fresh Opus recheck of the N1 SVG lane and the F8 failure paths.

## 7. Paths and integrity

The work root is `/tmp/tezbar-m810-opus-0513321/` (308 MB after cleanup; the private build caches were deleted).

| Path | sha256 prefix |
|---|---|
| `integrity/pin-ls-tree.txt` | `e2f3f7ea355c8b4c` |
| `integrity/verify-before.txt` = `verify-after.txt` (2415/0/0) | `910e963e3489c75f` |
| `integrity/archive-tar-after.sha256` | archive `99f3a842a5f68e2e` |
| `integrity/evidence-sha256.txt` | the full inventory of every file below |
| `logs/31…40-pristine-native-*.log` | terminal `bab47b99`, controller `3a5620c5`, m10 `29973c58`, panel `74ead423`, n2 `ceb01f97`, n1 `afaff574`, n4 `e015380c`, assets `721982c5`, closure `6b0bdf21`, quit `c5b3930e` |
| `logs/61-core-stream.log`, `62-core-private-pty.log` | `0451862a`, `db8cecce` |
| `logs/70-pristine-bench-panel.log`, `71-pristine-native-recheck.log` | `40d090df`, `dbb5d6b8` |
| `logs/50-probe-run-1.log` (**preserved failed attempt**: my probe's missing-lifetime compile error), `51-…`, `52-probe-run-3.log` (10/10 probes) | `4ea0df62`, `18131af4`, `2b876cd9` |
| `logs/11-svgprobe-run.log` (all SVG measurements), `10-svgprobe-build.log` | `ecb76d77`, `e6c48e97` |
| `logs/80-scoped-rustfmt-check.log`, `81-diff-check.log` | `92c227aa`, `e3b0c442` (empty) |
| `probes/opus_m810_probe.rs` + `.diff` (terminal P1–P9) | `c5c83f5f`, `f0945027` |
| `probes/opus_m810_f4_probe.rs` + `.diff` | `3c4c9c60`, `e5925535` |
| `probes/svgprobe/src/verbatim.rs` (PIN `asset_transport.rs:343-501`, verbatim), `main.rs`, `svgprobe-release-bin` | `25947cb8`, `35537e29`, `5c4ac39d` |
| `tools/gen_svg{,2,3,4}.py` (regenerate `probes/svgcases/`), `tools/verify_blobs.sh` | `31e03809`, `1074f2a9`, `f81a1a8b`, `d808fbfd`, `f70547c7` |

The probes were appended only to the archive copies of `terminal/tests.rs` and `command_bar/terminal_integration_tests.rs`. Both were restored byte-for-byte (blobs `623d6038…` and `01b7f46e…`) before the after-verification and the pristine recheck.
