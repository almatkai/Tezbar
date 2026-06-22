# Raymes Refinement Loops

This file is the handoff between hourly refinement passes. Do not mark the work complete until at least 10 iterations have run, all 30+ sampled extensions have build and runtime evidence, the active-screen image path works end to end with a vision model, and the full relevant test/typecheck/lint suite passes.

## Progress

- Completed iterations: 10 / 10 minimum
- Sampled extensions: 30 / 30+ manifests inspected
- Extension runtime executions: 24 / 30
- Automation: `refine-raymes-hourly` (active, hourly, attached to this thread)

## Iteration 1 - 2026-06-20

### Fixed

- Added a typed active-display screenshot attachment flow to AI Chat. Electron captures the display under the pointer at a bounded resolution while temporarily hiding the Raymes window; Tauri uses its native screenshot command.
- Added Pi RPC image payload support (`images` on `prompt`) with base64/type/count/size validation.
- Agent runs now await Pi's prompt-acceptance response instead of possibly hanging after a rejected prompt.
- Added a 15-minute upper bound so a stuck provider or tool call cannot keep a run alive forever.
- Added focused prompt-image unit tests.
- Added sampled-extension compatibility for `getSelectedFinderItems`, `getAvatarIcon`, and `withCache`, used by `image-to-ascii`, `imgur`, `gmail`, `memberstack`, `google-maven-repository`, and `essay`.

### Verification

- Baseline suite: 61 tests passed before edits.
- New image tests: 3 passed.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint: passed.
- React Doctor: no correctness errors; one pre-existing-style performance warning for JSX passed as a `Hint` prop in `AgentChatView.tsx`.

### Deterministic 30-extension sample

Selected by sorting `md5("raymes-loop-1:" + extensionName)` from the repository's `extensions/` tree:

`wistia`, `raydoom`, `imgur`, `devdocs`, `google-maven-repository`, `cilium-docs`, `image-to-ascii`, `essay`, `slack-status`, `reverso-context`, `memberstack`, `teslamate`, `github`, `voice-to-text-windows`, `leetcode`, `popcorn`, `vercast`, `gmail`, `openstatus`, `rsync-commands`, `porkbun`, `rednote-viewer`, `models-dev`, `ohmyzsh-git-alias`, `diskutil`, `whimsical`, `open-in-shopify-admin`, `forgejo`, `papago-translate`, `parse-logs`.

All 30 package manifests parsed successfully. Together they contain 95 commands: 84 `view`, 6 `no-view`, and 5 `menu-bar`. Twenty-three use preferences, seven use command arguments, and all 30 declare runtime dependencies. This is manifest coverage only; do not count it as runtime execution.

The 30 source trees were materialized in `/tmp/raymes-extensions-audit`. TypeScript AST analysis found 39 runtime imports from `@raycast/api` and 17 from `@raycast/utils`. Remaining known runtime gaps are OAuth (`OAuthService`, `getAccessToken`, `withAccessToken`) and `useAI`; type-only imports such as `LaunchProps`, `Tool`, and `MutatePromise` do not require runtime shims.

## Iteration 2 - 2026-06-20

### Fixed

- Restored the Tauri backend build after the screen-attachment work by adding `desktopCapturer`, display size, and opacity contracts to `electron-shim.ts`. Tauri still captures through its native Rust command.
- Added a reusable `scripts/extension-runtime-harness.ts` that runs external package commands through Raymes' actual bundler and runtime with the backend shims.
- Fixed List pagination leaking into nested action traversal. The 30th visible row previously lost all actions; the real `ohmyzsh-git-alias` command now renders 30 rows with 120 actions and four actions on the boundary row.
- Added `Form.FilePicker`, `Form.TagPicker`, `Form.Dropdown.Item`, and `Form.Dropdown.Section` runtime tokens.
- Preserved boolean and string-array form values through renderer, preload, IPC sanitization, and action invocation. Checkboxes no longer submit string booleans and file pickers can submit multiple selected paths.
- Added renderer controls for file and tag pickers and option extraction from nested Raycast form items.

### Runtime evidence

- `ohmyzsh-git-alias/index`: view; 30-row paginated List, 120 actions, boundary-row actions verified.
- `models-dev/background-sync`: no-view; safe disabled-preference path completed.
- `parse-logs/parse-log-file`: view; Form now contains both `Form.TextField` and `Form.FilePicker`, with submit action.
- `voice-to-text-windows/select-mode`: view; stateful 5-row List with five actions.

### Verification

- Tauri backend esbuild completed successfully. The existing esbuild `require.resolve` externalization warning remains non-fatal.
- Full Vitest suite: 65 tests passed across 13 files.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint: no errors; existing Fast Refresh warnings remain in the Raycast API barrel.
- React Doctor: the iteration-local chained-iteration warning was fixed; the earlier `AgentChatView` JSX-prop warning remains.
- Live browser verification was unavailable because the Electron/Tauri app exposes no in-app browser tab. Do not count the screenshot feature as visually verified.

## Iteration 3 - 2026-06-20

### Fixed

- Extended the compatibility harness with repeatable refreshes, load-more, server search, and opt-in action invocation while preserving the last concrete view across `unchanged` refreshes.
- Implemented Raycast-style `useFetch`, `useExec`, and `useLocalStorage` utility hooks.
- Added `URLSearchParams` to the extension VM global environment.
- Component exceptions now return `ok: false` with normalized cross-realm error messages instead of silently becoming an empty successful Detail view.
- Fixed sectioned List pagination so all sections share the parent 30-row page limit. `devdocs` previously rendered 35 rows on its first page.

### Runtime evidence

- `devdocs/search-docsets`: network `useFetch` plus three effect/refresh cycles; server-search path executed; final sectioned List contains 30 total rows (5 preferred + 25 available), 108 actions, and `hasMore`.
- `diskutil/show-volumes`: `useExec` completed the safe permission check and read-only `/usr/sbin/diskutil list -plist` flow; loading settled without a false empty-success exception.
- `google-maven-repository/show-google-maven-repository`: network-backed List hydrated; final view has one dropdown plus 30 rows, 91 actions, and `hasMore`.
- `ohmyzsh-git-alias/index`: load-more operation verified 30→60 rows and 120→240 actions in one session (already counted in the prior 4/30 baseline).

### Classified failures not counted

