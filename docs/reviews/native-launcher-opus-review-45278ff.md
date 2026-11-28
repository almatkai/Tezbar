# Native launcher prototype — Opus 5.5 critical review

Pin: worktree `Tezbar-native-rust-agent`, branch `feat/native-launcher-prototype`, HEAD `45278ff`
(clean). Implementation range `ba9c624..45278ff`. Live-probed artifact: prebuilt preview app, source
`f3876757` (`--version` dirty=false, bench_control=true), executable sha256 `29c7e449…5dd0`
(matches HANDOFF). Reviewer: Claude Opus 5.5, no subagents, no source edits or commits.

The report is written outside the tree on purpose. An untracked file in `docs/reviews/` would make
`build.rs` stamp `dirty=true`, and `matched.py` would then reject the next rebuild.

## Verdict

- **Design:** pass with two required fixes. Layout, tokens, type sizes and copy match spec §2. The
  only accent use is the one allowed by §2.7, and none of the §2.8 patterns are present. Real
  app/file icons render with the red and blue channels swapped (F1). That is a visible defect and
  must be fixed before visual QA.
- **Ready for full measurement:** **no.** One confirmed, live-reproduced memory leak in B (F2) and
  three harness fairness/provenance gaps (F3–F5) need fixing first. Each fix is small. After they
  land, a rebuild at the fixing SHA plus one smoke is enough. No new review round is needed for
  F3–F5 if the fixes match the descriptions below.
- **No functional defect** was found in the core seam, session/generation logic, exactly-once
  activation, control authentication or endpoint lifecycle, within the reviewed scope.

## Findings (most severe first)

### F2 — P1, CONFIRMED live: icon textures leak in the GPU atlas on every hide/show and LRU eviction
- Where: `prototypes/native-launcher/src/launcher_view.rs:259-261` (`hide` clears `icons`/`lru`),
  `:418-421` (LRU eviction), `:403` (new `RenderImage` per fetch).
- Cause: GPUI's `RenderImage` has no `Drop`. Atlas tiles are released only through
  `cx.drop_image(image, Some(window))`, which GPUI's own `image_cache.rs:240-280` calls on
  eviction. The prototype drops the `Arc` without calling it. Every hide clears the cache, and every
  show re-fetches and re-uploads icons under new `ImageId`s.
- Trigger/effect (live, PID 16637, isolated review profile, control channel only):
  - 30 cycles of `show → query "Orchard project meeting" → settle → hide` (no growth from these
    rows): `IOAccelerator (graphics)` stayed at 8288 KB; footprint 64.6 MiB.
  - 30 cycles of `show → query "doc" → settle → 1.5 s → hide`: 12 MB / 68.7 MiB.
  - 60 such cycles: 20 MB / 76.8 MiB, still growing roughly linearly.
- Why it matters: it inflates B in the hidden condition (B does a `doc` query before hiding) and
  the retained condition (more than 128 distinct file icons are evicted, plus the open-timing
  hide/show). That biases the RAM experiment against B for reasons unrelated to GPUI's
  intrinsic cost.
- Fix: before removing an image from `icons` (eviction, and in `hide` when clearing), call
  `cx.drop_image(image, Some(w))` for every `Some(image)`. `hide` already has `w`; for eviction,
  pass `w` from `update_in`. Re-run the same 60-cycle probe: IOAccelerator should stay flat.

### F1 — P1 (visual), CONFIRMED: real icons render with red and blue channels swapped
- Where: `launcher_view.rs:403`, `RenderImage::new(vec![Frame::new(i.to_rgba8())])`.
- Cause: GPUI's `RenderImage` is BGRA (`gpui/src/assets.rs:42`, and `elements/img.rs:678-680`
  swaps RGBA→BGRA before constructing it). The prototype passes RGBA.
- Evidence: `evidence/mac-light-results.png`. Safari's dial renders orange with a blue needle;
  the real icon has a blue dial and a red needle. Every app and file thumbnail is affected.
