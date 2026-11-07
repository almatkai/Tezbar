# M9 native extensions — pre-compile handoff

This is **unexported source work**, not acceptance or verified parity. It continues the three files left by the quota-failed M9 attempt, in `Tezbar-native-rust-agent`, branch `feat/native-gpui-app`. M7 is parent-frozen at `290e414`; M7 parent repair/source-freeze gates and M8 composition have priority. No Cargo, Bun, esbuild, application, installation, profile, credential, clipboard, provider, MCP, release, GitHub Actions, stage, commit, push, PR, branch or worktree operation was performed by this continuation. Only the two owned surface directories, extension fixtures and `docs/native-m09` were changed. No subagents were launched.

## Exact integration API

The existing constructor and navigation signatures remain:

```rust
StoreView::new(Arc<dyn ExtensionTransport>, &mut Window, &mut Context<StoreView>) -> StoreView
RuntimeView::new(Arc<dyn ExtensionTransport>, Arc<dyn ExtensionAssets>, &mut Window, &mut Context<RuntimeView>) -> RuntimeView
RuntimeView::activate(Value, u64, &mut Window, &mut Context<RuntimeView>) -> Result<(), String>
RuntimeView::navigate(&Navigation, &mut Window, &mut Context<RuntimeView>) -> Result<(), String>
RuntimeView::set_visible(bool, &mut Window, &mut Context<RuntimeView>)
RuntimeView::core_event(&CoreEvent, &mut Window, &mut Context<RuntimeView>)
RuntimeView::dispose(&mut Context<RuntimeView>)
RuntimeView::drain() -> ExtensionFuture
```

Both surfaces also expose `set_task_indicator(AnyView, &mut Context<Self>)` for the existing parent M6 shared footer indicator. The setter only retains a supplied view; it creates no task service. New optional Store image injection:

```rust
StoreView::set_assets(Arc<dyn ExtensionAssets>, &mut Context<StoreView>)
StoreView::activate(&mut Window, &mut Context<StoreView>)
StoreView::set_visible(bool, &mut Window, &mut Context<StoreView>)
StoreView::core_event(&CoreEvent, &mut Context<StoreView>)
StoreView::attach_events(&CoreEvents, &mut Context<StoreView>)
StoreView::dispose(&mut Context<StoreView>)
StoreView::drain() -> ExtensionFuture
RuntimeView::attach_events(&CoreEvents, &mut Context<RuntimeView>)
RuntimeView::files_chosen(u64, Result<Vec<String>, String>, &mut Window, &mut Context<RuntimeView>)
RuntimeView::color_chosen(u64, Result<[f64; 4], String>, &mut Window, &mut Context<RuntimeView>)
```

`StoreEffect::{Back, Notice(String), OpenRuntime(Value)}` and `ExtensionEffect::{Back, Close, Notice(String), PickFiles { ticket, multiple }, ColorWheel { ticket, coordinates }}` are GPUI events with redacted Debug. Color order is hue, saturation, value, alpha. Host results are session/generation-ticket fenced. Parent suppresses blur while its file/colour facility is open. M9 does not execute an OS helper or register a handler.

`StoreEffect::OpenRuntime` is emitted only for a successful, model-validated view response. The parent constructs/delivers one Navigation, calls `navigate`, and acknowledges only Ok; it retains both view entities. Invalid/no-view replies leave the previous screen usable. Same runtime navigation nonce is idempotent. Dispose is separate from hiding and emits one core disposal; parent retains/drains at the appropriate final leave boundary. Runtime sends no requests from `new` or `Render`.

`ExtensionTransport::{send(ExtensionRequest), drain()}` remains finite. `.wire()` lists the complete channel mapping. Parent must synchronously admit work when `send` is called and retain admitted core mutations independently of the returned future. `drain` must await those admissions and disposal, including when GPUI interest is dropped. No response error payload is displayed or logged. Runtime request tokens and the production `RuntimeSession` arbitrate stale replies, invalidations, searches, polling and selection; the adapter has no parallel session state machine. Event-driven `refreshMs:null` has no recurring wake task; legacy/interval sessions have one visible-only wake owner. Search bursts discard queued obsolete searches and invalidate model interest immediately.

## Asset API and bounds

The original `ExtensionAssets::{scope(extension_id), load(scope, source, effect)}` remains. Parent has requested a HOLD on `extension_runtime/assets.rs` while lifting its exact types/trait to exported `crate::asset_transport`; do not edit that file or parent-owned `parent-contracts.md`. Production validates scope issuance and both public path fields against its opaque canonical registry scope. Runtime clears scope on hide and resolves it again on show; it never mutates issued fields. `AssetScope` must come from the core's canonical registry, never tree `assetsPath`. `load` must canonicalize again at read time, reject symlink escapes, validate HTTPS/userinfo/redirects, apply finite timeout, enforce 2 MiB input and decoded-allocation bounds, and cancel queued/running work when the future is dropped. The production lease cache owns 128 entries / 16 MiB total and rejects stale handles. The adapter releases GPUI allocations with `drop_image`; Markdown holds Weak references and opaque generation tickets.

