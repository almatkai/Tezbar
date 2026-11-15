# Native GPUI build tooling

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
`pnpm build` compiles the native executable; private bundle assembly remains an explicit command.

New assemblies use resource manifest schema 2. The manifest records the release version,
source SHA, dirty/bench/shader features, GPUI/compiler policy and a SHA-256 binding to the
exact metadata record read from the copied binary. Both runtime shaders and compiled
Metal are admitted when the binary and manifest agree. The private preview builder chooses runtime shaders; the public release builder
uses compiled Metal and records that distinction in its metadata. Pinned codesign authenticates the
entire executable and seals the manifest/resources; it remains mandatory at production
admission and before update swap. A whole-executable digest inside that same sealed
manifest would create a signing cycle. Schema 1 or record-less archives are deliberately
refused by the new native validator; they require rebuilding, never an admission bypass.
The native release builder keeps the existing updater feed, archive naming and minisign contract.
This validator accepts schema 2, GPUI `c84af5b99cbc366d675064e52b23664a015b6e35`
and Rust policy `1.98.1` only. Future schema/GPUI/compiler policy revisions require an
explicit compatibility and validator-upgrade plan; installed validators do not admit
arbitrary future revisions. `--version` retains its existing fields and also provides
a live reference to the same static record independently of production admission.

Default `dev`, `preview`, `build` and release entrypoints now select native GPUI tooling.
Bun stays cold until an explicit JS/Raycast extension operation; built-in UI,
launcher search and extension metadata run in Rust. `dev.mjs --help` describes
isolated local resource modes. `release.mjs` requires reviewed `--resources`,
a fresh `--output`, the committed version/channel and release notes; `--dry-run`
checks those inputs without building, signing or reading keys.
The retired renderer is retained under `tests/oracles/legacy-renderer` for regression
coverage, and its host/configuration are archived under `docs/native-m11`. Native
policy/icon/catalog inputs live under `packaging`. The historical cutover handoffs
remain historical evidence, not acceptance of the current working tree. Public
signing, Windows installer/update acceptance and publication retain their existing
release gates; source cutover does not authorize publication.

Release-builder constraints remain explicit: produce thin, single-architecture
executables. The record scanner refuses multiple records, including universal
Mach-O binaries; assembly must capture exactly one record before distribution.
Changes to schema, compiler/GPUI policy or Developer ID require an independently
tested validator-compatibility/bridge-release plan against the oldest supported
installed validator. The successor inventory lists that as a release gate.
It also requires the new binary's own non-GUI startup admission check on the
final signed bundle; approval by the previous version's validator alone is
insufficient. The macOS release builder runs the sealed app with
`--verify-installation` before creating its DMG, updater archive or receipt.
This invokes the same admission check as an ordinary launch without opening
windows or accessing a user profile. Private preview assembly skips production
admission because ad-hoc bundles are deliberately inadmissible.
