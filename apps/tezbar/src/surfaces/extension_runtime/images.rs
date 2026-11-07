//! Image allocations belong to the production lease cache. Render only peeks.
use super::*;
use tezbar_ui::markdown::ImageTicket;
use tezbar_ui_model::extensions::images::{
    self, Effect as ImageEffect, ImageCache, Lease, Source, Variant,
};
#[derive(Default)]
pub(super) struct Images {
    cache: ImageCache,
    leases: BTreeMap<String, (String, Lease)>,
    decoded: BTreeMap<String, Arc<RenderImage>>,
    jobs: BTreeMap<u64, Task<()>>,
    documents: BTreeMap<String, (String, ImageTicket)>,
    epoch: u64,
    pending: VecDeque<(u64, AssetScope, Source, ImageEffect, u32)>,
}
impl Images {
    pub(super) fn get(&self, key: &str) -> Option<Arc<RenderImage>> {
        let (_, lease) = self.leases.get(key)?;
        self.decoded.get(self.cache.peek(*lease)?).cloned()
    }
    fn releases(&mut self, effects: Vec<ImageEffect>, cx: &mut App) {
        for e in effects {
            if let ImageEffect::Release(id) = e {
                if let Some(image) = self.decoded.remove(&id) {
                    cx.drop_image(image, None);
                }
            }
        }
    }
}
impl RuntimeView {
    #[cfg(test)]
    pub(super) fn image_probe(&self) -> (usize, usize) {
        (self.image_state.jobs.len(), self.image_state.decoded.len())
    }
    #[cfg(test)]
    pub(super) fn image_dimensions(&self) -> Vec<(u32, u32)> {
        self.image_state
            .decoded
            .values()
            .map(|image| {
                let s = image.size(0);
                (s.width.0 as u32, s.height.0 as u32)
            })
            .collect()
    }

