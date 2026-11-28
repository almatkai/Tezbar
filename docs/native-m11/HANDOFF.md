# M11 source preparation — not accepted or integrated

Selected tree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`.
Branch `feat/native-gpui-app`; starting HEAD `409dbf3c5950b0065aff900b2906b548660bc264`.
The shared branch advanced externally to `dd7bc194d335cd33caee45e1ce4d4cc1ef54a9a3` during this lane.
M11 staged/committed nothing. Final hashes distinguish baseline versus current read-only references.
M8 WIP and M9/M10 drafts remain peer-owned. No exports, Cargo/manifests/locks,
existing App/Launcher/core/model/updater/release scripts, versions or defaults were edited.
No build, fixture test, release assembly, signing, profile access, installed-app access,
clipboard, credentials, Keychain, installer, desktop launch or remote work occurred.

The source is deliberately unexported. `rustfmt` success and `node --check` success are
syntax/format evidence only. Every named test below is **UNRUN**. M11, O-LS, O-NSIS,
full migration and updater continuity remain unaccepted.

## Implemented source and exact edges

- `apps/tezbar/src/legacy_state/mod.rs`: `ReadReport`, reason-only diagnostics,
  `prepare(existing, revision, source)`, `commit_candidate(plan, revision, native_exists)`.
  Calls the existing `tezbar_ui_model::ui_state::UiState::{decode,import_legacy}`.
  Returns its real `ImportReport`/`UiState`, not a second durable model. No write API.
- `legacy_state/webkit.rs`: immutable, URI-escaped, read-only SQLite; WAL/SHM/journal
  presence refuses import rather than creating/replaying/changing sidecars. Query-only,
  trusted-schema off, 2 MiB page cache, mmap off, connection-local VM/allocation limits;
  progress callback limits work to about 1,000,000 VM instructions. No extensions enabled
  or loaded. Source file <=64 MiB; selected value <=2 MiB. Length queried before cell copy.
  Schema must be a table; query only the fixed allowlist; caches aren't queried.
  Modern origin decoder checks both top and client origins from the binary `origin`
  sidecar; hashed directory names are never treated as origin proof. Bounded discovery
  under an explicit root, <=256 directory entries and exactly two origin levels.
- `legacy_state/leveldb.rs`: ordinary file reads only. <=256 file entries, <=64 MiB total
  scanned/decompressed bytes, <=4 MiB compressed/log/decompressed block, <=2 MiB retained
  item, <=100,000 parsed SST/log physical records and <=100,000 batch operations. Current
  manifest, bytewise comparator, VersionEdit tags 1–7/9, live table selection/deletion,
  CRC32C masking, fragmented 32 KiB logs, write-batch sequences, SST restart/index blocks,
  internal-key sequences/deletions, raw/Snappy blocks. Declared Snappy output length is
  checked **before allocation**. Obsolete files aren't salvage sources. Log recovery follows
  `number >= log_number || number == prev_log_number`. Equal-sequence contradictory values,
  missing live files, changed inventories, unknown compression/format and damaged input stop
  import. UTF16/Latin1 are decoded with their actual tags, never assumed UTF8. Unknown
  encodings cannot expose an older value behind a deletion. Reads no DB LOCK or user key
  contents into logs; only the fixed UI keys and VERSION can be retained.
- `app/update_integration.rs`: typed `Client`, bounded command queue, watch-coalesced status,
  `Actor::run` future for the **existing** runtime, and finite `Effect`s. One portable
  `Updater`; no HTTP client/policy/parser/certificate logic duplicated in the app. Inbox is
  drained only after reserving capacity. Check/download/restart nonces are from UpdateInbox;
  channel changes use their separate Settings domain. Busy operations reject explicitly.
  Cancel drops one pending future then advances the generation; download RAII remains
  upstream-owned. Check retry starts a new generation. `autoUpdateEnabled` controls automatic
  verified download; Ready retains the verified artifact until explicit restart. A manual
  installer link must exactly equal the current portable manualAvailable result. An internal
  InstallDeferred reply restores ownership when quit is declined. Preview commands are
  rejected; preview constructor/run performs no network or channel-profile read.
- `app/production_launch.rs`: pure `plan(Environment, executable)` → existing host `Profile`,
  actual core `common::CorePaths`, UI-state/layout/legacy roots and startup order. No new
  runtime/process, no production runner, no release-identity bypass. Existing Preview CLI guard
  is untouched. Primary paths, identity, log name, tray/hotkey and 8s shutdown are preserved
  as composition inputs. This plan itself does **not** establish release identity.
- `scripts/native/package-macos.mjs`: pure inventory/metadata/provenance planning, streaming
  inventory capture and an explicit unsealed assembly function for a later authorized build.
  No automatic CLI or imports with side effects. Source symlinks fail closed; parent must
  flatten pnpm resources into reviewed staging. Exact resource roots include worker, vec0,
  SQLite dylib, icon/tray, four pinned helpers, embedding runtime ZIP, license, esbuild and
  Windows scripts. No downloaded model cache is copied, and no Bun process is launched.
  `assembleMacos` creates a fresh OS-temp stage, exclusive output files, copies pinned bytes
  and rehashes output. Failure stages remain inspectable. Result is ASSEMBLED_UNSEALED,
  publishable=false; it is never a substitute for a release receipt or signature checks.
- `packaging/windows/installer.nsi`: locked upstream template, with only WebView2 defines
  and its entire section removed. Same registry/shortcuts/reinstall/uninstall/downgrade,
  passive/silent and RunAsUser code. Includes original utils/FileAssociation and both licenses.
  It still needs the locked Handlebars context, language files and NSIS plugins; no installer
  was generated or run. `windows-rehearsal-plan.mjs` is a pure proposal, with zero commands.

Parent authorized planned edges (no manifest/lock edit in this lane):

| Edge | Exact version/features/license/checksum | API sources |
| --- | --- | --- |
| App → rusqlite | `=0.32.1`, `default-features=false`; reuse core's existing `bundled` feature/engine. `load_extension` is unified by core but reader never enables/loads extensions. No additional reader features. MIT. Registry SHA256 `7753b721174eb8ff87a9a0e799e2d7bc3749323e773db92e0984debb00019d6e` | existing registry source `rusqlite-0.32.1/src/lib.rs`, `types/value_ref.rs`; `ffi::{sqlite3_limit,sqlite3_progress_handler}` through the existing libsqlite3-sys |
| App → snap | `=1.1.1`, `default-features=false`, no features. BSD-3-Clause. Registry SHA256 `1b6b67fb9a61334225b5b790716f609cd58395f895b3fe8b328786812a40bc3b` | saved primary `upstream/snap-api.rs`, crates.io `snap-metadata.json`; `raw::{decompress_len,Decoder::decompress}`; Encoder only in fixtures |

No GPUI pin/toolchain changes: GPUI c84af5b99cbc366d675064e52b23664a015b6e35,
app Rust 1.98.1, core/host/update/models 1.85 gates stay. No Zed GPL UI/editor/terminal crates.
Node is build tooling only; first-party shipped runtime remains Rust/GPUI, Bun compatibility
lazy and core-owned for third-party extensions.

## Parent integration questions / finite gates

1. Export `legacy_state` and the two App drafts only at the safe M8 source/lock gate. Resolve
   exactly the two approved dependencies above. Typecheck and run every source fixture on both
   platforms; FFI callback lifetime, SQLite engine flags and installer bytes are not verified
   by rustfmt. `CorePaths` import intentionally uses `tezbar_core::common`, the real public path.
2. Launcher owns `UiState` and existing persistence. Introduce a narrow bootstrap path that
   prepares import before Launcher/Settings hydration, serializes its revision/existence
   check with `write_atomic`, then hydrates the committed result. No second writer/config
   executor. Existing valid native state wins even when importedLegacy=false/default; corrupt
   native state is preserved and requires recovery. Failed atomic write does not ack import;
   retry the same candidate only while revision unchanged and native path absent. Changes or
   a newer native file supersede it. Interrupted/unsupported transport returns Retry and
   **must not be saved partially by automatic hydration/default persistence**. Malformed
   individual UI values use existing helper defaults and key-only rejection diagnostics.
   Marker is `importedLegacy` in the same atomic JSON, never a separate once file. Successful
   import is required before a future explicit Storage cleanup; this lane implements no
   deletion. Confirmed empty-origin discovery needs its own complete empty report; Missing
   alone is not evidence that legacy state was empty.
3. Mount one updater actor Task through RuntimeOwner/gpui_tokio. Deliver the latest frame
   on the GPUI main thread to Launcher + task indicator + live Settings via their existing
   status/event route and `SettingsContent::set_update`. Connect SettingsEffect Update,
   UpdateChannel and AutoUpdate to typed commands. Route CheckedAt/AutoUpdate/ClearLastCheck
   through Launcher UiState owner. Existing Launcher `CoreEvent::Other("update-status")`
   currently records timestamps for checked_update itself: reconcile that narrow branch so
   one completion causes one write, not both paths. Retain coalesced frames when Settings is
   closed; new Settings must receive current frame. Bound App effect admission and retain
   the one receiver Task; no polling in render.
4. Portable updater currently constructs a reqwest client for each operation. If “one client”
   means one reusable HTTP client, add a finite client field/new_with_client seam **there**, not
   in this app draft. Existing parser, manualAvailable, platform fallback, minisign, pinned
   requirement, TeamID and install implementations remain the authority. Parent coordinator
   must consume Effect::Install using `VerifiedArtifact::stage_macos/stage_windows`,
   `StagedMac::install_over` / `StagedWindows::launch`, then existing quit/relaunch ownership.
   Preserve previous bundle until confirmed relaunch, call rollback on failure. Preserve
   Windows current argv as separate strings passed to `StagedWindows::arguments`, never
   preaggregate an empty argument. On declined quit return artifact through InstallDeferred.
   Stage/install failure after consuming the artifact still needs a finite portable reset/
   failure seam (current portable status can remain Downloading after a successful download).
   Do not claim install retry or relaunch continuity is integrated until this is repaired and
   rehearsed. This draft's Ready means verified download retained, pending restart/install.
5. Production admission: verify current bundle metadata/resources and the existing certificate
   pin before consuming LaunchPlan or constructing updater Config. Do not expose Mode or
   identity admission to env/IPC. Keep supervisor and native-v2 CLI dispatch first. Reuse
   single-instance, tray/hotkey and 8s quit ownership, not an alternate App. Capture exact
   production resources from executable path; do not reuse preview resource overrides.
6. macOS assembly/signing later must run release-guidelines inventory, source SHA and build
   gates first; use existing resource signing, helper TeamID validation, deep/strict outer
   verification and **exact designated requirement equality**, then updater minisign. Existing
   allow-unsigned prerelease exception never authorizes ad-hoc production identity. Explicit
   local-preview is com.tezbar.preview.native, OS-temp only, ad-hoc and never publishable;
   no LSUIElement. Preserve truthful testing-preview/platformVerified receipt fields.
7. O-LS/O-NSIS still require disposable real-format staged profiles/installers. Source fixtures
   do not establish platform acceptance. Modern WebKit binary origin support follows the
   saved primary snapshot; legacy per-origin .localstorage files, alternate WKWebsiteData
   directories, OS-specific revisions and WAL replay are unavailable until traced/rehearsed.
   Multiple valid candidates must not be merged arbitrarily. Windows explicit data-folder
   versus EBWebView fallback must be resolved on a disposable profile before automatic
   discovery is exported. No real user's hive/profile may serve as the experiment.

## Source discrepancies to resolve without inventing product behavior

- §1.5 says 15 browser keys but lists **14**; `UiState::KEYS` and reachable TS agree on 14.
  Ledger P-15 is native window/layout files. See browser-key-map.json: all 15 ledger rows
  mapped, zero invented fifteenth localStorage value.
- Spec updater prose says 4h; reachable App.tsx uses UPDATE_CHECK_INTERVAL_MS, currently
  **12h**, and existing Rust update_policy::INTERVAL is also 12h. Actor reuses the existing
  constant; +3s host and +5s conditional renderer timers are retained, single-flight.
- Tauri locked source supplies app_local_data_dir as WebView2's user data folder. This points
  to `<data>/Default/Local Storage/leveldb`; `<data>/EBWebView/Default/...` is a separate
  candidate in the prose, not a proven actual path for this app. See source-trace.md.
- macOS minimum metadata is 10.13 per spec; that metadata alone proves no GPUI deployment
  compatibility. Build/link and old/new update rehearsals remain mandatory.

## Validation status

Formatting/syntax only: rustfmt on owned Rust; `node --check` on four new .mjs files.
All Rust/JS fixture suites **UNRUN**, no Cargo/Bun/esbuild/build commands. No SDK acceptance,
SQLite acceptance, installer acceptance, typecheck, runtime, AX/IME, memory or Windows result
is claimed. Failed source lookups are retained in upstream/fetch-failures.txt and
failed-experiments.txt. Parent owns integration/reviews/final full Mac+Windows validation,
staged update rehearsals, full Opus review, manual AX/IME/window matrices and matched RAM.
