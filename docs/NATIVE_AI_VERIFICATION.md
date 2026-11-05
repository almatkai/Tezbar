# Native AI implementation verification

## Post-closure TypeScript cleanup

After migration review closure, `86fc4ad2e7e6de8211ac3965be3cb9c9c2b85886`
removed the unused TypeScript core and its retired implementation tests. The
[cleanup verification](TS_BACKEND_CLEANUP.md) records the exact scope and retained
extension worker. Rust and renderer sources remain unchanged; worker imports now
target their existing host bridges directly and its generated bundle was updated.
The current retained TypeScript suite passes **744 tests / 7 existing skips**.
The historical 951-test count below includes the removed legacy tests. Typecheck,
frontend/backend builds and the actual Bun extension-worker fixture pass. The
native AI review findings are not reopened by deleting the unused implementation.

## Final implementation and review closure

The native AI migration and its mandatory Claude Opus 5.5 review/repair gate are
complete on `feat/native-rust-agent`, based on the explicitly selected
`v0.3.0-beta.7` (`e246752149dbbede5e44720a0e35a3eacf2d66a9`). Final tested
implementation: **`08a50f8a8f3e622ee1ff45b3b2ad123a879c2d73`**, tree
`47ec0cb90f04d56cb40fda4a33a0ec6d4b20c7e3`. Documentation-only follow-ups through
`e2d19a0` recorded the final review; no implementation changed during that review.

The original full three-scope audits and every repair/re-review are preserved
unchanged in `docs/reviews`. Their final closures combine as follows:

| Scope | Final Opus evidence | Status |
| --- | --- | --- |
| Provider wire/protocols and credentials | [Wire closure at `847ea62`](reviews/native-ai-opus-wire-847ea62.md) | Closed, 0 actionable findings; code remains byte-identical |
| Extension trust/security, OAuth, secrets and owned client lifecycle | [Standards closure at `fd8c356`](reviews/native-ai-opus-standards-fd8c356.md) | Closed, 0 actionable findings; native code remains byte-identical |
| Runtime, conversation ownership, approvals, Settings and persistence | Earlier R/U/N-R/C-R/T-R1–T-R6 closures plus [final T-R7 closure at `dd8e8cf`](reviews/native-ai-opus-runtime-dd8e8cf.md) | Closed, 0 outstanding actionable findings |

The last runtime reviewer independently ran nine probes: all fail before the
Copilot repair and pass afterward, including sign-in ordering, late/removed
providers, pending edits, failed-save recovery, client-ID failure and a successful
token poll. Its full snapshot suite passed **960 tests / 9 existing skips**,
comprising the 951 permanent tests plus those nine external probes.

| Final required check | Result |
| --- | --- |
| Permanent TypeScript unit/integration suite | **951 passed, 9 existing skips, 0 failed** |
| Typecheck and frontend build | Pass |
| macOS Rust 1.98 locked library suite | **608 passed, 9 existing ignores, 0 failed** |
| Windows x64 Rust 1.90 locked library suite | **538 passed, 8 existing ignores, 0 failed** |
| Rust 1.85 offline library check, backend lifecycle and Windows non-test check | Pass |
| Source identity and diff checks | Pass; final native sources/scripts are byte-identical to tested `756072e` |

Detailed exact-source Windows staging, hashes, commands and retained logs follow
in the historical evidence below. No new skip/ignore was introduced to pass a
gate. Reviewers' explicitly non-gating observations remain recorded in their
reports; this closure does not claim the absence of all possible defects.

Built-in AI execution is Rust, with five provider protocols, durable history,
native tools/auth/catalogs and MCP-based extensibility. React remains the UI;
explicitly connected MCP servers may use any language. The
[extension author guide](AI_EXTENSIONS.md) documents connections, manifests,
instructions, commands, profiles and custom providers.

Validation used temporary profiles, fixture keys, mocked IPC and owned loopback
servers. **Live upstream provider/MCP interoperability and actual desktop window
delivery were not exercised.** Existing Windows OCR/semantic UI Automation limits
remain documented. No production profile, app install/launch, push, PR, Actions
or release was used. This completes the implementation/review task, not a release.

All "pending" statuses below describe earlier passes and are superseded by this
final closure.

## Historical: Copilot sign-in persistence — source `08a50f8`

Validated implementation: `08a50f8a8f3e622ee1ff45b3b2ad123a879c2d73`, tree
`47ec0cb90f04d56cb40fda4a33a0ec6d4b20c7e3`, on `feat/native-rust-agent`.
The [latest Opus audit](reviews/native-ai-opus-runtime-8e12f1a.md) closes T-R6
and all shared provider-save callsites, but identifies T-R7 (P1): Copilot's direct
device-sign-in write replaces the whole provider-config map with its owned key.

