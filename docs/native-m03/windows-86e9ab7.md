# Windows verification attempt — 86e9ab7

**Blocked by disk capacity; no Windows test passed or failed because no test executable ran.**
This is M2/M3 foundation/controller evidence only. It establishes neither M4 UI parity
nor completion of the Rust/GPUI migration. Machine-readable details:
[dedicated handoff](windows-86e9ab7.handoff.json).

## Frozen source and ownership

Tested source: `86e9ab733815e0396cc2780cbdec64f3e2040eba`. Selected local tree:
`/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`, branch `feat/native-gpui-app`; HEAD remained at the pin during this task.
The uncommitted M4 app/UI changes belonged to another child and were not edited,
staged, committed or transferred. The archive contains the pin's **old** app/UI
skeleton, not those dirty replacements/additions. No GPUI app build was attempted.
Only this document and its adjacent dedicated JSON were added to the repository.
No common verification, ledger, source, manifest or lock file was changed.

The Mac generated `git archive` from the full commit ID and a SHA-256 manifest from
all immutable Git blobs, comparing the archive member set and content against it.
Windows verified the archive and **all 1,800 tracked files before building**, again
after the failed gates and after task-private NTFS compression: zero missing,
changed or unexpected source files. The archived source has no `.git` metadata;
no clone, fetch, development checkout or Git command ran on Windows.

| Item | SHA-256 |
| --- | --- |
| `source-86e9ab7.tar.gz` (110,319,775 bytes) | `ea7c7d62356ce15012526b21e344475eb73f8d7afc78db118e780853dac93abb` |
| `manifest-86e9ab7.json` | `e3cbaf4e93a1a2d0cfa303ecb544731896f2b361adaf5d7dd886bae02bb2a24b` |

Mac raw evidence: `/tmp/tezbar-native-m23-windows-86e9ab7-evidence/`. The complete file manifest and
`raw-evidence-index.json` are there; the latter hashes the copied logs and helpers.

## Machine, routing and build isolation

Authorized fallback: `User@100.77.254.64`, hostname **DESKTOP-OB83S8J**. The parent
reported the environment link pending; this task did not call/poll link tools or
invent remote identifiers. SSH used existing configured keys. No secret was needed.

Fresh OS-temp build stage (retained):

```text
C:\Users\User\AppData\Local\Temp\tezbar-native-m23-86e9ab7-e063108b26e5
```

Windows 11 Pro 10.0.26200 x64; i5-12400F (6 cores / 12 logical processors),
16,608,804 KiB total RAM, 7,277,812 KiB free RAM at initial inventory.
Initial C: free space was **5,638,025,216 bytes**. It was **2,114,609,152 bytes**
at the start of the full core attempt and reached **zero** during later gates.
An unrelated Rust 1.85 helper build was observed concurrently; it was not touched.
The evidence does not attribute all disk consumption to either build.

Existing explicit Rust **1.98.1** was rechecked using the existing rustup executable:
`rustc 1.98.1 (48a229cea 2026-09-01)`, `cargo 1.98.1 (797e8a9bc 2026-08-05)`.
The PATH cargo is standalone 1.85, so commands called the exact 1.98.1 cargo, rustc
and rustdoc paths resolved by `rustup which --toolchain 1.98.1`. This implements the
requested `cargo +1.98.1` selection without changing defaults or installing tools.
Existing VS 2022 BuildTools and Windows SDK 10.0.22621.0 rc/fxc were found;
only the build process inherited `VsDevCmd`'s environment. No GPUI shader build ran.

Every gate used `CARGO_BUILD_JOBS=2`, `--locked`, offline resolution, test threads 2,
a new private target, disabled incremental compilation and debug symbols, disabled
Cargo automatic cleanup/toolchain installation, and a private fixture-temp directory.
The old build target/artifacts were not reused. Existing registry archives/index and
pinned Cargo git dependency cache were **copied** into a private Cargo cache; their
Git metadata is dependency-cache data, outside the archived source. Earlier cache,
staging, scripts and evidence were read only. No package version or lock changed.

The older cache lacked `qrcode` metadata. The private cache was seeded with 681
package metadata records and 41 locked archives (40 from the Mac cache; one public
`cpufeatures-0.3.1` archive downloaded from static.crates.io). Added archives were
checked against the lock checksums; the transferred seed archive was also hashed.
There was no tool installation or live provider/MCP access.

## Actual gates and counts

All logs below are copied to `/tmp/tezbar-native-m23-windows-86e9ab7-evidence/remote-final/evidence-round3/`.
The literal executable, arguments, timestamps, disk readings and native exit codes
are in `gates.json`. SSH/runner exit zero is **not** a passing Cargo gate.

