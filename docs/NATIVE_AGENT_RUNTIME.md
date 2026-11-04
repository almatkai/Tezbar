# Native AI runtime

Implementation is in progress against [NATIVE_AI_SPEC.md](NATIVE_AI_SPEC.md).
The full specification and its acceptance matrix are **not yet complete**. The
mandatory Claude Opus 5.5 review has not run. Do not treat this branch as ready
for release.

All owned agent and completion execution now uses Rust HTTP adapters. Pi, npm,
Node, Bun and `opencode run` are not execution fallbacks. React remains the
renderer. Optional external MCP programs and model servers remain external.
Legacy `agentRuntime` values are preserved in configuration and ignored when
routing. `pi:check` and `pi:install` report the native runtime without probing or
installing a CLI. Existing Pi packages are listed as not loaded.

## Implemented paths

- Canonical text, images, thinking, tool calls and correlated results, with opaque
  replay limited to the original provider, protocol and model.
- OpenAI Chat and Responses, Anthropic Messages, native Gemini/Cloud Code and
  Ollama encoders and terminal-validated streaming decoders. DeepSeek uses Chat.
- Native `agent:run`, `chat:run`, query answers, actions, metadata and extension AI.
- Current-turn retries before visible text, with one encoded request body and no
  replay of the agent loop. A completed tool remains recorded if a later turn fails.
- Private atomic conversation turn files, content-addressed image storage and a
  128 MiB image budget. Interrupted outstanding calls receive unknown-outcome
  results. Renderer history IDs and legacy text fallback remain compatible.
- Live Review/default/Auto/Turbo policy, remembered exact shell commands, sensitive
  file checks and conservative review for unknown shell syntax before shortcuts.
- Native file read/write/edit, directory listing/name/content search, indexed
  launcher/Knowledge routes, shell execution and screen capture/OCR.
- POSIX process groups and a shared Windows Job Object helper. Windows children
  start suspended and are attached to their job before being resumed.
- Explicit per-model and custom-provider protocols. OpenCode uses a documented
  HTTP route and an attributed catalog snapshot, with an actionable missing-key
  error rather than impersonating its CLI. Copilot can exchange a rejected GitHub
  token for an in-memory session token.
- Antigravity existing-account selection, refresh/store preservation, native
  Cloud Code envelopes and attributed model routing from pi-antigravity 0.11.0.
- AI extension imports, encrypted secrets, explicit Test then Enable, native stdio
  and HTTP JSON-RPC, modern/legacy initialization, bounded schema validation,
  discovery, pinned tool definitions and immutable per-run lookup tables.
- Instructions, local prompts/commands, profile tool narrowing and AGENTS.md.
  Settings has an inline single-column extension list; the composer can insert
  resolved prompt text without sending it.

## Permissions and limits

Approval prompts and process ownership are not an OS sandbox. Approved commands
and extension programs run with the person's OS permissions. Auto can mutate
files inside cwd; Turbo can also mutate files outside cwd. Sensitive file paths,
destructive commands and complex shell syntax still require review.

The shared limits are in `runtime/limits.rs`: 100 provider turns, 256 tool calls,
32 calls per turn, 8 MiB streamed bytes per turn, 1 MiB tool arguments, 64 KiB
model-facing result text, 4 MiB text context and 2 MiB editable files. The UI
receives at most 8,000 characters of stage output. Old tool results may be elided
under context pressure; opaque blocks are retained.

Connection secrets use AES-GCM and a separate OS-protected key. No plaintext
fallback is used. Tests inject an in-memory KeyStore and temporary profiles.
Scrubbing matches exact secret values and known patterns; transformed secrets
cannot be reliably detected.

## Outstanding implementation and verification

These are unfinished product requirements, not upstream restrictions:

- Native Antigravity PKCE/account-management UI, live catalog merging, account and
  endpoint failover, project discovery, image generation and Google search tools.
- Copilot authoritative model endpoint metadata, complete refresh/device/session
  fixture matrix, and native OpenCode catalog/key-source fixtures.
- MCP OAuth, complete modern-error/version negotiation, legacy session restart and
  stream resume, subscriptions/catalog refresh and idle restart/backoff.
- Project extension review/trust UI and byte-change revocation, resource templates,
  image-bearing prompt/resource insertion and all remaining result/schema limits.
- Full extension edit/variable/secret/policy/log/review UI, local prompt authoring,
  profile selection before the first conversation turn, both-theme accessibility
  verification and the complete renderer acceptance matrix.
- The complete shell-policy port, multiple-edit batches, exact original-path
  revalidation, Windows path normalization and the active-time budget excluding
  approval waits.
- The exhaustive NoPi marker, auth/secret, persistence, cancellation/concurrency and
  protocol-image acceptance matrix, real Windows results and the required review.

Fixture coverage proves the application's side of the exercised protocols. No
live paid-provider calls, production profiles, desktop launches or installations
are used for this work. Live provider compatibility and performance are unverified.

## Verification

Use `pnpm typecheck`, `pnpm test`, `pnpm build:frontend`,
`node scripts/check-backend-lifecycle.mjs` and:

```sh
CARGO_BUILD_JOBS=2 cargo +1.98.0 test --manifest-path src-tauri/Cargo.toml --lib --locked
```

The declared MSRV remains 1.85. The added jsonschema 0.29.1 uses no remote-resolution
features and declares Rust 1.71.1. Running the project suite on Rust 1.98 does not
establish that the existing locked dependency graph supports 1.85.
