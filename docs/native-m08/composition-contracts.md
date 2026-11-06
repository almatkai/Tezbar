# M8 retained composition — linked contract

Selected worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`,
branch `feat/native-gpui-app`, parent HEAD `409dbf3c5950b0065aff900b2906b548660bc264`.
All work is unstaged. M9/M10 source remains unexported. Original first-gate files
and logs in `integration.md`, `verification.md`, `source-freeze-first-gate.json`
and `logs/` are historical evidence and are not rewritten by this receipt.

## Owner and event flow

`Launcher` owns one `ChatIntegration`, retaining one `Entity<ChatView>` across
home/back, native hide/show, Settings and later chat navigation. The production
constructor injects the existing CoreClient and SAME SettingsStore already used
by Settings/onboarding/model selection. It creates no provider/run/config executor.
`receive_navigation` acknowledges the parent inbox once and advances its local
nonce floor before later local actions. Boot preserves exact prompt/cwd/resume/
capture semantics. Deferred activation checks the exact retained navigation.
Render mounts ChatView before search chrome/key handlers; core-ready does not
restart search over a mounted surface or onboarding.

Before first activation, persistent CoreEvents subscriptions own
`agent:event`, `llm-config-changed`, and `ai-extensions-changed`. They deliver the
actual payload through a weak entity and retained window. Hidden runs keep their
streams, approvals, canonical history and cwd/model ownership. Native capture
continues to use the existing native-window CaptureLease; it never logically
hides ChatView and invalidates its own capture ticket.

ChatView has ONE queue of `(emit_time_mount, ChatEffect)`.
`take_host_effects()` is the only parent consumer. The existing `take_effects()`
drains that same queue, stripping stamps for first-gate callers; it does not copy
or redeliver effects. EventEmitter is not also subscribed by the parent.
`ChatSource { entity: EntityId, mount: u64 }` travels through App's mailbox,
dialogs and image jobs. Navigation invalidates the old source synchronously,
before deferred replacement activation. Hidden sources and stale A effects cannot
open Settings/review/read images for B. Background changes notify the host
observer immediately, independently from transcript frame coalescing.

App `m8_effect` routes Back, Settings and ReviewProject to the actual retained
owners. Review sends cwd+nonce through existing M7 `settings_project_review`;
there is no global unscoped review request. Settings retains the launcher's
origin. The parent's one TaskIndicator entity is injected into ChatView and owns
chat statuses and resume routing; standalone chat's fallback footer is suppressed
when that shared footer exists. Its polling runs only for home or actually mounted
chat, and stops on hide/Settings/utilities/onboarding/real dispose.

## Host images and input

ChooseImages uses GPUI's native path prompt and CoreClient's bounded chosen-image
reader. ClipboardImage uses GPUI's asynchronous clipboard API only for the explicit
product effect. Tests use ClipboardItem conversion and an injected future; no
OS clipboard is read. Both return via the existing preparation ticket plus source
stamp. The chooser's reserved DialogLease survives asynchronous completion and is
released even for a stale/refused effect. Refused relay admission completes image
tickets with safe errors instead of leaving the composer locked.

DocumentImage uses the same core asset service for HTTPS/data authority, with no
arbitrary local execution/extension root. Markdown discovers requests at content
update, keeps weak image handles and uses generation tickets. Decode runs outside
Render; cache ownership performs `cx.drop_image` on GPUI's thread at hide, removal,
replacement or dispose. Existing canonical `chat:image` reads remain authoritative
for historical user attachments. Retry never invents payload IDs or copies images.

Actual GPUI input tests cover composition, committed text, Shift Enter, Enter,
focused native approval down/up, Stop and New shortcuts through Launcher. Explicit
Shift Enter handles a raw key path lacking a shared action binding; it applies
only to the focused noncomposing message input. Approval removal restores message
focus only when a disappearing approval control owned focus, including expiry.
Mode edits never approve a pending card. Native platform IME/AX acceptance is open.

## Quit and failures

App calls `m8_prepare_quit` before its existing state/shared-Settings/layout barrier
and before supervisor shutdown. `dispose_chat` and `ChatView.dispose` run once.
Dispose cancels auxiliary/history/image/meta/preparation jobs, stops runs, and
retains mutation jobs (append/start/cancel/approval/delete/clear/turn writes).
The release fallback does not dispose a second time.

`quit_settlement() -> Result<bool, String>` returns false while mutations settle,
error only while dirty turns remain, and true once durable. It does not use a stale
general-purpose UI error as a barrier. `retry_quit_persistence(window,cx)` retries
only dirty persistence after disposal; no repeated Cancel or tool replay.
`Launcher::drain_chat` observes settlement with an eight-second bound, retaining
the same entity. App's existing cancellation/error presentation and M7 global
barriers remain authoritative. Auxiliary HTTP cannot hold this save drain open.

M7 Settings/onboarding/window/model queue implementation and app/m7.rs are byte-
identical to parent HEAD. Their N1 fresh-save/error state, latest-intent layout
queue, source/generation fences and fail-open boot gate remain intact.

## Shared assets and next lanes

See `../native-m09/asset-parent-api.md` for the linked shared asset API, bounds,
registry capabilities and decoder details. M9 `extension_runtime/assets.rs` is
ONLY a public re-export; no other M9/store/runtime/fixture file is edited here.
M10 dependencies, module exports and terminal stream/input changes are excluded
from this M8 input freeze and acceptance evidence.

Compiled-input freeze and all gate results are recorded separately in
`composition-source-freeze-02.json`, `composition-gates-02.json` and
`composition-verification.md`. Independent Opus, Windows, manual desktop,
accessibility, GPU/RAM and full migration acceptance are not complete.
