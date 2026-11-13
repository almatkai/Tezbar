# M8/M10 residual closure recheck @ 0e6e455

Reviewer: Claude Opus 5.5. Independent and working alone: no subagents, no repairs, no staging, commits or pushes. Date: 2026-11-09.

Pin: `0e6e4558993ada1fbb49973ceb1d23e74009c623` (tree `b9119273…`) on `feat/native-gpui-app`, selected worktree `Tezbar-native-rust-agent`.

Scope:
- R1–R7 of `tezbar-gpui-m810-r1r7-opus-fec3257.md` (sha256 `1ad45407a2142fda…7aee2`; the copy in the pin matches) and of the earlier `…-opus-0513321.md`.
- The follow-up repairs N-R1F, N-R2X, N-R4W, N-R5A, S1 and D1, plus the M9 status-icon route through the same SVG decoder.
- M9 ownership and the M11 lifecycle are out of scope.

Evidence labels:
- **[RUN]**: I executed it against the pin.
- **[SRC]**: source trace only.
- **[REC]**: recorded by the root, not rerun by me.
- **[GONE]**: the recorded artifact no longer exists, so I could not verify it.

## Verdict

**NOT ACCEPTED as M8/M10 residual closure.**
- N-R2X is still open. The new CSS matching-work gate can be bypassed in two independent ways.
- The same stylesheet path also has an unbounded memory amplifier, N-R2M (new).
- Every other residual in scope is closed, or closed at the logic level with its platform acceptance still pending.
- This review does not accept the migration and does not authorize a Tauri cutover. Tauri stays the default.

| ID | Verdict @0e6e455 | Remaining |
|---|---|---|
| R1 / N-R1F (SVG layers, ordinary clip+mask) | **CLOSED** (macOS standalone release probe of byte-exact pin code; not app RAM) | O-SVG-CPU (pre-existing) |
| R2 / N-R2X (CSS matching work) | **NOT CLOSED** | **B1** MEDIUM–HIGH: the gate re-tokenizes `Display` output, which hides combinators. 613 B admitted; the decode passed a 20 s CPU cap. **B2** MEDIUM: per-step attribute cost is unbounded. 1.08 MB with one rule took 8.3 s. |
| N-R2M (new) | **OPEN, MEDIUM (memory)** | One `*{marker:…}` rule costs ≈ 100 KB per element. An admitted 65 KB SVG extrapolates to ≈ 816 MB transient. |
| R3 (dashes) | CLOSED; `path_budget` unchanged since fec3257 | — |
| R4 / N-R4W (failed replay, then live gap) | **CLOSED** | — |
| R5 / N-R5A (wheel starving keys) | **CLOSED** | O-R5E (observation) |
| R6 (stale Create, startup queries) | CLOSED at the logic layer | Real ConPTY UNRUN |
| R7 / O-TERM-KEYS | CLOSED: spec, ledger and code agree; a portable decision test runs | Physical Windows keyboard and AltGr UNRUN |
| S1 (long literals) | CLOSED for the four named sites | Same-class, test-only residue at `asset_transport_tests.rs:275` (nit) |
| D1 (manifest erratum) | CLOSED | — |
| M9 status-icon routing | No budget bypass | Inherits N-R2X-B1/B2 and N-R2M |

## 0. Integrity and method

### Lost `/tmp` evidence

- The Mac rebooted at 2026-11-08 14:16 +05 (`kern.boottime`), which cleared `/tmp`. Everything below was dated 2026-11-07 and is gone:
  - `/tmp/tezbar-m911-root-final-20261107/`: the clean binary `01414172…`, `clean-native-receipt.json`, its build log and inputs;
  - `/tmp/tezbar-native-m911-validation-20261107-2sefx6og/`: 211 external proofs and the dirty binaries `063f6178…` and `6d14eeed…`;
  - both prior `/tmp` reports;
  - `/tmp/tezbar-followup-prepared-20261107.txt`;
  - the root targets `/tmp/tezbar-m9-c01-validation-Rffw07`;
  - the prepared SDK seed under `/var/folders/…/T`.