- `cilium-docs/index`: current permissive `cheerio` range installs v1.2, whose ESM package no longer has the default export used by the extension. Resolve dependency/interop behavior before counting.
- `openstatus/show-monitors`: required preferences are unavailable to the external-package harness, causing initialization failure. Add isolated preference injection rather than using real user credentials.

### Verification

- Full Vitest suite: 67 tests passed across 13 files.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint and `git diff --check`: passed.
- Tauri backend esbuild: passed with the existing non-fatal esbuild externalization warning.
- Live screenshot/UI verification remains pending because no in-app browser tab is exposed.

## Iteration 4 - 2026-06-20

### Fixed

- Added isolated command preference overrides to the package runner and runtime harness, so external extensions can be tested without reading or mutating real user credentials.
- Added harness dependency reporting for requested and resolved versions, plus structured failure output and a `--preferences` JSON option.
- Added `mapResult` support to `useFetch`.
- Fixed legacy Cheerio default-import compatibility in both install-time builds and runtime fallback builds. The transform is deliberately limited to default imports from `cheerio`; CommonJS resolution now uses `require` and `node` conditions.
- Added standard web globals needed by modern dependencies to the extension VM, including `Blob`, `File`, `FormData`, events, messaging channels, Web Crypto, performance, base64 helpers, and structured cloning.
- Added `Grid.Fit.Fill`, `Grid.Fit.Contain`, `environment.canAccess`, and an explicit `AI` compatibility surface so dependent extensions can render or degrade predictably.

### Runtime evidence

- `openstatus/show-monitors`: isolated fake token preference; Zod requested `^3.24.1`, resolved `3.25.76`; initial view rendered, unauthorized requests settled cleanly to `isLoading: false` with no rows.
- `cilium-docs/index`: Cheerio requested `^1.0.0-rc.12`, resolved `1.2.0`; legacy default-import transform exercised; three refreshes settled to a 30-row List with 60 actions and `hasMore`.
- `whimsical/info`: Detail view rendered 1,871 markdown characters without actions.
- `rednote-viewer/view`: dependencies installed with pnpm lifecycle scripts disabled; isolated grid/cookie preferences exercised; invalid-cookie state rendered as a one-item Grid with one recovery action.

### Verification

- Full Vitest suite: 68 tests passed across 13 files, including the isolated-preference regression test.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint and `git diff --check`: passed.
- Tauri backend esbuild: passed with the existing non-fatal `require.resolve` externalization warning.
- Live screenshot/UI verification remains pending because no in-app browser tab is exposed.

## Iteration 5 - 2026-06-20

### Fixed

- Added provider-backed Raycast `AI.ask` and `useAI` compatibility. Extension AI requests use the configured action provider without tools, reject empty prompts, check provider availability, and time out after 60 seconds.
- Added `OAuthService.github`, `OAuthService.slack`, `getAccessToken`, and `withAccessToken` compatibility for personal access tokens. Missing tokens render an authentication-required Detail instead of crashing; full interactive OAuth remains pending.
- Made AI provider initialization lazy, avoiding provider/config startup overhead for extensions that do not use AI and allowing the standalone runtime harness to remain headless.
- Added standard VM `global`, `globalThis`, and `window` self-references plus React `useDebugValue`, fixing Sugar 2 and Zustand 3 initialization in `rsync-commands`.
- Corrected `environment.commandMode` and made explicit package preferences completely bypass the installed-extension preference registry.
- Fixed an action/navigation race where an initial asynchronous state update could rerender the root command and discard an `Action.Push` target.
- Hardened packaged-esbuild detection when Electron is importable but its `app` object is not initialized, as in standalone tooling.

### Runtime evidence

- `raydoom/index`: initial episode List rendered two sections and eight actions; invoking the first episode action rerendered the difficulty List with 15 actions.
- `rsync-commands/rsync-commands`: Sugar `2.0.6` and Zustand `3.7.2` initialized after the VM/hook fixes; LocalStorage hydration settled; invoking “Create new entry” pushed a 17-field Form with four actions.
- `image-to-ascii/convert-image-to-ascii`: Finder selection effect completed without sending a file; final Form contains five fields and one submit action.
- `slack-status/setStatus`: isolated invalid personal token exercised `OAuthService`, `withAccessToken`, and `getAccessToken`; the real Slack client rejected with `invalid_auth`, loading settled to false, and the List retained three sections and 37 actions.

### Verification

- Full Vitest suite: 71 tests passed across 13 files, including OAuth, AI hook/direct-call, and push-race regressions.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint and `git diff --check`: passed.
- Tauri backend esbuild: passed with the existing non-fatal `require.resolve` externalization warning.
- Live screenshot/UI verification remains pending because no in-app browser tab is exposed.

## Iteration 6 - 2026-06-20

### Fixed

- Added a structured runtime effect ledger for clipboard writes, URL opens, Finder reveals, toasts, HUDs, and AppleScript. Package-path runs can use record-only mode, allowing the compatibility harness to prove side effects without touching the desktop.
- Returned effects on view, no-view, and action results and capped each session ledger at 50 entries.
- Made shown `Toast` objects live: later `style`, `title`, and `message` mutations now update runtime feedback and effect evidence.
- Routed AppleScript through the record-only boundary, preventing extension tests from opening browser locations or synthesizing keystrokes.
- Fixed `getFrontmostApplication` to honor Raycast's non-null contract by returning a stable Raymes fallback when macOS lookup fails or returns an empty name.
- Added a package-scoped `OAuth.PKCEClient` surface with token persistence, expiry checks, token removal, authorization-request generation, and explicit interactive-authorization failure. This lets OAuth extensions load and reuse existing tokens while browser callback handling remains pending.

### Runtime evidence

- `open-in-shopify-admin/open-in-shopify-admin`: no-view command completed in record-only mode; emitted animated and failure toasts plus a recorded browser-inspection AppleScript, settling on “You don't have any website open” without controlling the desktop.
- `porkbun/ping`: real API call with isolated invalid credentials returned “Invalid API key”; recorded animated processing and failure toasts.
- `vercast/open-latest-deployment`: isolated invalid token reached the auth check; live toast mutation settled from animated loading to a failure toast titled “Invalid token”.
- `gmail/newwebmail` and `gmail/openinbrowser`: both no-view commands loaded the Google API dependencies and PKCE client, then degraded through recorded failure toasts with the explicit interactive OAuth limitation instead of crashing.
- Together with `models-dev/background-sync`, all six sampled no-view commands now have runtime execution evidence. Gmail counts once toward the 30-extension total.

