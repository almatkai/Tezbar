# Concrete cutover inventory, not executed

`cutover-inventory.json` lists the actual files under renderer/Tauri/prototype and root
frontend configs, retained shared imports with source referrers, unresolved relative imports,
and candidate npm dependency changes. It is a conservative static inventory, not permission
for a deletion command. Dynamic import/embed paths and worker fixture compilation remain gates.
No deletion, dependency removal, lock regeneration or default entrypoint change was made.

Order for the parent after parity/review:

1. Relocate **before deleting src-tauri**: icons/icon.icns + icon.ico, macos-signing.json,
   embedding runtime catalog, release identity/version metadata and updater public key into
   reviewed packaging paths. Change every existing consumer in one reviewed cutover. Keep
   the current exact values, policies and hashes; no new public version chosen by this lane.
   Preserve macos-permissions.plist in the existing core path. Pin source templates/licenses.
2. Integrate the first-launch read-only import and one UI-state write owner; repair all
   failure/retry and discovered-origin cases; stage old-to-native updates in disposable
   installs on Mac and Windows. Keep old artifacts for rollback. Preserve core file formats
   and old browser stores, so downgrade continues to find its existing data.
3. Promote native build configuration with existing release guard reuse, same naming,
   resource layout/receipts, single Windows manifest, toolchains and GPUI revision. Handle
   the shaders U2 decision explicitly before production. No --app default changes before
   full parity gates; no public channel choice in this lane.
4. Validate shared worker/scripts imports against the JSON inventory. Keep src/main,
   src/raycast-api, esbuild/native modules, worker tests and scripts/typechecking. Remove a
   shared module/test only when no retained worker/script importer remains and its Rust
   replacement test mapping has passed. Do not infer that a React import in an extension
   fixture makes first-party renderer necessary: distinguish the worker compatibility shim.
5. Parent changes main.rs/App production composition and package.json scripts at cutover;
   retain exact preview guard for explicit previews and no env/IPC identity bypass. Confirm
   the ordinary installed executable links GPUI and no WebKit/WebView2Loader/Tauri/wry/tao;
   no first-party React fallback. No persistent first-party JS runtime is introduced.
6. Only then delete the reviewed concrete renderer/Tauri/prototype paths and frontend
   dependencies/configs. Re-run both platform checks, license/source audits, legacy import,
   updater/installer rehearsals and full Opus correctness/design review. Manual AX/IME/window
   matrices and matched full-feature RAM remain mandatory. No CI/Actions or real profiles.

Production push/PR/merge/tag/install/release is separately authorized later. A source cutover
is not a release. Existing release-guidelines inventory, signing and dual-platform publication
rules are unchanged and still required before any actual assembly or publication.
