# Raycast Store Rust continuation

User authorization, 2026-11-20: continue implementing and shipping all Store extensions, prioritizing popular entries without Rust ports. Available models may be used and switched when credit limits require it. Build only on local machines, including Tailscale-linked hosts; never use GitHub Actions builds.

## Baseline

- Branch: `port/rust-extensions-beta9`; extensive preexisting working-tree changes preserved.
- 105 unique native catalog records; 98 Loved mappings. A mapping is not proof of upstream parity or desktop acceptance.
- 11 records initially have published artifact references for both supported targets; 94 have empty artifact references.
- First 50 popular Store entries inventoried with pinned command metadata. See `POPULAR-INVENTORY.md`, `popular-inventory.json` and `popular-mapped-command-audit.json`.
- Live Store comparison found missing commands in Google Translate, VS Code, Google Search and Lorem Ipsum. Preserve these as outstanding work.

## Work in progress

Progress after baseline:

- All 362 popular pages observed: 3,618 slots, 3,377 unique IDs, 241 duplicate slots. This pagination observation is not proof of exhaustive Store identity coverage.
- Seven already-published dual-target packages integrated after exact archive/policy verification. Current catalog has 18 pinned records, with desktop acceptance unchanged; current-production-importer gate is compiling locally.
- Lorem Ipsum updated to native development 0.1.1 with six commands, adding upstream HTML and Markdown generators. Windows and Mac unit/process suites each pass 26 tests. Windows debug package inventoried; optimized dual-platform packages being prepared. No release URL invented.
- Chrome, Linear and Brew implementations are active in independent sibling directories. Claude hit its weekly limit during Linear source acquisition; an available Codex 6.2 Sol agent continued the task.

- Google Chrome: standalone Rust implementation in sibling `Tezbar-rust-ports/google-chrome`, pinned upstream source, synthetic profile tests. Browser automation remains macOS-only as upstream declares. No release/desktop acceptance claimed.
- Existing public releases: independently verify both target archives, complete inventories, executable pins and exact current catalog policy before promoting development metadata. Base64 audit passed these static checks and Windows protocol identity; production importer verification pending.
- Build current headless production importer locally to exercise actual isolated installs.

## Verification

- `pnpm typecheck`: PASS on the baseline working tree.
- `pnpm test --maxWorkers=2`: PASS, 750 passed / 3 skipped, 80 test files passed / 1 skipped.
- These gates must be rerun after relevant integration changes.

## Constraints

OpenCode is disabled. Gemini's Antigravity connection is incompatible with the current T3 release and needs an update. Codex and Claude are callable. No model unavailable in the capability catalog is claimed used.

The repository-referenced `.agents/skills/release-guidelines/SKILL.md` is absent from this checkout and HEAD. Native build, signing and CLI documentation remains available. Any app publication must still satisfy the explicit dual-platform, pinned signing and atomic release/feed requirements in AGENTS.md and `docs/MACOS_SIGNING.md`.

Full Store conversion is an ongoing objective. Development packages, catalog integration, published artifacts and desktop acceptance are separate evidence stages.
