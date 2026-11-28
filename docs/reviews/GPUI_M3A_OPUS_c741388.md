# Opus 5.5 critical review: M3 Lane A launcher/utility models at `c741388`

Scope: the worktree `Tezbar-native-rust-agent`, branch `feat/native-gpui-app`, pin `c741388`
(implementation `97e6cab`, HTML repair `cc7823f`). The review covers only the pure Rust models
listed in the brief. It is not a verdict on the full app, on GPUI views, on M4 (`apps/tezbar`,
`tezbar-ui`) or on the M3B agent/settings controllers.

Method: I traced the source of every scoped Rust file against the original TS (`CommandBar.tsx`,
`CommandBar.lifecycle.test.ts`, `commandBarInputMode.ts`, `searchMode.ts`, `calculator.ts`,
`colorConverter.ts`, `textTransform{,ers}.ts`, `qrImage.ts`, `emojiSearch.ts`,
`currency/*`, `NotesView.tsx`, `ClipboardView.tsx`, `SnippetsView.tsx`, `pinnedCommands.ts`,
`theme.ts`, `terminalPreferences.ts`, `updater.ts`, `releaseNotes.ts`, `newItemShortcut.ts`, `App.tsx`).
I checked JS semantics with Node. I ran two public-API probes from the scratchpad, compiled with
toolchain 1.98.1 against the existing `target/debug` rlibs: `libtezbar_core-d110ace71e6ca6d8`
(built after `97e6cab`; core is unchanged since then) and `libtezbar_ui_model-a13c436ded423225`
(launcher-models with html5ever). The pinned scope matched HEAD (`git diff c741388` was empty for
the three crates). I made no source edits and did no workspace build, reindex, desktop, network,
profile or clipboard access.

Timing caveat: both rlibs are **debug** builds, so absolute times are pessimistic. The
complexity class is unaffected.

## Verdict

**Not accepted yet.** The pure instant-answer functions are close to the TS behaviour. The
launcher controller has one structural gap (AI/terminal submit) and several event-flow
divergences from the CommandBar lifecycle it claims to replace. The notes and JSON paths have
confirmed data and performance defects. Repair the High/Medium items, then get a fresh review.

## High

### H1. The launcher controller cannot express AI-mode or terminal submit; AI Enter resumes an old chat (Confirmed, old≠new)
- Path: `launcher/controller.rs` `Input::Enter` → `session.enter` → `activation_effects`. There
  is no `OpenAiChat` / `ResolveDirectory` / `OpenTerminal` effect. `launcher.rs::ai_boot` has no
  caller or test.
- TS (`CommandBar.tsx` ~3725–3790): in AI mode, Enter resolves the `/dir`. It resumes a chat
  only when `aiChatSelectionActiveRef` is set (the user navigated into the list). Otherwise it
  submits the prompt (`aiChatBootForInput`) or opens a new chat. Terminal Enter opens the selected
  session or a new terminal with the command.
- Probe: input `" summarize my day"` with one chat row, then Enter → `Execute{row_id:"command:resume"…}`.
  The prompt is never submitted. Terminal mode fails its generation, so Enter does nothing.
- Fix: add explicit `Submit` effects for AI (`AiBoot`, including directory resolution and
  failure) and terminal (`OpenTerminal{command|session}`). Add a "list selection active" state
  (TS uses `selectedSearch = -1`), and drive `ai_boot` from the controller with tests.

### H2. `transform::json_format` / clipboard JSON action is quadratic in object keys (Confirmed)
- Path: `instant/transform.rs` `visit_map`: duplicate-key detection does
  `v.iter_mut().find(|e| e.0 == k)` for every entry. The function also parses up to 3 times
  (`apply_clipboard_action` → `Value`, then `json_format` → `Value` and the ordered `Json`).
- Probe timings (debug lib): 5k keys/49 KB 179 ms; 20k/209 KB 2.5 s; 40k/429 KB **8.0 s**. The
  1 MiB input bound allows about 100k keys. TS `JSON.parse` takes milliseconds.
- Fix: keep a `HashMap<String, usize>` index for duplicates (last value wins, first position
  kept), parse once, and run the transform off the UI thread in the adapter.

## Medium

