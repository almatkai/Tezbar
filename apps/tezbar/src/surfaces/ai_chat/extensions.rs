use super::*;
impl ChatView {
    pub(super) fn slash_items(&self, cx: &App) -> Vec<usize> {
        let draft = self.input.read(cx).content.as_ref();
        if self.slash_dismissed
            || self.prompt.is_some()
            || !draft.starts_with('/')
            || !draft[1..]
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
            return vec![];
        }
        let query = draft[1..].to_lowercase();
        self.items
            .iter()
            .enumerate()
            .filter(|(_, i)| {
                i["name"]
                    .as_str()
                    .unwrap_or("")
                    .to_lowercase()
                    .starts_with(&query)
            })
            .take(64)
            .map(|(i, _)| i)
            .collect()
    }
    pub(super) fn choose_prompt(&mut self, index: usize, w: &mut Window, cx: &mut Context<Self>) {
        let Some(mut item) = self.items.get(index).cloned() else {
            return;
        };
        if self.active().is_none_or(|c| c.busy()) || self.clear_inflight || self.disposed {
            return;
        }
        if item["arguments"].as_array().is_some_and(|a| a.len() > 32) {
            self.error = Some("This prompt has too many arguments.".into());
            return;
        }
        self.prompt_generation += 1;
        if item["kind"] == "agents" {
            if self
                .active()
                .is_some_and(|c| !c.turns.is_empty() || c.busy())
            {
                self.error = Some("Choose a profile before the first turn.".into());
                return;
            }
            let mut s = self.selection();
            s.profile = item["id"]
                .as_str()
                .or_else(|| item["name"].as_str())
                .map(str::to_string);
            if let Some(id) = self.controller.active.clone() {
                let _ = self.controller.select(&id, s);
                self.controller.edit_draft(&id, String::new());
                self.sync_input(w, cx);
            }
            return;
        }
        item["cwd"] = json!(self.selection().cwd);
        self.arguments.clear();
        for arg in item["arguments"].as_array().into_iter().flatten().take(32) {
            if let Some(name) = arg["name"].as_str() {
                let input = cx.new(|cx| {
                    let mut v = TextInput::new(cx);
                    v.configure(FieldOptions {
                        name: name.to_string().into(),
                        ..Default::default()
                    });
                    v
                });
                self.arguments.insert(name.into(), input);
            }
        }
        self.slash_dismissed = true;
        self.prompt = Some(item);
        if let Some(input) = self
            .prompt
            .as_ref()
            .and_then(|p| p["arguments"].as_array())
            .and_then(|a| a.first())
            .and_then(|a| a["name"].as_str())
            .and_then(|name| self.arguments.get(name))
        {
            input.focus_handle(cx).focus(w, cx);
        } else {
            self.resolve_prompt(cx);
        }
        cx.notify();
    }
    pub(super) fn resolve_prompt(&mut self, cx: &mut Context<Self>) {
        let Some(item) = self.prompt.clone() else {
            return;
        };
        let Some(session) = self.controller.active.clone() else {
            return;
        };
        let mut args = serde_json::Map::new();
        for arg in item["arguments"].as_array().into_iter().flatten() {
            let name = arg["name"].as_str().unwrap_or("");
            let text = self
                .arguments
                .get(name)
                .map(|f| f.read(cx).content.to_string())
                .unwrap_or_default();
            if arg["required"] == true && text.trim().is_empty() {
                self.error = Some(format!("Enter {name} before inserting the prompt."));
                return;
            }
            args.insert(name.into(), json!(text));
        }
        self.prompt_generation += 1;
        let arguments = args
            .into_iter()
            .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
            .collect();
        match self.controller.resolve_prompt(
            &session,
            item["id"]
                .as_str()
                .or_else(|| item["name"].as_str())
                .unwrap_or("")
                .into(),
            arguments,
        ) {
            Ok(Effect::ResolvePrompt {
                token, arguments, ..
            }) => self.prompt_effect(token, arguments, cx),
            Ok(_) => {}
            Err(e) => self.error = Some(e.into()),
        }
    }
    pub(super) fn prompt_effect(
        &mut self,
        token: u64,
        arguments: BTreeMap<String, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self.prompt.clone() else {
            self.controller.prompt_resolved(token, Err(()));
            return;
        };
        let Some(session) = self.controller.active.clone() else {
            return;
        };
        self.request(
            ChatRequest::Resolve {
                resource: item["kind"] == "resources",
                item,
                arguments: json!(arguments),
            },
            runtime::Completion::Prompt {
                token,
                generation: self.prompt_generation,
                session,
                revision: self.draft_revision,
            },
            cx,
        );
    }

    pub(super) fn prompt_complete(
        &mut self,
        token: u64,
        generation: u64,
        session: String,
        revision: u64,
        result: ChatReply,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if generation != self.prompt_generation
            || revision != self.draft_revision
            || self.controller.active.as_deref() != Some(&session)
            || !self.controller.visible
            || self.disposed
            || self.clear_inflight
            || self.deleting.contains(&session)
            || self
                .controller
                .conversation(&session)
                .is_none_or(|c| c.busy())
        {
            return;
        }
        match result {
            Ok(v) => {
                let text = v["text"].as_str().unwrap_or("");
                if text.len() > 128 * 1024 {
                    self.controller.prompt_resolved(token, Err(()));
                    self.error = Some("Resolved prompt is too large.".into());
                    return;
                }
                let images = v["images"].as_array().cloned().unwrap_or_default();
                if let Err(e) = self.attach_images(&session, images) {
                    self.controller.prompt_resolved(token, Err(()));
                    self.error = Some(e);
                    return;
                }
                self.controller.prompt_resolved(token, Ok(text.into()));
                self.prompt = None;
                self.arguments.clear();
                self.draft_revision += 1;
                self.sync_input(w, cx);
                self.refresh_thumbnails(w, cx);
                self.input.focus_handle(cx).focus(w, cx);
            }
            Err(_) => {
                self.controller.prompt_resolved(token, Err(()));
                self.error = Some(
                    "Could not resolve this contribution. Check its connection in Settings.".into(),
                )
            }
        }
    }
}
