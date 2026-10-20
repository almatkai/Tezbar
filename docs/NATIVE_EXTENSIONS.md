# Native extension pilot

The first native port is the installed **Perplexity** extension's
`ask-perplexity` command. It uses the same launcher entry, settings and React
Form renderer as the JavaScript extension. Rust owns its command execution,
validation, URL construction and runtime sessions.

This is a compiled-in port of one extension, not a TypeScript-to-Rust compiler
or a replacement for the entire Raycast SDK. Unported extensions continue to use
the isolated compatibility worker.

## Trying the pilot

Build and launch this branch of Tezbar with Perplexity installed. Search for
**Ask Perplexity**. A nonempty `query` launch argument opens the search directly;
launching without an argument displays the **Ask Anything** form. The existing
**Use Perplexity App** command preference selects the native app URL instead of
the browser URL. The Perplexity app must be installed to handle its URL scheme.

The port opens Perplexity; it does not fetch or display an AI answer inside
Tezbar. It requires no Perplexity API key. The native command does not need a
JavaScript bundle or Bun at execution time; installing other extensions may
still start the worker.

## Behavior retained

- Empty queries fail with `Query cannot be empty`; whitespace is preserved.
- Browser searches use `https://www.perplexity.ai/search?q=…`.
- App searches use `perplexity-app://search?q=…`.
- Query encoding follows `URLSearchParams`: spaces become `+`, literal plus
  signs become `%2B`, and Unicode is encoded as UTF-8.
- Extension and command preferences use the existing registry and profile.
- Opening failures are reported, allowing the user to correct settings or retry.

The original Raycast source declares drafts and fallback text. Tezbar's current
launch contract does not supply these, and its JavaScript host treats form
values as ephemeral. This pilot does not add persistent drafts or fallback-text
launches. It reuses Tezbar's existing navigation behavior.

## Verification

On 2026-10-21, type checking, 594 TypeScript tests (9 skipped), 175 native
library tests (2 opt-in tests ignored), and 8 release-policy checks passed.
The real Rust-host compatibility-worker smoke also passed separately with
Bun 1.2.5, verifying that JavaScript extensions remain available.

Six native pilot tests cover the public extension dispatcher with isolated
profiles, no JavaScript bundle and recording host adapters. They check form
creation, validation, scoped preferences, exact URL encoding, failed opens,
concurrent/cancelled actions, bounded session capacity, disposal, uninstall,
restart and worker fallback. Test queries never open a real browser or app.
All six also passed in the optimized release test build on Rust 1.85.

The local macOS application was built with Rust 1.85 and ad-hoc signing. An
isolated profile containing the installed Perplexity manifest and icon, but no
extension JavaScript or Bun, reached native-core readiness with no child process.
Desktop automation could not attach to the release window, so this is a startup
check, not a completed visual or live browser/app-opening check.

## Source and attribution

Behavior was checked against the locally installed manifest and compiled
`perplexity/.sc-build/ask-perplexity.js`, which retains the source marker
`src/ask-perplexity.tsx`.

- Extension author: **third774**; contributor: **sbertix**.
- [Upstream source](https://github.com/raycast/extensions/blob/master/extensions/perplexity/src/ask-perplexity.tsx).
- The manifest declares MIT; the upstream repository's notice is retained in
  [raycast-extensions-MIT.txt](../licenses/raycast-extensions-MIT.txt).
- Installed icons remain in the installed extension; no icon is copied into
  this port.

See [RUST_CORE.md](RUST_CORE.md) for the host architecture and build commands.
