> Current linked status and superseding evidence: [linked-handoff.md](linked-handoff.md). The original draft below is retained as historical context.

# M10 integration contract — unexported source lane

Base: `409dbf3c5950b0065aff900b2906b548660bc264`, `feat/native-gpui-app`, only this worktree. Parent/M8 owns Cargo, exports, Request dispatch, App, windows, durable state and host effects. No Cargo invocation is authorized. Source-only fixtures are **unrun**, not compile evidence.

## Finite seam requested from parent after M8 gate

- Export `tezbar_ui::terminal_grid` and `surfaces::terminal` only after dependency release.
- `TerminalView::new(Arc<dyn TerminalTransport>, &UiState, window, cx)`; `activate(Payload::Terminal { .. }, nonce, window, cx)` is explicit work admission. A duplicate nonce does nothing. `set_visible(bool, window, cx)` stops blink and paint while retaining grid, selection and display offset. Hidden views continue consuming subscribed output; hide/Settings sends a real core Detach to start canonical keepalive while retaining all local terminal state. Show resumes attachment without resetting/replaying the retained grid. `core_event(&CoreEvent, cx)` receives ordered events; `drain()` returns typed host effects; `dispose(cx)` requests detach, never kill/delete. Keep owner alive until `is_quiescent()` before destroying. Core shutdown owns actual kill/wait, <=8s.
- Implement `TerminalTransport::send(TerminalRequest)` on the **existing CoreClient** via a new finite `Request::Terminal(TerminalRequest)` arm delegating to `wire()`. Same request admission and existing Tokio runtime, no new core/runtime/process. Do not make an arbitrary channel transport.
- Subscribe `terminal:data`, `terminal:exit`, `terminal:sessions-changed` using existing CoreEvents **before** `activate`. Call `listeners_ready(data_ready, exit_ready, cx)` as each subscription becomes owned; create/attach requires both. Route subscription lifetime through retained entity, including hide/Settings; release after dispose drain.
- Parent consumes `TerminalEffect::{Back { sentinel: ">" }, OpenSettings, SessionsToggle, OpenUrl, Copy, ReadPaste, Zoom, Pins}`. Copy/paste are typed host effects, synthetic in fixtures. Scheme-check URLs again in host. Pins update existing UiState owner and existing persistence transaction, never a second file writer. Zoom is whole-interface zoom; terminal font size stays local 8..24, default12.
- Existing `windows::terminal_sessions` owns panel placement, parent relationship, close handler, focus and 80ms blur grace. Feed rows from `session_rows`, call `select_session` or `new_session` on the retained terminal owner. Do not create another terminal process owner or panel window implementation. Parent routes panel toggle from existing menu/shortcut.
- Drain effects promptly after callbacks/completions through `TerminalEffect` GPUI event notification. App back interception must yield to marked IME and configure drawer before returning `>`.

## O-ALAC dependency proposal (not applied)

Add **both** `apps/tezbar/Cargo.toml` and `crates/tezbar-ui/Cargo.toml` dependency:

```toml
alacritty_terminal = { version = "=0.26.0", default-features = false }
```

App needs `TermMode` and selection types for protocol input; UI owns actual Term/Processor. No Zed terminal/editor/ui/theme dependency. Existing GPUI c84af5b/Rust1.98.1 and core/model1.85 unchanged. Published package is edition2024, MSRV1.85.0, Apache-2.0 with LICENSE-APACHE. Verified against https://docs.rs/alacritty_terminal/0.26.0/alacritty_terminal/ and the published registry archive https://static.crates.io/crates/alacritty_terminal/alacritty_terminal-0.26.0.crate (read in memory, no install).

Exact proposed lock node: name `alacritty_terminal`, version `0.26.0`, source `registry+https://github.com/rust-lang/crates.io-index`, checksum `bda177466b9524d59f1b12f0dd30b68696788e9992a7e959021c4a0ed96fcf59`. Direct dependency requirements: base64 ^0.22.0, bitflags ^2.4.1, home ^0.5.5, libc ^0.2, log ^0.4, parking_lot ^0.12.0, polling ^3.8.0, regex-automata ^0.4.3, unicode-width ^0.2.0, vte ^0.15.0 (`std,ansi`, no default); unix rustix ^1.0.0 (`std`, no default), rustix-openpty ^0.2.0, signal-hook ^0.4.3; Windows miow ^0.6.0, piper ^0.2.1, windows-sys ^0.59.0 (console/foundation/security/library-loader/threading/windows-programming). No optional serde edge. Cargo resolution and the complete transitive lock delta remain parent-owned and unverified; no fabricated frozen/offline resolution claim.

