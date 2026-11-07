//! Asset effects are capabilities, not authorization to access arbitrary files.
//! Parent supplies ONE shared bounded decoder/transport; this module never opens a client.
use gpui::RenderImage;
use std::{collections::BTreeSet, future::Future, path::PathBuf, pin::Pin, sync::Arc};
use tezbar_ui_model::extensions::images::{Effect, Source};

pub type AssetFuture<T> = Pin<Box<dyn Future<Output = Result<T, String>> + Send>>;
#[derive(Clone)]
pub struct AssetScope {
    /// Canonical installed root, obtained from core registry (never an extension prop).
    pub extension_root: PathBuf,
    pub allowed_legacy: BTreeSet<PathBuf>,
}
pub struct AssetResult {
    pub image: Arc<RenderImage>,
    /// Decoded allocation size, not compressed HTTP/file length.
    pub decoded_bytes: usize,
}
pub trait ExtensionAssets: Send + Sync + 'static {
    fn scope(&self, extension_id: String) -> AssetFuture<AssetScope>;
    fn load_catalog(&self, _source: Source, _effect: Effect) -> AssetFuture<AssetResult> {
        Box::pin(async { Err("Catalog image transport is unavailable".into()) })
    }
    /// Process Fetch/ReadFile/DecodeData/ResolveAppPath from production ImageCache.
    /// Canonicalize ReadFile and root again at read time; deny symlink escapes.
    /// HTTPS credentials/redirect downgrade denied; 2 MiB encoded AND decoded cap,
    /// max_width from effect, finite timeout. Dropping future cancels read/decode.
    /// Release is executed on GPUI's thread by the owner, never here.
    fn load(&self, scope: AssetScope, source: Source, effect: Effect) -> AssetFuture<AssetResult>;
    fn load_variant(
        &self,
        scope: AssetScope,
        source: Source,
        effect: Effect,
        _max_width: u32,
    ) -> AssetFuture<AssetResult> {
        self.load(scope, source, effect)
    }
    fn load_catalog_variant(
        &self,
        source: Source,
        effect: Effect,
        _max_width: u32,
    ) -> AssetFuture<AssetResult> {
        self.load_catalog(source, effect)
    }
}

// Store artwork has no installed package root. The shared parent transport may
// implement this finite HTTPS-only capability using the SAME fetch/decode queue.
// Its default is explicit unavailability, never an arbitrary filesystem scope.

use crate::core_client::{CoreClient, Request, RequestAdapter};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use tezbar_core::assets::{ASSET_BYTES, CHAT_SOURCE_BYTES, DEADLINE, data_uri};

