# Native agent runtime: stage 1

Agent requests can now execute inside the existing Rust/Tauri process without Pi,
Node, global Pi extensions or a localhost tool gateway. This changes `agent:run`
(and agent intents from `query`); the old text-only `chat:run`, actions, voice and
extension AI requests keep their existing adapters. The AI SDK renderer already
builds a prompt containing the selected saved conversation; that complete prompt
is passed unchanged to the native model. Saved stages, canonical answers and
run-specific event/approval/cancel IPC remain unchanged.

## Selecting a runtime

Set `agentRuntime` through the existing `setLlmConfig` / `llm-config-set` interface:

```ts
await api.setLlmConfig({ agentRuntime: 'auto' }) // default when absent
await api.setLlmConfig({ agentRuntime: 'native' }) // unsupported provider => error
await api.setLlmConfig({ agentRuntime: 'pi' }) // explicit compatibility mode
```

No settings screen or profile migration is required. The field persists using the
existing configuration store. Release/version metadata is unchanged.

| Provider | `auto` route | Native limits |
| --- | --- | --- |
| `openai` | Native | API-key authenticated `/chat/completions`, text + function tools only |
| `openai-compatible`, `custom:*` | Native | Configured base URL, optional bearer key, same protocol |
| Anthropic, Ollama, Copilot, Gemini, DeepSeek, TokenRouter | Pi | Native protocol/auth/reasoning compatibility has not been established |
| Antigravity, OpenCode | Pi | Existing provider/extension behavior |

`openaiCompatibleBaseURL` takes precedence over `baseURL`, as in existing provider
configuration; `/chat/completions` is appended unless already present. Custom
servers must implement streamed Chat Completions function tools, IDs, string JSON
argument deltas, `finish_reason` and `[DONE]`. Responses API, legacy
`function_call`, multiple choices, reasoning/signature extensions and providers
that omit terminal markers are unsupported. Model compatibility is not inferred
from its name. Live providers/models have not been validated by these tests.

Stage 1 rejects image attachments before sending HTTP. It also rejects renderer
requests marked `historyHasImages`; older saved images are not silently converted
to text by the native runtime. Use Pi compatibility or a new text-only conversation.
Existing renderer conversation-window selection still applies; the native runtime
never silently truncates the supplied prompt or tool history.

## Tools and permissions

Tools are `bash`, UTF-8 `read`/`write`/`edit`, `project_search`, `launcher_search`,
`pc_search`, and `pc_read`. Search routes call `search:all` and the native Knowledge
search/read routes directly. Launcher results use the same metadata projection as
the Pi gateway: UI icons and image/base64 payloads are not sent to the model.

File paths must resolve inside canonical cwd, including symlinks. Outside-cwd file
tools are rejected in stage 1. Parent directories must already exist. Writes use
a sibling temporary file and atomic replacement; edits require exactly one
occurrence, counting overlapping matches. Existing permissions are retained.
Paths and edit ambiguity are rechecked after approval. These checks and prompts
are **not an OS sandbox**; approved shell commands run with the user's privileges.

Review mode prompts for every tool, including reads and searches. In default,
auto and turbo, only exact `pwd`, `whoami`, and `ls` commands run automatically;
they execute directly using fixed executable paths. Every other shell command
(including every chain, expansion, redirection, destructive or unknown command)
requires confirmation. Shell execution uses `/bin/bash --noprofile --norc`, removes
`BASH_ENV`/`ENV`, and requires a Unix host. File mutations always prompt in all
modes. This deliberately asks more often than Pi during migration. Native tools
ignore remembered exact/prefix command allowances; the existing "always" response
acts as "once" and does not persist file contents or new native command rules.
Pi's existing remembered-command behavior is retained.

An approval is tied to its live run and unique approval ID. A denied, cancelled,
expired (300 seconds), or stale approval cannot authorize an effect. Cancellation
is rechecked immediately before effects. Duplicate live run IDs are rejected;
concurrent runs have separate call histories, approvals and cancellation tokens.

## Protocol, failure and resource guarantees

