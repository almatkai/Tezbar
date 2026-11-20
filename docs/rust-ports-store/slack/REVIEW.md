# Independent Slack development-port review

Reviewed 2026-11-20. Read-only review of the frozen standalone Slack port, pinned upstream source, current host contracts and supplied evidence. No build, executable, user profile, real account or desktop operation was run. This report is the only file written by the reviewer.

The findings below describe the original source freeze `930e53ae38a341774d0132f35027766815a9e568d4ba213479deadd69505b590`. The parent began repairs during review. Current live files already differ from that freeze; historical evidence must not be presented as verification of repaired source.

## Findings

### P2 — saved unread selections outside the first directory page cannot be retained on Save

`src/ui.rs:230-235` deliberately adds saved conversation IDs as picker options when they are absent from the currently loaded directory. However, `save-config` in `src/main.rs:663-672` validates every submitted ID exclusively against `state.directory`.

Reproduction: load a second directory page, save conversation C234 from that page, reopen Configure Conversations (which resets the directory to page one), leave C234 selected, and Save. The visible saved choice fails with `Unknown conversation choice`. The original fixture verifies that C234 appears as an option but removes it before Save (`scripts/protocol-smoke.py:134-139`), so it misses this path.

Accept loaded directory choices or the persisted selection for the same authenticated workspace. Do not trust `state.selected` alone, since form paging can replace it with client-submitted values. Cover reopen-and-keep-save as well as a forged unknown conversation ID.

**Repair handback:** the parent has added the persisted-workspace validation path and fixture changes in the live source. Fresh build/package/process evidence is required; no independent repair sign-off is claimed here.

### P2 — message pagination fetches rows that its fixed display bound never exposes

`src/ui.rs:388-395` sets a fixed 1,000-row display limit for non-emoji lists and truncates `actionRows` to that prefix. `src/ui.rs:550-551` nevertheless advertises message `hasMore` using only Slack's remaining pages, while `search_messages` accepts up to 10,000 accumulated results (`src/main.rs:142-150`).

For a query with more than ten full 100-message pages, each additional Load More fetches/appends another page but the displayed rows and their actions remain the same first 1,000 entries. `hasMore` continues promising further content. Either provide a bounded way to expose subsequent rows, or stop pagination at an explicitly stated display bound. The fixture should cover the first page that crosses 1,000 and the terminal bound, rather than only its two-result pagination example.

Unread lists also use the fixed 1,000-row display bound; their search text currently does not filter `state.rows`, despite the limit banner recommending a narrower search. This reinforces the need to distinguish collected-row limits from accessible UI rows, but the primary confirmed pagination trigger above is search messages.

**Repair handback:** the live UI/source now includes a shared message-result limit and a terminal `hasMore` condition added by the parent. Validate the final bounded behavior against fresh executable evidence.

### P2 — marking a conversation read truncates the cursor behind current-second messages

`src/main.rs:695` sends `now().to_string()` to `conversations.mark`. `model::now` uses `Utc::now().timestamp()`, which discards fractional seconds (`src/model.rs:9-10`).

A message with timestamp `5000.900001` can be displayed and marked at time `5000.95`; the sent cursor is only `5000`, so it remains before that displayed message. The method can report success while the immediately refreshed unread list still contains the message. The pinned upstream `SlackClient.ts:404` preserves the millisecond fraction with `new Date().getTime() / 1000`.

Slack defines `ts` as the read cursor identifying the most recently seen message. See the [official conversations.mark argument reference](https://docs.slack.dev/reference/methods/conversations.mark/). Use an exact appropriate displayed/latest message timestamp or preserve fractional current time according to the intended whole-conversation behavior. Cover a message received in the same second; the original fixture only checks that the method was called and returns a constant old history timestamp.

## Evidence independently checked

- The original optimized Windows archive SHA-256 is `7ec213f407fadc69b8394cdc662437ec4a1a238e4bb7ae13dd318919dddaff0f`.
- All **238 inventoried members plus extension.json** matched their recorded SHA-256 values; ZIP CRC checks passed, duplicate names were absent, and packaged policy matched the root manifest excluding only files/artifacts.
- The executable SHA-256 is `44459b416dbe84562dcd79acd7460195dd0e09cb228e0731849225b6c11e7c2e`. Fixture endpoint and platform-effect environment-hook marker strings are absent from the packaged executable. The packager also checks feature-free build-info, executable identity, protocol and all nine command names.
- All **116** retained upstream files matched their recorded Git blob identity and size under `UPSTREAM.json` at commit `f8e165c85832e16d9c24152bbb98b4871b954fc8`.
- Original extension preference objects and every source command field matched the native manifest; additional native runtime/concurrency/platform fields do not replace those original definitions.
- Supplied verification records six Rust tests and 15 real JSONL/owned-loopback fixture groups, with recorded zero exit codes for formatting, tests, fixture build/run, optimized build and packaging. The process suite services real child stdio and records synthetic HTTP/host effects; it is stronger than model-only tests, but its assertions do not cover the three original triggers above.
- At package audit time, `src/main.rs` and `scripts/protocol-smoke.py` had already changed from the original freeze due to parent repairs. Subsequent UI changes were also observed. Rebuild and refreeze before claiming package acceptance for repaired source.

## Contracts inspected without another confirmed serious defect

The production API origin is fixed to `https://slack.com/api/`; route names are constrained, redirects are disabled, bearer headers are sensitive, and proxy/transport/API errors avoid raw credentials and response bodies. API response bodies are bounded to 4 MiB, requests have connect/total deadlines, and the code adds no application-level automatic replay loop. The 429 and failed-POST fixture paths support their stated limited checks.

Rendered action IDs are collected into a server-owned allowlist and fenced by process-wide view revisions before row mutations. Message recipients must resolve to loaded directory entries or the original selected message; thread replies retain their conversation and parent timestamp and reject scheduling. Existing message drafts are carried across recipient-directory paging. Side-effect uncertainty after transport timeout remains explicitly reported.

Platform shortcut source is fixed, not interpolated from user text; owned subprocesses have deadlines and kill-on-drop. Real OS focus, Accessibility, Slack URI behavior, proxy/account scopes, GPUI rendering and native Mac execution remain acceptance gaps, not verified defects. All nine launch commands and original preference definitions are represented. Managed OAuth, AI tools/resource provider, attachments/rich presentation and legacy-cache migration are explicitly unfinished and should remain distinct from command entry-point coverage.
