use super::*;
// Matches core ai::legacy::normalize_images; preview decoding is independent of admission.
const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
const MAX_ENCODED_BYTES: usize = 4 * ((MAX_IMAGE_BYTES + 2) / 3);
const MAX_PAYLOADS: usize = 32;
const MAX_INPUT_IMAGES: usize = 4;
#[derive(Default)]
pub(super) struct ImagePool {
    pub(super) values: BTreeMap<String, Value>,
    owners: BTreeMap<String, String>,
}
impl ImagePool {
    pub(super) fn insert(&mut self, session: &str, image: Value) -> Result<Reference, String> {
        let data = image["data"].as_str().ok_or("Image has no data")?;
        let mime = image["mimeType"].as_str().ok_or("Image type missing")?;
        let encoded = base64_data(data);
        if !matches!(mime, "image/png" | "image/jpeg" | "image/webp")
            || encoded.is_empty()
            || encoded.len() > MAX_ENCODED_BYTES
        {
            return Err("Attach a PNG, JPEG or WebP image of at most 8 MiB.".into());
        }
        let existing = self
            .values
            .iter()
            .find(|(id, v)| {
                self.owners.get(*id).is_some_and(|owner| owner == session)
                    && v["mimeType"] == mime
                    && v["data"]
                        .as_str()
                        .is_some_and(|d| base64_data(d) == encoded)
            })
            .map(|(id, _)| id.clone());
        let id = if let Some(id) = existing {
            id
        } else {
            if self.values.len() >= MAX_PAYLOADS {
                return Err("Image payloads are still retained by other conversations. Remove an attachment or finish those chats.".into());
            }
            let id = identity("image");
            self.owners.insert(id.clone(), session.into());
            self.values.insert(id.clone(), image.clone());
            id
        };
        Ok(Reference::Image {
            blob_id: id,
            mime_type: mime.into(),
            name: image["name"]
                .as_str()
                .unwrap_or("Image")
                .chars()
                .take(128)
                .collect(),
            width: image["width"].as_u64().and_then(|n| n.try_into().ok()),
            height: image["height"].as_u64().and_then(|n| n.try_into().ok()),
        })
    }
    pub(super) fn for_references(&self, refs: &[Reference]) -> Result<Vec<Value>, String> {
        if refs.len() > MAX_INPUT_IMAGES {
            return Err("Agent accepts at most 4 images per prompt".into());
        }
        refs.iter()
            .filter_map(|r| {
                if let Reference::Image { blob_id, .. } = r {
                    Some(
                        self.values
                            .get(blob_id)
                            .cloned()
                            .ok_or_else(|| "Image not available".into()),
                    )
                } else {
                    None
                }
            })
            .collect()
    }
    pub(super) fn remove(&mut self, id: &str) {
        self.values.remove(id);
        self.owners.remove(id);
    }
    pub(super) fn remove_owner(&mut self, owner: &str) {
        let ids = self
            .owners
            .iter()
            .filter(|(_, s)| s.as_str() == owner)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.remove(&id);
        }
    }
    pub(super) fn prune_owners(&mut self, c: &ChatController, deleting: &BTreeSet<String>) {
        let ids = self
            .owners
            .iter()
            .filter(|(_, id)| c.conversation(id).is_none() && !deleting.contains(*id))
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.remove(&id);
        }
    }
    pub(super) fn retain_controller(&mut self, c: &ChatController) {
        let ids = self
            .owners
            .iter()
            .filter(|(id, owner)| {
                c.conversation(owner).is_some_and(|v| {
                    !v.busy()
                        && !v
                            .attachments
                            .iter()
                            .chain(v.turns.iter().flat_map(|t| t.references.iter()))
                            .any(|r| matches!(r,Reference::Image{blob_id,..}if blob_id==*id))
                })
            })
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            self.remove(&id);
        }
    }
}
impl ChatView {
    pub(super) fn attachment_ticket(&mut self) -> Result<u64, String> {
        let c = self.active().ok_or("Open a conversation")?;
        if !self.controller.visible
            || c.busy()
            || self.deleting.contains(&c.id)
            || self.clear_inflight
            || self.disposed
        {
            return Err("Wait until this conversation is ready".into());
        }
        let session = c.id.clone();
        if self.attachment_tickets.len() >= 4 {
            return Err("An image action is still finishing".into());
        }
        self.next_job += 1;
        self.attachment_tickets.insert(
            self.next_job,
            AttachmentTicket {
                session,
                revision: self.draft_revision,
                mount: self.mount,
            },
        );
        Ok(self.next_job)
    }
    fn valid_attachment(&self, t: &AttachmentTicket) -> bool {
        !self.disposed
            && !self.clear_inflight
            && self.controller.visible
            && self.controller.active.as_deref() == Some(&t.session)
            && t.mount == self.mount
            && t.revision == self.draft_revision
            && !self.deleting.contains(&t.session)
            && self
                .controller
                .conversation(&t.session)
                .is_some_and(|c| !c.busy())
    }
    // All-or-nothing staging, including rollback of payloads on descriptor admission failure.
    pub(super) fn attach_images(
        &mut self,
        session: &str,
        images: Vec<Value>,
    ) -> Result<(), String> {
        if images.len() > MAX_INPUT_IMAGES {
            return Err("Agent accepts at most 4 images per prompt".into());
        }
        let before = self.blobs.values.keys().cloned().collect::<BTreeSet<_>>();
        let result = (|| {
            let refs = images
                .into_iter()
                .map(|image| self.blobs.insert(session, image))
                .collect::<Result<Vec<_>, _>>()?;
            let existing = self
                .controller
                .conversation(session)
                .ok_or("Conversation unavailable")?
                .attachments
                .iter()
                .chain(refs.iter())
                .filter_map(|r| {
                    if let Reference::Image { blob_id, .. } = r {
                        Some(blob_id)
                    } else {
                        None
                    }
                })
                .collect::<BTreeSet<_>>();
            if existing.len() > MAX_INPUT_IMAGES {
                return Err("Agent accepts at most 4 images per prompt".into());
            }
            self.controller
                .attach_references(session, refs)
                .map(|_| ())
                .map_err(str::to_string)
        })();
        if result.is_err() {
            let added = self
                .blobs
                .values
                .keys()
                .filter(|id| !before.contains(*id))
                .cloned()
                .collect::<Vec<_>>();
            for id in added {
                self.blobs.remove(&id);
            }
        }
        result
    }
    /// Parent chooser/clipboard calls once. No real clipboard access in this adapter.
    pub fn attachments_result(
        &mut self,
        ticket: u64,
        images: Result<Vec<Value>, String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(t) = self.attachment_tickets.remove(&ticket) else {
            return;
        };
        if !self.valid_attachment(&t) {
            return;
        }
        if let Err(e) = images
            .map_err(|_| "Could not attach image. Choose a PNG, JPEG or WebP image.".to_string())
            .and_then(|images| self.attach_images(&t.session, images))
        {
            self.error = Some(e);
        }
        self.refresh_thumbnails(w, cx);
        self.input.focus_handle(cx).focus(w, cx);
        cx.notify();
    }
    pub(super) fn capture_complete(
        &mut self,
        token: u64,
        result: ChatReply,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((pending, t)) = self.pending_capture.take() else {
            return;
        };
        if token != pending {
            self.pending_capture = Some((pending, t));
            return;
        }
        self.controller.capture_failed(token);
        if !self.valid_attachment(&t) {
            return;
        }
        if result
            .and_then(|image| self.attach_images(&t.session, vec![image]))
            .is_err()
        {
            self.error =
                Some("Screen capture failed. Check Screen Recording permission and retry.".into());
        }
        self.refresh_thumbnails(w, cx);
        self.input.focus_handle(cx).focus(w, cx);
        cx.notify();
    }
}
fn base64_data(data: &str) -> &str {
    data.trim()
        .split_once(',')
        .filter(|(p, _)| p.starts_with("data:image/"))
        .map(|(_, s)| s)
        .unwrap_or(data.trim())
}
// Same STANDARD decoder and 8 MiB source budget as the canonical core input lane.
pub(super) fn image_bytes(data: &str) -> Result<Vec<u8>, String> {
    use base64::Engine as _;
    let data = base64_data(data);
    if data.is_empty() || data.len() > MAX_ENCODED_BYTES {
        return Err("Invalid image encoding or size".into());
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| "Invalid image encoding".to_string())?;
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("Image exceeds 8 MiB".into());
    }
    Ok(bytes)
}
fn decode_image(bytes: &[u8]) -> Option<image::RgbaImage> {
    crate::asset_transport::decode_chat_preview(bytes)
}
pub(super) fn thumbnail_key(session: &str, turn: &str, index: usize, r: &Reference) -> String {
    if let Reference::Image { blob_id, .. } = r {
        if !blob_id.is_empty() {
            return blob_id.clone();
        }
    }
    format!(
        "chat-image:{}:{session}:{}:{turn}:{index}",
        session.len(),
        turn.len()
    )
}
#[derive(Clone)]
pub(super) struct PendingImage {
    cache: tezbar_ui::image_cache::Ticket,
    source: String,
    mount: u64,
    waiters: Vec<(String, tezbar_ui::markdown::ImageTicket)>,
    decoding: bool,
}
impl ChatView {
    fn thumbnail_current(&self, key: &str) -> bool {
        self.active().is_some_and(|c| {
            c.attachments
                .iter()
                .enumerate()
                .any(|(i, r)| thumbnail_key(&c.id, "", i, r) == key)
                || c.turns.iter().any(|t| {
                    t.references
                        .iter()
                        .enumerate()
                        .any(|(i, r)| thumbnail_key(&c.id, &t.id, i, r) == key)
                })
        })
    }
    pub(super) fn refresh_thumbnails(&mut self, _w: &mut Window, cx: &mut Context<Self>) {
        if !self.controller.visible || self.disposed {
            return;
        }
        let Some(c) = self.active() else {
            return;
        };
        let session = c.id.clone();
        let refs = c
            .attachments
            .iter()
            .enumerate()
            .map(|(i, r)| (String::new(), i, r.clone()))
            .chain(c.turns.iter().flat_map(|t| {
                t.references
                    .iter()
                    .enumerate()
                    .map(move |(i, r)| (t.id.clone(), i, r.clone()))
            }))
            .collect::<Vec<_>>();
        for (turn, index, r) in refs {
            if !matches!(r, Reference::Image { .. }) {
                continue;
            }
            let key = thumbnail_key(&session, &turn, index, &r);
            if self.thumbnail_attempts.contains(&key) || self.jobs.len() >= 128 {
                continue;
            }
            let Some(cache) = self.images.request(&key) else {
                continue;
            };
            self.thumbnail_attempts.insert(key.clone());
            let pending = PendingImage {
                cache,
                source: key,
                mount: self.mount,
                waiters: vec![],
                decoding: false,
            };
            if let Reference::Image { blob_id, .. } = &r {
                if let Some(image) = self.blobs.values.get(blob_id).cloned() {
                    self.begin_image_decode(pending, ImageBytes::Encoded(image), cx);
                    continue;
                }
            }
            if !turn.is_empty() && index < 8 {
                self.request(
                    ChatRequest::HistoryImage {
                        session: session.clone(),
                        turn,
                        index,
                    },
                    runtime::Completion::HistoryImage { pending },
                    cx,
                );
            } else {
                self.begin_image_decode(
                    pending,
                    ImageBytes::Raw(Err("Image unavailable".into())),
                    cx,
                );
            }
        }
    }
    pub(super) fn history_image_complete(
        &mut self,
        pending: PendingImage,
        result: ChatReply,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if pending.mount != self.mount || !self.controller.visible || self.disposed {
            self.images.complete(pending.cache, None, w, cx);
            return;
        }
        self.begin_image_decode(
            pending,
            ImageBytes::Encoded(result.unwrap_or(Value::Null)),
            cx,
        );
    }
    fn current_waiter(
        &self,
        document: &str,
        ticket: &tezbar_ui::markdown::ImageTicket,
        cx: &App,
    ) -> bool {
        self.documents
            .get(document)
            .is_some_and(|d| d.view.read(cx).image_requests().contains(ticket))
    }
    pub(super) fn request_document_image(
        &mut self,
        document: &str,
        ticket: tezbar_ui::markdown::ImageTicket,
        cx: &mut Context<Self>,
    ) {
        if !self.controller.visible || self.disposed {
            return;
        }
        if let Some(image) = self.images.get(&ticket.source) {
            if let Some(doc) = self.documents.get(document) {
                doc.view.update(cx, |v, cx| {
                    v.set_image(&ticket, Some(image), cx);
                });
            }
            return;
        }
        let same = self
            .document_images
            .iter()
            .find(|(_, p)| {
                p.source == ticket.source
                    && p.mount == self.mount
                    && p.waiters.iter().any(|(d, t)| self.current_waiter(d, t, cx))
            })
            .map(|(id, _)| *id);
        if let Some(id) = same {
            let pending = self.document_images.get_mut(&id).unwrap();
            if pending.waiters.len() < 128
                && !pending
                    .waiters
                    .iter()
                    .any(|(d, t)| d == document && t == &ticket)
            {
                pending.waiters.push((document.into(), ticket));
            }
            return;
        }
        // A removed/re-added source needs a fresh cache ticket; old decode cannot populate it.
        if self.document_images.len() >= 128 {
            if let Some(doc) = self.documents.get(document) {
                doc.view.update(cx, |v, cx| {
                    v.set_image(&ticket, None, cx);
                });
            }
            return;
        }
        let Some(cache) = self.images.request(&ticket.source) else {
            return;
        };
        self.next_job += 1;
        let id = self.next_job;
        let source = ticket.source.clone();
        self.document_images.insert(
            id,
            PendingImage {
                cache,
                source: source.clone(),
                mount: self.mount,
                waiters: vec![(document.into(), ticket)],
                decoding: false,
            },
        );
        self.emit(ChatEffect::DocumentImage { ticket: id, source }, cx);
    }
    /// Parent resolves Markdown HTTPS/authorized assets; historical previews use chat:image.
    pub fn document_image_result(
        &mut self,
        ticket: u64,
        bytes: Result<Vec<u8>, String>,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(p) = self.document_images.get_mut(&ticket) else {
            return;
        };
        if p.decoding {
            return;
        }
        p.decoding = true;
        let pending = p.clone();
        if pending.mount != self.mount
            || !self.controller.visible
            || self.disposed
            || self.jobs.len() >= 128
        {
            self.document_images.remove(&ticket);
            self.images.complete(pending.cache, None, w, cx);
            return;
        }
        self.decode_image_job(ticket, pending, ImageBytes::Raw(bytes), cx);
    }
    fn begin_image_decode(
        &mut self,
        mut pending: PendingImage,
        bytes: ImageBytes,
        cx: &mut Context<Self>,
    ) {
        self.next_job += 1;
        let id = self.next_job;
        pending.decoding = true;
        self.document_images.insert(id, pending.clone());
        self.decode_image_job(id, pending, bytes, cx);
    }
    fn decode_image_job(
        &mut self,
        id: u64,
        pending: PendingImage,
        bytes: ImageBytes,
        cx: &mut Context<Self>,
    ) {
        self.next_job += 1;
        let job = self.next_job;
        let window = self.window;
        self.decode_jobs.insert(job);
        let slots = self.decode_slots.clone();
        let task = cx.spawn(async move |this, cx| {
            let decoded = gpui_tokio::Tokio::spawn(cx, async move {
                decode_blocking(slots, move || {
                    let bytes = match bytes {
                        ImageBytes::Raw(b) => b.ok(),
                        ImageBytes::Encoded(v) => {
                            v["data"].as_str().and_then(|s| image_bytes(s).ok())
                        }
                    };
                    bytes.and_then(|b| decode_image(&b))
                })
                .await
                .ok()
                .flatten()
            })
            .await
            .ok()
            .flatten();
            let _ = cx.update_window(window, |_, w, cx| {
                let _ = this.update(cx, |v, cx| {
                    v.jobs.remove(&job);
                    v.decode_jobs.remove(&job);
                    let current = v.document_images.remove(&id);
                    let valid = current.as_ref().is_some_and(|p| {
                        p.mount == v.mount
                            && v.controller.visible
                            && !v.disposed
                            && (p.waiters.is_empty() && v.thumbnail_current(&p.source)
                                || p.waiters.iter().any(|(d, t)| v.current_waiter(d, t, cx)))
                    });
                    v.images
                        .complete(pending.cache, if valid { decoded } else { None }, w, cx);
                    if let Some(p) = current {
                        let image = v.images.get(&p.source);
                        for (document, ticket) in p.waiters {
                            if let Some(doc) = v.documents.get(&document) {
                                doc.view.update(cx, |v, cx| {
                                    v.set_image(&ticket, image.clone(), cx);
                                });
                            }
                        }
                    }
                    v.refresh_thumbnails(w, cx);
                    v.pump_document_images(cx);
                    cx.notify();
                });
            });
        });
        self.jobs.insert(job, task);
    }
    pub(super) fn pump_document_images(&mut self, cx: &mut Context<Self>) {
        let waiting = self
            .documents
            .iter()
            .flat_map(|(id, d)| {
                d.view
                    .read(cx)
                    .image_requests()
                    .into_iter()
                    .map(move |t| (id.clone(), t))
            })
            .collect::<Vec<_>>();
        for (id, ticket) in waiting {
            self.request_document_image(&id, ticket, cx);
        }
    }
}
enum ImageBytes {
    Raw(Result<Vec<u8>, String>),
    Encoded(Value),
}