- **Consequence [GONE]:** I cannot verify the clean-PIN binary hash, its schema-2 record or its `--version` agreement. The same applies to the preserved dirty binaries. I make no claim about any of them.
  - I did not build a clean binary. My archive build is `dirty=true` by construction.
  - No later commit records the clean-build receipt: `git grep 01414172` finds nothing in `6432fd9` or `0228b4f`.
- The input reports are byte-preserved in the pin:
  - `docs/native-m10/followup-20261107/tezbar-gpui-m810-r1r7-opus-fec3257.md` has sha256 `1ad45407…`, as required;
  - `docs/native-m10/root-m810-repairs/tezbar-gpui-m810-opus-0513321.md`.
- User preview PID 79164 no longer exists (after the reboot). I never signalled, launched or replaced any application.

### Pin and live worktree

- `git archive 0e6e455` went to a private `src/`. `ls-tree` blob re-hash matched 3243/3243 at the start and at the end: 0 bad, 0 missing, 0 extra, modes equal. Tar sha256 `68153122…`.
- The live worktree had moved on before I started: HEAD `0228b4f` (two commits after the pin), with dirty entries going from 82 to 84 and 0 staged. These changes are not mine.
  - I used only read-only Git commands there: `rev-parse`, `ls-tree`, `archive`, `show`, `diff`, `log`, `grep`.
  - I never built, wrote, staged or checked out in it, and I created no Git checkout or worktree.

### Verification of the root evidence [RUN]

- **Hash ledgers.** All entries match:
  - m911 `evidence-sha256.json`: 96/96;
  - parent completion: 5/5;
  - M10 follow-up: 54/54.

  Each ledger covers every file in its directory except itself.
- **`compiled-inputs.json`: 427/427 equal the pin bytes.** The recorded gates compiled exactly the pin's code. They ran on 6ffa4e5 plus WIP, not as clean-pin reruns.
- **`validated-tree.json`.** I recomputed the aggregate as `afcafe60…`.
  - 3118 of 3121 files equal the pin.
  - The other 3 are the M9 status-icon handoff, receipt and source-hashes, rewritten after the gates. Their originals are preserved byte-for-byte in `docs/native-m09/status-icons-followup/checkpoints/unrun-01/` (`523e8ca7…`, `1d313331…`, `13c009e8…`).
- **`receipt.json`: 28/28 gate records equal the receipt.**
  - All 28 log hashes match.
  - Every gate is `stable`, with the before aggregate equal to the after aggregate.
  - Failures are preserved and labelled: `native-check-01` and `red-01` are compile errors; `red-02` and `red-03` are meaningful 0/1 REDs.
- **External proofs: 0 of 211 exist now** because of the reboot. 137 of them have hash-identical copies in the repo. Byte-exact raw logs and patches were left as they are; I did not dump the cutover or oracle patches.

### Builds

**Native test harness.**
- Command: `cargo +1.98.1 test --locked --offline -j2 -p tezbar-native --lib --features runtime-shaders --no-run`.
- Environment: `CARGO_INCREMENTAL=0` and, per command only, `TEZBAR_NATIVE_SOURCE_SHA=<PIN>`. `build.rs` needs a SHA when Git is absent; `own_repository=false` keeps `dirty=true` [SRC]. No toy Git fixtures ran.
- **Private target.** An APFS clone of the root `target/debug/{.fingerprint,build,deps}`, taken while no cargo or rustc process was running.
  - `Cargo.lock` and every `Cargo.toml` are identical between the pin and the live tree.
  - I deleted every workspace-crate fingerprint and artifact (59 directories and 31,889 files) from the clone, and touched every file in my source copy.
  - All 8 workspace units were rebuilt: 6 crates and 2 build scripts.
- **Input identity.** The test binary's dep-info lists 138 first-party inputs, all byte-equal to the verified archive. The binary lists **460 tests**, the same as the recorded count.

**SVG probe.**
- A standalone release crate that `include!`s the byte-exact `asset_transport.rs:369-725` (`verbatim.rs` sha256 `4b3a7843…`).
- Its lock matches the pin lock for 67/67 third-party packages (name, version, source and checksum).
- Each case ran in its own subprocess under `ulimit -t` and `/usr/bin/time -l`.
- The upstream `.crate` checksums for resvg 0.46.0, usvg 0.46.0, simplecss 0.2.2, tiny-skia(-path) 0.11.4, roxmltree 0.21.2 and svgtypes 0.16.2 equal `Cargo.lock`. I read their source from my own extracted copies.

