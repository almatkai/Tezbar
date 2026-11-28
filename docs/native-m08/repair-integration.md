# M8 repair + M10 shared lifecycle checkpoint

This document supersedes the lifecycle/resource assertions in the historical M8
composition receipt. Original reviews, logs and composition receipts are retained.
Worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch
`feat/native-gpui-app`, base `b626f28476d136a292ed2385e0e1bc2ddedd1489`.
All code remains dirty, unstaged and uncommitted. No GUI or production profile is used.

| Review ID | Implementation and actual production seam exercised |
| --- | --- |
| H1 | App preliminary phase calls `prepare_chat_quit`: input fence + mutation settlement, no unmount/Cancel. Shared Settings, layout and launcher barriers complete before final `drain_chat` disposal. Failure revives the same ChatView and reacquires its Settings owner if retired. Existing final drain API/tests remain intact. Terminal preparation fences input, retains session/grid/configure, and drains Detach without disposing; failed quit clears the fence. App uses one eight-second save/drain/shutdown budget and the same independently owned core shutdown task. Single-instance retirement remains after successful durable saves. |
| H2 | Chat preview gets the SAME NativeCore asset semaphore. Permit is acquired before spawn and moved INTO `decode_blocking` closure. Cache release aborts async decode waiters; actual running jobs retain permits. Slow blocking-job test performs three cache-release cycles, cancels four awaiters, proves no fifth starts, and stale cache tickets never upload. |
| M1 | XML preflight via usvg's already-locked roxmltree re-export counts reference expansion with memoized bounded work, cycle/depth checks, namespace-aware element/attribute names and real XML comment handling. Filters/patterns are explicitly refused by this preview subset. Parsed render tree, including subroots, has a second 8192-node/depth64 budget before rasterization. Tests reject nested-use bomb under50ms and render a legitimate use-based icon. No dependency added, href resolvers remain denied, dimensions/allocation and unpremultiplication remain bounded. |
| M2 | FIFO semaphore admission waits cancellation-safely within the existing overall ten-second operation deadline. Test holds all four shared slots, starts chosen attachment, releases one, and asserts successful read. |
| M3 | Canonicalization, authorization, open, fstat and bounded read run in ONE blocking closure owning the permit. Unix uses NONBLOCK+CLOEXEC; FIFO is refused before reading. Cancelled-wait test proves permit is not returned before actual job completion. Unix fd-relative root/ancestor traversal remains unchanged. Windows locked traversal is described below. |
| L1 | Prepared user-turn append AND preparation config are tracked mutation jobs. App preparation settles them before destructive stop. Held-append fixture asserts non-durable settlement while in flight, then exactly one canonical user turn before final shutdown. |
| L2 | Background effects coalesce. Full queue refuses explicitly, removes attachment ticket, or completes document-image ticket through its usual error path. It never emits an unqueued effect.80 attachment attempts then completion leave zero tickets. |
| L3 | Native model uniform_list receives full catalog length; silent256 truncation removed. |
| L4 | Core `ai/ext/mod.rs` computes `serde_json::to_string_pretty(&args)` then scrubs/truncates explicitly; `ai/legacy.rs::approval_prompt_identity` emits that command string. Native `approval_body` is the exact seam used by rendered card. Actual App/retained-event test asserts nested indented JSON unchanged there, then explicitly approves. Provider/extension execution is synthetic, not a live tool/loopback tool execution claim. |

## Windows scope security and test handoff

`crates/tezbar-core/src/assets_windows.rs` walks from a local disk volume root,
opens every component with BACKUP_SEMANTICS+OPEN_REPARSE_POINT, checks HANDLE
metadata attributes and disk type, and holds all ancestor handles through final
read. READ sharing ONLY excludes existing/new write and delete handles: this
covers rename/replacement and in-place reparse writes. Each normalized handle
path must match its expected canonical path. Original request spelling is checked
before canonicalization may erase an ADS, 8.3 or symlink alias. Device/UNC paths,
reparse points, DOS names, colon/tilde/trailing-dot-or-space components fail closed.
Exact legacy grants use the same locked traversal. This uses stable kernel32 FFI
plus Rust OpenOptionsExt/MetadataExt, with no new dependency/feature edge.

Security is not a filesystem sandbox: hardlinks, ordinary same-path file-content
writes before admission and extensions' existing user authority remain outside
this path-capability policy. Read-sharing may refuse paths while other writers
hold handles; UNC/network roots are explicitly unsupported here. Windows behavior
is UNEXECUTED on this Mac and remains a platform acceptance blocker, not a pass.

Run in the parent's isolated real-Windows checkout with Developer Mode enabled
(or an elevated DISPOSABLE test shell for symlink creation), no production profile:

```powershell
cargo +1.98.1 test -p tezbar-core --lib --locked --offline -j2 assets::windows::tests:: -- --test-threads=1 --nocapture
cargo +1.85.0 test -p tezbar-core --lib --locked --offline -j2 assets::windows::tests:: -- --test-threads=1 --nocapture
cargo +1.98.1 test -p tezbar-core --lib --locked --offline -j2 assets:: -- --test-threads=1
```

The self-contained Rust harness creates temp directories only and checks file/
directory symlink, junction, ADS/DOS/8.3/device/UNC refusal, exact legacy grant,
removal, held-ancestor write/delete refusal, and concurrent ancestor/root/nested
junction swaps (outside bytes must never return). Parent linked environment:
`e8399889-5c93-4fa0-b469-36ba9262ff6b`. No remote action was performed here.

## Evidence and acceptance boundary

See `repair-source-freeze.json`, `repair-gates.json`, `repair-logs/` and
`repair-receipt.json`. Every final individual command checks all frozen source
hashes before/after. Development red compile/fixture logs are preserved. First full
native attempt found two callers relying on the existing destructive drain API;
new preliminary API preserves those original tests without weakening assertions.
Original Opus red probes remain in `/tmp/tezbar-m8-opus-*`; none was modified.

M7 N-B1 is parent-authored, narrowly applied from exact hashed patches between
gates; its red/green evidence is recorded separately in this same gate ledger.
Other protected M7 content, both root-added app fixtures, M9/M11 HOLD content,
provider/auth wire and TypeScript suites are unchanged by this implementation.

Fresh parent Opus review, real Windows, manual IME/AX/GPU/window placement and RAM
validation remain required. No zero-actionable-findings, low-RAM or full migration
acceptance is asserted. M9/M11 remain pending; Bun/TS is compatibility only.

Final source freeze was revised once after component gates solely to strengthen
`ai_chat/tests.rs`: it now actually fills64 effect slots, refuses both chooser and
clipboard tickets synchronously, asserts background coalescing, then settles all
accepted tickets. That is the ONLY source difference from the359-input component
freeze84313f28. Production/core/UI/model/MSRV inputs are identical. Focused fixture
and full native/bench suites were rerun against final freeze32ecb739. No unchanged
core/UI/model suites were repeated for this test-only change.