- Fix: `let mut rgba = i.to_rgba8(); for p in rgba.chunks_exact_mut(4) { p.swap(0, 2); }` before
  `Frame::new`. After the fix, re-check the dark-mode document thumbnails, which render as black
  pages in `mac-dark-handoff-7617803.png` (cause not established).

### F3 — P2: the hidden (and visible) condition runs different workloads in A and B
- Where: `prototypes/native-launcher/scripts/measure.py:141-156` (B) vs `:157-161` (A).
- B hidden: `open_timings` (hide, show), `show`, query `doc` (80 rows plus icon fetch/decode/upload),
  then hide. A hidden: `show`, `hide` with an empty input. Spec §6.4: "show once, hide". B visible
  also runs `open_timings`, an extra `show`, and two `doc` queries before the visible run.
- Effect: B's hidden/visible samples carry extra icon, atlas and qlmanage history that A's do not.
  This compounds F2.
- Fix (either): (a) run `open_timings` only in a dedicated B run or condition, and make hidden B a
  plain `show` → `hide`; or (b) give A the same steps (A `show`/`hide` pair, plus the `doc` query
  through the existing JS `input` setter before the hide). Record the chosen step list in each
  result JSON.

### F4 — P2: `matched.py` verifies one B binary and bundles a different one
- Where: `matched.py:10` checks `--version` of `--b-binary`. `bundle-macos.sh:21` always copies
  `prototypes/native-launcher/target/release/tezbar-launcher-prototype`.
- Trigger: `--b-binary` points anywhere other than that path (for example a preserved f387675 copy
  while `target/release` holds a later or dirty build). B is then measured from an unverified
  binary. `binary_sha256` records the copy's hash but no source check runs on it. Building the
  default (no `bench-control`) release into the same target dir also replaces that file. That case
  at least fails loudly, at the control-socket wait.
- Fix: give `bundle-macos.sh` an explicit `--binary <path>`. In `matched.py`, pass `a.b_binary`,
  then assert that the bundled executable's `--version` equals the verified one and includes
  `bench_control: true`.

### F5 — P2: A's provenance is never checked; the "matched resources" SHA marker only restates the fixture's SHA
- Where: `matched.py` has no A check. `stage-a.py:10` builds A's Resources from the *current
  checkout* via `stage-resources.py`, which writes `.tezbar-prototype-source` from the fixture's
  own marker (`stage-resources.py:18`). B/C then "match" A's resources only through that copied
  string.
- Reachable failure: the README's A path `target/tauri-a/release/tezbar` does not exist yet. If an
  older A is reused (the 404429e A was built with Rust 1.85), the run proceeds silently and
  reintroduces the compiler confound the README says to remove.
- Fix (cheap and conclusive): each binary embeds its compiler commit (`strings | grep /rustc/`). For
  B it is `48a229ce…`, equal to 1.98.1. In `matched.py`, assert that A and B carry the same single
  `/rustc/<hash>` and that it equals `rustc +1.98.1 -vV`'s commit-hash. Also record A's sha256 and
  the `git rev-parse HEAD` plus `git status --porcelain` captured when A was built, in a small
  build receipt next to the binary, and assert that receipt's SHA equals the fixture source.

### F6 — P3: `measure.py` labels the harness checkout as the source SHA
- `measure.py:126` `source_sha = git rev-parse HEAD` of the repo, not of the binary under test. With
  `fixture.py --source <older>` (documented in the README) the field is wrong. Rename it to
  `harness_sha`, and add `binary_version` (B/C `--version`; A from the F5 receipt and rustc hash).

### F7 — P2 (UX): pressing Down past the viewport jumps the selected row to the top
- Where: `launcher_view.rs:491-492` uses `scroll_to_item(selected, ScrollStrategy::Top)`. At the
  pin, the non-strict path (`uniform_list.rs:433-450`) applies `Top` whenever the item is below the
  viewport. Pressing Down from the last visible row (around row 15) moves the list about 15 rows at
  once instead of one.
