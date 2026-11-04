# Runtime review recovery notes at 118cb7b

The mandatory Claude Opus 5.5 runtime re-review stopped at its account rate limit before producing a final report. These are coordinator recovery notes from its fixture probes, not a completed reviewer verdict. The wire and standards re-reviews are preserved separately, unchanged.

The probe snapshot is `/private/tmp/tezbar-runtime-probe-118cb7b`. Its tests use the real React components and AI SDK with mocked native IPC.

## U1: deleting a running conversation can recreate it

The additional `PROBE delete of a running conversation when native done overtakes the delete reply` in `src/renderer/agentChat/ownership.test.ts` fails: the session exists again after deletion. Native deletion cancels the run first; a `done` event can precede the delete response, while renderer completion persistence still owns the session. Fix renderer ownership before awaiting deletion, coordinate persistence/cancellation, and preserve failure recovery. Verify the late event cannot recreate the session or contaminate another conversation. Check clear-all for the same race.

## U2: extension-owned providers expose misleading edit controls

The `PROBE contributed provider form ownership` in the snapshot's `SettingsView.knowledge.test.ts` supplies a provider with `_extensionId: 'team'`. Settings still shows Remove provider, a key field labelled as local config, and editable provider/model protocol controls. Native filtering correctly prevents persisting extension-owned data, so these controls cannot reliably do what they advertise. Trace the spec's ownership contract, show the extension owner and direct edits to AI Extensions; preserve useful selection behavior. Verify ordinary user-defined providers stay editable and disabling an extension removes its contributions.

## U3: Settings autosave uses the render before the edit

The `PROBE builtin key typed once` types/pastes a fixture OpenAI key and advances timers. The input contains the new key but the only saved patch still contains the old empty key, including after unmount. `setAiApiKey(value); persistAiSettings()` builds the patch from the old render. The same pattern appears for base URL and several model mutations. This behavior predates the migration, but it is on the delivered native setup path and must be fixed. Audit these call sites, debounce current values or pass complete explicit overrides, preserve provider ownership and flush real pending edits on close without spuriously saving initial hydration. Add real-component tests for a single paste, typing, provider switch, model edits and closing with a pending save.

## Closure still required

The reviewer also inspected original R1–R20 and P3 items, but no final disposition was emitted. A fresh Opus 5.5 runtime review must check them against the final source; these recovery notes do not replace that gate. Do not claim live provider interoperability or desktop visual QA from these fixture tests.
