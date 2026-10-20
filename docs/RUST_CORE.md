# Rust core and extension compatibility worker

Tezbar's built-in backend runs inside the Tauri Rust process. The renderer keeps
its existing IPC/event contracts. A separate Bun process is used only for the
JavaScript extension ecosystem; there is no general TypeScript backend fallback.

## Ownership

| Component                                                                             | Owner                                   |
| ------------------------------------------------------------------------------------- | --------------------------------------- |
| IPC dispatch, startup, shutdown, native windows and shortcuts                         | Rust / Tauri                            |
| Launcher ranking, application/file index, notes, snippets, clipboard history          | `native_core/launcher`                  |
| Provider requests, agent lifecycle/events, chat persistence, voice and CLI adapters   | `native_core/ai`                        |
| Document extraction, durable indexing queue, SQLite FTS/vector data, local embeddings | `native_core/knowledge`                 |
| Commands, permissions, safety confirmations/audit, system data and background status  | `native_core/system`                    |
| OS opening, capture, input, settings windows and paste                                | Native platform adapters                |
| PTY / ConPTY sessions, output, resize and termination                                 | `native_terminal` and `terminal_bridge` |
| Installed extension metadata, preferences and catalog                                 | `native_core/extensions`                |
| Extension installation/build and Raycast JavaScript execution                         | Isolated extension worker               |

External tools remain external: Pi/OpenCode, local speech engines, document
converters, optional llama.cpp inference and macOS Swift helpers are launched
when their features need them. Moving orchestration to Rust does not rewrite
those third-party tools or the React frontend.

## IPC and process lifecycle

Renderer requests enter the native dispatcher directly. Internal `core:*` routes
are rejected at the renderer boundary. A frozen inventory of 155 pre-migration
IPC channels checks that every old route has an owner; this verifies coverage,
while service fixtures verify payloads and behavior.

Starting Tezbar, searching, opening settings, reading installed extensions and
using notes do not start or download Bun. Extension execution/install operations
start the compatibility worker on demand. The host uses bounded, strict UTF-8
JSON frames separated by LF, correlated requests/replies, readiness and operation
deadlines, and an explicit host-service allowlist. Logs go to stderr. Worker
failure rejects pending calls; a later request can start a replacement. App
shutdown stops the worker and its process tree. After first use, the worker stays
alive until shutdown or restart so extension timers and sessions remain valid.

The worker is process isolation, not an OS security sandbox. Extensions can run
JavaScript and subprocesses with the user's permissions. Core functionality does
not import or execute the legacy TypeScript services.

Native startup/retry state still drives the renderer's existing recovery UI.
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

- Data defaults to Tauri's platform app-local-data directory.
- Configuration defaults to `~/.openray`.
- `TEZBAR_DATA_DIR` and `TEZBAR_CONFIG_DIR` select isolated data/config directories
  for testing. Use both to keep a smoke run independent of a real profile.
- Extension preferences, notes, chat events/provider metadata, search learning,
  clipboard pins and knowledge progress have persistence fixtures.

## Build and checks

Use Node.js 22+, the repository's pnpm version, and Rust 1.85+. Bun 1.2.5 is the
pinned bootstrapped extension runtime. Node/pnpm are build tools, not dependencies
of the installed Rust core.

```sh
pnpm install --frozen-lockfile
pnpm typecheck
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml --lib --locked
node scripts/check-backend-lifecycle.mjs
pnpm build:native
pnpm test:extension-worker
pnpm build:frontend
# Requires the built worker and an installed Bun; missing prerequisites fail.
cargo test --manifest-path src-tauri/Cargo.toml --lib --locked \
  native_core::extension_worker::integration_tests::real_rust_host_bun_roundtrip_and_termination \
  -- --ignored --exact
```

`build:backend` now builds only `dist-backend/extension-worker.js`, checks its
dependency graph against accidental core imports, and stages the extension/native
resources. It removes the old `main.js`, Knowledge worker and JS supervisor
bundles. The old TypeScript core sources remain as migration reference and
regression fixtures; they are neither app entrypoints nor packaged core services.

The native lifecycle fixture starts the services with no JS bundle in its test
profile, writes data, shuts down, reopens and verifies persistence. The actual
worker smoke uses a temporary extension and simulated host services to check
Unicode, clipboard operations, AI forwarding, errors and disconnect cleanup.
Neither test needs credentials, model downloads or the user's clipboard.

The Rust-host worker test also accepts `TEZBAR_TEST_BUN` for an existing Bun
executable. With `cargo test --release`, `TEZBAR_TEST_RESOURCES` can select an
actual package's resource directory (on macOS, `Tezbar.app/Contents/Resources`)
to exercise the production worker/resource lookup.

## Validation scope

Local macOS validation includes Rust 1.85 compilation, native and TypeScript
fixtures, the built Bun worker with 1.2.5 and 1.4.0, native helpers and desktop
packaging. A separate-profile debug desktop smoke with Bun absent from PATH verified
Unicode note search/opening, settings, Knowledge state and the installed-extension
list without creating a JS child process.

Final local checks on 2026-10-21 passed 594 TypeScript tests (9 skipped), 169
native library tests (2 opt-in tests ignored), the separately invoked real-host
worker test, type checking and 8 release-policy tests. Native tests ran on Rust
1.85 with the default thread stack. The local macOS release app was built with
ad-hoc signing and updater-artifact generation disabled; it is not a published or
notarized release. Its isolated startup reached Rust-core readiness with no child
process. The optimized Rust-host test also passed against the actual bundled
worker/resources with Bun 1.2.5. UI automation could not attach to this release window, so that attempt
does not establish a release UI smoke pass.

The CI matrix covers Windows x64 and macOS Intel/Apple Silicon. A configured CI
matrix is not evidence of a completed run: Windows runtime/installer validation,
real OS permission/input flows and live provider/model integrations still need
their respective hosts or credentials. No numeric memory or latency improvement
is claimed without a comparable release-build benchmark. Earlier measurements
are retained separately in [the pre-Rust performance baseline](PERFORMANCE_AND_RECOVERY.md).
