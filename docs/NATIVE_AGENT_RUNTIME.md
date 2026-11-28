# Native AI runtime

Built-in AI execution is owned by Rust. Pi, Node, npm, Bun and `opencode run`
are not execution fallbacks. React remains the renderer; model servers, speech/
ML engines, OS helpers and explicitly connected MCP programs remain external.
The implementation follows [NATIVE_AI_SPEC.md](NATIVE_AI_SPEC.md). Review reports,
repair dispositions, current closure status and platform evidence are recorded in
[NATIVE_AI_VERIFICATION.md](NATIVE_AI_VERIFICATION.md). This branch has not been
released.

## Entry points and providers

`agent:run`, `chat:run`, query answers, actions, chat metadata and extension AI
all use the same native provider seam. `provider.rs` handles tool-free callers;
`runtime/agent.rs` owns the agent loop. `wire/` contains five terminal-validated
adapters: OpenAI Chat, Responses, Anthropic Messages, Gemini/Cloud Code and
Ollama. Protocol adapters perform no network I/O. `wire/transport.rs` owns HTTP,
stream deadlines, redaction and retries.

Supported built-in routes are OpenAI, OpenAI-compatible/custom providers,
Anthropic, Ollama, Copilot, Gemini, DeepSeek, TokenRouter, Antigravity and
OpenCode. Configured endpoints and explicit provider/model protocols take
precedence. Unknown OpenCode models need an explicit protocol, rather than a
model-name guess. Custom providers may select any of the five protocols and use
encrypted header references when supplied by an AI extension.

- Copilot reuses native GitHub device/refresh credentials. Rejected direct tokens
  can be exchanged once for an in-memory session token. Model endpoint metadata
  and credential-scoped catalogs determine model routes. No account policy is
  changed.
- Gemini uses its native API on the default host; a custom compatibility base
  retains Chat unless explicitly configured otherwise. DeepSeek retains its
  required Chat reasoning fields.
- Antigravity account refresh, PKCE sign-in, selection/removal, catalog merging,
  Cloud Code envelopes, bounded account/endpoint failover, grounded search and
  image generation run natively. Existing account stores remain in place;
  unrelated auth entries and unknown fields survive updates. This integration
  uses an unofficial upstream API and is not endorsed by Google.
- OpenCode uses supported native HTTP APIs and key precedence: configured key,
  environment, then its existing auth file. The attributed static catalog and
  native discovery run without the OpenCode CLI; no OpenCode program is started.
  Current upstream free-tier restrictions produce an
  actionable credential/model error; Tezbar does not impersonate OpenCode.

Routing/request ports and the catalog snapshots are attributed to MIT
pi-antigravity 0.11.0 (Rahul Arya) and pi-ai (Mario Zechner), with license copies
in `licenses/`. Their JavaScript is read-only reference material, never loaded.

## Durable history and execution boundaries

`runtime/transcript.rs` stores canonical text, images, thinking, tool calls and
correlated results. Unique call IDs survive a bounded history window and repeated
provider wire IDs. Opaque signed/encrypted blocks replay only to their exact
provider, protocol and model. Foreign thinking is dropped. Gemini generations
requiring signatures receive observations for unsigned history; signed parts
remain separate.

Per-conversation turns are private atomic JSON files under
`data_dir/llm/transcripts`, with content-addressed image blobs. Renderer prior IDs
select the ordered history window; old text-only conversations still load.
Missing image bytes become an explicit placeholder. Interrupted unanswered calls
receive an unknown-outcome result. Deleting chats/turns removes their native
history and unreferenced blobs. Image storage is bounded to 128 MiB per
conversation.

A provider turn must reach its protocol terminal signal before any tool batch
runs. The complete batch is validated before its first effect. Retry is below
the agent loop, bounded to two retries and the current encoded request only,
before visible text. Safe retries reuse identical request bytes. A later timeout,
rate limit or disconnect cannot replay earlier tools. Partial visible text is
retained with an error marker. MCP tool effects are never automatically replayed.

The shared limits in `runtime/limits.rs` include 100 provider turns, 256 calls,
32 calls per turn, 8 MiB streamed bytes per turn, 1 MiB arguments, 64 KiB
model-facing result text, 4 MiB text context and 2 MiB editable files. Active run
time is 30 minutes, excluding approval waits. Stage output is at most 8,000
characters with its tool source. Old tool results may be elided under context
pressure; opaque blocks remain intact. Blocking file/schema operations run off
the async executor and schema validation has a bounded worker pool/deadline.

## Tools and permission policy

Native tools cover regular file read/write/edit (including atomic edit batches
and optional parent creation), bounded list/find/grep/project search, indexed
Knowledge and launcher search, shell execution, screen capture/images and macOS
OCR. Antigravity adds `google_search` and `generate_image` when linked.
Generated images stay in `cwd/generated-images`; paths are checked again before
writing. The tool registry reflects the platform: bash on macOS, PowerShell on
Windows with explicit UTF-8 output.

