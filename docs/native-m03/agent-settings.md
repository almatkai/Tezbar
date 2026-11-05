# M3 Lane B: agent/settings presentation models

**Review status: needs repairs.** Opus found controller correctness/parity bugs,
including cumulative effect replay and trimmed AI context. See the immutable
[review](../reviews/GPUI_M3B_OPUS_a52240a.md) and current
[repair status](agent-settings-review-repairs.md). The delivery evidence below
describes its tested checkpoint; passing tests do not override these findings.

This is model progress for the full Rust/GPUI migration. No GPUI view ledger row is
closed by this lane. The parent continues M4–M11 and owns integration, all manifests,
locks, exports, common orchestration and commits. Work is confined to
`Tezbar-native-rust-agent`, branch `feat/native-gpui-app`.

## Implemented interface map

| Module | Interface and behavior | Executor integration |
| --- | --- | --- |
| `chat.rs`, `chat/meta.rs` | `ChatController`: one owner per conversation; nonced boot and saved-history loads; frozen send/retry preparation; canonical stored user identities; model/cwd/profile/permission selections; stream events before start reply; incremental text and ordered stage updates; canonical final text; thinking content suppressed; approval focus/Enter/Deny/remember capability and independent mode changes; background runs; stop; preparation/deletion fences and recovery; confirmed turn deletion; per-turn failed persistence retention and retry; capture generations and attachment references; prompt resolution and once-per-folder review intent; bounded history and idle owner release. `FrameCoalescer`, metadata heuristics, provider error presentation. | Subscribe to `agent:event` before executing `Start`; use the supplied run id. Apply `SaveSelection` durably before completing send preparation. Resolve image blob references only in the request adapter. Core remains authoritative for signed provider history, tool outcomes, provider execution and image validation. Feed start response and append/persist/delete replies back with their tokens. Inject fresh ids and monotonic milliseconds. |
| `settings.rs`, `settings/providers.rs` | `WriteQueue`: serial read/merge/save protocol; logical revisions independent of execution order; per-field successful revision ownership; provider removal tombstones; conditional model fallback after explicit selections; deduplicated failed retries; retired owner drains; optional discovery failures do not block close; acknowledged close waits without timeout and failures retain recoverable message; only unacknowledged close has 3 s fallback. `Draft` hydration fencing; `ProviderDraft` provider isolation, generation-fenced discovery and metadata retention. `DeviceSignIn` save/start/poll/cancel lifecycle. Provider configuration/model normalization and picker options read the existing core catalog. | Enqueue edits when intent occurs, before navigation/close. Debounce rendering if needed, not logical revision assignment. Execute `Read` against fresh **durable** config, not the merged extension-contribution view, then `Save` its generated patch. Call `finish(false)` for either exceptions or `{ok:false}`. Pump the queue through newer owners even after a failed old write. Finish device provider save before OAuth. |
| `settings/contributions.rs` | `Contributions`: all 23 existing `ai-extensions:*` actions identified; explicit import preview then Add; successful test/review required for enabling; review descriptors retained; write-only scoped secrets; owned dirty instructions; fresh list generations; explicit OAuth/reset/resource/project actions. UI-facing state redacts secret reference ids. | Execute typed action and positional args through existing core. Apply core trust-pin/review validation; this module never supplies authority to bypass it. Retain opaque original manifests for edits and expose only redacted display data to views. No MCP client or provider engine is created here. |
| `knowledge_settings.rs` | `KnowledgeSettings`: initial unknown state; 10 s read timeout, 15 s mutation timeout; superseding request generations; quiet serial 1.5 s healthy visible polling; explicit retry after read failure; retained snapshot controls; fresh-read reconciliation barrier that survives Cancel; status events cannot unlock it; legacy/structured Q8/Q4 failure attribution; selected-index count fencing; row actions, pending accessible names, focus recovery policy, semantic summaries/count separation, progress below 100%, authoritative weight sizes; root/depth/speed/pause/resume/start effects and exact variant arg positions. | Check `accepts_read` before dispatching deferred effects. Feed every completion with token, including failure. `recover_owned_focus` takes whether focus is still owned so it cannot steal deliberate focus movement. Actual GPUI focus handles/live regions remain view work. |
| `extensions/{runtime,forms,catalog,images}.rs` | `RuntimeSession`: validated finite worker tree; session/generation/latest-response ownership; event-driven vs legacy polling; hidden invalidation coalescing; stable row-id selection, clamped fallback, near-end pagination; client filtering vs server search; callback ids, shortcuts, action filter, navigation stack/back callbacks; form submit guard and notices; typed effects, worker scripts never replayed. `FormState`: controlled authoritative fields, submit-only draft preservation, typed field edits, required/errors/options, file/tag/date values, wheel acknowledgement; command-scoped preference partition. `Catalog`: paged latest search, installed metadata overlay, runtime/platform/check/acquisition truth, progress and selection. `ImageCache`: scoped leases, request coalescing, late completion release, byte/entry limits; source resolution for builtin/glyph/data/blob/file/HTTPS. | Build `TreeReply` from the serialized Bun worker tree; no React or arbitrary WebView. Request replies must carry `RequestToken`; call `refresh_finished`, `no_view`, or `failed` so guards release. Drain typed effects once. Native image fetch/decode, durable image cache, canonical-path/symlink validation and GPUI texture handles are adapter work. A `Source::File` is an intent, not permission to read a file. |
| `terminal.rs` | `Sessions`, `Session`, `Defaults`: explicit selection, numeric shortcuts, stable session identity and restore-command metadata, labels/path policy, pins/default persistence intents; linear overlap replay and attachment generations. Session serde preserves native `status` and unknown metadata. `ListenerGate` requires data and exit readiness before Create/Attach. | Core owns shell selection/PTY creation, writes, resize, kill and history. Actual subscriptions, emulation, marked text and window routing remain M10/host integration. No shell is launched by this module or its tests. |
| `vault.rs` | Detection rules, stable icon-id registry/validation/fallback, copy status/unlock/cancel/copy ordering and generation fence; redacted draft and frozen save payload, name-touch policy, lock clears sensitive draft and pending save; clipboard-history removal only after durable save success. | Core retains crypto, lock/unlock, Keychain/DPAPI and 30 s clipboard clearing. Use `prepared_save(token)` for that frozen payload, then `saved`. No secret/clipboard/system access occurs in tests. |

