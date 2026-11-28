use super::*;
use model::ClipboardEffect as E;
#[derive(Default)]
pub(super) struct ConversionUi {
    pub owner: Option<String>,
    pub active: Option<tezbar_core::instant::transform::ClipboardAction>,
    pub pending: Option<u64>,
}
impl Utilities {
    pub(super) fn clipboard_effect(&mut self, e: E, cx: &mut Context<Self>) {
        let request = match &e {
            E::List { .. } => UtilityRequest::ClipboardList,
            E::Restore { id, .. } => UtilityRequest::ClipboardRestore(id.clone()),
            E::Delete { id, .. } => UtilityRequest::ClipboardDelete(id.clone()),
            E::TogglePin { id, .. } => UtilityRequest::ClipboardPin(id.clone()),
            E::LoadImage { id, .. } => UtilityRequest::ClipboardImage(id.clone()),
            E::Clear { .. } => UtilityRequest::ClipboardClear,
            E::WriteText(text) => UtilityRequest::WriteText(text.clone()),
            E::Reveal(id) => {
                self.request(
                    UtilityRequest::ClipboardReveal(id.clone()),
                    Completion::Simple,
                    false,
                    cx,
                );
                return;
            }
            E::Hide => {
                cx.emit(UtilityEffect::Hide);
                return;
            }
            E::VaultDraft {
                source_clipboard_id,
                text,
            } => {
                self.pending_vault_seed = Some((source_clipboard_id.clone(), text.clone()));
                cx.emit(UtilityEffect::Navigate(Surface::Vault, Payload::None));
                return;
            }
            E::Transform {
                request,
                action,
                text,
                ..
            } => {
                let (request, action, text) = (*request, *action, text.clone());
                self.clipboard_ui.owner = self
                    .clipboard
                    .selected_entry()
                    .map(|entry| entry.id.clone());
                self.clipboard_ui.active = Some(action);
                self.clipboard_ui.pending = Some(request);
                let epoch = self.epoch;
                let formatter = self.presentation.clone();
                if self.jobs.len() >= 16 {
                    self.clipboard_ui.pending = None;
                    self.clipboard.conversion_complete(
                        request,
                        Err("Too many pending requests. Try again.".into()),
                    );
                    return;
                }
                self.next_job += 1;
                let id = self.next_job;
                let task = cx.spawn(async move |this, cx| {
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            if matches!(
                                action,
                                tezbar_core::instant::transform::ClipboardAction::Count
                            ) {
                                Ok(tezbar_core::instant::transform::format_clipboard_count(
                                    &tezbar_core::instant::transform::clipboard_count(&text),
                                    &formatter,
                                ))
                            } else {
                                tezbar_core::instant::transform::apply_clipboard_action(
                                    action, &text,
                                )
                            }
                        })
                        .await;
                    let _ = this.update(cx, |this, cx| {
                        this.jobs.remove(&id);
                        if this.epoch == epoch {
                            let accepted = this.clipboard.conversion_complete(request, result);
                            if this.clipboard_ui.pending == Some(request) {
                                this.clipboard_ui.pending = None;
                            }
                            if accepted
                                && matches!(
                                    action,
                                    tezbar_core::instant::transform::ClipboardAction::Qr
                                )
                            {
                                this.qr_preview(cx);
                            }
                            cx.notify();
                        }
                    });
                });
                self.jobs.insert(
                    id,
                    Job {
                        _task: task,
                        durable: false,
                    },
                );
                return;
            }
            E::SaveQr { png } => {
                cx.emit(UtilityEffect::SaveQr(png.clone()));
                return;
            }
            E::WritePng(bytes) => UtilityRequest::WritePng(encode_png(bytes)),
        };
        self.request(request, Completion::Clipboard(e), false, cx);
    }
    pub(super) fn clipboard_complete(&mut self, e: E, r: Reply, cx: &mut Context<Self>) {
        match e {
            E::List { request } => {
                self.clipboard.loaded(request, r.and_then(decode));
                if let Some(e) = self.clipboard.select(self.clipboard.selected) {
                    self.clipboard_effect(e, cx);
                }
            }
            E::LoadImage { request, id } => {
                let result = r.and_then(|value| {
                    if value.is_null() {
                        Ok(None)
                    } else {
                        value["dataUrl"]
                            .as_str()
                            .map(|s| Some(s.to_string()))
                            .ok_or("Clipboard image response is missing dataUrl.".into())
                    }
                });
                let url = result.as_ref().ok().cloned().flatten();
                self.clipboard.image_preview_loaded(request, result);
                if self
                    .clipboard
                    .selected_entry()
                    .is_some_and(|e| e.id == id && !e.is_secret)
                {
                    if let Some(url) = url {
                        self.decode_preview(id, url, cx);
                    }
                }
            }
            E::Restore { request, .. }
            | E::Delete { request, .. }
            | E::TogglePin { request, .. }
            | E::Clear { request } => {
                for e in self.clipboard.completed(request, r.and_then(success)) {
                    self.clipboard_effect(e, cx);
                }
            }
            _ => {
                if let Err(e) = r.and_then(success) {
                    self.error = Some(e)
                } else {
                    self.message = Some("Copied".into())
                }
            }
        }
    }
    pub(super) fn clipboard_action(&mut self, a: Action, cx: &mut Context<Self>) {
        if matches!(a, Action::Convert(_)) {
            self.qr_image = None;
        }
        if matches!(a, Action::Select(_) | Action::Filter(_)) {
            self.preview_image = None;
            self.qr_image = None;
        }
        let selection_before = self
            .clipboard
            .selected_entry()
            .map(|entry| entry.id.clone());
        let e = match a {
            Action::Select(i) => self.clipboard.select(i),
            Action::Activate => self.clipboard.restore(),
            Action::Pin => self.clipboard.pin(),
            Action::Delete => self.clipboard.delete(),
            Action::Clear => Some(self.clipboard.clear()),
            Action::Filter(f) => self.clipboard.set_filter(f),
            Action::Convert(a) => self.clipboard.convert(a),
            Action::CopyResult => self.clipboard.copy_conversion(),
            Action::VaultDraft => self.clipboard.vault_draft(),
            Action::SaveToNotes => {
                self.save_clipboard_to_notes(cx);
                None
            }
            Action::Reveal => self
                .clipboard
                .selected_entry()
                .map(|e| E::Reveal(e.id.clone())),
            Action::SaveQr => {
                self.qr_worker(false, cx);
                None
            }
            Action::CopyQr => {
                self.qr_worker(true, cx);
                None
            }
            Action::Copy => self
                .clipboard
                .selected_entry()
                .filter(|e| !e.is_secret)
                .and_then(|e| e.text())
                .map(|s| E::WriteText(s.into())),
            _ => None,
        };
        if selection_before
            != self
                .clipboard
                .selected_entry()
                .map(|entry| entry.id.clone())
        {
            self.clipboard_ui = Default::default();
        }
        if let Some(e) = e {
            self.clipboard_effect(e, cx);
        }
        if let Some(entry) = self.clipboard.selected_entry().filter(|e| !e.is_secret) {
            let id = entry.id.clone();
            if let Some(url) = self.clipboard.image(&id).map(String::from) {
                self.decode_preview(id, url, cx);
            }
        }
    }
    pub(super) fn clipboard_rows(&self) -> Vec<(String, String)> {
        self.clipboard
            .visible()
            .iter()
            .map(|e| {
                let label = if e.is_secret {
                    "Concealed item · Save to Vault to view".into()
                } else {
                    match &e.content {
                        model::ClipboardContent::Text { preview, .. } => preview.clone(),
                        model::ClipboardContent::Image { width, height, .. } => {
                            format!("Image · {width} × {height}")
                        }
                        model::ClipboardContent::File { preview, .. } => preview.clone(),
                    }
                };
                (e.id.clone(), label)
            })
            .collect()
    }
}
impl Utilities {
    fn decode_preview(&mut self, id: String, url: String, cx: &mut Context<Self>) {
        if self.jobs.len() >= 16 {
            return;
        }
        let Some(ticket) = self.images.request(&id) else {
            self.preview_image = self.images.get(&id);
            return;
        };
        self.next_job += 1;
        let job = self.next_job;
        let epoch = self.epoch;
        let window = self.window;
        let task = cx.spawn(async move |this, cx| {
            let decoded = cx
                .background_executor()
                .spawn(async move {
                    decode_url(&url)
                        .and_then(|bytes| tezbar_ui::image_cache::decode_png(&bytes).ok())
                })
                .await;
            let _ = window.update(cx, |_, w, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.jobs.remove(&job);
                    this.images.complete(ticket, decoded, w, cx);
                    if this.epoch == epoch
                        && this
                            .clipboard
                            .selected_entry()
                            .is_some_and(|e| e.id == id && !e.is_secret)
                    {
                        this.preview_image = this.images.get(&id);
                        if this.preview_image.is_none() {
                            this.error = Some(
                                "Image preview is unavailable or exceeds the 8 MiB limit.".into(),
                            );
                        }
                    }
                    cx.notify();
                });
            });
        });
        self.jobs.insert(
            job,
            Job {
                _task: task,
                durable: false,
            },
        );
    }
    fn qr_worker(&mut self, copy: bool, cx: &mut Context<Self>) {
        let Some(entry) = self
            .clipboard
            .selected_entry()
            .filter(|e| !e.is_secret)
            .cloned()
        else {
            return;
        };
        if self.jobs.len() >= 16 {
            return;
        }
        let id = entry.id.clone();
        let epoch = self.epoch;
        self.next_job += 1;
        let job = self.next_job;
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut c = model::Clipboard::default();
                    c.rows = vec![entry];
                    if copy { c.copy_qr() } else { c.qr_export() }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.jobs.remove(&job);
                if this.epoch != epoch || this.clipboard.selected_entry().is_none_or(|e| e.id != id)
                {
                    return;
                }
                match result {
                    Ok(e) => this.clipboard_effect(e, cx),
                    Err(e) => this.error = Some(e),
                }
                cx.notify();
            });
        });
        self.jobs.insert(
            job,
            Job {
                _task: task,
                durable: false,
            },
        );
    }
}
pub(super) fn encode_png(bytes: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}
pub(super) fn decode_url(url: &str) -> Option<Vec<u8>> {
    use base64::Engine;
    let text = url.strip_prefix("data:image/png;base64,")?;
    if text.len() > 8 * 1024 * 1024 {
        return None;
    }
    base64::engine::general_purpose::STANDARD.decode(text).ok()
}

