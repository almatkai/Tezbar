# Native launcher prototype (GPUI) — design and implementation spec

Status: design spec, 2026-11-05. Not implemented. Branch `feat/native-launcher-prototype`,
based on `d2467f6c509ee6107e6103c5e96b90e946cf7c58` (Rust-core work, not `origin/main`).

## Decision this prototype informs

The approved question: does a native Rust/GPUI launcher (a search input and a result list
backed by the existing Rust search) reduce whole-app memory, CPU and latency enough to justify
moving more UI off the WebView? The prototype answers only that. It is not a frontend rewrite.
Raycast-compatible extension UI, settings, AI chat, terminal and every other surface stay in
the current WebView app. An HTML mockup cannot answer this question.

Labels used below:

- **Verified**: checked in this tree or in an official upstream source on 2026-11-05, with
  a reference.
- **Decision**: proposed by this spec. The implementer may change it only with a recorded reason.
- **Open**: unknown until implementation or measurement.

No benchmark result is predicted here. The prototype has not been built or run.

---

## 1. Scope

In scope:

1. A standalone GPUI executable with one launcher window: an editable search field, a
   virtualized result list and a footer.
2. Results come only from the existing Rust `search:all` handler. Activation goes only
   through the existing `search:execute` handler.
3. A feature-gated facade inside the `tezbar` crate. It is the prototype's only way into
   `native_core`.
4. An isolated identity, profile and control channel for measurement.
5. A matched measurement on macOS and, where the host allows it, Windows.