Sign-in now saves through `persistAiSettings` and waits on the ordered owned-key
merge before starting the device flow. A failed Copilot save stops sign-in and
retains its retryable error. The optional legacy top-level client-ID write remains
a scalar patch. No native, queue or bridge source changed.

- `pnpm test`: **951 passed, 9 existing skips, 0 failed**; 125 files passed,
  2 skipped. `/private/tmp/tezbar-ai-copilot-save-final-ts.log`.
- `pnpm typecheck`: pass; `/private/tmp/tezbar-ai-copilot-save-final-typecheck.log`.
- `pnpm build:frontend`: pass; `/private/tmp/tezbar-ai-copilot-save-final-frontend.log`.
- `git diff --check`: pass. Native Mac **608** / Windows **538**, MSRV and lifecycle
  evidence remains applicable to byte-identical `756072e` native sources/scripts.

Four permanent component cases verify existing peer credentials, a provider saved
after mount (including custom client ID), a rejected save and an explicit
`ok:false` save. OAuth calls use mocks only. Defect/failure expectations were red
before repair, then all four passed. Logs: `/private/tmp/tezbar-ai-copilot-save-{red,green}.log`.
**Fresh targeted Opus 5.5 closure of T-R7 is pending.** Other reviewed scopes remain
closed. No real sign-in, live provider, production credentials, app launch, release,
PR or Actions were used. Earlier live/desktop verification limits still apply.

## Historical: live-write ownership — source `935233a`

Validated implementation: `935233a0f47d88740c48d1d6f94eb1a8dd0f3179`, tree
`49cd23e841e8bb2a112c132170c6e79dfd8583a4`, on `feat/native-rust-agent`.
The [latest Opus audit](reviews/native-ai-opus-runtime-a0b3922.md) closes T-R4,
T-R5, cross-owner ordering and the shared-queue API. It leaves one P3, T-R6:
a live first write can overwrite definitions added by a retired owner's Retry.

The owned-ID merge now applies to every non-removal write; supersession checks
remain retry-only. Two permanent regressions first reproduced T-R6, then passed:
failed live URL or Create followed by retired Retry and a fresh live URL edit.
Both preserve the active provider's definition/config/models/selection. The
creation variant also verifies its own later retry preserves both providers.

- `pnpm test`: **947 passed, 9 existing skips, 0 failed**; 125 files passed,
  2 skipped. `/private/tmp/tezbar-ai-live-ownership-final-ts.log`.
- `pnpm typecheck`: pass; `/private/tmp/tezbar-ai-live-ownership-final-typecheck.log`.
- `pnpm build:frontend`: pass; `/private/tmp/tezbar-ai-live-ownership-final-frontend.log`.
- `git diff --check`: pass. Native sources/scripts and queue/bridge code are
  unchanged in this repair. Mac **608** / Windows **538** native evidence below
  still applies to byte-identical `756072e` native sources.

Focused defect logs: `/private/tmp/tezbar-ai-live-ownership-{red,green}.log`.
**Fresh targeted Opus 5.5 closure of T-R6 is pending.** Wire/security and previous
runtime findings remain closed on their reported triggers. Non-gating observations
from the review were not expanded into unrelated changes. No publishing, app
launch or live provider validation occurred; existing validation limits below apply.

## Historical: retry ownership and intent order — source `f5f8cd0`

Validated implementation: `f5f8cd0964998231da433d29444bd7036c678ad2`, tree
`1792f00d77a88e03c59236b99caecd80cd849f30`, on `feat/native-rust-agent`.
Changes since `30905e2` are confined to Settings, its window-owned write queue,
and real-component tests. Native sources/scripts remain byte-identical to the
Mac/Windows-tested `756072e` source below.

The [latest Opus runtime report](reviews/native-ai-opus-runtime-509f53b.md) closes
T-R3 and identifies T-R4/T-R5. Repairs give each failed write one shared retry
record, preserve newer failed intents, merge retries by owned provider IDs, and
keep newer credentials, definitions, model lists and activation (`@active`).
Retry keeps its original logical revision in the shared queue; saved revisions
advance monotonically by intent order, so an old retry cannot clear a newer live
owner's unsaved removal. Local success cleanup also respects newer failed intents.

Six new permanent tests first failed, then passed: three newer-success scenarios,
two repeated-retry/removal cases with exactly two writes per round, and an older
retired retry against a newer live removal failure. The latter reproduced a
shared-queue variant beyond the reviewer's sketch and motivated retaining logical
intent revisions in `enqueueSettingsWrite`. No skips/ignores were added.

| Check | Result / retained log |
| --- | --- |
| `pnpm test` | **945 passed, 9 existing skips, 0 failed**; 125 files passed, 2 skipped; `/private/tmp/tezbar-ai-draft-ownership-final-ts.log` |
| `pnpm typecheck` | Pass; `/private/tmp/tezbar-ai-draft-ownership-final-typecheck.log` |
| `pnpm build:frontend` | Pass; `/private/tmp/tezbar-ai-draft-ownership-final-frontend.log` |
| `git diff --check` | Pass |
| Unchanged native implementation | Mac **608** / Windows **538** passed, with existing ignores; MSRV/lifecycle checks below |

