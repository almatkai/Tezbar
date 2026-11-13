# GPUI application cutover — 2026-11-13

Selected worktree: `Tezbar-native-rust-agent`, branch `feat/native-gpui-app`, base
`0228b4fa289d5fce78bc3cf179575c4011c59f7c`. The current migration is uncommitted.
This is source/tooling validation, not a public release or installation receipt.

## Runtime ownership

All application and extension presentation uses Rust/GPUI. Default development,
build, preview, Windows and release routes select native tooling. Node/pnpm are
build/test tools. The former renderer is retained only under
`tests/oracles/legacy-renderer`; retired Tauri source/configuration is under
`docs/native-m11/legacy-tauri-archive`. Signing/resource policies and icons moved
to `packaging` with their pinned contents preserved. Native onboarding embeds
its icon from `resources/app-icons`, outside the test oracle.

JavaScript/Raycast compatibility remains an isolated worker. Only explicit
run/install/reinstall requests may discover/start Bun. Startup, launcher search,
settings, metadata, preferences and built-in services stay in Rust. Stale session
controls cannot activate or revive a worker. A new explicit run can recover after
a crash; an already-used live worker remains available for timers/background
ownership until shutdown/restart. Native Rust extension restoration is unchanged.

The updater signer CLI remains a build utility for the existing signature format;
no Tauri application build, frontend mount or WebView fallback is allowed.

## Validation

- `pnpm typecheck`: passed after source relocation.
- `pnpm test`: 748 passed, 7 explicitly skipped; all legacy oracle tests retained.
- Native application library, rebuilt with Rust 1.98.1 locked/offline and
  `runtime-shaders`: 527 passed, 0 failed/ignored, including the actual Bun worker
  and nine GPUI SDK scenarios. Private fixture resources and TestHost were used.
- Parent cold-start/first-use integration rerun: 2 passed, including the opt-in
  real Bun test. Launch attempts are counted before runtime discovery/spawn.
  First-use/reuse assertions include the actual worker PID.
- Native release/signing/UI-policy subset: 42 passed, 2 explicitly skipped
  certificate/desktop checks requiring separately provided signing credentials.
- Native-host tooling boundary fixtures: 30 passed, 1 Windows-only skip.
- Host normal Cargo graph: 423 packages; no Tauri/wry/tao/WebView UI family.
  Entrypoint source inspection passed. This alone is not binary-link proof.

The all-target offline dependency gate remains unverified because
`accesskit_windows v0.34.0` is not cached. No dependency download was substituted
for that failed gate. Windows compilation, installer execution, live network
installation, platform signing/notarization, installation and update acceptance
are not claimed. Native distribution currently targets macOS arm64 and Windows
x64; Intel macOS requires a native builder/runtime acceptance before admission.
Stable publication remains blocked by the existing certificate/runtime policy.
No version bump, production profile change, installation or publication occurred.

## Reproduction

```sh
pnpm typecheck
pnpm test
node --test scripts/native/dev.test.mjs scripts/native/ui-policy.test.mjs scripts/native/release.test.mjs
node scripts/native/ui-policy.mjs --check --target host
# Explicit private prepared SDK fixture, built offline from current JS sources:
node tests/fixtures/extensions/prepare.mjs
# Set TEZBAR_M09_PREPARED/TEZBAR_M09_BUN from that fixture before native SDK tests.
cargo +1.98.1 test --locked --offline -j2 -p tezbar-native --features runtime-shaders --lib -- --test-threads=1
# Existing Bun and built worker only; missing prerequisites fail:
TEZBAR_TEST_BUN=/path/to/bun TEZBAR_TEST_RESOURCES=/prepared/resources \
cargo +1.98.1 test --locked --offline -j2 -p tezbar-core startup_demand_tests -- --include-ignored --test-threads=1
pnpm build
```

`pnpm build` is a native executable build. Private preview assembly and public
release packaging have separate explicit resource/signing inputs; see
`scripts/native/README.md`. GUI checks use an isolated, marked temporary profile.
