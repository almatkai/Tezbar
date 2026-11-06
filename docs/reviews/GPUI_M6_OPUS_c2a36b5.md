# Tezbar GPUI M6 utilities + M5 R-1..R-3 closure — independent Opus review

- **Reviewer:** Claude Opus 5.5 (independent review sub-agent; no subagents spawned)
- **Pin:** `c2a36b52fffc191ff09a3b461013bdab6196fefa`, tree `26aca058df8a46df69feb2dc2cff3cd36674615f`, branch `feat/native-gpui-app`
- **Base for the diff:** `35dca9b` → pin (`7e21f21` M6 utilities + `c2a36b5` root lifecycle/route repairs). The canonical Notes selection and bounded release HTTP from `30f7084` (an ancestor of `35dca9b`) were read in their pinned form.
- **Scope:** A = full M6 correctness, spec and design; B = targeted fresh closure of M5 R-1, R-2 and R-3. This review does **not** accept M7–M11, the full migration or any release.

## Verdict

**M6 is not acceptable at this pin.** There are **10 actionable functional findings** (1 High, 6 Medium, 3 Low) plus standards and design items. The High finding is reproducible with real keystrokes: the native rich Notes editor writes typed text into the wrong paragraph. This happens when typing into an empty note, after Enter, and after list continuation. The bad HTML is then autosaved.

**M5 R-1, R-2 and R-3 are closed.** None of the original F-01–F-08 cases regressed. There is one M7 hand-off note on R-3 (the Settings tab is not carried).

All actionable findings need parent repairs and fresh regressions before M6 can be accepted.

---

## 1. Method and integrity

