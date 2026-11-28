# M9 F4 status icons — source-stable candidate, gates UNRUN

## Status and authority

**Implementation candidate only. Not compiled, not accepted.** Cargo/Bun gates,
including the new regressions and all nine real-SDK scenarios, are **UNRUN** for
this candidate. Parent holds the shared compiled graph while the disjoint M11
lane edits it. No baseline restoration has been performed or authorized yet.
Scoped rustfmt parsing/format checks, source inspection and whitespace checks do
not establish Rust type correctness or runtime correctness.

- Selected worktree: `/Users/almat/Desktop/code/Tezbar APP/Tezbar-native-rust-agent`.
- Branch: `feat/native-gpui-app`; base PIN:
  `6ffa4e50fc0a45927319f9d791677ed40f78a948`.
- This is a dirty mixed working tree. M11 sources belong to the parent/other lane;
  this handoff does not claim a whole-tree freeze or ownership of those changes.
- Exact owned-file hashes and the separately owned one-line `app.rs` binding are
  recorded in `source-hashes.json`. The whole `app.rs` hash is only an observation;
  its disjoint M11 hunks remain under parent ownership.
- Evidence directory:
  `/tmp/tezbar-status-icons-followup-20261107-0krdj070/`.
  Earlier `implementation-pre-freeze/`, `baseline-pin/`, and the prepared, unused
  gate recorder are preserved. They are not a validated final source manifest.

## Input finding and source dispositions

The copied report
`docs/native-m10/followup-20261107/tezbar-gpui-m9-d1-d15-opus-fec3257.md`, F4,
and that directory's README/receipt explicitly leave real declared status icons
OPEN. The duplicate gear-as-title issue was already repaired. Historical reports,
hashes, clean-PIN receipts, binaries, failed runs and prepared worker/seed inputs
are unchanged by this lane.

| Finding | Candidate disposition; verification remains UNRUN |
| --- | --- |
| F4: every declaration becomes a generic gear | Worker passes the actual serialized icon to a native-only Host capability. GPUI binds a weak existing NativeCore; registry-scoped assets use the shared transport and decoder. Gear is used only for the declared Gear identity, not as a substitution. |
| Parent correction 1: per-icon `CoreClient::new` made fresh lifecycle/admission owners | Removed from resolver and its tests. `installed_scope`, `load_asset`, and `decode_admitted` are the single shared implementation in `asset_transport.rs`. Status calls pass the existing NativeCore and no application-icon request capability. Ordinary UI application-icon effects retain their existing shared CoreClient adapter. No new runtime, AssetService, client, font database, provider or WebView. |
| Parent correction 2: title-only/refused paths blank on Windows | Windows gets a fixed 16×16 straight-RGBA circle when no icon was declared, and a distinct exclamation-in-circle when a declaration cannot be used. These use existing Circle/exclamation symbol semantics, not a pretend copy of the refused declaration. Tooltip suffixes distinguish `no icon declared` and `icon unavailable`. Good declared pixels are never replaced by a fallback. macOS keeps no fallback bitmap on these paths and no duplicate glyph title on icon-only success. |
| Parent correction 3: promised documentation absent | This handoff, `receipt.json`, and `source-hashes.json` now explicitly record the UNRUN state and limitations. |
| Async completion can outlive status ownership | Final publication rechecks session identity, scoped status identity and checked monotonic revision. Registry scope is refreshed after loading. Invalidation invalidates pending publication and coalesces a follow-up under one refresh owner/session. Disposal, null roots, replacement, disable/worker teardown cannot publish through a missing/stale owner. These are source dispositions, not executed proofs. |
| Optional menu-only update optimization | Deferred. Whole-root equality is coalesced, but changing menu data may still fetch/decode an unchanged declaration again. No new retained bitmap cache or independent cache-revocation state was introduced. |

## Semantics and bounds

- Launcher hide/navigation retains MenuBarExtra until disable/remove/app quit,
  per the explicit user policy. No foreground/background lifetime policy redesign.
- Tauri's `sdk_status_items` opt-in remains off. The portable core stays Rust 1.85.
- Canonical installed registry scope is required even for native builtin/data
  resolution. Local assets use the existing source resolver, canonical scope
  checks and descriptor-safe core reader; arbitrary SDK paths never enter the
  legacy native-V2 decoder. Relative sources are package-root relative, e.g.
  `assets/pixel.png`; canonical installed absolute/file sources retain the shared
  policy. Symlink/outside/traversal sources are refused.