**One addition:** `load_catalog(Source, Effect) -> AssetFuture<AssetResult>` has a default unavailable result. Implement using the SAME parent fetch/decode queue, accepting `Source::Remote` with strict HTTPS only. A catalog screenshot has no installed root; do not manufacture filesystem authority for it. Store loads at most four screenshots. `StoreView::set_assets` opts into that capability. No HTTP client/runtime/OS integration is created in either surface.

Runtime limits local image effect concurrency to eight with a bounded pending queue, and limits collection image leases to a selection window. It parses only the selected item detail, rather than one Markdown view for every row. Long lists, grids and action panels use GPUI uniform lists. Parent must validate wheel/scroll behaviour and make visible-range-driven icon loading complete; the current image window follows selection, not arbitrary pointer scroll. Builtin/glyph fallback is native text and currently covers only a small icon subset; full builtin icon/tint parity remains open.

**Agreed variant overloads:** the parent is adding `load_variant(scope, source, effect, max_width)` with a default delegation to `load`, and `load_catalog_variant(source, effect, max_width)` defaulting to `load_catalog`. Runtime now queues and passes `Variant::width()` at dispatch; Store screenshots pass `Variant::Screenshot.width()`. Thus local/data effects keep Icon/Avatar 128, Artwork 640 and Screenshot 980 even though production ReadFile/DecodeData omit width. Existing fake implementations stay compatible. Parent CoreClient overrides the new methods; legacy load/load_catalog stay conservative at width 640. Generic non-PNG decoding remains the parent transport's responsibility.

## Parent core seams required after M8's first gate

1. **Completed by parent at `9d0c910`**: programmatic launch, default unchanged. See parent-owned `parent-contracts.md`.

```rust
WorkerLaunch::new(PathBuf /* bun */, PathBuf /* bundle_directory */) -> Result<WorkerLaunch, String>
ExtensionWorker::with_launch(CoreContext, WorkerLaunch) -> ExtensionWorker
ExtensionsCore::with_worker_launch(CoreContext, WorkerLaunch) -> ExtensionsCore
NativeCore::new_with_extension_worker(CoreContext, WorkerLaunch) -> Result<Arc<NativeCore>, String>
```

`native_harness::Fixture::isolated(prepared_json_path, bun_path, runtime_handle)` now constructs this actual core, copies the seed into its own temporary profile, uses TestHost and ISOLATED_PREVIEW startup, refuses external extension search paths, and retains the temporary profile until worker shutdown. It does not execute a fixture by constructing it. The default debug worker still selects worktree `dist-backend`; explicit launch removes that problem for these tests. Real fixtures must use the actual worker source bundled into OWN temporary output, not that shared directory and not a fake protocol script. No UI/IPC/environment arbitrary-executable option is requested.

2. A finite fake OS boundary for real-core fixture calls. `TestHost` already answers confirms, surfaces and status items. It does **not** intercept `clipboard:write`: launcher clipboard still calls arboard directly. Platform clipboard/open/reveal/selection similarly need a test-only or explicit finite capability, not a replacement worker/whole-core router. The fixture transport delegates every extension request to the actual initialized `NativeCore::request`. Parent must supply the boundary before running desktop-action cases; `desktop_services:false` alone is insufficient permission isolation.

3. SDK `closeMainWindow` currently returns immediately in the worker shim; no close host message exists. Fixture 7 intentionally asserts a host close and will fail until the parent bridges it. Do not infer a new close from old HUD diagnostic records.

4. SDK `MenuBarExtra` currently serializes nodes and callbacks but does not call host.tray.set/remove. Native V2's existing process_host does have scoped status items and must remain intact. Fixture 8 intentionally asserts status lifecycle; parent needs the SDK bridge through the existing host/status seam.

5. Worker ActionPanel/Section nodes are discarded by `walkRuntimeNodes`; section titles are absent in the wire action objects and model `Action`. The native panel cannot reconstruct those titles. Preserve group data in the existing worker/model contract before marking E-08 complete.

6. Production FormState uses `value` but does not initialize from `defaultValue`. Native controls deliberately use FormState rather than a second default/controlled-state implementation. Preserve default-value semantics in that model before fixture 4 acceptance. Worker item detail lives in `props.detail`; the adapter now projects that typed Node and its metadata/Markdown instead of assuming children.

7. Model pagination reads `pagination.hasMore`; worker also emits `__hasMore` for truncated client collections. Align the authoritative model before claiming internal long-list paging. Re-running inline PreferenceSetup currently has no original argumentValues in TreeReply/Navigation; preserve those through an agreed core/navigation seam for argument-bearing commands.

## Ownership and delivery

M7 retains all installed Extensions Settings (E-04/ST-21), preferences/hotkeys/aliases/toggles and its durable writer. M9's preferences module is **only** first-run `Tezbar.PreferenceSetup`; it calls production `Preferences::merge`, `edit` and `partition`, then existing core preferences channels and a re-run. Inline replacement preserves the delivered parent nonce instead of fabricating a new one, so it cannot swallow a later parent Navigation. Store Browse origin remains the existing parent route. M8 chat and shared Markdown/CoreClient/App/Launcher/events/windows/manifests/locks/ledger were untouched.

Fixtures and native source-case map are described in `verification.md` and `source-case-map.json`. Every E row remains explicitly unaccepted. Parent independent review, Opus 5.5 design acceptance, all post-export native/worker/full-suite gates, and manual Windows/IME/Narrator/GPU/RAM/live-install evidence remain required.
