use super::*;
impl SettingsContent {
    pub(super) fn contribution_effect(
        &mut self,
        e: settings::contributions::Effect,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            e.action,
            ContributionAction::OAuthStart | ContributionAction::OAuthCancel
        ) {
            let key = (
                text(&e.args[0]),
                text(e.args.get(1).unwrap_or(&Value::Null)),
            );
            self.oauth_sessions.insert(key, e.token);
        }
        self.request(
            SettingsRequest::Contribution {
                action: e.action,
                args: e.args,
            },
            Completion::Contribution(e.token, e.action),
            cx,
        )
    }
    pub(super) fn accept_contribution(
        &mut self,
        e: Result<settings::contributions::Effect, &'static str>,
        cx: &mut Context<Self>,
    ) {
        match e {
            Ok(e) => self.contribution_effect(e, cx),
            Err(e) => self.error = Some(e.into()),
        }
    }
    pub(super) fn prepare_contributions(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        self.field(
            "instructions",
            "Custom instructions",
            FieldKind::Local,
            &self.contributions.instructions.clone(),
            false,
            true,
            w,
            cx,
        );
        let display = self.contributions.display.clone();
        for e in display["extensions"].as_array().into_iter().flatten() {
            let id = text(&e["id"]);
            let dirty = self
                .fields
                .iter()
                .any(|(key, field)| key.starts_with(&format!("secret.slot.{id}.")) && field.dirty);
            if !dirty {
                self.secret_slots
                    .insert(id.clone(), self.contributions.secret_slots(&id));
            }
            if let Some(descriptors) = self.secret_slots.get(&id) {
                let labels = descriptors
                    .iter()
                    .enumerate()
                    .map(|(i, d)| (i, format!("{} · {}", d.label, d.scope)))
                    .collect::<Vec<_>>();
                for (i, label) in labels {
                    self.field(
                        &format!("secret.slot.{id}.{i}"),
                        &label,
                        FieldKind::Local,
                        "",
                        true,
                        false,
                        w,
                        cx,
                    );
                }
            }
            for collection in ["connections", "providers", "prompts", "commands", "agents"] {
                self.field(
                    &format!("manifest.{id}.{collection}"),
                    &format!("{collection} JSON"),
                    FieldKind::Local,
                    &text(&e[collection]),
                    false,
                    true,
                    w,
                    cx,
                )
            }
            for (i, c) in e["connections"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                for field in ["command", "args", "url"] {
                    let value = if field == "args" {
                        c[field]
                            .as_array()
                            .map(|a| a.iter().map(text).collect::<Vec<_>>().join("\n"))
                            .unwrap_or_default()
                    } else {
                        text(&c[field])
                    };
                    self.field(
                        &format!("connection.{id}.{i}.{field}"),
                        field,
                        FieldKind::Local,
                        &value,
                        false,
                        field == "args",
                        w,
                        cx,
                    );
                }
            }
        }
    }
    pub(super) fn contribution_action(
        &mut self,
        intent: ContributionIntent,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use ContributionIntent as I;
        let e = match intent {
            I::Preview => Some(Ok(self.contributions.import_preview(self.value("import")))),
            I::AddPreview => Some(self.contributions.add_preview()),
            I::Select(id) => {
                self.selected_extension = Some(id);
                None
            }
            I::Test(id) => Some(Ok(self.contributions.test(&id))),
            I::Enable(id, v) => Some(self.contributions.enable(&id, v)),
            I::Remove(id) => {
                self.confirm(Action::Contribution(I::Remove(id)), w, cx);
                None
            }
            I::Review(id) => {
                match self.contributions.review(&id) {
                    Ok(v) => self.review = Some(v),
                    Err(e) => self.error = Some(e.into()),
                }
                None
            }
            I::Accept(id) => {
                let reviewed = self.review.as_ref().filter(|v| v["id"] == id);
                if let Some(reviewed) = reviewed {
                    let pins = reviewed["connections"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|c| c["needsReview"] == true)
                        .filter_map(|c| {
                            c["id"].as_str().map(|id| {
                                (
                                    id.to_string(),
                                    json!({"pins":c["pendingPins"],"trustPin":c["trustPin"]}),
                                )
                            })
                        })
                        .collect::<serde_json::Map<_, _>>();
                    Some(self.contributions.accept_review(&id, Value::Object(pins)))
                } else {
                    self.error = Some("Review the changed descriptors first.".into());
                    None
                }
            }
            I::Operation(id, action, args) => Some(self.contributions.operation(&id, action, args)),
            I::SaveRow(id, collection, index, field) => {
                let raw = self.value(&format!("connection.{id}.{index}.{field}"));
                let value = if field == "args" {
                    json!(raw.split('\n').collect::<Vec<_>>())
                } else {
                    json!(raw)
                };
                Some(
                    self.contributions
                        .edit_row(&id, &collection, index, &field, value),
                )
            }
            I::SaveCollection(id, collection) => {
                let raw = self.value(&format!("manifest.{id}.{collection}"));
                match serde_json::from_str::<Value>(&raw) {
                    Ok(v) => {
                        if collection == "connections"
                            && v.as_array()
                                .is_some_and(|rows| rows.iter().any(|c| c["toolPolicy"] == "allow"))
                        {
                            let name = self.contributions.display["extensions"]
                                .as_array()
                                .into_iter()
                                .flatten()
                                .find(|e| e["id"] == id)
                                .map(|e| text(&e["name"]))
                                .unwrap_or_default();
                            if name.is_empty() || self.value("allow.confirm") != name {
                                self.error =
                                    Some("Type the extension name to allow all tools.".into());
                                return;
                            }
                        }
                        Some(self.contributions.update(&id, &collection, v))
                    }
                    Err(_) => {
                        self.error = Some("Enter valid contribution JSON.".into());
                        None
                    }
                }
            }
            I::SetTool(id, name, enabled, connection) => Some(Ok(self.contributions.set_tool(
                &id,
                &name,
                enabled,
                connection.as_deref(),
            ))),
            I::ResetSecrets => {
                self.confirm(Action::Contribution(I::ResetSecrets), w, cx);
                None
            }
            I::SaveSavedSecret(id, index) => {
                let key = format!("secret.slot.{id}.{index}");
                let value = self.value(&key);
                let slot = self
                    .secret_slots
                    .get(&id)
                    .and_then(|slots| slots.get(index))
                    .map(|d| d.slot.clone());
                match slot {
                    Some(slot) => {
                        let effect = self.contributions.set_saved_secret(&slot, value);
                        if effect.is_ok() {
                            if let Some(f) = self.fields.get_mut(&key) {
                                f.value.clear();
                                f.dirty = false;
                                f.input.update(cx, |v, cx| v.clear_silently(w, cx));
                            }
                        }
                        Some(effect)
                    }
                    None => {
                        self.error = Some("Reload secret fields before saving.".into());
                        None
                    }
                }
            }
            I::ReloadSecretFields(id) => {
                for (key, field) in &mut self.fields {
                    if key.starts_with(&format!("secret.slot.{id}.")) {
                        field.value.clear();
                        field.dirty = false;
                        field.input.update(cx, |v, cx| v.clear_silently(w, cx));
                    }
                }
                self.secret_slots.remove(&id);
                self.prepare_contributions(w, cx);
                None
            }
            I::SaveSecret(id, key, provider) => {
                let value = self.value("secret.value");
                let e = self
                    .contributions
                    .set_secret(&id, &key, value, provider.as_deref());
                if let Some(f) = self.fields.get_mut("secret.value") {
                    f.value.clear();
                    f.input.update(cx, |v, cx| v.clear_silently(w, cx));
                }
                Some(Ok(e))
            }
            I::UseAgentsMd(value) => {
                self.contributions.display["useAgentsMd"] = json!(value);
                self.contributions
                    .edit_instructions(self.value("instructions"));
                Some(Ok(self.contributions.save_instructions(value)))
            }
            I::Instructions => {
                self.contributions
                    .edit_instructions(self.value("instructions"));
                Some(Ok(self.contributions.save_instructions(
                    self.contributions.display["useAgentsMd"] == true,
                )))
            }
            I::ReviewProject(cwd) => Some(self.contributions.operation(
                &cwd,
                ContributionAction::ReviewProject,
                vec![json!(cwd)],
            )),
            I::TrustProject => {
                let state = &self.contributions.display;
                Some(self.contributions.operation(
                    "project",
                    ContributionAction::TrustProject,
                    vec![
                        state["project"]["folder"].clone(),
                        state["project"]["entryHash"].clone(),
                        state["projectReviewId"].clone(),
                    ],
                ))
            }
            I::DismissProject => Some(self.contributions.operation(
                "project",
                ContributionAction::DismissProject,
                vec![self.contributions.display["projectReviewId"].clone()],
            )),
        };
        if let Some(e) = e {
            self.accept_contribution(e, cx)
        }
    }
}
