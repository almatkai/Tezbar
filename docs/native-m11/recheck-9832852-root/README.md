# Parent repairs after the 9832852 M11 recheck

This is a partial implementation handoff on top of 6ffa4e5, not M11 acceptance.
The report is retained byte-for-byte; its digest is in `receipt.json`.

- **R0 / M3:** the true first-install branch creates the native preferences parent
  before its existing atomic-if-absent write. The old test no longer creates the
  config directory artificially: it snapshots first install, creates only data
  and logs, asserts config is absent, then bootstraps. Its full native execution
  awaits the shared source freeze with the status-icon lane.
- **R1 / M2:** preflight checks the actual bundle with `access(W_OK)` and checks
  bundle ownership for a sticky parent, then runs the existing parent-directory
  probe. It rejects symlink bundles before quit. The new real mode-0555 fixture
  fails with the original function, passes after the repair, preserves the old
  contents and cleans its private files. Both updater toolchains pass **43 tests**
  with **one existing ignored signing test**. Cross-user/ACL acceptance is still
  unrun; this is the current user's real filesystem-permission boundary.
- **R2 / H2:** build provenance requires the canonical Git root to equal the native
  workspace and the native build script to be tracked. Release-note inputs must
  also be tracked inside that workspace to permit `dirty=false`. Git uses
  `--no-optional-locks`. The build tool refuses an enclosing Git repository too.
  The actual build script reproduces the old `false:2` in the ignored nested
  workspace; the fixed test passes normal clean/dirty dependency builds, the
  ignored nested workspace before/after edit, external notes and no-Git archives.
- **R4:** the production install-failure log now records the original error,
  including any retained rollback path. This path is traced, not a real install.

`root-small-repairs.patch` and `root-small-source-hashes.json` describe these seven
files before a co-owner extends `build.mjs` or `app/m11.rs`. They are starting
inputs, not a final integrated freeze. Nothing was staged or committed.

All failed attempts remain: an initial exact updater filter matched zero tests
and the wrapper refused to count it; the corrected filter demonstrates the real
failure. A macOS `mode_t` conversion failed compilation, then `u32::from` corrected
it and both final updater runs passed. Raw logs retain their trailing newlines.
The scoped updater gates hashed their crate and direct policy/manifests; they do
not claim the concurrently edited native application stayed frozen.

R3/R5/R6/R7/R8 need the separate integration repair. Cold-start timing, the other
recorded observations and all Windows, installer/signing, profile, GUI, IME,
accessibility, GPU and RAM acceptance remain open. The product default and held
cutover patch are unchanged. No installation or publication occurred.

`preflight-first-attempt.rs` retains the code that failed on macOS mode_t;
`preflight-final.rs` and the final source hashes contain the corrected conversion.