**Safety limits.**
- No exponential matcher ran without a CPU cap.
- The largest memory run was 205 MB. The 816 MB and 18-minute figures are extrapolations, not runs.

### My runs versus recorded results

| Gate or check | Recorded | Mine, at the pin archive |
|---|---|---|
| Native normal / bench, full | 460 / 462 [REC] | Not rerun in full: real-SDK tests need the wiped seed. **Focused run: 88 passed, 0 failed, 0 ignored, 372 filtered.** Filters: `asset_transport::tests`, `surfaces::terminal::`, `command_bar::terminal_integration`, `windows::terminal_sessions`, `app::m10`, plus the weak status-binding test (`logs/31`). |
| Follow-up regressions (3 SVG, 2 terminal) | in 460 [REC] | **5/5**, inside the focused run |
| 14 `review_m810` tests and the Windows key decision test | [REC] | **14/14 and 1/1**, inside the focused run |
| 5 real-Bun status-icon tests | `native-status-01`: 6/6 [REC] | **Not rerun**: they need `TEZBAR_M09_PREPARED`, which was wiped |
| My native probes | — | **9/9**: 7 terminal and 2 SVG-admission probes, in a copy that differs only by 3 probe files (`logs/37`). One failed attempt is preserved: it failed because I string-matched the redacted `Debug` of `TerminalRequest` (`logs/33-…FAILED…`). |
| Standalone SVG probe | — | 77 executions over 47 cases, plus a rule re-tokenization dump (`logs/10`–`17`) |
| rustfmt `--check` on 7 owned files | pass [REC] | **pass** (`logs/20`) |
| UI 57, SDK 9 (Los Angeles), core 7, TS 748 with 7 skipped, Node 14, Python 1, updater 43 + 1 ignored on two toolchains, Rust 1.85 check | [REC] | Not rerun. Compiled-input identity is confirmed above. |
| Clean-PIN binary `01414172…`, schema-2 record and `--version` | [REC] | **[GONE]**; not verifiable |

## 1. Findings, most severe first

### N-R2X-B1 — MEDIUM–HIGH (availability): the gate re-tokenizes `Display` output, which does not round-trip — [RUN]+[SRC]

**Mechanism.**
1. `svg_preflight` takes each rule's steps and descendant branching from `rule.selector.to_string()`, fed back into `simplecss::SelectorTokenizer` (`asset_transport.rs:696-718`).
2. simplecss `Display` prints attribute values in single quotes without escaping (`selector.rs:381`). The original value is legal CSS when it was double-quoted, and it can contain `'`, `,`, `{` or `/`.
3. The re-tokenizer then stops *successfully*: at `,` or `{` (`selector.rs:623`), or at a bare `/` (`:614`). Or it swallows the real combinators into one quoted string. No `Err` is raised, so the conservative fallback at `asset_transport.rs:703-713` never runs.
4. usvg still matches the original parsed selector against every element (`usvg parse.rs:378`). It uses the unmemoized descendant backtracking at `selector.rs:174-192`.

**Trigger.** `<style>[a="x'],"] g g … g {fill:red}</style>` followed by D nested `<g>` elements.
- `Display` prints `*[a='x'],'] g g …`. Re-tokenization yields **2 tokens and 0 descendant combinators**.
- A variant without a comma, `[a="x'][c="] g … g [e="][f='"]`, re-tokenizes to 4 tokens and 0 combinators (`logs/17`).

**Measured** (release build, byte-exact pin code). Preflight returned `Ok(())` in 0.02–0.04 ms for every bypass case (`logs/10`–`12`):

| Case | Bytes | `svg_tree` (preflight, then usvg) |
|---|---|---|
| comma D24 k8 / k10 / k12 | 317 / 321 / 325 | Ok after 21.8 / 64.3 / 118.7 ms |
| comma D28 k12 | 353 | Ok after 1,020 ms |
| **comma D32 k12 / quote D32 k12** | **381 / 410** | **Ok after 6,939 / 6,855 ms** |
| **comma D40 k12, comma D60 k30, quote D60 k30** | **437 / 613 / 642** | **Killed at the 20 s CPU cap** (SIGXCPU); peak footprint ≈ 1.2 MB |
| Control `x g…`, D24–D60 | 308–604 | Refused by preflight: the gate works when `Display` round-trips |

