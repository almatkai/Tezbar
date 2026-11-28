# Native launcher prototype — local review handoff

This is a runnable, isolated GPUI launcher backed by the current Rust core. The
[2026-11-05 measured decision](../../docs/performance/native-launcher-matched-7434ba1-20261105.md)
records 20/20 valid background-idle runs and Windows absolute B/C observations. The
original strict CPU gate failed; parent review and complete input/IME/accessibility QA
remain open. The final measured artifact source is 7434ba1, harness 0c00d26. Historical
handoff sections below retain earlier snapshots; use the report artifact manifest for
these measurements. No push, PR, merge, release or Actions run.

## Build and run on this Mac

Work only in `Tezbar-native-rust-agent`, branch `feat/native-launcher-prototype`.
Read `../../docs/MACOS_SIGNING.md` before building or sealing bundles. The normal app
keeps Rust 1.85 and its original lockfile. The prototype's own toolchain is 1.98.1;
its official GPUI rev is `c84af5b99cbc366d675064e52b23664a015b6e35`.

```sh
# From the selected worktree root; no default toolchain change.
set -e
export CARGO_TARGET_DIR="$PWD/prototypes/native-launcher/target"
cargo +1.98.1 build --release --locked --manifest-path prototypes/native-launcher/Cargo.toml --features bench-control
/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/lock-drift.py > prototypes/native-launcher/evidence/lock-drift.json

# Generate a fresh deterministic 20,000-file profile. Never use the real profile.
prototype_root="$(/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/fixture.py)"
/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/stage-resources.py "$prototype_root"
prototypes/native-launcher/scripts/bundle-macos.sh --local-preview "$prototype_root"
/opt/homebrew/bin/python3 - "$prototype_root" <<'PY'
import pathlib,secrets,sys
p=pathlib.Path(sys.argv[1])/'control-token';p.write_text(secrets.token_hex(32));p.chmod(0o600)
PY
"$prototype_root/Tezbar Launcher Prototype.app/Contents/MacOS/tezbar-launcher-prototype" \
  --show --theme dark --profile "$prototype_root" --resources "$prototype_root/resources" \
  --control-token-file "$prototype_root/control-token"
```

Run from a second terminal, with that exact `prototype_root`:

```sh
/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/control.py "$prototype_root" status
/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/control.py "$prototype_root" query --text saf
/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/control.py "$prototype_root" hide
/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/control.py "$prototype_root" show
/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/control.py "$prototype_root" quit
```

To try the measured 7434ba1 prototype without rebuilding, create a new disposable
profile and local-preview bundle from its preserved binary. This opens no control
listener; quit with Cmd-Q. The installed app is unaffected.

```sh
prototype_binary="$PWD/prototypes/native-launcher/evidence/artifacts-7434ba1/mac/tezbar-launcher-prototype"
prototype_source="$("$prototype_binary" --version | /opt/homebrew/bin/python3 -c 'import json,sys; print(json.load(sys.stdin)["source"])')"
prototype_root="$(/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/fixture.py --source "$prototype_source")"
/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/stage-resources.py "$prototype_root"
prototypes/native-launcher/scripts/bundle-macos.sh --local-preview "$prototype_root" --binary "$prototype_binary"
"$prototype_root/Tezbar Launcher Prototype.app/Contents/MacOS/tezbar-launcher-prototype" \
  --show --theme dark --profile "$prototype_root" --resources "$prototype_root/resources"
```

`--theme light|dark|system` is a launch setting. `--headless` takes the same arguments
and starts the same core before any GPUI application is constructed. Without
`bench-control` there is no control listener. Controls are bounded JSON-lines with a
profile-local random token; Unix socket mode 0600, Windows local-only owner-DACL pipe.
No raw core commands or JS execution are exposed by the prototype. Activation rechecks
the private action envelope even if a Rust caller edits the public support projection. `query` replaces the
field; `type` inserts through `EntityInputHandler::replace_text_in_range`. Both reply with
`expected_generation`: wait for that generation and `painted_generation` before measuring.
`status` includes scan readiness, counts/row IDs, generations and timing, without returning
clipboard text. Held Enter, pending-generation Enter and duplicate activation are guarded.

