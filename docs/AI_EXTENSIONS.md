# Adding tools and instructions to AI mode

Open Settings → AI → AI Extensions. Add a program already installed on your
computer, a web connection, or import/paste a configuration. Review the literal
command and arguments or URL, then choose Test connection. A successful test
shows the available tools. Choose Enable to make them available to later AI runs.
Adding or importing alone never starts a program or opens a connection.

Programs use MCP over newline-delimited stdio. Web connections use Streamable
HTTP. Modern 2026-07-28 and legacy initialization are supported in the currently
exercised fixtures. OAuth and the rest of the acceptance matrix remain unfinished;
see [the runtime status](NATIVE_AGENT_RUNTIME.md).

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
