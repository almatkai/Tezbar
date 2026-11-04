# Native AI implementation verification

## Four targeted-review variants — source `756072e`

Validated implementation: `756072ee682eae773626b3d1b78d807217014580`, tree
`0147dc5caa1e3dfbe94ab2214c5f7e8b808994a0`, in the same clean task worktree
and branch `feat/native-rust-agent`. The following evidence/ledger update is a
documentation-only successor to that tested source. No implementation edits
occurred during its final checks.

The completed full Opus reviews at `1623d47` and targeted reviews at `847ea62`
remain unchanged under `docs/reviews`. Wire closed with no actionable findings;
the four remaining variants (T-R1, S8-R1, S8-R2, S5-V1) now have implementation
repairs and permanent regressions. **Fresh Opus closure of those four repairs is
still pending.** No overall migration or release sign-off is claimed here.
The wire/protocol/provider/agent code reviewed at `847ea62` is byte-identical;
the changes are confined to extension lifecycle/secrets and Settings selection.

| Required check | Final observed result |
| --- | --- |
| `pnpm typecheck` | Pass |
| `pnpm test` | 928 passed, 9 existing skips, 0 failed; 125 files passed, 2 skipped |
| `pnpm build:frontend` | Pass |
| macOS Rust 1.98 full library suite, locked, two test threads | 608 passed, 9 existing ignores, 0 failed; 155.29 s |
| Rust 1.85 offline locked library check | Pass |
| Backend lifecycle fixture | 1 passed, 0 failed |
| Windows Rust 1.90 non-test library check | Pass |
| Windows full locked library suite, two test threads | 538 passed, 8 existing ignores, 0 failed; 102.53 s |
| `git diff --check` | Pass |

The two new real-component selection regressions and all 40 extension Rust tests
also passed before the complete suites. The Windows DPAPI fixture now exercises
a second permanent key failure with no ciphertext, an explicit reset and another
encrypted roundtrip. Access-denied/unavailable states remain refused with and
without ciphertext. The catalog-stop regression waits for a real fixture request
before Remove/shutdown and checks completion under one second, no successful
adoption and empty client/start registries. No skips or ignores were added.

### Exact-source Windows staging

Authorized Tailscale SSH: `User@100.77.254.64`, `DESKTOP-OB83S8J`. Isolated stage:
`C:\Users\User\AppData\Local\Temp\tezbar-native-c7271bc-20261104\rev-756072e-closure4`.
The release checkout remains clean on `release/0.3.0-beta.7` and was not edited.
The cached immutable `8f70d2e` archive plus a committed 34-file overlay were used,
with no deleted paths. Windows verified **all 1,784 tracked files** against the
final commit before building and **1,778 non-generated tracked inputs** afterward;
only the six regenerated `dist-backend/` files are excluded after the build.
This is full-file snapshot verification, not a newly transferred full archive.

| Artifact | SHA-256 |
| --- | --- |
| `overlay-756072e.tar.gz` | `06e859fd46faae52f0f7b491c08f555ef91d50f89f28283f087e7baeaacabc28` |
| `manifest-756072e.json` | `176aca5c2746193d8f14e33008abf29fc5652a04bbac9bd9b1bd14afdc5f9018` |

The local manifest and overlay are in `/private/tmp/tezbar-four-stage-756072e`.
Recipe: `/private/tmp/tezbar-four-windows-756072e.ps1`. The script was copied and
executed with `powershell.exe -File` after the encoded SSH command exceeded the
transport command-line limit; no source or validation was changed. It uses frozen
offline dependencies, backend resource staging and the same full native commands
as the earlier verified recipes. Execution-policy bypass applies to that process
only; no persistent Windows policy or profile was changed.

### Retained final logs

- `/private/tmp/tezbar-four-typecheck.log`
- `/private/tmp/tezbar-four-final-ts.log`
- `/private/tmp/tezbar-four-final-frontend.log`
- `/private/tmp/tezbar-four-final-macos.log`
- `/private/tmp/tezbar-four-final-msrv.log`
- `/private/tmp/tezbar-four-final-lifecycle.log`
- `/private/tmp/tezbar-four-final-windows.log`
- `/private/tmp/tezbar-four-windows-rust-check.log`
- `/private/tmp/tezbar-four-windows-rust-full.log`