- I made an OS-temp snapshot of the pin with `git archive` at `/tmp/tezbar-gpui-m6-opus-c2a36b5-snap/src`. All **1890/1890 blobs** match `git ls-tree -r` (`git hash-object --no-filters`; symlinks hashed by their target), with 0 mismatches.
- Probes ran in a separate copy, `/tmp/tezbar-gpui-m6-opus-c2a36b5-probe/src`. The only change there is 171 appended test lines in `utilities/tests.rs`, saved as `evidence/probe-tests.rs`. The pristine snapshot was not edited.
- The selected live tree was only read (`git show`, `git diff`, `git status`). Nothing was edited, staged, committed, reset, switched, pushed, installed or launched. The live tree has since moved to `6d2f7df` with concurrent M7 work; no live file was used as evidence.
- **XERJ was not used.** It is known to be down and was not needed: exact `rg`, Git and pinned source were authoritative. Nothing was reindexed and no alternate server was started.
- **Build settings:** my own target (an APFS clone of my earlier M5-review target, in this session's scratchpad), Rust `+1.98.1`, `--locked --offline`, `-j2`, `--test-threads=2`, `CARGO_INCREMENTAL=0`, `--features runtime-shaders`, `TEZBAR_NATIVE_SOURCE_SHA=c2a36b5…`. `Cargo.lock` SHA `06e2cbf1…b592` was unchanged before and after, in both copies.
- **No external access:** no live network, provider, MCP, real profile, clipboard, Keychain/DPAPI, process spawn or desktop launch. All fixtures are synthetic.

Evidence is in `/tmp/tezbar-gpui-m6-opus-c2a36b5-evidence/`: `pristine-native.log`, `pristine-ui.log`, `probe1.log`, `probe2.log`, `probe3.log` and `probe-tests.rs`.

---

## 2. Spec / functional findings (actionable)

### F-1 — HIGH — Rich Notes projection sends typed text into the wrong paragraph

**Trigger.** Any of these, with real keystrokes:
- Type into an empty note. The canonical empty document is `<div><br></div>`, which is also what `Document::html()` produces after you delete everything.
- Press Enter at the end of a paragraph, then type.
- Press Enter to continue a list, then type.

**Call sites**
- `crates/tezbar-ui/src/rich_text.rs:148-157` (`Projection::from_document`): a placeholder `<br>` inside a block emits a boundary `\n` *and* a model `\n`.
- `rich_text.rs:196-212` (`position` / `model_at`): the caret just after the boundary maps to the *previous* block's end offset.
- `crates/tezbar-ui-model/src/notes.rs:360`: `Document::replace` inserts into the first text node that satisfies `start <= offset + count`, which is left-biased. If no text node matches, `notes.rs:392` appends a top-level text node.
- `notes.rs:1078`: `Document::enter` creates a `<div><br></div>` placeholder.

**Reproduction.** These are real GPUI keystrokes through `Utilities` → `RichText` → `TextInput` → `Notes::edit_with_selection` (`probe1.log`, re-run in `probe2.log` and `probe3.log`):

| Start HTML | Caret | Keys | Native result | Expected |
| --- | --- | --- | --- | --- |
| `<div><br></div>` | 0 | `a` | `<div><br></div>a` (visual `"\n\na"`, the char lands on line 3) | `<div>a</div>` |
| `<div>abc</div>` | 3 | `enter d` | `<div>abcd</div><div><br></div>` | `<div>abc</div><div>d</div>` |
| `<ul><li>one</li></ul>` | 3 | `enter x` | `<ul><li>onex</li><li><br></li></ul>` | `<ul><li>one</li><li>x</li></ul>` |
| `<div>a</div><div><br></div><div>b</div>` | — | — | visual `"a\n\n\nb"` (an extra blank line) | `"a\n\nb"` |

**Consequence**
- Basic note typing is broken.
- The wrong HTML is autosaved through `notes:update`, so it is persisted corruption.
- Stored contenteditable notes that contain `<div><br></div>` or trailing `<br>` render with extra blank lines, and the caret lands on the wrong line.
- None of the 28 M6 fixtures types after Enter or into an empty note. They assert only the HTML right after Enter (`tests.rs:489-492`, `1055`).

**Concrete fix**
1. In `Projection`, treat a block whose only content is a placeholder `<br>` (and a trailing `<br>` in a block) as an empty visual line. Emit the boundary only, with no extra `\n`.
2. Give each visual point its owning block path. An insertion at a block's start or inside an empty block then targets that block: replace its placeholder `<br>` with the text, or insert into that block's first text node, instead of the global left-biased walk. A path-aware `Document::replace_in_block`, or a right-bias flag on `replace` for block-start carets, both work.
3. Keep the existing join, list-exit and undo semantics.

**Required regressions:** the four rows above, plus Enter then Backspace, typing at the start of the second paragraph, and an IME commit into an empty block.

### F-2 — MEDIUM — Rich edit range is guessed from a text diff, so edits next to identical characters hit the wrong node

**Trigger.** A forward-delete, or a selection delete, of a character that is followed by an identical character with different style or structure. Examples:
- `<strong>a</strong>a` with the caret at 0, then Delete.
- A `<br>` newline next to a block-boundary newline.

**Call sites**
- `crates/tezbar-ui/src/text_input.rs`: `ExternalEdit::Replace { before, after, before_selection, after_selection }` does not carry the replaced range, although `replace_text_in_range` knows it.
- `rich_text.rs:486-500`: the range is rebuilt from the common prefix and suffix, which always places the edit at the right-most point in a run of identical characters.

**Reproduction.** `opus_probe_forward_delete_bold_char_before_identical_plain_char`: `<div><strong>a</strong>a</div>`, caret 0, `delete` gives `<div><strong>a</strong></div>`. The user deleted the bold character; the plain one was removed instead.

**Consequence**
- Style is corrupted: the remaining text keeps formatting the user deleted.
- A `br`-versus-paragraph structure can be joined wrongly while the visual text looks identical, so the saved HTML differs from the edit the user made.

**Concrete fix**
- Add the exact replaced byte range in `before` and the inserted text to `ExternalEdit::Replace`.
  - Ordinary edits: use `range` and `new_text` from `replace_text_in_range`.
  - IME commits: use the composition start from `before_composition_selection` / `marked_range`.
- In RichText, use that range instead of the prefix/suffix diff.
- Default plain and password inputs never set `external_edits`, so they are unaffected.

**Required regression:** this probe, plus the Backspace counterpart and a `br`/boundary Delete case.

### F-3 — MEDIUM — Saving a clipboard secret to Vault, then hiding, leaves the secret in clipboard history

**Trigger.**
1. In Clipboard, choose Save to Vault on a secret entry.
2. In Vault, Unlock, then Save.
3. The launcher hides (Esc or blur-hide) before `vault:save` replies.

In the core, `vault:save` also runs `launcher.local_refresh()` before it returns (`tezbar-core/src/lib.rs:272-281`), which widens that window.

**Call sites**
- `utilities/mod.rs:378`: hiding calls `vault.conceal()`, which increments `session` and calls `model.lock()`, clearing `model.save`.
- `vault.rs:130` drops every completion from an older session, including the **durable** Save.
- `vault.rs:225-235`: the source `clipboard:delete` is issued only from that dropped completion.

**Reproduction.** `opus_probe_vault_save_then_hide_still_deletes_clipboard_source`: `vault:save` is sent once, and `clipboard:delete` for the secret is sent **0** times (expected 1). Wire: `[…, "vault:unlock", "vault:list", "vault:save"]`.

**Consequence**
- The secret stays in plain clipboard history after it was saved to Vault.
- This contradicts `integration.md` ("deleted only after durable save success") and TS `VaultView.tsx:264-265`, which deletes in the same async continuation whatever happens to the view.

**Concrete fix**
- Capture `clipboard_entry_id` and the token in the Save completion when it is emitted.
- On a successful reply, always issue the durable `ClipboardDelete`, regardless of session. Fence only the UI presentation (form reset, list refresh) by session.

**Required regression:** the probe above.

### F-4 — MEDIUM — An unsaved snippet form with an error blocks quit, and cannot be cancelled after reopening

**Trigger.**
1. Snippets → New, then Save with a validation error (for example an empty title, which gives "Title is required", or a core "Another snippet already uses this trigger").
2. Hide the launcher or navigate away.
3. Quit from the tray or menu.

**Call sites**
- `utilities/mod.rs:698-702`: drain re-saves; a local validation error is skipped.
- `mod.rs:723-726`: the drain returns `Err` while `snippets.draft.is_some() && snippets.error.is_some()`.
- `mod.rs:329-330` and `378-380`: navigate and hide clear `fields` but keep `snippets.draft`. Nothing rebuilds the form on show or re-navigate.
- `utilities_integration.rs:274` labels the error `Notes: …`.
- `app.rs` quit failure path: cancels quit, opens the Settings window and shows the error.

**Reproduction.** `opus_probe_invalid_snippet_form_does_not_block_quit_drain`: `drain()` returns `Err("Title is required")`.

**Consequence**
- Normal quit is refused with a mislabelled "Notes: Title is required" message, and the Settings window opens.
- After reopening, the form is invisible. Esc emits Back, not snippet Cancel, so the only way out is the hidden New → Cancel sequence.
- TS never blocked quit on an unsaved snippet form.

**Concrete fix**
- Do not include unsubmitted or locally invalid snippet drafts in the durable drain. Either retain only core-admitted writes that failed, or keep a draft but never let it veto quit.
- If drafts are retained across hide, rebuild the form on show/navigate. Otherwise discard the draft on navigate.
- Label each utility error by its owner (Notes, Snippets, Vault).

**Required regressions:** an invalid draft, then hide, then quit succeeds; reopening shows the form or no draft.

### F-5 — MEDIUM — Native builds never bundle the current release notes, so the read marker never fires

**Trigger.** Open Release Notes for the current version in any normal build.

**Call sites**
- `command_bar/utilities_integration.rs:50` reads `option_env!("TEZBAR_APP_RELEASE_NOTES")`, but no build script or packaging script sets it. `rg` finds only this reader, and `apps/tezbar/build.rs` emits no such `rustc-env`.
- TS bundles `docs/releases/${version}.md` (`vite.config.ts:10-20`). `docs/releases/0.3.0-beta.7.md` exists at the pin.
- `tezbar-update/src/notes.rs` (`Notes::open`) → `State::Error("This build is missing its Tezbar release notes.")`.

**Consequence**
- The current-version notes always show an error.
- `ReleaseRead` is never emitted (`release_notes.rs:70-83`), so `ui_state.release_notes_version` never reaches the current version. The post-update "Release Notes" row (`command_bar/mod.rs:854`) is then never retired.

This is deterministic from source; it was not executed against a packaged binary.

**Concrete fix**
- In `apps/tezbar/build.rs`, read `TEZBAR_RELEASE_NOTES_PATH` or `docs/releases/${CARGO_PKG_VERSION}.md`.
- Emit `cargo:rustc-env=TEZBAR_APP_RELEASE_NOTES=…`, or generate an `include_str!` file in `OUT_DIR`, with `rerun-if-changed` / `rerun-if-env-changed`. Fail or warn loudly in release profiles when the file is missing.

**Required regression:** a build-script unit test, or a test asserting non-empty bundled notes for `CARGO_PKG_VERSION`.

### F-6 — MEDIUM — System Monitor shows raw JSON keys instead of formatted stats (Y-01)

**Call site:** `utilities/system_monitor.rs:20-64`. Rows are `"{section} · {key}: {raw value}"`, and nested objects and arrays are printed as JSON strings.

**Consequence**
- Users see output like `memory · totalBytes: 17179869184`.
- TS `SystemMonitorView.tsx:16-40` and `280-330` format bytes, rates and uptime, with meters and per-section summaries.
- Y-01 explicitly requires formatting tests; there are none.

**Concrete fix:** port the TS formatting helpers and section composition into the model (`tezbar-ui-model::utilities`), with unit tests against the TS cases. Render labelled rows or tiles, flat per §5.6.

### F-7 — MEDIUM — The background-task / update indicator is on the wrong surface (U-31)

**Call sites**
- In the native app the indicator is only in the Utilities sheet (`utilities/view.rs:936-958`).
- TS renders it in the **command bar footer** (`CommandBar.tsx:5700` → `ui/CommandSurface.tsx:245`) and in AI chat. TS utility views do not show it.
- The native launcher footer (`command_bar/view.rs:1575-1613`) has no indicator.

**Consequence**
- From the home command bar, users cannot see indexing, update-download or extension task status.
- The "Update ready → Restart" and "indexing → open" actions are only reachable by first opening a utility.
- Every visible utility polls `background-tasks:list` once a second (`mod.rs:457-459`), which TS utility views never did.

**Concrete fix:** move the indicator and its actions into the Launcher footer, reusing the same model, events and actions (the current `set_update_status` plus the update-status event). Keep it out of utility sheets, unless the ledger records the change as a deliberate deviation.

### F-8 — LOW — A new Release Notes navigation reopens the last historical version

**Call site:** `release_notes.rs:28` (`version.or_else(|| self.release.version.clone())`). `navigate` never resets `release.version`.

**Reproduction.** `opus_probe_new_release_navigation_opens_current_not_last_historical`: open the notes, Enter `0.2.0`, go to Clipboard, open Release Notes again. It shows `0.2.0`, but TS always opens the current version.

**Concrete fix:** clear `release.version` when a navigation brings a fresh nonce. Keep it only for a hide/show resume.

### F-9 — LOW — Notes list rows lack summary and date (U-09)

**Call sites**
- `utilities/view.rs:75-88` renders the title only. The summary is computed (`notes.rs:81-88`) but never shown.
- TS `NotesView.tsx:535-540` shows title, summary and `formatDate(updatedAt)`.

**Concrete fix:** render the summary and a locale-formatted date through the supplied presentation formatter.

### F-10 — LOW — "Edit in Settings" from a utility discards the utility destination (M7 hand-off)

**Call sites**
- Any `OpenSettings` effect (Indexing "Edit in Settings", background tasks) calls `Launcher::navigate(Settings)` (`utilities_integration.rs:145-149`). `receive_navigation` then sets `navigation = Settings intent` (`command_bar/mod.rs:1383-1402`).
- `App::settings()` calls `hide()`, which clears that Settings intent (`mod.rs:569-580`).

**Consequence**
- When the launcher reopens, the user lands on Commands, not the utility they left. TS left the launcher surface unchanged.
- The requested tab is acknowledged and then dropped, because `App::settings()` takes no intent or tab.

**Concrete fix**
- Do not overwrite `Launcher.navigation` for Settings intents.
- Have `Event::Navigate` hand the intent or tab directly to the Settings owner. This is what M7 needs anyway.

### Checked and found correct (no finding)

- **Requests are finite and canonical.** Every `UtilityRequest::wire()` channel and argument shape matches the real core handlers: `notes:update {createdAt,text}`, `snippets:add` → `{label,trigger,body}` with the real `snippet:user:<uuid>` ID consumed, `clipboard:image` → `{dataUrl,width,height,byteSize}`, `knowledge:sources:list {query,offset,limit}`, and `vault:*` sends IDs only. `Debug` is redacted.
- **Core work stays owned and bounded.** It is detached by `owned_request` (`tokio::spawn` with the permit), so dropping a hidden waiter cancels nothing. Admission uses the same 16-permit `CoreClient` semaphore. Release HTTP is awaited inline, and `gpui_tokio::Tokio::spawn` aborts on drop. The HTTP helper accepts the exact tag prefix only, with a version charset, the exact Accept header, no redirects or retries, 1 MiB and 10 s limits, and lazily caches a successful client.
- **The paste path is canonical.** Emoji paste sends one `search:execute` `copy-and-paste-text`. The core writes the clipboard, awaits `HideForPaste`, sleeps 80 ms and presses ⌘V/Ctrl+V, with the same recording as TS `executeSearchAction`. The UI performs no second write or paste.
- **Quit drain is sound.** `notified.enable()` runs before the state check. Completion removes the job, applies the canonical result and calls `notify_waiters`. Failed notes are retried once per explicit drain, with no replay loop. The Launcher-state and utility error slots are independent, though `durable_write_error` reports only the first (cosmetic).
- **Render does no work, and lifecycle holds.** `render` issues no request. The Utilities owner is lazily created once, with the current `update_status` applied before the first navigation. Hide retains the utility and stops visible polling. Show resumes it and focuses the editor without a command search (parent fixture 5). Back acknowledges the intent.
- **Password and plain TextInput paths are unaffected.** Styled ranges are refused for password fields. Delegation is off by default, and password runs use the masked display length.
- **Scope rules hold.** No new crate, runtime, process, WebView, `unsafe` or Keychain/DPAPI code appears in M6 files. Core, host and MSRV crates are unchanged by `35dca9b..c2a36b5`; the only manifest change is the `tezbar-ui` feature line.

---

## 3. Standards findings (non-functional or evidence hygiene)

- **S-1 (Low) — subscriptions accumulate.** `vault.rs:410` and `snippets.rs:123` push each form field's `Committed` subscription into the permanent `self.subscriptions` vector, so every New or Edit leaks three subscriptions for the whole session. Push them into `field_subscriptions`, which is cleared alongside `fields`.
- **S-2 (Low) — one stale evidence entry.**
  - `docs/native-m06/source-case-map.json` → `parent_composition_tests` still records `sha256 e484f935…` and `count 4`. That is the `7e21f21` file; the pin has `f616ecad…` with **5** fixtures.
  - `parent-composition.md` still says "Four parent fixtures" and cites the earlier 129 bench-control / pre-commit binary without a historical label. `closure-repairs.md` does describe the fifth fixture.
  - Refresh the entry, or label it historical. The 21 lane hashes, 19 source references, 28 fixtures and 90 TS case locations all verify (§5).
- **S-3 (Nit) — hand-written base64, silent image drop.** `clipboard.rs:318-365` hand-writes base64 encode/decode although `base64` is already a workspace dependency used by the core. Also, `decode_url` silently drops previews whose base64 is larger than 8 MiB, with no size message. Reuse the crate and surface a short "Image too large to preview" line.
- **S-4 (Nit) — duplicated checks.** `jobs.len() >= 16` is checked twice in `decode_preview` and in `qr_preview`.

### Design notes (heuristic; manual visual review still outstanding)

- Vault filter chips use `Debug` names (`ApiKey`, `SshKey`, `view.rs:711`). The draft kind chips use raw serde names (`api-key`, `view.rs:616`). Use the TS display labels.
- Clipboard marks pinned rows with a text prefix, `Pinned · …`. §5.6 requires a pin glyph in the trailing slot.
- The confirmation text "Delete the selected items? This cannot be undone." is reused for **Clear history**, which is inaccurate copy.
- Clipboard (7 actions plus 7 conversion chips plus 3) and Indexing (8) show long rows of identical ghost buttons. TS used keycap hints in the footer and an action menu; this is close to the rejected "identical button grid" pattern.

---

## 4. M5 closure (Scope B)

The original M5 report hash verifies: `docs/reviews/GPUI_M5_OPUS_35dca9b.md` = `f0087c4d…6330`.

| ID | Status | Evidence |
| --- | --- | --- |
| **R-1** | **Closed** | `command_bar/mod.rs:1077-1078`: the `activate-deep-search` row branch now calls `finish_open` (`ActivationHandled`, ticket-matched at `ui-model launcher/controller.rs:399-405`), keeping the visible model. No other deep-search path uses `finish_local`. The fixture `review_deep_search_row_retains_live_model_and_pin_activation` passes in my run (Enter, then Alt+1 runs the pin once). |
| **R-2** | **Closed** | `confirm_quit` always calls `finish_open(&activation)` by ticket. Only the error assignment is fenced by `generation`. Because `ActivationHandled` only releases its own ticket, a stale completion cannot release another one. `review_quit_completion_releases_its_ticket_after_results_refresh` (Cancel and failed variants, with invalidation while held) passes in my run. |
| **R-3** | **Closed** | `App::event(Navigate)` calls `put`, then `receive_navigation`, which acknowledges immediately. Deferred utility delivery keeps its own clone (`route_utility_navigation`). `Command` returns early from `put` and never enters the inbox. `hide` and `return_home` acknowledge the current intent. `UpdateInbox::put` reuses the original nonzero nonce for an identical outstanding action; distinct actions stay FIFO, bounded at 16, with monotonic nonces. No consumer relies on inbox history: `rg` finds no production `take`/`pending` consumer. The 64-delivery Settings/AI/Terminal/extension/Command fixture and the pure queue test pass in my run. **M7 note:** Settings intents are acknowledged before `App::settings()`, which takes no intent or tab (see F-10). M7 must take the tab directly from `Event::Navigate`. |

**Original F-01–F-08:** no regression found in their call sites or in the 65 named M5 cases (all `ok` in my run).

**Map integrity:** 65 named cases exist and are `ok` in `/tmp/tezbar-m5-m6-parent-final-native-2.log` and in my log. All 16 `source_sha256` and 18 `reference_sha256` entries match pinned files individually; no root-tree inference was used. The historical `v3` and `historical_pre_closure_35dca9b` sections remain labelled.

---

## 5. Coverage: my execution vs relied-on evidence

**Executed by me (pinned archive, settings as in §1)**
- Native library: **133 passed, 0 failed, 0 ignored** (`pristine-native.log`).
- `tezbar-ui`: **47 passed, 0 failed, 0 ignored**, plus 0 doc tests (`pristine-ui.log`).
- Opus probes (probe copy only): **9 probes, 8 fail (F-1 ×4, F-2, F-3, F-4, F-8), 1 passes** (Enter inside the note editor does not trigger Copy/Activate). Logs: `probe1.log`, `probe2.log`, `probe3.log`.
- Hash and map checks:
  - M6 `file-hashes.json` SHA = `f44be4d1…c8`; all **21** lane hashes match.
  - **19** unique M6 source-reference hashes match.
  - **28** named fixtures are at their cited lines and are `ok` in my run.
  - **90** original TS case locations match their cited lines.
  - The inherited M3 map hash matches.
  - The M5 hashes and cases are as in §4.

**Relied on, not re-executed by me**
- The parent's 134 bench-control run.
- The `ui-final.log` lane run.
- Notes model 370 cases on Rust 1.98.1 and 1.85.
- Release/HTTP 14 cases on 1.85 (loopback).
- The parent's clean-build receipt and binary SHA `67269d39…ad93` (no WebKit, `runtime_shaders=true`).

The preserved failures are acknowledged and were not counted as passes: M6 drain 126/1, root resume 132/1, and the earlier compiler errors.

**Coverage gaps the fixtures miss (now shown by probes):** typing after Enter or into an empty note; edit-range ambiguity; hide during a durable Vault save; an invalid snippet draft at quit; release re-navigation. No fixture asserts System Monitor formatting or Notes row summary/date.

**Unverified (out of reach for this review):** full migration (M7–M11); manual GUI and screenshots in light, dark, zoom and reduced motion; physical IME candidate placement; VoiceOver and Narrator; Windows build/GUI; GPU texture and whole-app RAM; live GitHub HTTP; real Keychain/Touch ID; real clipboard and paste focus restore; native save dialog and file reveal.

---

## 6. Required repairs before M6 acceptance

1. **F-1 and F-2:** repair the Notes projection and pass exact replaced ranges from TextInput. Add keystroke regressions: empty note, Enter then type, list continuation then type, the `<div><br></div>` projection, and forward-delete style.
2. **F-3:** make the durable Vault save follow-up (clipboard source deletion) independent of the session, with a hide-mid-save regression.
3. **F-4:** stop unsaved or invalid snippet drafts from vetoing quit, make retained drafts visible or discard them, and label errors by owner.
4. **F-5:** bundle `docs/releases/<version>.md` in the native build and test it.
5. **F-6 and F-7:** format System Monitor stats with tests, and move the background-task/update indicator to the Launcher footer.
6. **F-8 to F-10:** reset the release version on a fresh navigation; add the Notes summary and date; keep the utility destination for Settings and hand the tab to the M7 owner.
7. **S-1 and S-2:** fix the subscription accumulation and the stale composition-map entry. The design notes go to the next Opus design pass.

Zero findings here are speculative: every functional item is either reproduced by a probe or is a deterministic consequence of pinned source shown at the cited lines.