| Gate / exact arguments after the pinned cargo executable | Exit | Actual test count | Result / raw log |
| --- | ---: | --- | --- |
| `test --locked -p tezbar-core --features native-ui-instant -- --test-threads=2` | 101 | 0 executed; pass/fail/ignore inventory unavailable | blocked; `core.log` |
| `test --locked -p tezbar-host -- --test-threads=2` | 101 | 0 executed; inventory unavailable | blocked; `host.log` |
| `test --locked -p tezbar-update -- --test-threads=2` | 101 | 0 executed; inventory unavailable | blocked; `update.log` |
| `test --locked -p tezbar-ui-model --features launcher-models,agent-models -- --test-threads=2` | 101 | 0 executed; inventory unavailable | blocked; `ui-model.log` |
| `check --locked --lib -p tezbar-core -p tezbar-host -p tezbar-update -p tezbar-ui-model --features tezbar-core/native-ui-instant,tezbar-ui-model/launcher-models,tezbar-ui-model/agent-models` | 101 | non-test check | blocked; `windows-libraries.log` |

Core reached compilation of the pin's library and test library, then failed writing
`libtezbar_core-f266a0e78b22e372.rlib`: **“There is not enough space on the disk.
(os error 112)”**, followed by LLVM's **“IO failure on output stream: no space on
device.”** Host failed writing keyboard-types/windows-sys artifacts. Updater recorded
`LNK1318 FILE_SYSTEM (3)` for a build-script PDB alongside an explicit disk-full
windows-sys write error. UI-model and the library check also failed artifact writes
with error 112. These logs establish an infrastructure blocker, not a source defect.
No source repair, test edit, new ignore, skip or broad filter was introduced.

Consequently the Windows NSIS argv/ShellExecuteW wrapper, SID/mutex/pipe takeover,
short/retiring single-instance paths, NativeDesktop dispatch, and new M3
context/effects/settings/images/legacy-terminal/notes behavior remain **unverified
on this pin on Windows**. The Mac's 337-controller count is not a Windows result.

## Preserved attempts and final audit

- `run-gates-host.log` and `remote-final/evidence/`: all five initial exit-101
  cache-resolution failures (`qrcode` index entry missing), before compilation.
- `local-helper-preparation-failures.log`: unavailable Python TOML module/version
  during helper preparation; resolved using the existing interpreter, no install.
- `cache-seed.json`, `cache-seed.tar.gz`, `run-gates-round2-host.log`: verified
  private-cache seed, followed by a helper setup failure because its private
  fixture-temp directory already existed. No Cargo command started in that attempt.
  A new fixture/evidence directory was used for round 3; no source was changed.
- `run-gates-round3-host.log`, `remote-final/evidence-round3/`: every full command,
  native error, exit code, initial warning and partial compiler artifact retained.
- `capacity-stop*-host.log`: ownership checks found no matching active core Cargo
  process/coordinator after completion; no process was stopped. Helper errors are
  retained, including an invalid escaped diagnostic path.
- `target-compression-host.log`, `recover-headroom-host.log`, compression logs:
  NTFS compression was confined to this task's target, cached registry sources and
  archived source. No artifact was deleted. Final source hashes still match.
- `audit-final-host.log`, `remote-final/audit-final/`: all 1,800 source hashes match;
  no owned stage/fixture process, cargo/rustc/link/compact process or Tezbar process
  remained. No test executable had run, so no test child-process fixture started.
  Final C: free space: **274,378,752 bytes**.

All writes were confined to this task's OS-temp stage and Mac evidence files plus
the two assigned repository documents. `C:\Users\User\Desktop\Tezbar` was never
modified. No product executable/installer was launched or replaced; no production
profile, credential, clipboard, live provider or MCP call was used. No installed
profile was inspected or recursively hashed: unchanged-product assurance follows
from the recorded confined commands and no product launch, not an invented
before/after product/profile hash audit. No unrelated process was killed, no
service was started, and no global PATH/toolchain/VS/SDK/registry/system setting
was changed. No git stage/commit/push/merge/PR/release/Actions/install was performed.

## Required continuation

Restore several GiB of C: headroom (recommend **at least 6 GiB free** before resuming)
without deleting preserved staging/evidence. Reverify this same archive and every
tracked file, then rerun the unchanged full locked gates using a new evidence and
fixture directory. Existing task-private dependency/target cache can be considered
for reuse; do not rerun the guarded round-3 helper into its existing evidence folder.
No source fix is justified by the current compiler evidence.

Windows/Bun live fixtures remain deferred to M9/whole-app acceptance. Parent's Mac
typecheck, 745 TS passes with seven existing skips and real Bun simulated-host fixture
were not rerun here. No GPUI app/UI build, desktop/DirectX, IME, Narrator or full-app
RAM result is claimed. Fresh independent Opus review, M4–M11 native surfaces and
cutover, and full-app RAM acceptance remain required for the user's full migration.