API checked in published source: `Term::new(Config, &Dimensions, EventListener)`, `vte::ansi::Processor::advance`, `Term::resize`, `renderable_content`, `selection_to_string`, `Selection::new/update`, `TermMode`, `Config::scrolling_history`, `Config::osc52`. Use only parsing/grid APIs; never `tty`, event_loop or PTY creation from this crate. OSC52 clipboard access disabled; explicit selection Copy goes to host.

## Finite upstream limitations requiring integration decisions

Current core attach returns one `recentOutput: String` read from the complete durable log; there is no byte offset/streaming snapshot API or event PTY generation. Core emits final data before exit and internally fences PTY generations, but the UI cannot manufacture generation provenance absent on the wire. UI fences mount, selection and request completion and bounds its pre-attach queue; it must not claim exact repeated-output overlap is a byte-offset protocol. Existing overlap algorithm is preserved. For large durable history, request an ordered bounded chunk replay seam (snapshot watermark plus subsequent event offset/generation); do not tail arbitrary raw VT bytes. This is a requested finite core dependency, not permission to edit core here.

## Compile release request

Once source is ready, parent must explicitly release: the two manifest entries + lock resolution, two module exports, finite Request/transport adapter, retained App mounting/subscriptions/effect drain, existing sessions panel action composition. Then run frozen-source `cargo check/test ... --locked --offline -j2` with exact package/target scope chosen after lock freeze. Record source hashes and logs; fix real API errors. Until that release no build/test/manifest or shared source edit. Windows, real IME/AX, multi-monitor, GPU and RAM remain manual/final gates; full migration remains incomplete.

## Source handoff refinements

- `listeners_ready(data: bool, exit: bool, cx)` accumulates readiness; both must be true before create/attach. Parent owns the actual channel subscriptions, not this surface. Feed the same ordered CoreEvent stream; do not subscribe a second copy and also call core_event manually.
- Call `layout_changed(cx)` from the parent layout owner **after** measured first layout and on bounds/text-zoom/interface-zoom changes. Paint caches bounds/mono metrics only. It does not start requests or timers. `set_theme(light, cx)` updates the ANSI palette. Keep the terminal retained across hide and Settings; hide sends Detach and show resumes Attach while preserving its grid. Call `dispose` only for actual final teardown. `is_quiescent` means no pending work; inspect `error()` as well, since a detach can fail.
- The transport is serial and retained through generation changes/dispose. Never drop the entity while a create/attach is outstanding. If parent cannot retain it, it must implement the equivalent completion-owned detach guard on its existing CoreClient request owner. No detached UI futures or second runtime.
- Additional finite core seam requested: `terminal:write-bytes` with typed bytes (e.g. byte array in its finite request), <=64KiB and the **same existing PTY writer**. Current `terminal:write` converts UTF-8 String to bytes; legacy X10 mouse coordinates >=95 cannot be represented faithfully. This source refuses invalid UTF-8 legacy reports; SGR and UTF8 modes work through String. The new byte route must remain first-party and inaccessible to extension/generic RPC. It is not added to wire() until parent releases it.
- Current core accepts <=64KiB per write and can return false under writer backpressure. UI splits large UTF-8 input at codepoint boundaries, atomically admits all chunks within its 1MiB/60-work bound (four slots reserved for cleanup), sends serially, and reports false acceptance instead of claiming success. A rejected write cancels the queued suffix. Mid-paste writer rejection needs a core atomic batch/admission seam for complete paste recovery; that remains a finite integration limitation. No silent retry that might duplicate input.
- Historical device queries are parsed for grid state but their PTY replies are discarded. Only live buffered suffix/device queries produce finite writes. OSC52 is disabled; there is no desktop clipboard path in grid code.
- App must intercept window-close/back through the terminal's composition/configure state before emitting the `>` sentinel. Actual native IME focus, GPU shaping, AX announcements and multi-monitor behavior remain unverified. The terminal name uses the existing TextInput; marked input has its own transient span and candidate bounds use the native shaper.
