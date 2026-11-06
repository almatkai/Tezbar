# Shared native assets — linked M8 parent API

M8 exports `crate::asset_transport` and constructs exactly one
`NativeCore.assets: AssetService` through existing `NativeCore::configured`.
Default and explicit WorkerLaunch constructors retain their routing. M9 remains
unexported; its `extension_runtime/assets.rs` is only this shared public re-export:

```rust
pub use crate::asset_transport::{AssetFuture, AssetResult, AssetScope, ExtensionAssets};
```

The original public shape is unchanged:

```rust
AssetScope { extension_root: PathBuf, allowed_legacy: BTreeSet<PathBuf> }
AssetResult { image: Arc<RenderImage>, decoded_bytes: usize }
AssetFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send>>
```

`ExtensionAssets` preserves `scope(id)`, `load(scope, source, effect)` and
`load_catalog(source, effect)`. It adds default-compatible `load_variant` and
`load_catalog_variant`, each with `max_width: u32`. Defaults preserve existing fake
implementations. CoreClient overrides all methods and uses the same core owner.
M9 must pass acquired `Variant::width()` into the variant method: ReadFile and
DecodeData effects do not carry it. Fetch/ResolveAppPath widths must agree with
that argument. Legacy load methods use width640. Valid widths are1..=980, further
limited by decoded-byte budget; small originals are never upscaled.

## Capability and I/O policy

The public UI AssetScope is advisory. Per local read, CoreClient resolves the root
against current canonical installed registry identities and compares the exact
registry-derived legacy set. Core AssetScope has private fields, no deserializer,
and is issued only from the registry. Unknown/forged roots and uninstall are
rejected. No untrusted runtime result or JSON allowlist chooses an execution root.

Root/file are canonicalized on each read. Root replacement by a symlink fails.
Unix scoped opens traverse directory descriptors using openat/O_NOFOLLOW, so
root-ancestor, nested and final symlink swaps after admission cannot widen scope. Exact legacy
files derive only from canonical installed-manifest icon candidates; they grant
no directories or wildcard. Windows uses per-component reparse/symlink checks;
Windows race/platform acceptance remains open. AppPath validates extension/scope/
effect identity before existing finite `asset:icon`. Catalog has no file scope.
Chosen chat files originate only from a native host dialog, not extension props.

Service construction has no HTTP I/O/client. Explicit fetch lazily initializes one
reqwest client, shared by all consumers. Four nonwaiting admission slots bound
fetch/read/decode. HTTPS only, no URI credentials, no redirect downgrade, at most
four redirects, ten-second fetch/read/decode waiter deadline, status and declared
length checks, streamed encoded-byte cap. Automatic compression is disabled and
compressed HTTP is refused. Futures cancel async I/O on drop. A running blocking
read/decoder keeps its slot until completion and drops late allocations; this is
not a claim of preemptive cancellation of blocking CPU/filesystem work.

Data URIs require exact supported MIME+base64 header, strict canonical STANDARD
base64 and bounded source bytes. Chat Markdown has HTTPS/data authority only.
Malformed, oversized, removed and stale assets fail without a texture upload.

## Decode and texture ownership

Actual PNG/JPEG/WebP/GIF/TIFF (first frame) and SVG are supported. Extension asset
source AND decoded upload caps are2MiB. Raster header dimensions are at most16,384
per side and decoder allocation is bounded at128MiB before thumbnail upload.
Chat source admission is separately8MiB, four PNG/JPEG/WebP inputs; chat previews
fit1024²/4MiB. Preview failure never rejects a valid source attachment. Shared
M8 GPUI cache remains32MiB/128 entries/four pending tickets. M3 model image cache
retains its16MiB budget/leases/tokens/generations. There is no second service GPU
cache. The calling GPUI owner executes all texture Release effects via drop_image.
Markdown holds only weak handles and update-owned source tickets.

SVG uses direct resvg0.46.0/usvg0.46.0 (Apache-2.0 OR MIT), already locked at the
pinned GPUI revision. Both image href resolvers are disabled, preventing nested
file/data image reads. SVG source/node count is bounded (8192 markup openings),
and pixmap dimensions obey the variant/upload budget. GPUI's default resolver is
not used because it permits local file reads. No GPL editor/UI/theme code is copied.

Manifest changes: app image features explicitly include png/jpeg/webp/gif/tiff;
app adds exact `resvg = "=0.46.0"`. Cargo.lock adds only the app->resvg dependency
edge: no package version/checksum/download/license or toolchain drift. The first
development check materialized that edge offline; final gates are locked/offline.

Actual tests and hashes: `../native-m08/composition-verification.md`,
`composition-gates-02.json`, `composition-source-freeze-02.json`. These establish
M8/shared-asset compilation and fixtures, not M9 integration or Windows/manual/
network-server/GPU/RAM/full migration acceptance.
