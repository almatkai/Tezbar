# M8 native chat — first compiler gate, 2026-11-07

Owned source is frozen and unstaged on `feat/native-gpui-app`, after parent explicitly
released the first Cargo gate and exported `ai_chat` / `Request::Chat` / CoreClient
transport. Tested parent HEAD: `0c3dbea17b4681fb55887470c23897009420dc18`.
No M8 compilation was invoked before that release. No stage/commit/push/PR was run.

Actual results:

| Operation | Result | Retained evidence |
| --- | --- | --- |
| First `cargo check` | failed: 13 own view errors (Stateful Div roles; uniform_list signature) | `logs/first-check.log` |
| Second `cargo check` | passed; shared/core warnings only plus two own dead-code warnings | `logs/check-02.log` |
| First native target compile | failed: one own test borrow across focus call | `logs/native-first.log` |
| Native target 02 | 24 passed, 3 failed | `logs/native-02.log` |
| Native target 03 | 26 passed, 1 failed | `logs/native-03.log` |
| Native target 04 | failed: own test helper key lifetime | `logs/native-04.log` |
| Native target 05 | **27 passed, 0 failed, 0 ignored**, 187 other tests filtered | `logs/native-05.log` |
| Owned rustfmt parse/check, whitespace, TS source hashes and test-filter audit | passed | `logs/owned-static-checks.log`, source map and freeze JSON |

Commands used Rust **1.98.1**, `CARGO_BUILD_JOBS=2`, `--locked --offline`:

```
cargo +1.98.1 check -p tezbar-native --lib --features runtime-shaders --locked --offline
cargo +1.98.1 test -p tezbar-native --lib --features runtime-shaders --locked --offline surfaces::ai_chat -- --test-threads=2
```

The three runtime failures were investigated separately. Composer Enter exposed an
actual production bug: GPUI resolves the shared TextInput Newline action before
low-level key capture. The production view now captures that action with focused
field and composition guards. The core-clear fixture expected a transport Err,
where canonical storage returns `{ok:false,error}`; its assertion now checks that
real refusal. The accessibility probe expected a full tree from TestWindow, which
at this pin lacks `PlatformWindow::a11y_init` (same independently proven M7 limit).
The original probe is preserved in `accesskit-red-probe.rs.txt`; the test retains
its name and asserts the actual production card/button AccessKit roles/names,
Message multiline options and scrubbed production diagnostics/error values.
Native target 03 then found that the fixture emitted key-down only while native
buttons activate on key-up. The corrected fixture emits a real down/up pair and
also exercises marked composition. No ignored/skipped/deleted tests were added.

The 27 tests exercise actual GPUI ChatView/TextInput/Markdown/ImageCache production
components and the real finite request wire. Coverage includes owner/background
streaming and partial stop, failed persistence and manual save, delayed config and
append preparation deletion fences, failed-delete deferred durability, clear
failure and >200 saved rows, exact retry identity/canonical user reads, repeat
nonces, capture errors and stale cleanup, atomic/deduplicated image attachments,
8 MiB source budget, current/historical image fences, native composition and
approval keyboard separation, owner-scoped Settings merges, timeline projection,
Markdown host-ticket decode/release and diagnostic redaction.

The large-output fixture supplies **21.875 MiB** of tool stages through actual
ChatView `core_event` handling (10 runs × 70 × 32 KiB), then asserts chronological
last16 canonical context IDs, zero copied clean fallback bodies, <4096-byte run
wire and bounded visible projection. It does not claim every signed/tool replay
assertion from the parent core regression suite.

Two tests use real AiCore/task-private durable storage. The HTTP fixture binds one
ephemeral 127.0.0.1 listener, uses a synthetic Ollama model, feeds the finite Run wire,
receives the native answer and persists response metadata, then reads exact
canonical USER-turn image bytes and rejects wrong-session/assistant/deleted data.
The storage fixture writes 240 sessions, observes the 200-row list cap, clears all
with one request and proves a not-yet-appended owned session cannot later append.
Neither fixture starts desktop services, touches actual profiles/credentials,
uses a real clipboard/permission dialog, or contacts an external provider/MCP.

The 72 originating TS source-case rows retain their original hashes. Each native
mapping describes only demonstrated assertion subsets. Incorrect inherited
mappings (for example retry-delete mapped to a send-delete-only native fixture)
were removed instead of calling those full source cases green. All 27 native
filters exist in the passing log. C22–C30 remain with M7; primary ledger rows are
not closed by this checkpoint. Full case parity and independent review remain
open where the source map explicitly says so.

Bounds: controller 32 retained owners, 256 identity window,16 canonical context
IDs,2 MiB presentation eviction target protecting the newest pair (may exceed),
4 MiB text/turn,512 stages ×32 KiB output/run. Adapter four images/prompt and8 MiB
source/image,32 payload slots (at most357,913,984 encoded bytes before transient
wire clones); released at acknowledged Start. Preview decode8 MiB source,
16,384 dimension ceiling,128 MiB allocation,1024² thumbnails; shared cache32 MiB,
128 entries,four in-flight tickets.128 view jobs,200 visible history rows,
256 contributions/64 matches,32 argument fields,128 expanded rows,80 diagnostics
×2000 chars,32 KiB expanded output. Presentation coalesces16 ms; background status
emits only changed snapshots. No Render I/O/timers or tool replay executor exists.

Parent owns actual retained App/Launcher composition and host asset/clipboard/file
routes, Settings review, footer injection, final all-native/bench checks, final
check + normal runtime-shaders build, whole-source freeze and mandatory fresh
Opus5.5 review. Those gates have **not** been claimed here. Full platform
AccessKit/VoiceOver/Narrator, physical IME, Windows, GPU/RAM, real providers and
native visual acceptance remain separate. Native remains opt-in until M11.

`source-freeze-first-gate.json` records exact own source hashes. `handoff-files.json`
records all own deliverables except itself. No other source/manifests/lock/export/
App/Launcher/shared-UI/model/core file was edited by this child.
