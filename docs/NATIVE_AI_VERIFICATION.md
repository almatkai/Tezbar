# Native AI implementation verification

Source under test: `42e2ccbd797d08ec8cd2f58967c54ae10ffa56ae` on
`feat/native-rust-agent`, in the existing `Tezbar-native-rust-agent` tree.
Subsequent documentation-only commits do not change the tested implementation.
No push, PR, release, app installation or production-profile launch was performed.
The coordinator's mandatory Claude Opus 5.5 review has not yet run.

## macOS

Apple Silicon, local isolated fixtures, 2026-11-04:

| Check | Observed result |
| --- | --- |
| `pnpm typecheck` | Pass, both TypeScript projects |
| `pnpm test` | 896 passed, 9 skipped; 124 files passed, 2 skipped |
| `pnpm build:frontend` | Pass |
| `CARGO_BUILD_JOBS=2 cargo +1.98.0 test --manifest-path src-tauri/Cargo.toml --lib --locked` | 539 passed, 9 ignored, 0 failed; 54.71 s |
| `CARGO_BUILD_JOBS=2 cargo +1.85.0 check --manifest-path src-tauri/Cargo.toml --lib --locked --offline` | Pass; declared MSRV unchanged |
| `RUSTUP_TOOLCHAIN=1.98.0 CARGO_BUILD_JOBS=2 node scripts/check-backend-lifecycle.mjs` | 1 passed, 0 failed |
| `git diff --check` | Pass |

Logs retained locally for the coordinator: `/tmp/tezbar-native-final-typecheck-4.log`,
`/tmp/tezbar-native-final-ts-3.log`, `/tmp/tezbar-native-final-frontend-3.log`,
`/tmp/tezbar-native-final-macos-rust-5.log`, `/tmp/tezbar-native-final-msrv-3.log`
and `/tmp/tezbar-native-final-lifecycle-3.log`.

## Windows x64

Host: `DESKTOP-OB83S8J`, `User@100.77.254.64` over Tailscale. Toolchain:
`1.90.0-x86_64-pc-windows-msvc`. The release checkout was never edited.

The immutable source archive was produced with `git archive` from the source
commit above. SHA-256:

```text
d8e6477efffaecfece3e7c96f80d6203c1866d8e4c6b8f7482136ba50f368e0a
```

Staging:
`C:\Users\User\AppData\Local\Temp\tezbar-native-c7271bc-20261104\rev-42e2ccb`.
Task target cache is its sibling `rust-target`. Commands use the explicit Rust
1.90 toolchain path, `CARGO_BUILD_JOBS=2`, and `cmd.exe /d /c` for pnpm with log
redirection inside cmd. pnpm dependencies were installed frozen/offline into
staging. No toolchain, application or runtime installation was changed.

| Check | Observed result |
| --- | --- |
| Frozen/offline pnpm dependency staging | Pass |
| `pnpm build:backend` | Pass; pinned backend resources staged from the same archive |
| `cargo check --manifest-path src-tauri/Cargo.toml --lib --locked` | Pass, non-test Windows library |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib --locked -- --test-threads=2` | 468 passed, 8 ignored, 0 failed; 97.56 s |
| Release checkout status afterward | Clean on `release/0.3.0-beta.7` |

The successful full suite includes
`process::windows_tests::powershell_unicode_and_job_owned_tree` (Unicode output,
real root/grandchild termination within two seconds),
`ext::secrets::windows_dpapi_key_roundtrip_uses_only_an_isolated_temp_profile`,
compiled MCP stdio modern/legacy calls, crash/flood/cancellation/shutdown,
HTTP/OAuth fixtures, canonical provider dispatch and native file/path tests.
Windows-only code was exercised on Windows, not inferred from macOS compilation.

Staging retains `install.log`, `backend.log`, `rust-check.log` and `rust-full.log`.
The coordinator's local captured result is `/tmp/tezbar-native-windows-7.log`;
the isolated launcher recheck is `/tmp/tezbar-native-win-launcher-recheck.log`.

Provenance: `7188ec2` passed Windows compilation and 464 tests (8 ignored).
A default-concurrency run of `df2f37d` passed 467 tests and failed the existing
`timed_out_launcher_query_settles_and_recovers` fixture at its recovery-event
wait. That fixture configures an 80 ms RPC deadline and passed alone on the same
unchanged source (1 passed, 6.73 s). The final full-suite run uses two test threads
to avoid this fixture's load sensitivity; no unrelated launcher production or
test code was changed. Final-source results above take precedence.

## Fixture coverage map

These are inspectable tests, not live-provider compatibility or performance claims.
All AI fixtures use temporary home/data/config paths and fixture credentials.
NoPi markers and source guards reject owned Pi/Node/npm/Bun/OpenCode execution;
optional read-only `opencode models` is the catalog exception.

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

The required full Claude Opus 5.5 code/design review and repairs belong to the
coordinator after this implementation handoff. This document does not mark that
review, live-provider interoperability or release approval complete.