Review always asks. Per-request mode overrides stay pinned; otherwise the next
tool reads live settings. Exact remembered shell commands and tool descriptor
allowances are checked only after destructive, sensitive and unknown/complex
shell cases. Auto can mutate ordinary files inside cwd; Turbo can also operate
outside cwd. Sensitive files and destructive/complex shell syntax still ask.
Outside-cwd operations follow the explicit file policy in `runtime/policy.rs`.
Search traversal omits sensitive/dependency paths, and direct paths are
canonicalized and revalidated after approval. The native shell policy is
intentionally stricter than the old permissive TypeScript regex shortcuts.

Approvals are not an OS sandbox. Approved commands and extension programs run
with the person's OS permissions. POSIX process groups and Windows Job Objects
own descendants; Windows processes start suspended, join the job, then resume.
Cancellation fences process starts and final file mutations, kills owned process
trees, cancels pending approvals/calls, and ignores late progress/results.

## AI Extensions

`ext/` owns manifests, encrypted secrets, project trust, native MCP, resources,
prompts, instructions, profiles and declarative providers. Settings uses the
existing primitives and a single-column list. Add/import/paste does not run a
program. The person sees the literal command/arguments or URL, tests it, then
enables it. Project `.tezbar/ai-extension.json` files are review notices only;
location and raw bytes are pinned, and any byte change revokes trust.

The Rust MCP host supports stdio and Streamable HTTP, current `2026-07-28`
stateless metadata plus common legacy handshakes, bounded discovery and local-only
JSON Schema validation. OAuth supports protected-resource/authorization-server
discovery, preregistered clients or DCR, PKCE S256, issuer/resource checking and
refresh. No hosted CIMD document is invented. Unsupported server requests such
as roots, sampling and elicitation return an explicit error.

Each run has an immutable collision-safe name-to-connection/descriptor table.
Changed descriptors revoke remembered allowances and need review. Catalog
notifications refresh the next snapshot, never mutate a running one. Calls and
cancellation on shared connections are correlated independently; disabling a
connection cancels in-flight/startup work and refuses later calls. Idle clients
stop after ten minutes; failed startups/crashes have bounded backoff.

Only declared variables plus a small OS allowlist reach child processes. Secrets
use AES-GCM and a separate OS-protected key through the existing KeyStore seam
(Keychain on macOS, DPAPI on Windows); failure has no plaintext fallback. Tokens
are scrubbed from progress, discovery, results and errors and never returned in
account IPC. Exact-value scrubbing cannot identify arbitrary transformed secrets.
Tests use temporary profiles and an injected in-memory key store.

Global/extension/profile/project/request instructions have a defined bounded
order. `AGENTS.md` from cwd is optional and cannot override permissions. Prompts,
resource text/images and slash commands resolve to editable drafts, never send
automatically. Profiles narrow tools and persist with the conversation. See
[AI_EXTENSIONS.md](AI_EXTENSIONS.md) and the example manifest for authoring.

## Compatibility

Existing settings, credentials, conversations and database locations remain.
Legacy `agentRuntime` values are accepted/persisted but ignored for routing.
`pi:check`/`pi:install` report native availability and do not probe/install a CLI;
legacy observation aliases report native run state. Existing Pi JavaScript
packages are listed as not loaded, with MCP as the replacement seam. The
Raycast-compatible extension worker is a separate feature and remains unchanged.
The Pi policy resource is absent from packaging/build staging.

## Verification and limits

Final verification results are recorded in [NATIVE_AI_VERIFICATION.md](NATIVE_AI_VERIFICATION.md).
The suite uses real `AiCore::handle` dispatch, loopback HTTP/SSE/NDJSON providers,
compiled Rust MCP stdio fixtures, fake OAuth servers, temporary profiles and
NoPi marker executables. It covers every built-in provider/protocol, signed and
image-bearing multi-turn history, auth, permissions, retries, cancellation,
secret/trust boundaries and renderer interactions in both theme settings.

Commands:

```sh
pnpm typecheck
pnpm test
pnpm build:frontend
CARGO_BUILD_JOBS=2 cargo +1.98.0 test --manifest-path src-tauri/Cargo.toml --lib --locked
RUSTUP_TOOLCHAIN=1.98.0 CARGO_BUILD_JOBS=2 node scripts/check-backend-lifecycle.mjs
```

No live paid-provider calls, production profiles, desktop launches, app installs,
installer/release builds or performance benchmarks were used. Official-SDK server
interop and credentialed upstream behavior remain manual release checks; fixture
success establishes Tezbar's side of the protocols. Copilot/Antigravity internal
APIs and upstream OpenCode restrictions can change independently.

The declared MSRV remains 1.85. Added jsonschema 0.29.1 disables remote-resolution
features and declares Rust 1.71.1. The locked macOS library also passes `cargo +1.85.0 check --lib --locked
--offline`; tests run with 1.98 on macOS and 1.90 on Windows. The mandatory full Claude Opus 5.5 review remains with the coordinator.
