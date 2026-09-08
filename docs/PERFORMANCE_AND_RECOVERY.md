# Idle memory and backend recovery

## Changes

| Source of overhead or failure | Change |
| --- | --- |
| Windows clipboard polling launched synchronous PowerShell reads every 100 ms, even when idle. Search refresh also read the clipboard synchronously. | The native host checks Windows' clipboard sequence counter and notifies the backend only when it changes. Reads are asynchronous, coalesced, and time limited. Search refresh no longer probes the clipboard. |
| Settings created a second WebView at startup and retained it after closing. | Create Settings on demand, off the Windows event loop, and destroy its WebView on close. |
| A hidden Tauri window remained `document.visibilityState === "visible"` in WebView2, so the launcher kept laying out transparent content and running animations while idle. | Synchronize the main WebView with native show/hide events and remove the hidden document body from layout. Rendering resumes when the launcher is shown. |
| Application, file, and renderer icon maps retained every data URL. | Each cache has an LRU limit of 256 entries and 4 MiB of string storage. Oversized entries are returned without being retained. |
| Expiration checks left abandoned search queries in memory. | Bound renderer query history to 32 cached result sets and backend history to 64, pruning expired entries on insertion. |
| The always-mounted background-task badge called `knowledge:snapshot` while hidden. That initialized a 4.4 GiB Knowledge database and launched a second Bun process even when no Knowledge feature was in use. | Background-task polling now observes Knowledge only after it has been explicitly activated. Normal and empty launcher searches use the lightweight file index; deep search and the Knowledge UI initialize content indexing on demand. |
| The initial file refresh retained up to 75,000 complete document objects plus duplicate ID collections at once. | Stream file metadata through 400-document batches and discard each batch after its index delta is written. |
| Dictation was always mounted in the launcher and opening Settings probed speech engines and model state regardless of the selected tab. | Remove the launcher dictation control and Voice navigation entry. Speech/model code is lazy-loaded only for an explicit voice API call; system read-aloud remains available on demand. |
| Extension runtimes, terminal services, AI agents, provider implementations, and installers were evaluated with the backend even when unused. | Load optional feature modules on first use and retain cleanup handles only for modules that were actually opened. |
| The supervisor blocked reading stdout from a process that could remain alive but unresponsive. | Drain stdout independently, check process exit, bound socket writes, and require a response to periodic health probes. An unresponsive backend is replaced after a 30-second probe timeout; probes start at 10-second intervals. |
| Slow startup could exhaust a five-second connection deadline repeatedly. | Allow 30 seconds to connect, drain output during startup, and defer search/Knowledge initialization until IPC is installed. |
| A crash or slow start looked like an empty launcher. | Show startup text before the application module loads, then native startup/reconnect/failure status with a Retry action. Keep mounted views and input intact during recovery; refresh launcher queries when a new generation connects. |
| A killed backend could leave its indexing child behind. | Give the indexing worker a parent-lifetime stdin pipe and exit when it closes. Completed workers explicitly exit. |

Automatic recovery retains backoff to avoid a rapid crash loop. After four short-lived failures it retries every 60 seconds; Retry interrupts that wait. Application shutdown stops the supervisor instead of spawning another generation.

## Verification

- TypeScript type checking, frontend/backend builds, focused clipboard/cache tests, and native tests.
- `node scripts/check-backend-lifecycle.mjs` after `pnpm build:backend` checks a real Bun process: localhost connection, health response, and clean exit when the host disconnects. It uses a temporary app-data directory and prints no user payloads. One local run connected in 333 ms and answered its first probe in 36 ms; these are a smoke check, not a comparative benchmark.
- The Windows idle test advances one minute and verifies that no clipboard helper reads occur until a native change notification arrives.
- The broader Knowledge service test completed its functional assertions but hit `EBUSY` while removing its still-open SQLite fixture on Windows. That test uses inline indexing, not the changed worker entrypoint.
- A release-to-release Windows sample used the same application profile. The final row was sampled after about 90 seconds with the existing 4.4 GiB Knowledge profile present but not activated. Short-lived clipboard helpers were counted separately. These are directional measurements from one machine, not a formal benchmark:

  | Persistent launcher processes | Private bytes | Working set | CPU, one-core equivalent | CPU, 12-thread machine | Observed PowerShell clipboard helpers |
  | --- | ---: | ---: | ---: | ---: | ---: |
  | Installed 0.0.4 baseline | 635.9 MiB | 660.3 MiB | 14.21% | 1.18% | at least 16 in 20.1 s |
  | First optimized release | 502.8 MiB | 636.2 MiB | 5.55% | 0.46% | 0 |
  | Current on-demand release | 406.8 MiB | 583.7 MiB | 2.97% | 0.25% | 0 |
  | Baseline to current | -229.1 MiB (-36.0%) | -76.6 MiB (-11.6%) | -79.1% | -79.1% | eliminated |

  The final process tree contained one Bun backend and no Knowledge worker: 237.8 MiB private for the backend, 159.2 MiB for the WebView2 family, 8.6 MiB for the native host, and 1.2 MiB for its console host. The baseline CPU figure does not include CPU already consumed by clipboard PowerShell processes that started and exited between samples, so the CPU improvement is conservative. WebView2 diagnostics confirmed that the current hidden launcher has `body { display: none }` and no running document animations; showing and hiding it restores and reapplies that state respectively.
- Cold startup was visually checked in the release WebView while the initial backend was deliberately suspended. The window showed **Starting Tezbar…**, an explanation that search and commands were being prepared, and **Retry now**. Resuming/replacing the backend restored the normal launcher.
- A post-connection backend was suspended to simulate a hang. The supervisor replaced it in 37.6 seconds, the old backend and its Knowledge child were both gone, and the new generation reached ready. The visible reconnect state kept the launcher mounted and showed **Reconnecting to Tezbar…**, explanatory text, and **Retry now**. A manually requested restart displayed that banner and returned to ready in about 0.8 seconds.
- On-demand Knowledge behavior was checked against the existing profile: the hidden launcher kept zero Knowledge workers, an explicit deep search returned 25 results, and the indexing worker then appeared as expected.
- After the on-demand changes, TypeScript type checking, the release build, 16 focused tests, all 25 native tests, and the live backend lifecycle check passed again.

## Further profiling

Repeat the release comparison over longer five-minute windows and record launch peak, settled idle, repeated Settings/search use, and an explicitly activated Knowledge indexing pass separately. On-demand indexing is expected to add a temporary second Bun process while real work is running.

The remaining structural costs are the persistent Bun runtime and search/SQLite state in the main backend, plus the WebView2 process family. The next material reductions would be moving the file refresh to a short-lived worker, rebuilding the lightweight search/IPC core in Rust, or destroying the launcher WebView after a long hidden timeout and accepting a cold-resume delay. macOS' remaining subprocess-based clipboard polling should also be measured separately.