    pub(super) fn release_images(&mut self, cx: &mut App) {
        self.image_state.epoch += 1;
        self.image_state.jobs.clear();
        self.image_state.pending.clear();
        self.image_state.leases.clear();
        self.image_state.documents.clear();
        let effects = self.image_state.cache.idle();
        self.image_state.releases(effects, cx);
        for entry in self.documents.values() {
            entry.view.update(cx, |v, cx| v.reset_images(cx));
        }
    }
    pub(super) fn resolve_scope(&mut self, cx: &mut Context<Self>) {
        if self.scope.is_some() || !self.visible {
            return;
        }
        let Some(s) = &self.session else {
            return;
        };
        let key = s.key.clone();
        let epoch = self.image_state.epoch;
        let a = self.assets.clone();
        let id = key.extension_id.clone();
        // One scope lookup per activation/show. Never trust assetsPath in a worker prop.
        self.next_ticket += 1;
        let ticket = self.next_ticket;
        let job = cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, async move { a.scope(id).await }).await;
            let _ = this.update(cx, |v, cx| {
                v.auxiliary.remove(&ticket);
                if v.visible
                    && v.image_state.epoch == epoch
                    && v.session.as_ref().is_some_and(|s| s.key == key)
                {
                    if let Ok(Ok(scope)) = result {
                        v.scope = Some(scope);
                        v.prepare_images(cx);
                    }
                    cx.notify();
                }
            });
        });
        self.auxiliary.insert(ticket, job);
    }
    pub(super) fn document_image(
        &mut self,
        id: String,
        ticket: ImageTicket,
        cx: &mut Context<Self>,
    ) {
        if !self.visible
            || self.disposed
            || !self
                .documents
                .get(&id)
                .is_some_and(|d| d.view.read(cx).image_requests().contains(&ticket))
        {
            return;
        }
        let key = format!("doc:{id}:{}", ticket.source);
        self.image_state
            .documents
            .insert(key.clone(), (id, ticket.clone()));
        self.prepare_images(cx);
    }
    pub(super) fn prepare_images(&mut self, cx: &mut Context<Self>) {
        if self.scope.is_none() {
            return;
        }
        let mut wanted = BTreeMap::new();
        for row in self
            .rows
            .iter()
            .skip(self.viewport.start)
            .take(self.viewport.len())
        {
            for (key, raw, variant) in view::node_images(&row.node, &row.id, self.image_light) {
                wanted.insert(key, (raw, variant));
            }
        }
        if let Some(s) = &self.session {
            if matches!(s.root.kind, NodeKind::Detail | NodeKind::ListItemDetail) {
                for (key, raw, variant) in view::node_images(&s.root, "root", self.image_light) {
                    wanted.insert(key, (raw, variant));
                }
            }
        }
        // Keep decoded leases while the corresponding document still contains
        // the source; image_requests() intentionally omits already-ready images.
        for (key, (doc, ticket)) in &self.image_state.documents {
            if self.documents.get(doc).is_some_and(|d| {
                d.view
                    .read(cx)
                    .document
                    .blocks
                    .iter()
                    .flat_map(|b| &b.images)
                    .any(|(source, _)| source == &ticket.source)
            }) {
                wanted.insert(key.clone(), (ticket.source.clone(), Variant::Artwork));
            }
        }
        let active_documents = self
            .session
            .as_ref()
            .map(|s| {
                if s.root.kind == NodeKind::List || s.root.kind == NodeKind::Grid {
                    self.rows
                        .iter()
                        .find(|r| Some(&r.id) == s.selection.as_ref())
                        .and_then(|r| view::item_detail(&r.node))
                        .map(|n| {
                            view::documents(&n)
                                .into_keys()
                                .collect::<std::collections::BTreeSet<_>>()
                        })
                        .unwrap_or_default()
                } else {
                    view::documents(&s.root).into_keys().collect()
                }
            })
            .unwrap_or_default();
        wanted.retain(|key, _| {
            !key.starts_with("doc:")
                || self
                    .image_state
                    .documents
                    .get(key)
                    .is_some_and(|(doc, _)| active_documents.contains(doc))
        });
        let pending = self
            .documents
            .iter()
            .filter(|(id, _)| active_documents.contains(*id))
            .flat_map(|(id, e)| {
                e.view
                    .read(cx)
                    .image_requests()
                    .into_iter()
                    .map(move |t| (id.clone(), t))
            })
            .collect::<Vec<_>>();
        for (id, t) in pending {
            let key = format!("doc:{id}:{}", t.source);
            wanted.insert(key.clone(), (t.source.clone(), Variant::Artwork));
            self.image_state.documents.insert(key, (id, t));
        }
        let removed = self
            .image_state
            .leases
            .keys()
            .filter(|k| !wanted.contains_key(*k))
            .cloned()
            .collect::<Vec<_>>();
        for key in removed {
            if let Some((_, lease)) = self.image_state.leases.remove(&key) {
                let e = self.image_state.cache.release(lease);
                self.image_state.releases(e, cx);
            }
            self.image_state.documents.remove(&key);
        }
        for (key, (raw, variant)) in wanted {
            self.request_image(key, raw, variant, cx);
        }
        self.publish_images(cx);
    }
    fn request_image(
        &mut self,
        key: String,
        raw: String,
        variant: Variant,
        cx: &mut Context<Self>,
    ) {
        let Some(scope) = self.scope.clone() else {
            return;
        };
        if self
            .image_state
            .leases
            .get(&key)
            .is_some_and(|(old, _)| old == &raw)
        {
            return;
        }
        if let Some((_, lease)) = self.image_state.leases.remove(&key) {
            let e = self.image_state.cache.release(lease);
            self.image_state.releases(e, cx);
        }
        let Ok(mut source) = images::resolve(&raw, &scope.extension_root, &scope.allowed_legacy)
        else {
            return;
        };
        if !std::path::Path::new(&raw).is_absolute() && !raw.contains(':') {
            if let Source::File(path) = &source {
                if let Ok(relative) = path.strip_prefix(&scope.extension_root) {
                    if !relative.starts_with("assets") {
                        source = Source::File(scope.extension_root.join("assets").join(relative));
                    }
                }
            }
        }
        if matches!(source, Source::Blob(_)) {
            return;
        }
        let Ok((lease, effect)) = self.image_state.cache.acquire(source.clone(), variant) else {
            return;
        };
        self.image_state.leases.insert(key, (raw, lease));
        let Some(effect) = effect else {
            return;
        };
        let token = match &effect {
            ImageEffect::ResolveAppPath { token, .. }
            | ImageEffect::Fetch { token, .. }
            | ImageEffect::ReadFile { token, .. }
            | ImageEffect::DecodeData { token, .. } => *token,
            ImageEffect::Release(_) => return,
        };
        if self.image_state.pending.len() >= 128 {
            self.image_state.cache.failed(token);
            return;
        }
        self.image_state
            .pending
            .push_back((token, scope, source, effect, variant.width()));
        self.pump_images(cx);
    }
    fn pump_images(&mut self, cx: &mut Context<Self>) {
        while self.image_state.jobs.len() < 8 {
            let Some((token, scope, source, effect, max_width)) =
                self.image_state.pending.pop_front()
            else {
                break;
            };
            let assets = self.assets.clone();
            let epoch = self.image_state.epoch;
            let job = cx.spawn(async move |this, cx| {
                let result = gpui_tokio::Tokio::spawn(cx, async move {
                    assets.load_variant(scope, source, effect, max_width).await
                })
                .await;
                let _ = this.update(cx, |v, cx| {
                    v.image_state.jobs.remove(&token);
                    if epoch != v.image_state.epoch || !v.visible {
                        if let Ok(Ok(r)) = result {
                            cx.drop_image(r.image, None);
                        }
                        return;
                    }
                    match result {
                        Ok(Ok(r)) => {
                            let id = format!("{epoch}:{token}");
                            v.image_state.decoded.insert(id.clone(), r.image);
                            let e = v.image_state.cache.ready(token, id, r.decoded_bytes);
                            v.image_state.releases(e, cx);
                        }
                        _ => v.image_state.cache.failed(token),
                    }
                    v.publish_images(cx);
                    v.pump_images(cx);
                    cx.notify();
                });
            });
            self.image_state.jobs.insert(token, job);
        }
    }
    fn publish_images(&mut self, cx: &mut Context<Self>) {
        for (key, (doc, ticket)) in &self.image_state.documents {
            if let Some(entry) = self.documents.get(doc) {
                if let Some(image) = self.image_state.get(key) {
                    entry.view.update(cx, |v, cx| {
                        v.set_image(ticket, Some(image), cx);
                    });
                }
            }
        }
    }
}

