# Native AI: complete Rust agent and AI Extensions

Status: implementation spec, 2026-11-04. Base: `feat/native-rust-agent` at `6f32a99`
(`v0.3.0-beta.7` = `e246752`). Supersedes the "stage 1" limits in
[Native agent runtime](NATIVE_AGENT_RUNTIME.md) when delivered.

## 1. Outcome

Every AI request in Tezbar executes inside the Rust core. A clean machine with no
Node, npm, Pi, Bun or OpenCode installed can use all built-in providers (given their
credentials), all built-in tools, image input and multi-turn tool conversations on
macOS and Windows. A person can add capabilities to AI mode without recompiling
Tezbar, in any language, by connecting an MCP server or importing a manifest.

### In scope

- One native path for `agent:run`, `chat:run`, `query`, `ai:action`,
  `chat:generate-meta`, `core:ai:extension-ask` and `host.ai.ask`.
- Providers: `openai`, `openai-compatible`, `custom:*`, `anthropic`, `ollama`,
  `copilot`, `gemini`, `deepseek`, `tokenrouter`, `antigravity`, `opencode`.
- Windows shell execution and process-tree cancellation.
- AI Extensions: MCP over stdio and Streamable HTTP, manifests, instructions, prompts,
  slash commands, agent profiles and declarative custom providers.
- Legacy IPC aliases that answer from native state and never start Pi or npm.

### Out of scope

- Rewriting community Pi/JavaScript extensions (`pi-agents`, `pi-notify`, and so on).
  They stop loading. Settings says so; equivalents can return as MCP servers.
- The React renderer, ML inference engines, speech engines, document converters and
  macOS Swift helpers. These stay as they are. AI servers such as Ollama stay external.
- Loading unsigned native code into the Tezbar process, or embedding a JavaScript
  plugin runtime in the core. The Raycast extension worker is a separate product area
  and is unchanged.
- MCP elicitation, sampling and roots in this version (see §7.8).

### Definition of done

1. No code path under `native_core/ai` spawns Pi, Node, npm, or OpenCode for execution.
   `pi.rs`, `pi_loop.rs`, `gateway.rs`, the `raymes-pi-policy.ts` resource and the
   `agentRuntime` branch are gone.
2. Every row of the acceptance matrix (§11) passes through real dispatch with loopback
   fake servers and temporary profiles.
3. `pnpm typecheck`, `pnpm test` and the full Rust suite pass on macOS. The Windows
   rows pass on a real Windows host over Tailscale, or are reported as not run.
4. Full code review by Claude Opus 5.5 finds nothing outstanding.

No live-provider, RAM or latency claim is made by this work. Fixtures prove our side of
each protocol; only a live credentialed run proves a provider's side (§13).

### Implementation decisions after design review

- Preserve explicit provider/model protocol choices. Route new native requests by
  authoritative model metadata or an explicit protocol setting; do not guess a
  protocol from a model name or silently rewrite configured endpoints.
- Native permissions must honor the existing mode intent, including live Review
  changes and remembered allowances. Unknown/complex shell syntax and destructive
  commands require review before remembered/Turbo shortcuts; sensitive files always
  require review. Broader Auto/Turbo file behavior must be explicit in the UI help.
- OpenCode's current official docs restrict free-tier chat to OpenCode itself.
  Native access must use supported credentials/models and report restrictions
  truthfully; do not impersonate its client or bypass access checks. Model discovery is native HTTP/static only; opening Settings never executes a PATH program.
- Complete the requested runtime and extension features, and report upstream-only
  uncertainties separately from verified implementation behavior. No built-in
  provider may retain a Pi/Node execution fallback.

## 2. Current state this replaces

| Area | Now | Problem |
| --- | --- | --- |
| Routing | `native::selected` picks native only for OpenAI-style providers | All others go to Pi |
| Text path | `provider.rs::stream` flattens messages to strings | No images, signatures, or thinking; bad tool JSON becomes `{}`; `kind == "anthropic"` injects tool results as user text |
| Antigravity | `stream` and `agent:run` call `pi_run` | Needs Pi and its plugin |
| OpenCode | `opencode run` | Runs another agent with its own tools, outside Tezbar approvals |
| Copilot | `api.githubcopilot.com/chat/completions` for text; Pi with `COPILOT_GITHUB_TOKEN` for agents | Two different endpoints; Claude/GPT models need other protocols |
| Native limits | 24 turns, 64 calls, 256 KiB per turn, 64 KiB args, 8,000 chars per result, text only, cwd only, Unix only, Allow once only | Migration regression compared with Pi |
| Conversation memory | Renderer flattens prior turns into one string | Signatures, tool history and images do not survive between turns |
| Extensions | `pi-extensions:list` shows a read-only list | Nothing can be added |

## 3. Design decisions

1. **Canonical transcript, thin wire adapters.** One Rust transcript type holds text,
   images, thinking, tool calls and results. Five pure adapters translate it. Opaque
   provider data (signatures, encrypted reasoning) travels with its origin and is
   replayed byte-for-byte only to the same provider and model.
2. **Tools run only after a provably complete turn.** Retry happens below the agent
   loop and only for the current provider turn, so a retry cannot repeat an effect.
   There is no whole-run replay.
3. **MCP is the extension seam.** It is a real standard with servers in any language.
   Tezbar speaks both the current stateless revision (`2026-07-28`) and the earlier
   handshake revisions (dual-era client), because most servers in use today are legacy.
4. **Nothing runs until a person has seen it.** No manifest, project file or import
   starts a process or opens a connection before the person approves the exact command
   or URL. Trust is pinned to a hash; any change revokes it.
5. **Tool identity comes from a snapshot table, never by parsing a name.** Each run gets
   an immutable map from model-facing name to `(connection, original tool, descriptor
   hash)`.
6. **Credentials stay where they live.** Antigravity accounts stay in the Pi account
   files, OpenCode keys in its `auth.json`, Tezbar keys in `config.json`. Rust reads
   them in place and writes refreshed tokens back to the same store. It needs the files,
   not Pi.

## 4. Module layout

All paths are under `src-tauri/src/native_core/ai/`. The goal is deep modules: a small
interface over a lot of behaviour, tested through that interface.

```text
mod.rs              IPC dispatch only; delegates everything below
complete.rs         complete(CompletionRequest) -> text. All tool-free callers.
runtime/
  agent.rs          run_agent(RunRequest) -> Outcome. The turn loop.
  transcript.rs     Canonical types and replay rules (§5)
  store.rs          Per-conversation transcript and image blob persistence
  limits.rs         All numeric limits, in one place (§6.5)
  policy.rs         Approval decisions (§6.3), ported from raymes-pi-policy.ts
wire/
  mod.rs            TurnRequest, TurnEvent, AssistantTurn, TurnError
  transport.rs      HTTP send, deadlines, cancellation, turn-local retry
  sse.rs            SSE and NDJSON framing (extract from process::Lines usage)
  openai_chat.rs  openai_responses.rs  anthropic.rs  gemini.rs  ollama.rs
providers/
  route.rs          resolve(cfg, model) -> Route { protocol, url, auth, quirks }
  catalog.rs        llm-list-models and provider status for every provider
  copilot.rs  antigravity.rs  opencode.rs
tools/
  registry.rs       Built-ins plus the run snapshot: definitions, prepare, execute
  shell.rs  files.rs  search.rs  screen.rs  antigravity_tools.rs
ext/
  model.rs          AiExtension, Connection, Contribution types
  store.rs          ai-extensions.json, trust records
  secrets.rs        Encrypted secret store
  manifest.rs       Manifest and `mcpServers` import parsing
  snapshot.rs       Per-run tool snapshot and naming
  instructions.rs   Instruction assembly, prompts, commands, agent profiles
  mcp/client.rs  mcp/stdio.rs  mcp/http.rs  mcp/oauth.rs  mcp/schema.rs  mcp/result.rs
legacy.rs           pi:* and pi-extensions:* aliases (§9)
```

Interface of the wire seam (five real adapters, so a real seam):

```rust
pub trait WireProtocol {
    fn encode(&self, req: &TurnRequest, route: &Route) -> Result<WireRequest, TurnError>;
    fn decoder(&self) -> Box<dyn TurnDecoder>;
}
pub trait TurnDecoder {
    fn feed(&mut self, bytes: &[u8], on: &mut dyn FnMut(TurnEvent)) -> Result<(), TurnError>;
    fn finish(self: Box<Self>) -> Result<AssistantTurn, TurnError>; // Err if not terminal
}
```

`encode` and decoders do no I/O and are unit-tested with recorded fixtures. Only
`transport.rs` touches the network. Test endpoints reach production code through an
`Endpoints` struct on `AiCore` (`github.com`, `api.github.com`, Google OAuth, Cloud Code,
Copilot, OpenCode), overridable only under `#[cfg(test)]`.

## 5. Transcript and multi-turn fidelity

### 5.1 Types

```rust
enum Block {
    Text { text: String, opaque: Option<Opaque> },
    Image { mime: ImageMime, blob: BlobRef },
    Thinking { text: String, opaque: Option<Opaque> },
    ToolCall { id: CallId, wire_id: String, name: String, args: Box<RawValue>, opaque: Option<Opaque> },
    ToolResult { call: CallId, wire_id: String, name: String, blocks: Vec<Block>, is_error: bool },
}
struct Opaque { origin: Origin, value: serde_json::Value }
struct Origin { protocol: Protocol, provider: String, model: String }
```

`CallId` is Tezbar's own unique ID (`c<turn>_<index>`). `wire_id` is what the provider
sent, kept exactly. Arguments are kept as the original JSON text.

Opaque values are never logged, never emitted in `agent:event`, never sent to another
origin and never shown in the UI. Thinking text is not sent to the renderer; the UI keeps
the existing "Thinking…" stage.

### 5.2 Replay rules by protocol