impl Utilities {
    fn qr_preview(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self
            .clipboard
            .selected_entry()
            .filter(|e| !e.is_secret)
            .cloned()
        else {
            return;
        };
        let id = entry.id.clone();
        let key = format!("qr:{id}");
        if self.jobs.len() >= 16 {
            return;
        }
        let Some(ticket) = self.images.request(&key) else {
            self.qr_image = self.images.get(&key);
            return;
        };
        self.next_job += 1;
        let job = self.next_job;
        let epoch = self.epoch;
        let window = self.window;
        let task = cx.spawn(async move |this, cx| {
            let decoded = cx
                .background_executor()
                .spawn(async move {
                    let mut model = model::Clipboard::default();
                    model.rows = vec![entry];
                    match model.qr_export() {
                        Ok(E::SaveQr { png }) => tezbar_ui::image_cache::decode_png(&png).ok(),
                        _ => None,
                    }
                })
                .await;
            let _ = window.update(cx, |_, w, cx| {
                let _ =
                    this.update(cx, |this, cx| {
                        this.jobs.remove(&job);
                        this.images.complete(ticket, decoded, w, cx);
                        if this.epoch == epoch
                            && this.clipboard.conversion.as_ref().is_some_and(|r| {
                                r.as_ref().is_ok_and(|s| s.starts_with("data:image/"))
                            })
                            && this
                                .clipboard
                                .selected_entry()
                                .is_some_and(|e| e.id == id && !e.is_secret)
                        {
                            this.qr_image = this.images.get(&key);
                        }
                        cx.notify();
                    });
            });
        });
        self.jobs.insert(
            job,
            Job {
                _task: task,
                durable: false,
            },
        );
    }
}