Tests used fixture profiles, keys, owned processes and loopback providers only.
No production credentials, live Tezbar provider calls, app install/launch,
Actions, push, PR or release were used. Live upstream interoperability and actual
desktop window/event delivery remain untested; the previous documented Windows
OCR/semantic UI Automation limits are unchanged. The source-reviewed window
ordering and five-wire closure from the completed Opus audits remain applicable.

## Historical: eighteen closure-audit repairs — source `21d3e91`

Validated implementation: `21d3e911a03bbea64ffb39324d6822ca343f9ad7`, tree
`ef522242f9f06e69ae7b501b0dae2c8727d32461`, on existing
`feat/native-rust-agent` in `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`.
Started clean at `31bc3e29879e54c24725da2bc77a1454d406c168`, the docs-only
successor of reviewed `1623d47`. Authorized base remains `e246752149dbbede5e44720a0e35a3eacf2d66a9`
(`v0.3.0-beta.7`); no branch, worktree, origin/main or dev transition was made.
This verification/ledger follow-up changes **documentation only after that tested
source**. All code and build inputs remain byte-identical to `21d3e91`.

All eighteen N-W1–N-W5, S1–S10 and C-R1–C-R3 findings have implementation repairs
and permanent regression evidence in the [repair ledger](reviews/NATIVE_AI_REVIEW_FIXES.md).
**Fresh targeted Claude Opus 5.5 review of all eighteen repairs and touched-path
regressions remains required with the parent. This is not overall migration
closure, reviewer sign-off or release approval.** The three completed
`native-ai-opus-{wire,standards,runtime}-1623d47.md` reports are unchanged
(`git diff --quiet 31bc3e2..21d3e91 --` those three reports succeeds).

Implementation commits: `5dddcb4` (eighteen repairs), `c4b8888` (destroy even if
launcher restoration fails), `96a55cf` (unique revocation archives/rollback),
`74c13ed` (shared Settings ordering and deterministic Windows fixtures),
`21d3e91` (cache unchanged redaction regexes to preserve the bounded stderr tail).
No delegation, push, PR, release, Actions, app installation/launch, production
profile/key read, paid model call or third-party MCP service was used. Tests used
fixture profiles/keys, loopback servers and owned processes. One bounded XERJ
attempt returned an irrelevant hit; subsequent searches used rg/direct paths,
without index repair.

### macOS Apple Silicon — 2026-11-04/05

All checks below cover final implementation `21d3e91`. A full-file SHA-256
manifest comparison also verified all 1,781 tracked worktree files against that
committed archive after the checks; no source edits occurred during the final
runs. Tests were serialized with respect to native builds.

| Required check | Final observed result |
| --- | --- |
| `pnpm typecheck` | Pass |
| `pnpm test` | 926 passed, 9 existing skips, 0 failed; 125 files passed, 2 skipped |
| `pnpm build:frontend` | Pass |
| `CARGO_BUILD_JOBS=2 cargo +1.98.0 test --manifest-path src-tauri/Cargo.toml --lib --locked -- --test-threads=2` | 606 passed, 9 existing ignores, 0 failed; 154.49 s |
| `CARGO_BUILD_JOBS=2 cargo +1.85.0 check --manifest-path src-tauri/Cargo.toml --lib --locked --offline` | Pass |
| `RUSTUP_TOOLCHAIN=1.98.0 CARGO_BUILD_JOBS=2 node scripts/check-backend-lifecycle.mjs` | 1 passed, 0 failed |
| `git diff --check` | Pass |

Final local logs:

- `/private/tmp/tezbar-18-final-typecheck.log`
- `/private/tmp/tezbar-18-final-ts.log`
- `/private/tmp/tezbar-18-final-frontend.log`
- `/private/tmp/tezbar-18-final-macos.log`
- `/private/tmp/tezbar-18-final-msrv.log`
- `/private/tmp/tezbar-18-final-lifecycle.log`

### Windows x64 — exact committed files, 2026-11-05

Authorized SSH host `User@100.77.254.64` (`DESKTOP-OB83S8J`), Rust 1.90.0,
`CARGO_BUILD_JOBS=2`, `--test-threads=2`. Unique stage:
`C:\Users\User\AppData\Local\Temp\tezbar-native-c7271bc-20261104\rev-21d3e91-closure18`.
The release checkout `C:\Users\User\Desktop\Tezbar` was not edited or used for
these builds; no Windows git worktree was created.

