# M9 integration APIs

This documents the composed implementation in the selected `feat/native-gpui-app` worktree. Earlier integration and verification documents are retained as historical drafts. Headless execution is evidence of the assertions listed in the final receipt; it does not accept the platform or visual parity matrix.

## Native routing and ownership

`Launcher::configure_extensions(CoreClient)` supplies the existing core to the retained `StoreView` and `RuntimeView`. Both are exported by `surfaces/mod.rs` and mounted by the command bar. The Store's Settings action and runtime Preferences action reach the existing M7 Extensions Settings route. No second settings writer is introduced.

Runtime navigation requires a successfully parsed `TreeReply`. Unavailable or malformed delivery retires its navigation admission synchronously, exposes an error and preserves the current screen. Valid delivery acknowledges the original nonce once after mounting. Failure, no-view replies and hotkey failures preserve the launcher. Hiding retains the entities and session while stopping visible work. Reopening attaches weak subscriptions if they were disposed.

`StoreEffectEmission` and `ExtensionEffectEmission` carry the generation at emission. The parent checks entity identity, current navigation, visibility and generation before handling effects. Runtime host effects additionally carry the worker session and `hostSource` captured by the actual action invocation. Delayed controls from an older mount cannot close or alter a newer screen. File selection uses the existing App dialog lease; stale rejection also releases the reserved lease. Rendering does not start requests or timers.

## One core transport

`ExtensionTransport::{send, drain}` is implemented by `CoreClient`; `Request::Extension(ExtensionRequest)` is the adapter seam. `ExtensionRequest` has a finite set of variants for Store list/search/install/uninstall, run/invoke/search/refresh/load-more/dispose, preference setup/get/set, open and copy. Its `wire()` function maps these to the existing core channels. Both request and desktop-effect Debug output redact payloads.

Admission occurs when `send` is called, before polling the returned GPUI future. A spawned core operation owns its semaphore permit through completion, so dropping the view's waiter does not abandon a mutation. Ordinary operations have 16 permits and disposal has a reserved 16 permits on the same core/runtime. Full admission returns a visible retry error. `drain` waits for both sets of admitted operations. App quit runs the extension disposal/drain after the preliminary durable chat, terminal, settings and utility barriers; a failed preliminary quit does not irreversibly dispose extension state.

## Host boundary

`Host::desktop_effect(DesktopEffect) -> Option<Result<Value, String>>` is called at the existing native execution points after their validation and policy checks. The finite variants are clipboard read/sequence/text/html/image/files, selected text/files, image read, open app, open with, open and reveal. The default `None` follows the original native routine. `TestHost` always returns `Some`, including denial, and therefore cannot fall through to the real desktop. Clipboard text is retained in full; diagnostic truncation is separate from execution data.

`Host::extension_control(session, source, ExtensionControl)` transports Close, Preferences, ClearSearch and HUD once through the existing core/native owner. GPUI publishes a source-stamped `extension:ui` event and fences it in the active runtime. The worker uses action-local async context so delayed controls keep their original source. Push and pop continue to mutate the actual worker navigation stack. The rolling `effects` list is diagnostic only and is never replayed by native refresh.

SDK MenuBarExtra uses the existing `Host::status_items` implementation and the shared Native V2 tray validator/executor. Core-generated status scope identities are registered from actual admitted run replies. Callbacks retain that session/action authority, work while the launcher is hidden, and stop after retirement. Hidden invalidation refreshes are owned by the core worker. Nested sections, submenus and separators use the existing status menu representation, with depth 8 and 128-item bounds. Rerun, disposal, disable, uninstall, shutdown and worker EOF remove owned status items. No additional tray process is created.

## Canonical worker/model wire changes

* Actions preserve `ActionPanel.Section` metadata in `Action.section`.
* Form state reads `defaultValue`, dropdown defaults and serialized date values from the canonical tree; required validation remains enforced after correcting a submitted form.
* Pagination accepts `hasMore` and `__hasMore` and advances client pagination after server exhaustion.
* Inline preference setup retains the original `argumentValues` for the rerun.
* Worker snapshots carry `viewRevision`; owned refresh includes the last seen revision. A hidden status refresh cannot consume the update that the GPUI view still needs.
* React invalidations coalesce per session/generation; `refreshMs: null` remains event-driven. Worker requests, host futures, sessions, frames and native queues retain finite admission limits.

## Assets and native controls

All production image scopes come from the installed canonical registry through `crate::asset_transport`. Worker `assetsPath` does not widen authority. The accepted core asset service, registry path locks, decoder and transport are unchanged. Runtime List/Grid admission follows the visible scroll viewport with bounded leases and passes `Variant::width` for local, data and remote requests. Built-in icons have meaningful glyphs/tint and accessible labels. Markdown continues to use weak `set_image` delivery with typed tickets.

Forms use the established native text, password, date, checkbox, dropdown, tag and file controls. The native HSV wheel uses the existing form value/callback ownership and bounded drawing; it does not create another desktop service. Search, action sections, metadata, list/grid pagination, loading/empty/error/back states and the shared footer remain inside the retained native surfaces.

The native wheel renders Rust-supplied format rows and dispatches their existing action IDs, including Save Color. Legacy image-only colour detail uses the existing `tezbar_core::instant::color::build_color_format_rows` formatter for all nine rows, retaining wheel coordinates in the canonical FormState and copying through the finite core transport. Pointer input outside the wheel is ignored.

## Root M10 composition

Root's 15-file patch was applied as unified hunks. `app.rs`, `events.rs` and `command_bar/mod.rs` were composed with M9 changes; no base-file replacement was used. Root's compile correction retains the existing selected-row `terminal_pin(cx)` and renames only the new pin delta path to `apply_terminal_pin_change(id, toggle, cx)`. Its test literals use `11u8` and the locked Alacritty `MOUSE_REPORT_CLICK` constant. Patch hashes, later corrections and actual gate results are recorded separately in the completion receipt.

Root's subsequent bench correction derives Deserialize for SessionRow, defaults only cwd/running/active and keeps id/title required. The authenticated bench panel channel calls the feature-gated `fixture_rows`, which accepts both legacy `[id, title]` tuples and typed rows without dropping metadata. A bench-only regression uses this actual decoder for legacy, rich, minimal and malformed values.
