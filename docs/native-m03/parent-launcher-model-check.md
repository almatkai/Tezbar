# Parent check of M3 launcher/utility models

Delivered source checkpoint: `97e6cab`. The parent verified all 24 owned source/data
hashes, all 24 provenance records, all 170 mapped source-case filters and the six
promoted prototype session filters against the saved passing logs. The original
19 TS files still passed their 170 tests; owned-table verification reports exact
585-emoji and 98-currency-alias equality with project source.

The delivered feature-isolated checks were 138 UI-model harnesses, 651 core tests
with eight existing ignores, 87 instant-answer tests and nine release-note tests.
These are overlapping package/filter counts, not an additive acceptance total.
Source-case count is also separate from the number of Rust test functions.

Parent source review found two rich-HTML compatibility failures:

- Legacy self-closing `<br/>` became visible literal markup instead of a line break.
- Named character references such as `&copy;`, `&eacute;` and `&mdash;` became visible
  entity text. The original editor decodes them through a textarea's HTML parser.

Both regressions failed before the fix. Sanitization now canonicalizes self-closing
breaks, and a text-only HTML tokenizer decodes character references in one pass.
Three permanent regressions also cover no double decoding, unknown entities,
multi-code-point named references, semicolonless references and HTML numeric rules.
A separate Node/jsdom fixture confirmed the old editor's DOM behavior for breaks
and textarea decoding. No real browser, profile or clipboard was accessed.

The optional `html5ever 0.29.1` dependency is confined to `launcher-models`; it is
MIT OR Apache-2.0 and has Rust 1.70 minimum support. The tokenizer collects text in
RCDATA state without a matching end tag. It constructs no DOM or WebView, interprets
no script and performs no I/O. Existing document size/depth/undo bounds remain.

The integrated `launcher-models,agent-models` suite now passes **306/306**, with no
skips, on Rust 1.98.1 and explicit Rust 1.85 offline. This includes 28 shared update
policy cases only once. Logs:

- `/tmp/tezbar-native-m03-parent-notes-before.log`: the two reproduced failures.
- `/tmp/tezbar-native-m03-parent-combined.log`: integrated Rust 1.98.1 pass.
- `/tmp/tezbar-native-m03-parent-combined-msrv.log`: integrated Rust 1.85 pass.
- `/tmp/tezbar-native-m03-parent-combined-lockcheck.json`: unchanged watchlist and
  Tauri registry versions; only explicit new parser packages in the root lock.

The native app now enables both controller feature groups; the standalone GPUI
component crate keeps them opt-in. Effect adapters, actual surface behavior,
currency/time locale formatting, platform IME/accessibility, M11 legacy storage
discovery and full-app memory verification remain separate acceptance work.
Quicklinks preserve their existing legacy read/search/open-url behavior; no CRUD
surface or backend API was invented. Fresh Opus review of this slice is pending.
