//! Store screenshots use the same parent-owned transport and production leases.
use super::*;
use crate::surfaces::extension_runtime::assets::ExtensionAssets;
use tezbar_ui_model::extensions::images::{
    Effect as ImageEffect, ImageCache, Lease, Source, Variant,
};
#[derive(Default)]
pub(super) struct Artwork {
    transport: Option<Arc<dyn ExtensionAssets>>,
    cache: ImageCache,
    selected: Option<String>,
    leases: Vec<Lease>,
    images: BTreeMap<String, Arc<RenderImage>>,
    jobs: Vec<Task<()>>,
    epoch: u64,
}
impl Artwork {
    pub(super) fn image(&self, index: usize) -> Option<Arc<RenderImage>> {
        self.images
            .get(self.cache.peek(*self.leases.get(index)?)?)
            .cloned()
    }
}
impl StoreView {
    /// Optional shared parent asset capability; does not change StoreView::new.
    pub fn set_assets(&mut self, assets: Arc<dyn ExtensionAssets>, cx: &mut Context<Self>) {
        self.release_artwork(cx);
        self.artwork.transport = Some(assets);
        self.prepare_artwork(cx);
        cx.notify();
    }
    pub(super) fn release_artwork(&mut self, cx: &mut App) {
        self.artwork.epoch += 1;
        self.artwork.jobs.clear();
        self.artwork.selected = None;
        self.artwork.leases.clear();
        let e = self.artwork.cache.idle();
        self.release_artwork_effects(e, cx);
    }
    fn release_artwork_effects(&mut self, effects: Vec<ImageEffect>, cx: &mut App) {
        for e in effects {
            if let ImageEffect::Release(id) = e {
                if let Some(image) = self.artwork.images.remove(&id) {
                    cx.drop_image(image, None);
                }
            }
        }
    }
    pub(super) fn prepare_artwork(&mut self, cx: &mut Context<Self>) {
        if !self.visible {
            return;
        }
        let selected = self.selected().map(|e| e.id.clone());
        if self.artwork.selected == selected {
            return;
        }
        let urls = self
            .selected()
            .and_then(|e| e.metadata.get("screenshotUrls"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        self.release_artwork(cx);
        self.artwork.selected = selected;
        let Some(assets) = self.artwork.transport.clone() else {
            return;
        };
        let epoch = self.artwork.epoch;
        for url in urls.iter().take(4).filter_map(Value::as_str) {
            // Resolve under an inert root solely to reuse the strict scheme/userinfo
            // validator, then admit Remote only. No local or catalog-supplied scope.
            let Ok(Source::Remote(url)) = tezbar_ui_model::extensions::images::resolve(
                url,
                std::path::Path::new("/__catalog_no_files__"),
                &BTreeSet::new(),
            ) else {
                continue;
            };
            let source = Source::Remote(url);
            let Ok((lease, effect)) = self
                .artwork
                .cache
                .acquire(source.clone(), Variant::Screenshot)
            else {
                continue;
            };
            self.artwork.leases.push(lease);
            let Some(effect) = effect else {
                continue;
            };
            let ImageEffect::Fetch { token, .. } = &effect else {
                continue;
            };
            let token = *token;
            let assets = assets.clone();
            let job = cx.spawn(async move |this, cx| {
                let result = gpui_tokio::Tokio::spawn(cx, async move {
                    assets
                        .load_catalog_variant(source, effect, Variant::Screenshot.width())
                        .await
                })
                .await;
                let _ = this.update(cx, |v, cx| {
                    if v.artwork.epoch != epoch || !v.visible {
                        if let Ok(Ok(r)) = result {
                            cx.drop_image(r.image, None);
                        }
                        return;
                    }
                    match result {
                        Ok(Ok(r)) => {
                            let id = format!("catalog:{epoch}:{token}");
                            v.artwork.images.insert(id.clone(), r.image);
                            let e = v.artwork.cache.ready(token, id, r.decoded_bytes);
                            v.release_artwork_effects(e, cx);
                        }
                        _ => v.artwork.cache.failed(token),
                    }
                    cx.notify();
                });
            });
            self.artwork.jobs.push(job);
        }
    }
}
