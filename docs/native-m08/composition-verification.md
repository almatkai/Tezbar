# M8 composition verification — final linked checkpoint

Selected worktree `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`,
branch `feat/native-gpui-app`, HEAD `409dbf3c5950b0065aff900b2906b548660bc264`.
Implementation and evidence remain unstaged. No branch/worktree switch, commit,
push, PR, release, Actions, binary/UI launch or explicit installation command was
performed. This checkpoint does not accept the full migration.

## Frozen inputs and actual results

Final freeze: [composition-source-freeze-02.json](composition-source-freeze-02.json).
It contains327 conservative compiled/build/asset inputs and26 owned/coowned source
hashes. Aggregate SHA-256:
`36d27bb8c6798a1850eaea51fe535093bfcb08f70a64b4e70c2dd7bd309282b3`.
Every input was checked before/after every final gate; none changed. Protected
Settings/onboarding/window/model queue and app/m7.rs files equal parent HEAD.
The root workspace GPUI pin/toolchain are unchanged. M9/M10 sources are unexported
and excluded from the compiled input inventory. Concurrent peer drafts are not
M8 deliverables. HEAD alone is not proof of these dirty workspace inputs.

All18 final commands and exit codes are in
[composition-gates-02.json](composition-gates-02.json). Rust commands use `-j2`,
`--locked --offline`; native normal/bench builds enable runtime-shaders and only
the bench test gate enables bench-control. Tests use `--test-threads=2`.

| Gate | Actual result |
| --- | --- |
| Rust1.98.1 full normal native lib |254 passed,0 failed,0 ignored |
| Rust1.98.1 full bench-control native lib |255 passed,0 failed,0 ignored |
| Shared native UI |52 passed,0 failed,0 ignored |
| Normal native check and normal debug executable build |both pass |
| Core assets filter |10 passed (includes existing system/voice cases) |
| Canonical registry asset_scope |2 passed |
| Core run_selection, including actual loopback request routing |3 passed |
| Core chat/history/clear filter |7 passed |
| Shared ui-model all features, Rust1.98.1 |379 passed |
| Core and ui-model all-features MSRV checks, Rust1.85.0 |both pass |
| Core assets on1.85.0 |10 passed |
| Shared ui-model all features on1.85.0 |379 passed |
| Cargo metadata, normal dependency tree, otool linkage, git diff --check |all pass |

Scoped rustfmt checks cover the new owned modules and edited chat/shared UI
modules, without recursively rewriting protected parent code. Their commands and
TS checks are in [composition-static-02.json](composition-static-02.json).
`pnpm typecheck` passes. `pnpm test --maxWorkers=2` passes:81 files/745 tests,
with1 file/7 existing skipped tests. No TS source/skip changes were made; the
separate TS input snapshot was verified unchanged. Native/shared-UI/model suites
have zero ignored tests. No assertions were deleted or disabled to pass a gate.

Normal debug executable `target/debug/tezbar`,202436920 bytes, SHA-256:
`f86c946aa4d674410fbd1b84481a47e46ce510715a01c5e7b616c3fd908ef544`.
[composition-artifact-02.json](composition-artifact-02.json) records actual size,
features, HEAD/dirty metadata and source proof. The executable was inspected, never
launched. `otool -L` contains no WebKit; the normal Cargo tree has no Tauri/Wry/Tao.
Metadata/tree/linkage raw logs are preserved. Existing core warnings and unused
chat action/scroll fields remain visible; they were not silenced for this gate.

## API and case evidence

[composition-contracts.md](composition-contracts.md) records actual App/Launcher,
source-stamped effects, persistent event subscriptions, host image handling,
shared SettingsStore/footer, input/focus and quit/retry APIs.
[composition-case-map.json](composition-case-map.json) maps demonstrated assertion
subsets to ledger rows; no full ledger row is declared closed.
[../native-m09/asset-parent-api.md](../native-m09/asset-parent-api.md) and
[asset-parent-final.json](../native-m09/asset-parent-final.json) record the shared
asset API, limits, exact registry scope shape and file hashes.

All27 first-gate native test identifiers still occur as passing in both full final
native logs. Original `ai_chat/tests.rs` and `tests/core_loopback.rs` are byte-
identical to parent HEAD. The old eight-file first-gate snapshot remains historical;
four production chat files now have authorized integration changes, with their new
hashes in the final freeze. Original first-gate documents, source map and logs are
unchanged. [composition-audit-02.json](composition-audit-02.json) records the audit.