Each turn streams visible text while buffering tool arguments. Both the matching
`finish_reason` and `[DONE]` are required before any tool is executable. The whole
tool batch is validated first: unknown tools, invalid arguments/paths, malformed
JSON, oversized calls, duplicate IDs and ambiguous edits reject the batch before
any approval or mutation. This is validation atomicity, not transaction rollback:
a runtime failure in a later valid tool does not undo earlier completed effects.

The assistant's ordered calls and exact call IDs are appended to history, followed
by correlated `role: tool` results. Tool failures and permission denials become
bounded error results for the model; cancellation stops the loop. Reused call IDs
are rejected across the entire run, even if their arguments match.

Native errors never enter Pi's whole-launch retry or invoke a compatibility
fallback. **Stage 1 makes no automatic HTTP retries**, including 429/503 responses:
HTTP status and a numeric Retry-After are reported without replay. A later model
failure cannot repeat a completed command or file mutation. HTTP error bodies and
unrelated subprocess warnings are not concatenated into native provider errors.
Network, HTTP, model, protocol, validation, tool and permission errors are labeled.

Headers have a 30-second deadline, stream silence a separate 60-second deadline,
each provider turn a 600-second maximum, and the run a 900-second maximum. Healthy
streams continue past the header deadline. Shell commands have a 60-second limit,
search routes 20 seconds. Cancellation drops HTTP requests and terminates shell
process groups using the existing descendant-process cleanup.

Limits: 24 provider turns, 64 total calls, 16 calls per turn, 256 KiB streamed
bytes per turn, 64 KiB argument JSON per call, 1 MiB serialized message context,
256 KiB per UTF-8 file, and 8,000 Unicode characters per tool result/stage. The
argument limit also limits practical write size. Output includes `outputTruncated`
and elapsed `durationMs`. Existing subprocess capture has hard limits of 16 MiB
stdout and 4 MiB stderr; exceeding these fails the tool. Narrow literal project
search skips `.git`, dependencies/build folders and symlinks, limits depth to 16,
entries to 2,000, files to 1,000, successfully read content to roughly 8 MiB, and results to a bounded
prefix. It is intentionally incomplete and reports truncation when a scan budget
is reached. Local tools do not provide hostile concurrent-filesystem isolation.

## Local validation and remaining migration

`native_tests.rs` uses temporary profiles/directories and loopback HTTP/SSE
servers through actual `agent:run`, `agent:approve`, `agent:cancel` and event
dispatch. Pi is pointed at a nonexistent fixture binary for native cases. Fixtures
cover write/read/final answer, request schemas and correlated tool history,
malformed/partial streams and batches with zero effects, duplicate IDs, review,
remembered/destructive/complex commands, deny/stale/concurrent approvals,
cancellation during headers/body/approval/child execution, ambiguous edits, Unicode
output truncation, direct indexed search, routing and a side-effect counter that
stays ONE when the next provider request fails. Pi compatibility tests use a local
mock process. No external AI API, credentials or real profile is used.

Run `pnpm typecheck`, `pnpm test`, and
`CARGO_BUILD_JOBS=2 cargo test --manifest-path src-tauri/Cargo.toml --lib --locked`.
Direct Cargo builds need the checksum-pinned embedding payload and existing
Extension Worker/resources staged first (see `EMBEDDING_RUNTIME.md`).

Future slices can add authenticated protocol adapters, model-specific image
encoding, richer command permissions, turn-local transport retries, streaming tool
output and additional tools. They must retain correlation, cancellation and
no-replay guarantees. Rust cannot eliminate remote provider outages. No RAM,
speed, desktop smoke, Windows shell, packaging or live-provider claim is made.

Implementation verification on macOS arm64 (Rust 1.98.0): `pnpm typecheck` passed;
`pnpm test` passed 884 tests with 9 skipped; `CARGO_BUILD_JOBS=2 cargo +1.98.0 test
--manifest-path src-tauri/Cargo.toml --lib --locked` passed 506 tests with 9 ignored.
The 17 added Rust fixtures include the actual 30-second post-effect header timeout.
The checksum-pinned embedding ZIP was staged locally and `pnpm build:backend`
prepared the normal test resources; the desktop application was not launched.
