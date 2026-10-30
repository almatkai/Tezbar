# Independent runtime-package review

Reviewed commit `cf8001b9b81c40c403ba7083df7b9a18aef39a1d` in the authorized
performance worktree. The [implementation report](beta17-runtime-package-results.md)
retains its original measurements and hashes. This review adds no new speed or
RAM claim; uniform fivefold runtime/search and one-fifth RAM remain unachieved.

The parent independently checked all 23 source receipts, 13 executable receipts
and 579 evidence receipts against the recorded bytes, with no mismatch before
the review fix below. Five package-pin checks, 16 release-policy checks and eight
selected release Rust checks passed. The selected Rust checks include bundle
installation, cancellation, corruption repair, failed-publication rollback and
Q4/legacy-Q8 compatibility.

The parent also launched the actual ordinary release app in a fresh isolated
profile. It loaded the pinned bundled library through the production path,
preserved the prior model manifests, and left zero owned survivors. The app
contained neither benchmark marker. Evidence remains in ignored
`build/performance-runtime-20261030/parent-ordinary-check/`; its result records
executable SHA `ebd2178d4d2911e756924a7d40bf7e9383ef7cad79d58139297b0a1c8b4ee972`
and package SHA `75698aa84eb35398cee7b3afb6fee7b9bdb1db7a3aedc3cc43af25780100e475`.

## Review fix: unbundled development lookup

`CorePaths.resource_dir` is the source root for an unbundled development app.
The package implementation looked under `<source>/embedding-runtime/`, although
the verified build hook stages the ZIP under
`<source>/build/embedding-runtime/resources/`. The parent reproduced that the
former file was absent while the latter existed. An actual model launch from
that development configuration would therefore fail to find its runtime.

The resolver now uses the staged location only when both the development build
condition and the exact source-root identity hold. Packaged and release paths
still require their own app resource, with no fallback or environment override.
A regression checks the development path, packaged paths in both modes and the
absence of a source fallback in release mode. All four bundle regression checks,
typecheck and all 747 TypeScript tests pass after this fix (9 tests skipped).
The full serialized release Rust suite also passes: 449 tests, 8 ignored; log
`build/performance-runtime-20261030/parent-rust-suite.log`. Formatting and diff
checks pass.

The package/library pins and production release resource location are unchanged.
The ordinary activation above tested the original reviewed release executable;
it is not mislabelled as a rebuilt executable of this follow-up source commit.
The final cumulative comparison must record its own rebuilt candidate identity.

## Remaining work

The round-by-round measurements use different immediate baselines. A final
comparison against the original beta.17 production code is still needed to
report total gains without multiplying ratios from separate cohorts. Publication,
Developer ID credentials, installed-app replacement and other worktrees remain
outside this task's existing authorization.