Red/green focused logs: `/private/tmp/tezbar-ai-draft-ownership-{red,green}.log`.
Wire/security closure remains applicable to unchanged native code. **Fresh targeted
Opus 5.5 review of T-R4/T-R5 and touched queue/Settings paths is pending.** No overall
completion is declared. Fixtures/mock IPC only; no app launch, live model/MCP,
production credentials, publishing, PR or Actions. Desktop events and live upstream
interoperability remain unverified.

## Historical: retired-draft removal barrier — source `30905e2`

Validated implementation: `30905e2affcb14e3f985e17a44dfdce5185325c2`, tree
`25f8305a0c9593fcbb1e7e8f7f1c41fc29c5deae`, on `feat/native-rust-agent`.
Only Settings UI/tests changed since `b2b4f77`; native sources and scripts remain
byte-identical to the macOS/Windows-tested `756072e` source.

The [latest runtime audit](reviews/native-ai-opus-runtime-dacc2e0.md) closes T-R2
and its failure/retry repairs, but identifies T-R3: an older failed peer edit or
failed provider creation can restore a subsequently deleted provider on retired
owner Retry. The implementation now retains each draft's first revision across
repeated failures, filters later-deleted custom IDs from all captured maps and
definitions, and refuses their stale activation. Filtered selections are applied
only after removal fallbacks, without retaining an unfiltered captured selection
map. A newer intentional Create is allowed.

- `pnpm test`: **939 passed, 9 existing skips, 0 failed**; 125 files passed,
  2 skipped. Log: `/private/tmp/tezbar-provider-resurrection-final-ts.log`.
- `pnpm typecheck`: pass; `/private/tmp/tezbar-provider-resurrection-final-typecheck.log`.
- `pnpm build:frontend`: pass; `/private/tmp/tezbar-provider-resurrection-final-frontend.log`.
- `git diff --check`: pass. Native checks remain Mac **608** / Windows **538**
  passed, with unchanged existing ignores and the MSRV/lifecycle checks below.

Four permanent failure cases first reproduced T-R3 (both original scenarios,
with and without a second failed Retry), then passed after repair. Two controls
verify a failed Create can be retried and an explicitly removed provider can be
created again with the same ID. One initial control assertion incorrectly required
a nonempty default custom model, although a new custom provider has none; the
assertion was corrected to require the selection entry, without changing source.
Red/green logs: `/private/tmp/tezbar-provider-resurrection-{red,green}.log`.

Wire closure at `847ea62`, extension/security closure at `fd8c356`, and earlier
runtime closures remain applicable to unchanged code. **Fresh targeted Opus 5.5
closure of T-R3 and touched retry paths remains pending.** No overall migration
completion is declared. Validation uses mocked IPC/fixture profiles only; live
provider interoperability and desktop window delivery remain untested. No push,
PR, Actions, app launch/install or release occurred.

## Historical: provider-removal repair — source `b2b4f77`

Validated UI implementation: `b2b4f778a1e45e3c4b7cc7385212d391195f5ea0`, tree
`b0b43ceb0a874282e8558717cb0a321312c992a8`, on `feat/native-rust-agent`.
The only implementation changes since `756072e` are `SettingsView.tsx` and its
real-component regression tests. Native sources and lifecycle/build inputs are
byte-identical to the macOS/Windows-tested `756072e` source below.

- `pnpm test`: **933 passed, 9 existing skips, 0 failed**; 125 files passed,
  2 skipped. Log: `/private/tmp/tezbar-provider-final-ts.log`.
- `pnpm typecheck`: pass. Log: `/private/tmp/tezbar-provider-current-typecheck.log`.
- `pnpm build:frontend`: pass. Log: `/private/tmp/tezbar-provider-final-frontend.log`.
- `git diff --check`: pass. No skips or ignores were added.
- Native evidence remains **608 macOS / 538 Windows tests passed**, with the same
  existing ignores; MSRV, lifecycle and non-test Windows checks passed below.

The unchanged [wire closure](reviews/native-ai-opus-wire-847ea62.md) and
[extension/security closure](reviews/native-ai-opus-standards-fd8c356.md) have
zero actionable findings. The [runtime review](reviews/native-ai-opus-runtime-fd8c356.md)
closed T-R1 and left only T-R2. This repair flushes pending model edits before
provider removal, supersedes that provider's failed drafts on successful removal,
and recomputes deletion from current records on Retry to preserve peer credentials.
Five permanent regressions cover both original triggers, failed pending saves,
retired-owner retry and a newer peer key after a failed removal. The failure cases
were reproduced before the corresponding repairs.

