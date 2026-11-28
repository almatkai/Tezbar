# Obsolete TypeScript backend cleanup

Verified locally on macOS Apple Silicon, 2026-11-05, from
`e2d19a0444312bd1d8afe2db836ec9ec6d937e69` on `feat/native-rust-agent`.
This is a dependent cleanup of the completed native migration. Historical
migration evidence and Opus reviews retain their original counts and conclusions.

## Removed and retained scope

- Deleted 140 files under `src/main`: 92 non-test TypeScript files (including
  declarations), 46 implementation-only test files, and two JS supervisors.
  This removes agent/Pi, provider/chat, search, Knowledge, IPC/server/router,
  storage, voice, terminal, system services and unused native-platform helpers.
- Deleted the old `tests/bridge-test.ts` manual core probe, the generated
  `dist-backend/agent/raymes-pi-policy.ts`, and two tracked stale `.tsbuildinfo`
  caches (already excluded by the repository's ignore rules).
- Retained 39 files in `src/main`: 23 non-test TypeScript files and 16 test files.
  They implement extension installation/catalog/build/runtime, Raycast APIs,
  native-helper extension adapters, framed worker/host requests, desktop adapters
  for development and tests, and SQLite compatibility. These are active JS seams.
- Moved the application snapshot DTO into `src/shared/applicationSearch.ts`.
  Extension consumers now import worker services/window state directly; removed
  the build aliases to deleted core paths, Pi's Vitest alias and Pi test-profile
  setup. Active manual extension probes now mock worker seams and remain present.
- Kept the build's forbidden-core-input guard and obsolete-output removal.
  Committed regenerated worker JS, source map and metafile because this repository
  tracks them. Native vector libraries remain byte-for-byte unchanged; frontend
  output is ignored. No generated incidental output was restored or copied from
  another checkout.

Rust sources/Cargo configuration, renderer behavior, release/signing policy and pins,
real credentials/profiles, historical verification reports and third-party
reference/license material were not changed. User-installed MCP programs retain
their language-independent native-host contract.

## Reachability and dependencies

The bounded XERJ query was followed by exact source checks because its existing
index is incomplete/stale. A TypeScript AST walk of retained tracked source,
script and test files checked imports, re-exports, type imports, literal dynamic
imports/requires and test mocks: zero missing relative modules. Exact searches
also checked Rust/build/manual/performance scripts and old entrypoint/asset paths.
Manual review of extension `createRequire` calls confirmed their paths resolve
inside the extension package or Node modules, rather than the removed core.

The generated esbuild metafile still has exactly the same 22 `src/` inputs as the
base revision: 20 extension-only `src/main` files plus shared extension repository
and catalog utilities. It contains no removed core inputs. The additional
non-bundled `src/main` files are the development desktop adapter, Node SQLite test
adapter and SQLite declarations. Actual extension source compilation, SQL, AI and
platform-host requests are covered by the Bun fixture below.

Removed the unused root `@anthropic-ai/sdk` declarations from dependencies and
devDependencies and the unused direct `zod` declaration. pnpm removed the SDK and
its now-unused `json-schema-to-ts`, `ts-algebra` and `@babel/runtime` lock entries.
`zod` remains transitively resolved for the live AI SDK packages. No surviving
package versions changed.

Retained `@ai-sdk/react` and `ai` (renderer transport), `fuse.js` (renderer and
extension catalog), `esbuild` and platform binaries (extension compiler),
`better-sqlite3` (extension SQL/development imports; bundled Bun and Node-test
aliases remain), and `sqlite-vec` (native Rust Knowledge resources staged by the
build). Removing a JS Knowledge implementation does not remove native assets.

## Validation

Environment: Node 26.7.0, pnpm 11.17.0, Bun 1.4.0.

| Command/check | Result |
| --- | --- |
| `pnpm install --lockfile-only --ignore-scripts` | Lockfile updated; obsolete package entries pruned |
| `pnpm install --frozen-lockfile --ignore-scripts --offline` | Passed |
| `pnpm typecheck` | Passed, renderer/shared and main/manual projects |
| `pnpm test` | 744 passed, 7 existing skips; 80 files passed, 1 skipped; zero failures |
| `pnpm build:frontend` | Passed |
| `pnpm build:backend` | Passed; embedding payload matched the committed pin; isolation guard passed |
| `pnpm test:extension-worker` | Passed actual bundle build and Bun process fixture |
| Retained module graph / generated bundle input comparison | No missing relative imports; unchanged 22 source input set |
| `git diff --check` | Passed |

The `src/**/*.test.ts` inventory decreased from 127 to 81 files solely by deleting
46 tests of retired implementations. No retained tests were removed or newly
skipped. The worker smoke now additionally compiles a temporary TypeScript
extension and verifies native-host application snapshots and selected text,
Unicode SQL reads and rejected SQL writes through Bun's SQLite shim. Existing
checks cover prebuilt JS/view actions, clipboard text/image/HTML/paste, AI
forwarding, host errors, forbidden core requests and EOF cleanup. Its host is an
in-memory fixture; files/databases/extensions live only under its owned temporary
directory and are removed afterward. Unit tests continue to use temporary profiles
and mocks, including the new worker window-state seam.

The existing `@raycast/api` peer warning requests React 19 type definitions while
this app uses React 18. It is unchanged by this cleanup. The lockfile verifies and
both builds/typecheck pass.

No desktop app, production profile or extension install was opened or modified.
No live provider/MCP calls, releases, signing or GitHub Actions were run. Manual
probes requiring installed extensions, sibling repositories or external audit
files were preserved/typechecked but not executed. Bun 1.2.5 and Windows were not
retested here. Rust suites were not repeated because Rust source and Cargo
configuration did not change; the updated extension bundle was instead exercised
through the actual Bun fixture. Prior reviewed native evidence remains in
[NATIVE_AI_VERIFICATION.md](NATIVE_AI_VERIFICATION.md).

## Coordinator acceptance

The coordinator independently inspected the complete deletion/non-deletion diff
and lockfile changes at `86fc4ad`, verified unchanged Rust/renderer/signing sources
and immutable review reports, and compared both esbuild source-input sets: exactly
the same 22 paths, all still present. It reran typecheck, the complete retained
suite (744 passed / 7 existing skips), and the real Bun worker fixture successfully.
No additional implementation changes or skips were needed.

Retained coordinator logs:

- `/private/tmp/tezbar-cleanup-parent-typecheck.log`
- `/private/tmp/tezbar-cleanup-parent-tests.log`
- `/private/tmp/tezbar-cleanup-parent-worker.log`
