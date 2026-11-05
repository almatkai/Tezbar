# M3B repair evidence

Selected tree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`,
branch `feat/native-gpui-app`, starting HEAD
`c741388ed83335b73fd09847ae3af5c59d6e05c5`. No branch/worktree change, staging,
commit, push, PR, release, Actions, installation or Windows-host edit was made.
Peer M4 changes were present at entry and were neither edited nor synchronized.
During work the parent advanced HEAD to `164a1aa` with its log dependency edge
(`Cargo.lock`/app manifest); no owned repair file was committed by this lane.
Primary ledger/common verification/manifests/locks/exports remain parent-owned.

The original [Opus review](../reviews/GPUI_M3B_OPUS_a52240a.md) remains immutable
and rejected. This wave implements all F1–F12 plus create/update and regex repairs;
[per-ID ledger](agent-settings-review-repairs.md) lists the changes and precise tests.
Fresh independent Opus acceptance is still required. The full migration continues
through M4–M11; no GPUI view, DOM, actual focus, paint or host row is closed here.

## Reproducible validation

All logs/scripts/source manifests are in
`/tmp/tezbar-native-m03b-repair-evidence/`. Every Cargo invocation uses
`CARGO_BUILD_JOBS=2` and the existing lock. No core regression suite was repeated.

| Check | Result | Log |
| --- | --- | --- |
| F1 failing-before probe | Worker records incorrectly produced 3 executable effects; regression failed | `f1-before.log` |
| F2 failing-before probe | Follow-up context was only s-r3 instead of eight prior turn IDs after 70×32 KiB stage outputs per run; regression failed | `f2-before.log` |
| Parent F3 failing-before/after | Parent repaired failed creation field loss and retry order; retained, updated only explicit creation constructors | `/tmp/tezbar-native-m03-parent-f3-before.log`, `/tmp/tezbar-native-m03-parent-f3-after.log` |
| Full agent-models during repairs | 220 passed / 0 failed / 0 ignored before final added mapping/float regressions | `agent-final.log` |
| `cargo +1.98.1 test --locked -p tezbar-ui-model --features launcher-models,agent-models` | **337 passed / 0 failed / 0 ignored** | `combined-final.log` |
| `cargo +1.85.0 check --offline --locked -p tezbar-ui-model --features launcher-models,agent-models` | passed | `msrv-check.log` |
| `cargo +1.85.0 test --offline --locked -p tezbar-ui-model --features launcher-models,agent-models` | **337 passed / 0 failed / 0 ignored** | `msrv-tests.log` |
| Actual core durable wire integration, explicitly rustc 1.85 | passed: controller PersistTurn → chat:append/get, responseMeta including normalized floating usage, optional stages, image descriptors, failed-delete deferred durability, tombstone rejection | `core-wire-probe-final.log`, `core-wire-probe.rs`, `run-wire-probe.py` |
| `pnpm typecheck` | passed, renderer + main | `typecheck-final.log` |
| `pnpm test` | **745 passed / 7 existing skipped** across 82 files (one existing skipped file) | `typescript-final.log` |
| Original worker tests + new simulated-host system-effects fixture | 30 passed / 2 existing skipped; host clipboard/open/reveal calls once per explicit action, no calls on polls, 60 actions across sliding windows | `worker-unit.log` |
| `pnpm test:extension-worker` | actual Bun process passed, including 60 actions with full 10,010-character clipboard payloads and image-file copies; 50-entry record window, monotonic totals and two unchanged polls per action | `bun-worker-final.log` |
| Mapping/hash/ownership audit | all 354 rows retained; 31 original source hashes unchanged; every mapped filter appears in passing log; immutable report and core source unchanged | `verification.json`, `owned-files.json`, `source-test-hashes.json` |
| Formatting and whitespace | only owned Rust sources formatted; whitespace passes | `format-check.log`, `diff-check.log` |

Initial failures are retained: the before logs, first TakeWhile reverse compile
failure (`first-after.log`), two intentionally obsolete assertions caught during
repair (automatic approval focus and discarded inactive Knowledge error), a fixture
mistake assuming unchanged worker polls included trees (`bun-worker.log`), and a
standalone probe selecting mixed-toolchain cached rlibs (`core-wire-probe.log`).
The actual core probe then exposed `tokenCount:42.0`; reading it as u64 failed,
which motivated the final regression and numeric compatibility repair. The final
probe links the 1.85-compatible cached rlibs explicitly and runs only chat storage
channels under a task-private temp fixture. It starts no core services or agent run.

## Delivered scope and extra worker files

Owned Rust changes are limited to chat(+tests), Settings(+tests, providers,
contributions), Knowledge(+tests), extensions runtime(+tests)/images(+tests), and
terminal. Vault and extension catalog/forms were inspected but needed no edit.
`owned-files.json` provides SHA-256 for every delivered source/document.

Exact extra TS/fixture files (F1 authorization):

1. `src/main/extension-runner.ts`: six additive lines: initialize/increment the
   per-session total, include it beside effects in view and both no-view replies.
2. `src/shared/extensionRuntime.ts`: optional effectSequence on the existing result
   variants; existing effect shapes, serialized protocol version and dependency
   graph are unchanged. The TS renderer still only consumes effectNotice.
3. `src/main/extension-runner-effects.test.ts`: new simulated desktop-runtime fixture
   exercises system-mode clipboard/open/reveal ownership and notice sequences.
4. `scripts/test-extension-worker.ts`: additive effect-wave command and assertions
   using its existing actual-Bun/in-memory-host helper. Existing checks stay intact.

The backend build rewrote three tracked `dist-backend/extension-worker*` artifacts;
those generated bytes were copied into the evidence directory and restored to
entry HEAD bytes. No generated build artifact is part of this repair delivery.
No original renderer/shared/main test file was changed; original test-source
hashes are verified. Existing Rust fixtures were migrated to explicit CreateProvider;
two old expectations were corrected because they asserted the reviewed bugs.

## Integration limits

[Adapter interface notes](agent-settings.md#repaired-interfaces-for-m7m10-adapters)
are required when wiring M7–M10. No assigned repair is delegated to a future adapter:
all controller state transitions/intents are implemented and tested here. Real
native authorization/path canonicalization, app-icon lookup, decoding/texture
release, focus acknowledgement, AccessKit names/actions, geometry and painting
must still be implemented and accepted in their owning milestones. The image
mapping explicitly marks that native portion partial rather than model-complete.
Legacy workers cannot identify a genuinely new identical notice without sequence;
conservative duplicate suppression is documented and never re-executes a record.
Canonical signed/image/tool payloads remain core-owned; no replacement transcript
store or unbounded fallback payload cache was introduced.

No real model/provider/MCP, credential, profile, Keychain, clipboard, URL opening,
file reveal, script or PTY/shell effect was invoked. Worker desktop functions in
unit tests are spies, and actual Bun clipboard calls terminate at the fixture's
in-memory host. No live endpoint was contacted by the repairs.