### Verification

- Full Vitest suite: 73 tests passed across 13 files, including record-only effects and PKCE token persistence.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint and `git diff --check`: passed.
- Tauri backend esbuild: passed with the existing non-fatal `require.resolve` externalization warning.
- Live screenshot/UI verification remains pending because no in-app browser tab is exposed.

## Iteration 7 - 2026-06-20

### Fixed

- Added `--arguments` JSON support to the external extension harness and made action invocation target the latest concrete view after refreshes instead of the initial view.
- Changed the React shim to defer `useEffect` and `useLayoutEffect` callbacks until the complete runtime tree has rendered, while preserving dependency and cleanup behavior. This fixes temporal-dead-zone crashes in extensions that define effect helpers later in the component body.
- Added `setImmediate` and `clearImmediate` to the extension VM for Puppeteer/WebSocket compatibility.
- Protected Electron screen attachment capture with `BrowserWindow.setContentProtection(true)` before opacity hiding and a 120 ms compositor delay, then restored opacity and content protection in `finally`. This is intended to exclude the Raymes window from the attached screen image.
- Added a functional `Cmd+Shift+S` active-screen shortcut in an open AI chat and from Recent Chats. Recent Chats opens a fresh chat and captures immediately; both AI footers now show the “Attach screen” shortcut, and the button tooltip includes it.

### Runtime evidence

- `leetcode/daily-challenge`: public GraphQL request settled to a Detail with 2,281 markdown characters and three open/copy actions.
- `papago-translate/index`: isolated launch argument `initializeText=hello` plus same-language preferences exercised command arguments without credentialed network access; LocalStorage hydration produced one history row and five actions.
- `popcorn/search-streams`: clean LocalStorage run transitioned from loading List to the terms Detail; invoking the hydrated accept action persisted acceptance and rendered the main List with two sections and three actions. The earlier synchronous-effect exceptions no longer occur.
- `reverso-context/context`: required argument and full preference set launched its declared headless Chrome/Puppeteer path. Adding immediate timers removed the WebSocket crash; the bounded run remained in loading state and is still a follow-up performance/settling gap.

### Verification

- Full Vitest suite: 74 tests passed across 13 files, including deferred effect ordering.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint passed with four existing CommandBar hook-dependency warnings; `git diff --check` passed.
- Tauri backend esbuild passed with the existing non-fatal `require.resolve` externalization warning.
- React Doctor completed with 11 warnings: one existing CommandBar event-effect warning and ten JSX-prop performance warnings in the established Hint usage pattern.
- The content-protection change cannot be visually verified until the already-running main process reloads; do not count active-screen vision verification as complete yet.

## Next Loops

1. Continue from 23/30. The remaining sampled packages are `wistia`, `imgur`, `essay`, `memberstack`, `teslamate`, `github`, and `forgejo`; use isolated invalid credentials and wait for loading to settle before counting evidence.
2. Add interactive OAuth browser callback handling on top of the persisted PKCE client. Personal-token OAuth services and stored PKCE tokens work; first-time interactive authorization does not.
3. Treat the five menu-bar commands separately and add explicit menu-bar serialization/renderer behavior instead of silently treating them as ordinary views.
4. After the existing main process reloads naturally, live-test `Cmd+Shift+S` from Recent Chats and an open chat. Verify the protected capture excludes Raymes, then send it to a configured vision-capable model; also cover multi-display selection, Screen Recording denial, preview/removal, cancellation, and the 8 MB guard.
5. Add bridge tests with a fake Pi JSONL process for prompt rejection, image forwarding, timeout, abort, early process exit, malformed JSON, and stderr propagation.
6. Run the complete Vitest suite, both TypeScript projects, backend build, ESLint, and React Doctor after each substantive UI change. Do not start or restart the already-running app.

## Iteration 8 - 2026-06-20

### Fixed

- Connected Raycast List pagination to the actual scrollable list element. Mouse and trackpad scrolling now request the next page near the bottom, using the existing in-flight guard so repeated scroll events cannot issue duplicate loads.
- Fixed terminal startup commands being lost when the shell prompt arrived before `terminalCreate` resolved. The initial command and cwd now travel in the create request and are applied by the backend when the session starts.
- Replaced Bun's broken `node:child_process` terminal fallback with `Bun.spawn` and explicit stdout/stderr stream pumps. Tauri uses a pipe-safe login shell; Electron retains the real `node-pty` path. The renderer waits for Tauri event listeners before shell creation, Bun sessions translate xterm Enter from carriage return to newline, and pipe-mode input is echoed back to xterm.
- Added `/path/>` routing in the launcher. Typing `>` while browsing an absolute path switches to terminal mode, keeps that path as the working directory, and opens the built-in terminal there on Enter. Pasted `/path/>command` input follows the same path.
- Added `~` expansion to terminal cwd resolution and display the selected working directory in the terminal prompt/header.
- Fixed the Gemini/required-preference empty-view bug. The backend was correctly constructing `Tezbar.PreferenceSetup`, but then passed that internal node through the Raycast JSX walker, which discarded it and synthesized “This extension returned an empty view.” Internal Tezbar roots now serialize directly.
- Required extension preferences now always receive explicit first-run onboarding before command execution, even when a manifest supplies a placeholder/default. Saving the API key unlocks and immediately reruns the command. Setup copy is generic to the extension rather than incorrectly describing every setup as Google Translate.

### Runtime evidence

- A temporary installed extension with a required API-key preference returned `Tezbar.PreferenceSetup` on first launch; after saving the key, the same installed command returned its real `Detail` view.
- A real Bun JSONL backend probe created a terminal in the repository, returned the requested cwd, accepted simulated xterm keystrokes `l`, `s`, and Enter, echoed the input as `terminal:data`, and emitted the complete repository listing as a second `terminal:data` event. This covers the same sidecar/event protocol used by Tauri.
- Extension coverage remains 23/30; this iteration repaired shared runtime/UI behavior and does not claim a new real-extension sample.

### Verification