**Fresh targeted Claude Opus 5.5 closure of T-R2 and the touched save/retry paths
is pending. Overall migration completion is not declared yet.** No live model,
production profile, desktop launch, push, PR, Actions or release was used. Live
upstream interoperability and actual desktop window/event delivery remain
unverified. The existing Windows OCR/UI Automation limitations are unchanged.

## Historical: four targeted-review variants — source `756072e`

Validated implementation: `756072ee682eae773626b3d1b78d807217014580`, tree
`0147dc5caa1e3dfbe94ab2214c5f7e8b808994a0`, in the same clean task worktree
and branch `feat/native-rust-agent`. The following evidence/ledger update is a
documentation-only successor to that tested source. No implementation edits
occurred during its final checks.

The completed full Opus reviews at `1623d47` and targeted reviews at `847ea62`
remain unchanged under `docs/reviews`. Wire closed with no actionable findings;
the four remaining variants (T-R1, S8-R1, S8-R2, S5-V1) now have implementation
repairs and permanent regressions. **Fresh Opus closure of those four repairs is
still pending.** No overall migration or release sign-off is claimed here.
The wire/protocol/provider/agent code reviewed at `847ea62` is byte-identical;
the changes are confined to extension lifecycle/secrets and Settings selection.

| Required check | Final observed result |
| --- | --- |
| `pnpm typecheck` | Pass |
| `pnpm test` | 928 passed, 9 existing skips, 0 failed; 125 files passed, 2 skipped |
| `pnpm build:frontend` | Pass |
| macOS Rust 1.98 full library suite, locked, two test threads | 608 passed, 9 existing ignores, 0 failed; 155.29 s |
| Rust 1.85 offline locked library check | Pass |
| Backend lifecycle fixture | 1 passed, 0 failed |
| Windows Rust 1.90 non-test library check | Pass |
| Windows full locked library suite, two test threads | 538 passed, 8 existing ignores, 0 failed; 102.53 s |
| `git diff --check` | Pass |

The two new real-component selection regressions and all 40 extension Rust tests
also passed before the complete suites. The Windows DPAPI fixture now exercises
a second permanent key failure with no ciphertext, an explicit reset and another
encrypted roundtrip. Access-denied/unavailable states remain refused with and
without ciphertext. The catalog-stop regression waits for a real fixture request
before Remove/shutdown and checks completion under one second, no successful
adoption and empty client/start registries. No skips or ignores were added.

### Exact-source Windows staging

Authorized Tailscale SSH: `User@100.77.254.64`, `DESKTOP-OB83S8J`. Isolated stage:
`C:\Users\User\AppData\Local\Temp\tezbar-native-c7271bc-20261104\rev-756072e-closure4`.
The release checkout remains clean on `release/0.3.0-beta.7` and was not edited.
The cached immutable `8f70d2e` archive plus a committed 34-file overlay were used,
with no deleted paths. Windows verified **all 1,784 tracked files** against the
final commit before building and **1,778 non-generated tracked inputs** afterward;
only the six regenerated `dist-backend/` files are excluded after the build.
This is full-file snapshot verification, not a newly transferred full archive.

| Artifact | SHA-256 |
| --- | --- |
| `overlay-756072e.tar.gz` | `06e859fd46faae52f0f7b491c08f555ef91d50f89f28283f087e7baeaacabc28` |
| `manifest-756072e.json` | `176aca5c2746193d8f14e33008abf29fc5652a04bbac9bd9b1bd14afdc5f9018` |

The local manifest and overlay are in `/private/tmp/tezbar-four-stage-756072e`.
Recipe: `/private/tmp/tezbar-four-windows-756072e.ps1`. The script was copied and
executed with `powershell.exe -File` after the encoded SSH command exceeded the
transport command-line limit; no source or validation was changed. It uses frozen
offline dependencies, backend resource staging and the same full native commands
as the earlier verified recipes. Execution-policy bypass applies to that process
only; no persistent Windows policy or profile was changed.

### Retained final logs

- `/private/tmp/tezbar-four-typecheck.log`
- `/private/tmp/tezbar-four-final-ts.log`
- `/private/tmp/tezbar-four-final-frontend.log`
- `/private/tmp/tezbar-four-final-macos.log`
- `/private/tmp/tezbar-four-final-msrv.log`
- `/private/tmp/tezbar-four-final-lifecycle.log`
- `/private/tmp/tezbar-four-final-windows.log`
- `/private/tmp/tezbar-four-windows-rust-check.log`
- `/private/tmp/tezbar-four-windows-rust-full.log`

Tests used fixture profiles, keys, owned processes and loopback providers only.
No production credentials, live Tezbar provider calls, app install/launch,
Actions, push, PR or release were used. Live upstream interoperability and actual
desktop window/event delivery remain untested; the previous documented Windows
OCR/semantic UI Automation limits are unchanged. The source-reviewed window
ordering and five-wire closure from the completed Opus audits remain applicable.