### M1. Invalidation ignores the event's `text`, skips no Deep queries, wipes every cache and runs while hidden (Confirmed, old≠new)
- TS (`CommandBar.tsx` 2328–2338): it refreshes only when `event.text` is absent or equals the
  current request input (or the argument value). It never refreshes Deep queries and keeps the
  candidate cache. Lifecycle test "refreshes the latest query on invalidation" asserts that the
  `unrelated` event causes no request. Core emits `{"text","queryId"}` from
  `extensions/process.rs:486`.
- Rust: `Input::Invalidate` has no payload. It calls `cache.invalidate()` (home, candidates and
  query LRU), so `display_rows()` shows an empty preview until the refresh returns. It also
  commits a search while `visible == false` (probe: hidden + Invalidate → `Search{query:""}`), and
  `Show` then commits a second one. `BackendChanged` behaves the same way while hidden.
- Fix: `Invalidate { text: Option<String> }` with the TS filter and the Deep exclusion; keep the
  caches, or invalidate only the matching exact-query entry. While hidden, mark the session dirty
  and let `Show` do the one authoritative refresh.

### M2. Activation ownership is never released on backend restart or a lost completion (Confirmed behaviour, impact depends on the executor contract)
- `Launcher.activation` and `session.activating` are cleared only by a matching
  `ActivationComplete`. `BackendChanged` resets only the search flight.
- Probe: Enter (Execute) → `BackendChanged` → refresh → Enter returns `[]` with
  `activating=true`. Every later Enter or click is ignored until a completion arrives that may
  never come.
- Fix: retire the in-flight activation on `BackendChanged` (synthesise an error), or add an
  activation deadline. At minimum, document that the executor must always complete a ticket,
  including after transport loss.

### M3. Successful activation hides without clearing input or mode (Confirmed; impact depends on whether the host echoes `Input::Hide`)
- `ActivationComplete Ok` calls `session.hide()` and sets `visible=false`. It leaves
  `self.input`, `composing` and `composition_draft` set. Probe: after activating in `/Users`,
  `Show` → `CompletePath{query:""}` instead of `Search{""}`, and the stale text would be shown.
- Fix: route all hides through one helper (the `Input::Hide` body).