- Full Vitest suite: 75 tests passed across 13 files, including the new required-credential first-run/unlock regression.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint and `git diff --check`: passed with four existing CommandBar hook-dependency warnings and no errors.
- Tauri backend esbuild: passed with the existing non-fatal `require.resolve` externalization warning.
- React Doctor returned to the prior 11 warnings after removing the new cwd-only state render: one existing CommandBar event-effect warning and ten established JSX-prop performance warnings.
- Native live checks for mouse pagination, terminal interaction, and Gemini setup remain pending until the already-running process picks up the main/backend changes; it was not restarted per instruction.

## Next Loops

1. Live-test mouse/trackpad pagination on Uninstall Application after the running backend reloads; confirm one near-bottom request adds the next 30 rows and does not duplicate requests.
2. Live-test a blank built-in terminal, `>pwd`, `/Users/almatkairatov/Desktop/code/Raymes/>pwd`, pasted `/path/>command`, `~/>pwd`, resize, color output, and interactive Ctrl+C. Confirm the header cwd matches the shell cwd.
3. Remove Gemini's local preferences in an isolated copy, verify first launch renders the API-key setup (never the empty fallback), save an invalid isolated key, and confirm later Gemini commands skip setup and reach their normal API error view.
4. Continue from 23/30 with `wistia`, `imgur`, `essay`, `memberstack`, `teslamate`, `github`, and `forgejo`, recording actual settled runtime evidence for each.
5. Continue the existing screenshot vision verification, interactive OAuth callback, menu-bar serialization, and Pi bridge-test tasks from the prior loop.

## Iteration 9 - 2026-06-20

### Fixed

- Added VM implementations for `@raycast/utils` `useFrecencySorting`, `useForm`, `FormValidation`, and `useSQL`, including persistent ranking, form values/validation/submission, SQLite result loading, and promise `onData`/`onWillExecute` callbacks.
- Added `OAuthService.google` compatibility. Google Calendar now reaches Raymes's explicit authentication surface instead of crashing during module initialization; interactive Google OAuth remains incomplete.
- Made `useSQL` execute through the system SQLite CLI instead of a native Node addon, avoiding ABI mismatches between Node, Electron, and the Bun Tauri sidecar.
- Added Settings > Extensions. It embeds the two-pane extension catalog/manager and shows editable manifest preferences for installed extensions, including masked API keys/passwords, required fields, dropdowns, checkboxes, descriptions, and local save behavior.
- Added a typed `preferences:setup` bridge through Electron preload and Tauri so the settings UI reads actual manifest schemas and saved values.
- Hardened prebuilt extension installation: every compatible manifest command must have a matching `.sc-build/<command>.js`. Partial archives now fail validation and fall back to a source install instead of appearing successfully installed but failing at launch.

### Named extension runtime evidence

- `gif-search/favorites`: the previous `useFrecencySorting is not a function` crash is gone. The real installed extension hydrated five service groups and settled to a six-child Grid.
- `perplexity/ask-perplexity`: rendered a Form with a working submit action; submitting `raymes runtime test` recorded the correct `https://www.perplexity.ai/search?q=raymes+runtime+test` open effect.
- `google-chrome/search-history`: queried the real Chrome History database, discovered two profiles, and rendered 30 history rows with 150 actions. The prior `useSQL is not a function` crash is gone.
- `google-calendar/list-events`: `OAuthService.google` initializes successfully and the command renders the authentication-required Detail instead of throwing. This is compatibility evidence, not successful Google authorization.
- `speedtest/index`: the installed CLI command ran to completion and rendered its results List/Detail metadata with 27 actions; the prior command-level failure did not reproduce.
- `screenocr/recognize-text-fullscreen`: remains blocked. Its installed prebuilt archive contains only `preferences.js` although the manifest declares four commands. The new validator correctly rejected it as incomplete and attempted the source fallback, but GitHub's tree API returned HTTP 403 rate-limit exceeded. Upstream source was independently pulled and confirms all command entry files exist.

### Verification

- Full Vitest suite: 86 tests passed across 16 files, including a VM regression covering form, frecency, Google OAuth factory, SQLite loading, and async refresh.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint and `git diff --check`: passed with four existing CommandBar hook-dependency warnings and no errors.
- Tauri backend esbuild: passed with the existing non-fatal `require.resolve` externalization warning.
- React Doctor: no errors and 13 warnings. Existing CommandBar event/JSX-prop warnings remain; the extension manager is also flagged for component size and related state count.
- Deterministic sample coverage remains 23/30. The named extensions above are additional evidence and do not inflate the original sample count.

## Next Loops

1. Complete interactive OAuth callback/token persistence for Google Calendar and other `OAuthService` factories. The factory surface now loads, but an actual browser authorization round-trip is still required.
2. Make source installation resilient to GitHub API rate limits by adding a tarball/git fallback, then reinstall and runtime-test all four ScreenOCR commands, including Tauri screen capture/native OCR behavior.
3. Test Notion and GitHub from a clean preference state through Settings > Extensions: verify required token fields, masked storage, save/reload, invalid-token API responses, and cross-command reuse.
4. Split `ExtensionsView` into list, details, and preference editor components and consolidate its related state to clear the new React Doctor maintainability warnings.
5. Continue the deterministic sample from 23/30 with `wistia`, `imgur`, `essay`, `memberstack`, `teslamate`, `github`, and `forgejo`.
6. Finish active-screen vision-model verification, menu-bar serialization, Pi bridge failure tests, and the remaining terminal/live pagination checks before completion.

## Iteration 10 - 2026-06-20

### Fixed

- Added a last-resort sparse Git source installer against `almatkai/raymes-extensions`. It activates only when the prebuilt bundle and GitHub tree API paths fail, and checks out only the requested extension directory.
- Fixed Tauri source builds by externalizing the esbuild JavaScript API from `dist-backend/main.js` and guarding Electron-only `process.resourcesPath` access in the Bun sidecar.
- Made source installation atomic with command-count validation. An extension is no longer reported installed when `buildAllCommands` builds fewer commands than its manifest declares.
- Treat empty password preferences as credential onboarding even when an extension incorrectly marks the field optional. This fixes Notion and GitHub, whose tokens are operationally required but declare `required: false`.
- Rebuilt the user's installed ScreenOCR source package to contain all 4/4 command bundles. Entry resolution is repaired; native execution remains incomplete because `swift:../swift` does not yet expose Vision functions in Raymes.

### Runtime evidence