Actual new fixtures exercise retained Launcher/ChatView/CoreEvents/SettingsStore,
actual App dispatch/Settings/quit, GPUI TestPlatform path prompts and native input
key-down/up/composition, temp canonical core storage and real asset decoders.
Provider execution, clipboard payload acquisition and auxiliary Settings/AI reads
are synthetic. Existing core transport tests use loopback providers only.
No GUI, real clipboard, permissions flow, shipping profile or live provider/MCP was
used. See the early Keychain-path disclosure below before interpreting this claim.

## Failed development attempts retained

Every failed log remains in `composition-logs/`. Earlier draft formatting logs
are historical. In particular:

- `check-01.log`: three own drain result type errors; corrected.
- `composition-01..06.log`: prematurely dropped fixture Relay receiver, nonexistent
  fixture cwd, a private test field access, publishing inside an already borrowed
  window, and missing host notification for Background effects. The production
  emit path now notifies promptly; actual payload fixtures publish outside a
  borrowed Window. Later composition07/09 and the final full logs pass.
- `composition-08.log`: fixture Row::command prefixed the supplied chat ID. The
  fixture now uses the real history ID. Footer testing also required the production
  receive-navigation nonce floor so a later local resume never reused a screen nonce.
- `app-01..06.log`: fixture WindowHandle result/lifetime/private Lifecycle setup,
  production GpuiHost requiring a GUI main thread, fixture bookkeeping in the
  mailbox, raw Shift Enter not inserting a newline and focus staying on a removed
  approval button. A test-only disabled-desktop constructor and actual input/focus
  fixes resolve them. App07 and the final full logs pass.
- `assets-01.log`: decoder upscaled a1px original; now native dimensions cap the
  thumbnail. A draft assertion wrongly expected a16385px chat preview despite the
  documented16384 ceiling; it now asserts rejection above the limit and retains
  the positive preview assertion at the valid boundary. No limit was loosened.
- `core-assets-01.log`: draft legacy grant used macOS `/var` alias instead of an
  exact canonical `/private/var` file. Fixture grants now use the registry's exact
  canonical shape; production policy was not loosened.
- `root-ancestor-probe-01.log`: a real own race defect. Swapping an ancestor of the
  issued root after authorization redirected the old scoped open. Final code
  traverses root ancestors and file components from `/` with openat/O_NOFOLLOW.
  The same refusal assertion passes in both final core10-test runs.

The first development check materialized the authorized already-locked resvg
edge offline without `--locked`; every final gate is locked/offline. Manifest delta
is explicit app image features plus exact resvg0.46.0. Cargo.lock adds only the
app->resvg edge, with no package version/checksum/license/toolchain drift.

**Keychain disclosure:** early development composition fixtures forwarded
`ai-extensions:list` through normal NativeCore. Its macOS secret-availability path
can call the real Keychain API, even with isolated paths/TestHost. No secret
appeared in tool output, but this run cannot claim that Keychain was never touched.
This was identified and disclosed during work. The final frozen Backend scripts
auxiliary Settings/AI channels; config and canonical chat storage still use temp
core storage. All final fixtures use that corrected boundary.

## Remaining limits and next ownership window

Independent fresh Opus review, parent acceptance/commit, Windows build/race
validation, physical IME/AX/VoiceOver/Narrator, manual desktop/GPU/RAM and full
migration acceptance remain open. HTTPS policy/stream/drop tests are synthetic,
not a real remote HTTPS-server acceptance claim. Blocking read/decoder work is
not preemptible; it retains its admission slot until completion. Cache/allocation
bounds are not measured process/GPU memory results. No live provider/MCP acceptance.

After all final gates and hash checks completed, parent/M10 was released ONLY for
core terminal/pty.rs, one new internal stream module declared from pty.rs, and
terminal_bridge.rs. Subsequent edits are outside this M8 freeze/receipt. M8 can own
the two exact alacritty_terminal0.26.0 manifest edges, finite lock delta and exports
in a separately released next window; none is added here. M9's assets.rs remains
ONLY the shared public re-export. No other peer files are edited or bundled.