Staging used the verified cached immutable `8f70d2e` archive plus a `git archive`
overlay of all 31 changed tracked files through `21d3e91` (zero deleted paths).
**This was full final-file manifest verification, not transfer or hashing of a
new full source archive.** Before building, Windows verified every one of the
1,781 expected tracked files against the final committed snapshot and refused
unexpected files. After building, all 1,775 non-generated tracked inputs were
rechecked. `pnpm build:backend` regenerates six tracked `dist-backend/` paths,
so post-build verification excludes that generated output directory. The full
pre-build check includes it.

| Item | SHA-256 / identity |
| --- | --- |
| Cached `source-8f70d2e.tar.gz` | `cb6c63ca3f2440ca896ce5454d333b6043bdd7411028a2a40e2a26ae89749f0a` |
| `overlay-21d3e91.tar.gz` | `92700ed97cc1dacdf10050172941270ec3aec753ed4aa726ad73c09b7ddce414` |
| `manifest-21d3e91.json` | `e0b472a777a05aec9065e9d0378134f4c8ebc33c6b55dc773d8965add56b0270` |
| Manifest commit | `21d3e911a03bbea64ffb39324d6822ca343f9ad7` |
| Manifest tree | `ef522242f9f06e69ae7b501b0dae2c8727d32461` |

The stage installed dependencies with frozen/offline pnpm and `--ignore-scripts`,
built backend resources, then ran non-test `cargo check --manifest-path
src-tauri/Cargo.toml --lib --locked` and the full corresponding `cargo test`
command. Both passed. Full Windows suite: **536 passed, 8 existing ignores,
0 failed, 102.66 s, exit 0**. The two focused startup-tail regressions also passed
independently before the full run.

The full run includes actual DPAPI wrong-entropy decryption failure and explicit
archive/reset; a sharing-denied key replacement that preserves key, ciphertext
and revocations; compiled exact argv recording through relative exe/cmd/PATHEXT
paths; trailing backslashes, spaces, empty/following arguments; and the gated
in-flight MCP call, independent Test probe and retired-client disable cases.
Windows owned Job tests remain enabled. No new skips or ignores were added.

Retained local Windows evidence:

- `/private/tmp/tezbar-18-21d3e91-windows.log` — manifest identity and full run result
- `/private/tmp/tezbar-18-post-build.log` — post-build non-generated input verification
- `/private/tmp/tezbar-18-windows-21d3e91-logs/rust-check.log`
- `/private/tmp/tezbar-18-windows-21d3e91-logs/rust-full.log`
- `/private/tmp/tezbar-18-windows-21d3e91-logs/startup-focused.log`
- `/private/tmp/tezbar-18-windows-21d3e91-logs/failed-start-focused.log`

### Failures investigated and remaining limits

Earlier Windows runs exposed invalid-parameter (87) from a deliberately truncated
DPAPI blob; that code correctly remains refused. The permanent-failure fixture
now protects a structurally valid blob with wrong entropy and exercises actual
CryptUnprotectData through the production key loader. A timing-based slow-call
fixture was replaced with an explicit owned release gate. More significantly,
Windows proved that compiling six scrub regexes per line consumed the stderr
drain grace and lost the traceback tail; caching the identical patterns repaired
that production path. Both modern/cached-era startup regressions now pass on
Windows. Early local test assertion/harness errors were corrected, and two
launcher recovery deadline failures occurred in overlapping exploratory runs;
the final serialized complete suite passes without changing those tests or
quarantining bugs. Only final-source results above are acceptance evidence.

No desktop window was launched. C-R1 hide/restore/destroy ordering and C-R2's
native unacknowledged-close fallback are source-verified against pinned Tauri
2.11.2; renderer tests exercise the real bridge, acknowledgements, long writes,
failed saves, default consumer, Permissions and remounted save queues, not OS
window/event delivery. Actual NAT64/intranet and upstream provider acceptance
remain untested; resolver-policy and loopback fixtures establish Tezbar's side.
Windows OCR remains unsupported as documented. No new provider/model fallback
was added. The parent's fresh Opus review remains the next required gate.

## Historical verification — `8f70d2e` / `ae2fe00`

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