The product's own debug test binary agrees (`logs/37`): its preflight admits the 613 B and 642 B inputs and refuses the 604 B control. A D16/k6 instance decodes there (9.9 ms in debug), which proves the bypass reaches usvg.

**Reachability** [SRC]:
- **Chat images.** Markdown images, `data:image/svg+xml;base64` URIs and remote images go to `decode_blocking`, holding a permit from the core's shared 4-slot `preview_admission()` (`references.rs:538,609-622`; `chat_integration.rs:202`).
- **Extension assets and the new M9 status icons.** They go through `load_asset` → `decode_admitted`. Its 10 s timeout drops only the waiter, while the permit stays inside `spawn_blocking` (`asset_transport.rs:207-222`).
- Identical chat sources are coalesced. Four *distinct* bypass SVGs therefore pin all four shared slots, and four CPU cores, until restart.

**Fix direction.**
- Never infer selector structure from `Display`. Count tokens over the original stylesheet text with the same tokenizer `consume_rule_set` uses, or bound branching by the whitespace runs in the raw rule text.
- Add regressions for values containing a comma, a brace, a slash and an odd quote. The 613 B case should be refused in under 1 ms.

### N-R2X-B2 — MEDIUM (availability): per-step matching cost is unbounded through attribute values — [RUN]+[SRC]

**Mechanism.**
- A single counted step can be one `attribute_matches` call. That is a roxmltree attribute scan, plus `split(' ')` over the whole value for `~=` and class selectors (`usvg parse.rs:669-674`; `simplecss selector.rs:31`).
- The work model counts steps, not bytes. A child combinator does not multiply the estimate at all.

**Trigger.** `<g class="a a a …">` (L bytes), N `<g/>` children, and R copies of `[class~=zz]>g{fill:red}` (`logs/14`):

