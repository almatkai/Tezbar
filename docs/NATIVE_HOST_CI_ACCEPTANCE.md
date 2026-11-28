# Native external Rust host CI acceptance

The existing quality job in `.github/workflows/ci.yml` runs its full Rust library
suite once per matrix entry. Windows x64 and macOS ARM additionally require real
public-package branches and the current production CLI. Intel Mac keeps the
normal suite and explicitly reports unsupported real-package acceptance because
no Intel release artifact is published.

## Public fixtures and proof

`scripts/native-host-acceptance.py prepare --out <owned CI temp directory>` fetches
only the ten named public packages from the checked-in catalog's versioned asset
URLs. It never downloads Port Manager. Archive and executable SHA256 pins come
from `v2_catalog.json`, not a downloaded manifest. ZIP bounds match the host:
128 MiB compressed, 512 MiB expanded, 4,096 members. Closed inventory, every member
digest, regular-file/path rules, target executable header, and all identity,
capability, command, migration and policy fields are checked before use. No
extension executable is run by preparation or CLI acceptance.

The raw Google Calendar 1.4.4 reference is fetched from the pinned release asset
directory. It is accepted only if exactly equal to the trusted catalog object
with its empty `files` field removed. It remains raw for the omitted-files test.
Perplexity 1.0.0 uses the trusted catalog reference directly. Downloads have
socket/deadline/byte bounds and public HTTPS origin restrictions; no credential,
private source, secret input or authenticated asset channel is added.

SDK 0.1.1 is checked out into CI temp storage from public immutable commit
`e955911f0bf5b558f9d3eb5183df0adc95c22af2`, with HEAD and package version checked.
Rust 1.85.0 builds only its native `sdk-fixture` using `--locked -j1`. No path to a
developer's SDK checkout or floating branch is used.

`rust-tests` sets the verified SDK, Perplexity manifest/ZIP and raw Calendar
manifest/ZIP environment inputs for the **existing full library pass**. It
requires four successful fixture test names, the real Perplexity branch proof,
the Calendar derived-inventory/rollback proof and no required missing-fixture
message. Two SDK tests use captured/inert host services; the real Perplexity form
uses a captured shell router. No browser, model API, device, capture or signal
action is performed. Missing or invalid required fixtures fail the job.

Cargo builds use one job, Rust tests one thread, and frontend tests two workers.
The CI-only `ci-cargo-one-job.rs` PATH shim translates the existing supervisor
fixture's hard-coded `cargo ... -j2` to `-j1`; it forwards to the absolute original
Cargo executable. It is compiled into CI temp storage and never shipped in the
app. Intel uses the same wrapper with no public/SDK fixture claim. The separately
unignored existing Rust/Bun worker test remains unchanged.

## Current production executable

After backend/frontend builds and library tests, supported runners run
`cargo build --locked --release --bin tezbar -j1`. This builds the production
entry point; it does not run Tauri packaging, sign, make NSIS/update artifacts,
bump versions, deploy or upload a release.

`cli` executes that binary with owned fresh data/config/temp paths, closed stdin,
30-second per-process deadlines and no GUI startup. Its checks cover:

- Empty discovery; canonical private Port remote refusal and unchanged profile.
- Local imports of all ten public native packages and installed discovery.
- Exact active-pointer digest, resolved identity/policy, closed installed
  inventories, all member hashes, native executable digest, and Mac permissions.
- Expected complete/partial/unavailable command metadata from declared platform
  coverage, recorded explicitly in `cli-summary.json`.
- Rejected wrong-SHA public Perplexity import with the whole temporary profile,
  pointer, installed bytes and seeded Unicode preferences unchanged.