impl RuntimeView {
    pub(super) fn schedule_viewport(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.viewport_pending || !self.visible {
            return;
        }
        self.viewport_pending = true;
        let owner = cx.entity().downgrade();
        w.on_next_frame(move |w, cx| {
            let _ = owner.update(cx, |v, cx| {
                v.viewport_pending = false;
                if !v.visible {
                    return;
                }
                let t = tezbar_ui::tokens::tokens(w);
                let grid = v
                    .session
                    .as_ref()
                    .is_some_and(|s| s.root.kind == NodeKind::Grid);
                let row = if grid { 166. } else { tezbar_ui::tokens::ROW } * t.zoom;
                let scroll = v.list_scroll.0.borrow();
                let top = ((-f32::from(scroll.base_handle.offset().y) / row)
                    .max(0.)
                    .floor() as usize)
                    .saturating_sub(1);
                let count = (f32::from(scroll.base_handle.bounds().size.height) / row)
                    .ceil()
                    .max(1.) as usize
                    + 3;
                drop(scroll);
                if grid {
                    let ranges = v.grid_ranges();
                    let start = ranges.get(top).map(|r| r.start).unwrap_or(v.rows.len());
                    let end = ranges
                        .get((top + count).min(ranges.len()).saturating_sub(1))
                        .map(|r| r.end)
                        .unwrap_or(start);
                    v.viewport = start..end.min(start + 108);
                } else {
                    v.viewport = top.min(v.rows.len())..(top + count.min(108)).min(v.rows.len());
                }
                v.prepare_images(cx);
                cx.notify();
            });
        });
    }
}