/// The permit belongs to actual blocking work, independently of cache epochs.
async fn decode_blocking<T: Send + 'static>(
    slots: Arc<tokio::sync::Semaphore>,
    decode: impl FnOnce() -> T + Send + 'static,
) -> Result<T, tokio::task::JoinError> {
    let permit = slots
        .acquire_owned()
        .await
        .expect("decode owner remains open");
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        decode()
    })
    .await
}

#[cfg(test)]
mod decode_repair_tests {
    use super::*;
    #[tokio::test]
    async fn actual_blocking_decodes_hold_four_slots_across_three_release_cycles() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let slots = Arc::new(tokio::sync::Semaphore::new(4));
        let count = Arc::new(AtomicUsize::new(0));
        let gate = Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
        let mut cache = tezbar_ui::image_cache::OwnershipCache::<u8>::default();
        let mut jobs = Vec::new();
        for cycle in 0..4 {
            for n in 0..4 {
                let ticket = cache.request(&format!("{cycle}-{n}")).unwrap();
                let slots = slots.clone();
                let count = count.clone();
                let gate = gate.clone();
                jobs.push((
                    ticket,
                    tokio::spawn(async move {
                        decode_blocking(slots, move || {
                            count.fetch_add(1, Ordering::SeqCst);
                            let (lock, wake) = &*gate;
                            let mut released = lock.lock().unwrap();
                            while !*released {
                                released = wake.wait(released).unwrap();
                            }
                            1
                        })
                        .await
                        .unwrap()
                    }),
                ));
            }
            if cycle == 0 {
                tokio::time::timeout(Duration::from_secs(1), async {
                    while count.load(Ordering::SeqCst) < 4 {
                        tokio::task::yield_now().await;
                    }
                })
                .await
                .unwrap();
            }
            cache.release(|_| {});
        }
        tokio::task::yield_now().await;
        assert_eq!(
            count.load(Ordering::SeqCst),
            4,
            "a fifth blocking job started before actual completion"
        );
        // Abort async awaiters for actual running work; permit must stay in closure.
        for (_, job) in &jobs[..4] {
            job.abort();
        }
        assert_eq!(slots.available_permits(), 0);
        *gate.0.lock().unwrap() = true;
        gate.1.notify_all();
        let mut uploaded = 0;
        for (ticket, job) in jobs {
            if let Ok(value) = job.await {
                if cache.finish(ticket, Some((value, 1)), |_| {}) {
                    uploaded += 1;
                }
            }
        }
        assert_eq!(uploaded, 0, "late epoch results reached texture seam");
        tokio::time::timeout(Duration::from_secs(1), async {
            while slots.available_permits() < 4 {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}
