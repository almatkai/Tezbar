//! View-only read/session fencing surrounds the production M3 Vault mutation controller.
//! No secret-bearing type implements an unredacted Debug.
use super::*;
use tezbar_ui_model::vault::{Draft, Effect as E, Kind, Vault};
#[derive(Default)]
pub struct VaultScreen {
    pub model: Vault,
    pub(super) icon_open: bool,
    pub(super) icon_query: String,
    pub(super) icon_selected: usize,
    pub supported: Option<bool>,
    pub rows: Vec<Header>,
    pub detail: Option<Detail>,
    pub query: String,
    pub kind: Option<Kind>,
    pub selected: usize,
    pub revealed: bool,
    pub loading: bool,
    pub error: Option<String>,
    pub requested: Option<String>,
    session: u64,
    read: u64,
    pub(super) busy: bool,
}
#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub icon: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}
#[derive(serde::Deserialize)]
pub struct Detail {
    pub id: String,
    pub name: String,
    pub kind: Kind,
    pub icon: Option<String>,
    pub value: String,
    pub note: Option<String>,
}
#[derive(Clone)]
pub(super) enum Completion {
    Status(u64),
    Unlock(u64),
    List(u64, u64),
    Detail(u64, u64, String),
    Delete(u64, String),
    Effect(u64, E),
    Save(u64, u64, Option<String>),
}
impl VaultScreen {
    pub fn conceal(&mut self) {
        self.revealed = false;
        self.icon_open = false;
        self.detail = None;
        self.session += 1;
        self.busy = false;
        self.loading = false;
        self.model.lock();
    }
    pub fn visible(&self) -> Vec<&Header> {
        let q = self.query.to_lowercase();
        self.rows
            .iter()
            .filter(|r| self.kind.is_none_or(|k| r.kind == k) && r.name.to_lowercase().contains(&q))
            .collect()
    }
}
impl Utilities {
    pub(super) fn vault_refresh(&mut self, cx: &mut Context<Self>) {
        self.vault.loading = true;
        self.request(
            UtilityRequest::VaultStatus,
            super::Completion::Vault(Completion::Status(self.vault.session)),
            false,
            cx,
        );
    }
    fn vault_list(&mut self, cx: &mut Context<Self>) {
        self.vault.read += 1;
        self.request(
            UtilityRequest::VaultList,
            super::Completion::Vault(Completion::List(self.vault.session, self.vault.read)),
            false,
            cx,
        );
    }
    fn vault_detail(&mut self, id: String, cx: &mut Context<Self>) {
        self.vault.detail = None;
        self.vault.revealed = false;
        self.vault.read += 1;
        self.request(
            UtilityRequest::VaultGet(id.clone()),
            super::Completion::Vault(Completion::Detail(self.vault.session, self.vault.read, id)),
            false,
            cx,
        );
    }
    fn vault_effect(&mut self, e: E, cx: &mut Context<Self>) {
        let completion = if let E::Save { token } = &e {
            Completion::Save(
                self.vault.session,
                *token,
                self.vault
                    .model
                    .draft
                    .as_ref()
                    .and_then(|d| d.clipboard_entry_id.clone()),
            )
        } else {
            Completion::Effect(self.vault.session, e.clone())
        };
        let (request, durable) = match &e {
            E::Status { .. } => (UtilityRequest::VaultStatus, false),
            E::Unlock { .. } => (UtilityRequest::VaultUnlock, false),
            E::Copy { id, .. } => (UtilityRequest::VaultCopy(id.clone()), false),
            E::Save { token } => {
                let Some(item) = self.vault.model.prepared_save(*token) else {
                    return;
                };
                (UtilityRequest::VaultSave(item.clone()), true)
            }
            E::Lock => (UtilityRequest::VaultLock, false),
        };
        self.request(request, super::Completion::Vault(completion), durable, cx);
    }
    pub(super) fn vault_complete(&mut self, c: Completion, r: Reply, cx: &mut Context<Self>) {
        // Cleanup belongs to the durable save, even if its view was concealed/replaced.
        if let Completion::Save(session, token, source) = c {
            let ok = r.and_then(success).is_ok();
            if ok {
                if let Some(source) = source {
                    self.request(
                        UtilityRequest::ClipboardDelete(source),
                        super::Completion::Simple,
                        true,
                        cx,
                    );
                }
            }
            if session == self.vault.session {
                self.vault.busy = false;
                self.vault.model.saved(token, ok);
                if ok {
                    self.fields.clear();
                    self.field_subscriptions.clear();
                    self.vault_list(cx);
                    self.defer_vault_form(cx);
                }
            }
            return;
        }
        let session = match &c {
            Completion::Status(s)
            | Completion::Unlock(s)
            | Completion::List(s, _)
            | Completion::Detail(s, _, _)
            | Completion::Delete(s, _)
            | Completion::Effect(s, _)
            | Completion::Save(s, _, _) => *s,
        };
        if session != self.vault.session {
            return;
        }
        match c {
            Completion::Save(_, _, _) => unreachable!(),
            Completion::Status(_) => {
                self.vault.loading = false;
                match r {
                    Ok(v) => {
                        self.vault.supported = v["supported"].as_bool();
                        self.vault.model.unlocked = v["unlocked"] == true;
                        if self.vault.model.unlocked {
                            self.vault_list(cx);
                            self.defer_vault_form(cx);
                        }
                    }
                    Err(e) => self.vault.error = Some(e),
                }
            }
            Completion::Unlock(_) => {
                self.vault.busy = false;
                match r {
                    Ok(v) if v["ok"] == true => {
                        self.vault.model.unlocked = true;
                        self.vault_list(cx);
                        self.defer_vault_form(cx);
                    }
                    Ok(v) => {
                        self.vault.error = Some(
                            if v["cancelled"] == true {
                                "Authentication cancelled."
                            } else {
                                "Could not unlock Vault."
                            }
                            .into(),
                        )
                    }
                    Err(e) => self.vault.error = Some(e),
                }
            }
            Completion::List(_, g) => {
                if g == self.vault.read {
                    self.vault.loading = false;
                    match r.and_then(decode::<Vec<Header>>) {
                        Ok(rows) => {
                            self.vault.rows = rows;
                            self.vault.error = None;
                            if let Some(id) = self.vault.requested.take() {
                                self.vault_detail(id, cx);
                            }
                        }
                        Err(e) => self.vault.error = Some(e),
                    }
                }
            }
            Completion::Detail(_, g, id) => {
                if g == self.vault.read {
                    match r.and_then(decode::<Detail>) {
                        Ok(detail) if detail.id == id => self.vault.detail = Some(detail),
                        Ok(_) => {
                            self.vault.error = Some("Vault item changed while opening.".into())
                        }
                        Err(e) => self.vault.error = Some(e),
                    }
                }
            }
            Completion::Delete(_, id) => {
                self.vault.busy = false;
                match r.and_then(success) {
                    Ok(()) => {
                        self.vault.rows.retain(|r| r.id != id);
                        if self.vault.detail.as_ref().is_some_and(|d| d.id == id) {
                            self.vault.detail = None;
                        }
                        self.vault_list(cx);
                    }
                    Err(e) => self.vault.error = Some(e),
                }
            }
            Completion::Effect(_, e) => {
                let next = match e {
                    E::Status { token } => match r {
                        Ok(v) => self.vault.model.status(token, v["unlocked"] == true),
                        Err(_) => {
                            self.vault.model.failed(token);
                            None
                        }
                    },
                    E::Unlock { token } => self
                        .vault
                        .model
                        .unlock_result(token, r.is_ok_and(|v| v["ok"] == true)),
                    E::Copy { token, .. } => {
                        self.vault.model.copied(token, r.and_then(success).is_ok());
                        None
                    }
                    E::Save { .. } => unreachable!("Save uses its durable completion owner"),
                    E::Lock => None,
                };
                if let Some(e) = next {
                    self.vault_effect(e, cx);
                }
            }
        }
    }
    pub(super) fn vault_action(&mut self, a: Action, w: &mut Window, cx: &mut Context<Self>) {
        match a {
            Action::VaultDraftKind(kind) => {
                if let Some(d) = &mut self.vault.model.draft {
                    d.kind = kind;
                }
            }
            Action::ChangeIcon => self.icon_open(w, cx),
            Action::DefaultIcon => {
                if let Some(d) = &mut self.vault.model.draft {
                    d.icon = None;
                }
                self.vault.icon_open = false;
                self.icon_focus.focus(w, cx);
            }
            Action::VaultIcon(icon) => {
                self.vault.icon_open = false;
                self.icon_focus.focus(w, cx);
                if tezbar_ui_model::vault::valid_icon(&icon) {
                    if let Some(d) = &mut self.vault.model.draft {
                        d.icon = Some(icon);
                    }
                }
            }
            Action::Lock => {
                self.vault.conceal();
                self.fields.clear();
                self.field_subscriptions.clear();
                self.request(
                    UtilityRequest::VaultLock,
                    super::Completion::Simple,
                    false,
                    cx,
                );
            }
            Action::Unlock => {
                if !self.vault.busy && self.vault.supported == Some(true) {
                    self.vault.busy = true;
                    self.request(
                        UtilityRequest::VaultUnlock,
                        super::Completion::Vault(Completion::Unlock(self.vault.session)),
                        false,
                        cx,
                    );
                }
            }
            Action::Select(i) => {
                self.vault.selected = i;
                if let Some(id) = self.vault.visible().get(i).map(|r| r.id.clone()) {
                    self.vault_detail(id, cx);
                }
            }
            Action::Copy | Action::Activate => {
                if let Some(id) = self
                    .vault
                    .visible()
                    .get(self.vault.selected)
                    .map(|r| r.id.clone())
                {
                    let e = self.vault.model.copy(id);
                    self.vault_effect(e, cx);
                }
            }
            Action::New => {
                if self.vault.model.unlocked && !self.vault.busy {
                    self.vault.model.paste_draft(String::new());
                    self.vault_form(w, cx);
                }
            }
            Action::Edit => {
                if self.vault.model.unlocked && !self.vault.busy {
                    if let Some(d) = &self.vault.detail {
                        self.vault.model.draft = Some(Draft {
                            kind: d.kind,
                            name: d.name.clone(),
                            value: d.value.clone(),
                            id: Some(d.id.clone()),
                            note: d.note.clone().unwrap_or_default(),
                            name_touched: true,
                            clipboard_entry_id: None,
                            icon: d.icon.clone(),
                        });
                        self.vault_form(w, cx);
                    }
                }
            }
            Action::Save => {
                if !self.vault.busy {
                    match self.vault.model.save() {
                        Ok(e) => {
                            self.vault.busy = true;
                            self.vault_effect(e, cx);
                        }
                        Err(e) => self.vault.error = Some(e.into()),
                    }
                }
            }
            Action::Cancel => {
                self.vault.model.draft = None;
                self.fields.clear();
                self.field_subscriptions.clear();
            }
            Action::RevealSecret => {
                if self.vault.model.unlocked {
                    self.vault.revealed = !self.vault.revealed;
                }
            }
            Action::Delete => {
                if !self.vault.busy && self.vault.model.unlocked {
                    if let Some(id) = self
                        .vault
                        .visible()
                        .get(self.vault.selected)
                        .map(|r| r.id.clone())
                    {
                        self.vault.busy = true;
                        self.request(
                            UtilityRequest::VaultDelete(id.clone()),
                            super::Completion::Vault(Completion::Delete(self.vault.session, id)),
                            true,
                            cx,
                        );
                    }
                }
            }
            Action::VaultKind(kind) => {
                self.vault.kind = kind;
                self.vault.selected = 0;
                self.vault.detail = None;
                self.vault.revealed = false;
            }
            _ => {}
        }
    }
    fn defer_vault_form(&self, cx: &mut Context<Self>) {
        if self.vault.model.draft.is_some() {
            let entity = cx.entity().downgrade();
            let window = self.window;
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, w, cx| {
                    let _ = entity.update(cx, |this, cx| this.vault_form(w, cx));
                });
            });
        }
    }
    fn vault_form(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.fields.clear();
        self.field_subscriptions.clear();
        let Some(d) = &self.vault.model.draft else {
            return;
        };
        let values = [
            ("Name", d.name.clone(), false),
            ("Value", d.value.clone(), true),
            ("Note", d.note.clone(), false),
        ];
        for (i, (name, text, password)) in values.into_iter().enumerate() {
            let f = self.field(name, &text, i > 0, password, w, cx);
            self.field_subscriptions
                .push(cx.subscribe(&f, move |this, _, e: &Committed, cx| {
                    match i {
                        0 => this.vault.model.edit_name(e.0.clone()),
                        1 => this.vault.model.edit_value(e.0.clone()),
                        _ => {
                            if let Some(d) = &mut this.vault.model.draft {
                                d.note = e.0.clone();
                            }
                        }
                    }
                    cx.notify();
                }));
            self.fields.push(f);
        }
        if let Some(first) = self.fields.first() {
            first.focus_handle(cx).focus(w, cx);
        }
    }
}
#[derive(serde::Deserialize)]
pub(super) struct IconChoice {
    pub id: String,
    pub label: String,
    pub keywords: Vec<String>,
    pub svg: String,
}
pub(super) fn icon_choices() -> &'static [IconChoice] {
    static CHOICES: std::sync::OnceLock<Vec<IconChoice>> = std::sync::OnceLock::new();
    CHOICES.get_or_init(|| {
        serde_json::from_str(include_str!("vault_icons.json")).expect("source-owned icon registry")
    })
}
impl VaultScreen {
    pub(super) fn icons(&self) -> Vec<&'static IconChoice> {
        let q = self.icon_query.trim().to_lowercase();
        icon_choices()
            .iter()
            .filter(|icon| {
                icon.label.to_lowercase().contains(&q)
                    || icon
                        .keywords
                        .iter()
                        .any(|word| word.to_lowercase().contains(&q))
            })
            .collect()
    }
}
impl Utilities {
    pub(super) fn icon_open(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.vault.icon_open = !self.vault.icon_open;
        if !self.vault.icon_open {
            return;
        }
        self.vault.icon_query.clear();
        let Some(draft) = &self.vault.model.draft else {
            self.vault.icon_open = false;
            return;
        };
        let selected = tezbar_ui_model::vault::resolve_icon(draft.kind, draft.icon.as_deref());
        self.vault.icon_selected = icon_choices()
            .iter()
            .position(|i| i.id == selected)
            .unwrap_or(0);
        self.icon_subscriptions.clear();
        let field = cx.new(|cx| {
            let mut field = TextInput::new(cx);
            field.configure(FieldOptions {
                name: "Filter icons".into(),
                placeholder: "Filter icons".into(),
                ..Default::default()
            });
            field
        });
        self.icon_subscriptions
            .push(Self::clipboard_subscription(&field, cx));
        self.icon_subscriptions
            .push(cx.subscribe(&field, |this, _, e: &Committed, cx| {
                this.vault.icon_query = e.0.clone();
                this.vault.icon_selected = 0;
                cx.notify();
            }));
        field.focus_handle(cx).focus(w, cx);
        self.icon_filter = Some(field);
    }
    fn focus_icon_cell(&self, w: &mut Window, cx: &mut Context<Self>) {
        if let Some(icon) = self.vault.icons().get(self.vault.icon_selected) {
            if let Some(index) = tezbar_ui_model::vault::ICONS
                .iter()
                .position(|id| *id == icon.id)
            {
                self.icon_cell_focuses[index].focus(w, cx);
            }
        }
    }
    pub(super) fn icon_key(&mut self, e: &KeyDownEvent, w: &mut Window, cx: &mut Context<Self>) {
        if self
            .icon_filter
            .as_ref()
            .is_some_and(|i| i.read(cx).marked_range.is_some())
        {
            cx.propagate();
            return;
        }
        let key = e.keystroke.key.as_str();
        let filter_focused = self
            .icon_filter
            .as_ref()
            .is_some_and(|f| f.focus_handle(cx).is_focused(w));
        if filter_focused && key == "down" {
            self.focus_icon_cell(w, cx);
            cx.stop_propagation();
            return;
        }
        if filter_focused && !matches!(key, "escape" | "enter") {
            cx.propagate();
            return;
        }
        match key {
            "escape" => {
                self.vault.icon_open = false;
                self.icon_focus.focus(w, cx);
            }
            "enter" => {
                let id = self
                    .vault
                    .icons()
                    .get(self.vault.icon_selected)
                    .map(|i| i.id.clone());
                if let Some(id) = id {
                    self.vault_action(Action::VaultIcon(id), w, cx);
                }
            }
            "left" | "right" | "up" | "down" => {
                let delta = match key {
                    "left" => -1,
                    "right" => 1,
                    "up" => -8,
                    _ => 8,
                };
                self.vault.icon_selected = self
                    .vault
                    .icon_selected
                    .saturating_add_signed(delta)
                    .min(self.vault.icons().len().saturating_sub(1));
                self.focus_icon_cell(w, cx);
            }
            _ => {
                cx.propagate();
                return;
            }
        }
        cx.stop_propagation();
        cx.notify();
    }
}
