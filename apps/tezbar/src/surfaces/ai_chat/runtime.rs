use super::*;
use tezbar_ui_model::settings::providers;
pub(super) enum Completion {
    Quiet,
    Config(u64),
    History(u64, u64),
    Conversation(u64, u64),
    Directories(u64),
    PreparationConfig {
        session: String,
        token: u64,
        user: Arc<Turn>,
        retry: bool,
        selection: Selection,
    },
    HistoryImage {
        pending: references::PendingImage,
    },
    Prepared {
        session: String,
        token: u64,
        user: Arc<Turn>,
        retry: bool,
        selection: Selection,
    },
    Start {
        run: String,
        payloads: Vec<String>,
    },
    Persist {
        session: String,
        token: u64,
    },
    Delete {
        session: String,
        token: u64,
    },
    DeleteStored(String),
    Clear(Vec<(String, u64)>),
    DeleteTurn(u64),
    Capture(u64),
    Directory {
        session: String,
        mount: u64,
    },
    Scope {
        generation: u64,
        cwd: Option<String>,
        project: bool,
    },
    Prompt {
        token: u64,
        generation: u64,
        session: String,
        revision: u64,
    },
    Meta(String),
}
pub(super) fn provider_title(config: &Value, provider: &str) -> String {
    config["customProviders"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|p| p["id"] == provider)
        .and_then(|p| p["title"].as_str().or_else(|| p["name"].as_str()))
        .unwrap_or(match provider {
            "openai" => "OpenAI",
            "deepseek" => "DeepSeek",
            "antigravity" => "Antigravity",
            "openai-compatible" => "OpenAI Compatible",
            "tokenrouter" => "TokenRouter",
            "gemini" => "Gemini",
            "anthropic" => "Anthropic",
            "ollama" => "Ollama",
            "copilot" => "GitHub Copilot",
            "opencode" => "OpenCode",
            _ => provider,
        })
        .to_string()
}
/// Resolve identities only. Credentials stay in the core and shared Settings owner.
pub(super) fn resolve_selection(
    selection: &Selection,
    config: &Value,
) -> Result<Selection, String> {
    let mut s = selection.clone();
    let provider = s
        .provider
        .clone()
        .or_else(|| {
            config["taskProviderOverrides"]["chat"]
                .as_str()
                .map(str::to_string)
        })
        .or_else(|| config["provider"].as_str().map(str::to_string))
        .ok_or("Choose a provider in Settings")?;
    let model = s
        .model
        .clone()
        .or_else(|| {
            config["taskModelOverrides"]["chat"]
                .as_str()
                .filter(|_| {
                    config["taskProviderOverrides"]["chat"]
                        .as_str()
                        .is_none_or(|p| p == provider)
                })
                .map(str::to_string)
        })
        .or_else(|| {
            config["providerSelectedModels"][&provider]
                .as_str()
                .map(str::to_string)
        })
        .or_else(|| {
            (config["provider"] == provider)
                .then(|| config["model"].as_str().map(str::to_string))
                .flatten()
        })
        .ok_or("Choose a model in Settings")?;
    if !providers::model_options(config, "")
        .iter()
        .any(|m| m.provider == provider && m.model == model)
    {
        return Err("Selected model is unavailable. Choose a model in Settings.".into());
    }
    s.provider_title = Some(provider_title(config, &provider));
    s.provider = Some(provider);
    s.model = Some(model);
    if s.approval_mode.is_none() {
        s.approval_mode = serde_json::from_value(config["agentApprovalMode"].clone()).ok();
    }
    Ok(s)
}
impl ChatView {
    pub(super) fn clear_all(&mut self, w: &mut Window, cx: &mut Context<Self>) {
        if self.clear_inflight {
            return;
        }
        self.clear_inflight = true;
        self.mount += 1;
        self.init_generation += 1;
        self.prompt_generation += 1;
        self.read_generation += 1;
        self.prompt = None;
        self.arguments.clear();
        self.document_images.clear();
        self.thumbnail_attempts.clear();
        self.documents.clear();
        self.images.release(w, cx);
        self.pending_capture = None;
        self.attachment_tickets.clear();
        let effects = self.controller.begin_clear();
        let mut tickets = vec![];
        for e in effects {
            match e {
                Effect::DeleteConversation { token, session_id } => {
                    tickets.push((session_id, token))
                }
                e => self.execute(vec![e], w, cx),
            }
        }
        self.request(
            ChatRequest::Clear(tickets.iter().map(|(id, _)| id.clone()).collect()),
            Completion::Clear(tickets),
            cx,
        );
    }
    pub(super) fn request(
        &mut self,
        request: ChatRequest,
        completion: Completion,
        cx: &mut Context<Self>,
    ) {
        let transport = self.transport.clone();
        self.future(
            Box::pin(async move { transport.send(request).await }),
            completion,
            cx,
        );
    }
    fn future(&mut self, future: ChatFuture, completion: Completion, cx: &mut Context<Self>) {
        if self.jobs.len() >= 128 {
            let view = cx.entity().downgrade();
            let window = self.window;
            cx.defer(move |cx| {
                let _ = window.update(cx, |_, w, cx| {
                    view.update(cx, |v, cx| {
                        v.complete(
                            completion,
                            Err("Too many chat actions are finishing. Retry.".into()),
                            w,
                            cx,
                        )
                    })
                });
            });
            return;
        }
        self.next_job += 1;
        let job = self.next_job;
        if matches!(
            &completion,
            Completion::Quiet
                | Completion::Start { .. }
                | Completion::Persist { .. }
                | Completion::Delete { .. }
                | Completion::DeleteStored(_)
                | Completion::Clear(_)
                | Completion::DeleteTurn(_)
        ) {
            self.mutation_jobs.insert(job);
        }
        let window = self.window;
        let task = cx.spawn(async move |this, cx| {
            let result = gpui_tokio::Tokio::spawn(cx, future)
                .await
                .unwrap_or_else(|_| Err("Chat request interrupted. Retry.".into()));
            let _ = cx.update_window(window, |_, w, cx| {
                let _ = this.update(cx, |v, cx| {
                    v.jobs.remove(&job);
                    v.mutation_jobs.remove(&job);
                    v.complete(completion, result, w, cx);
                });
            });
        });
        self.jobs.insert(job, task);
    }
    pub(super) fn execute(&mut self, effects: Vec<Effect>, w: &mut Window, cx: &mut Context<Self>) {
        for effect in effects {
            match effect {
                Effect::LoadHistory { token } => {
                    self.loading = true;
                    self.history_error = false;
                    self.request(
                        ChatRequest::List,
                        Completion::History(token, self.init_generation),
                        cx,
                    )
                }
                Effect::LoadConversation { token, session_id } => {
                    self.loading = true;
                    self.request(
                        ChatRequest::Get(session_id),
                        Completion::Conversation(token, self.init_generation),
                        cx,
                    )
                }
                Effect::AppendUser {
                    token,
                    session_id,
                    turn,
                } => {
                    if !self.metadata.contains_key(&session_id) {
                        let (title, icon) = chat::meta::heuristic(&turn.text, None);
                        self.metadata
                            .insert(session_id.clone(), (title, Some(icon)));
                    }
                    self.prepare(session_id, token, turn, None, cx)
                }
                Effect::PrepareRetry {
                    token,
                    session_id,
                    user_turn_id,
                    assistant_turn_id: _,
                } => {
                    let user = self
                        .controller
                        .conversation(&session_id)
                        .and_then(|c| c.turns.iter().find(|t| t.id == user_turn_id))
                        .cloned();
                    if let Some(user) = user {
                        self.prepare(session_id, token, user, Some(user_turn_id), cx);
                    }
                }
                Effect::Start(request) => {
                    self.runs.retain(|_, s| s != &request.session_id);
                    self.runs
                        .insert(request.run_id.clone(), request.session_id.clone());
                    let images = if request.retry {
                        Ok(vec![])
                    } else {
                        self.blobs.for_references(&request.references)
                    };
                    match images {
                        Ok(images) => {
                            let run = request.run_id.clone();
                            let payloads = request
                                .references
                                .iter()
                                .filter_map(|r| {
                                    if let Reference::Image { blob_id, .. } = r {
                                        Some(blob_id.clone())
                                    } else {
                                        None
                                    }
                                })
                                .collect();
                            self.request(
                                ChatRequest::Run { request, images },
                                Completion::Start { run, payloads },
                                cx,
                            )
                        }
                        Err(_) => {
                            let e = self.controller.start_result(
                                &request.run_id,
                                Err("Image no longer available. Attach it again.".into()),
                            );
                            self.execute(e, w, cx)
                        }
                    }
                }
                Effect::Cancel { run_id } => {
                    self.request(ChatRequest::Cancel(run_id), Completion::Quiet, cx)
                }
                Effect::PersistTurn {
                    token,
                    session_id,
                    turn,
                } => {
                    let c = self.controller.conversation(&session_id);
                    let selection = c.map(|c| c.selection.clone()).unwrap_or_default();
                    let title = self
                        .metadata
                        .get(&session_id)
                        .map(|m| m.0.clone())
                        .or_else(|| c.map(|c| c.title.clone()))
                        .unwrap_or_else(|| chat::title(&turn.text));
                    self.request(
                        ChatRequest::Append {
                            session: session_id.clone(),
                            title,
                            selection,
                            turn,
                        },
                        Completion::Persist {
                            session: session_id,
                            token,
                        },
                        cx,
                    );
                }
                Effect::DeleteConversation { token, session_id } => self.request(
                    ChatRequest::Delete(session_id.clone()),
                    Completion::Delete {
                        session: session_id,
                        token,
                    },
                    cx,
                ),
                Effect::DeleteTurn {
                    token,
                    session_id,
                    turn_id,
                } => self.request(
                    ChatRequest::DeleteTurn {
                        session: session_id,
                        turn: turn_id,
                    },
                    Completion::DeleteTurn(token),
                    cx,
                ),
                Effect::Capture { token, session_id } => {
                    self.pending_capture = Some((
                        token,
                        AttachmentTicket {
                            session: session_id,
                            revision: self.draft_revision,
                            mount: self.mount,
                        },
                    ));
                    self.request(ChatRequest::Capture, Completion::Capture(token), cx)
                }
                Effect::Approve {
                    run_id,
                    approval_id,
                    decision,
                } => self.request(
                    ChatRequest::Approve {
                        run: run_id,
                        approval: approval_id,
                        decision,
                    },
                    Completion::Quiet,
                    cx,
                ),
                Effect::SaveSelection(s) => self.save_selection(s, cx),
                Effect::FocusApproval { session_id } => {
                    if self.controller.visible
                        && self.controller.active.as_deref() == Some(&session_id)
                        && self.active().is_some_and(|c| c.approval.is_some())
                    {
                        self.allow_focus.focus(w, cx);
                        if self.allow_focus.is_focused(w) {
                            self.controller.focus(&session_id, ApprovalFocus::AllowOnce);
                        }
                    }
                }
                Effect::ResolvePrompt {
                    token, arguments, ..
                } => self.prompt_effect(token, arguments, cx),
                Effect::ReviewProject(cwd) => self.emit(
                    ChatEffect::ReviewProject {
                        cwd,
                        nonce: identity("review"),
                    },
                    cx,
                ),
            }
        }
        self.publish(cx);
        cx.notify();
    }
    fn prepare(
        &mut self,
        session: String,
        token: u64,
        user: Arc<Turn>,
        retry: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let selection = self
            .controller
            .conversation(&session)
            .map(|c| c.selection.clone())
            .unwrap_or_default();
        let transport = self.transport.clone();
        let store = self.store.clone();
        let owner = self.owner;
        let snapshot = selection.clone();
        self.future(
            Box::pin(async move {
                store.drain_owner(owner).await?;
                let config = transport
                    .send(ChatRequest::Config(selection.cwd.clone()))
                    .await?;
                let selection = resolve_selection(&selection, &config)?;
                Ok(json!(selection))
            }),
            Completion::PreparationConfig {
                session,
                token,
                user,
                retry: retry.is_some(),
                selection: snapshot,
            },
            cx,
        );
    }
    fn preparation_current(&self, session: &str, token: u64) -> bool {
        !self.disposed
            && !self.clear_inflight
            && !self.deleting.contains(session)
            && self
                .controller
                .conversation(session)
                .is_some_and(|c| matches!(c.phase, RunPhase::Preparing { token:t } if t == token))
    }
    pub(super) fn save_mode(&mut self, mode: ApprovalMode, cx: &mut Context<Self>) {
        self.store.enqueue(
            self.owner,
            vec![Edit::Global {
                field: "agentApprovalMode".into(),
                value: json!(mode),
            }],
        );
        let store = self.store.clone();
        let owner = self.owner;
        self.future(
            Box::pin(async move { store.drain_owner(owner).await.map(|_| Value::Null) }),
            Completion::Quiet,
            cx,
        );
    }
    fn save_selection(&mut self, s: Selection, cx: &mut Context<Self>) {
        let mut edits = vec![];
        if let (Some(provider), Some(model)) = (s.provider, s.model) {
            edits.push(Edit::TaskSelection {
                task: "chat".into(),
                provider,
                model,
            });
        }
        if edits.is_empty() {
            return;
        }
        self.store.enqueue(self.owner, edits);
        let store = self.store.clone();
        let owner = self.owner;
        self.future(
            Box::pin(async move { store.drain_owner(owner).await.map(|_| Value::Null) }),
            Completion::Quiet,
            cx,
        );
    }
    fn complete(
        &mut self,
        completion: Completion,
        result: ChatReply,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match completion {
            Completion::Quiet => {
                if !ok(&result) {
                    self.error=Some("Chat action failed. Retry the action; a tool is never replayed automatically.".into());
                }
            }
            Completion::Config(g) => {
                if g == self.read_generation {
                    if let Ok(v) = result {
                        self.config = v;
                        if let Some(id) = self.controller.active.clone() {
                            let mode = serde_json::from_value::<ApprovalMode>(
                                self.config["agentApprovalMode"].clone(),
                            )
                            .unwrap_or(ApprovalMode::Default);
                            let _ = self.controller.set_mode(&id, mode);
                        }
                        if let Some(id) = self.controller.active.clone() {
                            if self
                                .active()
                                .is_some_and(|c| !c.busy() && c.selection.provider.is_none())
                            {
                                if let Ok(s) = resolve_selection(&self.selection(), &self.config) {
                                    let _ = self.controller.select(&id, s);
                                }
                            }
                        }
                    } else {
                        self.error =
                            Some("Could not load AI settings. Open Settings or retry.".into());
                    }
                }
            }
            Completion::History(token, generation) => {
                if generation != self.init_generation {
                    return;
                }
                self.loading = false;
                if let Ok(v) = result {
                    let rows = v
                        .as_array()
                        .into_iter()
                        .flatten()
                        .take(200)
                        .filter_map(|v| {
                            Some(chat::HistoryRow {
                                id: v["id"].as_str()?.into(),
                                title: v["title"]
                                    .as_str()
                                    .unwrap_or("New chat")
                                    .chars()
                                    .take(128)
                                    .collect(),
                                preview: v["preview"].as_str().unwrap_or("").into(),
                                updated_at: v["updatedAt"].as_u64().unwrap_or(0),
                            })
                        })
                        .collect();
                    self.controller.history_result(token, rows);
                } else {
                    self.history_error = true;
                    self.error = Some("Could not load conversations. Retry Past chats.".into());
                }
            }
            Completion::Conversation(token, generation) => {
                if generation != self.init_generation {
                    return;
                }
                self.loading = false;
                if let Ok(v) = result {
                    let Some(session) = v["id"].as_str().map(str::to_string) else {
                        self.error = Some("Conversation is unavailable. Open Past chats.".into());
                        return;
                    };
                    if self.controller.active.as_deref() == Some(&session)
                        && self.controller.conversation(&session).is_some()
                    {
                        if let Some(title) = v["title"].as_str() {
                            self.metadata.insert(
                                session,
                                (
                                    title.chars().take(128).collect(),
                                    v["icon"].as_str().map(str::to_string),
                                ),
                            );
                        }
                    }

                    let turns = v["turns"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|t| Turn::from_core_wire(t.clone()).ok().map(Arc::new))
                        .collect();
                    let selection = Selection {
                        cwd: v["workingDirectory"].as_str().map(str::to_string),
                        provider: v["turns"]
                            .as_array()
                            .and_then(|ts| {
                                ts.iter()
                                    .rev()
                                    .find_map(|t| t["responseMeta"]["provider"].as_str())
                            })
                            .map(str::to_string),
                        model: v["turns"]
                            .as_array()
                            .and_then(|ts| {
                                ts.iter()
                                    .rev()
                                    .find_map(|t| t["responseMeta"]["model"].as_str())
                            })
                            .map(str::to_string),
                        provider_title: v["turns"]
                            .as_array()
                            .and_then(|ts| {
                                ts.iter()
                                    .rev()
                                    .find_map(|t| t["responseMeta"]["providerTitle"].as_str())
                            })
                            .map(str::to_string),
                        profile: v["agentProfileId"].as_str().map(str::to_string),
                        ..Default::default()
                    };
                    self.controller
                        .history_result_conversation(token, turns, selection);
                    self.refresh_scope(cx);
                } else {
                    self.error = Some("Could not load conversation. Retry Past chats.".into());
                }
            }
            Completion::HistoryImage { pending } => {
                self.history_image_complete(pending, result, w, cx);
            }
            Completion::PreparationConfig {
                session,
                token,
                user,
                retry,
                selection,
            } => {
                if !self.preparation_current(&session, token) {
                    return;
                }
                match result.and_then(|v| {
                    serde_json::from_value::<Selection>(v)
                        .map_err(|_| "Invalid preparation selection".into())
                }) {
                    Ok(selected) => {
                        let completion = Completion::Prepared {
                            session: session.clone(),
                            token,
                            user: user.clone(),
                            retry,
                            selection: selected.clone(),
                        };
                        if retry {
                            self.request(ChatRequest::Get(session), completion, cx);
                        } else {
                            self.request(
                                ChatRequest::Append {
                                    session,
                                    title: chat::title(&user.text),
                                    selection: selected,
                                    turn: user,
                                },
                                completion,
                                cx,
                            );
                        }
                    }
                    Err(_) => {
                        let e = self.controller.prepared_with_selection(
                            &session,
                            token,
                            Err("Could not prepare message".into()),
                            selection,
                            now(),
                        );
                        self.execute(e, w, cx);
                    }
                }
            }
            Completion::Prepared {
                session,
                token,
                user,
                retry,
                selection,
            } => {
                if !self.preparation_current(&session, token) {
                    return;
                }
                let canonical = if ok(&result) {
                    if retry {
                        result
                            .as_ref()
                            .ok()
                            .and_then(|v| v["turns"].as_array())
                            .and_then(|ts| ts.iter().find(|t| t["id"] == user.id))
                            .ok_or_else(|| "Stored user turn is unavailable".to_string())
                            .and_then(|t| {
                                Turn::from_core_wire(t.clone())
                                    .map(Arc::new)
                                    .map_err(|_| "Could not read canonical user".into())
                            })
                    } else {
                        Ok(user)
                    }
                } else {
                    Err("Could not prepare message".into())
                };
                let e = self.controller.prepared_with_selection(
                    &session,
                    token,
                    canonical,
                    selection,
                    now(),
                );
                self.execute(e, w, cx);
            }
            Completion::Start { run, payloads } => {
                if ok(&result) {
                    if let Some(session) = self.runs.get(&run) {
                        if let Some(c) = self.controller.conversation(session) {
                            if !matches!(c.phase, RunPhase::Preparing { .. }) {
                                let draft = c
                                    .attachments
                                    .iter()
                                    .filter_map(|r| {
                                        if let Reference::Image { blob_id, .. } = r {
                                            Some(blob_id.clone())
                                        } else {
                                            None
                                        }
                                    })
                                    .collect::<BTreeSet<_>>();
                                for id in payloads {
                                    if !draft.contains(&id) {
                                        self.blobs.remove(&id);
                                    }
                                }
                            }
                        }
                    }
                }
                let r = if ok(&result) {
                    result
                        .ok()
                        .and_then(|v| v["runId"].as_str().map(str::to_string))
                        .ok_or_else(|| "Agent returned an invalid run identity".into())
                } else {
                    Err("Could not start agent. Check the selected provider and retry.".into())
                };
                let e = self.controller.start_result(&run, r);
                self.execute(e, w, cx);
            }
            Completion::Persist { session, token } => {
                let success = ok(&result);
                self.controller.persisted(&session, token, success);
                if success
                    && !self.disposed
                    && !self.deleting.contains(&session)
                    && !self.clear_inflight
                    && !self.metadata_requested.contains(&session)
                {
                    if let Some(c) = self.controller.conversation(&session) {
                        let first = c
                            .turns
                            .iter()
                            .find(|t| t.role == chat::Role::User)
                            .map(|t| t.text.clone());
                        let answer = c
                            .turns
                            .iter()
                            .rev()
                            .find(|t| t.role == chat::Role::Assistant)
                            .map(|t| t.text.clone());
                        if let (Some(prompt), Some(answer)) = (first, answer) {
                            self.metadata_requested.insert(session.clone());
                            self.request(
                                ChatRequest::GenerateMeta {
                                    session: session.clone(),
                                    prompt,
                                    answer,
                                },
                                Completion::Meta(session.clone()),
                                cx,
                            );
                        }
                    }
                }
                if !success {
                    self.error =
                        Some("Response is kept here but could not be saved. Retry saving.".into());
                }
            }
            Completion::Delete { session, token } => {
                let success = ok(&result);
                let e = self.controller.delete_result(&session, token, success);
                self.deleting.remove(&session);
                if success {
                    self.runs.retain(|_, s| s != &session);
                    self.blobs.remove_owner(&session);
                }
                self.execute(e, w, cx);
                if !success {
                    self.error =
                        Some("Could not delete conversation. Its response is retained.".into());
                }
            }
            Completion::DeleteStored(session) => {
                self.deleting.remove(&session);
                if ok(&result) {
                    if let Some(rows) = &mut self.controller.history {
                        rows.retain(|r| r.id != session);
                    }
                } else {
                    self.error = Some("Could not delete conversation. Retry.".into());
                }
            }
            Completion::DeleteTurn(token) => {
                self.controller.turn_deleted(token, ok(&result));
                if !ok(&result) {
                    self.error = Some("Could not delete message. Retry.".into());
                }
            }
            Completion::Capture(token) => self.capture_complete(token, result, w, cx),
            Completion::Directory { session, mount } => {
                if self.mount == mount && self.controller.active.as_deref() == Some(&session) {
                    if let Ok(v) = result {
                        if let Some(path) = v.as_str().or_else(|| v["path"].as_str()) {
                            self.action(Action::SetDirectory(path.to_string()), w, cx)
                        }
                    }
                }
            }
            Completion::Scope {
                generation,
                cwd,
                project,
            } => {
                if generation == self.read_generation && self.selection().cwd == cwd {
                    if let Ok(v) = result {
                        if project {
                            if v["project"].is_object() {
                                self.project = cwd.filter(|p| !self.dismissed_projects.contains(p));
                            }
                        } else {
                            self.items = v
                                .as_array()
                                .map(|a| a.iter().take(256).cloned().collect())
                                .unwrap_or_default();
                        }
                    } else {
                        self.error = Some(
                            "Could not load AI contributions. Retry Past chats or open Settings."
                                .into(),
                        );
                    }
                }
            }
            Completion::Prompt {
                token,
                generation,
                session,
                revision,
            } => self.prompt_complete(token, generation, session, revision, result, w, cx),
            Completion::Clear(tickets) => {
                self.clear_inflight = false;
                let success = ok(&result);
                for (id, token) in tickets {
                    let e = self.controller.delete_result(&id, token, success);
                    self.execute(e, w, cx);
                }
                if success {
                    self.controller.history = Some(vec![]);
                    self.blobs = Default::default();
                    self.runs.clear();
                    self.diagnostics.clear();
                    self.documents.clear();
                    self.images.release(w, cx);
                } else {
                    self.error = Some(
                        "Could not clear conversations. Responses are retained; retry clearing."
                            .into(),
                    );
                }
            }
            Completion::Meta(session) => {
                if self.controller.conversation(&session).is_some() {
                    if let Ok(v) = result {
                        if let Some(title) = v["title"].as_str() {
                            let title = title.chars().take(128).collect::<String>();
                            let icon = v["icon"].as_str().map(str::to_string);
                            if self.metadata.len() >= 32 {
                                if let Some(id) = self.metadata.keys().next().cloned() {
                                    self.metadata.remove(&id);
                                }
                            }
                            self.metadata.insert(session.clone(), (title.clone(), icon));
                            if let Some(rows) = &mut self.controller.history {
                                if let Some(row) = rows.iter_mut().find(|r| r.id == session) {
                                    row.title = title;
                                }
                            }
                        }
                    }
                }
            }
            Completion::Directories(g) => {
                if g == self.init_generation {
                    if let Ok(v) = result {
                        self.recent_directories = v
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|p| p["kind"] == "directory")
                            .filter_map(|p| p["path"].as_str().or_else(|| p["value"].as_str()))
                            .take(12)
                            .map(str::to_string)
                            .collect();
                    }
                }
            }
        }
        if self.controller.visible {
            self.sync_input(w, cx);
            self.refresh_documents(cx);
            self.refresh_thumbnails(w, cx);
        }
        self.prune();
        self.blobs.retain_controller(&self.controller);
        self.publish(cx);
        cx.notify();
    }
    pub fn core_event(
        &mut self,
        channel: &str,
        mut payload: Value,
        w: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.disposed && channel != "agent:event" {
            return;
        }
        if channel == "llm-config-changed" || channel == "ai-extensions-changed" {
            self.refresh_scope(cx);
            return;
        }
        if channel != "agent:event" {
            return;
        }
        for field in ["line", "message"] {
            if let Some(raw) = payload[field].as_str() {
                payload[field] = json!(scrub(raw, &self.config));
            }
        }
        let run = payload["runId"].as_str().unwrap_or("");
        let Some(owner) = self.runs.get(run).cloned() else {
            return;
        };
        if self
            .controller
            .conversation(&owner)
            .is_some_and(|c| !c.busy())
        {
            return;
        }
        if self.controller.conversation(&owner).is_none()
            && !self.deleting.contains(&owner)
            && !self.clear_inflight
        {
            return;
        }
        if payload["sessionId"].as_str().is_some_and(|s| s != owner) {
            return;
        }
        if let Some(count) = payload["tokenCount"].as_u64() {
            self.controller.token_count(run, count);
        }
        if payload["type"] == "log" {
            let list = self.diagnostics.entry(owner.clone()).or_default();
            if list.len() >= 80 {
                list.remove(0);
            }
            list.push(
                payload["line"]
                    .as_str()
                    .unwrap_or("")
                    .chars()
                    .take(2000)
                    .collect(),
            );
        }
        let restore_focus = self.approval_has_focus(w);
        if let Ok(event) = serde_json::from_value::<chat::Event>(payload) {
            let e = self.controller.event(event);
            if !e.is_empty() {
                self.execute(e, w, cx);
            }
        }
        if restore_focus && self.active().is_some_and(|c| c.approval.is_none()) {
            self.input.focus_handle(cx).focus(w, cx);
        }
        self.publish(cx);
        if self.controller.visible && self.frame.is_none() {
            let window = self.window;
            self.frame = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let _ = cx.update_window(window, |_, w, cx| {
                    let _ = this.update(cx, |v, cx| {
                        v.frame = None;
                        v.refresh_documents(cx);
                        if v.follow {
                            v.transcript.scroll_to_end();
                        }
                        v.sync_input(w, cx);
                        cx.notify();
                    });
                });
            }));
        }
    }
    pub(super) fn refresh_scope(&mut self, cx: &mut Context<Self>) {
        self.read_generation += 1;
        let cwd = self.selection().cwd;
        self.prompt_generation += 1;
        self.prompt = None;
        self.arguments.clear();
        self.item_scope = cwd.clone();
        self.items.clear();
        self.project = None;
        self.request(
            ChatRequest::Config(cwd.clone()),
            Completion::Config(self.read_generation),
            cx,
        );
        self.request(
            ChatRequest::ComposerItems(cwd.clone()),
            Completion::Scope {
                generation: self.read_generation,
                cwd: cwd.clone(),
                project: false,
            },
            cx,
        );
        if cwd.is_some() {
            self.request(
                ChatRequest::Contributions(cwd.clone()),
                Completion::Scope {
                    generation: self.read_generation,
                    cwd,
                    project: true,
                },
                cx,
            );
        }
    }
    pub(super) fn refresh_documents(&mut self, cx: &mut Context<Self>) {
        if !self.controller.visible {
            return;
        }
        let turns = self
            .active()
            .map(|c| {
                c.turns
                    .iter()
                    .cloned()
                    .chain(c.response().cloned().map(Arc::new))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.projection = view::project(
            &turns,
            self.active()
                .and_then(|c| c.response())
                .map(|r| r.id.as_str()),
            self.active().map(|c| c.timeline.as_slice()).unwrap_or(&[]),
        );
        let count = self.projection.len();
        if self.transcript.item_count() != count {
            self.transcript.reset(count);
        } else {
            self.transcript.remeasure();
        }
        let texts = self
            .projection
            .iter()
            .filter_map(|r| {
                if let view::Row::Text {
                    id,
                    turn,
                    range,
                    settled,
                } = r
                {
                    Some((id.clone(), turn.clone(), range.clone(), *settled))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let ids = texts.iter().map(|t| t.0.clone()).collect::<BTreeSet<_>>();
        self.documents.retain(|id, _| ids.contains(id));
        for (id, turn, range, settled) in texts {
            if !self.documents.contains_key(&id) {
                let view = cx.new(|_| MarkdownView::default());
                let document_id = id.clone();
                let sub = cx.subscribe(&view, move |v, _, e: &DocumentEffect, cx| match e {
                    DocumentEffect::Copy(text) => {
                        v.request(ChatRequest::Copy(text.clone()), Completion::Quiet, cx)
                    }
                    DocumentEffect::OpenLink(url) => {
                        if let Some(url) = tezbar_ui::markdown::safe_link(url) {
                            v.request(ChatRequest::OpenUrl(url), Completion::Quiet, cx);
                        }
                    }
                    DocumentEffect::Image(ticket) => {
                        v.request_document_image(&document_id, ticket.clone(), cx)
                    }
                });
                self.documents.insert(
                    id.clone(),
                    DocumentEntry {
                        view,
                        _subscription: sub,
                    },
                );
            }
            let text = turn.text.get(range).unwrap_or("");
            let revision = text
                .as_bytes()
                .iter()
                .fold(0u64, |a, b| a.wrapping_mul(31).wrapping_add(*b as u64));
            self.documents[&id]
                .view
                .update(cx, |v, cx| v.set_content(revision, text, settled, cx));
        }
    }
}

fn scrub(raw: &str, config: &Value) -> String {
    fn secrets(v: &Value, sensitive: bool, out: &mut Vec<String>) {
        match v {
            Value::Object(m) => {
                for (k, v) in m {
                    let key = k.to_ascii_lowercase();
                    secrets(
                        v,
                        sensitive
                            || [
                                "key",
                                "token",
                                "password",
                                "secret",
                                "authorization",
                                "headers",
                            ]
                            .iter()
                            .any(|word| key.contains(word)),
                        out,
                    )
                }
            }
            Value::Array(a) => {
                for v in a {
                    secrets(v, sensitive, out)
                }
            }
            Value::String(s) if sensitive && s.len() >= 4 => out.push(s.clone()),
            _ => {}
        }
    }
    let mut values = vec![];
    secrets(config, false, &mut values);
    values.sort_by_key(|s| std::cmp::Reverse(s.len()));
    let mut text = raw.to_string();
    for value in values {
        text = text.replace(&value, "[redacted]");
    }
    text
}