Out of scope, and visibly unsupported where a result needs it: pins and recents, AI mode,
terminal mode (`>`), application mode (`` ` ``), the Deep Search UI, inline arguments,
extension views, the vault UI, quick-look, the calculator, currency and color rows, which the
renderer computes today, resize and scale profiles, the global hotkey, the tray and the updater.

---

## 2. Visual brief

The prototype must look like the current Tezbar launcher, rebuilt natively. It is not a new
brand. All values come from the current sources: `src/renderer/styles.css` (the "Launcher —
single sheet" block and the late `--ln-*` overrides at lines ~2084–2126 win the cascade),
`src/renderer/ui/CommandSurface.tsx`, `src/renderer/ui/primitives.tsx` and
`src/renderer/CommandBar.tsx` (result rows ~5230–5570, footer ~5740–5840).

### 2.1 Composition

```
┌──────────────────────────────────────────────────────────── 760 px ──┐  radius 18, 1px inset rim
│  ┌──┐                                                                 │
│  │⌕ │  saf▌                                                     44 px │  header, 14 px side inset, 10 px gap
│  └──┘                                                                 │
├───────────────────────────────────────────────────────────────────────┤  1 px hairline
│ ┌─────────────────────────────────────────────────────────────────┐   │  list: 6 px side inset, 6 px top
│ │ ▣  Safari  /Applications/Safari.app                Application  │   │  36 px row, selected = accent fill
│ │ ▣  Safari Technology Preview  /Applications/…      Application  │   │
│ │ ▤  safari-receipts.txt  ~/Documents                       File  │   │
│ │ ▢  Add quick note: saf  Quick notes                       Note  │   │
│ │ ⋮  (virtualized; last row clears the footer by 38 px)           │   │
├───────────────────────────────────────────────────────────────────────┤  1 px hairline
│  24 results                                    Open ↵     Hide Esc    │  32 px footer, floats over the list
└───────────────────────────────────────────────────────────────────────┘  height 640 px
```

Wireframe glyphs stand for 22 px icons. They are not literal characters.

| Element | Value (Verified source) |
| --- | --- |
| Window | 760 × 640 logical px, the size a new display gets (`docs/launcher-layout.md`). Frameless, no native shadow, transparent outside the rounded sheet. Prototype: fixed size, centered on the active display. |
| Sheet | radius 18 (`--r-window: 18px`), fill `--ln-sheet`, rim `inset 0 0 0 1px var(--ln-rim)` drawn above the content. The command surface is a "sheet" surface with no inner padding (`App.tsx` `SHEET_SURFACES`). |
| Header row | height 44, horizontal padding 14 (`px-3.5`), item gap 10 (`gap-2.5`), bottom border 1 px `--ln-hairline`. |
| Search badge | 24 × 24, radius 7, fill `#5b5ef0`, white search glyph 14 × 14, stroke 2. |
| Input | Geist 15 px regular, color text-1, caret in text-1. Placeholder in text-4. |
| List | 6 px side inset (`px-1.5`), 6 px top padding, bottom padding 32 + 6 = 38 px so the last row clears the floating footer. |
| Row | height 36 (min-height 36, padding 7 × 8), radius 8 (`--r-row`), gap 10 between icon, text and tag. |
| Row icon | 22 × 22, radius 6. Real app/file icon if available, otherwise a category glyph (§2.5). |
| Title | 13 px, weight 500, text-1, at most 70% of the text column, single line, ellipsis. |
| Subtitle | 12 px, text-4, baseline-aligned 8 px after the title, ellipsis. |
| Kind tag | 10.5 px, line height 14, padding 2 × 6, radius 5, fill `--ln-pill`, text-3. Labels from `SEARCH_CATEGORY_LABELS` (`Application`, `File`, `Document`, `Note`, `Vault`, `Extension`, `Command`, `Snippet`, `Quicklink`, `Clipboard`, `Store`). |
| Footer | height 32, left padding 8, right padding 12, top border 1 px `--ln-hairline`, fill `--ln-bar-float`, floats over the list (absolute bottom). |
| Footer hint | label 11 px text-3, then a key group: height 16, radius 5, padding 0 × 5, fill `--ln-pill-strong`, keys 10 px text-2. 12 px between hints. |
| Footer status (left) | 11 px text-3, single line, ellipsis. |

### 2.2 Color tokens

Exact values from `styles.css`. RGB triplets are opaque unless an alpha is shown.

| Token | Dark (default) | Light (`data-theme='white'`) |
| --- | --- | --- |
| `--ln-sheet` | `rgba(28,29,33,0.95)` | `rgba(250,250,251,0.96)` |
| `--ln-rim` | `rgba(255,255,255,0.08)` | `rgba(15,23,42,0.20)` |
| `--ln-hairline` | `rgba(255,255,255,0.065)` | `rgba(15,23,42,0.10)` |
| `--ln-hover` | `rgba(255,255,255,0.05)` | `rgba(15,23,42,0.045)` |
| `--ln-pill` | `rgba(255,255,255,0.06)` | `rgba(15,23,42,0.05)` |
| `--ln-pill-strong` | `rgba(255,255,255,0.09)` | `rgba(15,23,42,0.07)` |
| `--ln-bar-float` | `rgba(22,23,27,0.78)` | `rgba(247,247,249,0.82)` |
| `--ln-accent` (selected row) | `#4f52d9` | `#4f52d9` (not overridden) |
| Search badge | `#5b5ef0` | `#5b5ef0` |
| text-1 | `rgb(245,246,250)` | `rgb(27,37,57)` |
| text-2 | `rgb(188,190,200)` | `rgb(62,76,101)` |
| text-3 | `rgb(134,138,150)` | `rgb(91,106,129)` |
| text-4 | `rgb(94,98,112)` | `rgb(99,113,135)` |
| danger | `rgb(248,113,113)` | `rgb(220,38,38)` |
| Selected row: title / subtitle / tag | `#fff` / `rgba(255,255,255,0.72)` / `#fff` on `rgba(255,255,255,0.16)` | same |

**Decision**: the theme follows the OS appearance and updates live. A `--theme light|dark`
launch flag exists for QA screenshots. The renderer's `tezbar:theme` value lives in WebView
`localStorage`, so the prototype cannot read it.

**Decision**: on macOS, use GPUI `WindowBackgroundAppearance::Blurred` with the 0.95-alpha sheet
fill, matching the current `backdrop-filter: blur(40px)` sheet. On Windows, use an opaque sheet
in the same RGB. Visual QA records which mode each platform actually rendered.

### 2.3 Type

- Family: `Geist` if it is installed locally, otherwise the system UI face (`.SystemUIFont`
  on macOS, `Segoe UI` on Windows), configured as GPUI font fallbacks. The current app also
  uses Geist only when installed (`index.html` comment). Do not bundle a font.
- Font features `cv11`, `ss01`, `ss03` when Geist resolves. Letter-spacing `-0.005em` may be
  omitted if GPUI cannot express it; record the deviation.
- Only these sizes: 15 (input), 13/500 (title and empty-state line), 12 (subtitle), 11 (footer),
  10.5 (tag), 10 (keycaps). No bold headings, no display type.

### 2.4 States

Each state needs a reference screenshot in light and dark during visual QA (§8).

| State | Presentation |
| --- | --- |
| Core starting | Input editable and focused. List empty. Footer status `Starting search…`. Typing is accepted and searched as soon as the core is ready (latest text only). |
| Core failed to start | List area shows one 13 px line in danger: `Search couldn't start: {core error}`. Footer: `Hide Esc`. |
| Empty input | The list renders whatever the core returns for `search:all("")`; if that is empty, one 13 px text-3 line at 16 px from the list top: `Type to search apps, files, notes and commands.` No home rail, no pins. |
| Typing / waiting | Previous results stay on screen; never clear them and never flash an empty list. After 150 ms with a request outstanding, footer status shows `Searching…`. No spinner, no skeleton rows, no shimmer. |
| Results | Rows as in §2.1. Footer status `{n} results` (`1 result`). The first row is selected when a new generation arrives. |
| Selected row | Full-row fill `--ln-accent`, radius 8, white text treatment (§2.2). This is the only accent-filled element. |
| Pointer over row | Real pointer movement over a row selects it, as `handleRowMouseMove` does today. Scrolling under a still pointer does not change selection. There is no separate hover fill. |
| Keyboard focus | Focus lives in the input; the caret is the focus indicator, as today. Rows are not tab stops. No focus rings, glows or halos. |
| No results | One 13 px text-2 line `No results for “{query}”` at 16 px from the list top. (Rare: the core appends an "Add quick note" row for any non-empty query.) |
| Search error | The list is replaced by one 13 px danger line `Search failed: {core error}`. The stale list is not kept, because it no longer matches the input. |
| Unsupported row | Same layout and rank as any row. The tag text becomes `Not in prototype`, 10.5 px text-4, without the pill fill. Selection still fills it with accent. Enter shows footer status `{title} isn't available in the prototype` for 3 s and does nothing else. |
| Activating | Footer status `Opening {title}…` after 150 ms. Further Enter presses are ignored. |
| Activation failed | Footer status in danger: `Couldn't open {title}: {message}`. Window stays open and the selection stays put. |
| Activation succeeded | Window hides. The next show starts with an empty input. |

Footer hints: `Open ↵` when the selected row is supported (omit it otherwise), then `Hide Esc`.
No hints for features the prototype lacks (no Pin, Save note, Deep Search, Quick Look).

### 2.5 Icons

- **Decision**: application and file icons come from the existing core channel
  `asset-icon:data-url` (`native_core/system/assets.rs`, Verified). The prototype decodes the
  returned PNG data URL into a GPUI image. It requests icons only for rows inside the
  virtualized viewport, keeps an LRU of at most 128 entries keyed by `kind:path`, and drops it
  on hide. This mirrors the renderer's `loadAssetIcon` cache.
- When no icon is available, show a 22 × 22 radius-6 tile in `--ln-pill-strong` holding a 14 px
  monochrome glyph in text-2, one glyph per category: app grid, document, note, key (vault),
  puzzle (extension), command chevron, snippet braces, link. Do not invent colored category tiles.

### 2.6 Motion

- No entrance animation and no fade. The window appears complete on its first frame.
- The selection fill moves instantly. The current `GlideList` slide is optional; if added, it
  is at most 90 ms ease-out and disabled under reduced motion.
- No rolling placeholder. The current `RollingText` hint is replaced by a static placeholder
  `Search apps, files and commands`.

### 2.7 Accent tension (read before review)

The brief bans "default purple-blue accents". Tezbar's existing launcher already uses indigo in
exactly two places: the selected-row fill `#4f52d9` and the search badge `#5b5ef0`. These are
the product's established tokens, not a template default. **Decision**: keep both, exactly where
they are today, and nowhere else. The Opus reviewer should treat any additional indigo/violet as
a defect: tinted borders, focus glows, gradients, colored footer text or an accent-colored scrollbar.

### 2.8 Prohibited

Reject the prototype in review if it contains any of these:

- Gradients of any kind, including the legacy `.glass-shell` top-light gradient (the current
  launcher overrides it away).
- Glass cards or panels inside the sheet; nested rounded containers; drop shadows under rows.
- Colored accent stripes on any side of a row, banner or status. That includes the current
  pinned-row amber bar, since pins are out of scope.
- Decorative badges, "NEW"/"BETA" chips, a prototype watermark, counters in pills, status dots.
- Identical icon grids, category tiles in different hues, emoji as icons.
- ALL-CAPS labels, section eyebrows, numbered markers.
- Spinners, skeletons and shimmer in the result list.
- Generic copy ("Oops", "Something went wrong", "Let's get started").
- Any accent use beyond §2.7.

---

## 3. Behaviour

Product-level rules come first. Implementation notes are kept to the minimum needed to make them
testable.

### 3.1 Text input

- **Real editable text** on GPUI's `EntityInputHandler`, modeled on the official
  `crates/gpui/examples/input.rs` at the pinned revision. Verified: that example implements
  `replace_and_mark_text_in_range`, `marked_text_range`, `bounds_for_range`,
  `character_index_for_point`, select-all, copy, cut and paste.
- **IME**: marked (composing) text renders with an underline; candidate windows anchor at
  `bounds_for_range`. While marked text exists, Enter commits the composition and does not
  activate, Esc cancels the composition and does not hide, and Up/Down go to the IME.
- **Decision**: only committed text is searched; marked text is not sent to the core. This
  differs from the WebView, which fires `input` during composition. Excluding IME from the
  latency script keeps the comparison fair.
- **Selection and clipboard**: caret movement, Shift+arrows, word movement (Alt+arrows on
  macOS, Ctrl+arrows on Windows), Home/End, select-all, copy, cut, paste, mouse click and drag
  selection, double-click word selection. Newlines in pasted text become spaces; the field is
  single-line. Paste uses GPUI's platform clipboard only on an explicit user paste.
- Undo/redo is not required. If absent, list it as a limitation.

Key bindings:

| Action | macOS | Windows |
| --- | --- | --- |
| Select all / copy / cut / paste | ⌘A / ⌘C / ⌘X / ⌘V | Ctrl+A / Ctrl+C / Ctrl+X / Ctrl+V |
| Word left/right (with Shift: select) | ⌥← / ⌥→ | Ctrl+← / Ctrl+→ |
| Line start/end | ⌘← / ⌘→, Home/End | Home / End |
| Delete word back | ⌥⌫ | Ctrl+Backspace |
| Character palette | ⌃⌘Space | Win+. (OS) |
| Move selection | ↑ / ↓ | ↑ / ↓ |
| Activate selected | ↵ | Enter |
| Hide | Esc | Esc |
| Quit prototype | ⌘Q | Alt+F4 |

Up at the first row and Down at the last row do nothing (no wrap). Held arrow keys repeat.
A held Enter does not repeat (see §3.4).

### 3.2 Search flow

- Each committed text change creates a **generation** `g` (monotonic `u64`) for the exact
  input string.
- There is at most one request to the core in flight and one pending replacement. A new input
  replaces the pending one. Requests are coalesced in an 8 ms window. That is not a typing
  debounce: the deadline is not extended by more input. This mirrors
  `src/renderer/latestSearch.ts`. Core requests cannot be cancelled; the UI owns stale-response
  suppression.
- A response is applied only if its generation equals the generation of the current input.
  Older responses are dropped, even if they arrive late and out of order. A dropped response
  never changes the list, the selection, the footer or the icon requests.
- The query string goes to `search:all` byte-for-byte as committed, prefix characters
  included. The prototype does no trimming, ranking, merging, deduplication or optimistic
  first-frame scoring. Verified: the renderer currently does optimistic client-side ranking
  (`src/renderer/optimisticSearch.ts`), but the prototype must not copy it. The core already
  ranks, deduplicates and truncates to 80 rows (`launcher/mod.rs:650–767`).
- Background behavior stays the core's: `search:all` calls `note_foreground()`, which pauses the
  background file scanner for 1.2 s. That is identical in both builds.

### 3.3 Result identity and selection

- A result's identity is the core's `id` string; GPUI list items are keyed by it.
- When the generation of a new response differs from the displayed one, selection resets to
  index 0. That is also the current behavior after an input change.
- The list scrolls to keep the selected row fully visible above the floating footer.
  Wheel and trackpad scrolling never move the selection.
- **Virtualization**: GPUI `uniform_list` with a fixed 36 px row. Verified: the official
  `uniform_list` example exists at the pin. Only visible rows (plus overscan) are laid out and
  request icons. The core caps rows at 80; virtualization is still required, so the prototype
  does not rely on that cap.

### 3.4 Activation ownership (no effect replay)

- `render()` is pure. Side effects happen only in action handlers (Enter, click) and in
  explicitly spawned tasks.
- An activation captures an immutable **ticket**: `{generation, resultId, rank, query, action}`,
  where `action` is the core's JSON action, opaque to the UI, and `rank` is 1-based, as today's
  `runSelectedSearchResult(item, i + 1)` sends it.
- Enter is accepted only when the displayed results belong to the current input. If a newer
  generation is pending, Enter is held (one slot) for up to 1 s. When that generation's results
  arrive, the then-selected row (index 0) is activated. If they don't arrive in time, the
  footer shows `Still searching — press Enter again` and nothing runs.
- At most one activation is in flight. Enter and click are ignored until it resolves.
  Key-repeat Enter events are ignored.
- `search:execute` is called **exactly once** per ticket, with
  `{"action": <action>, "context": {"resultId", "query", "rank"}}`. It records usage itself
  (Verified: `launcher/mod.rs:576–607` calls `db.record`), so the prototype never calls
  `search:record-usage` separately.
- Hiding, re-showing, theme changes, re-renders and late search responses never re-run a
  ticket. A ticket is dropped after it resolves.
- On `{"ok": true}` the window hides and the input clears. On `{"ok": false, "message"}`, or on
  an error, the window stays open and shows the failure (§2.4).

### 3.5 Which actions work

Classification follows the current `LauncherCore::execute` (`launcher/mod.rs:769–953`) and the
platform handlers. A row's support status is set once by the facade, not by the UI.

| Core action type | Prototype | Why (Verified) |
| --- | --- | --- |
| `open-app` | **Works** | `core:platform:open-app` uses `/usr/bin/open -a` on macOS and `explorer.exe`/`open_target` on Windows. No `AppHandle` needed. Needs the facade's desktop-services flag (§4.3) so `apps` is populated. |
| `open-file` | **Works** | `open_target`, no `AppHandle`. |
| `open-url` | **Works** | `open_target`. |
| `open-with-app` | **Works** | Platform command, no `AppHandle`. |
| `add-note` | **Works** | Writes to the *fixture* profile's notes, then refreshes the index. |
| `copy-text` (incl. snippets) | **Works in code; live QA needs approval** | Writes the real OS clipboard through the clipboard worker. Automated QA must not run it on the user's session (§7). |
| `copy-and-paste-text` | Unsupported | `core:platform:paste-text` needs an `AppHandle` to hide the app and synthesizes ⌘V. |
| `run-extension-command` | Unsupported | May open an extension view (WebView) or need the extension worker UI. |
| `invoke-command` (e.g. "Deep Search …") | Unsupported | Routes to `core:commands:execute`, which opens app surfaces. |
| `run-native-command` | Unsupported | `core:native:execute` commands may emit UI surface events; not audited per command. |
| `run-shell` | Unsupported | Out of scope for a measurement prototype. |
| `install-extension` | Unsupported | Needs safety-authorization UI. |
| `open-vault-item` | Unsupported | Renderer-only vault flow. |
| Anything unknown | Unsupported | Default. |

Unsupported rows stay at their rank (§2.4). The prototype never reorders or filters results to
hide them. Their share of results is part of what this prototype reveals.

### 3.6 Window lifecycle

- `show`: brings the window forward on the active display, focuses the input, selects nothing in
  the text. On macOS the app runs as an agent: `LSUIElement` in the prototype bundle, so no Dock
  icon.
- `hide`: on Esc, after a successful activation, or on focus loss (click outside). On macOS
  this returns focus to the previously active app, like the current hide.
- 5 s after a hide, if the window is still hidden, call the facade's `trim_idle_memory()`. That
  mirrors `schedule_idle_memory_trim` (`src-tauri/src/lib.rs:280–300`, Verified) and keeps the
  memory comparison fair.
- No global hotkey. The window is shown via the control channel (§6) or the `--show` launch
  flag. Never register `Alt+Space`/`Control+Space`.

---

## 4. Seam to the Rust core

### 4.1 Current entrypoints (Verified)

| Fact | Reference |
| --- | --- |
| `native_core` is a private module of the `tezbar` crate (lib name `app_lib`). | `src-tauri/src/lib.rs:5` `mod native_core;` |
| `NativeCore::new(ctx) -> Result<Arc<Self>, String>`, `start()`, `request(channel, args)`, `trim_idle_memory()`, `shutdown()`. `request` rejects `core:` channels and waits up to 30 s for readiness. | `native_core/mod.rs:44, 73, 108, 184, 248` |
| `CoreContext::new(paths: CorePaths, app: Option<tauri::AppHandle>)`. `CorePaths` has `data_dir`, `config_dir`, `resource_dir`, `temp_dir`, `home_dir`. Events are dropped when `app` is `None`. | `native_core/common.rs:19–62` |
| The production host builds `CorePaths`, calls `CoreContext::new(paths, Some(handle))`, `NativeCore::new`, then spawns `core.start()` on Tauri's async runtime. | `lib.rs:3104–3155` |
| **Without an `AppHandle`, the launcher skips the app/file scan, the file watcher, the clipboard observer and the app enumeration.** | `launcher/mod.rs:134` (`clipboard.start(app.is_some())`), `:160` (scan and watch), `:354` (refresh apps), `:777` (open-app discovery) |
| The scanners do not use the `AppHandle`. `providers::applications`, `scan` and `watch` take `CoreContext` and paths only. | `launcher/providers.rs:59, 368, 454` |
| Global shortcuts are skipped without an `AppHandle`. | `platform.rs:73–76` |
| `native_core` calls crate-root functions that need Tauri windows (`crate::set_backend_app_visibility`, `crate::hide_window`, `crate::native_input::press_key`, …). They are reachable only through channels the prototype never calls. | `rg "crate::" src-tauri/src/native_core` |
| `CoreContext` depends on the `tauri::AppHandle` type, so any consumer of `native_core` compiles and links the `tauri` crate (and wry/WebKit on macOS) even if it never creates a Tauri app. | `common.rs:9, 31` |
| Production `desktop-bench` uses a Unix socket and does not compile on Windows. | `desktop_bench.rs` (`tokio::net::UnixListener`, `std::os::unix`) |
| Startup does not touch Keychain/Vault: `VaultService::new` stores handles only; vault search rows come from the on-disk index (`stores.rs:423`); AI secrets load lazily per connection. The fixture profile has no vault or AI connections. | `vault/mod.rs:48`, `ai/ext/secrets.rs` |

### 4.2 Decision: a feature-gated facade in the `tezbar` crate

Add one public module behind a new Cargo feature, `native-launcher-facade`. Default builds,
release packaging and `desktop-bench` do not enable it. `native_core` stays private. The facade
is a deep module: a handful of methods hide `CoreContext` construction, isolation checks,
readiness, JSON projection, action classification and the activation envelope.

```rust
// src-tauri/src/launcher_facade.rs — #[cfg(feature = "native-launcher-facade")]
//! The only out-of-crate entry into native_core, for isolated launcher prototypes.
//! Never opens the production profile. Never creates Tauri windows.

pub struct PrototypeProfile { /* validated, private */ }
impl PrototypeProfile {
    /// `root`: absolute canonical path inside std::env::temp_dir() containing the marker
    /// `.tezbar-native-launcher-prototype`. Derives data/, config/, home/, temp/.
    /// `resource_dir`: read-only Resources of a Tezbar build of the same source SHA.
    /// Refuses the production data dir, ~/.openray and any path outside the OS temp dir.
    pub fn open(root: &Path, resource_dir: &Path) -> Result<Self, String>;
}

pub struct Launcher { /* Arc<NativeCore> */ }
impl Launcher {
    /// Call inside a Tokio runtime. Builds CoreContext (no AppHandle, desktop services on),
    /// NativeCore::new, and returns once the core is constructed. Readiness is `ready()`.
    pub fn start(profile: &PrototypeProfile) -> Result<Self, String>;
    pub async fn ready(&self) -> Result<(), String>;
    /// `search:all` verbatim; projects rows. No ranking, merging or filtering.
    pub async fn search(&self, query: &str) -> Result<Vec<Row>, String>;
    /// Exactly one `search:execute` for supported rows; unsupported rows never reach the core.
    pub async fn activate(&self, ticket: Ticket) -> Activation;
    /// `asset-icon:data-url`, decoded to PNG bytes.
    pub async fn icon(&self, icon: &IconRef) -> Option<Vec<u8>>;
    pub async fn trim_idle_memory(&self);
    pub async fn shutdown(&self);
}

pub struct Row { pub id: String, pub title: String, pub subtitle: String,
                 pub category: String, pub support: Support, pub icon: Option<IconRef>,
                 /* private: action JSON */ }
impl Row { pub fn ticket(&self, query: &str, rank: usize) -> Ticket; }
pub enum Support { Native, Unsupported }
pub enum Activation { Done, Failed(String), Unsupported }
```

The interface is the test surface. Facade tests run headless against a temporary profile
(feature-gated `cargo test --features native-launcher-facade`). They cover: refusing
non-temp/production paths; `search` returning core rows in core order for a seeded fixture;
unsupported rows never calling `search:execute`; one usage record per activation; no
`AppHandle`-only channel being reached.

### 4.3 Minimal production-source changes

These are the only source edits outside the prototype package and the new facade file.
Default-build behavior must stay identical.

1. `src-tauri/Cargo.toml`: `native-launcher-facade = []` under `[features]`. No new
   dependencies; `src-tauri/Cargo.lock` must stay byte-identical.
2. `src-tauri/src/lib.rs`: `#[cfg(feature = "native-launcher-facade")] pub mod launcher_facade;`
3. `native_core/common.rs`: add a private `desktop_services: bool` to `CoreContext`. It equals
   `app.is_some()` in `new()`. Add `pub fn runs_desktop_services(&self) -> bool`, plus a
   `#[cfg(feature = "native-launcher-facade")] pub(crate) fn with_headless_desktop_services(self) -> Self`.
4. `native_core/launcher/mod.rs`: replace the four `ctx.app.is_some()` checks at lines 134, 160,
   354 and 777 with `ctx.runs_desktop_services()`. Without the feature, the flag equals
   `app.is_some()`, so behavior is unchanged. Do not change `platform.rs` shortcut gating.
5. Tests: the existing `cargo test --locked --lib` passes unchanged. Add one test showing a
   headless context still skips scanning by default. Run `cargo +1.85.0 check --locked --lib`
   and `cargo +1.85.0 check --locked --lib --features native-launcher-facade` so the facade
   respects the app's 1.85 MSRV.

Rejected alternatives:

- **Extract `native_core` into a shared crate now.** It depends on `tauri::AppHandle` and on
  crate-root window functions (§4.1). Decoupling it is a broad production refactor the
  prototype doesn't need. Revisit only if the measurement justifies migration.
- **Add GPUI as an optional dependency of `src-tauri`.** Optional dependencies still enter
  `src-tauri/Cargo.lock` and can move shared crate versions for the production app.
- **A GPUI client that talks to a running Tauri app over IPC.** The measurement would then
  include both processes and both UI stacks, which answers nothing.

### 4.4 GPUI and Tauri event loops: limits

- Verified by design: GPUI's `Application::run` and Tauri/tao's `EventLoop::run` each take
  over the main thread and the platform application object (`NSApplication` delegate and run
  loop on macOS), and neither returns. They cannot both run in one process. The prototype
  process runs **only** GPUI: no `tauri::Builder`, no tao event loop, no WebView, no tray, no
  single-instance plugin.
- The core still needs Tokio. **Decision**: `main()` builds one multi-threaded Tokio runtime
  with the default worker count (cores). That matches the default `tauri::async_runtime`
  shape, which matters for a fair thread/stack comparison. The runtime is handed to GPUI via
  the official `gpui_tokio::init_from_handle` (Verified at the pin). UI code awaits core work
  through `gpui_tokio::Tokio::spawn` and never blocks the main thread. Shutdown runs
  `Launcher::shutdown()` from GPUI's app-quit hook, with a 5 s bound.
- The prototype binary still **links** `tauri`, `wry` and, on macOS, `WebKit.framework`
  (§4.1), so dyld loads WebKit at startup even though no WebView is created. That biases
  memory **against** the prototype. Record the WebKit image's dirty/resident size from
  `vmmap -summary` so the bias is quantified, not guessed.
- Not answered here, and needed later for "Raycast UI stays WebView on demand": a WebView
  surface beside a GPUI launcher would need either a separate on-demand WebView process or a
  raw `wry` WebView parented to a GPUI window via raw window handles. Neither is verified, and
  neither belongs in this prototype.

### 4.5 Prototype package layout

```
prototypes/native-launcher/          # standalone Cargo package, own [workspace], own Cargo.lock
  Cargo.toml
  Cargo.lock                         # seeded from src-tauri/Cargo.lock (§5.5)
  rust-toolchain.toml                # channel = "1.98.1"
  src/main.rs                        # runtime + GPUI app bootstrap, flags
  src/launcher_view.rs               # header, list, footer, states (§2, §3)
  src/text_input.rs                  # EntityInputHandler field (from gpui examples/input.rs)
  src/search_session.rs              # generations, latest-wins, activation ticket (§3.2–3.4)
  src/theme.rs                       # tokens from §2.2 only
  src/bench_control.rs               # #[cfg(feature = "bench-control")] socket / named pipe
  macos/Info.plist                   # com.tezbar.prototype.nativelauncher, LSUIElement
  scripts/bundle-macos.sh            # assemble .app, ad-hoc sign (local preview only)
```

Add `prototypes/native-launcher/target/` to `.gitignore`. `search_session.rs` is plain Rust
with no GPUI types, so generation/staleness/ticket rules are unit-tested without a window.

---

## 5. Dependencies and build feasibility

### 5.1 Upstream facts (Verified 2026-11-05, primary sources only)

| Fact | Source |
| --- | --- |
| Official repository `zed-industries/zed`: owner `zed-industries`, not a fork. Homepage of the `gpui` crate is `https://gpui.rs`, repository `https://github.com/zed-industries/zed`. | GitHub API `repos/zed-industries/zed`; crates.io API `crates/gpui` |
| crates.io `gpui` latest is **0.2.2**, published 2025-10-22 by `mikayla-maki`. It is the older single-crate layout (platform code inside `gpui`, `cocoa 0.26`, `metal 0.29`, `windows 0.61`); it does have a `runtime_shaders` feature. | crates.io API `crates/gpui`, `crates/gpui/0.2.2` |
| Upstream now splits platforms: `gpui`, `gpui_platform`, `gpui_macos`, `gpui_apple`, `gpui_windows`, `gpui_tokio`, … Only `gpui` declares `publish = true`; the others inherit `publish = false` from the workspace, so they come only from git. `gpui/Cargo.toml` still says `version = "0.2.2"`. | `Cargo.toml` `[workspace.package] publish = false`; `crates/*/Cargo.toml` |
| Latest Zed **stable** tag `v1.27.0` → commit `c84af5b99cbc366d675064e52b23664a015b6e35` (2026-11-04, "v1.27.x stable"). Same crate layout; `runtime_shaders` present in `gpui_apple` and `gpui_platform`; toolchain pin `1.98.1`. | `git/ref/tags/v1.27.0`; files at that commit |
| `gpui_platform` feature `runtime_shaders = ["gpui_macos/runtime_shaders", …]` → `gpui_apple/runtime_shaders`. With it, `gpui_apple/build.rs` writes a stitched `.metal` source and does **not** run `xcrun metal`/`metallib`; `metal_renderer.rs` compiles shaders at runtime with `newLibraryWithSource:options:error:`. Without it, the build runs `xcrun metal`, which needs the Metal toolchain from Xcode. | `crates/gpui_apple/build.rs`, `crates/gpui_apple/src/metal_renderer.rs` lines 39–42, 276–290 |
| macOS text needs `gpui_platform` feature `font-kit`, or glyphs don't render. Windows needs no features (Win32 + DirectWrite). | `crates/gpui/README.md` |
| No `rust-version` is declared anywhere in the Zed workspace; the README says "use the latest version of stable Rust"; `rust-toolchain.toml` pins `1.98.1`; edition 2024. | `rust-toolchain.toml`, `Cargo.toml`, README |
| Zed's root `[patch.crates-io]` replaces `async-task` with `zed-industries/async-task` rev `015845a19fc31752c02367567d3b8900a406f9da`. Root patches do not apply to downstream git consumers. That fork is one commit ahead of `smol-rs/async-task` ("Fix task waker vtable identity"): a correctness fix, not an API change. | `Cargo.toml` line ~1040; GitHub compare API |
| `gpui_windows/build.rs` compiles HLSL with `fxc.exe` (from `GPUI_FXC_PATH`, `PATH` or the latest Windows SDK) **only in release**; debug builds compile shaders at runtime. `gpui` default feature `windows-manifest` embeds a resource with `embed-resource`, which needs a resource compiler. | `crates/gpui_windows/build.rs`, `crates/gpui/build.rs` |
| Official examples at the pin include `input.rs` (IME-capable text field with `EntityInputHandler`) and `uniform_list.rs`. `WindowKind::{Normal, PopUp, Floating}` and `WindowBackgroundAppearance::{Opaque, Transparent, Blurred}` exist. | `crates/gpui/examples/`, `crates/gpui/src/platform.rs` |

### 5.2 Local machine facts (Verified)

- macOS 26.3 arm64. `xcode-select -p` → `/Library/Developer/CommandLineTools`.
  `xcrun --find metal` fails ("unable to find utility"). SDK `MacOSX.sdk` 26.5 is present.
- rustup toolchains: `1.85.0`, `1.98.0`, `1.98.1`. **Disclosure**: `1.98.1` was auto-installed
  by rustup on 2026-11-05 during this research, when `rustc --version` ran inside a
  downloaded copy of Zed's `rust-toolchain.toml`. It is a user-level rustup change; no system
  SDK or Xcode was touched.
- The build prerequisites of `app_lib` exist in this worktree (ignored files):
  `build/embedding-runtime/resources/embedding-runtime.zip`, `dist/`, `dist-backend/`.
  `src-tauri/build.rs` requires them (embedding payload hash check; Tauri resources). A path
  dependency on `src-tauri` runs that build script, and its identity guard passes because the
  default config is used (`com.tezbar.app` / `Tezbar`).
- Windows host `DESKTOP-OB83S8J` was last documented on Rust `1.90.0-x86_64-pc-windows-msvc`
  (`docs/WINDOWS_SUPPORT.md`). Its fxc/rc availability and interactive-session access are
  **not** verified.

### 5.3 Decision: GPUI source and pin

Use **git** dependencies on the official repository, pinned to the stable tag commit:

```toml
# prototypes/native-launcher/Cargo.toml
[package]
name = "tezbar-launcher-prototype"
version = "0.0.0"
edition = "2024"
publish = false
rust-version = "1.98.1"

[workspace]   # standalone: never joins another workspace

[features]
bench-control = []

[dependencies]
app_lib = { package = "tezbar", path = "../../src-tauri", default-features = false,
            features = ["native-launcher-facade"] }
gpui = { git = "https://github.com/zed-industries/zed", rev = "c84af5b99cbc366d675064e52b23664a015b6e35" }
gpui_platform = { git = "https://github.com/zed-industries/zed", rev = "c84af5b99cbc366d675064e52b23664a015b6e35",
                  default-features = false, features = ["font-kit", "runtime_shaders"] }
gpui_tokio = { git = "https://github.com/zed-industries/zed", rev = "c84af5b99cbc366d675064e52b23664a015b6e35" }
tokio = { version = "1", features = ["rt-multi-thread", "macros", "time", "sync"] }

[patch.crates-io]
# Parity with Zed's own build (one correctness commit, §5.1).
async-task = { git = "https://github.com/zed-industries/async-task.git", rev = "015845a19fc31752c02367567d3b8900a406f9da" }
```

Why not crates.io `gpui 0.2.2`: it is a year older than the stable pin, predates the platform
split and recent macOS/Windows fixes, and its API differs from the current official examples
the field must be built from. The git pin is official, immutable and matches what Zed ships.
The cost is a large one-time git fetch of the Zed repository (GitHub reports ~540 MB packed).
Use `CARGO_NET_GIT_FETCH_WITH_CLI=true` if libgit2 struggles.

`gpui_platform`'s default feature `multithreaded` exists only for `gpui_web` (wasm), so
disabling defaults is safe on desktop. If any feature combination fails to compile at the pin,
record the exact error. Do not switch to a fork.

### 5.4 macOS build without Xcode

- **Decision**: build with `gpui_platform/runtime_shaders`. It is an official, supported
  feature (§5.1) and needs no Metal compiler at build time. Do not install Xcode, do not run
  `xcode-select --switch` and do not download a Metal toolchain. The brief forbids those
  system changes.
- Consequences to measure and label:
  1. The first window compiles Metal shaders at runtime. This adds startup time and may
     start Apple's Metal compiler XPC service. The all-process accounting (§6.3) includes it if
     it is responsibility-attributed to the prototype. Report its presence and lifetime.
  2. A production GPUI build would ship precompiled `metallib`. Results from a
     `runtime_shaders` build are labeled "runtime-shader build" and are not presented as
     production-equivalent startup numbers.
  3. The OS shader cache makes later launches faster than the first. Opening measurements
     report the first launch separately from repeated launches.
- **Open**: compile errors in `cbindgen`-generated headers or Objective-C bindings. The
  first milestone (§9, M0) exists to surface these before any launcher work.

### 5.5 Toolchain isolation and lock drift

- `prototypes/native-launcher/rust-toolchain.toml` pins `1.98.1`. It applies only to commands
  run inside that directory. The main app keeps `rust-version = "1.85.0"`; its build and the
  facade feature must still pass `cargo +1.85.0 check` (§4.3).
- Seed `prototypes/native-launcher/Cargo.lock` by copying `src-tauri/Cargo.lock`, then add the
  GPUI dependencies without `cargo update`. Cargo keeps existing entries where compatible.
- Add a drift check (small Python script, run before every measurement build). For every
  package name present in both lockfiles, report the version pair. Any version change of a crate
  the core uses (`tokio`, `rusqlite`, `serde_json`, `notify`, `walkdir`, `arboard`, `regex`,
  `unicode-normalization`, `image`, `reqwest`, `tauri`) must be listed in the measurement
  report. The goal is to show the core ran the same code in both builds.
- Use a separate `CARGO_TARGET_DIR` (`prototypes/native-launcher/target`), so the app's cached
  `src-tauri/target` is not disturbed.

### 5.6 Windows build

- The prototype needs Rust `1.98.1` on the Windows host. The documented host toolchain is
  `1.90.0`. Installing an additional user-level rustup toolchain there needs explicit user
  approval; do not assume it.
- Debug builds avoid `fxc.exe`. Release builds need it; verify with `where fxc` or a Windows
  Kits path, or set `GPUI_FXC_PATH`. The resource compiler for `windows-manifest` is likely
  present because Tauri Windows builds already need one, but verify it.
- The `tezbar` path dependency still runs `src-tauri/build.rs` on Windows. Its macOS-only
  embedding check is skipped, and the Tauri resources must exist in the staged source.
- Build from a SHA-256-verified `git archive` into a task-specific staging directory, as
  `docs/WINDOWS_SUPPORT.md` describes. Never touch `C:\Users\User\Desktop\Tezbar`.

### 5.7 Identity and signing

- macOS bundle `Tezbar Launcher Prototype.app`, identifier `com.tezbar.prototype.nativelauncher`,
  `LSUIElement = true`, created by `scripts/bundle-macos.sh` (not the Tauri bundler), staged in
  an OS-temp fixture directory and ad-hoc signed (`codesign --sign -`). That falls under the
  documented local-preview exception (`docs/MACOS_SIGNING.md`): not publishable, never
  `com.tezbar.app`, never installed to `/Applications`, never re-signing an existing app.
  The `src-tauri/build.rs` guard is untouched; the prototype never runs Tauri's bundler.
- No Keychain, Vault, TCC or privacy changes. If macOS shows any privacy prompt for the new
  identity (for example pasteboard access by the clipboard observer, which both builds run),
  do not approve it on the user's behalf. Record it.
- Windows: a plain executable in the task staging directory. No installer, registry
  entries, startup task or file associations.
- Never stop, replace, re-sign or read the profiles of the installed `/Applications/Tezbar.app`
  (beta.7). Do not repurpose `HOME` or `CODEX_HOME`. No production hotkey.

### 5.8 Blockers and risks

| Item | Status |
| --- | --- |
| Metal compiler missing | **Not a blocker**: `runtime_shaders` (Verified). Labeled deviation. |
| GPUI git build at the pin compiles with the `tezbar` path dependency on Rust 1.98.1 | **Open**: first milestone. |
| Windows Rust 1.98.1, fxc and rc | **Open**: needs host checks and approval for a toolchain install. |
| Windows Tauri baseline: `desktop-bench` is Unix-socket only | **Blocker for a matched Windows comparison** until a named-pipe control path exists for the Tauri build. Without it, Windows reports prototype-only absolute numbers, labeled as not comparable. |
| Windows visual runs over Tailscale SSH | **Limitation**: SSH runs outside the interactive desktop (§7.2). |
| Prototype links WebKit through `tauri` | Known bias against the prototype; quantified via `vmmap` (§4.4). |
| Real app icons are fetched via the core but decoded and cached by GPUI | Different decoder and cache than the WebView. Reported with the memory results. |

---

## 6. Measurement plan

Rule: a GPUI-only window compared with the full Tauri app total does **not** prove a causal
gain. Comparisons are valid only between builds of the same source SHA, on the same fixture,
with the same core services, measured by the same harness. Anything else is labeled separately.

### 6.1 Configurations

| Label | What runs | Use |
| --- | --- | --- |
| **A: Tauri launcher** | `tezbar` release with `--features desktop-bench`, same source SHA as B, existing isolated runtime identity `com.tezbar.performance` (`desktop_bench.rs`), staged and ad-hoc sealed as in the 64f6727 report. | Baseline |
| **B: GPUI launcher** | Prototype release (`runtime_shaders`) with `--features bench-control`. | Candidate |
| **C: Core-only floor** | Prototype binary with `--headless`: same facade, same Tokio runtime, core started with desktop services, **no GPUI application created**. | Shared floor. B − C ≈ GPUI UI cost and A − C ≈ Tauri host + WebView cost, both labeled *estimates*. |
| Reference only | 64f6727 result: 121.925 MiB median total; host 33.610, WebContent 62.157, GPU 16.032, networking 7.126, audio 3.000; CPU 0.367% of one core. | Context only. Different SHA, empty profile, not matched. Never used as A. |

### 6.2 Service graph (must be listed in every report)

| Service | A | B | C |
| --- | --- | --- | --- |
| `NativeCore::start` (launcher, AI no-op start, extensions, background Knowledge start) | yes | yes | yes |
| Initial app/file scan at +5 s, file watcher, 10 s refresh loop | yes | yes (desktop-services flag) | yes |
| Clipboard observer | yes | yes | yes |
| Idle trim 5 s after hide | yes | yes (§3.6) | n/a (never shown) |
| Global shortcut registration (`Control+Alt+Shift+F19`) | yes | **no** | no |
| Tray icon | yes | **no** | no |
| Updater startup check (+3 s, one-shot) | yes | **no** | no |
| WebView (WebContent, GPU, Networking) | yes | no | no |
| GPUI renderer (Metal / DirectX) | no | yes | no |
| Linked WebKit framework, no WebView | n/a | yes | yes |

Every "no" in a row the other build has is a service difference. Report it next to the
numbers. Do not hide it in a footnote. If a difference turns out to be material (for
example, the updater check leaves a resident HTTP client), add a variant that equalizes it
before drawing conclusions.

### 6.3 Fixture

- One generator script makes a marked OS-temp fixture (`.tezbar-native-launcher-prototype`
  and `.tezbar-desktop-benchmark` markers). It is copied byte-for-byte for every run, so A, B
  and C each get a fresh identical copy. No user data, credentials, extensions, models, vault
  items or AI connections. `hasCompletedOnboarding: true`.
- Synthetic home: a deterministic seeded tree (fixed seed recorded) of a fixed number of files
  (proposed: 20,000 across Desktop/Documents/Downloads/Pictures, realistic names and
  extensions), 50 notes, 50 snippets, 20 quick links in the stores' existing file formats.
  Applications come from the real read-only `/Applications` and system app directories in all
  builds; record the enumerated count.
- `resource_dir` for B and C points at the read-only `Contents/Resources` of the matched
  A build, so Knowledge and extension startup see the same resources.
- Readiness for measurement: core ready, launcher status `ready` (initial scan finished),
  indexed document count equal across builds (±0). Then stabilization as below.

### 6.4 Memory and CPU protocol (macOS)

- Reuse the accounting in `docs/performance/desktop.py`. Each sample sums
  `proc_pid_rusage(RUSAGE_INFO_V4)` physical footprint over the host, its recursive
  descendants and every process whose `responsibility_get_pid_responsible_for_pid` equals the
  host. That catches XPC services with PPID 1: WebKit processes for A, a Metal compiler service
  for B if present. RSS is secondary. CPU is interval user+system time across the same set.
- Conditions, each measured separately:
  1. **Hidden idle**: show once, hide, then a 120 s warmup and the stabilization rule (last 30
     one-second intervals: mean CPU < 2% and p95 < 5% of one core, extended until met),
     then 60 s of samples at about 1 Hz.
  2. **Visible idle**: show with the fixed query `doc` committed and results rendered, 120 s
     warmup, 60 s of samples.
  3. **Retained after work**: run the §6.5 latency script, hide, then 120 s warmup and 60 s
     of samples.
- After each timed interval, run `footprint -p` and `vmmap -summary` for every owned PID,
  including the WebKit image size in B and C (§4.4).
- Order: A B B A A B per condition (three runs each). C runs once per session, before and
  after, to detect drift. Record host state: swap used, other heavy processes, power source.
  Installed beta.7 keeps running throughout; record that.

### 6.5 Latency definitions

Every metric names its start and end; both builds use the same definitions.

| Metric | Start | End |
| --- | --- | --- |
| Open (app-observed) | Harness monotonic time just before writing `{"op":"show"}` to the control channel. | App reports the first frame rendered after show with the input focused. GPUI: `on_next_frame` after the render that made the window visible. Tauri: the existing double-rAF receipt after the visibility event. |
| Open (OS-observed) | Same start. | First CoreGraphics poll (≤2 ms interval, `window_visibility.py`) that shows the PID owning an on-screen window with the expected bounds. |
| Input echo | Synthetic text applied to the input model. B: control op `type` inserts through the same `replace_text_in_range` path as keyboard input. A: the existing responsiveness harness's JS `input` dispatch. | First rendered frame that contains the new text. |
| Input → results | Same start. | First rendered frame whose list shows the `search:all` response **for that exact generation**. |
| Core search time | B: facade call to `search:all` begins. A: the existing JS wrapper around `window.tezbar.searchAll` begins (includes IPC; labeled as such). | The corresponding response is received. Separates core/IPC time from UI time. Adding host-side timing to A would be new instrumentation; if added, it goes behind `desktop-bench` only. |
| Activation | Enter handled. | `search:execute` response received (window hide is excluded). Only `open-file` on a fixture `.txt` and `add-note`. |

- "Rendered frame" means content committed plus the following frame callback in both stacks.
  It is not photon time. Both harness injections are app-internal synthetic input, not OS-level
  keystrokes. OS-level injection (`CGEventPost`) needs a PostEvent/Accessibility grant for the
  harness. Use it only if the user has already granted it; never grant it here.
- Script: a fixed list of queries derived from the fixture (one-character, three-character,
  exact app name, file prefix, a multi-word note phrase, a no-match string), typed character by
  character at 70 ms, and separately pasted as one burst. Ten repetitions, the first discarded
  as warmup. Report p50, p95 and max per metric per build, plus the number of stale responses
  dropped.
- First search after show is reported separately from subsequent searches.

### 6.6 Reporting and claim rules

- Report every run, not only medians: per-process and per-role footprint (min/median/max),
  RSS, CPU, latencies and the service graph table.
- "Lower memory" may be claimed only if every B run is below every A run for that condition
  and the service differences are listed. Overlapping ranges are reported as **inconclusive**.
- Do not state ratios like "5×" unless the measured medians support exactly that ratio, and
  the ranges and service differences sit next to it. No extrapolation to populated profiles,
  production signing, precompiled shaders or Windows from macOS numbers.
- B − C and A − C are estimates of UI-stack cost, not proof of what a full migration would save.

### 6.7 Windows metrics (when the host allows)

- Process tree from Toolhelp parent links (WebView2 browser, renderer and GPU processes
  descend from the host). No command-line capture.
- Primary metric: summed `PrivateUsage` (commit). Secondary: `WorkingSetSize` and, where
  available, `PrivateWorkingSetSize`. Windows numbers are never compared with macOS numbers.
- Control channel: a named pipe for B (`bench-control`). A matched A needs the same for the
  Tauri build (§5.8 blocker); until then, Windows reports B and C only.

---

## 7. Acceptance checklists

### 7.1 macOS (Apple Silicon, this Mac)

Build:

- [ ] `src-tauri/Cargo.lock` unchanged. `cargo test --locked --lib` passes in `src-tauri`.
  `cargo +1.85.0 check --locked --lib` passes with and without `native-launcher-facade`.
- [ ] Facade tests pass (`--features native-launcher-facade`).
- [ ] The prototype builds in release with the pinned rev, `runtime_shaders` and `font-kit`,
  without Xcode and without `xcrun metal`. Build log saved.
- [ ] Lock-drift report generated (§5.5).
- [ ] Bundle assembled, ad-hoc signed, `codesign --verify --strict` passes; identifier
  `com.tezbar.prototype.nativelauncher`.
- [ ] `pnpm typecheck` and `pnpm test` still pass (AGENTS.md quality gate).

Run (isolated fixture; installed beta.7 untouched; its PIDs and hashes checked before and after):

- [ ] Starts, reaches core ready, scans the fixture; no Dock icon; no global hotkey registered.
- [ ] Typing Latin, Cyrillic (Russian and Kazakh layouts), a dead key (⌥e e → é), emoji via
  ⌃⌘Space, and Japanese Kana/Pinyin IME composition all work. Enter and Esc during
  composition behave as in §3.1.
- [ ] Select-all, word movement, mouse selection, copy and cut within the field, paste into
  the field from text the tester places themselves.
- [ ] Up/Down/Enter/Esc per §3.1. Pointer movement selects; scrolling does not.
- [ ] Stale suppression: fast typing with an artificially slowed core (debug delay option in
  the facade test build only) never shows results for older text; unit tests for
  `search_session.rs` cover out-of-order responses and Enter during a pending generation.
- [ ] Exactly one `search:execute` and one usage record per activation, verified via the
  fixture's `click_events` table; held Enter does not repeat.
- [ ] `open-file` (fixture `.txt`) and `open-app` (Calculator) work. Ask the user before running
  them, because they open real apps on their desktop. Unsupported rows show
  `Not in prototype` and do nothing.
- [ ] `copy-text` is **not** exercised live unless the user explicitly approves touching the
  system clipboard.
- [ ] Light and dark screenshots of every §2.4 state. Capture with `screencapture -l<windowid>`
  only if the shell already has Screen Recording permission; otherwise the user captures them.
  Never grant TCC here.
- [ ] Quit via the control channel: zero owned survivors; fixture clipboard stores scrubbed
  with the existing `scrub_fixture_clipboard` without reading them.

### 7.2 Windows (x64 host via Tailscale)

- [ ] Host checks recorded: Rust toolchains, `fxc.exe`, resource compiler, whether an
  interactive user session is logged in and unlocked.
- [ ] With approval, Rust `1.98.1` installed user-level; else stop and report.
- [ ] Debug, then release prototype build from a verified `git archive` in task staging.
- [ ] Facade tests and `cargo check --locked --lib` for `src-tauri` pass on Rust 1.90 and 1.98.1.
- [ ] `--headless` (C) runs over SSH: core ready, fixture scanned, clean shutdown.
- [ ] **SSH limitation**: Tailscale SSH sessions are non-interactive and have no visible
  desktop. GPUI windows, DirectX presentation, IME and focus cannot be validated there, and
  results from such a session are not visual QA. Visible runs need either the user at the
  console, or a one-off scheduled task in the logged-in user's interactive session, created
  with explicit approval and removed afterwards.
- [ ] In an interactive session: the same functional list as macOS, with Windows bindings,
  Microsoft Pinyin or Japanese IME, Russian layout, light/dark, screenshots taken by the user
  or by an approved in-session method.
- [ ] Memory and CPU per §6.7; labeled "prototype-only" while the Tauri named-pipe control
  path is missing.

---

## 8. Review gates

1. **Build gate**: §7.1 build items green; evidence directory with logs, hashes and the lock
   drift report.
2. **Native visual QA**: screenshots of every §2.4 state, light and dark, macOS (and Windows
   when available), placed beside screenshots of the current WebView launcher on the same
   query and fixture.
3. **Opus design review** (Claude Opus 5.5, critical): check against §2, including §2.7 and
   §2.8, and against the current launcher screenshots. Type sizes, spacing, row rhythm,
   selection treatment, light-theme contrast, IME rendering, copy. Any prohibited pattern is a
   rejection. Fixes go through a new round; "close enough" is not acceptance.
4. **Measurement gate**: §6 executed. The report follows §6.6, and service differences and
   deviations (runtime shaders, linked WebKit, Windows status) are listed first.
5. **Decision memo**: whether broader UI migration is worthwhile, citing only measured values.

---

## 9. Implementation order

| Milestone | Content | Exit |
| --- | --- | --- |
| M0 | Prototype package with only GPUI: a 760 × 640 frameless window with static header text, built with the pin, `runtime_shaders`, `font-kit`. macOS release; Windows debug if available. | Compiles and shows text. Exact errors recorded otherwise. Stop and report if the pin cannot build. |
| M1 | Facade (§4.2–4.3) plus headless tests; `--headless` mode (C). | Facade tests green; 1.85 check green; Cargo.lock unchanged. |
| M2 | Text field (IME, selection, clipboard), session logic, virtualized list, footer, states, theme. | §7.1 run items green on macOS. |
| M3 | Icons via `asset-icon:data-url`, idle trim, `bench-control`, bundle script. | Visual QA screenshots; Opus review round 1. |
| M4 | Fixture generator, lock-drift check, measurement runs on macOS. | §6 report. |
| M5 | Windows build and interactive-session validation as allowed. | §7.2 items, or recorded blockers. |

Commit on `feat/native-launcher-prototype` only. No push, PR, release or GitHub Actions.

---

## 10. Known limitations

- Not a production launcher: no pins, modes, extensions, vault, calculator, hotkey,
  resizing, scaling, per-monitor placement or accessibility parity beyond what GPUI's
  built-in AccessKit support exposes. VoiceOver/Narrator behavior is recorded, not guaranteed.
- Activation covers only `open-app`, `open-file`, `open-url`, `open-with-app`, `add-note` and
  (code only) `copy-text`.
- macOS numbers come from a runtime-shader, ad-hoc-signed, instrumented local build on an
  empty-but-seeded synthetic profile with beta.7 running concurrently. Windows numbers, if any,
  are prototype-only until the Tauri named-pipe control exists.
- WindowServer/DWM compositing cost sits outside both apps' process accounting and is not
  measured.
- The prototype links Tauri/WebKit through the facade, which inflates its footprint.
  Removing that requires the `AppHandle` decoupling deferred in §4.3.