| L / N / R | Bytes | Time |
|---|---|---|
| 64 KiB / 1000 / 1, then R = 10 | 69.7 KB | 75.7 ms, then 599.8 ms |
| 256 KiB / 1000 / 10 | 266 KB | 2,477 ms |
| 64 KiB / 4000 / 10 | 82 KB | 2,415 ms |
| **1 MiB / 8000 / 1** | **1.08 MB** (`ASSET_BYTES` is 2 MiB) | **8,322 ms** |
| 1 MiB / 8000 / 131 (the gate's maximum; the product admits it) | 1.08 MB | Not run. Linear extrapolation: ≈ 18 min |

All of this time is spent in `Tree::from_data`, before `budget()`. It is a different mechanism from the O-SVG-CPU rasterization observation.

**Fix direction.** Charge attribute steps by attribute bytes, for example `elements × steps × branches × ⌈longest non-geometry attribute / 64⌉`. Alternatively, cap non-geometry attribute values whenever a stylesheet is present.

### N-R2M — MEDIUM (memory; new, present since the R2 32 KiB stylesheet budget): matched declarations are copied per element — [RUN]+[SRC]

**Mechanism.**
- For every matched element, `insert_attribute` allocates `StringStorage::new_owned(value)` (`usvg parse.rs:283`).
- The `marker` shorthand writes three copies (`:326-329`).
- The preflight bounds stylesheet bytes, not bytes × matched elements.

**Trigger.** `<style>*{marker:url(#aaaa…)}</style>` with a 32,000-character name, followed by N `<g/>` elements (`logs/13`):

| N | 0 | 256 | 512 | 1024 | 2048 | 8190 (64,860 B; the product admits it) |
|---|---|---|---|---|---|---|
| Peak footprint | 1.28 MB | 26.8 MB | 52.4 MB | 103.3 MB | **205.0 MB** | Not run. ≈ 816 MB at 99.5 KB per element |

- Each case takes about 14 ms of CPU. This is a memory spike, not a hang.
- With all four shared slots, about **3.3 GB** of transient memory is possible, roughly 50× the R1 layer budget.

**Fix direction.** Before usvg, bound `elements × Σ declaration value bytes` (×3 for `marker`), for example to ≤ 8 MiB. Ordinary icons sit orders of magnitude below that.

### Observations and nits (not counted)

- **O-R5E** [RUN, P6]. Wheel runs interleaved with keys still spend one FIFO entry per run.
  - 30 wheel+key pairs fill the 60-entry FIFO, and the next key is refused. With keys alone, 60 keys are admitted.
  - The byte reserve itself holds (P4): 1,044,000 B of wheel input left 4,576 B of headroom, and a 4,000 B paste was then admitted and flushed in order.
  - This does not regress the claimed fix.
- **O-GAP-Q** [SRC]. A gap resume treats bytes before the new attach watermark as history, like the existing expiry path. Ordinary (non-startup) terminal queries emitted during the gap get no reply. The startup case *is* answered once, to the right generation (P7).
- **O-HIDE** [SRC, unprobed, pre-existing]. If the view is hidden while any Resume is in flight, the final `detach_hidden()` in `install_stream` does nothing. The resumed attachment then stays attached while hidden, until the next show or navigation.
- **S1 residue** [SRC]. `asset_transport_tests.rs:275` (`std::fs::write(package.join("active.json"),serde_json::to_vec(…`) is still an overlong statement that rustfmt skips. It is test-only and pre-existing.
- **O-SVG-CPU.** The existing observation of a 12 s expensive valid SVG stays open. I did not re-measure it, and I infer no app-wide CPU or RAM result.

## 2. Per-ID closure detail

| ID | Caller / source at the pin | Evidence | Remaining |
|---|---|---|---|
| **R1 / N-R1F** | `budget()` at `asset_transport.rs:502-527`. Children receive `live+area`. Clip/mask scratch of 1.5·area is charged only at the group's own check. That matches resvg `render_group`, which allocates the clip and mask scratch after child layers are freed. | [RUN] 5 ordinary clip/mask shapes render:<br>• clip+mask, clip+2 opacities, nested masks, clip+mask on one group, clip+mask+opacity;<br>• straight BGRA, corners clipped.<br>62 nested layers, 5 full-canvas layers, and clip+mask under 2 opacities stay refused in < 0.1 ms.<br>1024 px with 1 opacity + clip+mask: 19.3 MB peak against a 5.4 MB baseline, i.e. ≈ 3.3 layer areas measured vs 3.5 charged (limit 4). The follow-up pixel test passes. (`logs/15`, `16`, `31`) | O-SVG-CPU; app RAM not measured |
| **Ordinary CSS** | — | [RUN] 7 selectors render exact `[0,0,255,255]`: `g > .a`, `svg .a`, the quoted `it's`, `#id`, `:first-child`, `.a`, `g rect.a`. The follow-up test passes. | — |
| **R2 / N-R2X** | `svg_preflight` at `:570-725` | [RUN] tables above | **B1, B2** |
| **N-R2M** | No gate | [RUN] table above | **open** |
| **R3** | `path_budget` unchanged since fec3257 | [RUN] `review_m810_r3` passes; fec3257 probes [REC] | — |
| **R4 / N-R4W** | `request_replay` at `state.rs:751-788`. A tokenless live gap becomes `Attach` as `Resume`. The guard skips when a Replay or Resume is queued, and `stream_data` returns early while replaying (`:905`). | [RUN] The follow-up test passes.<br>**P1:** a new process generation gives "Terminal process changed. Pending input was not sent.", replays from 0 with the new token, never sends the stale `x`, and routes later keys to `(a, gen-b)`.<br>**P2:** navigation mid-resume releases the stale token, detaches `(a, gen-a)` and starts no stale replay.<br>**P3:** an admission refusal shows "draining", sets `running=false` and leaves no loading queue. | — |
| **R5 / N-R5A** | `admit_input` at `state.rs:996-1039`. The view calls `write_scroll_bytes` only for non-mouse alternate-screen arrows (`view.rs:150`); mouse mode with Shift yields `Rows` (`input.rs:285-301`). | [RUN] The real-GPUI follow-up test passes (60 arrows, then `q`).<br>**P4:** the 4 KiB reserve holds, with exact order and both directions kept.<br>**P5:** runs split at keys and keep mixed directions. | O-R5E |
| **R6** | The stale Create/Attach and startup paths are unchanged; the new gap path reuses `install_stream`. | [RUN] Both root R6 tests pass.<br>**P7:** after a replay error and a gap, the startup `ESC[6n` is answered exactly once, as a `ProtocolReply` to `(s, gen-s)`.<br>P2 shows cleanup with distinct generations. | Real ConPTY UNRUN |
| **R7 / O-TERM-KEYS** | `key_for_platform` at `input.rs:33-167` (policy at `:47-51`); spec §5.5/§5.7 and ledger K-16/K-29 record the resolved user decision (Ctrl goes to the shell, Ctrl+Shift to Tezbar). | [RUN] `windows_control_is_shell_and_control_shift_is_tezbar` (portable, `macos=false`) | Physical Windows keyboard and AltGr |
| **S1** | The four named statements are now formatted, including `state.rs:239-245` (`concat!`) | [RUN] rustfmt pass and an exact read | `:275` nit |
| **D1** | Follow-up README, row D1 | [RUN] `root-m810-repairs/` diff from fec3257 to the pin is 0 lines | — |
| **M9 status icons** | `status_icons.rs:14-116`. Builtins use `decode_preview` under a permit; File/Remote/Data use `load_asset` → `decode_admitted`. AppPath is unavailable (`None` adapter). | [SRC] | Inherits B1, B2 and N-R2M |

## 3. Recommended order for the root

1. **N-R2X-B1.** Count combinators from the source text, not from `Display`. Add comma, brace, slash and odd-quote regressions.
2. **N-R2X-B2.** Charge attribute steps by attribute bytes.
3. **N-R2M.** Gate declaration bytes × matched elements.
4. Then run a fresh SVG-lane recheck. Windows ConPTY, the physical keyboard and app RAM remain platform or manual gates.

## 4. Artifacts and final state

Work root: `/tmp/tezbar-m810-opus-0e6e455-review/`, 1.1 GB. It contains:
- `src/`: the verified archive;
- `upstream/`: the checksum-verified crates;
- `probes/`: `svgprobe` source and lock, `svgcases/`, and `native/` (probe source and diffs);
- `tools/`, `logs/` and `bin/`.

| Path | sha256 prefix |
|---|---|
| `integrity/evidence-sha256.txt` (84 files) | `cb0a4d94bc43e3b1` |
| `pin.tar` / `ls-tree.z` | `6815312248fde7ea` / `53c9865dc15a022f` |
| `probes/svgprobe/src/verbatim.rs` / `main.rs` / `Cargo.lock` | `4b3a7843ddafe242` / `79e9fbab253cf555` / `1352d179dfa1837e` |
| `probes/native/opus_0e6e455_probe.rs` / `tools/gen_cases.py` | `9171d1b2d354418d` / `a0559aafd0565802` |
| `bin/svgprobe-release` | `9e378a3405ce3425` |
| `bin/tezbar_native-pristine` (focused 88/88) / `bin/tezbar_native-probe` (9/9) | `4834d401f11df02b` / `a225892e410ecce3` |
| `logs/31` focused / `logs/37` all probes / `logs/35` terminal probes | `8cd7c913d70e4e25` / `180d931cd316087d` / `1744e79018b78bcb` |
| `logs/10`–`17` (SVG) | `85071066`, `4923aaec`, `ec83798f`, `e094ce2f`, `6396136a`, `44f0d529`, `4dad01d7`, `330d039a` |

Final state:
- **Processes.** No processes of mine remain. I checked for cargo, rustc, test and probe processes.
- **Cleanup.** I deleted my own 6.0 GB target clone, two 342 MB source copies and the 210 MB probe target. I only read the root target, `node_modules` and retained binaries (via the APFS clone); I did not modify them.
- **Archive.** The verified archive re-checked 3243/0/0 at the end.
- **Not used.** No GUI, app launch, profile, Keychain, clipboard, live provider, network updater, signing, install, release or remote work.
- This report was not copied into the repository.