To use an already built older artifact after the branch advances, pass its `--version`
`source` value to `fixture.py --source <sha>`, and use resources bearing the same SHA.
Do not relabel resources from a different build. `stage-resources.py` without
`--from-resources` is a functional smoke convenience, not proof of matched A resources.
For measurement, point B/C directly at the corresponding A bundle's read-only
`Contents/Resources` (and use `--from-resources` to save its resource manifest).

After quitting, scrub only marked temporary fixture clipboard stores with the existing
`docs/performance/desktop.py::scrub_fixture_clipboard`; the measurement harness does this
in `finally`, after waiting for the owned process. Do not inspect those stores. Never
change HOME, TCC, Vault, Keychain, real provider settings or the installed application.

## Verification performed

Evidence is ignored under `evidence/`; old reports/data under `docs/performance` are unchanged.

| Check | Result / evidence |
| --- | --- |
| M0 official GPUI release, CLT, runtime shaders + font-kit | Built and real native text window observed via CUA; `m0-build.log` |
| Main default backend tests, Rust 1.85 | 609 passed, 9 existing ignored; `default-test-bounded.log` |
| Facade/full backend tests, Rust 1.85 | 611 passed, 9 existing ignored; `facade-test-handoff.log` |
| Rust 1.85 checks, default and facade | Passed; `msrv-default-handoff.log`, `msrv-facade-handoff.log` |
| Prototype session/control tests | 7 passed, no ignored; `prototype-test-quit-cleanup.log` (6 session tests plus Unix authentication) |
| Prototype release without controls | Built; `prototype-default-build-final.log`, `prototype-default-version-final.json` confirms `bench_control:false` |
| Main `pnpm typecheck` | Passed; `typecheck.log` |
| Main `pnpm test` | 744 passed, 7 existing skipped; `pnpm-test.log`; no new skips |
| Main frontend artifact for A | Built from unchanged renderer; `a-frontend-build.log` |
| Instrumented A release | Built locally with `desktop-bench,tauri/custom-protocol`; `a-build-404429e.log` |
| A/B/C functional smokes | Initial matched-resource paths: `mac-A5-smoke.json`, `mac-B2-A-resources-smoke.json`, `mac-C-smoke.json`; UI snapshot B/C: `mac-handoff-B-smoke.json`, `mac-handoff-C-smoke.json` |
| Exactly-once activation | B synthetic `add-note`: one `click_events` record; no clipboard write |
| Final normal quit / same-profile relaunch | Passed twice at f387675, 80 results each, endpoint removed after each quit; `mac-final-relaunch.json` |
| Representative CUA screenshots | `mac-dark-handoff-7617803.png` (current UI snapshot), `mac-light-results.png` (source 404429e); `mac-dark-core-failed-fixed.png` for startup failure |
| Installed beta | Path/hash recorded; unchanged binary hash and no owned prototype survivors after successful smokes. Initial installed PID 6376 later became 74254 during this long task; PID continuity is **not** claimed. No installed-app stop/relaunch/write was issued. |

The default test runner initially hit 2-second subprocess deadlines under high parallelism.
The single failing subprocess test passed alone, then the entire suite passed with four
test threads. No test was skipped or changed. The facade suite also passed with four threads.

`src-tauri/Cargo.lock` SHA-256 must remain
`a5d44a7ff003aa70e8892768f5a9502179d6657bd7cc12215fa6ed48f9a3f728`.
The prototype lock was seeded from it. Resolving GPUI required targeted futures/wasm/TOML
updates and an official registry `proc-macro-crate =2.0.0` constraint for Linux resolution
(2.0.1–2.0.2 pin an incompatible TOML dependency). No fork or upstream source patch.
The core watchlist versions are unchanged, but **shared futures crates move 0.3.32 →
0.3.34**. Other shared name/version-set additions are all listed in `lock-drift.json`;
extra platform versions do not necessarily replace the core's selected version. This is
a measurement confound; do not claim the complete dependency graphs are identical.

