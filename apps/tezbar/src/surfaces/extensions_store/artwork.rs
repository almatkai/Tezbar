//! Store screenshots use the same parent-owned transport and production leases.
use super::*;
use crate::surfaces::extension_runtime::assets::{AssetScope, ExtensionAssets};
use tezbar_ui_model::extensions::images::{
    Effect as ImageEffect, ImageCache, Lease, Source, Variant,
};
#[derive(Default)]
pub(super) struct Artwork {
    transport: Option<Arc<dyn ExtensionAssets>>,
    cache: ImageCache,
    selected: Option<String>,
    leases: Vec<Lease>,
    icon: Option<Lease>,
    images: BTreeMap<String, Arc<RenderImage>>,
    jobs: Vec<Task<()>>,
    epoch: u64,
    row_cache: ImageCache,
    row_leases: BTreeMap<String, (String, Lease)>,
    row_images: BTreeMap<String, Arc<RenderImage>>,
    row_jobs: BTreeMap<u64, (String, Task<()>)>,
    row_epoch: u64,
    row_scopes: BTreeMap<String, AssetScope>,
    row_scope_jobs: BTreeMap<String, Task<()>>,
    row_range: std::ops::Range<usize>,
    row_scope_failed: BTreeSet<String>,
    row_pending: Option<std::ops::Range<usize>>,
    row_scheduled: bool,
}
impl Artwork {
    pub(super) fn row_icon(&self, id: &str) -> Option<Arc<RenderImage>> {
        self.row_images
            .get(self.row_cache.peek(self.row_leases.get(id)?.1)?)
            .cloned()
    }
    pub(super) fn icon(&self) -> Option<Arc<RenderImage>> {
        self.images.get(self.cache.peek(self.icon?)?).cloned()
    }
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
        self.release_row_icons(cx);
        self.artwork.transport = Some(assets);
        self.prepare_artwork(cx);
        cx.notify();
    }
    pub(super) fn release_row_icons(&mut self, cx: &mut App) {
        self.artwork.row_epoch += 1;
        self.artwork.row_pending = None;
        self.artwork.row_scheduled = false;
        self.artwork.row_jobs.clear();
        self.artwork.row_scope_jobs.clear();
        self.artwork.row_scopes.clear();
        self.artwork.row_scope_failed.clear();
        self.artwork.row_leases.clear();
        let effects = self.artwork.row_cache.idle();
        self.release_row_effects(effects, cx);
    }
    fn release_row_effects(&mut self, effects: Vec<ImageEffect>, cx: &mut App) {
        for effect in effects {
            if let ImageEffect::Release(id) = effect {
                if let Some(image) = self.artwork.row_images.remove(&id) {
                    cx.drop_image(image, None);
                }
            }
        }
    }
    pub(super) fn schedule_row_icons(
        &mut self,
        range: std::ops::Range<usize>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if range.len() > 1 || self.artwork.row_pending.is_none() {
            self.artwork.row_pending = Some(range);
        }
        if self.artwork.row_scheduled {
            return;
        }
        self.artwork.row_scheduled = true;
        let owner = cx.entity().downgrade();
        let epoch = self.artwork.row_epoch;
        w.on_next_frame(move |_, cx| {
            let _ = owner.update(cx, |v, cx| {
                if v.artwork.row_epoch != epoch {
                    return;
                }
                v.artwork.row_scheduled = false;
                if let Some(range) = v.artwork.row_pending.take() {
                    v.prepare_row_icons(range, cx);
                }
            });
        });
    }
    pub(super) fn prepare_row_icons(
        &mut self,
        range: std::ops::Range<usize>,
        cx: &mut Context<Self>,
    ) {
        if !self.visible {
            return;
        }
        let Some(assets) = self.artwork.transport.clone() else {
            return;
        };
        let range = range.start.min(self.rows.len())..range.end.min(self.rows.len());
        self.artwork.row_range = range.clone();
        let wanted: BTreeMap<String, String> = self.rows[range]
            .iter()
            .filter_map(|e| {
                e.metadata
                    .get("iconUrl")
                    .and_then(Value::as_str)
                    .map(|url| (e.id.clone(), url.to_owned()))
            })
            .collect();
        self.artwork
            .row_scopes
            .retain(|id, _| wanted.contains_key(id));
        self.artwork
            .row_scope_jobs
            .retain(|id, _| wanted.contains_key(id));
        self.artwork
            .row_scope_failed
            .retain(|id| wanted.contains_key(id));
        self.artwork
            .row_jobs
            .retain(|_, (url, _)| wanted.values().any(|wanted| wanted == url));
        let stale: Vec<_> = self
            .artwork
            .row_leases
            .iter()
            .filter(|(id, (url, _))| wanted.get(*id) != Some(url))
            .map(|(id, _)| id.clone())
            .collect();
        for id in stale {
            if let Some((_, lease)) = self.artwork.row_leases.remove(&id) {
                let effects = self.artwork.row_cache.release(lease);
                self.release_row_effects(effects, cx);
            }
        }
        let epoch = self.artwork.row_epoch;
        for (id, url) in wanted {
            if self.artwork.row_leases.contains_key(&id) {
                continue;
            }
            let remote = tezbar_ui_model::extensions::images::resolve(
                &url,
                std::path::Path::new("/__catalog_no_files__"),
                &BTreeSet::new(),
            );
            let scope = self.artwork.row_scopes.get(&id).cloned();
            let source = if let Ok(Source::Remote(remote)) = remote {
                Source::Remote(remote)
            } else if let Some(scope) = &scope {
                let Ok(source) = tezbar_ui_model::extensions::images::resolve(
                    &url,
                    &scope.extension_root,
                    &scope.allowed_legacy,
                ) else {
                    continue;
                };
                source
            } else if self.catalog.installed.iter().any(|entry| entry.id == id)
                && !url.contains("://")
            {
                if !self.artwork.row_scope_jobs.contains_key(&id)
                    && !self.artwork.row_scope_failed.contains(&id)
                {
                    let assets = assets.clone();
                    let extension = id.clone();
                    let completed = id.clone();
                    let job =
                        cx.spawn(async move |this, cx| {
                            let result = gpui_tokio::Tokio::spawn(cx, async move {
                                assets.scope(extension).await
                            })
                            .await;
                            let _ = this.update(cx, |v, cx| {
                                v.artwork.row_scope_jobs.remove(&completed);
                                if v.artwork.row_epoch != epoch || !v.visible {
                                    return;
                                }
                                if let Ok(Ok(scope)) = result {
                                    v.artwork.row_scopes.insert(completed, scope);
                                    let range = v.artwork.row_range.clone();
                                    v.prepare_row_icons(range, cx);
                                } else {
                                    v.artwork.row_scope_failed.insert(completed);
                                }
                            });
                        });
                    self.artwork.row_scope_jobs.insert(id.clone(), job);
                }
                continue;
            } else {
                continue;
            };
            let job_url = url.clone();
            let Ok((lease, effect)) = self
                .artwork
                .row_cache
                .acquire(source.clone(), Variant::Icon)
            else {
                continue;
            };
            self.artwork.row_leases.insert(id, (url, lease));
            let Some(effect) = effect else {
                continue;
            };
            let token = match &effect {
                ImageEffect::Fetch { token, .. }
                | ImageEffect::ReadFile { token, .. }
                | ImageEffect::DecodeData { token, .. } => *token,
                _ => continue,
            };
            let assets = assets.clone();
            let job = cx.spawn(async move |this, cx| {
                let result = gpui_tokio::Tokio::spawn(cx, async move {
                    if let Some(scope) = scope {
                        assets
                            .load_variant(scope, source, effect, Variant::Icon.width())
                            .await
                    } else {
                        assets
                            .load_catalog_variant(source, effect, Variant::Icon.width())
                            .await
                    }
                })
                .await;
                let _ = this.update(cx, |v, cx| {
                    v.artwork.row_jobs.remove(&token);
                    if v.artwork.row_epoch != epoch || !v.visible {
                        if let Ok(Ok(r)) = result {
                            cx.drop_image(r.image, None);
                        }
                        return;
                    }
                    match result {
                        Ok(Ok(r)) => {
                            let id = format!("catalog-row:{epoch}:{token}");
                            v.artwork.row_images.insert(id.clone(), r.image);
                            let effects = v.artwork.row_cache.ready(token, id, r.decoded_bytes);
                            v.release_row_effects(effects, cx);
                        }
                        _ => v.artwork.row_cache.failed(token),
                    }
                    cx.notify();
                });
            });
            self.artwork.row_jobs.insert(token, (job_url, job));
        }
    }
    pub(super) fn release_artwork(&mut self, cx: &mut App) {
        self.artwork.epoch += 1;
        self.artwork.jobs.clear();
        self.artwork.selected = None;
        self.artwork.leases.clear();
        self.artwork.icon = None;
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
        let icon = self
            .selected()
            .and_then(|e| e.metadata.get("iconUrl"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        self.release_artwork(cx);
        self.artwork.selected = selected;
        let Some(assets) = self.artwork.transport.clone() else {
            return;
        };
        let epoch = self.artwork.epoch;
        let sources = urls
            .iter()
            .take(4)
            .filter_map(Value::as_str)
            .map(|url| (url.to_owned(), Variant::Screenshot))
            .chain(icon.into_iter().map(|url| (url, Variant::Icon)));
        for (url, variant) in sources {
            // Resolve under an inert root solely to reuse the strict scheme/userinfo
            // validator, then admit Remote only. No local or catalog-supplied scope.
            let Ok(Source::Remote(url)) = tezbar_ui_model::extensions::images::resolve(
                &url,
                std::path::Path::new("/__catalog_no_files__"),
                &BTreeSet::new(),
            ) else {
                continue;
            };
            let source = Source::Remote(url);
            let Ok((lease, effect)) = self.artwork.cache.acquire(source.clone(), variant) else {
                continue;
            };
            if variant == Variant::Icon {
                self.artwork.icon = Some(lease);
            } else {
                self.artwork.leases.push(lease);
            }
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
                        .load_catalog_variant(source, effect, variant.width())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset_transport::{AssetFuture, AssetResult, AssetScope};
    use ::core::prelude::v1::test;
    use std::sync::Mutex;

    struct Transport;
    impl ExtensionTransport for Transport {
        fn send(&self, _: ExtensionRequest) -> ExtensionFuture {
            Box::pin(async { Ok(json!([])) })
        }
        fn drain(&self) -> ExtensionFuture {
            Box::pin(async { Ok(json!(true)) })
        }
    }
    #[derive(Default)]
    struct Icons(Mutex<Vec<(Source, ImageEffect, u32)>>);
    impl ExtensionAssets for Icons {
        fn scope(&self, _: String) -> AssetFuture<AssetScope> {
            panic!("Catalog icons must never request a local extension scope")
        }
        fn load(&self, _: AssetScope, _: Source, _: ImageEffect) -> AssetFuture<AssetResult> {
            panic!("Catalog icons must use the bounded catalog capability")
        }
        fn load_catalog_variant(
            &self,
            source: Source,
            effect: ImageEffect,
            width: u32,
        ) -> AssetFuture<AssetResult> {
            self.0.lock().unwrap().push((source, effect, width));
            Box::pin(async { Err("fixture has no bitmap".into()) })
        }
    }
    #[gpui::test]
    fn visible_catalog_icons_deduplicate_and_never_acquire_local_scope(cx: &mut TestAppContext) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        cx.update(|cx| gpui_tokio::init_from_handle(cx, runtime.handle().clone()));
        let assets = Arc::new(Icons::default());
        let (view, cx) = cx.add_window_view(|w, cx| StoreView::new(Arc::new(Transport), w, cx));
        view.update(cx, |v, cx| {
            v.visible = true;
            v.artwork.transport = Some(assets.clone());
            v.rows = serde_json::from_value(json!([
                {"id":"one","name":"One","iconUrl":"https://fixture.invalid/icon.png"},
                {"id":"two","name":"Two","iconUrl":"https://fixture.invalid/icon.png"},
                {"id":"file","name":"File","iconUrl":"file:///private/fixture.png"},
                {"id":"relative","name":"Relative","iconUrl":"assets/icon.png"},
                {"id":"credentials","name":"Credentials","iconUrl":"https://user:pass@fixture.invalid/icon.png"}
            ])).unwrap();
            v.prepare_row_icons(0..5, cx);
            assert_eq!(v.artwork.row_leases.len(), 2);
        });
        runtime.block_on(async {
            for _ in 0..32 {
                cx.run_until_parked();
                tokio::task::yield_now().await;
            }
        });
        let calls = assets.0.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(
            matches!(&calls[0].0, Source::Remote(url) if url == "https://fixture.invalid/icon.png")
        );
        assert!(matches!(
            calls[0].1,
            ImageEffect::Fetch {
                max_width: 128,
                max_bytes: 2_097_152,
                ..
            }
        ));
        assert_eq!(calls[0].2, 128);
        drop(calls);
        view.update(cx, |v, cx| {
            v.prepare_row_icons(3..5, cx);
            assert!(v.artwork.row_leases.is_empty());
            assert!(v.artwork.row_jobs.is_empty());
            v.release_row_icons(cx);
        });
    }
    #[derive(Default)]
    struct InstalledIcons {
        scopes: Mutex<Vec<String>>,
        loads: Mutex<Vec<(Source, u32)>>,
    }
    impl ExtensionAssets for InstalledIcons {
        fn scope(&self, id: String) -> AssetFuture<AssetScope> {
            self.scopes.lock().unwrap().push(id);
            Box::pin(async {
                Ok(AssetScope {
                    extension_root: "/fixture/installed".into(),
                    allowed_legacy: BTreeSet::new(),
                })
            })
        }
        fn load(&self, _: AssetScope, _: Source, _: ImageEffect) -> AssetFuture<AssetResult> {
            panic!("Expected scoped variant")
        }
        fn load_variant(
            &self,
            scope: AssetScope,
            source: Source,
            _: ImageEffect,
            width: u32,
        ) -> AssetFuture<AssetResult> {
            assert_eq!(
                scope.extension_root,
                std::path::Path::new("/fixture/installed")
            );
            self.loads.lock().unwrap().push((source, width));
            Box::pin(async { Err("fixture bitmap unavailable".into()) })
        }
    }
    #[gpui::test]
    fn installed_row_icons_use_registry_scope_and_catalog_paths_do_not(cx: &mut TestAppContext) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        cx.update(|cx| gpui_tokio::init_from_handle(cx, runtime.handle().clone()));
        let assets = Arc::new(InstalledIcons::default());
        let (view, cx) = cx.add_window_view(|w, cx| StoreView::new(Arc::new(Transport), w, cx));
        view.update(cx, |v, cx| {
            v.visible = true;
            v.artwork.transport = Some(assets.clone());
            v.rows = serde_json::from_value(json!([
                {"id":"installed","name":"Installed","iconUrl":"assets/icon.png"},
                {"id":"catalog","name":"Catalog","iconUrl":"assets/icon.png"}
            ]))
            .unwrap();
            v.catalog.installed = vec![v.rows[0].clone()];
            v.prepare_row_icons(0..2, cx);
        });
        runtime.block_on(async {
            for _ in 0..64 {
                cx.run_until_parked();
                tokio::task::yield_now().await;
            }
        });
        assert_eq!(*assets.scopes.lock().unwrap(), vec!["installed"]);
        let loads = assets.loads.lock().unwrap();
        assert_eq!(loads.len(), 1);
        assert!(
            matches!(&loads[0].0, Source::File(path) if path == std::path::Path::new("/fixture/installed/assets/icon.png"))
        );
        assert_eq!(loads[0].1, 128);
    }
}