- Fix: `ScrollStrategy::Nearest`. Code-confirmed; not observed live, because no key-injection
  surface was available.

### F8 — P3: the GUI quit hook has 200 ms, not 5 s, for `NativeCore::shutdown`
- `launcher_view.rs:62-68` wraps shutdown in `timeout(5 s)`, but GPUI polls quit observers for at
  most `SHUTDOWN_TIMEOUT = 200 ms` (`gpui/src/app.rs:80,1272`). The live probe quit cleanly (exit 0,
  no survivors, endpoint removed) and the fixture has no extension workers, so there is no observed
  harm. Spec §4.4's "5 s bound" is not what runs, though. Fix: on control `quit` and the `Quit`
  action, await `core.shutdown()` (5 s bound) on Tokio, then call `cx.quit()`. Otherwise record
  the deviation.

### F9 — P3 (polish)
- Icons are fetched one at a time (`launcher_view.rs:396-406`; file icons go through qlmanage).
  The first screen pops in over seconds (see the dark handoff screenshot: rows 1–2 have real icons,
  the rest show glyph tiles). The renderer's `loadAssetIcon` issues these requests concurrently.
  Bounded concurrency (for example 4) would match it better.
- Empty and no-results lines (`launcher_view.rs:571-581`): spec §2.4 has "No results" in text-2,
  but the code uses text-3. The 16 px offset sits inside a 6 px top / 5 px side container, so the
  text lands at about 22/21 px.
- B drops its icon cache on hide (spec decision §2.5). A's `resolvedAssetIconCache`
  (`CommandBar.tsx:1093`) survives hides. List this under service differences in the measurement
  report (B re-runs qlmanage on every show).

## Spec dispositions (reviewed scope)

| Area | Result |
| --- | --- |
| §4.3 production seam | Matches: one feature, `desktop_services` private flag equal to `app.is_some()`, exactly 4 gates replaced, `platform.rs` untouched, new default-off test. No new deps. |
| §4.2 facade | Deep and minimal. Rows keep core order; there is no ranking or merging. `Ticket` is non-Clone and owner-bound. Activation re-classifies the private action (an edited `support` cannot escalate). Profile is a temp child plus marker plus symlink/inode checks. `status` is a fixed channel. |
| §3.2–3.4 session | Latest-wins with a fixed 8 ms window that typing does not extend; one request in flight and one pending; stale responses touch nothing; deferred Enter limited to 1 s and bound to its generation; a click on stale rows is refused; key-repeat Enter is ignored; late activation after hide does not clobber state. Unit-tested. No defect found. |
| §3.1 input/IME | Built on the official example. At the pin, GPUI sends keys to the IME first while composing (`gpui_macos/window.rs:2668-2720`), so Enter/Esc/Up/Down during composition behave as §3.1 by construction. Only committed text emits `Committed`. **Live IME, dead keys, palette, drag and double-click: untested** (no CUA). |
| §3.6 lifecycle | Hide on Esc, on success and on focus loss; trim 5 s after hide, cancelled by show; empty input on next show. Live: repeated show/hide/quit cycles were clean. |
| §2.2 tokens | All 22 token values check out to the byte (alpha rounding included). Selected-row fill `#4f52d9` and badge `#5b5ef0` are the only indigo. Text-selection fill is neutral `pill_strong`. |
| §2.1 layout | Header 43 + 1 px rim = 44, insets reduced by 1 px to compensate for the GPUI border taking layout space (CSS used an inset shadow). Correct. Bottom corners clip correctly (pixel check). Footer reservation deviation accepted. |
| §2.8 prohibited | None present (no gradients, glass, stripes, extra badges, spinners, all-caps, or generic copy). |
| §5.3 pin | Official `zed-industries/zed` rev `c84af5b9…` for gpui/gpui_platform/gpui_tokio, `runtime_shaders` + `font-kit`, async-task patch per spec. `LICENSE-GPUI` plus the Apache attribution header in `text_input.rs`. |
| Control | Token of at least 32 characters with a constant-work compare, 64 KiB/2 s bounds, socket mode 0600 in a 0700 mkdtemp root, Windows owner-SID DACL with remote clients rejected, endpoint removed on quit (verified live twice). The op set has no raw channel or JS. |
| Parent fix A (45278ff) | Correct: A retained now runs `show` before the JS workload and `hide` after it, so the visibility check sees hidden. |
| Parent fix B (45278ff) | Correct: B visible no longer calls `d.js`; the native `query` remains. |