## Measurement handoff

Initial A/B/C smokes used source `404429e706e9fa22eb677f205655d6ce3573fea4`, fixed seed 65001,
20,000 generated files, 50 notes, 50 fixture snippets and 20 quick links. A and B had the
same observed database counts: 235 applications, 14,997 files, 50 notes, 79 snippets
(including built-ins), 20 quick links, 10 commands and 41 native commands. Record actual
counts on every run rather than assuming all generated files are eligible for indexing.

The B/C smoke binaries at the UI handoff snapshot are clean source `76178030aacc7a8c18970a99f0e2a042d8d11018`.
The final code artifact is clean source **f3876757bc6f9d7578789273ac4f2c44ce4e42f4**.
Its explicit control-endpoint quit cleanup is verified by a same-profile relaunch
smoke. `evidence/HANDOFF.json` gives each final artifact’s exact source and hash. The
B at that snapshot repeats safe add-note activation and records exactly one usage event. It uses
locally staged read-only resources (`matched_A:false`), not a newly built matched A.
The earlier B smoke directly used A’s read-only resources.

The initial A smoke used Rust 1.85 and B/C used 1.98.1; this compiler difference is
another confound. Use 1.98.1 for **both** builds in the final measurement, while
keeping the separate 1.85 MSRV checks for the production crate.

These short smokes **do not close a memory/CPU/latency gate**. They have a 2-second warmup
and three samples, no stabilization, no repetitions and no cold-shader launch. They exist
to verify the control, accounting, activation, fixture and cleanup paths. Do not use the
old empty-profile 121.925 MiB reference as A or infer a migration ratio from these smokes.

Service differences must accompany any future numbers:

| Service | A: Tauri | B: GPUI | C: headless |
| --- | --- | --- | --- |
| NativeCore, app/file scan, watcher, refresh, clipboard observer, background Knowledge | yes | yes | yes |
| Hotkey / tray / updater startup | yes (fixture F19 hotkey) | no | no |
| WebView | yes | no | no |
| GPUI renderer | no | runtime-shader Metal / DirectX | no |
| Linked Tauri/WebKit on Mac | yes | yes, no WebView | yes, no WebView |
| Trim 5 seconds after hide | yes | yes | never shown |

Mac `measure.py` imports the preserved all-process physical-footprint/Mach-timebase
accounting from `desktop.py`, including responsibility-owned XPC with PPID 1. RSS is
secondary. Startup sampling records observed compiler helpers; absence in samples is not
proof a short-lived service never existed. The first M0 shader launch was not sampled;
all subsequent evidence is cache-warmed. `vmmap -summary` and `footprint -p` logs are in
the fixture root for each observed PID; no heap contents or process command lines are read.

Focus loss or unexpected window visibility invalidates a run; do not interact with
the desktop during a timed workload. A failed focus-interfered smoke was retained
(`mac-final-B-smoke.log`), then the repeat passed (`mac-final-B2-smoke.json`).

Full run defaults: 120-second warmup, 30 stable one-second CPU intervals (mean <2%,
p95 <5%), then 60 seconds of samples. `matched.py` runs C, then A B B A A B per condition,
then C, with fresh byte-identical fixture copies and manifest checks. It does not issue a
verdict. Open control receipt includes socket/poll overhead and is labeled separately
from OS CoreGraphics visibility polling. Input echo/results use committed input through
content plus following frame callback; not photon time. Core timing begins inside Tokio
immediately before the facade call. Tauri IPC timing remains labeled IPC. The retained A
workload reuses the preserved responsiveness harness, not new arbitrary frontend dispatch.

