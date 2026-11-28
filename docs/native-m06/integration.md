# M6 native utilities integration

Implementation lives entirely in `apps/tezbar/src/surfaces/utilities/` and
`crates/tezbar-ui/src/rich_text.rs`, plus the authorized optional TextInput hooks.
Parent owns module exports, existing model feature selection, CoreClient adapter,
release-notes HTTP, canonical Notes selection metadata, navigation and quit composition.
The live shared ledger is not edited by this lane. This is not full migration acceptance.

## Ownership API

`Utilities::new(Arc<dyn UtilityTransport>, &UiState, &mut Window, &mut Context<Self>)`
creates native controls without starting work. Call `attach_events(&CoreEvents, cx)`
once on this owner. `navigate(Navigation, window, cx)` consumes a nonce once, routes
`Payload::New`, `Payload::Note { created_at: u64 }`, `Payload::Vault { item_id }`, and
starts the selected view's reads. `set_visible(false, window, cx)` drops read/decode
awaits, stops monitor/background polling, cancels release fetches, conceals Vault,
releases image handles/textures, and preserves only durable writes. Dirty note
ownership continues while hidden. No render callback executes a core effect.

Parent retains the entity through `drain(cx) -> Task<Result<(), String>>` before quit.
Each explicit drain retries every failed dirty note once, then awaits that round and
other admitted durable writes. A new failure is returned without a retry loop. Notes
use their original `createdAt` identity and per-note revisions, including hidden drafts.
Snippet create completion consumes the real ID returned by `snippets:add`; no inferred ID.
Do not drop the owner to discard a failed drain.

`UtilityTransport::send(UtilityRequest)` uses the parent's existing 16-admission
CoreClient path. `UtilityRequest::wire()` is a finite canonical channel vocabulary,
with a redacted Debug implementation. `release(url,accept)` is the parent's bounded,
cancellable exact-tag transport, not a generic HTTP channel. Configure bundled notes
with `configure_release(current, bundled)` before opening. Enter a historical version
in the search field and press Enter for an explicit exact-tag fetch. No newer release
is silently substituted. `set_presentation(Arc<dyn PresentationFormatter + Send + Sync>)`
receives the parent's existing native locale formatter; fixtures use explicit English.

## Parent effects (exhaustive)

| UtilityEffect | Parent action |
| --- | --- |
| `Navigate(Surface, Payload)` | Queue through the existing typed navigation owner |
| `Back` | Return to Commands |
| `Hide` | Existing native hide lifecycle |
| `HideThenPaste(String)` | Await hide/focus restoration, then paste once; do not paste before hiding |
| `RecentEmoji(Vec<String>)` | Edit and persist the parent's UiState; never a separate JSON store |
| `ReleaseRead(String)` | Edit and persist the existing release-notes read-version field |
| `SaveQr(Vec<u8>)` | Existing leased native PNG save dialog and error handling |
| `OpenSettings(String)` | Existing Settings navigation, retaining write ownership |
| `Reveal(PathBuf)` | Existing native reveal host effect |
| `UpdateAction(navigation::UpdateAction)` | Existing update policy owner; installer execution stays M11 |
| `OpenExtension { extension_id, command_name }` | Existing CoreClient extension activation/result gate, then runtime navigation |

Clipboard-to-Vault draft seeds remain within the same utility owner. They are
adopted only on the Vault navigation, authenticated before saving, and their source
clipboard entry is deleted only after durable save success. Vault read/session
fences retire stale authentication, detail, save and delete callbacks. Its status/list
are canonical core data; no keychain, DPAPI or profile is accessed by fixtures.

## Rich editor

TextInput remains the sole native input/IME/selection/AccessKit implementation.
Optional external editing, styled byte spans and silent projection updates are off
for all existing plain/password fields. Styled ranges are validated, clamped,
normalized and rebased during marked text. IME underlining remains independent.
`ExternalEdit` carries one committed replacement with lossless before/after byte
selection. RichText converts visual Unicode positions through its structural
projection; it retains Document's allowed nodes and uses production replace/format.
The pure helpers join paragraph/list boundaries, preserve inline styles, convert
plain multiline paste to `<br>`, and exit empty list items without flattening siblings.
All changes run inside **one** `Notes::edit_with_selection` transaction. The canonical
Notes Patch owns both HTML and selection undo/redo; no second editor history exists.

Core storage probe tests remain in M3; this lane uses only synthetic transport/host
fixtures. No Tauri, WebView, DOM, first-party JS, Zed editor, runtime, process, server,
installed app launch or production storage is introduced.