impl ExtensionAssets for CoreClient {
    fn scope(&self, id: String) -> AssetFuture<AssetScope> {
        let core = self.0.clone();
        Box::pin(async move {
            let scope = tokio::task::spawn_blocking(move || core.extensions.asset_scope(&id))
                .await
                .map_err(|_| "Installed image scope is unavailable".to_string())??;
            Ok(AssetScope {
                extension_root: scope.root().into(),
                allowed_legacy: scope.allowed_legacy().clone(),
            })
        })
    }
    fn load(&self, scope: AssetScope, source: Source, effect: Effect) -> AssetFuture<AssetResult> {
        self.load_variant(scope, source, effect, 640)
    }
    fn load_catalog(&self, source: Source, effect: Effect) -> AssetFuture<AssetResult> {
        self.load_catalog_variant(source, effect, 640)
    }
    fn load_variant(
        &self,
        scope: AssetScope,
        source: Source,
        effect: Effect,
        max_width: u32,
    ) -> AssetFuture<AssetResult> {
        self.load_asset(Some(scope), source, effect, max_width)
    }
    fn load_catalog_variant(
        &self,
        source: Source,
        effect: Effect,
        max_width: u32,
    ) -> AssetFuture<AssetResult> {
        self.load_asset(None, source, effect, max_width)
    }
}
impl CoreClient {
    fn load_asset(
        &self,
        scope: Option<AssetScope>,
        source: Source,
        effect: Effect,
        width: u32,
    ) -> AssetFuture<AssetResult> {
        let client = self.clone();
        Box::pin(async move {
            // Source/effect must agree. Effects do not confer arbitrary file authority.
            let bytes = match (source, effect) {
                (
                    Source::Remote(source),
                    Effect::Fetch {
                        url,
                        max_bytes,
                        max_width,
                        ..
                    },
                ) if source == url => {
                    if max_width != width {
                        return Err("Image variant changed".into());
                    }
                    client
                        .0
                        .assets
                        .remote(&url, max_bytes.min(ASSET_BYTES))
                        .await?
                }
                (
                    Source::Data(source),
                    Effect::DecodeData {
                        data, max_bytes, ..
                    },
                ) if source == data => data_uri(&data, max_bytes.min(ASSET_BYTES))?,
                (
                    Source::File(source),
                    Effect::ReadFile {
                        path, max_bytes, ..
                    },
                ) if source == path => {
                    let scope = scope.ok_or("Catalog images have no file capability")?;
                    let core = client.0.clone();
                    let registered = tokio::task::spawn_blocking(move || {
                        let registered = core
                            .extensions
                            .asset_scope_for_root(&scope.extension_root)?;
                        if registered.allowed_legacy() != &scope.allowed_legacy {
                            return Err("Installed image scope changed".into());
                        }
                        Ok::<_, String>(registered)
                    })
                    .await
                    .map_err(|_| "Installed image scope is unavailable".to_string())??;
                    client
                        .0
                        .assets
                        .local(&registered, &path, max_bytes.min(ASSET_BYTES))
                        .await?
                }
                (
                    Source::AppPath {
                        extension_id: source_id,
                        path: source,
                    },
                    Effect::ResolveAppPath {
                        extension_id,
                        path,
                        max_width,
                        ..
                    },
                ) if source_id == extension_id
                    && source == path
                    && max_width == width
                    && scope.is_some() =>
                {
                    let scope = scope.unwrap();
                    let core = client.0.clone();
                    let registered = tokio::task::spawn_blocking(move || {
                        core.extensions.asset_scope(&extension_id)
                    })
                    .await
                    .map_err(|_| "Installed image scope is unavailable".to_string())??;
                    if registered.root() != scope.extension_root
                        || registered.allowed_legacy() != &scope.allowed_legacy
                    {
                        return Err("Installed image scope changed".into());
                    }
                    let value = RequestAdapter::send(
                        &client,
                        Request::AssetIcon {
                            kind: "application".into(),
                            path: path.to_string_lossy().into_owned(),
                        },
                    )
                    .await?;
                    data_uri(
                        value.as_str().ok_or("Application icon is unavailable")?,
                        ASSET_BYTES,
                    )?
                }
                _ => return Err("Image request capability does not match its source".into()),
            };
            client.decode_asset(bytes, width).await
        })
    }
    async fn decode_asset(&self, bytes: Vec<u8>, width: u32) -> Result<AssetResult, String> {
        tokio::time::timeout(DEADLINE, async {
            let permit = self.0.assets.decode_permit().await?;
            tokio::task::spawn_blocking(move || {
                let _permit = permit;
                decode_asset(&bytes, width)
            })
            .await
            .map_err(|_| "Image decoding failed".to_string())?
        })
        .await
        .map_err(|_| "Image decoding timed out".to_string())?
    }
    /// Chat Markdown has remote/data authority, never an arbitrary extension/file scope.
    pub(crate) fn document_image(&self, source: String) -> AssetFuture<Vec<u8>> {
        let core = self.0.clone();
        Box::pin(async move {
            if source.starts_with("data:") {
                data_uri(&source, CHAT_SOURCE_BYTES)
            } else {
                core.assets.remote(&source, CHAT_SOURCE_BYTES).await
            }
        })
    }
    /// Paths originate only from GPUI's host dialog. Batch admission is atomic at ChatView.
    pub(crate) fn chosen_chat_images(&self, paths: Vec<PathBuf>) -> AssetFuture<Vec<Value>> {
        let core = self.0.clone();
        Box::pin(async move {
            if paths.len() > 4 {
                return Err("Choose at most four images".into());
            }
            let mut result = Vec::new();
            for path in paths {
                let bytes = core.assets.chosen_image(&path).await?;
                result.push(chat_image_payload(
                    bytes,
                    path.file_name().and_then(|n| n.to_str()).map(str::to_owned),
                )?);
            }
            Ok(result)
        })
    }
}
/// Validate header metadata without allocating a full preview. Source admission is
/// independent from the smaller thumbnail budget and accepts the canonical 8 MiB.
pub(crate) fn chat_image_payload(bytes: Vec<u8>, name: Option<String>) -> Result<Value, String> {
    if bytes.len() > CHAT_SOURCE_BYTES {
        return Err("Image exceeds 8 MiB".into());
    }
    let format = image::guess_format(&bytes).map_err(|_| "Unsupported chat image")?;
    let mime = match format {
        image::ImageFormat::Png => "image/png",
        image::ImageFormat::Jpeg => "image/jpeg",
        image::ImageFormat::WebP => "image/webp",
        _ => return Err("Choose PNG, JPEG or WebP images".into()),
    };
    let (width, height) = image::ImageReader::with_format(std::io::Cursor::new(&bytes), format)
        .into_dimensions()
        .map_err(|_| "Invalid image header")?;
    Ok(
        json!({"data": STANDARD.encode(bytes), "mimeType":mime,"name":name,"width":width,"height":height}),
    )
}
/// First frame only. These RenderImages own decoded bytes, never an independent GPU cache.
/// The calling model lease/cache performs all GPU release on GPUI's thread.
fn decode_asset(bytes: &[u8], width: u32) -> Result<AssetResult, String> {
    if bytes.len() > ASSET_BYTES || !(1..=980).contains(&width) {
        return Err("Image exceeds asset bounds".into());
    }
    let rgba = decode_preview(bytes, width, ASSET_BYTES, ASSET_BYTES)?;
    let decoded_bytes = rgba.as_raw().len();
    Ok(AssetResult {
        image: Arc::new(RenderImage::new(vec![image::Frame::new(rgba)])),
        decoded_bytes,
    })
}
/// Shared M8 preview decoder; source admission remains independently capped at 8 MiB.
/// Call only from the existing fenced off-thread decode lane, never Render.
pub(crate) fn decode_chat_preview(bytes: &[u8]) -> Option<image::RgbaImage> {
    decode_preview(bytes, 1024, CHAT_SOURCE_BYTES, 4 * 1024 * 1024).ok()
}
fn decode_preview(
    bytes: &[u8],
    width: u32,
    source_limit: usize,
    decoded_limit: usize,
) -> Result<image::RgbaImage, String> {
    if bytes.len() > source_limit {
        return Err("Image exceeds source limit".into());
    }
    let max = width.min(((decoded_limit / 4) as f64).sqrt().floor() as u32);
    let prefix = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let svg = prefix.iter().copied().find(|b| !b.is_ascii_whitespace()) == Some(b'<');
    let rgba = if svg {
        decode_svg(bytes, max)?
    } else {
        let format = image::guess_format(bytes).map_err(|_| "Unsupported image format")?;
        if !matches!(
            format,
            image::ImageFormat::Png
                | image::ImageFormat::Jpeg
                | image::ImageFormat::WebP
                | image::ImageFormat::Gif
                | image::ImageFormat::Tiff
        ) {
            return Err("Unsupported image format".into());
        }
        let mut header = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
        let mut header_limits = image::Limits::default();
        header_limits.max_image_width = Some(16384);
        header_limits.max_image_height = Some(16384);
        header_limits.max_alloc = Some(128 * 1024 * 1024);
        header.limits(header_limits);
        let (w, h) = header
            .into_dimensions()
            .map_err(|_| "Invalid image header")?;
        if w == 0
            || h == 0
            || w > 16384
            || h > 16384
            || u64::from(w) * u64::from(h) * 4 > 128 * 1024 * 1024
        {
            return Err("Image exceeds decode allocation limit".into());
        }
        let mut reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(16384);
        limits.max_image_height = Some(16384);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let decoded = reader.decode().map_err(|_| "Image could not be decoded")?;
        let mut rgba = decoded.thumbnail(w.min(max), h.min(max)).into_rgba8();
        tezbar_ui::image_cache::rgba_to_bgra(rgba.as_mut());
        rgba
    };
    if rgba.as_raw().is_empty() || rgba.as_raw().len() > decoded_limit {
        return Err("Decoded image exceeds image limit".into());
    }
    Ok(rgba)
}

