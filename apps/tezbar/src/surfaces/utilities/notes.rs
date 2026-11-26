use super::*;
use note_model::Effect as E;
impl Utilities {
    pub(super) fn note_effect(&mut self, e: E, cx: &mut Context<Self>) {
        if let E::Create { request, .. } = &e {
            self.note_creates
                .insert(*request, (self.epoch, self.notes.selected));
        }
        let (request, durable) = match &e {
            E::List { .. } => (UtilityRequest::NotesList, false),
            E::Create { html, .. } => (UtilityRequest::NoteCreate(html.clone()), true),
            E::Save { note_id, html, .. } => (
                UtilityRequest::NoteSave {
                    id: *note_id,
                    html: html.clone(),
                },
                true,
            ),
            E::Delete { note_id, .. } => (UtilityRequest::NoteDelete(*note_id), true),
            E::Copy(text) => (UtilityRequest::WriteText(text.clone()), false),
            E::Message(message) => {
                self.message = Some(message.clone());
                return;
            }
        };
        self.request(request, Completion::Note(e), durable, cx);
    }
    pub(super) fn note_complete(&mut self, e: E, r: Reply, cx: &mut Context<Self>) {
        match e {
            E::List { request } => {
                self.notes.loaded(request, r.and_then(decode));
                if let Some(id) = self.requested_note.take() {
                    if let Err(e) = self.notes.select(id) {
                        self.error = Some(e);
                    }
                }
            }
            E::Create { request, .. } => {
                let selection = self.notes.selected;
                let current = self
                    .note_creates
                    .remove(&request)
                    .is_some_and(|(epoch, owner)| {
                        self.visible
                            && self.surface == Surface::Notes
                            && epoch == self.epoch
                            && owner == selection
                    });
                let created = r.and_then(request::note);
                if self.clipboard_notes.remove(&request) {
                    match &created {
                        Ok(_) => self.message = Some("Saved to Notes".into()),
                        Err(e) => self.error = Some(format!("Could not save to Notes: {e}")),
                    }
                }
                self.notes.created(request, created);
                if !current {
                    self.notes.selected = selection;
                }
            }
            E::Save { request, .. } => {
                self.notes.saved(request, r.and_then(success), self.now());
            }
            E::Delete { request, .. } => {
                self.notes.deleted(request, r.and_then(success));
            }
            _ => {
                if let Err(e) = r.and_then(success) {
                    self.error = Some(e)
                } else {
                    self.message = Some("Copied".into())
                }
            }
        }
        let ids = self
            .notes
            .rows
            .iter()
            .map(|n| n.created_at)
            .collect::<std::collections::BTreeSet<_>>();
        self.note_labels.retain(|id, _| ids.contains(id));
        for note in &self.notes.rows {
            if self
                .note_labels
                .get(&note.created_at)
                .is_none_or(|label| label.revision != note.updated_at)
            {
                self.note_labels.insert(
                    note.created_at,
                    NoteLabel {
                        revision: note.updated_at,
                        title: note_model::title(&note.text),
                        summary: note_model::summary(&note.text),
                        date: note_model::format_date(note.updated_at, &*self.presentation.0),
                    },
                );
            }
        }
        self.sync_note(cx);
    }
    pub(super) fn sync_note(&mut self, cx: &mut Context<Self>) {
        if let Some(doc) = self.notes.document() {
            let doc = doc.clone();
            let id = self.notes.selected;
            self.editor.update(cx, |e, cx| e.set_document(id, doc, cx));
        } else {
            self.editor.update(cx, |e, cx| {
                e.set_document(None, note_model::Document { nodes: vec![] }, cx)
            });
        }
        self.sync_note_view(cx);
    }
    /// Renders the selected note as Markdown; skipped while it is being edited.
    pub(super) fn sync_note_view(&mut self, cx: &mut Context<Self>) {
        if self.note_editing {
            return;
        }
        let source = self
            .notes
            .document()
            .map(|doc| doc.markdown())
            .unwrap_or_default();
        self.note_view_revision += 1;
        let revision = self.note_view_revision;
        self.note_view
            .update(cx, |view, cx| view.set_content(revision, &source, true, cx));
    }
    /// Switches the selected note between its rendered view and the editor.
    pub(super) fn set_note_editing(
        &mut self,
        editing: bool,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let editing = editing && self.notes.selected.is_some();
        self.note_editing = editing;
        if editing {
            self.editor.focus_handle(cx).focus(w, cx);
        } else {
            self.sync_note_view(cx);
            w.focus(&self.search.focus_handle(cx), cx);
        }
        cx.notify();
    }
    /// Clipboard History: the selected text entry becomes a new note.
    pub(super) fn save_clipboard_to_notes(&mut self, cx: &mut Context<Self>) {
        let Some(entry) = self.clipboard.selected_entry() else {
            return;
        };
        let text = match entry.text() {
            Some(text) if !entry.is_secret && !text.trim().is_empty() => text.to_string(),
            _ => {
                self.message = Some("Only text can be saved to Notes".into());
                cx.notify();
                return;
            }
        };
        let e = self.notes.create_from_text(&text);
        if let E::Create { request, .. } = &e {
            self.clipboard_notes.insert(*request);
        }
        self.note_effect(e, cx);
    }
    pub(super) fn edit_note(&mut self, e: tezbar_ui::rich_text::Edit, cx: &mut Context<Self>) {
        use tezbar_ui::rich_text::Edit;
        if e.owner() != self.notes.selected {
            return;
        }
        let now = self.now();
        let result = match e {
            Edit::Replace {
                start,
                end,
                text,
                styles,
                selection,
                ..
            } => self.notes.edit_with_selection(now, selection.before, |d| {
                tezbar_ui::rich_text::replace_visual_styled(d, start, end, &text, &styles)?;
                Ok(selection.after)
            }),
            Edit::Format {
                start,
                end,
                style,
                selection,
                ..
            } => self.notes.edit_with_selection(now, selection.before, |d| {
                d.toggle_style(start, end, style)?;
                Ok(selection.after)
            }),
            Edit::Enter {
                position,
                end,
                selection,
                ..
            } => self.notes.edit_with_selection(now, selection.before, |d| {
                tezbar_ui::rich_text::enter_visual(d, position, end)?;
                Ok(selection.after)
            }),
            Edit::List {
                ordered, selection, ..
            } => self.notes.edit_with_selection(now, selection.before, |d| {
                tezbar_ui::rich_text::format_list_visual(
                    d,
                    selection.before.start,
                    selection.before.end,
                    ordered,
                )?;
                Ok(selection.after)
            }),
            Edit::Undo { redo, .. } => {
                let result = self.notes.undo_with_selection(redo, now);
                if let Some(selection) = result.selection {
                    self.editor
                        .update(cx, |e, _| e.restore_selection(selection));
                }
                Ok(())
            }
        };
        if let Err(error) = result {
            self.error = Some(error);
            self.editor.update(cx, |e, _| e.force_refresh());
        }
        self.sync_note(cx);
        self.schedule(cx);
        cx.notify();
    }
    pub(super) fn note_action(&mut self, a: Action, w: &mut Window, cx: &mut Context<Self>) {
        match a {
            Action::New => {
                // A fresh note opens in the editor, ready to type.
                self.note_editing = true;
                let e = self.notes.create();
                self.note_effect(e, cx);
            }
            Action::EditNote(editing) => self.set_note_editing(editing, w, cx),
            Action::Select(i) => {
                if let Some(id) = self.notes.visible().get(i).map(|n| n.created_at) {
                    if self.notes.selected != Some(id) {
                        self.note_editing = false;
                    }
                    if let Err(e) = self.notes.select(id) {
                        self.error = Some(e);
                    }
                    self.sync_note(cx);
                }
            }
            Action::Delete => {
                if let Some(id) = self.notes.selected {
                    let e = self.notes.delete(id);
                    self.note_effect(e, cx);
                }
            }
            Action::Copy | Action::Activate => {
                if let Some(e) = self.notes.copy() {
                    self.note_effect(e, cx);
                }
            }
            Action::RetryNote => {
                self.notes.retry(self.now());
                self.tick(cx);
            }
            _ => {}
        }
    }
}
