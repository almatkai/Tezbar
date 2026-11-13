# N-R2X residual: SVG CSS matching estimate

The 2026-11-09 review of `6ffa4e5` found two remaining underestimates:
selector `Display` can produce a string whose tokenizer silently stops early,
and a class selector scans the attribute value rather than doing constant work.

The repair always counts printed whitespace as a descendant-combinator upper
bound. It no longer re-tokenizes the printed selector. The estimate also charges
each step for the maximum sum of attribute-name/value bytes on one XML element,
covering both attribute lookup and the full `~=` class-value scan. Saturating
arithmetic and the existing matching-work ceiling reject excess work before
usvg is called. This is deliberately conservative; whitespace inside attribute
values can only increase the estimate.

Two permanent regressions exercise preflight without executing an expensive
selector matcher. The quoted-value case proves tokenization ends successfully
but loses twelve descendant combinators. The class case uses a 1 MiB value.
Both were admitted before the repair and are now rejected by the work ceiling.

## Verification

- Before repair: the original descendant test passed; both new tests failed.
- After repair: 8 SVG tests passed, including the ordinary CSS, clip/mask,
  opacity and straight-BGRA pixel checks, stylesheet and layer/dash bounds.
- Typecheck passed; TypeScript: 748 passed, 7 existing skips.
- Scoped rustfmt and diff checks passed.

Rust validation used `rustc 1.98.1 --test` on the byte-exact production
`decode_svg`, `svg_tree` and `svg_preflight` functions and the permanent test
bodies, linked to the existing workspace dependency rlibs. No full native
Cargo/bench build was performed on the concurrently edited application tree.
`green-source.json` records the extracted-source hash and the exact command;
`receipt.json` records the two owned source hashes and evidence hashes.

The repair remains unstaged alongside separate ongoing migration work. It
does not establish independent-review closure, Windows or physical acceptance,
GPU/RAM results, or migration/cutover acceptance. The O-SVG-CPU observation is
unchanged. Historical review artifacts and evidence were preserved.
