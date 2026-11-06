# M3 Lane A — launcher and utilities models

Status: **model repairs implemented; fresh independent closure and native adapters pending**.
Current evidence supersedes historical validation below: [M3A repair evidence](launcher-review-repair-evidence.md) and [per-finding dispositions](launcher-review-repairs.md). No GPUI view ledger row is
marked native. Parent continues the full application migration and integrates the
controllers with GPUI surfaces and effect executors. This document does not claim
completion of the full migration.

Selected tree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`,
`feat/native-gpui-app`. No other checkout was edited. No staging, commits, manifests,
locks, prototype changes, shared parity ledger changes, release operations, real
profiles, clipboard, credentials or Keychain operations were performed by this lane.
Parent supplied the documented `launcher::text` export and refreshed shared locks.

## Files and interfaces

| Owned file/module | Interface and behavior |
| --- | --- |
| `tezbar-ui-model/src/launcher.rs` | `Row`/`Action` serde wire types; `input_mode`, `ai_boot`, request normalization/history; validated integer and multi-field `Arguments`; borrowed optimistic filtering using core `variants`, `tokens`, `app_match`, `surface_boost`; complete metadata coverage/deep recommendation; stable candidate merge; bounded `ResultCache`; keyboard and two-step `escape_result`; owner-token `EscapeGate`; Quick Look navigation; release-note row policy and recents |
| `launcher/session.rs` | Promoted `SearchSession<R>` and all six unchanged prototype regression case bodies; fixed 8 ms coalescing, one running request/one replaceable pending input, generation suppression, one-second deferred Enter, repeat suppression, stale click rejection, activation ownership; explicit transport-epoch reset for backend recovery |
| `launcher/controller.rs` | `Launcher::update(Input, now_ms) -> Vec<Effect>`; IME suppression, raw AI sentinel preservation, mode-specific search/path/application/chat-history effects, invalidation/backend restart, identity-bearing click and activation tickets, failed activation keeps input/error, successful current activation hides; pending speculative cached rows never authorize activation; completed one-UTF-16-unit Basic local results are authoritative |
| `ui_state.rs`, `ui_state/tests.rs` | `UiState`, `Pin`, `Theme`, terminal defaults, import/export/validation; existing valid native state wins, corrupt existing native file is an error and is not overwritten; unknown native JSON fields preserved; import report identifies rejected keys and ignored caches; explicit-path atomic private-file write adapter and temporary-directory fixture |
| `notes.rs` | Sanitized rich HTML subset; `Document::{load,html,replace,format,toggle_style,enter}` using code-point selections; block/list and inline formatting preserved, paste sanitization, list continuation, bounded delta undo/redo; title/summary derivation; `Notes` CRUD and revision/identity-owned 260 ms autosave, 1200 ms saved feedback, retry, stale/deleted-save suppression, per-note dirty drafts |
| `utilities.rs` | `Clipboard` typed content/filter/selection/restore/delete/pin/clear/image/convert/QR/vault-draft effects; secure previews and conversions concealed; bounded lazy image storage; snippets `Crud`/`SnippetDraft` with readonly safeguards, field/trigger bounds and uniqueness, confirmation for snippet deletion, revision-owned saves; legacy `Quicklink` schema only; `EmojiPicker` visible grid/recent identity and hide-then-paste effect; bounded UTF-16-accounted LRU `StringCache`; visible/manual `UtilityPoll<T>`, `Permissions`, paged `Indexing` with stale filter suppression/retry |
| `tezbar-core/src/instant.rs`, `instant/calculator.rs` | Bounded recursive-descent parser/evaluator; arithmetic, functions/constants, degrees, percentage-of, length/mass/temperature conversion; finite output, nesting and input limits. No eval, scripting engine or subprocess |
| `instant/currency.rs` | Natural/multilingual parsing, owned aliases, Fuse-compatible fuzzy matching, preference frequency/recency/pin policy; generation-owned rate effects and finite conversion; hook-equivalent `ScheduledConversion` at 110 ms; injected `AmountFormatter` for target/source currency text |
| `instant/color.rs` | CSS hex/RGB/HSL parsing, clamping and alpha; stable format keys/order, HEX/RGB/percent/HSLA/HSVA clipboard strings |
| `instant/transform.rs`, `transform_tests.rs` | JSON insertion/integer-key ordering; Unicode base64 and URL encoding; MD5/SHA-256/SHA-512; counts and report; all case styles; timestamp parsing with injected clock; clipboard auto-detection actions; one ordered JSON parse with indexed duplicates and JS finite-number display; structured locale formatter seam; bounded input |
| `instant/qr.rs` | Offline byte-mode level-M versions 1–10 QR, SVG compatibility data URL, direct 512×512 PNG and PNG data URL through existing image and optional qrcode; 213 UTF-8 byte cap; independent matrix decoder tests include multiblock RS and Unicode |
| `instant/emoji.rs`, `fuzzy.rs` and data/fixture JSON | All 585 project emoji rows and all 98 project currency aliases; UTF-16 Bitap ignore-location matching, field norm/weights and keyword product scoring; source-order ties, query expansion/stemming/category filtering; 57 source-generated top-20 fixtures. Fuse Apache-2.0 license retained alongside the adaptation |
| `tezbar-update/src/notes.rs` | `Notes::{open,complete,close}`, typed fetch/cancel effects, bundled current-version notes without HTTP, missing-bundle error without remote substitution, exact-tag verification, untruncated Markdown, bounded response, stale/cancel suppression; first-install/read/unread policy |
| `tezbar-update/src/lib.rs` | Only the necessary `pub mod notes;` export added by this lane |

Quicklinks have **no reachable first-party CRUD** in current TS or the ledger. Their
existing core `search/quick-links.json` shape (`id,name,template,createdAt`) and
`search:all`/`open-url` activation stay intact. No new quicklink channel, store or edit
surface is introduced. Parent clarified this after exact-source inspection.

## Source cases and provenance

[Machine-readable mapping](launcher-utilities-test-map.json) maps **170 original TS
cases to 169 distinct Rust test functions** (the two new-item routing cases share one
Rust function). `it.each` expansions count individually. It also identifies the six
prototype session cases and all source/table hashes. The original 28 updater-policy
cases remain in their existing module and are not duplicated.

51 launcher cases replay 102 recorded calls against destination functions. Expected
values were captured from the unchanged project TS implementations; original Vitest
assertions were independently run in the 170-case gate. Replay normalization only
normalizes JSON numeric representation (`0`/`0.0`) and optional wire defaults.
52 textTransformers cases and eight textTransform clipboard cases have distinct named
Rust tests. QR structural tests independently read the selected format mask, skip
functional modules and deinterleave data blocks before decoding payload bytes.

The 585 emoji records and 98 currency aliases were compared as decoded JS strings
against the actual TS exports, not merely counted. Data source hashes and the installed
Fuse source/license hashes are in the mapping. No external emoji corpus, GPL Zed
crate or second primary search/ranking implementation was introduced.

One source quirk is deliberately retained: the existing expanded mood queries (for
example `happy`) yield no Fuse results, and a stop-word-only query such as `the` yields
the browse list. The 57 fixture cases capture exact top-20 results for browse,
category, slang/mood, stemmed, accent, case, typo and country queries.

## Durable state and bounds

The spec says “15 browser-storage keys,” but enumerates **14 literal keys**. Ledger
P-15 is existing native window positions/layout profiles, requiring no browser import.
All 14 literal keys are accounted for; home/search-candidate caches are deliberately
not imported or exported. No real browser storage was discovered or opened.

Pins cap at 12 and renumber by insertion order; recent extension IDs cap at 20;
launcher query history keeps **one** entry; emojis cap at 24. Terminal session IDs
retain their exact text with a defensive 512-entry bound. Preferences preserve valid
three-letter currency codes; raw timestamps reuse the already-ported JS Number
parser, including hexadecimal/binary/octal forms. Auto-update is enabled only for
literal `true`, preserving current TS behavior. Native unknown fields round-trip.

Native state is limited to 4 MiB; one legacy key to 2 MiB. The file adapter writes a
same-directory `create_new` private temporary file, syncs it, renames atomically,
syncs the directory on Unix and cleans up on error. Fixtures only use a disposable
TempDir. Real WebKit/WebView2 read-only discovery and staged Windows/macOS migration
remain M11.

Candidate cache caps at 1000; home at 40; exact-query LRU at 24×100 rows. Filtering
borrows rows and clones shared row handles rather than icon payloads. Clipboard image
previews cap at eight entries and 16 MiB total. Text transforms cap at 1 MiB; calculator,
colour, currency and emoji input at 4096 bytes. Notes cap at 1 MiB with nesting guards;
undo stores changed spans, max 100 entries/4 MiB per draft, and open note drafts have
count/retained-size guards. No whole history/tree clone occurs on each editor input.

## Historical initial validation (superseded by repair evidence)

Raw logs: `/tmp/tezbar-native-m03-launcher-utilities-evidence/`.
All Cargo commands used `CARGO_BUILD_JOBS=2`, existing locks and the assigned feature.
No all-features or GPUI build was used by this lane.

| Gate | Result / log |
| --- | --- |
| Original assigned TS tests, 19 files | **170 pass**, zero skips — `ts-original.log` |
| UI-model with `launcher-models` | **138 pass**, zero skips (includes the existing 28 updater-policy tests) — `ui-handoff.log` |
| Core with `native-ui-instant`, full package | **651 pass**, 8 existing ignored — `core-final-accepted.log` |
| Latest instant-only regression gate | **87 pass**, zero skips, 572 unrelated filtered — `core-instant-final.log` |
| Portable updater notes, no default features | **9 pass**, 8 unrelated tests filtered — `update-notes-final.log` |
| Rust 1.85 UI-model + transitive core instant check | Passed — `msrv-handoff.log` |
| Rust 1.85 dependency-light updater check | Passed — `msrv-notes-final.log` |
| Owned TS data equality | Passed — `owned-tables.log` |

XERJ initially had an in-use workspace state; final reindex failed with the server
parent-memory circuit breaker (RSS 1962 MiB, 768 MiB write watermark). Bounded index
queries were followed by exact source reads; the incomplete index is never absence
evidence. See `xerj-final.log`. No shared search server was restarted.

Initial failure logs are retained: the first finite-result display guard failure;
initial compiler errors while assembling the destination modules; JSON fixture
escaping/numeric representation repairs; and the early emoji expectation corrected
against source-generated fixtures. No source test was removed or skipped.

## Integration and view acceptance

Parent can now enable `launcher-models` and connect the typed controller effects to
its `CoreClient` and host adapter. `Search` goes to the existing core search,
`CompletePath`/`CompleteApplications` to completion, `SearchChats` to history; renderer
callbacks feed generations/tickets back. `TerminalMode` is explicit and does not start
a PTY. Rates, note/snippet CRUD, image fetch, permissions, indexing, clipboard and
release-note effects do no work until an adapter executes them. No model, Bun process,
PTY, HTTP rate call or release-note HTTP request is started by controller construction.

No unresolved dependency or export request remains. Parent supplied the core text
export and clarified the legacy-only quicklink contract. `AmountFormatter` is the
intentional locale seam; connect it to the native platform formatter. Timestamp/count/relative data is now structured and formatted through injected `PresentationFormatter`; English fallback uses the actual local timezone and corrected numeric-auto wording. Native OS locale acceptance remains explicit. Native code always has SHA-256, so a browser insecure-context
FNV fallback is unnecessary.

Required remaining acceptance includes GPUI view wiring, marked-text/IME and native
editor selection integration, keyboard/focus/a11y, visible row sizing and icon failure
behavior, formatting/list editing interaction, screenshot/critical visual review,
platform clipboard/QR save dialogs and real source-localized currency/time display.
The CommandBar React lifecycle layout/DOM tests remain in TS; model session/recovery
semantics are covered here, while their native layout/paint equivalents belong to the
GPUI acceptance wave. No current native view or full-app performance claim is made.