```sh
# From clean selected worktree, after rebuilding both binaries at the same final SHA:
pnpm build:frontend
/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/build-a.py
export CARGO_TARGET_DIR="$PWD/prototypes/native-launcher/target"
cargo +1.98.1 build --manifest-path prototypes/native-launcher/Cargo.toml --locked --release --features bench-control
prototype_template="$(/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/fixture.py)"
/opt/homebrew/bin/python3 prototypes/native-launcher/scripts/matched.py \
  --template "$prototype_template" --a-binary "$PWD/prototypes/native-launcher/target/tauri-a/release/tezbar" \
  --b-binary "$PWD/prototypes/native-launcher/target/release/tezbar-launcher-prototype" \
  --output "$PWD/prototypes/native-launcher/evidence/matched-final"
```

Use fresh output directories. The complete matrix takes over an hour; review the code
and representative UI before spending that time. Preserve every raw run, failure and
per-process map. The old reference artifacts stay immutable.

## Windows handoff

The authorized host is `User@100.77.254.64` (`DESKTOP-OB83S8J`). T3 environment links
were unavailable; SSH was the authorized fallback. Its logged-in console session was
Active, but no remote CUA surface was available. No interactive DirectX/IME/focus result
is claimed. `C:\Users\User\Desktop\Tezbar` was never used or edited.

Task staging (all remote build/run paths below are within this directory):

```text
C:\Users\User\AppData\Local\Temp\tezbar-native-launcher-91e690194efa440ca6c7a3f874856a32
```

Rust 1.98.1 was installed user-level without changing the default. The existing Visual
Studio environment and SDK 10.0.22621 `fxc.exe`/`rc.exe` were used. The verified pre-cleanup git
archive `source-7617803.tar.gz` has SHA-256
`d99acb575bd55c70028cc73ae1ba462c74a854a98aee2619522e219276ddab81`;
`source-7617803-manifest.json` verifies all 1,683 tracked source files. The private
`cargo-home` and `target-1-98-1` are task-owned. No global SDK was installed.

Final source archives and source-file hash manifests are retained in staging and listed
in `evidence/HANDOFF.json`. The task build script points at that final archive.

The Windows default/facade backend checks passed on **both 1.90 and 1.98.1**, and the two
facade integration tests passed on 1.98.1 (no ignored tests). See
`evidence/windows-backend-repaired-host.log`. Those checks used source 79821f2; the
production seam has no changes after that snapshot.

Final source f387675 debug/release and all six session tests passed, with no ignored tests
(`windows-build-final-host.log`). The same final source headless C passed, including rejection of an
unauthenticated quit, 80 results for `doc`, 14,997 indexed files, all fixture stores and
exit 0 (`windows-headless-final.json`). The artifact path/hash and empty owned-process
audit are in `windows-artifact-final.json`. Three-second PrivateUsage/WorkingSet/CPU
samples (`windows-C-metrics-final.json`) are **collector smokes**, without
stabilization or a matched A. Cached generated SQLite outputs needed targeted task-cache
invalidation for the final rebuild; retained failure logs are not presented as successes.

From SSH, rerun the exact verified staged build (this script verifies the archive before
writing changed source files; logs stay in staging):

```powershell
$prototypeStage = 'C:\Users\User\AppData\Local\Temp\tezbar-native-launcher-91e690194efa440ca6c7a3f874856a32'
powershell -NoProfile -ExecutionPolicy Bypass -File "$prototypeStage\windows-build-handoff.ps1"
& "$prototypeStage\target-1-98-1\release\tezbar-launcher-prototype.exe" --version
```

For a new isolated headless run of that artifact (fixture generation, named-pipe auth
negative check, ready/query, metrics, quit and marked clipboard-store cleanup):

```powershell
& 'C:\Users\User\AppData\Local\Programs\Python\Python313\python.exe' "$prototypeStage\windows-headless-smoke.py"
```

That task-owned helper creates a fresh `fixture-C-<binary source SHA>-<random>` directory
on every run, preserving each previous fixture and raw log.
For manual controls use the source’s `scripts/control.py <fixture path> status|query|quit`.
The task-owned metrics collector is `scripts/windows-metrics.ps1 -RootPid <pid> -Seconds
60 -Output <tasktemp file>`; no exposed TCP server or arbitrary code-evaluation hook.

## Remaining review and platform QA

