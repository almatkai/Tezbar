# Private native build tooling

Local-preview bundles are ad-hoc test artifacts and cannot be opened by double-click:
launch their executable with the existing `--profile <marked OS-temp directory>` and
`--resources <matching Resources>` preview arguments. No-argument launch is reserved
for production admission and intentionally refuses an ad-hoc bundle.

`node scripts/native/build.mjs --release --local-preview --resources <reviewed staging>
--output <fresh OS-temp child>` compiles release and assembles a private preview.
Both assembly paths are required before Cargo runs. The executable is selected from
Cargo's JSON compiler-artifact messages by the native manifest and bin target, including
configured triples and custom target directories. Missing or ambiguous artifacts fail
closed. Check mode expects no executable. Rendered compiler diagnostics remain visible.
The built binary's static metadata and the unchanged source digest must agree before
assembly. No executable probe is needed. This is not a public signing/release path.
The held cutover patch forwards these same explicit assembly arguments from `pnpm build`.

New assemblies use resource manifest schema 2. The manifest records the release version,
source SHA, dirty/bench/shader features, GPUI/compiler policy and a SHA-256 binding to the
exact metadata record read from the copied binary. Both runtime shaders and compiled
Metal are admitted when the binary and manifest agree. This build tool still explicitly
chooses runtime shaders. Pinned codesign authenticates the
entire executable and seals the manifest/resources; it remains mandatory at production
admission and before update swap. A whole-executable digest inside that same sealed
manifest would create a signing cycle. Schema 1 or record-less archives are deliberately
refused by the new native validator; they require rebuilding, never an admission bypass.
No updater feed, archive name, minisign contract or updater client changes here.
This validator accepts schema 2, GPUI `c84af5b99cbc366d675064e52b23664a015b6e35`
and Rust policy `1.98.1` only. Future schema/GPUI/compiler policy revisions require an
explicit compatibility and validator-upgrade plan; installed validators do not admit
arbitrary future revisions. `--version` retains its existing fields and also provides
a live reference to the same static record independently of production admission.

The public native signed release builder/feed pipeline is still a required cutover gate.
`scripts/build-release.mjs` invokes Tauri and will not work after relocating `src-tauri`.
The new review-only successor and the required post-apply validation are recorded in
`docs/native-m11/recheck-followup/handoff.md`; neither held patch is applied or release-ready.
