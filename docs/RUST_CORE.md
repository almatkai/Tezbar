# Rust core and extension compatibility worker

Tezbar's application services run in Rust, with every window and extension view
rendered by GPUI. The isolated Bun worker provides JavaScript/Raycast extension
compatibility only; it starts on explicit extension use, not application startup.
Node and pnpm are build/test tools, not application processes.

## Ownership

| Component                                                                             | Owner                                   |
| ------------------------------------------------------------------------------------- | --------------------------------------- |
| IPC dispatch, startup, shutdown, native windows and shortcuts                         | Rust / GPUI                            |
| Launcher ranking, application/file index, notes, snippets, clipboard history          | `crates/tezbar-core/src/launcher`                  |
| Provider requests, agent lifecycle/events, chat persistence, voice and CLI adapters   | `crates/tezbar-core/src/ai`                        |
| Document extraction, durable indexing queue, SQLite FTS/vector data, local embeddings | `crates/tezbar-core/src/knowledge`                 |
| Commands, permissions, safety confirmations/audit, system data and background status  | `crates/tezbar-core/src/system`                    |
| OS opening, capture, input, settings windows and paste                                | Native platform adapters                |
| PTY / ConPTY sessions, output, resize and termination                                 | `tezbar-core/terminal` and `terminal_bridge` |
| Installed extension metadata, preferences and catalog                                 | `crates/tezbar-core/src/extensions`                |
| Ported extension commands (first pilot: Perplexity)                                    | `crates/tezbar-core/src/extensions`                |
| Extension installation/build and Raycast JavaScript execution                         | Isolated extension worker               |

External tools remain external: user-added MCP programs, local speech engines, document
converters, optional llama.cpp inference and macOS Swift helpers are launched
when their features need them. Moving orchestration to Rust does not rewrite
those third-party tools. Application presentation remains GPUI.

## IPC and process lifecycle

GPUI requests enter the native dispatcher directly. Internal `core:*` routes
are rejected at the renderer boundary. A frozen inventory of 155 pre-migration
IPC channels checks that every old route has an owner; this verifies coverage,
while service fixtures verify payloads and behavior.

Starting Tezbar, searching, opening settings, reading installed extensions and
using notes do not start or download Bun. Ported extension commands also run
without Bun; [the native extension pilot](NATIVE_EXTENSIONS.md) documents their
coverage. Unported extension execution and extension installation start the
compatibility worker on demand. The host uses bounded, strict UTF-8
JSON frames separated by LF, correlated requests/replies, readiness and operation
deadlines, and an explicit host-service allowlist. Logs go to stderr. Worker
failure rejects pending calls; a later request can start a replacement. App
shutdown stops the worker and its process tree. After first use, the worker stays
alive until shutdown or restart so extension timers and sessions remain valid.
Session refresh/action/search/load/dispose and status controls continue only owned
sessions on a live worker. Delayed controls after disposal or a crash never start
or restart Bun; a fresh explicit execution can recover it.

The worker is process isolation, not an OS security sandbox. Extensions can run
JavaScript and subprocesses with the user's permissions. Core functionality does
not import or execute the legacy TypeScript services.

Native startup/retry state drives the GPUI recovery screen.
Knowledge initialization and its first migration resume in the background;
launcher readiness does not await a large or locked Knowledge database. Requests
to Knowledge itself await its serialized initialization.
Unlike the old separate backend, a crash of the native process requires restarting
the application; the removed Bun supervisor cannot recover a crashed Rust host.

## Data compatibility

Existing JSON stores and SQLite databases remain in their existing profile
locations. Native migrations preserve rows and additive/unknown configuration
fields; writes use atomic replacement or database transactions. Invalid JSON is
reported rather than silently replacing the profile with defaults.

- Production data keeps the established platform app-local-data directory.
- Configuration defaults to `~/.openray`.
- `TEZBAR_DATA_DIR` and `TEZBAR_CONFIG_DIR` select isolated data/config directories
  for testing. Use both to keep a smoke run independent of a real profile.
- Extension preferences, notes, chat events/provider metadata, search learning,
  clipboard pins and knowledge progress have persistence fixtures.

## Native agent runtime

