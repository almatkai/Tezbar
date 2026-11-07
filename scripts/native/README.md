# Private native build tooling

Local-preview bundles are ad-hoc test artifacts and cannot be opened by double-click:
launch their executable with the existing `--profile <marked OS-temp directory>` and
`--resources <matching Resources>` preview arguments. No-argument launch is reserved
for production admission and intentionally refuses an ad-hoc bundle.

`node scripts/native/build.mjs --release --local-preview --resources <reviewed staging>
--output <fresh OS-temp child>` compiles release and assembles a private preview.
Both assembly paths are required before Cargo runs. Cargo target directory overrides
are resolved through metadata; the built binary's own provenance and the unchanged
source digest must agree before assembly. This is not a public signing/release path.
The held cutover patch forwards these same explicit assembly arguments from `pnpm build`.
