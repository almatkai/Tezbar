use super::*;
use model::CrudEffect as E;
impl Utilities {
    pub(super) fn snippet_effect(&mut self, e: E, cx: &mut Context<Self>) {
        let (request, durable) = match &e {
            E::List { .. } => (UtilityRequest::SnippetsList, false),
            E::Write { id, draft, .. } => (
                UtilityRequest::SnippetWrite {
                    id: id.clone(),
                    draft: draft.clone(),
                },
                true,
            ),
            E::Delete { id, .. } => (UtilityRequest::SnippetDelete(id.clone()), true),
            E::Copy { id, .. } => (UtilityRequest::SnippetCopy(id.clone()), false),
            E::ConfirmDelete(_) => {
                self.confirmation = Some((
                    Action::Delete,
                    self.rows().iter().map(|(id, _)| id.clone()).collect(),
                ));
                return;
            }
            E::Hide => {
                cx.emit(UtilityEffect::Hide);
                return;
            }
        };
        self.request(request, Completion::Snippet(e), durable, cx);
    }
    pub(super) fn snippet_complete(&mut self, e: E, r: Reply, cx: &mut Context<Self>) {
        match e {
            E::List { request, .. } => self.snippets.loaded(
                request,
                r.and_then(|v| {
                    decode::<Vec<model::Snippet>>(v)
                        .map(|v| v.into_iter().map(Into::into).collect())
                }),
            ),
            E::Write { request, id, .. } => {
                let result = r.and_then(|v| {
                    success(v.clone())?;
                    v["id"]
                        .as_str()
                        .map(String::from)
                        .or(id)
                        .ok_or("Snippet save did not return its created ID.".into())
                });
                let effects = self.snippets.write_completed(request, result);
                if self.snippets.draft.is_none() {
                    self.fields.clear();
                    self.field_subscriptions.clear();
                }
                for e in effects {
                    self.snippet_effect(e, cx);
                }
            }
            E::Delete { request, .. } | E::Copy { request, .. } => {
                for e in self.snippets.completed(request, r.and_then(success)) {
                    self.snippet_effect(e, cx);
                }
            }
            _ => {}
        }
    }
    pub(super) fn snippet_action(&mut self, a: Action, w: &mut Window, cx: &mut Context<Self>) {
        let id = self
            .snippets
            .visible()
            .get(self.snippets.selected)
            .map(|r| r.id.clone());
        match a {
            Action::New => {
                self.snippets.create();
                self.snippet_form(w, cx);
            }
            Action::Edit => {
                if id.as_ref().is_some_and(|id| self.snippets.edit(id)) {
                    self.snippet_form(w, cx);
                }
            }
            Action::Cancel => {
                self.snippets.cancel();
                self.fields.clear();
                self.field_subscriptions.clear();
            }
            Action::Save => match self.snippets.save() {
                Ok(e) => self.snippet_effect(e, cx),
                Err(e) => self.snippets.error = Some(e),
            },
            Action::Delete => {
                if let Some(id) = id {
                    if let Some(e) = self.snippets.delete(&id, true) {
                        self.snippet_effect(e, cx);
                    }
                }
            }
            Action::Copy | Action::Activate => {
                if let Some(id) = id {
                    if let Some(e) = self.snippets.copy(&id) {
                        self.snippet_effect(e, cx);
                    }
                }
            }
            Action::Select(i) => self.snippets.selected = i,
            _ => {}
        }
    }
    fn snippet_form(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.fields.clear();
        self.field_subscriptions.clear();
        let Some((_, d)) = &self.snippets.draft else {
            return;
        };
        for (i, (name, text)) in [
            ("Title", d.label.clone()),
            ("Trigger", d.trigger.clone()),
            ("Snippet", d.body.clone()),
        ]
        .into_iter()
        .enumerate()
        {
            let field = self.field(name, &text, i == 2, false, w, cx);
            self.field_subscriptions.push(cx.subscribe(
                &field,
                move |this, _, e: &Committed, cx| {
                    if let Some((_, d)) = &this.snippets.draft {
                        let mut d = d.clone();
                        match i {
                            0 => d.label = e.0.clone(),
                            1 => d.trigger = e.0.clone(),
                            _ => d.body = e.0.clone(),
                        };
                        this.snippets.change(d);
                        cx.notify();
                    }
                },
            ));
            self.fields.push(field);
        }
        if let Some(first) = self.fields.first() {
            first.focus_handle(cx).focus(w, cx);
        }
    }
    pub(super) fn field(
        &mut self,
        name: &str,
        text: &str,
        multiline: bool,
        password: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        let field = cx.new(|cx| {
            let mut f = TextInput::new(cx);
            f.configure(FieldOptions {
                name: name.to_string().into(),
                placeholder: name.to_string().into(),
                multiline,
                password,
                ..Default::default()
            });
            f.set_text(text, w, cx);
            f
        });
        self.field_subscriptions
            .push(Self::clipboard_subscription(&field, cx));
        field
    }
    pub(super) fn clipboard_subscription(
        field: &Entity<TextInput>,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe(
            field,
            |this, field, e: &tezbar_ui::text_input::ClipboardEffect, cx| match e {
                tezbar_ui::text_input::ClipboardEffect::Read => {
                    let selection = field.read(cx).selection_bytes();
                    this.request(
                        UtilityRequest::ReadText,
                        Completion::Paste(field.downgrade(), this.epoch, selection),
                        false,
                        cx,
                    );
                }
                tezbar_ui::text_input::ClipboardEffect::Write(text) => this.request(
                    UtilityRequest::WriteText(text.clone()),
                    Completion::Simple,
                    false,
                    cx,
                ),
            },
        )
    }
}