## Historical: eighteen closure-audit repairs — source `21d3e91`

Validated implementation: `21d3e911a03bbea64ffb39324d6822ca343f9ad7`, tree
`ef522242f9f06e69ae7b501b0dae2c8727d32461`, on existing
`feat/native-rust-agent` in `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`.
Started clean at `31bc3e29879e54c24725da2bc77a1454d406c168`, the docs-only
successor of reviewed `1623d47`. Authorized base remains `e246752149dbbede5e44720a0e35a3eacf2d66a9`
(`v0.3.0-beta.7`); no branch, worktree, origin/main or dev transition was made.
This verification/ledger follow-up changes **documentation only after that tested
source**. All code and build inputs remain byte-identical to `21d3e91`.

All eighteen N-W1–N-W5, S1–S10 and C-R1–C-R3 findings have implementation repairs
and permanent regression evidence in the [repair ledger](reviews/NATIVE_AI_REVIEW_FIXES.md).
**Fresh targeted Claude Opus 5.5 review of all eighteen repairs and touched-path
regressions remains required with the parent. This is not overall migration
closure, reviewer sign-off or release approval.** The three completed
`native-ai-opus-{wire,standards,runtime}-1623d47.md` reports are unchanged
(`git diff --quiet 31bc3e2..21d3e91 --` those three reports succeeds).

Implementation commits: `5dddcb4` (eighteen repairs), `c4b8888` (destroy even if
launcher restoration fails), `96a55cf` (unique revocation archives/rollback),
`74c13ed` (shared Settings ordering and deterministic Windows fixtures),
`21d3e91` (cache unchanged redaction regexes to preserve the bounded stderr tail).
No delegation, push, PR, release, Actions, app installation/launch, production
profile/key read, paid model call or third-party MCP service was used. Tests used
fixture profiles/keys, loopback servers and owned processes. One bounded XERJ
attempt returned an irrelevant hit; subsequent searches used rg/direct paths,
without index repair.

### macOS Apple Silicon — 2026-11-04/05

All checks below cover final implementation `21d3e91`. A full-file SHA-256
manifest comparison also verified all 1,781 tracked worktree files against that
committed archive after the checks; no source edits occurred during the final
runs. Tests were serialized with respect to native builds.

| Required check | Final observed result |
| --- | --- |
| `pnpm typecheck` | Pass |
| `pnpm test` | 926 passed, 9 existing skips, 0 failed; 125 files passed, 2 skipped |
| `pnpm build:frontend` | Pass |
| `CARGO_BUILD_JOBS=2 cargo +1.98.0 test --manifest-path src-tauri/Cargo.toml --lib --locked -- --test-threads=2` | 606 passed, 9 existing ignores, 0 failed; 154.49 s |
| `CARGO_BUILD_JOBS=2 cargo +1.85.0 check --manifest-path src-tauri/Cargo.toml --lib --locked --offline` | Pass |
| `RUSTUP_TOOLCHAIN=1.98.0 CARGO_BUILD_JOBS=2 node scripts/check-backend-lifecycle.mjs` | 1 passed, 0 failed |
| `git diff --check` | Pass |

Final local logs:

- `/private/tmp/tezbar-18-final-typecheck.log`
- `/private/tmp/tezbar-18-final-ts.log`
- `/private/tmp/tezbar-18-final-frontend.log`
- `/private/tmp/tezbar-18-final-macos.log`
- `/private/tmp/tezbar-18-final-msrv.log`
- `/private/tmp/tezbar-18-final-lifecycle.log`

### Windows x64 — exact committed files, 2026-11-05

Authorized SSH host `User@100.77.254.64` (`DESKTOP-OB83S8J`), Rust 1.90.0,
`CARGO_BUILD_JOBS=2`, `--test-threads=2`. Unique stage:
`C:\Users\User\AppData\Local\Temp\tezbar-native-c7271bc-20261104\rev-21d3e91-closure18`.
The release checkout `C:\Users\User\Desktop\Tezbar` was not edited or used for
these builds; no Windows git worktree was created.

Staging used the verified cached immutable `8f70d2e` archive plus a `git archive`
overlay of all 31 changed tracked files through `21d3e91` (zero deleted paths).
**This was full final-file manifest verification, not transfer or hashing of a
new full source archive.** Before building, Windows verified every one of the
1,781 expected tracked files against the final committed snapshot and refused
unexpected files. After building, all 1,775 non-generated tracked inputs were
rechecked. `pnpm build:backend` regenerates six tracked `dist-backend/` paths,
so post-build verification excludes that generated output directory. The full
pre-build check includes it.