Mac uses a process-scoped sandbox denying network and process creation. An inert
owned Python self-check must prove both denials. Windows starts each owned CLI
suspended, assigns it to a kill-on-close Job Object with active-process limit one,
then resumes it; total Job process accounting must remain one. Its self-check
intentionally attempts exactly one inert child and must prove creation fails.
Only that probe requires `childDenied: true` (an actual JSON boolean), exactly one
attempt, and `TotalProcesses == 2`. Microsoft documents that
[`TotalProcesses` includes failed associations due to a limit violation](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_accounting_information)
and that the [active-process limit terminates and rejects an excessive association](https://learn.microsoft.com/en-us/windows/win32/api/winnt/ns-winnt-jobobject_basic_limit_information).
Native Windows Python 3.12.10 at `6071fb4` (PR6 run 40208846318, job
121215534339) returned denial JSON and exit zero with total=2, active=1,
terminated=0. r47 incorrectly required exactly one limit termination. The probe
now permits termination counts zero or one **only with direct denial and
nonexecution proof**; the counter alone cannot prove or disprove creation
rejection. No exact Windows `errno` or `winerror` was recorded in that run.

The attempted child creates/writes an exclusive marker as its first payload
action. The parent owns a fresh temporary marker directory, passes the exact
path, and requires it echoed with integer `childAttempts: 1` and an OSError
record containing `errno`/`winerror` integers or null. The probe catches only
`OSError` from its single `subprocess.run`; those numeric fields are logged
without assuming specific native codes or printing error text. After successful
Job closure and owned-process reap, while that directory still exists, the
parent rejects any marker (including one created during cleanup) and adds
`childExecutionAbsent: true`. Successful creation, forged denial with a marker,
wrong/missing proof and cleanup failure cannot pass. Success requires the marker
directory to survive cleanup; it is then removed. The Mac fork/network probe
uses the same child proof.

`ActiveProcesses` is recorded and bounded by the total without requiring settled
zero. Real CLI calls retain strict `TotalProcesses == 1` and zero limit
terminations; no terminations are subtracted or child allowance inferred from a
command label. The query checks the 48-byte Win32 accounting ABI and returned
length. All three counters appear in native logs before output validation, in
accounting/exit/output failure messages, and in successful self-check proof JSON.
Job setup/accounting failures fail acceptance.
Both command runners close their owned job and reap their own `Popen` on every
exit path. If job assignment/resume or job termination fails, cleanup still kills
the owned process handle, including a suspended process that never joined the
job. Job closure cleans assigned descendants even if the leader already exited.
Reaping has a five-second cleanup bound; cleanup errors are reported without
replacing the original failure. A partially configured job is closed on setup
failure or cancellation. Deterministic WinAPI fakes exercise both runners on
non-Windows machines, including raw SDK-layout accounting bytes, denied/successful
probe attempts, strict CLI accounting, malformed output and cleanup; they do not
establish native Windows behavior. A Windows-only inert self-check also runs in
the existing helper-test step before SDK preparation and the production release
build, so guard regressions fail early without any workflow change.
Windows has process-scoped unreachable proxies only: **no hard offline network
block is claimed**. The existing source/synthetic library tests independently
assert private remote refusal before HTTP. No global firewall, security policy
or OS permission is changed. Failure logs stay in owned runner temp files; Rust
suite output and the public CLI integrity summary appear in job logs. No upload
step is added, and private package bytes are never read or logged.

## Limits and remaining gates

Private Port's real package/launcher environment inputs remain deliberately
unset. CI explicitly reports them pending; synthetic private/local-only,
rollback and supervisor tests still run. Public `GITHUB_TOKEN` cannot fetch those
private assets, and this integration requests no new secret. Prior actual Mac
private import and Port's own native CI are separate evidence, not this job's
Windows acceptance.

A green Windows job proves the production CLI public import boundary plus native
library supervisor/Job/EOF/host-death tests. It does not establish an NSIS install,
live app/window/console behavior, Windows storage ACLs, private package import or
actual desktop capabilities. Those remain scoped parent-owned Windows/private
and live QA gates. Run this workflow on the parent's final reviewed source (r40 acquisition guards
and r41 preference preservation, incorporating the earlier r35/r37 findings);
older frozen-binary helper checks cannot accept those fixes.

For helper-only local validation, use `validate-local --local-releases <existing
verified public release directory> --repo <source> --out <fresh temp directory>
[--binary <existing native app executable>]`. It rechecks the same pins without
network or SDK/Cargo builds, never reads Port bytes, and reports its narrower
scope. Critical helper boundaries have standard-library unit tests:
`python -m unittest discover -s scripts -p 'native_host_acceptance_test.py' -v`.
