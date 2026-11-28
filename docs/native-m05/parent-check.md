# Parent verification of M5 frozen delivery

The parent independently verified the final delivery against the on-disk
source and evidence before committing it:

- All 75 compiled input hashes and all 13 M5-owned hashes match, including their
  overlaps. All 18 source-reference hashes in the case map also match.
- Each recorded gate exited zero, without timeout, at unchanged `f3234cd` and
  unchanged hashes: both former native repros; normal app **85** and UI **43**;
  bench-control app **86**; native check, normal debug build, fmt and diff check.
- The preserved binary matches SHA-256
  `4191758ba707e15f1ae7f97c02a914461fe737a56138ebfad15e321dae2f9c18`.
  The parent independently inspected its linkage: no WebKit/Tauri/Wry is linked.
  Its receipt correctly records a preview, runtime shaders, dirty source at
  `f3234cd`, and `bench_control:false`. It is not a clean release artifact.
- The map records 50 M5 tests and 88 L/A/K rows. Test coverage does not substitute
  for the seven listed manual gates or for M6–M11 destination contents.

Evidence: `/tmp/tezbar-native-m05-evidence/parent-final-v3-verification.json`,
`final-v3-gate-results.json`, `final-v3-normal-build-receipt.json`,
`final-v3-compiled-source-sha256.json`. Earlier failures and model repairs remain
in the handoff; no tests or skips were weakened to obtain these results.

The parent also checked the additive `QueryOwned` wire against the core's owned
query contract and the public navigation enum against the router. Production
code starts no new runtime/process/provider client. A fresh Opus review still
must inspect the complete M5 implementation and repaired M3 controller paths.
Visual/OS/IME/accessibility/Windows/GPU/RAM acceptance remains open.