- PNG/SVG/data/HTTPS use the existing core AssetService and shared app decoder,
  including current SVG preflight/transient-memory budgets. HTTPS uses existing
  credential/redirect/timeout/body-size policy, not a separate network service.
- The single core-owned four-permit semaphore bounds image work. Blocking decode
  holds the permit inside the job, including after waiter cancellation. Existing
  encoded/decoded budgets are retained; status outputs are at most 24px per side
  for decoded sources and core validates the final bitmap boundary at 128px.
- Trusted Windows fallback generation is exactly 1 KiB, procedural, synchronous,
  and has no file/network/font/image-decoder work. It is not an independent image
  service. No asset/network work is initiated by render.
- Legacy native-V2 relative-icon contract remains; its previous unbounded
  `image::open` was replaced with a bounded read and 512px/2MiB decoder limits.
- Builtin paths reuse `raycast-api/list-runtime.tsx::SymbolIcon`; declared Gear
  preserves the prior native Gear pixels. Supported identities are exactly the
  match arms in `status_icons.rs` (Checkmark, XMark, Minus, ExclamationMark,
  Globe/Network/Wifi, Download/Upload, Link, Phone/Voice, Video/Play,
  Server/HardDrive, QuestionMark/Ping, CircleFilled and listed circle aliases).
  Unknown identities are refused, not mapped to Gear. This is not a complete
  Raycast builtin catalog. Native status glyph/blob/application/fileIcon/mask
  forms and unrecognized tint syntax are explicitly unsupported. Source wrappers
  and dark-first adaptive values follow the existing legacy representation.

## Regression sources (all execution UNRUN)

- `asset_transport/status_icons_tests.rs`: real prepared Bun worker → real core →
  TestHost; decoded Checkmark vs Gear, actual local PNG/SVG/data RGBA pixels,
  canonical path/symlink/identity refusals, shared four-slot admission, registry
  revocation after waiting, pending disposal/replacement and pending invalidation.
- `extension_worker_status_icon_tests.rs`: narrow scripted Host checks for
  A→B→A late-completion refusal, null/disposed roots, honest refusal, title-only
  policy, preservation of declared pixels, and no duplicated icon-only title.
- `process_host.rs::status_port_tests`: explicit `windows=true/false` portable
  fallback policy, visible/distinct bounded pixels, SDK path non-decoding and
  legacy native-V2 bounded relative-image behavior.
- `host_adapter.rs` weak-binding/no-core-cycle test. Existing nine real-SDK
  fixtures remain registered; additive fixture overlays are written only into
  each owned temporary profile. Prepared worker/seed inputs are not modified.
- Successful real-network HTTPS pixels have **not** been exercised. HTTPS policy
  reuse is source-traced; refusal tests do not establish a live HTTPS success.

## Required gate procedure after parent approval

No Cargo/Bun process or fixture was started by this lane; there is consequently
no owned fixture shutdown claim to infer from source inspection. The existing
user preview PID 79164 and its profile were not accessed, killed or replaced.

1. Obtain an explicit whole-tree freeze and separate permission for the two-file
   baseline substitution. Hash all compiled inputs, external prepared inputs,
   branch/HEAD and implementation backups before doing anything.
2. If authorized, replace **only** `extension_worker.rs` and
   `extensions/process_host.rs` with their PIN bytes in a parent-supervised
   `try/finally` wrapper. The `finally` must restore the exact saved implementation
   bytes on success, failed gate, timeout or exception, and emit independent hash
   restoration proofs. Verify the proofs before proceeding. Do not use reset,
   checkout, stage, commit, or a second worktree. No such wrapper has run yet.
3. Run the exact new real-Bun pixel regression against that frozen baseline;
   preserve its actual failure and source hashes. **Failure-before evidence is
   not yet produced.** Restore/prove the implementation before any green gate.
4. Exchange a renewed whole-tree freeze for green gates. Use explicit
   `cargo +1.98.1 ... --locked --offline -j2`, the agreed existing target cache,
   explicit Bun/prepared paths and a west-of-UTC timezone. Run the status tests,
   all nine exact real-SDK scenarios, and shared asset/SVG regressions. Use only
   inspected narrow core test filters plus a Rust 1.85 core check; never an
   unfiltered core suite. Record complete commands, exit codes, counts and
   before/after input stability. Kill/shutdown only fixture processes actually
   owned by these gates; preserve every failure.
5. Update this explicitly provisional receipt only with actual results. No full
   migration acceptance, Windows/physical tray/manual/RAM/GPU/signing/release
   acceptance may be inferred. No dependency/lock changes, staging, commits,
   push, merge, deployment, signing or install are part of this lane.