| Protocol | Preserved and replayed | Rule |
| --- | --- | --- |
| Anthropic Messages | `thinking` with `signature`, `redacted_thinking` with `data`, `tool_use`, all in original order | Return the assistant content array complete and unmodified. Streams build signatures from `signature_delta`. Never reorder or merge. |
| OpenAI Responses | `reasoning` items including `encrypted_content`; `function_call` items with `id` and `call_id` | Send `store: false`. Replay every item in order. Results use `function_call_output` with the same `call_id`. |
| OpenAI Chat | `tool_calls[].id`, `tool_calls[].extra_content`, message-level `reasoning_content`, `reasoning`, `reasoning_details` | Keep unknown call-level and listed message-level fields as opaque; replay to same origin. |
| DeepSeek (Chat) | `reasoning_content` on every assistant message once tools are present | Required by DeepSeek; omission is a 400. Never send required or named `tool_choice`. |
| Gemini / Cloud Code | `thoughtSignature` on text, thought and `functionCall` parts; part boundaries | Never merge signed parts. See §8.9 for unsigned calls on Gemini 3 and later. |
| Ollama | `thinking` string on assistant messages | Tool results use `role: "tool"` with `tool_name`; there are no wire IDs. |

**Changing model mid-conversation.** Thinking and opaque data from another origin are
dropped (Anthropic and Gemini reject or misuse foreign signatures). Text, images, tool
calls and results remain. Wire IDs are sanitized for the target by a deterministic map
into `[A-Za-z0-9_-]{1,64}`. An unsigned tool call sent to a signature-requiring Gemini
model is converted using the rule in §8.9.

**Duplicate IDs.** Duplicates inside one turn fail the batch with zero effects. A wire ID
reused in a later turn is legal (several providers restart at `call_0`); identity comes
from `CallId`. This replaces the stage 1 across-run rejection. Missing IDs (Ollama, Gemini,
some local servers) are synthesized as `tz_<turn>_<index>` and sent back only where the
protocol has an ID field.

### 5.3 Conversation persistence

Native owns canonical history so it survives between runs.

- Store: `data_dir/llm/transcripts/<sessionId>/<turnId>.json` and content-addressed
  `blobs/<sha256>`, written atomically with mode `0600`.
- `agent:run` gains optional fields `sessionId`, `userTurnId`, `assistantTurnId`,
  `priorTurnIds` and `instructions`. The transport sends these beside `task`.
- For each prior ID, native uses the stored turn if it exists. Otherwise it uses the
  renderer's text for that turn (`priorTurns: [{id, role, text, error?}]`). This keeps old
  conversations working. The renderer's existing `CHAT_CONTEXT_MAX_TURNS` window decides
  which IDs are sent.
- Requests with no `sessionId` (old callers) use `task` as one user message.
- A turn is written after each completed provider turn and each tool result. An
  interrupted run gets synthetic results for unanswered calls:
  `"Interrupted. The outcome of this call is unknown."`. Saved history therefore stays
  protocol-valid, and nothing is re-executed.
- `chat:delete-turn`, `chat:delete` and `chat:clear` delete the matching files and any
  unreferenced blobs.
- Image budget: 128 MiB per conversation. Eviction removes the oldest blobs.
  `historyHasImages` is accepted and ignored. An image with no stored bytes (older
  conversations stored only metadata) is sent as the text
  `[Earlier image is no longer available]`, and a `log` event says so. It is never
  described as if the model could see it.

### 5.4 Images

Accepted input: PNG, JPEG, WebP; at most 4 per prompt and 8 MiB each (existing
`normalize_images`). Per-provider size limits live in `limits.rs`, checked before sending,
and fail with a message naming the limit.

| Protocol | User image | Image in a tool result |
| --- | --- | --- |
| OpenAI Chat | `{type:"image_url",image_url:{url:"data:<mime>;base64,<data>"}}` | Tool message carries text; a following user message carries the image, labelled with the tool name |
| OpenAI Responses | `{type:"input_image",image_url:"data:…"}` | Same follow-up pattern |
| Anthropic | `{type:"image",source:{type:"base64",media_type,data}}` | Image block inside `tool_result.content` |
| Gemini / Cloud Code | `{inlineData:{mimeType,data}}` | `functionResponse` part followed by `inlineData` parts in the same user turn |
| Ollama | Bare base64 in message `images` | Follow-up user message with `images` |

Copilot adds `Copilot-Vision-Request: true` whenever a request contains an image.

**Models without vision** (no `vision` capability in the existing model list): on macOS,
run the existing `screenocr` helper and append the extracted text, as `pi_run` does today.
On Windows, and when OCR yields nothing, fail before sending with: "The selected model
cannot read images. Choose a vision model or remove the attachment." The OCR path moves
from `pi.rs` to `tools/screen.rs`.

## 6. Agent loop

### 6.1 Turn

1. Build the snapshot (§7.5) and system prompt (§7.7).
2. Stream one provider turn through `transport`. Emit text deltas as `message`. Buffer
   thinking and tool arguments; emit only a "Thinking…" stage for thinking.
3. A turn is complete only at the protocol's terminal signal:

   | Protocol | Terminal signal |
   | --- | --- |
   | OpenAI Chat | `finish_reason` present. `[DONE]` is expected, but EOF after `finish_reason` is accepted (some servers omit it). A JSON (non-SSE) body is one complete response. |
   | OpenAI Responses | `response.completed`. `response.failed` and `response.incomplete` are errors. |
   | Anthropic | `message_stop` |
   | Gemini / Cloud Code | SSE EOF after a candidate with `finishReason` |
   | Ollama | NDJSON line with `done: true` |

   Tool calls on an incomplete turn never run. `length`/`max_tokens` with unfinished tool
   JSON is a labelled error. Chat servers that send tool calls with `finish_reason: "stop"`
   are accepted if the stream terminated and every argument parses to a JSON object.
4. Validate the whole batch before any approval or effect: unknown tools, schema, paths,
   sizes, duplicates. One invalid call returns error results for that batch with zero
   effects (the model can correct itself), up to 3 consecutive times, then the run fails.
5. For each call in order: decide approval (§6.3), recheck cancellation, disablement and
   paths, execute, emit `stage` events, append a bounded result.
6. Repeat until a turn has no calls. Emit `answer`, then `done`.

### 6.2 Retry and failure

`transport.rs` retries a turn at most 2 times when all of these hold:

- The failure is a connect error, 408, 425, 429, 500, 502, 503, 504, or a disconnect
  before the terminal signal.
- No visible text has been emitted for this turn.
- The run is not cancelled.

Delay is `Retry-After` (capped at 60 s), otherwise 1 s then 3 s. A `stage` shows
`Retrying (n/2)`. The retried request body is byte-identical. Tools from prior turns have
already recorded their results, and no tool from this turn has run, so effects cannot
repeat.

After visible text, a disconnect is a labelled error: prior turns stay in the transcript,
and the person can continue. A failure after effects never restarts the run. The old
`retry_allowed` whole-launch retry is deleted.

Errors stay labelled as now (`Native network`, `HTTP`, `model`, `protocol`, `validation`,
`tool`, `permission`, `limit`), but the provider's error message is shown when it is a
bounded JSON `error.message`, passed through `redact_text` and truncated to 500 characters.
`format_error`'s model guidance is kept. Local stderr is never appended.

### 6.3 Approval policy

Port the regex policy from `src/main/agent/raymes-pi-policy.ts` to `runtime/policy.rs`
with its tests: `isAutoAllowedBash`, `isPersistentlyAllowedBash`, `isDestructiveBash`,
`isSafeProjectBash`, `preferredIndexedSearchForBash`. Keep the rule blocking broad personal
folder scans until `pc_search`/`launcher_search` has been tried in this run. The config
mode is read before every tool, so changing mode mid-run applies to the next tool; an
explicit `approvalMode` on the request pins it, as now.

| Action | Review | Default | Auto | Turbo |
| --- | --- | --- | --- | --- |
| Shell: remembered exact or prefix rule | Ask | Run | Run | Run |
| Shell: `isAutoAllowedBash` | Ask | Run | Run | Run |
| Shell: `isSafeProjectBash` | Ask | Ask | Run | Run |
| Shell: other | Ask | Ask | Ask | Run |
| Shell: destructive | Ask | Ask | Ask | Ask |
| Read, list, search, `launcher_search`, `pc_*` inside cwd | Ask | Run | Run | Run |
| Read or list outside cwd | Ask | Ask | Run | Run |
| Write or edit inside cwd | Ask | Ask | Run | Run |
| Write or edit outside cwd | Ask | Ask | Ask | Run |
| Any file tool on a sensitive path | Ask | Ask | Ask | Ask |
| Screen capture | Ask | Ask | Ask | Run |
| Extension tool | Ask | Per connection (§7.6) | Per connection | Per connection |

Sensitive paths: `~/.ssh`, `~/.aws`, `~/.gnupg`, `~/.pi/agent`, Tezbar's config directory,
`~/.local/share/opencode`, `.env*`, OS credential folders. Paths are checked after
canonicalization.

Approval events keep the existing shape. `canRemember` is true only for rememberable shell commands and
extension tools; sensitive, destructive, complex and mutation-option shell operations always require fresh review. "Always" stores an exact command in `agentAlwaysAllowedExactCommands`, as
Pi did, or a per-tool allowance for extensions. This removes stage 1's "Allow once only".

These checks are a safety prompt, **not an OS sandbox**. Approved commands and extensions
run with the person's privileges. The docs must say so.

### 6.4 Built-in tools

Tool definitions are the same on every provider. Schemas stay within the common subset
(object root, `properties`, `required`, `items`, `enum`, `description`).