Agent and completion execution now uses native Rust protocol adapters, with no
Pi/Node/OpenCode execution fallback. Canonical conversation history and AI
Extensions are owned by `crates/tezbar-core/src/ai`. Five wire protocols retain signed and
image-bearing history; all built-in providers, tools, catalogs/auth and explicitly
connected MCP extensions execute through Rust. GPUI owns presentation. Community
Pi JavaScript packages no longer load; MCP is the language-independent replacement
seam. [Native agent runtime](NATIVE_AGENT_RUNTIME.md) describes the modules,
permissions, compatibility and verification limits. Historical migration reviews remain evidence for their original revisions.

## Build and checks

Use Node.js 22+, the repository's pnpm version, and pinned Rust 1.98.1 for GPUI.
Bun 1.2.5 is the pinned bootstrapped extension runtime. Node/pnpm are build tools, not dependencies
of the installed Rust core.

```sh
pnpm install --frozen-lockfile
pnpm typecheck
pnpm test
cargo +1.98.1 test --locked --offline -j2 -p tezbar-core --lib
node scripts/check-backend-lifecycle.mjs
pnpm build:native
pnpm test:extension-worker
pnpm ui:check --target host
# Requires the built worker and an installed Bun; missing prerequisites fail.
cargo +1.98.1 test --locked --offline -j2 -p tezbar-core --lib \
  extension_worker::integration_tests::real_rust_host_bun_roundtrip_and_termination \
  -- --ignored --exact
```

`build:backend` now builds only `dist-backend/extension-worker.js`, checks its
dependency graph against accidental core imports, and stages the extension/native
resources. It removes the old `main.js`, Knowledge worker and JS supervisor
bundles. The obsolete TypeScript core implementations and their implementation-only
tests have been deleted. `src/main` now contains extension installation/build/runtime,
the worker's host bridges, and development/test desktop and SQLite adapters.
Shared renderer/worker contracts remain in `src/shared`; application snapshot types
no longer pull in the removed search implementation. Extension modules import the
worker bridges directly, without aliases to old core paths. The build still rejects
core modules in the worker's transitive input graph.

[TypeScript cleanup verification](TS_BACKEND_CLEANUP.md) records the deletion scope,
retained JavaScript dependencies and isolated checks. Historical migration counts
and reviews below describe their original revisions.

The native lifecycle fixture starts the services with no JS bundle in its test
profile, writes data, shuts down, reopens and verifies persistence. The actual
worker smoke uses a temporary extension and simulated host services to check
Unicode, clipboard operations, AI forwarding, errors and disconnect cleanup.
Neither test needs credentials, model downloads or the user's clipboard.

The Rust-host worker test also accepts `TEZBAR_TEST_BUN` for an existing Bun
executable. With `cargo test --release`, `TEZBAR_TEST_RESOURCES` can select an
actual package's resource directory (on macOS, `Tezbar.app/Contents/Resources`)
to exercise the production worker/resource lookup.

## Historical validation scope

The results below describe the earlier Rust-core migration before the GPUI source
cutover. They do not establish current native release or cross-platform acceptance.

Local macOS validation includes Rust 1.85 compilation, native and TypeScript
fixtures, the built Bun worker with 1.2.5 and 1.4.0, native helpers and desktop
packaging. A separate-profile debug desktop smoke with Bun absent from PATH verified
Unicode note search/opening, settings, Knowledge state and the installed-extension
list without creating a JS child process.

Rust core migration checks on 2026-10-21 passed 594 TypeScript tests (9 skipped), 169
native library tests (2 opt-in tests ignored), the separately invoked real-host
worker test, type checking and 8 release-policy tests. Native tests ran on Rust
1.85 with the default thread stack. The local macOS release app was built with
ad-hoc signing and updater-artifact generation disabled; it is not a published or
notarized release. Its isolated startup reached Rust-core readiness with no child
process. The optimized Rust-host test also passed against the actual bundled
worker/resources with Bun 1.2.5. UI automation could not attach to this release window, so that attempt
does not establish a release UI smoke pass.

The subsequent [Perplexity native pilot](NATIVE_EXTENSIONS.md#verification)
passed 175 native tests in total, including six new extension tests that also
passed in an optimized release build. Its documentation records the desktop
validation limits separately.

The CI matrix covers Windows x64 and macOS Intel/Apple Silicon. A configured CI
matrix is not evidence of a completed run: Windows runtime/installer validation,
real OS permission/input flows and live provider/model integrations still need
their respective hosts or credentials. No numeric memory or latency improvement
is claimed without a comparable release-build benchmark. Earlier measurements
are retained separately in [the pre-Rust performance baseline](PERFORMANCE_AND_RECOVERY.md).
