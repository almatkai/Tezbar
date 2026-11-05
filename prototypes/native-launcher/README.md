# Native launcher prototype — local review handoff

This is a runnable, isolated GPUI launcher backed by the current Rust core. It does not
replace the React/Tauri UI. Opus review, the complete native input/IME matrix and the
full measured migration decision remain open. No push, PR, merge, release or Actions run.

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
| Rust 1.85 checks, default and facade | Passed; `msrv-default.log`, `msrv-facade.log` |
| Prototype session/control tests | 5 passed, no ignored; `prototype-test-handoff.log` |
| Main `pnpm typecheck` | Passed; `typecheck.log` |
| Main `pnpm test` | 744 passed, 7 existing skipped; `pnpm-test.log`; no new skips |
| Main frontend artifact for A | Built from unchanged renderer; `a-frontend-build.log` |
| Instrumented A release | Built locally with `desktop-bench,tauri/custom-protocol`; `a-build-404429e.log` |
| A/B/C functional smokes | See `mac-A5-smoke.json`, `mac-B2-A-resources-smoke.json`, `mac-C-smoke.json` |
| Exactly-once activation | B synthetic `add-note`: one `click_events` record; no clipboard write |
| Representative CUA screenshots | `mac-dark-results.png`, `mac-light-results.png` (source 404429e) |
| Installed beta | PID/path/hash recorded; unchanged hash and no owned survivors after successful smokes |

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

Successful smokes used source `404429e706e9fa22eb677f205655d6ce3573fea4`, fixed seed 65001,
20,000 generated files, 50 notes, 50 fixture snippets and 20 quick links. A and B had the
same observed database counts: 235 applications, 14,997 files, 50 notes, 79 snippets
(including built-ins), 20 quick links, 10 commands and 41 native commands. Record actual
counts on every run rather than assuming all generated files are eligible for indexing.

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
CARGO_TARGET_DIR="$PWD/prototypes/native-launcher/target/tauri-a" cargo +1.98.1 build --manifest-path src-tauri/Cargo.toml --locked --release --features desktop-bench,tauri/custom-protocol
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
