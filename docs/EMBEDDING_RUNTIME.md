# App-owned EmbeddingGemma runtime

macOS arm64 bundles **b11474-tezbar-mean-no-logits-v1**. This is an app payload,
not a new model variant. Q4/Q8 weights, revision, model keys, stored vectors,
8192 context/batch/ubatch, threads, CPU parking and 120-second retention stay
unchanged. macOS Intel, Windows x64/arm64 and Linux x64 retain the existing
checksum-pinned upstream b11474 catalog; they do not select this arm64 payload.
Their patched-library recipes/execution have **not** been validated on this host.

## Trust and lifecycle

`src-tauri/runtime/embedding-macos-arm64.json` pins the ZIP and every extracted
file. `build.rs` refuses a missing/mismatched staged ZIP on macOS arm64. The ZIP
is bundled at `Resources/embedding-runtime/embedding-runtime.zip`, not fetched
from an unpublished URL or embedded into executable code pages. Ordinary and
benchmark apps use the same fixed resource lookup. There is no runtime/library
environment override.

Unbundled development apps use the verified staged ZIP under
`build/embedding-runtime/resources/`. This source-tree location is selected only
for development with the exact source resource root; packaged and release apps
never fall back to it when their own resource is missing.

Merely verifying installed weights does not extract a disabled model's runtime
or add that work to opening. Actual prewarm/launch verifies the app resource and
materializes the package under `<data>/knowledge/runtimes/<archive SHA-256>`.
Staging uses a sibling temporary directory, exact flat-file inventory, bounded
ZIP entries, file hashes and atomic rename. Cancellation before publication
removes staging. Failed replacement restores the old directory. Two variants
serialize package installation, not inference. Corrupt caches are never executed:
they are reconstructed from the authenticated bundled package, with the previous
cache retained until staging verifies. This repair does not download weights.

The in-process cache is populated by hashes, not by trusting a mutable manifest.
Foreground requests only check fingerprints; changes require background hashing.
No model `installed.json`, original runtime, weights or Knowledge database is
rewritten for this upgrade. An offline already-installed model works without a
runtime download. Rolling back the app still finds its untouched upstream runtime.
Fresh model installation keeps its existing pinned/atomic downloader and stores
that upstream runtime for backward compatibility; it then selects the app payload.
Old package directories are not eagerly deleted under running model processes.

## Reproduce and stage (local only)

`build:backend` automatically verifies the staged resource, or builds the pinned
local-only package if absent. It fails on mismatched staged bytes, never changes
pins, and never signs with distribution credentials. Non-arm64 targets package
only the license and keep their existing runtime defaults. Direct Cargo builds
require staging first. The release wrapper rejects this local-ad-hoc catalog for
signed distribution before reading signing secrets.

From the selected worktree, on macOS arm64 with Command Line Tools/AppleClang 21,
Python 3.9+ and Node/pnpm/Rust build prerequisites:

```sh
# Use a new output directory. No global pip/CMake installation is performed.
python3 scripts/build-embedding-runtime.py --output /tmp/NEW-runtime-build
# Optional: --source-archive, --official-archive, --cmake reuse verified local inputs.
# Optional: --library-build reassembles an audited result from the source recipe.

# This verifies against the COMMITTED pin; it never edits the pin.
python3 scripts/stage-embedding-runtime.py /tmp/NEW-runtime-build/embedding-runtime.zip
pnpm typecheck
pnpm test
pnpm release:test
python3 scripts/test-embedding-runtime.py
pnpm build:frontend
cargo test --release --manifest-path src-tauri/Cargo.toml --lib --locked -- --test-threads=1
```

Source is fixed at `b9acf138a1e28ce1fc23b5a4fc4b12444b50f7ea` (archive SHA
`0b7309d385c523c636231c10f808b226166fec13332277406361463ee2dc885d`). The
versioned patch is `docs/performance/llama-b11474-embedding-output.patch`; the
shared source build recipe is `docs/performance/build-llama-memory.py`. That recipe
builds both unchanged control and patched libllama, records compiler/CMake cache
and dependency/load commands, normalizes source-path macros, fixes RPATHs, and
ad-hoc signs the library without a timestamp. It reuses isolated CMake 3.31.6 or
installs its checksum-pinned macOS wheel into its own output directory (SHA
`da9d4fd9abd571fd016ddb27da0428b10277010b23bb21e3678f8b9e96e1686e`).
Source and CMake builds work with spaces in the selected worktree path.
Toolchain/SDK versions matter:
a different toolchain must produce a new review candidate, not silently alter pins.

The assembler downloads and checks the official arm64 archive (SHA
`ae7f002f2b477d14fea8ac4d93438684344cd18ec10d2b0ee9d5518b2a74def5`). The server,
common, all ggml numerical backends and Metal code are **unmodified official
bytes**. Only libllama and its aliases change. Internal aliases are materialized
as regular files; the deterministic ZIP contains provenance and upstream MIT
license (also in `licenses/llama.cpp-MIT.txt`). All `@rpath` dependencies must be
present, and non-system absolute dependencies are rejected. Generated ZIPs,
source/build trees and binaries belong in ignored artifact directories, never Git.

## Patch scope and review

Only an embedding-enabled Gemma-embedding2 context with mean pooling and no
samplers omits the vocabulary-logits output allocation. Other architectures,
non-mean pooling, generation and sampling retain logits. The following sampler
branch in upstream `output_reserve` still forces logits. Encode/decode copies
check the logits pointer; output reorder checks its size. Pooled embedding
storage and graph/kernel execution are unchanged. The app always uses the guarded
encoder path, not generation. Exact payload and boundary evidence is linked from
the performance report; do not shrink ubatch to hide allocation growth.

## Publication boundary

The checked local candidate is **ad-hoc signed**, not Developer ID/notarized.
Nothing here publishes a release, changes an installed app, edits channel feeds,
or signs with private credentials. No separate remote runtime asset is required.
Parent-owned release preparation can invoke the assembler with an explicitly
approved `--signing-identity 'Developer ID Application: …'`. That unexecuted path
signs/verifies every nested Mach-O before hashing; automatic preparation never
uses it. Review/commit its new Developer ID ZIP/catalog pins, stage that exact
package, rebuild and validate the app, then sign/notarize the app and follow
[the reviewed multi-platform release procedure](release-channels.md). Signing changes bytes: the current local
hashes are not hashes of a hypothetical distributable package. That external
credential/publication step remains unauthorized. Other platform defaults must
not be changed to this archive or to nonexistent assets.