## Known-limitation dispositions

- Undo/redo omitted: allowed by §3.1. Not material.
- Font features and letter-spacing: Geist is not installed on this Mac, so the system font is used
  and the features cannot apply. Not material.
- Footer reserves 32 px instead of floating: visually equivalent at rest. Not material to
  measurement. Keep it noted.
- Primary-display centering: material only for multi-display QA. Future acceptance item.
- Windows DirectX/IME unverified: future acceptance. It does not block macOS measurement. Windows
  numbers stay "prototype-only" per §6.7.

## Evidence and limits

- No `mcp__cua_repl` (or any macOS desktop-automation tool) was exposed in this session, so
  native text, selection, IME, pointer and scroll behaviour was **not** exercised. Design
  judgments come from the provided screenshots (older snapshots: 7617803/404429e) plus code
  reading.
- XERJ indexing failed (server memory circuit breaker rejected writes), so `rg` and direct reads
  were used.
- Live probe: two launches of the prebuilt f387675 preview app on its existing review profile
  (PIDs 16120 and 16637), control channel only, theme dark. Both quit with exit 0, no owned
  survivors and the endpoint removed. Fixture clipboard stores were scrubbed with
  `scrub_fixture_clipboard`. The installed app was not touched, and no other app was activated.
  Script and raw JSON:
  `/private/tmp/claude-501/-Users-almat-Desktop-code-Tezbar-APP/22463d83-11d0-4592-b1da-7cc4ee1923e4/scratchpad/probe/`.
- One unexplained event: in the first launch, the 4th show→query cycle (the first after a 7 s hidden
  pause) did not reach `painted_generation == generation` within 30 s. The second launch did not
  reproduce it over 99 cycles. I could not attribute it, and focus loss is possible. Recommend
  that `settled()` in `measure.py:87-92` include the last status in its TimeoutError, so a
  recurrence during the matrix can be diagnosed.
- Not run: builds, the full matrix, Windows probes, A launches.

## Parent repair list (in order)

1. F2: call `drop_image` on eviction and on hide (`launcher_view.rs:259-261, 418-421`). Re-run
   the 60-cycle probe.
2. F1: swap RGBA→BGRA before `RenderImage::new` (`launcher_view.rs:403`). Re-capture light and
   dark screenshots of the results view.
3. F3: make the hidden/visible step lists identical for A and B (`measure.py:141-161`).
4. F4: add `bundle-macos.sh --binary` and assert the bundled B's version (`matched.py`,
   `bundle-macos.sh:21`).
5. F5: assert equal `/rustc/<hash>` (1.98.1) for A and B, plus an A build receipt matched to the
   fixture SHA.
6. F6: rename `source_sha` to `harness_sha` and add `binary_version` (`measure.py:126`).
7. F7: `ScrollStrategy::Nearest` (`launcher_view.rs:492`).
8. Optional: F8 (shutdown before quit), F9 polish, and status-in-timeout for `settled()`.

Then rebuild A and B at the fixing SHA with 1.98.1, run one matched smoke (A, B and C, `--smoke`),
and start the full matrix. This review does not close the measurement gate or the visual-QA gate.
