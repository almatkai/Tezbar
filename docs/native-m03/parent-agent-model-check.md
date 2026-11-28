# Parent check of M3 agent/settings models

Delivered source checkpoint: `8d3d3f0` (22 lane-owned files).
The parent independently checked all 22 delivered hashes, the 31 original TS
source hashes, all 354 mapping rows and all 132 unique mapped Rust test filters
against the passing log. The delivered suite had 190 passing harnesses: 162 new
controller harnesses and 28 existing update-policy harnesses. Case-map counts are
not equivalent to test-harness counts or proof of GPUI view parity.

Reading the shared write-queue merge found one defect with two concrete triggers:

1. Fail an activation of custom provider A; remove A successfully from another
   owner; retry the retired activation. The old implementation selected A after
   its definition and credentials had been removed.
2. Keep an older Settings owner alive; remove A elsewhere; choose A in that stale
   owner. Its newer queue revision also selected a nonexistent provider.

Both parent regression tests failed before the repair. The fix follows the actual
target provider even for global activation intents, prunes failures superseded by
a removal, and requires a live custom definition or an explicit fresh recreation
before accepting provider-specific edits. A third regression proves explicit
recreation and activation still work. The retry case covers both deleting the
active provider and deleting an inactive activation target.

After the repair, the complete `agent-models` suite passed **193/193** with no skips
on Rust 1.98.1 and explicit Rust 1.85 offline. Original source tests were not edited.
Logs are `/tmp/tezbar-native-m03-parent-settings-before.log`,
`/tmp/tezbar-native-m03-parent-settings-after.log` and
`/tmp/tezbar-native-m03-parent-settings-msrv.log`.

These controllers still need effect adapters and GPUI views. In particular, use one
application-owned Settings intent queue across view owners, read durable config
rather than contributed display config, preserve fresh chat/provider selections,
and acknowledge every operation with its token. Real image authorization/decoding,
PTY subscriptions, focus/accessibility, service requests and all actual node views
remain explicit integration work. Three mapping rows remain host/renderer cases:
launcher-hotkey transaction, extension-hotkey event delivery and Detail wheel pixels.
No native UI row is closed by this parent check. Fresh Opus review is pending.