| Item | SHA-256 / identity |
| --- | --- |
| Cached `source-8f70d2e.tar.gz` | `cb6c63ca3f2440ca896ce5454d333b6043bdd7411028a2a40e2a26ae89749f0a` |
| `overlay-21d3e91.tar.gz` | `92700ed97cc1dacdf10050172941270ec3aec753ed4aa726ad73c09b7ddce414` |
| `manifest-21d3e91.json` | `e0b472a777a05aec9065e9d0378134f4c8ebc33c6b55dc773d8965add56b0270` |
| Manifest commit | `21d3e911a03bbea64ffb39324d6822ca343f9ad7` |
| Manifest tree | `ef522242f9f06e69ae7b501b0dae2c8727d32461` |

The stage installed dependencies with frozen/offline pnpm and `--ignore-scripts`,
built backend resources, then ran non-test `cargo check --manifest-path
src-tauri/Cargo.toml --lib --locked` and the full corresponding `cargo test`
command. Both passed. Full Windows suite: **536 passed, 8 existing ignores,
0 failed, 102.66 s, exit 0**. The two focused startup-tail regressions also passed
independently before the full run.

The full run includes actual DPAPI wrong-entropy decryption failure and explicit
archive/reset; a sharing-denied key replacement that preserves key, ciphertext
and revocations; compiled exact argv recording through relative exe/cmd/PATHEXT
paths; trailing backslashes, spaces, empty/following arguments; and the gated
in-flight MCP call, independent Test probe and retired-client disable cases.
Windows owned Job tests remain enabled. No new skips or ignores were added.

Retained local Windows evidence:

- `/private/tmp/tezbar-18-21d3e91-windows.log` — manifest identity and full run result
- `/private/tmp/tezbar-18-post-build.log` — post-build non-generated input verification
- `/private/tmp/tezbar-18-windows-21d3e91-logs/rust-check.log`
- `/private/tmp/tezbar-18-windows-21d3e91-logs/rust-full.log`
- `/private/tmp/tezbar-18-windows-21d3e91-logs/startup-focused.log`
- `/private/tmp/tezbar-18-windows-21d3e91-logs/failed-start-focused.log`

### Failures investigated and remaining limits

Earlier Windows runs exposed invalid-parameter (87) from a deliberately truncated
DPAPI blob; that code correctly remains refused. The permanent-failure fixture
now protects a structurally valid blob with wrong entropy and exercises actual
CryptUnprotectData through the production key loader. A timing-based slow-call
fixture was replaced with an explicit owned release gate. More significantly,
Windows proved that compiling six scrub regexes per line consumed the stderr
drain grace and lost the traceback tail; caching the identical patterns repaired
that production path. Both modern/cached-era startup regressions now pass on
Windows. Early local test assertion/harness errors were corrected, and two
launcher recovery deadline failures occurred in overlapping exploratory runs;
the final serialized complete suite passes without changing those tests or
quarantining bugs. Only final-source results above are acceptance evidence.

No desktop window was launched. C-R1 hide/restore/destroy ordering and C-R2's
native unacknowledged-close fallback are source-verified against pinned Tauri
2.11.2; renderer tests exercise the real bridge, acknowledgements, long writes,
failed saves, default consumer, Permissions and remounted save queues, not OS
window/event delivery. Actual NAT64/intranet and upstream provider acceptance
remain untested; resolver-policy and loopback fixtures establish Tezbar's side.
Windows OCR remains unsupported as documented. No new provider/model fallback
was added. The parent's fresh Opus review remains the next required gate.

## Historical verification — `8f70d2e` / `ae2fe00`

Native validation source: `8f70d2e7cf7beac2129155402587b82fded9c35b` on
`feat/native-rust-agent`, in the existing
`/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent` worktree.
Final renderer validation source: `ae2fe004e61538a33d6b661b4c4712d186a14640`.
Its only code changes after the native validation source are in three renderer
files; the Rust code and backend lifecycle script are byte-identical, verified
with `git diff --quiet 8f70d2e..ae2fe00 -- src-tauri scripts/check-backend-lifecycle.mjs`.
The Windows archive below proves the native checks for that unchanged Rust code;
it is not an archive of the later renderer commit. This documentation update is
a **docs-only follow-up to both tested sources**. Starting point was clean
`5271fd7`, whose tested implementation ancestor was `a28c564`. This repair addresses
all six N-R1–N-R6 findings in the unchanged full runtime report.

No push, PR, merge, release, Actions run, desktop application launch/install,
production credential/profile access or paid/live model call was performed.
All local and Windows runtime tests used isolated fixture profiles and keys.
**Fresh Claude Opus 5.5 closure remains pending with the parent**, including a
full runtime review. This is implementation verification, not release approval.

## macOS Apple Silicon — final source

2026-11-04. Native checks use `8f70d2e`; final renderer checks use `ae2fe00`.

