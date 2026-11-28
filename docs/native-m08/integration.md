# M8 retained native AI chat integration

Worktree `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch
`feat/native-gpui-app`. Parent exported the module/finite CoreClient transport at
`0c3dbea`. The first check and 27 native target tests pass. App/Launcher composition
is still parent-owned and pending; owned compiled source is frozen for that step.
No manifests, locks, App, event owner, shared UI, model or core files are owned here.

Stable public seams in `surfaces::ai_chat`:

- `ChatView::new(Arc<dyn ChatTransport>, SettingsStore, UiState, &mut Window, &mut Context<Self>)` constructs only local state and ordinary multiline TextInput entities.
- `activate(Boot, nonce: String, window, cx) -> Result<(), String>` accepts explicit per-invocation identity. The last 256 (Boot, nonce) pairs are replay-fenced before I/O; a nonce reused for a different Boot fails. Parent must acknowledge each delivered navigation once and retain this entity across hides. Boot::Submit sends the initial prompt once through the same preparation/append lane.
- `set_visible(bool,cx)`, `set_ui_state(&UiState,cx)`, `core_event(channel:&str,payload:Value,window,cx)`; parent routes persistent `agent:event`, `llm-config-changed`, `ai-extensions-changed` before any run is requested.
- `set_task_indicator(AnyView,cx)` injects the parent's one shared footer owner.
- `attachments_result(ticket, Result<Vec<Value>,String>,window,cx)` accepts host-owned image results. Entries have `data` (base64), `mimeType`, optional `name,width,height`. A ticket fences session, revision and mount; it is consumed once.
- `dispose(window,cx)` is actual teardown only; it stops runs and executes persistence effects. Parent must retain the entity while admitted effects settle. Hide never cancels core work.

`ChatTransport::send(ChatRequest)->ChatFuture` returns JSON through the real core.
Parent thin dispatch: `Request::Chat(request) => request.wire()`; CoreClient implements
ChatTransport exactly as SettingsTransport (no extra executor, process or provider).
`ChatRequest` has redacted Debug. `agent:run` emits top-level immutable
`runSelection:{provider,model}` and omits `approvalMode`, preserving the core's live
policy reads. The same SettingsStore owner drains TaskSelection/global mode writes;
no full providerConfigs or task override map is emitted.

Subscribe to `ChatEffect` OR drain `take_effects()` (choose one consumption path):
Back, Settings, ReviewProject{cwd,nonce}, ChooseImages{ticket}, ClipboardImage{ticket},
and Background(statuses). ReviewProject goes directly to the retained native Settings
`review_project(cwd,window,cx)`, with parent nonce deduplication. Chooser/clipboard
image work is performed only by the existing host owner, then completed by ticket.
No transport error retries tools. Permission mode changes never call Approve.

Clear first calls controller `begin_clear()` for every retained owner, executes its
Cancel effects, collects every DeleteConversation token, and sends one `chat:clear`
with `[null, ownedIds]`. Those bounded owner IDs include not-yet-appended sessions;
core combines them with its active run sessions before durable tombstoning.
The result settles every token via delete_result; all returned effects execute,
including deferred dirty writes after failed clear. Unadmitted history rows use
explicit response-owned DeleteStored and native tombstones. No displayed-row
approximation of clear is used.

Canonical context comes only from RunRequest::prior_turn_ids and core_prior_turns().
Rendered markdown/tool previews never become priorTurns. Retry sends no copied
image bytes; core resolves the canonical stored user/image/tool history by IDs.
Turn durable writes use to_core_wire; history uses from_core_wire. Descriptor-only
image references from history are never invented payload IDs or new run images.

Native desktop visual/IME/VoiceOver/Narrator/Windows/GPU/memory acceptance remains
separate; this is not full migration acceptance. Actual verification is recorded
in verification.md after the parent-authorized whole-source freeze.

Document images use `ChatEffect::DocumentImage{ticket,source}` followed by
`document_image_result(ticket,Result<Vec<u8>,String>,window,cx)`. Parent resolves
HTTPS/authorized asset sources through the existing host image owner; no arbitrary
file read is implied. Shared Markdown discovers sources during set_content and
emits its generation-owned ImageTicket. M8 coalesces same-source waiters, decodes
bounded bytes off-thread and owns strong textures solely in ImageCache; Markdown
keeps weak handles. Hide clears requests/documents and releases cache textures;
late host/decode completions are rejected by mount and source generations.

Historical attachment previews use `ChatRequest::HistoryImage { session, turn, index }`
→ `chat:image [{sessionId,turnId,index}]`. The reply is the exact canonical USER-turn
image `{mimeType,data}` or null for missing legacy bytes. The view never invents a
payload ID for a historical descriptor, attaches these bytes to the composer, or
sends them on retry. The lane reserves the same bounded ImageCache tickets as
Markdown, checks mount and current reference before upload, and handles missing
previews as unavailable. `Retry image previews` is explicit. Parent's core owns
session/turn authorization, deleted-session denial and index validation.

Input admission matches the existing core: four PNG/JPEG/WebP images, each at most
8 MiB decoded source, with STANDARD base64. Atomic batches roll back newly staged
payloads on failure and duplicate same-owner bytes share a descriptor. Thirty-two
short-lived encoded payload slots are bounded at 32 × 11,184,812 bytes maximum;
acknowledged Start releases input payloads once canonical core storage owns them.
Preview decoding is separate: 8 MiB compressed, dimension ceiling 16,384 per side,
128 MiB decoder allocation, then thumbnail to at most 1024×1024 and BGRA upload.
A preview decode limit never rejects an otherwise supported source attachment.
Four image tickets/decode slots and the shared 32 MiB cache cap uploads. No measured
GPU/RAM claim follows from these allocation limits.

GPUI at this pin resolves TextInput actions before low-level key capture. Chat
captures shared Newline/Up/Down actions at the root, with actual field focus and
composition checks. Shift Enter remains a newline. Approval activation uses the
shared native button key-down/up path; composer/mode changes do not call Approve.
TestWindow has no full AccessKit tree; production role/name assertions are distinct
from actual platform accessibility acceptance. The failed original probe is kept.

Frozen source hashes: `source-freeze-first-gate.json`. Evidence and every failed
attempt are retained in `logs/`; see `verification.md`. Parent may now add retained
App/Launcher composition, persistent event subscriptions, host image resolver,
Settings review routing and shared TaskIndicator injection. M8 source changes or
additional whole-app/bench/build checks require a fresh coordinated source freeze.