- ScreenOCR isolated Tauri install: rejected the incomplete prebuilt archive, encountered the expected GitHub API 403, sparse-cloned the fork, installed `raycast-cross-extension`, and built `preferences`, `recognize-text`, `recognize-text-fullscreen`, and `detect-barcode` (4/4).
- ScreenOCR fullscreen runtime: resolved and executed the rebuilt command, then proved the next blocker is the missing Swift bridge: `recognizeText is not a function`. Do not count ScreenOCR as runtime-compatible yet.
- Notion clean-state `search-page`: returned `Tezbar.PreferenceSetup` with the masked `notion_token` field plus extension and command preferences instead of entering an authentication-error view.
- GitHub clean-state `search-repositories`: returned `Tezbar.PreferenceSetup` with `personalAccessToken` and the complete preference schema. Saving an isolated invalid token unlocked the command; real viewer and repository GraphQL requests reached GitHub and returned HTTP 401 `Bad credentials`; refresh settled the List to `isLoading: false`.
- GitHub now counts toward the deterministic runtime sample, raising coverage from 23/30 to 24/30.

### Verification

- Full Vitest suite: 86 tests passed across 16 files. The credential regression now specifically covers an optional password field, matching Notion/GitHub manifests.
- Renderer and main TypeScript projects: passed.
- Tauri backend esbuild: passed without the previous bundled-esbuild warning.
- Targeted ESLint and `git diff --check`: passed with no new errors.
- Minimum iteration count is satisfied at 10/10. The automation remains active because runtime coverage, ScreenOCR native support, OAuth, and screenshot vision verification are incomplete.

## Next Task

Implement a Tauri/macOS native bridge for `swift:../swift` ScreenOCR exports (`recognizeText` and `detectBarcode`) using ScreenCaptureKit/Vision or a packaged Swift helper. Verify fullscreen OCR, selected-area OCR, barcode detection, clipboard output, Screen Recording denial, cancellation, and content-protected Raymes exclusion. Then continue the deterministic sample from 24/30 with `wistia`, `imgur`, `essay`, `memberstack`, `teslamate`, and `forgejo`.

## Post-loop compatibility continuation - 2026-06-20

The requested loop count remains exactly 10/10. This continuation does not increment it.

### Fixed

- Added a signed native macOS ScreenOCR helper using Vision text recognition and barcode detection. It supports fullscreen and interactive selected-area capture, recognition mode, language correction, custom words, language selection, line-break behavior, sound, and copying the captured image.
- Added `swift:`/`rust:` runtime exports for `recognizeText`, `recognize_text`, `detectBarcode`, and `detect_barcode`. The existing color-picker exports remain intact.
- Packaged the helper for both Electron and Tauri, added the Tauri sidecar path through `SCREENOCR_HELPER_PATH`, and included it in the native build chain.
- Made Electron and Tauri main/settings window content protection persistent instead of disabling it after each attachment. Electron fullscreen OCR also hides visible Raymes windows before invoking the helper.
- Kept active-screen opacity hiding and the compositor delay while no longer turning copyright/content protection back off afterward.

### Runtime evidence

- Native Vision OCR against the supplied terminal screenshot succeeded, recognized 76 characters, and included the expected word `terminal`. Screenshot contents were not printed.
- A generated CommonJS extension fixture using the real `require("swift:../swift")` shape invoked the packaged-helper bridge and recorded `recognized fixture text` reaching `Clipboard.copy`.
- The real installed `screenocr/recognize-text-fullscreen` command now resolves and calls `recognizeText`; the prior `recognizeText is not a function` failure is fixed.
- Direct harness fullscreen capture reached the helper but macOS denied display capture for the terminal-owned test process (`could not create image from display`). The command returned the extension's failure toast. This is denial-path evidence, not successful live fullscreen evidence, so ScreenOCR is not yet counted toward 30-extension runtime completion.

### Verification

- Signed Swift helper build: passed.
- Full Vitest suite before the bridge regression: 86/86 tests passed. Targeted extension-runner suite after adding it: 14/14 passed.
- Renderer and main TypeScript projects: passed.
- Tauri `cargo check`: passed.
- Tauri backend esbuild and `git diff --check`: passed.
- Targeted ESLint for `extension-runner.ts` and `ipc.ts`: passed. Including `index.ts` still reports its pre-existing line 38 CommonJS `require()` lint error; this pass did not introduce it.
- Persistent window protection and successful app-owned fullscreen capture still require live verification after the already-running process naturally reloads. The app was not restarted.

## Next Task

Live-run ScreenOCR from the reloaded Tauri app with Screen Recording granted. Verify fullscreen recognition copies text, the captured image excludes Raymes, selected-area OCR cancellation and success, barcode detection, and keep-image behavior. Then test `wistia`, `imgur`, `essay`, `memberstack`, `teslamate`, and `forgejo` to move deterministic coverage from 24/30 to 30/30; interactive OAuth and vision-model screenshot verification remain separate completion gates.

## Post-loop extension continuation - 2026-06-20

The requested loop count remains exactly 10/10. This continuation does not increment it.

### Fixed

- Added React `useSyncExternalStore` compatibility with snapshot comparison, deferred subscription, rerender signaling, and session cleanup. This fixes Zustand 5 initialization across all three Essay commands.
- Replaced the placeholder Axios shim with a real fetch-backed client supporting `axios.create`, `baseURL`, instance/default/request headers, query parameters, JSON bodies and responses, GET/POST/PUT/PATCH/DELETE, Axios-shaped HTTP errors, and Node-readable `responseType: "stream"` responses.
- Added an integration regression that runs an actual bundled extension fixture through `useSyncExternalStore`, an authorized relative Axios request, external-store notification, and rerender.

### Runtime evidence