The provider catalog is **not forked**: `provider_catalog()` uses `OnceLock` to parse
`crates/tezbar-core/src/ai/provider_defaults.json` directly at compile time. Core's
`ai::config` module is private; a future public read-only catalog export/channel can
replace this source-path include without changing these interfaces. No dependency or
export change is necessary to compile `agent-models` now.

## Ownership and memory invariants

Constructors perform no I/O and start no Tokio worker, Bun process, PTY, model,
network operation or native service. Effects own only the information the adapter
needs. Secret-bearing config, forms, manifests, run requests, drafts and effects do
not derive diagnostics that print their payloads. Vault payload Debug is redacted.
Chat diagnostic events are deliberately not cached; core's scrubbed live diagnostic
presentation still needs its M8 view adapter.

Chat retains at most 32 owners, at most 256 loaded turns per owner, a soft 2 MiB
history-text/tool-output budget (the current response may be up to 4 MiB), at most
16 failed turn saves before refusing further sends, 512 stages, eight attachment
references, 200 history summaries and 512-byte previews. `trim_idle` drops settled,
saved background owners. Dirty turn payloads remain `Arc` references so trimming
visible history cannot lose a failed write. These are controller bounds, not a
measured full-app RSS claim.

Trees cap depth at 64, nodes at 20,000 and serialized props at 2 MiB. Nested
navigation caps at 32. Image leases cap entries at 128, each resource at 2 MiB and
total ready resources at 16 MiB; last-release and hide release handles. No base64
image payload is copied through chat snapshots or history previews. Remote native
images follow the spec's HTTPS policy; legacy TS HTTP conversion is not reproduced.
The image policy is pure; fetching, decoding and canonicalizing trusted local paths
are still required before displaying resources. The lease cache has no unowned
in-memory LRU entries: release removes the entry immediately, matching current TS
ownership. A durable adapter cache can provide offline revisits.

## Regression mapping and evidence

`agent-settings-test-map.json` records every expanded source case separately with
source path, line, original title, parameter row and SHA-256. It points to real
controller test functions; many source cases share a Rust harness that exercises
several assertions. Rust harness count and TS parameter-row count are not presented
as equal. No old source test was edited, removed or skipped.