| Tool | Behaviour |
| --- | --- |
| `bash` (macOS) / `powershell` (Windows) | Exactly one is offered. macOS: `/bin/bash --noprofile --norc -c` with the login `PATH`. Windows: `powershell.exe -NoLogo -NoProfile -NonInteractive -Command`. The model is told which shell it has. Default 120 s, optional `timeoutSeconds` up to 600. |
| `read` | UTF-8, optional `offset` and `limit` in lines. Inside or outside cwd under §6.3. |
| `write`, `edit` | Stage 1's atomic replace. `edit` accepts one `oldText`/`newText` or `edits[]`, all validated before writing. A file may be up to 2 MiB. Parent directories can be created if `createParents` is true. |
| `ls`, `find`, `grep` | In-process and bounded; respect `.gitignore`-style skips (`.git`, `node_modules`, `target`, `dist`). These replace Pi's tools and `project_search`, which remains as an alias of `grep`. |
| `launcher_search`, `pc_search`, `pc_read` | Stage 1 routes, unchanged. The gateway HTTP server is deleted. |
| `screen_capture` | `core:platform:screenshot`. Returns an image for vision models. Without vision: macOS OCR text, or on Windows a clear error. |
| `generate_image`, `google_search` | Only when an Antigravity account is available (§8.9). Search counts as a read; image generation writes a file and follows the write row. |

### 6.5 Limits

All are constants in `limits.rs`. These replace stage 1 values that would regress work Pi
could do.

| Limit | Value |
| --- | --- |
| Provider turns / total tool calls per run | 100 / 256 |
| Calls per turn | 32 |
| Active run time (time waiting for approval excluded) | 1,800 s |
| Header wait / stream silence / single turn | remote30s, local/Ollama120s (configurable1–300s) / 90s / 600s total |
| Stream bytes per turn | 8 MiB (Anthropic, Responses and Gemini carry opaque reasoning) |
| Tool arguments per call | 1 MiB |
| Tool result sent to the model | 64 KiB of text. Truncation is stated in the result. |
| Stage output shown in UI | 8,000 characters (unchanged) |
| Text context | 4 MiB, images excluded |

When text context exceeds the budget, replace the oldest tool-result bodies with
`[Earlier output removed to fit the context limit]` and emit a `log` event. Tool call
records and opaque blocks stay. If it still does not fit, the run fails with a limit error.
Summarizing with the model is out of scope.

### 6.6 Processes and Windows

- `process.rs`: on Windows, spawn every child in a Job Object with
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, reusing the helper in `child_supervisor.rs` /
  `extensions/process.rs` (extract one shared helper). `taskkill` remains only as a
  fallback. `CREATE_NO_WINDOW` stays.
- Decode Windows child output as UTF-8 with lossy fallback; set
  `[Console]::OutputEncoding` to UTF-8 in the PowerShell preamble.
- Cancellation must end the whole tree within 2 s on both platforms and reject any
  following effect.