- Pulled `wistia`, `imgur`, `essay`, `memberstack`, `teslamate`, and `forgejo` together through a sparse checkout and inspected every manifest, command, preference, and dependency before choosing the next samples.
- Installed Imgur through the real Tauri backend `extension:install` IPC. The incomplete prebuilt archive was rejected because it lacked `index`; the GitHub API hit the expected 403, sparse Git fallback succeeded, and both declared command bundles were built.
- `imgur/index`: rendered the two-field Upload Form and submit action. Invoking Submit with no media returned the expected failure toast without network or desktop side effects.
- `imgur/view-images`: hydrated local storage, transitioned from loading to a settled empty four-column Grid, and produced no runtime errors. Imgur counts as deterministic extension 25/30.
- Installed Essay through the same production Tauri IPC. Before the fix, `list-notes`, `new-essay`, and `new-note` all failed with `useSyncExternalStore is not a function`.
- `essay/new-essay`: now renders its markdown Form with Publish and Write in Browser actions.
- `essay/new-note`: now renders its note/folder Form, sends the configured invalid bearer token to `https://api.essay.ink/note-folders`, receives a real HTTP 401, clears loading, and records the failure toast. The earlier malformed relative-URL failure is fixed.
- `essay/list-notes`: the Zustand crash is fixed, but Raycast's paginated `usePromise(factoryReturningPageLoader, deps)` overload is not implemented. It currently treats the page loader as resolved data and does not request page zero. Essay remains partial and is not counted toward 30.

### Verification

- Full Vitest suite: 88/88 tests passed across 16 files.
- Targeted extension-runner suite after stream preservation: 15/15 passed.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint and `git diff --check`: passed.
- Tauri backend esbuild: passed.
- Tauri `cargo check`: passed.
- The already-running application was not started or restarted.

## Next Task

Implement Raycast's paginated `usePromise` overload end to end: invoke page zero, expose `pagination.hasMore`, connect mouse/keyboard load-more to the async next-page loader, merge results without duplicate requests, propagate errors, and reset on dependency/search changes. Verify it against real `essay/list-notes`, then count Essay only if all three commands remain stable. Continue with `wistia`, `memberstack`, `teslamate`, and `forgejo`; deterministic coverage is 25/30.

## Post-loop pagination continuation - 2026-06-20

The requested loop count remains exactly 10/10. This continuation does not increment it.

### Fixed

- Implemented Raycast's paginated `usePromise(factoryReturningPageLoader, dependencies)` overload. Raymes now invokes page zero, stores the page cursor and `hasMore`, merges page data, calls `onData`, surfaces `onError`, and resets pagination when dependencies change.
- Connected server pagination to the serialized List `__hasMore` contract used by both mouse/trackpad and keyboard loading.
- Added single-flight protection around the entire page fetch plus rerender transaction. Concurrent bottom-scroll events now share one request and one rerender instead of advancing or rendering twice.
- Preserved existing client-side 30-row pagination for ordinary Lists; server and client pagination are handled independently.
- Added session disposal cleanup for pagination state.

### Runtime evidence

- A bundled pagination fixture requested page zero, rendered two rows with `hasMore`, merged page one into four rows, ignored duplicate concurrent load-more calls, stopped after `hasMore: false`, and reset to page zero with only new results after a search dependency changed.
- `essay/list-notes`: now invokes its page-zero loader instead of resolving the loader as data. The real request reached `https://api.essay.ink/notes` with the isolated invalid bearer token, returned HTTP 401, and settled from loading to a stable List without a runtime crash.
- `essay/new-essay`: remains stable as a markdown Form with Publish and Write in Browser actions.
- `essay/new-note`: remains stable as a note/folder Form; its isolated invalid credential reaches the real folder endpoint, returns HTTP 401, clears loading, and records the failure toast.
- The installed `raycast.essay/list-notes` command, invoked through the production Tauri backend without preference overrides, returned `Tezbar.PreferenceSetup` with exactly one required password field named `apiKey`. The extension does not execute before the key is configured.
- Essay now counts as deterministic extension 26/30.

### Verification

- Full Vitest suite: 89/89 tests passed across 16 files.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint and `git diff --check`: passed.
- Tauri backend esbuild: passed.
- Tauri `cargo check`: passed.
- The already-running application was not started or restarted.

## Next Task

Install and runtime-test `wistia` and `memberstack` through production Tauri IPC. Verify clean-state required password onboarding, save isolated invalid credentials, execute every declared command, wait for real API failures to settle, and fix shared runtime gaps before counting either package. Then cover `teslamate` and `forgejo`; deterministic coverage is 26/30. ScreenOCR live capture, interactive OAuth, and vision-model screenshot proof remain completion gates.

## Post-loop credentialed extension continuation - 2026-06-20

The requested loop count remains exactly 10/10. This continuation does not increment it.

### Fixed

- Made the Axios compatibility default export callable as `axios(config)`, matching Axios CommonJS/default-import behavior used by the Memberstack Admin SDK. Method calls and `axios.create()` remain available on the same function object.
- Matched Axios GET/HEAD semantics by ignoring configured request data for those methods instead of passing an illegal body to native `fetch`.
- Extended the Axios integration regression to cover `axios.create().get()`, callable `axios(config)`, authorization headers, JSON responses, and ignored GET data.

### Runtime evidence

- Installed Wistia and Memberstack through the production Tauri `extension:install` IPC. Registry indexing returned both Wistia commands (`my-medias`, `my-projects`) and Memberstack's `manage-members` command.
- Clean-state Tauri command execution blocked both Wistia commands behind `Tezbar.PreferenceSetup` with the required masked `wistiaApiToken` field.
- Clean-state Memberstack execution blocked behind `Tezbar.PreferenceSetup` with the required masked `secret_key` field.
- `wistia/my-medias`: isolated invalid credentials reached the real Wistia account/media requests, settled from loading to a one-item empty List with Add Media action, and recorded the extension's “Invalid Credentials” failure toast.
- `wistia/my-projects`: isolated invalid credentials reached the real Wistia account/project requests, settled to an empty non-loading List, and recorded the same specific credential failure. Wistia counts as deterministic extension 27/30.
- `memberstack/manage-members`: initially failed inside the SDK because the Axios default was not callable, then failed because GET data was forwarded as a fetch body. After both shared fixes, the SDK reached Memberstack's real API, settled loading, and recorded “The provided secret key is invalid.” Memberstack counts as deterministic extension 28/30.

### Verification

- Full Vitest suite: 89/89 tests passed across 16 files.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint and `git diff --check`: passed.
- Tauri backend esbuild: passed.
- Tauri `cargo check`: passed.
- The already-running application was not started or restarted.

## Next Task