| Check | Observed result |
| --- | --- |
| `pnpm typecheck` | Pass, renderer and main TypeScript projects |
| `pnpm test` | 918 passed, 9 skipped; 124 files passed, 2 skipped; 0 failed |
| `pnpm build:frontend` | Pass |
| `CARGO_BUILD_JOBS=2 cargo +1.98.0 test --manifest-path src-tauri/Cargo.toml --lib --locked -- --test-threads=2` | 598 passed, 9 ignored, 0 failed; 158.12 s |
| `CARGO_BUILD_JOBS=2 cargo +1.85.0 check --manifest-path src-tauri/Cargo.toml --lib --locked --offline` | Pass; declared MSRV unchanged |
| `RUSTUP_TOOLCHAIN=1.98.0 CARGO_BUILD_JOBS=2 node scripts/check-backend-lifecycle.mjs` | 1 passed, 0 failed |
| `git diff --check` | Pass |

Final logs retained locally:

- `/private/tmp/tezbar-parent-final-typecheck.log`
- `/private/tmp/tezbar-parent-final-ts.log`
- `/private/tmp/tezbar-parent-final-frontend.log`
- `/private/tmp/tezbar-nr-macos-final.log`
- `/private/tmp/tezbar-nr-msrv-final.log`
- `/private/tmp/tezbar-nr-lifecycle-final.log`

Early focused runs exposed invalid placeholder image bytes in the new OCR fixture
and test-harness omissions. They were corrected to use a valid canonical PNG and
the real bridge close adapter. The final full suites include the corrected
regressions; earlier logs are not substituted for final-source evidence.

The coordinator's renderer follow-up covers closing the native Settings window
while Permissions is displayed. Navigation now drains pending AI saves first,
and retains the editor and visible error if a save fails. Once Permissions is
open, the window owner acknowledges native close requests through the same real
bridge. Two real-App/bridge regressions cover successful and failed saves; the
full final renderer suite, typecheck and frontend build passed afterward. A first
new test exposed that the Permissions tab did not display a save error, which was
fixed before the final passing run. No native source changed in this follow-up.

## Windows x64 — exact committed archive

Host `DESKTOP-OB83S8J`, `User@100.77.254.64`, authorized Tailscale SSH fallback.
Toolchain `1.90.0-x86_64-pc-windows-msvc`, `CARGO_BUILD_JOBS=2`.
The release checkout at `C:\Users\User\Desktop\Tezbar` was never changed;
its final status remains clean on `release/0.3.0-beta.7`.

Created with `git archive --format=tar.gz` from native source `8f70d2e`, then
copied to the task temp root. SHA-256 was checked on Windows before extraction:

```text
cb6c63ca3f2440ca896ce5454d333b6043bdd7411028a2a40e2a26ae89749f0a
```

Local archive: `/private/tmp/source-8f70d2e.tar.gz`.
Staging: `C:\Users\User\AppData\Local\Temp\tezbar-native-c7271bc-20261104\rev-8f70d2e`.
Only the sibling `rust-target` task cache was shared. Native output was redirected
inside `cmd.exe /d /c` to avoid PowerShell native-stderr false failures.

| Check | Observed result |
| --- | --- |
| Archive SHA-256 verification | Pass, exact hash above |
| Frozen/offline pnpm dependencies, `--ignore-scripts` | Pass in isolated staging |
| `pnpm build:backend` | Pass, backend resources staged from the same archive |
| `cargo check --manifest-path src-tauri/Cargo.toml --lib --locked` | Pass, non-test Windows library |
| `cargo test --manifest-path src-tauri/Cargo.toml --lib --locked -- --test-threads=2` | 527 passed, 8 ignored, 0 failed; 107.49 s; exit 0 |
| Release checkout status afterward | Clean, read-only status check |

The real Windows run includes the N-R1/N-R3/N-R4/N-R5 native regressions,
including actual model-body shell summaries and fixture OCR through the parent
route. N-R2/N-R6 renderer seams are covered by the TypeScript suite above. It also
includes the corrected npm layout `npx` + `npx.cmd` test,
Windows DPAPI temporary-profile roundtrip, process tree/Unicode tests, all 13 wire
re-review regressions, private-host DNS pinning, key-loss reset and cached-era
startup crash fixtures. Windows behavior was exercised on Windows.

Local captured result: `/private/tmp/tezbar-nr-windows-final.log`.
Full copied native log: `/private/tmp/tezbar-nr-8f70d2e-windows-rust-full.log`.
Recipe: `/private/tmp/tezbar-native-ai-8f70d2e-windows.ps1`, run through the
previously inspected `/private/tmp/tezbar-native-remote.py` helper. Staging retains
`install.log`, `backend.log`, `rust-check.log` and `rust-full.log`.

## Ignored/skipped scope

No ignores/skips were added for this repair. The 9 Mac Rust ignores cover live
GitHub installer discovery, the optional built Bun extension-worker roundtrip,
scaled/release-only search/cache/allocation probes and the headless idle benchmark.
Windows has the corresponding 8 applicable ignores. The 9 Vitest skips remain
unchanged. These checks do not claim live provider interoperability, official-SDK
MCP server interop, desktop visual QA, installer/signing readiness or performance
acceptance. No built-in AI regression is waived by the counts above.