### M4. Single-character queries hit the backend (Confirmed, old≠new)
- TS skips `searchAll` for non-deep queries of length 1 (lifecycle test "keeps single-character
  local matching…"). Rust emits `Search{query:"a"}`.
- Fix: no request for length 1 (non-deep); show the optimistic preview and mark it displayed.
  Note that the stale-click rule then needs a defined activation policy for that preview.

### M5. Calculator precedence and syntax diverge (Confirmed against Node)
- `-2^2` → Rust `4`. JS rejects it (SyntaxError → null); the mathematical value is −4. Unary
  minus binds tighter than `^` because `atom()` handles unary operators.
- `2**3` → Rust `None`; TS `8` (deliberately rejected at `expr()`).
- `°` binds to the atom: `2°^2` → (2π/180)², while TS gives 2·(π/180)².
- `max(1, sqrt(-1))` → Rust `1` (f64::max drops NaN); TS null.
- `round(0.49999999999999994)` → `1`; JS `0`.
- Rust behaviour that is arguably better but undocumented: `010+1` → 11 (TS sloppy octal 9), and
  the comma operator is rejected.
- Fix: parse unary operators below `^` (or reject `-a^b` the way JS does), accept `**` as `^`,
  propagate NaN in min/max, and use a JS-exact round. Add these as tests.

### M6. Notes: legacy markup outside the old regex becomes visible text and is saved escaped (Confirmed)
- Old TS also used a regex sanitizer, but it put the result into `innerHTML`, so comments, `<o:p>`
  and similar unmatched markup stayed invisible DOM nodes. Rust `Document::load` treats them as
  text.
- Probe: `<div><!--StartFragment-->Hello<o:p></o:p></div>` → html
  `<div>&lt;!--StartFragment--&gt;Hello&lt;o:p&gt;&lt;/o:p&gt;</div>`, plain
  `<!--StartFragment-->Hello<o:p></o:p>`. `<div title="a>b">x` → visible `b">x`. This is the same
  class as the parent's `<br/>` and entity findings. The first edit persists the escaped text.
- Fix: in `sanitize_html`, drop comments, `<!…>`, `<?…?>` and namespaced or unknown tags
  (`<[a-z][\w:.-]*…>`), and handle quoted attribute values. Add jsdom-backed fixtures (Office or
  WebKit paste HTML).

### M7. Regexes compiled per call on hot paths (Confirmed)
- Example: `notes::plain_text` and `strip_markdown` compile 13 regexes per note, and `visible()`
  calls them for every note on each query. Probe (debug): 500 notes in **10.9 s**.
- Also: `color::float` (per channel, per keystroke), `parse_currency_query`/`extract` (3 per
  keystroke), `normalize_stored`, `sanitize_html`, `Document::load`, `count_text`,
  `split_words`, `parse_timestamp` and `apply_clipboard_action`.
- Fix: `LazyLock<Regex>` statics (already used in calculator and emoji). Cache plain text per
  note revision.

### M8. Router command mapping is wrong for Settings and loses update-state dispatch (Confirmed / Plausible)
- `router.rs::invoke("open-settings")` → `Settings("ai")`. TS `App.tsx:803` opens `general`;
  only `open-providers` opens `ai`. Probe confirmed.
- TS treats `check-for-updates`, `download-app-update` and `restart-app-update` as one
  status-dependent action: ready → restart, available → download, manualAvailable → open the
  installer page, downloading → no-op. Rust emits three literal effects, and manualAvailable has
  no effect. This is plausible only if the adapter re-dispatches by status; the interface
  invites the wrong mapping.
- Fix: map `open-settings` to general, and use one `UpdateAction` effect resolved against the
  update policy state.

### M9. Part of the case map is satisfied by test-harness re-implementations (Confirmed)
- `launcher/ts_cases.rs::run` re-implements `aiChatBootForInput`, `parseSearchQuery`,
  `deepSearchInput`, `deepSearchDraftInput`, `addLauncherQueryHistoryEntry` and
  `parseLauncherQueryHistory` inline, and never calls `ai_boot` or the `ui_state` parser. For
  `commandBarInputMode` it derives `parsedSearchQuery`, `isDeepSearchMode`, `slash`, `app` and
  `isCompletionInput` in the harness.
- Affected: 3 cases are fully harness-only (`launcherQueryHistory`: "keeps only the latest
  query", "safely parses persisted history"; `searchMode`: "treats a leading bang…"). 4 are
  partial (input-mode cases 0, 1, 4, 5). The "170 cases → 169 functions" mapping therefore
  overstates production coverage.
- Fix: route these calls to production functions (`ai_boot`, `request_input`/`input_mode`,
  `UiState::import_key`), or mark them `harness-only` in the map.

### M10. Locale seams are hidden inside English strings (Confirmed)
- `format_relative`: ±1 week/month/year → "1 week ago" / "in 1 month". Intl `numeric:'auto'`
  gives "last week" / "next month" (Node confirmed). Locale is fixed to English.
- `parse_timestamp` returns a finished string whose `Local:` line is UTC Display, and there is no
  structured instant for an adapter to localise. That contradicts "native localized presentation
  can use the parsed instant".
- Clipboard Count prints `1234` where TS `toLocaleString` prints `1,234`.
- Fix: return a struct (`ms`, ISO, instant, relative unit/value) and format through an injected
  formatter, as `AmountFormatter` already does for currency.

### M11. JSON number formatting differs from both the source text and JS (Confirmed)
- `{"a":10.0,"b":1e2,"e":-0.0,"d":12345678901234567890}` → Rust `10.0`, `100.0`, `-0.0`, the exact
  big integer. JS gives `10`, `100`, `0`, `12345678901234567000`. A UTF-8 BOM input is rejected
  ("Invalid JSON"), while TS `trim()` accepts it. JSON nested deeper than serde's 128 levels is
  silently string-encoded by the clipboard action.
- Fix: serialise f64 through `instant::number`, trim like JS, and decide or document the
  big-integer policy.

## Low

- **Clipboard previews.** TS lazily loads previews for `file` entries too
  (`ClipboardView.tsx:247`) and caches null results. Rust loads only `Image` entries and re-errors
  on every reselect. Images over 8 MiB get no preview (a deliberate bound; suggest an
  adapter-side thumbnail).
- **Clipboard selection and filters.** Selection is index-based, as in TS (parity). `query` and
  `filter` are public fields, so changing them bypasses the selection clamp.
- **Currency fallback.** `first()` lacks TS `firstCurrencyInText`'s compact-string fallback and
  the `firstCurrencyInText(left)` fallback. Rust adds a `฿` prefix/suffix and applies the `%`
  rejection to the whole input. Tokenisation keeps non-Latin/Cyrillic letters (for example
  Kazakh `ң`), which TS replaces with spaces.
- **`ui_state` decoding.** `UiState` has no `#[serde(default)]`, so a valid native file missing a
  later-added field becomes a hard "corrupt" error. Pin import is stricter than TS (`subtitle:
  null` or a numeric id drops the pin). Recent emojis and recent extension commands are not
  trimmed as in TS. On Windows, `rename` has no retry on a transient sharing violation, and mode
  0600 is Unix-only (ACL inheritance is assumed).
- **Snippets.** `Write` completion carries no created id, so a create that is saved again during
  a later revision can create twice. TS has the same race, and core's trigger uniqueness
  mitigates it. Validation is duplicated with core: ASCII case-folding here, Unicode
  `to_lowercase` in core.
- **Notes.** Copying an empty note emits `Copy("")`; TS shows "Note is empty". Every edit
  serialises the whole document 2–3 times, and `tick()` serialises every open draft (up to 32)
  twice per tick.
- **Notes list.** The filter uses plain text instead of TS raw HTML. This is an improvement, but
  undocumented.
- **Session.** `deferred_enter` uses `now > expires` while `expire_enter` uses `>=`
  (off-by-one at exactly 1000 ms; outcome depends on order).
- **Emoji.** Exact final-score ties sort by index, where TS keeps Fuse order (rare).
- **QR.** Error text differs from TS. The mask choice comes from the `qrcode` crate rather than
  the TS penalty loop; it is standard-valid and the decoder tests cover it.
- **base64.** The strict STANDARD engine rejects non-canonical trailing bits (`QR==`), which
  `atob` accepts.
- **Router.** Opening a specific note by row id (`note:<createdAt>`, category `quick-notes`) is
  not modelled. The adapter could use `Execute.row_id`.
- **Calculator bounds.** Correct; no eval, shell or subprocess.

## Verified as matching or sound
- **Session.** Fixed 8 ms window, one flight plus one replaceable pending request, stale
  generations dropped, held Enter suppressed, 1 s deferred Enter fires once, stale clicks
  rejected, all six prototype cases unchanged.
- **IME.** The draft is held until composition ends, and Hide recovers without `compositionend`.
- **Input mode.** Priority and the AI sentinel logic match `commandBarInputMode.ts`.
  `request_input` matches `searchRequestInput`. `history_entry`, `should_recall` and
  `escape_result` match `CommandBar.tsx` 3500–3570 exactly.
- **Ranking.** `good_metadata` and `deep_recommendation` match `searchMode.ts` (UTF-16 vs code
  point length only matters for astral input). Optimistic ranking reuses core
  `launcher::text`; there is no second ranking fork.
- **Color.** Matches TS, including the hex-first reorder. The only divergences are non-finite
  `parseFloat` quirks and the per-call regex.
- **Text transforms.** MD5, SHA, base64, URL, case and word count match apart from the
  sentence-regex lookahead nuance and `\s` set differences.
- **Fuse and emoji.** The Fuse Bitap adaptation, field norm, key weights and score
  multiplication match Fuse with `ignoreLocation`. Emoji preprocessing and boosts match.
- **QR.** Uses the existing `qrcode` crate (MIT/Apache), with a 213-byte cap at level M,
  versions 1–10.
- **Release notes.** `tezbar-update::notes` matches `releaseNotes.ts`: bundled notes, no remote
  substitution, tag check, and stale/cancel handling.
- **html5ever.** Decoding runs in the RCDATA state only; no DOM, script or I/O. It is optional
  under `launcher-models`.
- **Atomic writer.** Same-directory `create_new` 0600 temp file, `fsync`, `rename`, directory
  `fsync`, and cleanup on error.
- **Constructors.** No eager I/O, process, model or HTTP work in any constructor.
- **Quicklinks.** Legacy schema only; no new CRUD.

## Limitations
I did no GPUI, Windows host, real WebKit/WebView2, actual clipboard or live network work. Probe
timings come from debug rlibs. I did not re-run the TS suites or full Rust suites; I relied on the
parent's saved logs for pass counts. The notes `enter`/`toggle_style` list algorithms were only
sampled.

## Probe files
`scratchpad/probe/core_probe.rs`, `scratchpad/probe/ui_probe.rs` (compiled with
`rustup run 1.98.1 rustc --extern … -L target/debug/deps`).