Install TeslaMate and Forgejo through production Tauri IPC to reach deterministic coverage 30/30. For TeslaMate, verify all three required settings and both commands against an isolated unreachable local Grafana URL with bounded settling. For Forgejo, verify clean-state URL/token onboarding and all three commands against an isolated invalid Codeberg token. Fix shared URL, Axios, pagination, or action gaps before counting them. Do not declare completion afterward until ScreenOCR live capture excludes Raymes, a vision-capable model verifies the attached image, interactive OAuth works, and all completion gates are evidenced.

## Post-loop 30-extension continuation - 2026-06-20

The requested loop count remains exactly 10/10. This continuation does not increment it.

### Runtime evidence

- Installed TeslaMate and Forgejo through the production Tauri `extension:install` IPC. Registry indexing returned TeslaMate's `overview` and `charges` commands plus Forgejo's `repo-search`, `notifications`, and `user-repo-search` commands.
- Clean-state Tauri execution blocked both TeslaMate commands behind `Tezbar.PreferenceSetup` with all three required fields: `tmGrafanaUrl`, `dataSourceUuid`, and masked `saToken`.
- Clean-state Tauri execution blocked all three Forgejo commands behind `Tezbar.PreferenceSetup` with required `serverUrl` and masked `accessToken` fields.
- `teslamate/overview`: isolated settings targeted closed local port 9. The real POST query failed immediately, loading settled, and the command rendered its explicit “Failed to fetch TeslaMate overview” Detail.
- `teslamate/charges`: the same bounded Grafana failure exercised its query handler, settled loading, and rendered the extension's TeslaMate failure Detail. TeslaMate counts as deterministic extension 29/30.
- `forgejo/repo-search`: the real Codeberg repository-search endpoint returned HTTP 401 for the isolated invalid token; the List retained its six-option search dropdown and settled loading.
- `forgejo/notifications`: the real Codeberg notifications endpoint returned HTTP 401 after 2.3 seconds; polling observed the in-flight request, then the List settled to non-loading.
- `forgejo/user-repo-search`: the real Codeberg user-repositories endpoint returned HTTP 401; the List retained its All/Starred dropdown and settled loading. Forgejo counts as deterministic extension 30/30.
- The 30-extension deterministic runtime evidence requirement is satisfied. This is not overall completion evidence.

### Verification

- Full Vitest suite: 89/89 tests passed across 16 files.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint and `git diff --check`: passed.
- Tauri backend esbuild: passed.
- Tauri `cargo check`: passed.
- The already-running application was not started or restarted.

## Next Task

Use the naturally reloaded running Tauri app to execute ScreenOCR fullscreen capture with Screen Recording granted. Prove recognized text reaches the clipboard, capture/attachment images exclude every Raymes window through persistent content protection, selected-area cancellation and barcode flows settle, and a configured vision-capable model correctly describes a known non-Raymes screen marker from the attached image. Then implement and verify the first-time OAuth browser callback/token exchange for Google Calendar. Only pause the automation after those remaining gates and the final checks are recorded.

## Post-loop Google OAuth continuation - 2026-06-20

The requested loop count remains exactly 10/10. This continuation does not increment it.

### Fixed

- Replaced the Google `OAuthService` personal-token placeholder with an interactive loopback PKCE authorization flow. It now opens the provider authorization URL, validates a random state value, receives the code on a dynamic `127.0.0.1` callback port, exchanges it with a PKCE verifier, and invokes the extension's `onAuthorize` callback.
- Persisted Google access and refresh tokens in extension support storage with expiry metadata. Valid access tokens are reused, expired tokens can be refreshed, and `removeTokens` clears the stored session.
- Made `withAccessToken` await Google authorization before rendering the wrapped command. GitHub and Slack retain their existing personal-access-token behavior.
- Added a bounded authorization timeout and an immediate explicit failure in record-only harness mode so headless Google Calendar checks cannot hang waiting for a browser callback.

### Runtime evidence

- A local end-to-end OAuth server verified the generated client ID, S256 PKCE challenge, state, loopback redirect URI, callback handling, code exchange, refresh-token persistence, and second-run access-token reuse without reopening authorization.
- The real installed Google Calendar `list-events` bundle reached the new Google OAuth path. In record-only mode it terminated immediately with `Interactive OAuth requires system effect mode`, proving the production bundle no longer reaches the old undefined `OAuthService.google` or passive token-gate path.
- This is protocol and integration evidence, not a claim that a real Google account consent round trip has completed in the live app.

### Verification

- Full Vitest suite: 90/90 tests passed across 16 files.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint and `git diff --check`: passed.
- Tauri backend esbuild: passed.
- Tauri `cargo check`: passed.
- The already-running application was not started or restarted.

## Next Task

Use the naturally reloaded Tauri app to complete Google Calendar authorization through real Google consent, verify the loopback callback, persisted token reuse, and a real calendar List. Then complete the remaining ScreenOCR fullscreen/selection/barcode, persistent content-protection, clipboard, and vision-model attachment proofs. Pause the automation only after those live gates and final checks pass.

## Post-loop ScreenOCR visibility continuation - 2026-06-20

The requested loop count remains exactly 10/10. This continuation does not increment it.

### Fixed

- Fixed the Tauri Electron shim's `app.hide()` and `app.show()` methods. They previously did nothing, so the ScreenOCR runner's 120 ms hide delay could still capture Raymes even though the backend believed every window was hidden.
- Added a backend `app_visibility` control message. The Tauri host records exactly which Raymes webview windows are visible, hides all of them before capture, and restores only that set afterward instead of exposing previously hidden windows.
- Preserved startup content protection for the main window and creation-time protection for Settings; fullscreen ScreenOCR now combines persistent protection with real host-window hiding.
- Added an explicit native Screen Recording preflight failure. ScreenOCR now directs the user to the correct macOS Privacy & Security pane instead of returning the generic `Failed to capture an image` error.
- Added a regression test proving the Tauri shim emits ordered hide and restore requests.

### Runtime evidence

- Rebuilt and ad-hoc signed the native ScreenOCR helper.
- Ran Vision OCR against the supplied terminal screenshot through the real helper's `imagePath` path. It returned the visible prompt and directory names, including `Raymes`, `model-runner`, `rezka_load`, and `Hide window`, proving native image decoding and text recognition work.
- Ran a real fullscreen capture through the helper. macOS denied Screen Recording access, and the helper now returned the actionable permission message with the exact System Settings location. Fullscreen capture, clipboard, exclusion, and vision-model proof remain blocked until the OS permission is granted to the running app.
- No app terminal was attached to this automation thread and no matching live Tauri/backend process was visible from the shell, so this continuation does not claim live UI proof or restart the app.

