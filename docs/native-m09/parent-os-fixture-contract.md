# Parent M9 real-SDK desktop boundary and remaining bridges

This is a pending integration contract, not an implemented test seam. M8 currently
owns compiled shared source; no Host/platform/worker code is released by this file.
The nine SDK fixture sources and native runners are uncompiled/unrun at this point.

## Finite fake OS boundary

`TestHost` already intercepts surfaces, dialogs, capture, shortcuts and status items,
but current clipboard and open/reveal handlers can still reach arboard/opener/native
commands directly. `allow_interaction=true` is not a sandbox. Do not run the actions
fixture until every physical operation it exercises crosses a finite fake boundary.

Prefer a typed optional Host desktop-effect hook with a default production fallback,
inserted AFTER existing input validation and policy, immediately before actual OS
execution. Supported variants must be finite (clipboard text/read/write, any required
image/HTML/files variant, open/reveal), never an arbitrary channel override. TestHost
must explicitly handle or refuse each supported effect, and never fall through to
real OS APIs. Do not create a test-only fake worker or short-circuit the real core
router, authorization, serialization or command handlers.

Payload Debug must redact clipboard/HTML/file contents and URLs/paths. Keep actual
fake clipboard values only in private fixture memory; record lengths/counts for
assertion diagnostics. Check invalid requests still fail before any fake effect.
Production behavior stays on existing native routines; no second desktop executor,
clipboard writer, runtime or config model. The same Host seam can serve later GUI
host-effect fixtures without real system clipboard access.

Adapt DesktopRecords/native_harness to observe the actual TestHost boundary rather
than diagnostic worker records. Prove one 10,010-character copy, actual paste/open/
reveal, and sixty rolling SDK effects; unchanged refreshes must repeat none. The
worker's cumulative last50 diagnostic list is observational, never permission for
Rust to execute effects again.

## Actual SDK bridges still missing

Source inspection confirms worker `closeMainWindow`, `popToRoot`, `clearSearchBar`
and preferences entry points are stubs, while showHUD currently records feedback.
Decide each reachable required row from the authored spec/TS behavior and implement
its real finite core/native route. Never reinterpret an old cumulative diagnostic
record as a new desktop action. Hide/close must reach the existing Host surface and
owned App lifecycle once; keep source/session/generation fences and no double hide.

MenuBarExtra tokens/callback serialization alone do not establish native tray
lifecycle. Feed validated menu state into the existing core StatusItems owner with
extension/session-scoped identity, owned callback delivery through the SAME worker
and runtime, update on React invalidation, and cleanup on disposal/disable/remove/
shutdown. A hidden launcher must not be required for a tray click to work. Reuse the
existing native-V2 tray/host implementation where possible; do not add a UI-owned
tray process or replay menu actions from response history. Reject a worker's attempt
to overwrite another extension's scoped item.

All nine real SDK scenarios need actual registration and execution after the shared
asset/Host/worker seams and Cargo gate. A linked fake transport test is not real Bun
SDK evidence. Stop only task-owned children, preserve failures, and verify worker,
tray callbacks, subscriptions and asset leases are released. M9 acceptance and a
fresh Opus review remain pending.
