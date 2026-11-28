# Parent verification of M8 repairs and M10 integration

Verified all 359 original source hashes, all 42 owned files, all 33 gate log hashes and the binary against the handoff. The N-B1 patch exactly matches the parent's manifest. Parent independently reran four N-B1 cases, three actual-App failed-quit/continue cases and the four-slot decode lifetime fixture: all pass.

The child gates record native316, bench317, UI57, models379 and targeted core/MSRV checks. The existing full UI alternate-screen failure is resolved by correctly testing Alacritty's inherited cursor and explicit CUP; its original assertion remains after explicit clear/home. No gate was silently skipped or relabelled. Existing lock package versions and checksums are unchanged; the seven new nodes belong to the exact Alacritty0.26 edge.

A parent Windows-only correction after the freeze replaces textual-prefix Path::starts_with checks with parsed Prefix::VerbatimDisk checks. Its idempotence test and all Windows security/race fixtures are unexecuted on macOS. This one file is the only post-handoff source delta. It is excluded from the Mac compiler by cfg; no clean Windows-source or real-Windows pass is claimed. The original child receipt stays unchanged, with the explicit parent delta in repair-parent-verification.json.

The preserved debug binary still says b626f28, dirty=true, preview=true, runtime_shaders=true, bench_control=false; it is not a clean build of the forthcoming commit and is not a RAM measurement. It has no WebKit link or Tauri/Wry/Tao normal tree. No desktop launch, user profile, clipboard, live provider, install, push or release occurred.

Fresh Opus for M7/M8/M10, real Windows, M9/M11, final controller review and physical platform/RAM acceptance remain. The full first-party migration is not complete; the Tauri product remains the shipping default until final cutover gates.