### Verification

- Full Vitest suite: 91/91 tests passed across 17 files.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint, Rust formatting, and `git diff --check`: passed.
- Tauri backend esbuild: passed.
- Tauri `cargo check`: passed.
- The application was not started or restarted.

## Next Task

Grant Raymes Screen & System Audio Recording access in macOS, then use the running app to execute ScreenOCR fullscreen recognition and prove the copied text excludes all Raymes windows. Verify selected-area cancellation and barcode detection, followed by an AI screen attachment that a configured vision-capable model correctly describes. Complete the real Google Calendar consent round trip and persisted-token reuse before pausing the automation.

## Post-loop command preference continuation - 2026-06-20

The requested loop count remains exactly 10/10. This continuation does not increment it.

### Fixed

- Fixed Settings > Extensions omitting every command-specific preference. The panel previously requested only extension-level setup, so API options and command behavior fields declared under `commands[].preferences` were invisible.
- Preserved preference scope in the backend setup contract with `commandName` and `commandTitle` metadata.
- Settings now loads the extension scope and every declared command scope concurrently, shows command fields with compact command badges, and keeps same-named fields isolated by scope.
- Saving now partitions values into extension-wide and command-specific records before issuing concurrent persistence calls. Runtime onboarding remains command-scoped and unchanged.
- Extended the preference onboarding regression to verify command scope metadata alongside extension-wide credential fields.

### Runtime evidence

- Audited real installed SuperCmd-compatible manifests. The old Settings path omitted Brew's `showMetadataPanel` and `withoutThreshold`; Color Picker's `showColorName`, `primaryAction`, and `colorNamesPerGroup`; and Port Manager's `primaryPortAction`. All are now discoverable through their command setup requests.
- The regression fixture returned its global `apiKey` separately from the `Index` command's `resultLimit`, including the command name/title needed by the editor, while credential onboarding still blocked and unlocked normally.

### Verification

- Full Vitest suite: 91/91 tests passed across 17 files.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint and `git diff --check`: passed.
- React Doctor: the new sequential setup-request warning was fixed; remaining warnings are existing component-structure/performance debt and one load-error state batching warning.
- Tauri backend esbuild: passed.
- Tauri `cargo check`: passed.
- The application was not started or restarted.

## Next Task

After Screen Recording access is granted, complete the live ScreenOCR fullscreen, clipboard, exclusion, selected-area, barcode, and vision-model attachment proofs. Complete real Google Calendar consent and token reuse. In parallel, split the large Extensions preference editor into a focused component/reducer to address React Doctor's state batching and maintainability warnings without changing its new scoped persistence behavior.

## Post-loop preference editor refactor - 2026-06-20

The requested loop count remains exactly 10/10. This continuation does not increment it.

### Fixed

- Extracted the extension preference editor from the 600-line Extensions view into a focused component with a single reducer for loading, editing, saving, success, and failure states.
- Removed the prior load-error path's multiple React state updates. Each async outcome now commits one editor transition, and load failures render inline beside the affected preferences.
- Moved scope merging and save partitioning into a pure model module so same-named command fields have explicit, independently testable behavior.
- Kept all preference setup requests and persistence writes concurrent, command-scoped, and visually consistent with the existing compact Settings design.
- Split pure helpers from the React component to preserve Fast Refresh and eliminate the touched-file lint warnings.

### Runtime evidence

- A new scope model fixture merged a global `token` with `search.limit` and `recent.limit` without collisions, then partitioned edited values back into one extension record and two command records.
- Existing onboarding and extension runtime coverage remained stable after the component extraction.

### Verification

- Full Vitest suite: 93/93 tests passed across 18 files.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint and `git diff --check`: passed with no touched-file warnings.
- React Doctor no longer reports the preference editor's sequential-await or multiple-state-update warnings; total changed-file findings decreased from 15 to 13. Remaining findings are the larger Extensions shell, existing CommandBar event flow, and existing JSX-prop allocations.
- Tauri backend esbuild: passed.
- Tauri `cargo check`: passed.
- The application was not started or restarted.

## Next Task

Complete the live gates once macOS Screen Recording access is available: fullscreen OCR clipboard output, Raymes-window exclusion, selected-area cancellation, barcode detection, and vision-model screenshot description. Complete a real Google Calendar consent callback and token-reuse run. For code-only work, extract the Extensions catalog/detail shell and consolidate its eight related state values with a reducer while preserving keyboard and hover selection behavior.

## Post-loop extension catalog reducer - 2026-06-20

The requested loop count remains exactly 10/10. This continuation does not increment it.

### Fixed

- Consolidated the Extensions catalog's eight related state values into one reducer: query, loading, install progress, store results, installed results, selected extension, follow-selection mode, and status message.
- Made store reload success/failure atomic. A successful response commits both lists and clears loading in one transition; failures clear loading and expose an actionable message.
- Made hover, click, and keyboard selection update the selected ID and follow mode together, preventing transient mismatches in GlideList behavior.
- Made install start, progress completion, success, and failure remove/update progress and messages through single reducer transitions.
- Added pure reducer coverage for load completion, selection mode, install completion, and 100% progress cleanup.

### Runtime evidence

- Reducer fixtures proved load completion cannot leave the catalog loading, install completion clears its progress entry while retaining selection mode, and a 100% native progress event removes its entry.
- Existing keyboard navigation, hover selection, reload, scoped preferences, and extension runtime tests remained stable.

### Verification

- Full Vitest suite: 96/96 tests passed across 19 files.
- Renderer and main TypeScript projects: passed.
- Targeted ESLint and `git diff --check`: passed.
- React Doctor removed the catalog's many-related-state warning; changed-file findings decreased from 13 to 12 and the score increased from 9 to 10. The only Extensions-specific finding left is the 449-line detail shell.
- Tauri backend esbuild: passed.
- Tauri `cargo check`: passed.
- The application was not started or restarted.

## Next Task

Extract the Extensions detail/preview/resources view into focused components so the catalog shell falls below React Doctor's large-component threshold without changing selection, install, or preference behavior. Live completion still requires Screen Recording permission for OCR/vision proof and a real Google Calendar consent/token-reuse run.
