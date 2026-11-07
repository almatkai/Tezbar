> Current linked status and superseding evidence: [linked-handoff.md](linked-handoff.md). The original draft below is retained as historical context.

# M10 verification status

**Unexported source draft, not compiled or passing. Full native migration and M10 parity are incomplete.** No Cargo/test/build, shell fixture, GUI, shipping app, live profile/service, clipboard, permission change, install, staging, commit, push, PR, merge, Actions or release was performed in this lane. rustfmt syntax/format validation is not a typecheck.

The initial checkout/branch/HEAD matched the instructed path and `409dbf3c5950b0065aff900b2906b548660bc264`. Pre-existing untracked M8/M9 source was left intact. Parent shared-file work appeared during this lane; no such file was edited by M10. No checkout or branch switch, fetch, reset or synchronization was performed.

Read AGENTS.md, the XERJ skill, specification §5.7/§5.8/§3.5/§5.5/§10/M10 and ledger T-01..14, L-17, S-15, S-18, K-16, K-29, W-06, W-09. One bounded XERJ search had no usable index; no reindex. Followed current source with exact rg/git reads. Read TS terminal/session/preferences/transport cases and native core manager/bridge/models. Published Alacritty 0.26.0 source and LICENSE verified from registry archive in memory; no package installation or Cargo invocation.

Implemented source:

- crates.io Alacritty processor/Term, split UTF-8/CSI parsing, alternate screen, selection/semantic/line ranges, bounded100000 history, true/indexed/ANSI colour, dynamic colour queries, wide/combining cells, fixed-cell fallback positioning, compatible ASCII glyph runs per row, theme palettes, bar cursor and read-only AX buffer/cursor description.
- Finite request and injectable future transport; listener readiness; create/attach/detach ordering; snapshot overlap; queued exit; abandoned-create detach; nonce/request/selection/cwd/config/paste fences; bounded pre-attach buffers and input queue; explicit false write acceptance. Core owns PTY lifecycle and durable history.
- GPUI sheet/header/text zoom/configure drawer; typed host effects for Back(`>`), Settings, panel toggle, copy/paste, URL, pins and UI zoom. Hidden view paints nothing, stops blink/cwd timers and retains grid/selection/display offset. Hide sends core Detach to start its keepalive policy; show resumes without resetting the retained grid. No terminal process is created from render/new.
- Synthetic pure and GPUI lifecycle/keyboard/IME fixtures. `source-case-map.json` records each original TS case and unchanged HEAD reference hash; unrelated shortcut/hotkey transport cases stay parent-owned. No source case is counted as executed. Existing PTY tests are not run, and will require separately authorized shell fixtures.

Known integration/final gaps:

1. No exports, manifest/lock changes, CoreClient adapter, App mounting/event subscriptions, host-effect drain or sessions panel composition are authorized here. These belong to parent/M8, as specified in integration.md. No W-09 completion claim.
2. Complete durable snapshot is still one unbounded core String and is processed synchronously here in64KiB parser chunks. Chunks preserve VT state but do **not** bound response allocation or UI parse time. Large-history/RAM acceptance requires bounded ordered streaming replay plus watermark/generation; current overlap retains the TS ambiguity for repeated output. This is not marked solved.
3. Raw legacy mouse-byte encoding needs a byte-write seam; core String cannot faithfully carry non-UTF8 bytes. SGR/UTF8 mouse modes have source cases. Large paste core-rejection recovery needs atomic batch admission. No complete mouse/paste parity claim until those edges are released and tested.
4. Keyboard source table covers app interception and common xterm keys/modes; international layout/AltGr/keypad/dead-key behavior, actual OS IME, IME cancel/commit/focus transitions and AX notifications require platform gates. Source fixtures are not evidence of OS behavior.
5. No GPU, VoiceOver/Narrator, Windows ConPTY, multi-monitor, RAM, 100-cycle hide/show, quit deadline or visual parity acceptance. The parent must run its new Opus review; this lane spawned no agents and made no design changes beyond the authored native spec.

Finite compile release requested in integration.md. After parent explicitly releases narrow shared edits and freezes lock/source, suggested isolated gates (not executed):

```
cargo check -p tezbar-native -p tezbar-ui --all-targets --locked --offline -j2
cargo test -p tezbar-native --lib surfaces::terminal::tests --locked --offline -j2
cargo test -p tezbar-ui terminal_grid::tests --locked --offline -j2
```

Do not broaden to an unfiltered core test suite: existing core tests launch actual shells/PTYs. That requires a later explicit isolated fixture authorization. On release, resolve real API errors first, retain exact command/stdout/stderr/status logs and fresh source/reference hashes; never relabel rustfmt success as compiled evidence. If offline dependencies are missing, report the exact failure and request the finite dependency-fetch edge; do not install implicitly.
