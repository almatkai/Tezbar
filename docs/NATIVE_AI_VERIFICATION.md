# Native AI implementation verification

Native validation source: `8f70d2e7cf7beac2129155402587b82fded9c35b` on
`feat/native-rust-agent`, in the existing
`/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent` worktree.
Final renderer validation source: `ae2fe004e61538a33d6b661b4c4712d186a14640`.
Its only code changes after the native validation source are in three renderer
files; the Rust code and backend lifecycle script are byte-identical, verified
with `git diff --quiet 8f70d2e..ae2fe00 -- src-tauri scripts/check-backend-lifecycle.mjs`.
The Windows archive below proves the native checks for that unchanged Rust code;
it is not an archive of the later renderer commit. This documentation update is
a **docs-only follow-up to both tested sources**. Starting point was clean
`5271fd7`, whose tested implementation ancestor was `a28c564`. This repair addresses
all six N-R1–N-R6 findings in the unchanged full runtime report.

No push, PR, merge, release, Actions run, desktop application launch/install,
production credential/profile access or paid/live model call was performed.
All local and Windows runtime tests used isolated fixture profiles and keys.
**Fresh Claude Opus 5.5 closure remains pending with the parent**, including a
full runtime review. This is implementation verification, not release approval.

## macOS Apple Silicon — final source

2026-11-04. Native checks use `8f70d2e`; final renderer checks use `ae2fe00`.

| Check | Observed result |
| --- | --- |
| `pnpm typecheck` | Pass, renderer and main TypeScript projects |
| `pnpm test` | 918 passed, 9 skipped; 124 files passed, 2 skipped; 0 failed |
| `pnpm build:frontend` | Pass |
| `CARGO_BUILD_JOBS=2 cargo +1.98.0 test --manifest-path src-tauri/Cargo.toml --lib --locked -- --test-threads=2` | 598 passed, 9 ignored, 0 failed; 158.12 s |
| `CARGO_BUILD_JOBS=2 cargo +1.85.0 check --manifest-path src-tauri/Cargo.toml --lib --locked --offline` | Pass; declared MSRV unchanged |
| `RUSTUP_TOOLCHAIN=1.98.0 CARGO_BUILD_JOBS=2 node scripts/check-backend-lifecycle.mjs` | 1 passed, 0 failed |
| `git diff --check` | Pass |

Final logs retained locally:

- `/private/tmp/tezbar-parent-final-typecheck.log`
- `/private/tmp/tezbar-parent-final-ts.log`
- `/private/tmp/tezbar-parent-final-frontend.log`
- `/private/tmp/tezbar-nr-macos-final.log`
- `/private/tmp/tezbar-nr-msrv-final.log`
- `/private/tmp/tezbar-nr-lifecycle-final.log`

Early focused runs exposed invalid placeholder image bytes in the new OCR fixture
and test-harness omissions. They were corrected to use a valid canonical PNG and
the real bridge close adapter. The final full suites include the corrected
regressions; earlier logs are not substituted for final-source evidence.

The coordinator's renderer follow-up covers closing the native Settings window
while Permissions is displayed. Navigation now drains pending AI saves first,
and retains the editor and visible error if a save fails. Once Permissions is
open, the window owner acknowledges native close requests through the same real
bridge. Two real-App/bridge regressions cover successful and failed saves; the
full final renderer suite, typecheck and frontend build passed afterward. A first
new test exposed that the Permissions tab did not display a save error, which was
fixed before the final passing run. No native source changed in this follow-up.

## Windows x64 — exact committed archive

Host `DESKTOP-OB83S8J`, `User@100.77.254.64`, authorized Tailscale SSH fallback.
Toolchain `1.90.0-x86_64-pc-windows-msvc`, `CARGO_BUILD_JOBS=2`.
The release checkout at `C:\Users\User\Desktop\Tezbar` was never changed;
its final status remains clean on `release/0.3.0-beta.7`.

Created with `git archive --format=tar.gz` from native source `8f70d2e`, then
copied to the task temp root. SHA-256 was checked on Windows before extraction:

```text
cb6c63ca3f2440ca896ce5454d333b6043bdd7411028a2a40e2a26ae89749f0a
```

Local archive: `/private/tmp/source-8f70d2e.tar.gz`.
Staging: `C:\Users\User\AppData\Local\Temp\tezbar-native-c7271bc-20261104\rev-8f70d2e`.
Only the sibling `rust-target` task cache was shared. Native output was redirected
inside `cmd.exe /d /c` to avoid PowerShell native-stderr false failures.