- Paths: strip the `\\?\` prefix from canonical Windows paths where safe, so they compare
  correctly with cwd and display normally.

## 7. AI Extensions

### 7.1 Model

An **AI Extension** is a record the person added. It contributes any of:

| Contribution | Source |
| --- | --- |
| Connection | MCP over stdio or Streamable HTTP. Gives tools, resources, prompts |
| Instructions | Text added to the system prompt |
| Prompts and commands | Named templates and `/` commands in the composer |
| Agent profiles | Named instructions plus an optional tool allowlist |
| Custom providers | Declarative entries in the existing `customProviders` config |

Tezbar's own built-in tools are not extensions and cannot be disabled here.

### 7.2 Manifest

`tezbar-ai-extension.json`, version 1. Unknown top-level keys are ignored and reported.
Limits: 256 KiB file; 32 connections.

```json
{
  "manifestVersion": 1,
  "id": "com.example.github-tools",
  "name": "GitHub tools",
  "version": "1.2.0",
  "description": "Issues and pull requests.",
  "connections": [
    {
      "id": "github",
      "name": "GitHub",
      "transport": "stdio",
      "command": "github-mcp-server",
      "args": ["stdio"],
      "env": { "GITHUB_TOKEN": { "secret": "github-token", "label": "GitHub token" } },
      "toolPolicy": "ask"
    },
    {
      "id": "docs",
      "name": "Docs",
      "transport": "http",
      "url": "https://mcp.example.com/mcp",
      "auth": { "type": "oauth" }
    }
  ],
  "instructions": "Prefer draft pull requests.",
  "prompts": [
    {
      "name": "triage",
      "title": "Triage an issue",
      "description": "Summarize and label an issue",
      "arguments": [{ "name": "issue", "required": true }],
      "template": "Triage issue {{issue}}."
    }
  ],
  "commands": [{ "name": "triage", "prompt": "triage" }],
  "agents": [
    {
      "id": "reviewer",
      "name": "Reviewer",
      "instructions": "Review for correctness only.",
      "tools": ["read", "grep", "github__get_pull_request"]
    }
  ],
  "providers": [
    {
      "id": "custom:team-gateway",
      "title": "Team gateway",
      "baseURL": "https://llm.example.com/v1",
      "protocol": "openai-chat",
      "models": [{ "id": "team-large", "capabilities": ["tools"] }]
    }
  ]
}
```

Import also accepts the common `{"mcpServers": {"name": {"command", "args", "env", "url",
"headers"}}}` shape, so a person can paste configs written for other MCP hosts. Literal
values in `env` or `headers` whose key suggests a secret, or that match `redact_text`
patterns, move to the secret store on import.

Manifests contain no scripts, hooks or install steps. A person installs the server's
program themselves. Tezbar never runs `npm install` or downloads binaries for this feature.

### 7.3 Storage

- `config_dir/ai-extensions.json`: non-secret records, tool enablement, pins, trust.
  Atomic writes and tolerated unknown fields, like existing stores.
- `config_dir/ai-extension-secrets.bin`: AES-GCM using `vault/crypto.rs`. The key lives in
  the macOS Keychain (`vault/keychain.rs`) or is wrapped by DPAPI on Windows, behind the
  existing `KeyStore` trait. Tests use an in-memory `KeyStore`.
- If the key store is unavailable, adding a secret fails with a clear message; secrets are
  never written in plaintext as a fallback.
- IPC never returns a secret, only `hasValue: true`.

Existing provider keys remain in `config.json`. Moving them is a separate decision.

### 7.4 Trust, installation and state

States: `disabled → testing → enabled`, with `needsReview`, `needsAuth` and `failed`
overlaying `enabled`.

1. **Add** stores the record as `disabled`. Nothing runs.
2. **Test connection** is the first execution. It is always a direct user action, and the
   UI shows the exact command and arguments, or URL, first.
3. A successful test stores the tool descriptors and pins each descriptor hash.
4. **Enable** makes it available to subsequent runs.
5. **Disable** stops new calls immediately, including in live runs (checked before each
   approval and each call). In-flight calls are cancelled. The stdio process is stopped.
6. **Remove** deletes the record, its secrets, OAuth tokens and trust.

**Changed tools.** When a descriptor's hash differs from the pin, that tool is withheld
and the connection shows `needsReview`, with a diff of name, description and schema. A
remembered "Always" for it is cleared. New tools arrive disabled until reviewed. Removed
tools disappear. This defends against a server changing what it does after approval.

**Project files.** At run start, native reads, and only reads, at most one
`<cwd>/.tezbar/ai-extension.json` (also at the Git root if cwd is below it).

- Its connections, providers and commands are **not** activated, launched or contacted.
  No environment variable is expanded. No URL is fetched.
- Settings and the chat view show one line: "This folder has AI extensions. Not running."
  with a Review action.
- Review shows the exact commands and URLs. Accepting creates a trust record keyed by
  `sha256(canonical cwd + canonical entry JSON)` and goes through the normal Test and Enable
  steps. Any byte change revokes it.
- Project extensions are active only for runs whose cwd is inside that folder.

**AGENTS.md.** Reading it executes nothing, so `<cwd>/AGENTS.md` (64 KiB maximum) is
included by default, matching what Pi did, with a Settings toggle. It goes in a delimited
block stating that it is project-supplied and cannot change permissions.

### 7.5 MCP client

Implement a small in-tree client on existing `tokio`, `reqwest` and `serde_json`. Do not
adopt `rmcp`: the current release needs Rust 1.88 while `rust-version` is 1.85, and we need
exact control of dual-era detection, bounds and per-run cancellation. Add `jsonschema` with
remote-resolution features disabled, at a version that builds on the crate's
`rust-version`. An official SDK server is a manual interoperability check (§12), not a
dependency.

**Versions.** Modern `2026-07-28`; legacy `2025-11-25`, `2025-06-18`, `2025-03-26`. The
deprecated HTTP+SSE transport is not implemented; a server that only offers it fails with a
message saying so.

**Era detection**, cached per process or origin and re-probed if it later fails:

- stdio: send `server/discover` with a 5 s timeout. A `DiscoverResult` or recognized modern
  error means modern. Any other error or a timeout means run the legacy `initialize`,
  `notifications/initialized` handshake.
- HTTP: send a modern request with required headers. A `400` with a recognized modern
  JSON-RPC error means modern (retry with a `supported` version). Any other 4xx falls back to
  `initialize`. Never key the fallback on one error code.

**Modern requests** carry `_meta` keys `io.modelcontextprotocol/protocolVersion`,
`/clientInfo` and `/clientCapabilities` (`{}` in this version). HTTP adds `MCP-Protocol-Version`,
`Mcp-Method`, `Mcp-Name` (with the `=?base64?…?=` encoding when needed) and any valid
`x-mcp-header` mirrors as `Mcp-Param-*`; tools with invalid `x-mcp-header` are excluded.
`Accept` lists `application/json` and `text/event-stream`; both responses are handled. An
absent `resultType` means `complete`. An unknown `resultType` is invalid.

**Legacy requests** use the negotiated `MCP-Protocol-Version`, `MCP-Session-Id`, 404 session
restart, optional GET stream and `Last-Event-ID` resume. Server-initiated requests get
`-32601`, except `ping`, which gets an empty result.

**stdio.** One newline-delimited JSON message per line. The environment is built from an
allowlist (`PATH` from `login_path`, `HOME`, `USERPROFILE`, `APPDATA`, `LOCALAPPDATA`,
`TEMP`, `TMP`, `TMPDIR`, `SystemRoot`, `LANG`, `LC_ALL`) plus the connection's declared
entries. Tezbar's own environment, such as provider keys, is not inherited. Keep the last
200 stderr lines in memory, scrubbed; stderr never means failure. Shutdown closes stdin, waits
2 s, then escalates (`SIGTERM`/`SIGKILL` on the group; Job Object on Windows). Non-JSON stdout
lines are logged and ignored; more than 64 in a row, or one line over 16 MiB, fails the
connection and stops the process.

**Lifetime.** One process per connection, shared across runs, since MCP does not treat a
process as a conversation. Start on first need and stop after 10 idle minutes or at app
shutdown. A crash fails in-flight calls and restarts lazily with backoff (1, 5, 30 s), then
`failed`.

**Change notifications.** Modern: `subscriptions/listen` with `toolsListChanged`,
`promptsListChanged`, `resourcesListChanged` while a connection is running. Legacy:
`notifications/*/list_changed`. A change refreshes the catalog and affects the next run.
Failure to subscribe only means the catalog refreshes by `ttlMs` or at run start.

**Authorization (HTTP).** `none`, `token` (secret sent as `Authorization: Bearer` or a named
header, only to the configured origin, with cross-origin redirects refused) or `oauth`:

- Discover with `WWW-Authenticate` `resource_metadata`, then both well-known paths, then
  RFC 8414 and OIDC metadata in the required order.
- Authorization code + PKCE `S256` on a loopback redirect `http://127.0.0.1:<port>/callback`.
  Refuse if `code_challenge_methods_supported` is absent. Send `resource` on both requests.
  Verify `state`, and validate `iss` when present.
- Client identity: preregistered (fields in Advanced) first, then Client ID Metadata Document
  when a Tezbar-hosted document exists (§13), then Dynamic Client Registration with
  `application_type: "native"`.
- Persist tokens and client identity keyed by issuer + resource. Refresh on 401 once. On 403
  `insufficient_scope`, set `needsAuth` with the requested scopes; never loop.
- The stdio transport never uses this flow; its credentials come from `env`.

### 7.6 Discover → validate → approve → execute → result → cancel

**Discover.** `tools/list`, `prompts/list`, `resources/list`, `resources/templates/list` by
declared capability. Bounds: 20 pages, 512 items per kind, 64 KiB per descriptor, 4 MiB per
list, 10 s per request. A single malformed descriptor is dropped with a visible warning; the
others remain.

**Validate.**

- Tool names follow the MCP recommended charset. A name appearing twice in one server drops
  both, with a warning. Both `description` and `title` are capped at 2 KiB and treated as
  untrusted.
- `inputSchema` must be an object schema that compiles. Supported dialects: 2020-12 (default),
  draft-07. No network `$ref`. Bounds: depth 32, 2,000 subschemas, 100 ms per validation.
  Anything else rejects that tool.
- Annotations are hints only.
- For providers needing it, `schema_for_wire` dereferences local `$ref`s and strips
  unsupported keywords, with the original schema still used for validating arguments. Port the
  bounded dereference from `pi-antigravity/src/stream/stream.ts`.

**Naming.** Model-facing name: `<connectionSlug>__<sanitizedTool>`, matching
`^[A-Za-z0-9_-]{1,64}$`. Slugs are unique per connection, at most 16 characters, and fixed
when added. Overlong or colliding names (compared case-insensitively) become
`<slug>__<prefix>_<6-hex-of-original>`. Built-ins never contain `__`, so they cannot collide.
Lookup is only through the snapshot.

**Snapshot.** Built at run start from the stored catalog and immutable for that run. A stale
catalog is refreshed in the background. A connection is given 5 s to be ready. If not, its
tools are omitted and a `log` event plus one system-prompt line say which extension is
unavailable. Active extension tools are capped at 128 per run, in the person's order, with a
warning when exceeded. Runs own their calls: a registry maps `(connection, JSON-RPC id)` to
`runId`, so concurrent runs stay isolated and a run can cancel only its own calls.

**Approve.** Connection `toolPolicy`:

| Value | Label in UI | Default, Auto, Turbo behaviour |
| --- | --- | --- |
| `ask` (default) | Ask every time | Always prompt, except tools individually remembered |
| `allowReadOnly` | Allow read-only tools | Run when the server marks `readOnlyHint: true`. Otherwise prompt. The label states this relies on the server's own claim. |
| `allow` | Allow all | Run. Requires a typed confirmation of the connection name when set. |

Review mode always prompts. The card shows the extension's name, the tool, and exact arguments
as JSON (up to 4,000 characters, stating when more exist). Arguments are validated before the
prompt, and again before the call if the hash changed.

**Execute.** `tools/call` with a default 120 s timeout, per-connection up to 1,800 s. Progress
may reset the idle timer but not the hard cap. `input_required` with only `requestState` is
retried with the exact state and a new ID, at most 3 times. `input_required` with
`inputRequests` should not occur with empty capabilities; if it does, it is a tool error:
"This tool needs interactive input, which Tezbar does not support yet."

**A tool call is never retried automatically.** MCP asks clients to re-issue a request after a
broken stream, but a `tools/call` may already have had effects. Tezbar reports
"Connection lost. The outcome of this call is unknown." to the model and the UI; a new attempt
is a new model decision and goes through approval again. This is a deliberate, documented
deviation for `tools/call` only. Discovery requests are retried normally.

**Bounded result** (`mcp/result.rs`):

| Content | Handling |
| --- | --- |
| `text` | Added, up to the 64 KiB combined limit |
| `image` | PNG/JPEG/WebP, decoded and checked against the MIME by magic bytes, at most 8 MiB and 4 per result; sent per §5.4. Otherwise text `[Image omitted: <reason>]` |
| `audio` | `[Audio omitted]` |
| `resource_link` | URI and name as text. Never auto-fetched. |
| `resource` | Text bounded; blob `[Binary resource omitted]` |
| `structuredContent` | Validated against `outputSchema` if present. Used as JSON text only when `content` is empty. A violation is noted to the model. |
| `annotations.audience: ["user"]` | Shown in the stage, not sent to the model |
| `isError: true` | Bounded error result; the model may correct itself |
| JSON-RPC error | Bounded tool error with code |

All results are wrapped and described to the model as untrusted data. Secret values for that
connection and `redact_text` patterns are scrubbed from results, errors and logs. This catches
exact values only; a server that transforms a secret defeats it, and the docs must say so.

**Cancel.** Run cancellation: stdio sends `notifications/cancelled` with the request ID. Modern
HTTP closes the SSE response stream. Legacy HTTP POSTs `notifications/cancelled`. The call is
then abandoned; late responses are ignored by ID. The process is not killed unless the app is
closing or the extension is disabled.

### 7.7 Instructions, prompts, commands and profiles

System prompt order (each in its own delimited block, total 128 KiB):

1. Tezbar's base prompt, tools, shell type, platform, cwd.
2. Global custom instructions (Settings).
3. Enabled extensions' `instructions`, and MCP `instructions` from legacy `initialize`, labelled
   with the source. MCP-supplied text is capped at 8 KiB and marked as unverified.
4. Active agent profile.
5. `AGENTS.md` if enabled.

The renderer's lines in `buildAgentPromptFromChat` move to the base prompt so that they apply
once, not in every message. The final user message becomes the plain message.

**Composer.** Typing `/` as the first character of an empty AI composer lists prompts,
commands, agent profiles and resources by extension. Selecting a prompt with arguments shows
arguments inline. The resolved content goes into the draft for the person to read and edit before
sending. MCP `prompts/get` runs on selection, through `ai-extensions:prompt-resolve`, and is a user
action, so no approval prompt is needed. Text is capped at 64 KiB; images attach under the existing
limit. Role-labelled multi-message prompts become labelled text.

**Resources.** Selecting one attaches its text as a labelled block (64 KiB), or image as an
attachment. Models can also use `<slug>__list_resources` and `<slug>__read_resource` when the
connection declares `resources`; these count as read-only for `allowReadOnly`. URIs must come
from the server's listing or templates. `https://` resources are read through the server, not
fetched directly.

**Profiles.** The conversation's `agentProfileId` is stored on `ChatSession`. A profile's `tools`
list can only narrow the snapshot. Permissions and approvals are unaffected.

### 7.8 Deliberate limits in this version

- Client capabilities are `{}`: no elicitation, sampling or roots. Sampling and Roots are
  deprecated in MCP; elicitation is the next candidate because it needs a safe form UI.
- Pi TypeScript extensions are listed read-only as "Not loaded" (§9). There is no JS plugin host.
- No marketplace, no remote manifest URL import, no auto-install.

## 8. Provider matrix

Every provider reports truthful status in `llm-provider-statuses` with no Pi or OpenCode
execution. Keep existing config fields and task overrides (`config::selected`). Keep the
`0.0.0.0 → 127.0.0.1` rewrite. Only `http`/`https` URLs are allowed.

### 8.1 Summary

| Provider | Protocol | Endpoint | Auth |
| --- | --- | --- | --- |
| `openai` | Responses on the official host; Chat otherwise | `{base}/responses` | Bearer `apiKey`, required |
| `openai-compatible` | Chat (default) | `{base}/chat/completions` | Optional bearer |
| `custom:*` | Configured `protocol`, default Chat | Per protocol | Optional bearer |
| `anthropic` | Messages | `{base}/v1/messages` | `x-api-key` |
| `ollama` | Ollama chat | `{root}/api/chat` | None |
| `copilot` | By model (§8.6) | Copilot host | Copilot token chain |
| `gemini` | Gemini `streamGenerateContent` on the default host; Chat on a custom base | `/v1beta/models/{m}:streamGenerateContent?alt=sse` | `x-goog-api-key` |
| `deepseek` | Chat | `{base}/chat/completions` | Bearer |
| `tokenrouter` | Chat | `{base}/chat/completions` | Bearer, or `TOKENROUTER_API_KEY` |
| `antigravity` | Cloud Code envelope around Gemini | `/v1internal:streamGenerateContent?alt=sse` | Google OAuth account |
| `opencode` | By model (§8.8) | OpenCode gateway | API key chain |

### 8.2 `openai`

Official host: Responses with `store: false`, because its reasoning models lose continuity on Chat.
Streaming events: `response.output_text.delta`, `response.function_call_arguments.delta`/`.done`,
`response.output_item.done`, `response.completed`. A non-official base URL on this provider uses
Chat, matching stage 1. Per-model `AiProviderModel.protocol` overrides either choice. Send
`max_output_tokens` from the catalog or 16,000.

### 8.3 `openai-compatible`, `custom:*`, `tokenrouter`

Chat with `tools`, `tool_choice: "auto"`, `stream: true`, `max_tokens: 8192` unless the model
overrides it. No `developer` role or `store`. Stage 1's strictness is relaxed as described in
§5.2 and §6.1, without allowing unterminated tool calls to run. Legacy `function_call` is a
protocol error with no effects. Only `choices[0]` is read. Thinking fields are preserved opaquely.

Extend `CustomAiProvider` with optional `protocol` (`openai-chat`, `openai-responses`,
`anthropic`, `gemini`, `ollama`) and `headers` (values may be secret references). Absent means
`openai-chat`, so existing records keep working.

### 8.4 `anthropic`

Streams `content_block_start`, deltas `text_delta`, `thinking_delta`, `signature_delta`,
`input_json_delta`, `content_block_stop`, `message_delta`, `message_stop`, `ping`, `error`.
`anthropic-version` is a constant: `2026-10-21` on the official host, `2023-06-01` on custom
bases. `max_tokens` from the catalog or 16,000. System goes in `system`. Results are `tool_result`
blocks on a user message, never `[tool]` text. Do not send `thinking` unless the person or model
config enables it, and then use `{type:"adaptive"}`; never send `budget_tokens`, which later models
reject. Always accept and replay whatever thinking blocks arrive.

### 8.5 `ollama`

NDJSON `/api/chat` at `{base without /v1}`. `tool_calls[].function.arguments` is already an object;
validate it. Results: `{"role":"tool","tool_name":<name>,"content":<text>}`. Assistant replay
includes accumulated `content`, `thinking`, `tool_calls`. Send `think` only when enabled for the
model, since models without support reject it. Model listing: `/api/tags`, unchanged.

### 8.6 `copilot`

Facts here come from the existing Pi path and MIT `@earendil-works/pi-ai`; GitHub does not publish
this API, so it may change (§13).

**Credential chain.** Reuse `copilotGithubToken`, `copilotRefreshToken`, `copilotExpiresAt` and
`githubOAuthClientId` via existing `copilot_token()`, including device flow and refresh. Change the
device flow's requested scope from `repo workflow` to `read:user`; existing tokens continue to work.

**Base and token.**

1. Default: send the GitHub token as Bearer to `https://api.individual.githubcopilot.com`. This
   matches today's Pi behaviour with `COPILOT_GITHUB_TOKEN`.
2. On 401/403 before any bytes, exchange once at `https://api.github.com/copilot_internal/v2/token`,
   with the host taken from `proxy-ep` in the session token. Cache session token and host in memory
   only, per token hash, until 5 minutes before `expires_at`. Remember which form worked for that
   token.

**Protocol per model.** Read `/models`: use `supported_endpoints` when present. Otherwise `claude-*`
goes to `{base}/v1/messages` (Anthropic), `gpt-*` to `{base}/responses`, others to
`{base}/chat/completions`. Keep the existing non-chat model filter.

**Headers.** `User-Agent`, `Editor-Version`, `Editor-Plugin-Version`, `Copilot-Integration-Id:
vscode-chat` (values as in pi-ai's `COPILOT_HEADERS`), `Openai-Intent: conversation-edits`, `X-Initiator` (`user` when the last message is the
person's, else `agent`), `Copilot-Vision-Request: true` with images. A model-not-enabled error is
shown with a pointer to the person's Copilot settings. Tezbar does not change account policy.

### 8.7 `gemini`, `deepseek`

**Gemini.** When the base is the default `…/v1beta/openai` (or unset), use native
`generativelanguage.googleapis.com/v1beta/models/{m}:streamGenerateContent?alt=sse`: it exposes
`thoughtSignature` directly, and shares its encoder with Antigravity. A custom base stays Chat, with
`extra_content` preserved. Model listing keeps its current path. Function declarations use
`parametersJsonSchema` after `schema_for_wire`.

**DeepSeek.** Always Chat with `reasoning_content` round-trip. This replaces the Anthropic-compat route
in `pi_bridge`, so there is one code path, and DeepSeek's documented Chat rule is explicit. Do not send
required or named `tool_choice`.

### 8.8 `opencode`

Execution is HTTP only. `opencode run` is removed from every path, because it runs OpenCode's own
agent and tools outside Tezbar's approvals.

**Models.** Use native `GET {gateway}/v1/models` when a key exists and the static catalog.
Settings never executes `opencode models` or another PATH program for discovery.
Uncatalogued models expose an explicit per-model Protocol selector.

**Credentials, first found.**

1. `providerConfigs.opencode.apiKey` (add a field to Settings).
2. `OPENCODE_API_KEY`.
3. OpenCode's `~/.local/share/opencode/auth.json`, read in place and never written. The entry's format
   must be confirmed against OpenCode's source when implementing, and tested with a fixture.
4. None: send unauthenticated only for free-tier model IDs, and map 401 to "Add an OpenCode API key in
   AI settings." Do not claim anonymous access works; it has no official documentation (§13).

**Routes.** Strip `opencode/` to get the model ID. Base default is `https://opencode.ai/inference` from
the current official docs; `baseURL` overrides it. Choose by family: `claude-*` and listed Qwen Plus or
Flash models → `/anthropic/v1/messages`; `gpt-*`, `grok-*`, `muse-*` → `/openai/v1/responses`; `gemini-*`
→ `/google/v1beta/models/{m}`; all else → `/openai/v1/chat/completions`. `opencode-go/*` uses the
`https://opencode.ai/inference/go` gateway from the official Go documentation, chosen the same way. System One (`jev-*`) models are not chat
models and are excluded from the picker. Send `x-opencode-session` with the conversation ID, as a stable
per-conversation value.

### 8.9 `antigravity`

Port from `pi-antigravity` 0.11.0 (MIT, © Rahul Arya, github.com/Rahularya01/pi-antigravity). Add its
license to `licenses/pi-antigravity-MIT.txt` and cite the source in each ported file's header. The
project describes itself as unofficial and not endorsed by Google; Settings keeps this neutral but honest.

**Existing accounts.** Locate `PI_CODING_AGENT_DIR` or `~/.pi/agent`, reading `antigravity-accounts.json`
and the `antigravity` entry of `auth.json` in place. Pi need not be installed. Port `syncCurrentAuth`,
`findAccount`, active-account selection and the 5-minute refresh skew. Writes go back to the same files:

- `0600`, atomic rename, re-read under a lock file with stale-lock recovery, unknown fields kept.
- `auth.json` is rewritten only for its `antigravity` key; other providers' entries are untouched. An
  unreadable `auth.json` is never overwritten.
- Tokens are never logged, sent over IPC, or copied into Tezbar's config. Only email and account ID reach
  the UI, and only in Settings.

**New accounts.** Native PKCE `S256` sign-in with independent `state`, callback on `localhost:51121`,
5-minute timeout, and paste-the-callback-URL for when the browser cannot reach it, matching
`auth/oauth.ts`, including its public client constants and optional env overrides. Fetch project with
`loadCodeAssist`, falling back to the stable-ID derivation.

**Requests.** Port these with their tests:

- `convertMessages`, including signature validation, tool-ID sanitizing, the continuation text bridge, and
  the rule that unsigned tool calls to Gemini 3+ are replaced by `[Observation from …]` user text.
- `convertTools`: `parametersJsonSchema` for Gemini; allowlisted legacy `parameters` for Claude and GPT-OSS.
- `buildRequest`: envelope, `requestType`, `userAgent`, session and trajectory labels, `generationConfig`,
  `getThinkingConfig`.
- Headers including the versioned `User-Agent` that gates catalog visibility, as one constant.

**Routing and failure.**

- Runtime model mapping and fallback from `models/models.ts`.
- Endpoint order from `client.ts`. Walk to the next endpoint only on 403, 404, 429, 500, 502, 503, 504, before
  any bytes, and only if the 429 is not a quota wall.
- Quota 429: fail over to the next linked account (the active account changes, as in the plugin), for the
  same turn, before any bytes.
- Empty-response retry up to 2 times, before any visible output.
- All of this sits below the turn seam, so no effects can repeat. It counts toward the transport's bound.

**Model catalog.** `fetchAvailableModels` merged across endpoints, cached in
`data_dir/llm/antigravity-models.json` for 4 hours (no secrets). Read `models-store.json` if present as a
seed. Defaults last, as now.

**Availability.** `available` returns true only when an account exists, replacing the unconditional `true`.

**Tools.** Port `generate_image` and `google_search` (`image/image.ts`, `search/search.ts`). Generated images
go to `<cwd>/generated-images/`. Usage and quota commands were Pi TUI only and are not ported.

## 9. Legacy IPC

Channels stay, and the frozen inventory test still passes. None starts a process.

| Channel | New behaviour |
| --- | --- |
| `pi:check` | `{installed:true, version:"native"}` |
| `pi:install` | `{ok:true, method:"native", version:"native"}` with no process or download |
| `pi-extensions:list` | Discovered Pi packages/scripts only, with `enabled:false`, description "Not loaded. Tezbar no longer runs Pi extensions." AI Extensions use `ai-extensions:list`, preventing the Pi-only legacy consumer from mislabelling enabled native extensions. |
| `core:ai:pi-observe` | Native run state: `cwd`, `isStreaming`, counts, `model` |
| `core:ai:pi-query` | `get_state` and `get_session_stats` from native state; other commands return an error |
| `agentRuntime` | `auto`, `native`, `pi` accepted and persisted, ignored at runtime. `pi` logs once. |
| `RAYMES_PI_*`, `RAYMES_PI_BIN` | Ignored |

Remove `agent/raymes-pi-policy.ts` from `tauri.conf.json` and `scripts/build-backend.ts`. Remove `PiRow` and
Pi wording from onboarding, `aiProviders.ts` subtitles and Settings. The TypeScript under `src/main/agent` stays
as migration reference per `docs/RUST_CORE.md`, not as an app entry point.

New channels, added to `CHANNELS`, `tauri-bridge.ts` and `desktop-api.ts`:

```text
ai-extensions:list            -> { extensions, project, globalInstructions, useAgentsMd }
ai-extensions:parse-import    (text | filePath) -> preview, no side effects
ai-extensions:add             (draft) -> record, always disabled
ai-extensions:update          (id, patch)
ai-extensions:remove          (id)
ai-extensions:test            (id) -> { ok, era, version, tools, prompts, resources, error? }
ai-extensions:set-enabled     (id, enabled)
ai-extensions:set-tool        (id, tool, enabled)
ai-extensions:accept-review   (id)
ai-extensions:oauth-start     (id)
ai-extensions:oauth-cancel    (id)
ai-extensions:set-secret      (id, name, value)
ai-extensions:trust-project   (cwd, entryHash)
ai-extensions:composer-items  (cwd) -> prompts, commands, profiles, resources
ai-extensions:prompt-resolve  (ref, args) -> { text, images }
ai-extensions:read-resource   (ref) -> { text | image }
ai-extensions:set-instructions (text, useAgentsMd)
ai-accounts:antigravity:list | sign-in | sign-in-paste | cancel | switch | remove
```

Event: `ai-extensions-changed`, with no secrets. `core:*` boundary rules are unchanged.

## 10. Interface design

This brief is for implementation and later critical review. It extends the existing screen; it is not a
redesign. Target: `src/renderer/ai/AiExtensionsSection.tsx`, plus small parts in `AgentChatView.tsx` and
`SettingsView.tsx`. Do not grow `SettingsView.tsx` beyond mounting the section and the account panel.

### 10.1 Rules

**Use:** `Button`, `TextField`, `TextArea`, `SelectField`, `Message` from `ui/primitives`; `SettingsRow` and
`Divider` from Settings; native checkboxes as in the Models list; ink colour tokens; the type scale already
in this tab (13 px headings, 12–12.5 px rows, 11 px detail). Verify in dark and `data-theme='white'`.

**Do not:**

- Keep the old block's two-column card grid, feature chips, glowing or pulsing dots, emerald "active" badge or
  version chips.
- Add gradients, glass panels, new purple or blue accents, side stripes, icon grids, decorative icons,
  illustrations, or a modal when inline works.
- Add a separate Settings tab, or merge this with Settings > Extensions, which is the Raycast-compatible system.
- Use implementation words in main copy: "MCP", "stdio", "JSON-RPC", "schema", "snapshot", "era", "transport".
  "MCP" may appear once in helper text, because that is the term people search for, and in Advanced.
- Write marketing or reassurance copy, or claim that extensions are safe or sandboxed.
- Show raw tokens, even masked with partial characters. Say "Saved".

### 10.2 Settings > AI, placement

Same order as now: provider panel, AI Memory, Action Mode. The section titled **AI Extensions** replaces "Pi Agent
Extensions" at the same location (`SettingsView.tsx:2504`). Mount `<AiExtensionsSection />` under the existing
`Divider`. No outer bordered box; rows sit on the settings surface, like Memory and Action Mode.

```text
AI Extensions                                                     [Add ▾]
Add tools, prompts and instructions to AI mode. Extensions run on this
computer with your permissions. Tezbar asks before using their tools.
──────────────────────────────────────────────────────────────────────────
[x] GitHub                  14 tools · 3 prompts                 Details ›
[x] Team docs               Sign in required                     Details ›
[ ] Local notes             Couldn't start: command not found    Details ›
──────────────────────────────────────────────────────────────────────────
This folder has AI extensions. Not running.  ~/code/foo         [Review]
──────────────────────────────────────────────────────────────────────────
Instructions
  Custom instructions  [ textarea, 4 rows                                ]
  [x] Use AGENTS.md from the working folder
Prompts and commands                                                [New]
  /triage     Triage an issue · GitHub tools                     Edit  Remove
  /standup    Write my standup · Mine                            Edit  Remove
──────────────────────────────────────────────────────────────────────────
Pi extensions (2, not loaded)                                          ›
```

The Pi line appears only if such packages exist. Expanded, it lists their names and one sentence: "Tezbar no longer
runs Pi extensions. Add an equivalent connection to use its features." There are no actions on them.

### 10.3 Row states

Single-line rows. Status is words in `text-ink-3`; failures in the `Message` error colour. No dot.

| State | Status text | Row action |
| --- | --- | --- |
| Added, not tested | Not tested yet | Checkbox disabled; `Test` |
| Testing | Connecting… | Disabled checkbox |
| Enabled, not running | `14 tools · 3 prompts` | Details |
| Running | Same, no live indicator | Details |
| Sign-in needed | Sign in required | `Sign in` |
| Failed | `Couldn't start: <short reason>` | Details, `Try again` |
| Tools changed | Tools changed. Review | `Review` |
| Secret missing | Missing GitHub token | Details |

A list with 40 connections stays scrollable with the rest of the page. There is no inner scroll here.

Empty state, in place of rows: one line, "No extensions yet." plus the Add control. No illustration or example gallery.

### 10.4 Add

`Add ▾` menu: **Add connection**, **Import file…**, **Paste configuration…**. Choosing one expands an inline panel
directly under the section header, using the same treatment as the existing custom provider form.

**Add connection fields**

| Field | Control | Notes |
| --- | --- | --- |
| Name | `TextField` | Required, unique |
| Type | Two radio options | **Program on this computer**, **Web address** |
| Command | `TextField`, mono | Program only. The executable; no shell parsing |
| Arguments | `TextArea`, mono, one per line | Program only |
| Working folder | `TextField` + Browse | Program only, optional |
| Variables | Name, Value, `[ ] Secret` rows; Add variable | Program only |
| URL | `TextField` | Web address only; https, or http for localhost |
| Sign-in | `SelectField`: None, Access token, Sign in with browser | Web address only |
| Token | `TextField type=password` | "Saved" once stored |
| Header name | `TextField`, default `Authorization` | Under Advanced |
| Client ID | `TextField` | Under Advanced, for browser sign-in |
| Ask before using tools | `SelectField` | Ask every time (default), Allow read-only tools, Allow all |
| Time limit | Number, seconds | Under Advanced, default 120 |

Helper text under Type: "Connect any program or service that supports MCP."

Above the actions, once a command or URL is entered, show literally what will be run:

```text
Tezbar will run this with your permissions:
github-mcp-server stdio
```

Actions: **Test connection** and **Cancel**. Test saves the draft and connects. Then in place:

```text
Connected. 14 tools, 3 prompts.
[x] create_issue    Create a GitHub issue
[x] list_issues     List issues in a repository
…                                                   [Enable]  [Remove]
```

A failed test shows the short reason plus a `Show log` toggle revealing up to 20 scrubbed lines in a mono block, and
keeps the form for editing.

Choosing **Allow all** expands: "Type GitHub to allow all of its tools without asking." with a text field. Save is
disabled until it matches.

**Import.** A file picker or a mono paste box. **Preview** shows extension name, then one line per contribution, with each
command or URL shown verbatim. Warnings use plain words, such as "2 keys will be stored as secrets." Unknown fields are
listed. Nothing runs until **Add** and subsequent Test. Project review uses this same preview.

### 10.5 Details

`Details ›` expands the row in place. One open at a time.

```text
GitHub
Program     github-mcp-server stdio
Asks        [ Ask every time ▾ ]
Tools       [x] create_issue    Create a GitHub issue          Always allowed ×
            [x] list_issues     List issues in a repository
            Show all 14
Prompts     triage, standup, summarize
Files       3 available
Last error  (only when present)
            [Test]  [Edit]  [Show log]                [Remove]
```

**Remove** becomes inline: "Remove GitHub and delete its saved secrets?" `Remove` `Cancel`.
**Review changes** lists Added, Changed and Removed tools. For changed tools it shows old and new descriptions, and
schema changes as plain field lists, with `Accept` and `Keep disabled`.

### 10.6 Provider panel changes

- **Antigravity:** replace the Pi notice with an Accounts list, in the same place: email rows, "Active" in plain text,
  `Use`, `Remove`; `Sign in with Google` button. While waiting: "Finish signing in in your browser." with `Cancel` and an
  inline `Paste the callback URL instead` field. With no accounts, show only the sign-in control. One line of detail:
  "Uses your Google account through an unofficial integration."
- **OpenCode:** replace the CLI notice with an API key field using the same `SettingsRow` as other providers, showing
  "Found in OpenCode's settings" if the key comes from there.
- **Custom providers:** a `Protocol` `SelectField` in the existing form: OpenAI Chat (default), OpenAI Responses,
  Anthropic, Gemini, Ollama.
- `aiProviders.ts`: subtitle "Google Antigravity via Pi Agent" becomes "Google Antigravity". Remove Pi wording in
  onboarding; drop `PiRow`.
- No runtime selector anywhere. Do not expose `agentRuntime`.

### 10.7 AI chat

- **Composer `/` menu:** anchored above the composer with the visual style of `ApprovalModePicker`. Filter while typing.
  Group titles are extension names. Arrow keys, Enter, Esc; `aria-activedescendant`, focus remains in the input. `/` inside
  text does nothing. Launcher `/` behaviour is unchanged.
- **Prompt arguments:** an inline row above the composer, one small field per argument, in the same area as the attached
  screen row. Enter moves on; the last Enter inserts the text in the draft. It never auto-sends.
- **Agent profile:** a small text control beside the model picker: `Reviewer ×`. Nothing when unset.
- **Approval card:** the existing section, unchanged in style. Title: `Allow GitHub to use create_issue?`; body is the
  arguments as formatted JSON. `Allow once`, `Deny`, `Always allow this tool`. Shell cards regain `Always allow this command`.
- **Stages:** existing stage row. Label `GitHub · create_issue`, with the tool's `title` when given.
- **Project notice:** a single line above the composer in that folder's chat, once per folder per app session:
  "This folder has AI extensions. Not running." `Review` opens Settings at the section. Dismissible.
- **Unavailable extension:** existing diagnostics area, not a banner.
- **Thinking:** existing "Thinking…" stage. Do not render reasoning text.

### 10.8 Copy

Sentence case. Active voice. Short. Name the thing and the next step: "Couldn't start: command not found. Check the
command." Avoid "Oops", "seamlessly", "powerful" and exclamation marks.

## 11. Acceptance matrix

Harness: real `AiCore::handle` dispatch and `agent:event` capture, temporary `TEZBAR_DATA_DIR`,
`TEZBAR_CONFIG_DIR`, `home_dir`. `FakeProvider` is a loopback HTTP/SSE/NDJSON server that records request bodies and
headers. `FakeMcp` is compiled at test time using the `process_fixture` pattern, working on Windows too, plus a loopback HTTP
variant. Every test sets `RAYMES_PI_BIN` and a fixture `PATH` with marker scripts for `pi`, `node`, `npm`, `pnpm`, `bun`,
and `opencode`; each marker writes a file, and every test asserts none was created (**NoPi check**). Credentials are
fixtures only. No external endpoint, real profile or live AI call.

### Protocols and providers (P)

For each of `openai`, `openai-compatible`, `custom:*`, `anthropic`, `ollama`, `copilot` (each route), `gemini`, `deepseek`,
`tokenrouter`, `antigravity`, `opencode` (each route):

| ID | Scenario | Pass when |
| --- | --- | --- |
| P1 | Text answer through `agent:run` | Correct URL, headers, body; deltas and `answer`; NoPi |
| P2 | Two-tool run and final answer | Correlated IDs in the exact wire format; results in order |
| P3 | Parallel calls | All results returned for the batch, each correlated |
| P4 | User image | Exact encoding of §5.4 in the recorded body |
| P5 | Image in a tool result | Correct placement per §5.4 |
| P6 | Second run in a conversation | Prior thinking, opaque data and tool history byte-equal in the replay |
| P7 | Same conversation, different provider | Foreign opaque data absent; IDs valid for target; request accepted by fake's strict validator |
| P8 | `chat:run`, `query`, `ai:action`, `chat:generate-meta`, `core:ai:extension-ask`, `host.ai.ask` | Native call to fake; NoPi |
| P9 | `llm-list-models`, `llm-provider-statuses` | Correct source; truthful; NoPi |
| P10 | Fragmented bytes, split UTF-8, CRLF | Same result as whole-chunk delivery |

Protocol-specific:

| ID | Scenario | Pass when |
| --- | --- | --- |
| P11 | Anthropic `redacted_thinking` and `signature_delta` | Replayed unmodified and in order |
| P12 | Responses `encrypted_content` | Item replayed; `store:false` set |
| P13 | Gemini signature on parallel calls; signed text | Parts unmerged; signatures in place |
| P14 | Gemini 3 unsigned history | Converted per §8.9; no invalid `functionCall` |
| P15 | DeepSeek | `reasoning_content` on all prior assistant messages; no forced `tool_choice` |
| P16 | Ollama | Native `images`, `tool_name`; `done:true` terminal |
| P17 | Chat server without `[DONE]`; with `finish_reason:"stop"` on tool calls; JSON body; missing IDs | Runs only when complete and valid |
| P18 | Copilot vision and initiator headers | Present exactly when required |
| P19 | `opencode run` | Never spawned; `opencode models` is never spawned |

### Authentication (A)

| ID | Scenario | Pass when |
| --- | --- | --- |
| A1 | Copilot expired token with refresh token | One refresh at fixture endpoint, stored atomically, then request succeeds |
| A2 | Concurrent Copilot refresh | One refresh; both requests succeed |
| A3 | Copilot direct 401 then exchange | Session token used; base from `proxy-ep`; session token never written to disk |
| A4 | Copilot device flow success, cancel, stale | Stale never persists |
| A5 | Antigravity existing fixture store | Uses active account; no Pi installed |
| A6 | Antigravity expired token | Refresh, same store updated, mode `0600`, other `auth.json` entries byte-equal |
| A7 | Antigravity quota 429 | Fails over before bytes; first account unchanged except rotation fields |
| A8 | Antigravity concurrent write / corrupt `auth.json` | Lock respected / store not overwritten, clear error |
| A9 | Antigravity PKCE: state mismatch, timeout, paste, port busy | Each fails or succeeds as specified; no token in logs |
| A10 | OpenCode key chain, each source | Correct precedence; missing key message |
| A11 | Secret scan | Fixture tokens absent from events, logs, errors and IPC responses in all A and E tests |

### Modes and approvals (M)

| ID | Scenario | Pass when |
| --- | --- | --- |
| M1 | Full table of §6.3, each mode and action | Prompt or run exactly as listed |
| M2 | Change to Review mid-run, then to Turbo | Next tool follows the new mode |
| M3 | Per-request `approvalMode` | Pinned against config changes |
| M4 | Always allow for shell | Exact rule stored; used next time; not in Review |
| M5 | Deny, expiry, stale approval ID, concurrent runs | No effect; correct run affected |
| M6 | Destructive under Turbo | Asks |
| M7 | Sensitive path under Turbo, including through a symlink | Asks |
| M8 | Broad scan before indexed search | Blocked until search attempted |
| M9 | Ported regex policy | The TS test cases all pass in Rust |

### Failure and no replay (F)

| ID | Scenario | Pass when |
| --- | --- | --- |
| F1 | 503 then success, before text | Retried; identical body; two requests; side-effect counter stays 1 for earlier tool |
| F2 | `Retry-After` honoured and capped | Timing by paused clock |
| F3 | Disconnect mid text | Labelled error; no retry; prior turn kept |
| F4 | Disconnect mid tool arguments | No tool runs; retry permitted; final effect count correct |
| F5 | Disconnect after a completed tool, in the next turn | Effect count stays 1 |
| F6 | Malformed JSON, oversized stream, oversized args, duplicate IDs | Zero effects |
| F7 | 400, 401, 404 | No retry; bounded, redacted, actionable message |
| F8 | Cancel in headers, body, approval, tool, retry wait | Stops; no further effect or event |
| F9 | Interrupted run, then continue conversation | Synthetic result present; nothing re-executed |
| F10 | Context over budget | Oldest results elided; opaque blocks intact |

### Tools and platform (T)

| ID | Scenario | Pass when |
| --- | --- | --- |
| T1 | Read/write/edit/ls/find/grep inside and outside cwd | Behaviour and bounds per §6.4 |
| T2 | Symlink escape, change after approval | Rejected |
| T3 | Non-vision image on macOS, Windows | OCR text / clear error |
| T4 | Windows: run PowerShell tool end to end | Correct output and Unicode |
| T5 | Windows: cancel a tree with grandchild | All gone in 2 s; verified by PID |
| T6 | Windows: `\\?\` cwd and paths | Comparison correct |
| T7 | Windows: stdio extension start, call, cancel, shutdown | No orphaned process |
| T8 | macOS: process group cancel | All gone in 2 s |

### Extensions (E)

Each against both modern and legacy `FakeMcp`, stdio and HTTP, unless noted.

| ID | Scenario | Pass when |
| --- | --- | --- |
| E1 | Era detection, including legacy with silent probe, and 400 with modern error | Chooses correctly; caches |
| E2 | Discover, enable, call, result, answer | Full path through `agent:run` |
| E3 | Malformed descriptor among good ones | Only the bad one dropped; warning visible |
| E4 | Oversized list, descriptor, deep schema, remote `$ref`, unknown dialect, bad `x-mcp-header` | Rejected within bounds; no network |
| E5 | Duplicate tool names in a server; same names across two connections; overlong names | Deterministic names; correct routing; no parsing |
| E6 | Invalid arguments from the model | Not sent to the server; bounded error |
| E7 | Oversized text, line, image; bad base64; MIME mismatch; audio; resource link; `audience:user` | Handling of §7.6 exact |
| E8 | Non-JSON stdout, then floods | Ignored, then fails cleanly |
| E9 | `isError`, JSON-RPC error, unknown `resultType`, `input_required` variants | Handling of §7.6 exact |
| E10 | Cancel: stdio, modern HTTP, legacy HTTP | Correct mechanism recorded; late result ignored; other run's call unaffected |
| E11 | Server crash or stream break during `tools/call` | "Outcome unknown"; call counter stays 1 |
| E12 | 401 / 403 / OAuth full flow (fake AS): PKCE, `resource`, `iss`, refresh, step-up | Tokens only to the right origin, stored encrypted |
| E13 | Bearer with cross-origin redirect | Not followed; token not sent |
| E14 | Descriptor change | Tool withheld; `needsReview`; "always" cleared; accepted later |
| E15 | Disable during a run | Next call refused; in-flight cancelled |
| E16 | `list_changed` during a run | Current snapshot unchanged; next run updated |
| E17 | Two runs concurrently on one connection | Calls and cancels isolated |
| E18 | Project manifest in cwd | Marker command not run; URL gets zero requests; visible only as review item |
| E19 | Trust, then one byte changed | Revoked; nothing runs |
| E20 | Environment | Child sees allowlist plus declared entries; a host `SECRET_MARKER` is absent |
| E21 | Secret in stderr, error and result | Scrubbed everywhere |
| E22 | Key store unavailable | Secret save refused; no plaintext file |
| E23 | `mcpServers` import and manifest import | Parse without side effects; secrets moved |
| E24 | Instructions order, size caps, AGENTS.md toggle | Exact prompt recorded |
| E25 | Prompt resolve, resource read, profile tool narrowing | Correct draft text; snapshot narrowed only |
| E26 | App shutdown | All extension processes gone |

### Legacy and renderer (L)

| ID | Scenario | Pass when |
| --- | --- | --- |
| L1 | Every row of §9 | Exact shape; NoPi |
| L2 | `agentRuntime: "pi"` in config | Native run; config unchanged |
| L3 | Frozen IPC inventory | Passes |
| L4 | Source guard test | No `pi_binary`, `opencode run`, `npm` spawn in `native_core/ai`; every `process::command` call site is on an allowlist |
| L5 | Packaged resources | `raymes-pi-policy.ts` absent from `tauri.conf.json` |
| L6 | Renderer: transport fields, `/` menu, arguments, approval card variants | Vitest passes |
| L7 | Renderer: section states of §10.3, secrets never rendered | Vitest passes |
| L8 | Old conversation with image metadata only | Labelled placeholder |

## 12. Verification

```sh
pnpm typecheck
pnpm test
CARGO_BUILD_JOBS=2 cargo test --manifest-path src-tauri/Cargo.toml --lib --locked
node scripts/check-backend-lifecycle.mjs
```

Direct Cargo builds need the embedding payload and resources staged first, per `EMBEDDING_RUNTIME.md`.

**Windows.** Run the Rust suite on the Windows PC over Tailscale (see `AGENTS.md`), never on GitHub Actions. Fetch this branch
without touching other work, in a separate directory. T4–T7 require this host. If it is unreachable, state "not run" for
those rows. Do not substitute `cfg(windows)` compilation.

**Manual checks before release, none of which block the matrix:**

- Connect one official-SDK stdio server of each era.
- Separate-profile desktop smoke test with `PATH` lacking Pi and Node: a chat, a tool, an approval, an extension.
- One credentialed live call per provider the developer holds credentials for. Report which were and were not run.
- Critical design review of §10 in dark and light themes.

No performance figure may be stated without a comparable release-build benchmark.

## 13. Open risks and unverified facts

These are not reasons to drop a provider or capability. They are what fixtures cannot prove.

| Topic | Fact | Consequence |
| --- | --- | --- |
| Copilot API | Not publicly documented. Details come from pi-ai and today's working Pi path | May change without notice; errors must be readable. Needs a live check. |
| Antigravity | Unofficial integration of an internal API; `User-Agent` gates the catalog | Same. Port as one module with constants. Live check needed. Whether Google permits this use is for the person to judge; the upstream project says so. |
| OpenCode base URL | Official docs state `opencode.ai/inference`; pi-ai's catalog uses `opencode.ai/zen/v1` | Default to docs; confirm live. Overridable. |
| OpenCode auth file and anonymous use | The location is documented; entry format is not in the docs read; anonymous access is undocumented | Confirm against source. Treat 401 as "add a key". |
| Gemini Chat `extra_content` | The compatibility page confirms signatures are supported but not this field's shape in what was read | Generic passthrough covers it; native Gemini is the default path. |
| OpenAI tool-result images | Not confirmed in the pages read for Responses `function_call_output` | Hence the follow-up message pattern. |
| Anthropic `2026-10-21` | Taken from the current official example | Constant; easy to change. |
| CIMD | Needs a Tezbar-owned HTTPS metadata document, which does not exist | Falls through to preregistered or DCR. DCR is deprecated in MCP but available for at least 12 months. Hosting the document is a separate decision. |
| MCP `tools/call` re-issue | Spec says clients must re-issue after a broken stream | Tezbar deliberately defers to a new model decision to avoid repeated effects. |
| Regex shell policy | Not a parser and not a sandbox | Ported for continuity. Stated in docs. |
| Secret scrubbing | Exact-match only | Stated in docs. |
| Rust version | `rust-version` is 1.85; new crates must build on it | Choose compatible versions; raising it needs approval. |

## 14. Implementation order

One worktree, so sequence by dependency. Every step ends green and is committed separately. File ownership prevents overlap
when two steps run concurrently; only steps marked parallel may be.

| Step | Delivers | Owns | Matrix |
| --- | --- | --- | --- |
| 1 | Transcript, wire seam, SSE/NDJSON framing, `limits.rs`, Chat adapter, transport retry | `wire/*` (Chat), `runtime/transcript.rs`, `limits.rs` | P10, P17, F1–F4, F6–F7 |
| 2 | Agent loop on the seam; `complete.rs`; all callers moved; store and conversation fields | `runtime/agent.rs`, `store.rs`, `complete.rs`, `mod.rs`, `transport.ts`, `model.ts`, `chat.ts` | P1–P3, P6, P8, F5, F8–F10, L8 |
| 3 | Approval policy, tools, Windows process and shell | `runtime/policy.rs`, `tools/*`, `process.rs` | M*, T* |
| 4a (parallel) | Anthropic, Responses, Ollama adapters and images | `wire/{anthropic,openai_responses,ollama}.rs` | P4, P5, P11, P12, P16 |
| 4b (parallel) | Gemini adapter and Cloud Code envelope; Gemini and DeepSeek routes | `wire/gemini.rs`, `providers/route.rs` | P13–P15 |
| 5 | Copilot, OpenCode, catalog and status | `providers/{copilot,opencode,catalog}.rs` | P18, P19, P9, A1–A4, A10 |
| 6 | Antigravity: accounts, sign-in, requests, tools, license | `providers/antigravity.rs`, `tools/antigravity_tools.rs`, `licenses/` | A5–A9 |
| 7 | Remove Pi; legacy aliases; resource cleanup | `legacy.rs`; delete `pi.rs`, `pi_loop.rs`, `gateway.rs`, `native.rs`, `native_protocol.rs`; config | L1–L5, P7, A11 |
| 8 | Extension core: store, secrets, manifest, MCP stdio, discovery, snapshot, execute | `ext/*` except OAuth and HTTP | E1–E11, E14–E23, E26 (stdio) |
| 9 | MCP HTTP and OAuth | `ext/mcp/{http,oauth}.rs` | E1–E13 (HTTP) |
| 10 | Instructions, prompts, commands, profiles, IPC | `ext/instructions.rs`, `mod.rs`, `tauri-bridge.ts`, `desktop-api.ts` | E24, E25 |
| 11 | Interface, following §10 | `src/renderer/ai/*`, `AgentChatView.tsx`, `SettingsView.tsx`, `OnboardingView.tsx`, `aiProviders.ts` | L6, L7 |
| 12 | Docs: replace `NATIVE_AGENT_RUNTIME.md`, update `RUST_CORE.md` and `WINDOWS_SUPPORT.md`, add a short "AI Extensions" author guide. Windows run. | `docs/*` | §12 |

Design procedure for step 11: implementation follows §10; Opus then reviews the result critically in both themes; findings
go to a new implementation round with this brief and the findings.

## 15. Sources

- MCP: [Versioning](https://modelcontextprotocol.io/specification/versioning),
  [2026-07-28 changelog](https://modelcontextprotocol.io/specification/2026-07-28/changelog),
  [base protocol](https://modelcontextprotocol.io/specification/2026-07-28/basic/index),
  [compatibility](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning),
  [stdio](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/stdio),
  [Streamable HTTP](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports/streamable-http),
  [multi round-trip](https://modelcontextprotocol.io/specification/2026-07-28/basic/patterns/mrtr),
  [tools](https://modelcontextprotocol.io/specification/2026-07-28/server/tools),
  [prompts](https://modelcontextprotocol.io/specification/2026-07-28/server/prompts),
  [resources](https://modelcontextprotocol.io/specification/2026-07-28/server/resources),
  [deprecated features](https://modelcontextprotocol.io/specification/2026-07-28/deprecated),
  [2025-11-25 transports](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports),
  [lifecycle](https://modelcontextprotocol.io/specification/2025-11-25/basic/lifecycle),
  [authorization](https://modelcontextprotocol.io/specification/2025-11-25/basic/authorization).
- Anthropic: [thinking](https://platform.claude.com/docs/en/build-with-claude/thinking),
  [extended thinking](https://platform.claude.com/docs/en/build-with-claude/extended-thinking).
- OpenAI: [migrate to Responses](https://developers.openai.com/api/docs/guides/migrate-to-responses).
- Google: [thinking](https://aistudio.google.com/docs/thinking),
  [thought signatures](https://aistudio.google.com/docs/thought-signatures),
  [OpenAI compatibility](https://aistudio.google.com/docs/openai).
- DeepSeek: [thinking mode](https://api-docs.deepseek.com/guides/thinking_mode/),
  [tool calls](https://api-docs.deepseek.com/guides/tool_calls/).
- Ollama: [chat API](https://docs.ollama.com/api/chat), [tool calling](https://docs.ollama.com/capabilities/tool-calling).
- OpenCode: [Zen / Console](https://opencode.ai/docs/zen/), [providers](https://opencode.ai/docs/providers/).
- Local references, read only and MIT: `~/.pi/agent/npm/node_modules/pi-antigravity` 0.11.0;
  `@earendil-works/pi-ai` for Copilot and OpenCode routing facts. No credential file was read.
