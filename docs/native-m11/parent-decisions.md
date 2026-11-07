# Parent decisions for M11 integration

These are integration requirements, not passing test/build/rehearsal evidence.
M11's 51 source-only owned-file hashes were checked against source-audit.json with
zero mismatches. Its 25 fixtures remain UNRUN. M10 currently owns shared Cargo,
exports and compiled source; no release to race its frozen gates is implied.

- Preserve the **14 actual browser keys** from `UiState::KEYS` and reachable TS.
  The fifteen ledger rows include native layout/window data; invent no extra key.
  Home/candidate caches remain excluded. Correct the spec count during M11's later
  shared documentation window, keeping the discrepancy/evidence visible.
- Preserve the existing **12-hour update interval**. Both
  `src/shared/updater.ts::UPDATE_CHECK_INTERVAL_MS` and
  `tezbar-ui-model/update_policy.rs::INTERVAL` say twelve hours. The four-hour prose
  is an inventory error, not authorization to change product scheduling. Keep the
  existing +3s/+5s admission rules and single-flight ownership.
- Windows storage discovery is still O-LS. Trace/rehearse the exact locked Tauri
  WebView2 data directory on a disposable profile before automatic discovery. Do
  not treat a guessed missing EBWebView path as proof of an empty legacy profile.
  The same applies to legacy/current WebKit origin encodings and sidecars.
- A stale/corrupt/unsupported/WAL-bearing legacy database is not a completed empty
  import. Preserve source bytes, allow meaningful recovery/retry, and do not
  silently save defaults that permanently supersede still-recoverable user state.
  If supporting WAL requires a consistent bounded private DB/sidecar copy, writes
  to that disposable copy are allowed in a later implementation/test gate; never
  write/recover/compact the original. Prove original immutability with real-format
  fixtures. No actual user profile may be the experiment.
- Add reusable lazy HTTP-client ownership and a finite install-failure/reset seam
  to the existing portable updater in its later released scope. Do not create a
  second policy/parser/signature verifier in App. Installation decline must return
  the retained verified artifact; stage/install/relaunch failure must produce a
  retryable truthful state, preserve/rollback the old bundle, and never remain
  Downloading/Ready falsely. Test generations and channel changes against stale
  artifact returns. These remain unimplemented portable integration obligations.
- Production identity admission precedes production paths/updater/network. Keep
  Preview isolated and network-idle. The default product, old renderer and Tauri
  are switched/removed only after M9/M10/M11 parity and validation; current draft
  formatting is no acceptance. Local package builders are not signed releases.

Release/publication is not authorized. Actual build/sign/rehearsal must satisfy the
project release-guidelines and signing policy; no version/feed/branch promotion is
selected here. Full Mac/Windows/manual/Opus/RAM gates remain outstanding.
