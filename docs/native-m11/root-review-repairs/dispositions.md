# M11 root repair implementation, 2026-11-07

Base: fec3257. Original review `/tmp/tezbar-gpui-m11-opus-c01fa31.md` remains unchanged. This report records implementation and bounded headless evidence; it does not accept M11, a platform, the updater or full migration. No actual install/signing/profile/GUI work occurred.

| ID | Disposition |
| --- | --- |
| H1 | Backup ownership is now finite RAII. Successful process spawn explicitly closes/removes the previous-app directory. Spawn failure rolls back; failure to restore retains both recovery files with an exact diagnostic. Directory mtime is refreshed before returning the installed swap. The completion boundary deliberately matches the old updater's successful relaunch spawn, NOT a new-GUI-ready health check. The old claim that backups would be kept until readiness has been removed; no readiness handshake is claimed. Private bundle-swap tests cover rollback and successful cleanup. |
| H2 | Actual native build.rs rechecks Git on every Cargo invocation using a never-existing OUT_DIR provenance sentinel. Git/status failure, missing Git metadata or explicit SHA mismatching observed HEAD all mean dirty. A real two-crate temporary Cargo/Git fixture uses the actual build.rs: clean false:1 → dependency edit true:2 → commit false:2 → archive + explicit SHA true:2. No archive attestation bypass added. |
| M1 | The held cutover generator preserves original text newlines and emits standard no-newline markers. git apply --check is a required gate. Its default build routes to --release plus explicit private assembly, requiring --resources/--output before Cargo; it cannot silently produce a debug binary. This remains a review-only cutover, not applied or a public-release implementation. |
| M2 | Before quit/quiescence, macOS probes a newly owned private directory in the installation parent. Missing/unwritable parents refuse with an explicit move-to-writable-location message and return the verified artifact to the actor. No new admin prompt/elevation path is introduced. Actual standard-user /Applications acceptance remains O-signing. |
| M3 | First-install evidence is captured before this process creates profile/instance directories. Only absence of the known legacy container AND both core config/data directories permits automatic defaults when the read still reports Missing. Existing/ambiguous/unreadable upgrades stay fail closed; their message now explains both first-install and upgrade recovery. Existing native state still wins. The data directory is created after the snapshot, before Instance::claim (cold-start canonicalization otherwise failed). Private first-install/missing-upgrade tests cover the boundary. |
| M4 | The eight-second coordinator has a distinct ForcedAfterCommit outcome. Deadline before cancellation still refuses quit. Deadline after irreversible cancellation proceeds to bounded core shutdown/exit and explicitly logs unknown final-write completion; it never resumes a half-cancelled session or labels unknown writes saved. Known ordinary final-write errors retain the previous retry/cancel path. Two deterministic tests invoke the production deadline helper; real Cocoa timing and live terminal/provider acceptance are not inferred. |
| M5 | Confirmed against [LevelDB 1.23 log_reader.cc](https://raw.githubusercontent.com/google/leveldb/1.23/db/log_reader.cc): EOF truncation and incomplete final fragmented records are ignored. Implemented only for the newest live log, with RecoveredLogTail diagnostic. Older-log, interior-record, manifest, checksum and unknown-format errors remain fatal. Tests preserve committed values, deny the same torn log when a newer one exists, and keep checksum rejection. The prior torn-tail assertion intentionally changed to the verified upstream semantics. Windows execution stays blocked. |
| M6 | OPEN performance/platform measurement, not claimed fixed. Identity/resource admission remains before production environment/profile access, as the spec requires. Normal in-process hotkeys and AppKit reopen callbacks do not run main/admission again; an explicit second executable launch does. Reordering it without a safe pre-admission profile protocol would weaken the boundary. No cold signed-build latency has been measured, so no speed claim or numerical impact is inferred. Resolve during the signed O-signing/single-instance rehearsal. |

## Low/observation reconciliation

- L1: main uses args_os with explicit preview Unicode error instead of panic; Windows updater relaunch uses a lossless UTF16 NSIS path. Existing Unicode quoting tests remain; a surrogate-containing UTF16 test executes on macOS but is not a Windows host pass.
- L2: WebView2 accepts one unique direct OR EBWebView Default store, refuses an ambiguous pair. A real synthetic nested LevelDB fixture imports the expected theme. Actual old Windows UDF discovery remains O-LS; [Microsoft's UDF documentation](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/user-data-folder) describes subfolders but does not prove Tezbar's installed layout.
- L3: AutoUpdate reaches the actor only after its UiState write succeeds. Durable acknowledgements have monotonic tickets, so late older completions cannot reverse newer saved choices; failed actor sends retain the exact saved choice for explicit Retry and fail the existing barrier. No second writer/config executor added.
- L4: successful recovery applies imported appearance/tokens immediately, then refreshes windows before core/updater start.
- L5: OPEN non-gating privacy/OS rehearsal item: WAL recovery's 0700 temporary snapshot is removed normally, but a crash may leave a copy. No unsafe wildcard/PID-only cleanup or user-profile access added. A bounded ownership-aware orphan sweeper needs separate acceptance; originals remain untouched.
- L6: values alone tolerate lone UTF16 surrogates with replacement and key-only RepairedEncoding diagnostic. Keys, origin identity and malformed byte lengths remain strict; no values/paths are logged.
- L7: build helper resolves Cargo's actual target directory and debug/release profile. It captures source commit/dirty/content digest before building, rechecks it afterward and before assembly, and compares built executable metadata. No stale target/debug packaging under a newer source receipt.
- L8: --local-preview without both assembly inputs now fails before Cargo instead of silently building without packaging.
- L9: recorded scheduling/parity observation remains explicit. No duplicate updater actor/runtime or public feed change.
- L10: download-byte hardening observation remains open; current locked updater signature/HTTPS and existing timeout policy unchanged. No new live network tests.
- L11: scripts/native/README.md now explains why ad-hoc previews require explicit isolated --profile/--resources arguments and are not double-click production apps.

## Limits

Final gate receipts and failed compile attempts are preserved separately. Existing UI/models/core/worker/TS component evidence is reused only where sources are unchanged. New updater code is tested on both 1.98.1 and 1.85, with its existing live-network ignore retained. No Windows, physical GUI/IME/AX/GPU/RAM, old-profile discovery, certificate signing or real updater/NSIS rehearsal is claimed. Tauri remains default; cutover patch remains unapplied. Fresh independent review is still required.

## Additional edge regressions

The root's first newest-tail implementation was too permissive at the end of a
complete 32KiB physical block. The strengthened regression failed, then passed
after limiting tolerant physical EOF to a short final block, matching LevelDB's
reader distinction. Full-block crossing remains corruption. `m11-leveldb-boundary-red`
is preserved. The source snapshot helper also initially treated broken symlinks
as missing; a real Git fixture reproduced identical digests for changed link
contents. It now uses lstat and hashes the link target even when it is broken.
Both the RED and passing Node logs are retained. No test was removed or skipped.