- Parent/Opus critical code and visual review; no review delegation was performed here.
- Native IME/dead-key/palette matrix, mouse drag and double-click, word selection, paste
  supplied by the tester, full state screenshots, VoiceOver/Narrator. Cyrillic typing and
  select-all/delete were observed through CUA. OS IME composition was not claimed tested.
- `open-file` / `open-app` / `open-url` / `open-with-app` reuse the existing core handlers;
  live smoke activated only `add-note`. `copy-text` is implemented but was not exercised.
- Undo/redo omitted as allowed. Geist feature tags and letter-spacing are not applied.
  Footer reserves 32 px instead of overlaying list content, preserving the visible layout.
  Initial centering uses GPUI's primary display; active-display re-centering needs QA/work.
  GPUI AX exposed only the window in CUA; accessibility parity is not established.
- macOS data is ad-hoc local-preview and runtime-shader only; the prototype still links
  Tauri/WebKit. No production-equivalent startup or broad migration claim.
- Windows: use the exact archive and manifests in tasktemp, explicit pinned Cargo **and**
  RUSTC paths, a dedicated target, existing SDK fxc/rc. An SSH compile is not DirectX/IME/
  focus validation. No available CUA surface controls the remote console. `windows-metrics.ps1`
  collects Toolhelp-owned PrivateUsage/WorkingSet/CPU; it does not compare platforms.
  A Windows matched baseline control transport is deferred while interactive QA is blocked.

## Parent repairs after Opus review 45278ff

F1/F2: RGBA icons are converted to GPUI BGRA. Hide, replacement and LRU eviction
explicitly remove their GPU atlas entries. Icon lookup/decode has concurrency four.
F7/F9: keyboard navigation scrolls to the nearest viewport edge; the no-results
line uses text-2. Control quit and Cmd-Q/Alt-F4 await bounded core shutdown before
GPUI quit (an OS-forced termination can still bypass this hook).

F3: hidden A/B start hidden, then show once and hide, with no icon query. Visible
A/B show once and commit doc. Opening timings and the fixed latency workload run
only in the retained condition in both apps. Each result records the preparation
steps; smoke results separately identify their extra synthetic activation work.
A retains its resolved icon cache on hide while B releases it, a declared difference.

F4–F6: bundling accepts an explicit binary, which is re-verified after signing.
Build A with scripts/build-a.py from a clean checkout. Its adjacent build receipt
binds source, binary SHA-256, resource hashes and compiler commit; stale receipts,
resource changes and compilers other than Rust 1.98.1 are refused. B embeds its
compiler commit and exposes it with --version. Results separate binary source from
harness source. The full runner also requires equal indexed category counts.

The parent functional validation also found two harness assumptions: the current
WebView deliberately makes no native search request for one-character input, and
hide acknowledgement precedes WindowServer visibility. Fresh-reply comparisons use
the same six queries starting at two characters in both apps (one-character native
result timing is omitted explicitly). Both variants now reach the requested OS
visibility state before warmup sampling. Failed smoke outputs remain preserved.

All macOS A/B/C measurement processes launch through LaunchServices from their
separate temporary bundles. Directly spawning B from the harness made the host
inherit T3's responsibility domain, which could omit reparented XPC helpers. The
harness now refuses a run unless the root PID owns its responsibility domain;
C is bundled as well. Earlier direct-spawn smokes are functional evidence only.

## Final measurement notes after Opus recheck 4da6ff8

The A build command now rebuilds the frontend itself and receipts all dist hashes
before Cargo compiles it; staging refuses changed frontend output. Rebuild A/B at
one clean source commit before the final matrix. Receipt tests include stale frontend
and native --version failures. A latency rows now retain the fixed query for grouping.
Both modes record input and actual signed/measured executable hashes separately.

Do not compare ready_ms as startup latency: A waits for bridge/renderer readiness,
whereas B waits for the scan. Opening frame/OS observations have separate labels.
Character typing still sends B's first character through Rust while A filters locally;
that extra work remains included and is a declared latency confound, not a pure engine
comparison. A visible preparation waits two frames; query work settles during warmup.