| Check | Observed result |
| --- | --- |
| Archive SHA-256 verification | Pass, exact hash above |
| Frozen/offline pnpm dependencies, `--ignore-scripts` | Pass in isolated staging |
| `pnpm build:backend` | Pass, backend resources staged from the same archive |
| `cargo check --manifest-path src-tauri/Cargo.toml --lib --locked` | Pass, non-test Windows library |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib --locked -- --test-threads=2` | 527 passed, 8 ignored, 0 failed; 107.49 s; exit 0 |
| Release checkout status afterward | Clean, read-only status check |

The real Windows run includes the N-R1/N-R3/N-R4/N-R5 native regressions,
including actual model-body shell summaries and fixture OCR through the parent
route. N-R2/N-R6 renderer seams are covered by the TypeScript suite above. It also
includes the corrected npm layout `npx` + `npx.cmd` test,
Windows DPAPI temporary-profile roundtrip, process tree/Unicode tests, all 13 wire
re-review regressions, private-host DNS pinning, key-loss reset and cached-era
startup crash fixtures. Windows behavior was exercised on Windows.

Local captured result: `/private/tmp/tezbar-nr-windows-final.log`.
Full copied native log: `/private/tmp/tezbar-nr-8f70d2e-windows-rust-full.log`.
Recipe: `/private/tmp/tezbar-native-ai-8f70d2e-windows.ps1`, run through the
previously inspected `/private/tmp/tezbar-native-remote.py` helper. Staging retains
`install.log`, `backend.log`, `rust-check.log` and `rust-full.log`.

## Ignored/skipped scope

No ignores/skips were added for this repair. The 9 Mac Rust ignores cover live
GitHub installer discovery, the optional built Bun extension-worker roundtrip,
scaled/release-only search/cache/allocation probes and the headless idle benchmark.
Windows has the corresponding 8 applicable ignores. The 9 Vitest skips remain
unchanged. These checks do not claim live provider interoperability, official-SDK
MCP server interop, desktop visual QA, installer/signing readiness or performance
acceptance. No built-in AI regression is waived by the counts above.

## Fixture coverage map

These are inspectable tests, not live-provider compatibility or performance claims.
All AI fixtures use temporary home/data/config paths and fixture credentials.
NoPi markers and source guards reject owned Pi/Node/npm/Bun/OpenCode execution;
model catalogs use native HTTP plus static metadata; no OpenCode CLI is started.

| Area from the specification | Main evidence |
| --- | --- |
| Built-in providers, five protocols, two tools, user/tool images, continuation | `canonical_tests::every_provider_and_protocol_executes_through_agent_dispatch`; each provider and Copilot/OpenCode protocol route uses real `agent:run` dispatch |
| Signatures, encrypted reasoning, DeepSeek fields, Ollama native fields, UTF-8 framing | `wire::tests` byte-at-a-time decoding and exact-origin replay checks; Gemini unsigned-history and Anthropic unfinished-block tests |
| Durable resume, model switch, image blobs, crash repair | `canonical_tests::durable_signed_history_resumes_by_id_and_switches_models_without_foreign_opaque`, `a_crashed_saved_call_gets_an_unknown_result_and_is_never_reexecuted`, store tests |
| Query/chat/action/metadata/extension callers and aliases | `ai::tests`, `native_tests`, frozen IPC inventory and native profile lifecycle fixture |
| Retry/no replay, deadlines, malformed batches, cancellation | `native_tests::completed_effect_stays_once_across_byte_identical_turn_retry`, `native_next_header_deadline_never_replays_completed_effects`, malformed/edit-batch, header/body/approval/cancel fixtures |
| Permissions, sensitive paths, outside-cwd modes, live Review | `runtime::policy::tests`, `native_tests::native_permission_mode_changes_apply_to_later_tools_in_the_same_run`, remembered/destructive/complex and concurrent approval fixtures |
| Native authentication and catalogs | `providers::tests`: serialized Copilot refresh, session exchange, credential-scoped catalog routing, device cancel/stale/success, Antigravity store preservation/quota/endpoint failover, native search/images and OpenCode auth/catalog; `oauth::tests` PKCE/state/issuer/cancel |
| MCP stdio/HTTP, modern/legacy, trust, calls and secrets | `ext::tests`: compiled stdio fixture plus loopback HTTP/AS fixtures, DCR/PKCE/resource/refresh, session recovery, descriptors/schema, resource/template/prompt images, notifications, concurrent cancellation/disable and crash/flood no-replay checks |
| Encryption and provider secret editing | `ext::secrets::tests`, provider-secret reference/namespace test, renderer password/save checks, Windows DPAPI temp-profile fixture |
| Project imports, instructions, prompt drafts and profiles | Project marker stays absent before review; byte changes revoke trust; contribution ordering/scoping tests; renderer import preview, slash argument and first-turn profile tests |
| Renderer design behavior in both themes | `AiExtensionsSection.test.ts`, `ComposerExtensions.test.ts`, `agentChat/ownership.test.ts`: Test before Enable, tool review, saved password inputs, import without auto-start/send, keyboard arguments and approvals |
| Windows process/file paths | `process::windows_tests::powershell_unicode_and_job_owned_tree`, native file/approval tests, extended-path policy check, MCP compiled stdio and native extension process tests |
| macOS process cancellation | `native_tests::native_cancel_kills_shell_descendants_and_prevents_late_mutation`, process-group fixtures |

The old shell regex bypass cases are deliberately stricter where the coordinator
required sensitive, destructive and unknown/complex syntax to precede remembered
or Turbo shortcuts. Those old permissive expectations are not compatibility
requirements.

## External checks and remaining acceptance step

No live paid AI request, real provider credentials, production profile, desktop
smoke, official-SDK server interoperability session, installer/signing build, or
RAM/latency benchmark was performed. Fixture servers establish Tezbar's side of
wire formats and effect ownership; they cannot establish current provider access,
account entitlements, quota or undocumented upstream behavior. OpenCode's upstream
free-tier restrictions remain respected and actionable. Windows OCR and semantic
UI Automation are separate pre-existing platform gaps.

Initial Opus reviews, the 118cb7b re-review reports and the full runtime
a28c564 report remain unchanged under `docs/reviews`. All N-R1–N-R6, W1–W15,
N1–N8 and U1–U3 repairs are recorded with regressions
in `docs/reviews/NATIVE_AI_REVIEW_FIXES.md`. The coordinator must obtain fresh
Claude Opus 5.5 wire, standards and **full runtime** closure against the repaired
source. Passing fixtures and the repair ledger do not replace that gate. This
document does not mark review closure, live-provider interop or release approval
complete. Older private-resource OAuth grants without trust-zone provenance may
require one new sign-in; locked/denied OS key-store access never enables reset.
