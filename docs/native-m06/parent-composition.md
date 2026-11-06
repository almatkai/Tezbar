# Parent M6 composition and verification

The native Launcher lazily creates one retained `Utilities` owner using its existing
`CoreClient`. It supplies the current updater status, native locale formatter,
release-note identity, core event subscription, and existing `UiState`. Native
utility content replaces the command-bar chrome while that surface is active.
Each delivered navigation is acknowledged without discarding other destinations.
Back restores the command field; hide stops the utility's visible work and retains
durable edits.

Typed utility effects use the existing app owners. Emoji paste sends one canonical
`copy-and-paste-text` action: the core awaits HideForPaste, restores focus, waits
80 ms, and presses the paste shortcut. The UI does not perform a second clipboard
write or paste. QR export uses the existing leased PNG save dialog. Extension
activation shares the successful-view gate; a stale completion cannot change the
current surface. Updater actions enter the separate typed updater inbox for M11.

Quit starts a fresh Launcher-state save and a utility drain. Their errors remain
independent; a successful config or preference write cannot clear a failed note
save. Each explicit utility drain retries failed note owners once, awaits admitted
durable effects, and reports a fresh failure without an automatic retry loop.
Its completion notification is enabled before checking work, preventing a missed
wakeup. The unchanged parent test caught the earlier polling-clock stall, which
remains recorded in the lane's failed log.

Four parent fixtures exercise the actual Launcher and Utilities entities: ordered
navigation and acknowledgement; Back and native command input; a single canonical
paste effect; and failed-note quit blocking followed by a successful explicit
retry, with independent Launcher-state error ownership.

## Evidence checked by the parent

- All 21 lane file hashes match `file-hashes.json`; its SHA-256 is
  `f44be4d16fe3c977e86cc521a79beacb7f178d5d810b1a3409795f8f3527acc8`.
- All 19 source-reference hashes and all 28 named M6 fixture definitions match
  `source-case-map.json`.
- Lane final logs record **128 native tests** (including four parent fixtures)
  and **47 UI tests**, with zero failures or ignored tests.
- Parent check, normal debug build and **129 bench-control tests** passed on Rust
  1.98.1 with `--locked --offline`, two build jobs and two test threads.
- All 351 compiled-input/reference hashes stayed fixed during those parent gates;
  exact paths and commands are in the evidence files below.
- Package formatting and whitespace checks passed. Neither Cargo lockfile changed.

Parent evidence is under `/tmp/tezbar-native-m06-parent-evidence/`: `gates.json`,
`compiled-source-hashes.json`, `map-verification.json`, individual logs and
`normal-linkage.txt`. The saved normal preview binary is `tezbar-m06-normal-preview`,
SHA-256 `573fdb309bf794029ee033969c3a5aef54c128582895b3322ce9755a79cce759`.
It truthfully reports source `35dca9bad4c44198dbc1c1e2e7b687ab0b6a9b2f`,
dirty `true`, preview `true`, runtime shaders enabled and bench control disabled.
Its linkage contains no WebKit. This is a pre-commit integration binary, not a
clean-source, signed-distribution or installed-app result.

The earlier canonical Notes-selection and bounded release HTTP contracts were
committed separately in `30f7084`; see `parent-contracts.md` for their independent
Rust 1.85/1.98 evidence. No unchanged whole-core, frontend or Bun suite was rerun
for these Rust-native view changes. Their final migration gates remain required.

Independent Opus review, physical platform/IME/accessibility checks, screenshots,
full-app RAM measurement and M7–M11 remain outstanding. No shipping-app install,
real profile or clipboard access, push, PR, release or Actions run occurred.
