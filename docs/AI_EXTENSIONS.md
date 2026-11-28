# Adding tools and instructions to AI mode

Open Settings → AI → AI Extensions. Add a program already installed on your
computer, a web connection, or import/paste a configuration. Review the literal
command and arguments or URL, then choose Test connection. A successful test
shows the available tools. Choose Enable to make them available to later AI runs.
Adding or importing alone never starts a program or opens a connection.

Programs use MCP over newline-delimited stdio. Web connections use Streamable
HTTP. The host supports current 2026-07-28 stateless metadata and common legacy
initialization. Browser sign-in uses OAuth discovery, PKCE and either your
pre-registered Client ID or a service's dynamic registration endpoint. If the
service supports neither, use an access token or ask its administrator to register
Tezbar. There is no invented Tezbar-hosted client metadata document.

Extensions run with your OS permissions. Review always asks. Other modes follow
the connection policy: ask every time, allow tools marked read-only by the server,
or allow all after confirming the connection name. Server annotations are claims,
not a sandbox. Changed tool definitions require another review. A tool call is
never automatically retried after a lost connection: its outcome may be unknown.

Common `mcpServers` configurations are accepted. Literal environment/header
values with secret-looking names are moved into the encrypted connection secret
store. No secret is written in plaintext if the OS key store is unavailable.
Only explicitly declared variables and a small OS environment allowlist reach a
program. Install server programs yourself; Tezbar never installs their packages.

A version 1 manifest can add a connection, instructions, prompts and profiles:

```json
{
  "manifestVersion": 1,
  "id": "com.example.notes",
  "name": "Notes",
  "connections": [{
    "id": "notes",
    "name": "Notes",
    "transport": "stdio",
    "command": "notes-mcp-server",
    "args": ["stdio"],
    "toolPolicy": "ask"
  }],
  "instructions": "Cite the note title when using a note.",
  "prompts": [{
    "name": "summarize",
    "title": "Summarize a note",
    "arguments": [{"name": "topic", "required": true}],
    "template": "Summarize my notes about {{topic}}."
  }],
  "commands": [{"name": "summarize", "prompt": "summarize"}],
  "agents": [{
    "id": "reviewer",
    "name": "Reviewer",
    "instructions": "Review and cite sources.",
    "tools": ["read", "grep"]
  }]
}
```

Profiles can narrow the available tools; they cannot grant permissions. Type `/`
at the start of the AI composer to choose a prompt. Resolved text is inserted as
a draft for editing and is never automatically sent.

Pi JavaScript packages no longer load. To reuse their behavior, expose an MCP
server in any language and connect it explicitly. The Raycast-compatible extension
worker is a separate feature and is unchanged.

Provider routing facts and Antigravity ports are attributed to their MIT sources
in `licenses/`. No external reference package is executed by the native runtime.

## Connections and trust

Use Edit connection to change a command, variables, URL, token or policy. Secret
inputs show Saved, never a token fragment. Changes require another test; Test does
not enable a connection. Changing a saved secret also disables the extension
until it has been tested again. Remove deletes its encrypted saved secrets.
Review always asks for each tool. An exact tool allowance is bound to its
connection and descriptor hash; a changed definition revokes it.

Only explicitly declared environment variables plus basic OS startup variables
reach a program. Use `{ "secret": "NOTES_TOKEN", "label": "Notes token" }` as an
`env` or header value, then enter it in Settings. Secret-looking literal values in
imports are moved into encrypted storage before the manifest is saved. Exported
configurations should contain references, not credentials. Secrets need the OS key
store; Tezbar refuses to save them if it is unavailable.

A project can put the same manifest in `.tezbar/ai-extension.json`. Tezbar shows a
review notice in that folder; it never automatically starts it. Review and add it,
then test and enable. The exact location and bytes are pinned. Any change,
including whitespace, revokes trust and tool allowances. Project contributions
are available only within that folder.

## Prompts, profiles and custom providers

Settings includes local prompt, command alias and profile editors. Prompt
arguments marked required must be supplied before insertion. Remote prompts and
resources are discovered from the connection and resolve only through that same
server; Tezbar does not fetch arbitrary resource links itself. Image content is
inserted as a composer attachment, text as an editable draft. Supported resource
templates are also available to the model through the connection's resource tools.

A profile's `tools` list narrows the existing registry and grants no permissions.
Built-ins use names such as `read`, `grep` and `project_search`. For extension
tools, copy the **Profile tool name** shown in connection Details; do not construct
it from a display label. Omitting `tools` keeps available tools; `[]` allows none.
Profiles can be chosen before the first turn and remain with that conversation.

An enabled manifest can contribute `providers` with a `custom:` ID, `baseURL`, an
explicit `protocol`, model metadata and headers. Protocol choices are
`openai-chat`, `openai-responses`, `anthropic`, `gemini` and `ollama`. Header secret
references use the same encrypted store as connection secrets. Enter referenced
provider keys under the extension's Details and choose Save key, then enable the
extension. Disabling the
extension removes its provider; a conversation using it gets a clear unavailable
provider error rather than silently selecting another model.

See [the complete example](examples/tezbar-ai-extension.json). Its program and
URLs are placeholders; replace them with software/services you have chosen.
Import is a preview, not installation or startup. No remote-manifest importer,
marketplace, package auto-install, sampling, elicitation or roots capability is
advertised. Unsupported requests receive explicit errors.

Tools, resource contents and server instructions are untrusted input. Catalogs
and result text/images have size/type limits; remote schema references are
refused. Lost calls can have an unknown outcome and are not automatically
reissued. Exact secret values are redacted, but transformed secrets cannot be
reliably recognized. Approval and process ownership are not an OS sandbox.

## Recovering a missing connection key

If the OS-protected credential key is missing, or Windows DPAPI reports a permanent
invalid-data/key-state failure, AI Extensions offers
**Reset saved connection secrets**. The action archives the unreadable encrypted
store (and an existing unreadable Windows key file), creates a new protected key
and asks you to re-enter connection and extension-provider credentials. Affected
connections require Test again; affected providers show credential re-entry controls.
Nothing starts or signs in automatically. Unrelated definitions remain unchanged.
Restoring the old key is the route to recover the archived credentials. A locked
or access-denied/transient key-store failure must be made accessible; it cannot use
this reset or a plaintext fallback. A failed key-file replacement restores the original
ciphertext/key; unreadable archives are retained for manual recovery.

Providers supplied by an extension show their owner in AI settings. Edit their
endpoint, protocol and credentials under AI Extensions. Provider/model selection
remains available in the provider settings.