## Fixture coverage map

These are inspectable tests, not live-provider compatibility or performance claims.
All AI fixtures use temporary home/data/config paths and fixture credentials.
NoPi markers and source guards reject owned Pi/Node/npm/Bun/OpenCode execution;
model catalogs use native HTTP plus static metadata; no OpenCode CLI is started.

| Area from the specification | Main evidence |
| --- | --- |
| Built-in providers, five protocols, two tools, user/tool images, continuation | `canonical_tests::every_provider_and_protocol_executes_through_agent_dispatch`; each provider and Copilot/OpenCode protocol route uses real `agent:run` dispatch |
| Signatures, encrypted reasoning, DeepSeek fields, Ollama native fields, UTF-8 framing | `wire::tests` byte-at-a-time decoding and exact-origin replay checks; Gemini unsigned-history and Anthropic unfinished-block tests |
| Durable resume, model switch, image blobs, crash repair | `canonical_tests::durable_signed_history_resumes_by_id_and_switches_models_without_foreign_opaque`, `a_crashed_saved_call_gets_an_unknown_result_and_is_never_reexecuted`, store tests |
| Query/chat/action/metadata/extension callers and aliases | `ai::tests`, `native_tests`, frozen IPC inventory and native profile lifecycle fixture |
| Retry/no replay, deadlines, malformed batches, cancellation | `native_tests::completed_effect_stays_once_across_byte_identical_turn_retry`, `native_next_header_deadline_never_replays_completed_effects`, malformed/edit-batch, header/body/approval/cancel fixtures |
| Permissions, sensitive paths, outside-cwd modes, live Review | `runtime::policy::tests`, `native_tests::native_permission_mode_changes_apply_to_later_tools_in_the_same_run`, remembered/destructive/complex and concurrent approval fixtures |
| Native authentication and catalogs | `providers::tests`: serialized Copilot refresh, session exchange, credential-scoped catalog routing, device cancel/stale/success, Antigravity store preservation/quota/endpoint failover, native search/images and OpenCode auth/catalog; `oauth::tests` PKCE/state/issuer/cancel |
| MCP stdio/HTTP, modern/legacy, trust, calls and secrets | `ext::tests`: compiled stdio fixture plus loopback HTTP/AS fixtures, DCR/PKCE/resource/refresh, session recovery, descriptors/schema, resource/template/prompt images, notifications, concurrent cancellation/disable and crash/flood no-replay checks |
| Encryption and provider secret editing | `ext::secrets::tests`, provider-secret reference/namespace test, renderer password/save checks, Windows DPAPI temp-profile fixture |
| Project imports, instructions, prompt drafts and profiles | Project marker stays absent before review; byte changes revoke trust; contribution ordering/scoping tests; renderer import preview, slash argument and first-turn profile tests |
| Renderer design behavior in both themes | `AiExtensionsSection.test.ts`, `ComposerExtensions.test.ts`, `agentChat/ownership.test.ts`: Test before Enable, tool review, saved password inputs, import without auto-start/send, keyboard arguments and approvals |
| Windows process/file paths | `process::windows_tests::powershell_unicode_and_job_owned_tree`, native file/approval tests, extended-path policy check, MCP compiled stdio and native extension process tests |
| macOS process cancellation | `native_tests::native_cancel_kills_shell_descendants_and_prevents_late_mutation`, process-group fixtures |

The old shell regex bypass cases are deliberately stricter where the coordinator
required sensitive, destructive and unknown/complex syntax to precede remembered
or Turbo shortcuts. Those old permissive expectations are not compatibility
requirements.

## External checks and remaining acceptance step

No live paid AI request, real provider credentials, production profile, desktop
smoke, official-SDK server interoperability session, installer/signing build, or
RAM/latency benchmark was performed. Fixture servers establish Tezbar's side of
wire formats and effect ownership; they cannot establish current provider access,
account entitlements, quota or undocumented upstream behavior. OpenCode's upstream
free-tier restrictions remain respected and actionable. Windows OCR and semantic
UI Automation are separate pre-existing platform gaps.

Initial Opus reviews, the 118cb7b re-review reports and the full runtime
a28c564 report remain unchanged under `docs/reviews`. All N-R1–N-R6, W1–W15,
N1–N8 and U1–U3 repairs are recorded with regressions
in `docs/reviews/NATIVE_AI_REVIEW_FIXES.md`. The coordinator must obtain fresh
Claude Opus 5.5 wire, standards and **full runtime** closure against the repaired
source. Passing fixtures and the repair ledger do not replace that gate. This
document does not mark review closure, live-provider interop or release approval
complete. Older private-resource OAuth grants without trust-zone provenance may
require one new sign-in; locked/denied OS key-store access never enables reset.