The current source has **81** expanded Knowledge controller cases, plus six
embedding IPC variants and 45 AI Settings cases in `SettingsView.knowledge.test.ts`.
The spec's 60/72 counts predate subsequent regression additions. All 81 controller
cases and all variants have entries. Across the 31 inspected regression source files
there are 354 expanded cases. The Detail wheel pixels test remains native-renderer
work. Two legacy Tauri transport assertions (launcher-hotkey transaction and
extension-hotkey event delivery) belong to the host/core-client adapter, not this lane.
All model mappings still leave DOM geometry, actual focus/a11y, platform event
subscriptions, pixels and user-visible GPUI behavior pending.

Raw evidence is in `/tmp/tezbar-native-m03-agent-settings-evidence/`, including source
inventory generation scripts, prior failures, targeted Cargo output, Rust 1.85
checks and owned-file hashes. Only `tezbar-ui-model --features agent-models` was
tested. Launcher models stayed off. No whole-workspace formatting, manifest/lock
edit, staging, commit, push, release, install, live endpoint, real profile, Keychain
or clipboard operation was performed by this lane.

The first test failure was an overstrict retry fixture expecting a queue item even
after a newer success had correctly superseded it. The next run caught an incorrect
textarea-Enter fixture and a real stale client-search action-reply bug. Another run
caught an unattributed semantic error still displaying a healthy index label; that
presentation bug was repaired. Original failed logs are retained. One online Rust
1.85 check refused a contemporaneous root lock resolution; the offline locked check
passed without changing any lock. Final commands/results follow below.

## Remaining integration and explicit limits

No UI row is marked native here. No GPUI renderer, markdown/code theme, actual focus
or keyboard binding, file dialog, texture decoder, PTY emulator, standard Settings
or onboarding screen is implemented by these model-only files. `UiEvent` and
`NodeKind` define the renderer seam, not an implemented GPUI node renderer. Opus
visual/accessibility review and dual-platform host checks are still required later.

The app executor must preserve the ordering and token contracts above, particularly
fresh durable Settings reads, subscription-before-run, canonical stored user replies,
per-turn persistence acknowledgements, scope-owned resource leases and completed
form invocations. Typed task/contribution result payloads are still largely the
existing JSON wire values; they do not replace the core schema, trust or validation.
UI presentation errors are separate from diagnostics; raw provider/secret payloads
must never be logged by adapters. `Boot::Submit` initializes a draft with fresh
identity; the view invokes `send` once using injected turn/run ids, rather than
implicitly starting work during construction. Parent integration must preserve that
one invocation.

No dependency request, root export change or integration blocker remains for this
feature-gated package. The parent must still implement and test the effect adapters
and all assigned GPUI views before full migration parity can be claimed.

## Final targeted validation

| Command | Result | Evidence |
| --- | --- | --- |
| `CARGO_BUILD_JOBS=2 cargo +1.98.1 test --locked -p tezbar-ui-model --features agent-models` | **190 passed, 0 failed, 0 ignored**; 162 Lane B harnesses + 28 unchanged updater-policy harnesses | `tests-final.log` |
| `CARGO_BUILD_JOBS=2 cargo +1.85.0 test --offline --locked -p tezbar-ui-model --features agent-models` | **190 passed, 0 failed, 0 ignored**, same source and test inventory | `msrv-tests.log` |
| `cargo +1.85.0 check --offline --locked -p tezbar-ui-model --features agent-models` | passed | `msrv-offline.log` |
| Mapping/source audit | 354 expanded TS case rows; all 351 referenced model filters executed; three explicit native-renderer/host-only entries; 81 Knowledge controller rows mapped; original TS source hashes unchanged | `verification.json`, `source-cases.json` |
| 100-cycle controller/image lifecycle fixtures | passed; settled owners and owned image handles released | tests `one_hundred_settled_surface_cycles_release_background_owners`, `one_hundred_mount_hide_cycles_release_image_handles` |
| Owned source formatting and diff whitespace check | passed | `format-check.log`, `diff-check.log` |

Existing core warnings are outside this lane and were not modified to quiet the
build. `owned-files.json` lists the exact 22 owned files and final SHA-256 values.
`agent-settings.patch` contains only those files, with no manifest, lock, root
export, core, host, app, peer-controller or shared-ledger changes. The patch is a
review artifact; the files already exist in the shared worktree. No lane commit
was made, and concurrent peer edits are not claimed to be a clean working tree.