fn decode_svg(bytes: &[u8], width: u32) -> Result<image::RgbaImage, String> {
    // GPUI's pinned SvgRenderer uses default usvg href resolution, which can read
    // local paths. Use the SAME locked Apache/MIT resvg with both resolvers denied.
    // No Zed editor/UI/theme code is copied. Nested images must get their own leases.
    if bytes.iter().filter(|b| **b == b'<').count() > 8192 {
        return Err("SVG exceeds node budget".into());
    }
    svg_preflight(bytes)?;
    let options = resvg::usvg::Options {
        image_href_resolver: resvg::usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_data(bytes, &options).map_err(|_| "Invalid SVG image")?;
    fn budget(
        g: &resvg::usvg::Group,
        left: &mut usize,
        layers: &mut usize,
        segments: &mut usize,
        depth: usize,
    ) -> Result<(), String> {
        if !g.filters().is_empty() {
            return Err("SVG filters are unsupported".into());
        }
        if g.should_isolate() {
            *layers = layers
                .checked_sub(1)
                .ok_or("SVG exceeds isolated layer budget")?;
        }
        if depth > 64 {
            return Err("SVG exceeds render depth".into());
        }
        for node in g.children() {
            *left = left.checked_sub(1).ok_or("SVG exceeds render budget")?;
            if let resvg::usvg::Node::Path(path) = node {
                let count = path.data().segments().take(*segments + 1).count();
                *segments = segments
                    .checked_sub(count)
                    .ok_or("SVG exceeds geometry budget")?;
            }
            if let resvg::usvg::Node::Group(g) = node {
                budget(g, left, layers, segments, depth + 1)?;
            }
            let mut result = Ok(());
            node.subroots(|g| {
                if result.is_ok() {
                    result = budget(g, left, layers, segments, depth + 1);
                }
            });
            result?;
        }
        Ok(())
    }
    budget(tree.root(), &mut 8192, &mut 64, &mut 16384, 0)?;
    let size = tree.size();
    let max = width as f32;
    let scale = (max / size.width()).min(max / size.height()).min(1.);
    let w = (size.width() * scale).ceil().max(1.) as u32;
    let h = (size.height() * scale).ceil().max(1.) as u32;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).ok_or("Invalid SVG size")?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut pixels = pixmap.take();
    // tiny-skia is premultiplied; GPUI's RenderImage contract is straight BGRA.
    for p in pixels.chunks_exact_mut(4) {
        let a = p[3] as u32;
        if a != 0 {
            for c in &mut p[..3] {
                *c = ((*c as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
        p.swap(0, 2);
    }
    let rgba = image::RgbaImage::from_raw(w, h, pixels).ok_or("Invalid SVG pixels")?;
    Ok(rgba)
}

// usvg re-exports this already-locked XML parser. Count expanded reference work
// BEFORE usvg expands <use>, then count the actual render tree before rasterization.
// Namespace prefixes/comments cannot disguise amplifiers from the XML parser.
fn svg_preflight(bytes: &[u8]) -> Result<(), String> {
    use resvg::usvg::roxmltree::{Document, Node, NodeId};
    let text = std::str::from_utf8(bytes).map_err(|_| "Invalid SVG XML")?;
    let doc = Document::parse(text).map_err(|_| "Invalid SVG XML")?;
    let mut ids = std::collections::HashMap::new();
    let mut geometry_bytes = 256 * 1024usize;
    for n in doc.descendants().filter(|n| n.is_element()) {
        // Bound path allocation before usvg constructs geometry. Element count
        // alone does not constrain a single d/points attribute with many vertices.
        for attr in n
            .attributes()
            .filter(|a| matches!(a.name(), "d" | "points"))
        {
            geometry_bytes = geometry_bytes
                .checked_sub(attr.value().len())
                .ok_or("SVG exceeds geometry source budget")?;
        }
        if let Some(id) = n.attribute("id") {
            if ids.insert(id, n).is_some() {
                return Err("Duplicate SVG id".into());
            }
        }
        // Patterns/filters recursively amplify raster work beyond node count.
        // Fail closed for this preview subset; gradients and ordinary icons work.
        if matches!(n.tag_name().name(), "filter" | "pattern" | "marker") {
            return Err("SVG paint amplifier is unsupported".into());
        }
    }
    fn cost<'a>(
        n: Node<'a, 'a>,
        ids: &std::collections::HashMap<&'a str, Node<'a, 'a>>,
        memo: &mut std::collections::HashMap<NodeId, usize>,
        path: &mut Vec<NodeId>,
    ) -> Result<usize, String> {
        if let Some(v) = memo.get(&n.id()) {
            return Ok(*v);
        }
        if path.len() >= 64 || path.contains(&n.id()) {
            return Err("SVG reference cycle/depth".into());
        }
        path.push(n.id());
        let mut total = 1usize;
        for c in n.children().filter(|n| n.is_element()) {
            total = total.saturating_add(cost(c, ids, memo, path)?);
            if total > 8192 {
                return Err("SVG exceeds expansion budget".into());
            }
        }
        if n.tag_name().name() == "use" {
            for a in n.attributes().filter(|a| a.name() == "href") {
                let id = a
                    .value()
                    .strip_prefix('#')
                    .ok_or("External SVG reference denied")?;
                let target = ids.get(id).ok_or("Missing SVG reference")?;
                total = total.saturating_add(cost(*target, ids, memo, path)?);
            }
        }
        if total > 8192 {
            return Err("SVG exceeds expansion budget".into());
        }
        path.pop();
        memo.insert(n.id(), total);
        Ok(total)
    }
    cost(
        doc.root_element(),
        &ids,
        &mut Default::default(),
        &mut Vec::new(),
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "asset_transport_tests.rs"]
mod tests;
