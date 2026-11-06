//! Shared executor: intent revisions are assigned synchronously, before a view can leave.
use super::{SettingsRequest, SettingsTransport};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Mutex},
};
use tezbar_ui_model::settings::{CLOSE_ERROR, Edit, Effect, OwnerId, WriteQueue};
#[derive(Default)]
struct State {
    queue: WriteQueue,
    next_owner: u64,
    canonical: Option<Value>,
    revision: u64,
    settled: u64,
    config_revision: u64,
    config_settled: u64,
    next_operation: u64,
    operations: VecDeque<(u64, SettingsRequest)>,
    results: BTreeMap<u64, Result<Value, String>>,
    failed: BTreeMap<u64, SettingsRequest>,
    active: BTreeMap<u64, tokio::task::JoinHandle<()>>,
    in_transit: BTreeSet<u64>,
    last_copy: u64,
    latest_mutations: BTreeMap<String, u64>,
    validation: BTreeMap<(OwnerId, String), &'static str>,
    aliases: BTreeMap<String, (OwnerId, String)>,
    aliases_pending: bool,
    alias_read_failed: bool,
    executor_failed: bool,
}
#[derive(Clone)]
pub struct SettingsStore {
    state: Arc<Mutex<State>>,
    worker: Arc<tokio::sync::Mutex<Option<tokio::task::JoinHandle<Result<(), String>>>>>,
    transport: Arc<dyn SettingsTransport>,
    completion: Arc<tokio::sync::Notify>,
    config_lane: Arc<tokio::sync::Mutex<()>>,
    mutation_lane: Arc<tokio::sync::Mutex<()>>,
}
impl SettingsStore {
    pub fn new(transport: Arc<dyn SettingsTransport>) -> Self {
        Self {
            state: Arc::default(),
            worker: Arc::default(),
            transport,
            completion: Arc::default(),
            config_lane: Arc::default(),
            mutation_lane: Arc::default(),
        }
    }
    pub fn transport(&self) -> Arc<dyn SettingsTransport> {
        self.transport.clone()
    }
    pub fn owner(&self) -> OwnerId {
        let mut s = self.state.lock().unwrap();
        s.next_owner += 1;
        let id = s.next_owner;
        s.queue.register(id);
        id
    }
    pub fn retire(&self, owner: OwnerId) {
        self.state.lock().unwrap().queue.retire(owner);
    }
    pub fn enqueue(&self, owner: OwnerId, edits: Vec<Edit>) -> u64 {
        {
            let mut s = self.state.lock().unwrap();
            s.revision += 1;
            s.config_revision += 1;
            self.completion.notify_waiters();
            self.completion.notify_one();
            s.queue.enqueue(owner, edits)
        }
    }
    pub fn discover(&self, owner: OwnerId, edit: Edit) {
        let mut s = self.state.lock().unwrap();
        s.revision += 1;
        s.config_revision += 1;
        s.queue.discover(owner, vec![edit]);
        self.completion.notify_waiters();
        self.completion.notify_one();
    }
    pub fn validation(&self, owner: OwnerId, key: &str, error: Option<&'static str>) {
        let mut s = self.state.lock().unwrap();
        if let Some(e) = error {
            s.validation.insert((owner, key.into()), e);
        } else {
            s.validation.remove(&(owner, key.into()));
        }
    }
    pub fn alias(&self, owner: OwnerId, id: String, value: String) {
        let mut s = self.state.lock().unwrap();
        s.revision += 1;
        s.config_revision += 1;
        s.aliases.insert(id, (owner, value));
        s.aliases_pending = true;
        self.completion.notify_waiters();
        self.completion.notify_one();
    }
    pub fn retry(&self) {
        let mut s = self.state.lock().unwrap();
        s.revision += 1;
        s.config_revision += 1;
        s.queue.retry(false);
        if s.alias_read_failed {
            s.aliases_pending = true;
        }
        let failed = std::mem::take(&mut s.failed);
        s.operations.extend(failed);
        self.completion.notify_waiters();
        self.completion.notify_one();
    }
    pub fn admit(&self, request: SettingsRequest) -> Result<u64, String> {
        let mut s = self.state.lock().unwrap();
        if s.operations.len() + s.failed.len() + s.active.len() + s.in_transit.len()
            >= if request.interrupt() { 16 } else { 15 }
        {
            return Err("Settings actions are still finishing. Retry.".into());
        }
        s.revision += 1;
        s.next_operation += 1;
        let id = s.next_operation;
        if let Some(scope) = request.mutation_scope() {
            s.latest_mutations.insert(scope.clone(), id);
            s.failed
                .retain(|_, old| old.mutation_scope().as_ref() != Some(&scope));
        }
        if matches!(request, SettingsRequest::PracticeCopy(_)) {
            s.last_copy = id;
        }
        s.operations.push_back((id, request));
        self.completion.notify_waiters();
        self.completion.notify_one();
        Ok(id)
    }
    pub async fn operation_result(&self, id: u64) -> Result<Value, String> {
        let interrupt = {
            let mut s = self.state.lock().unwrap();
            let at = s
                .operations
                .iter()
                .position(|(token, request)| *token == id && request.interrupt());
            at.and_then(|at| {
                let item = s.operations.remove(at)?;
                s.in_transit.insert(item.0);
                Some(item)
            })
        };
        if let Some((id, request)) = interrupt {
            self.launch_operation(id, request);
        }
        // The UI waits for its own ticket, while Close waits for every admitted operation.
        // Keeping those waits distinct lets Cancel acknowledge a superseded hung mutation.
        self.start_worker().await;
        loop {
            let notified = self.completion.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(result) = self.result(id) {
                return result;
            }
            let present = {
                let s = self.state.lock().unwrap();
                s.active.contains_key(&id)
                    || s.in_transit.contains(&id)
                    || s.operations.iter().any(|(key, _)| *key == id)
            };
            if !present {
                return Err("Could not complete Settings action. Retry.".into());
            }
            notified.await;
        }
    }
    pub fn result(&self, id: u64) -> Option<Result<Value, String>> {
        self.state.lock().unwrap().results.remove(&id)
    }
    pub fn pending(&self) -> bool {
        let s = self.state.lock().unwrap();
        s.revision != s.settled || !s.operations.is_empty()
    }
    pub fn canonical(&self) -> Option<Value> {
        self.state.lock().unwrap().canonical.clone()
    }
    pub fn failures(&self) -> usize {
        {
            let s = self.state.lock().unwrap();
            s.queue.failure_count()
                + s.failed.len()
                + s.validation.len()
                + usize::from(s.alias_read_failed)
                + usize::from(s.executor_failed)
        }
    }
    pub fn drain_owner(&self, owner: OwnerId) -> tezbar_core::host::HostFuture<()> {
        let this = self.clone();
        Box::pin(async move {
            let target = this.state.lock().unwrap().config_revision;
            this.start_worker().await;
            loop {
                let wait = this.completion.notified();
                tokio::pin!(wait);
                wait.as_mut().enable();
                {
                    let state = this.state.lock().unwrap();
                    if state.executor_failed {
                        return Err(CLOSE_ERROR.into());
                    }
                    if state.config_settled >= target {
                        return if state.queue.owner_failure_count(owner) > 0
                            || state.validation.keys().any(|(id, _)| *id == owner)
                        {
                            Err(CLOSE_ERROR.into())
                        } else {
                            Ok(())
                        };
                    }
                }
                wait.await;
            }
        })
    }
    async fn start_worker(&self) {
        if let Ok(mut worker) = self.worker.try_lock() {
            if worker.as_ref().is_none_or(|t| t.is_finished()) {
                if let Some(done) = worker.take() {
                    if done.await.is_err() {
                        self.state.lock().unwrap().executor_failed = true;
                    }
                }
                let pump = self.clone();
                *worker = Some(tokio::spawn(async move { pump.pump().await }));
            }
        }
    }
    pub fn drain(&self) -> tezbar_core::host::HostFuture<()> {
        let this = self.clone();
        Box::pin(async move {
            // A cancelled GPUI waiter cannot strand the model between Read and Save.
            let mut worker = this.worker.lock().await;
            loop {
                if worker.is_none() {
                    let pump = this.clone();
                    *worker = Some(tokio::spawn(async move { pump.pump().await }));
                }
                let joined = worker.as_mut().unwrap().await;
                if joined.is_err() {
                    this.state.lock().unwrap().executor_failed = true;
                }
                let result = joined.map_err(|_| CLOSE_ERROR.to_string()).and_then(|v| v);
                worker.take();
                if this.pending() {
                    continue;
                }
                return result;
            }
        })
    }
    pub fn drain_with(&self, barrier: super::EffectBarrier) -> tezbar_core::host::HostFuture<()> {
        let store = self.clone();
        Box::pin(async move {
            loop {
                store.drain().await?;
                barrier.drain().await?;
                if !store.pending() && barrier.is_clean() {
                    return Ok(());
                }
            }
        })
    }
    fn operation_finished(
        &self,
        id: u64,
        request: SettingsRequest,
        mut result: Result<Value, String>,
    ) {
        let mut s = self.state.lock().unwrap();
        s.active.remove(&id);
        if request
            .mutation_scope()
            .is_some_and(|scope| s.latest_mutations.get(&scope) != Some(&id))
        {
            result = Ok(Value::Null);
        }
        if request.recoverable_write() && !success(&result) {
            s.failed.insert(id, request);
        }
        if s.results.len() >= 64 {
            if let Some(key) = s.results.keys().next().copied() {
                s.results.remove(&key);
            }
        }
        s.results.insert(
            id,
            result.map_err(|_| "Could not complete Settings action. Retry.".into()),
        );
        drop(s);
        self.completion.notify_waiters();
    }
    fn launch_operation(&self, id: u64, request: SettingsRequest) {
        let owner = self.clone();
        let transport = self.transport.clone();
        let (start, ready) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            let _ = ready.await;
            let _mutation = if request.ordered_mutation() {
                Some(owner.mutation_lane.lock().await)
            } else {
                None
            };
            let obsolete = request.mutation_scope().is_some_and(|scope| {
                owner.state.lock().unwrap().latest_mutations.get(&scope) != Some(&id)
            });
            let result = if obsolete {
                Ok(Value::Null)
            } else if matches!(
                request,
                SettingsRequest::DevicePoll | SettingsRequest::PracticeCopy(_)
            ) {
                let _guard = owner.config_lane.lock().await;
                if matches!(request, SettingsRequest::PracticeCopy(_))
                    && owner.state.lock().unwrap().last_copy != id
                {
                    Ok(Value::Null)
                } else {
                    transport.send(request.clone()).await
                }
            } else {
                transport.send(request.clone()).await
            };
            owner.operation_finished(id, request, result);
        });
        let mut state = self.state.lock().unwrap();
        state.in_transit.remove(&id);
        state.active.insert(id, task);
        drop(state);
        let _ = start.send(());
    }
    async fn pump(&self) -> Result<(), String> {
        loop {
            let aliases = {
                let mut s = self.state.lock().unwrap();
                if s.aliases_pending {
                    s.aliases_pending = false;
                    Some(s.aliases.clone())
                } else {
                    None
                }
            };
            if let Some(aliases) = aliases {
                // Every not-yet-saved alias is included, so a newer successful map write cannot
                // silently supersede another alias whose earlier write failed.
                match self.transport.durable_config().await {
                    Ok(mut current) => {
                        self.state.lock().unwrap().alias_read_failed = false;
                        if !current["commandAliases"].is_object() {
                            current["commandAliases"] = serde_json::json!({});
                        }
                        let mut owner = 0;
                        for (id, (o, value)) in aliases {
                            owner = o;
                            current["commandAliases"][id] = serde_json::json!(value);
                        }
                        self.state.lock().unwrap().queue.enqueue(
                            owner,
                            vec![Edit::Global {
                                field: "commandAliases".into(),
                                value: current["commandAliases"].clone(),
                            }],
                        );
                    }
                    Err(_) => {
                        self.state.lock().unwrap().alias_read_failed = true;
                    }
                }
            }
            let (effect, revision, config_revision) = {
                let mut s = self.state.lock().unwrap();
                (s.queue.next_effect(), s.revision, s.config_revision)
            };
            let Some(Effect::Read { token }) = effect else {
                {
                    let mut state = self.state.lock().unwrap();
                    if !state.aliases_pending {
                        state.config_settled = config_revision;
                    }
                }
                self.completion.notify_waiters();
                // Operations on other stores/runtimes remain owned but concurrent. In
                // particular a download cancellation must not queue behind its hung start.
                // DevicePoll can persist provider credentials, so it stays on the config lane.
                let operations = {
                    let mut s = self.state.lock().unwrap();
                    let operations = s.operations.drain(..).collect::<Vec<_>>();
                    for (id, _) in &operations {
                        s.in_transit.insert(*id);
                    }
                    operations
                };
                for (id, request) in operations {
                    self.launch_operation(id, request);
                }
                let notified = self.completion.notified();
                tokio::pin!(notified);
                notified.as_mut().enable();
                let (active, queued) = {
                    let s = self.state.lock().unwrap();
                    (
                        !s.active.is_empty() || !s.in_transit.is_empty(),
                        !s.operations.is_empty(),
                    )
                };
                if queued {
                    continue;
                }
                if active {
                    notified.await;
                    continue;
                }
                let mut s = self.state.lock().unwrap();
                // The queue was examined under the same synchronous owner lock; a new intent
                // increments revision and is drained on the next waiter even after this return.
                if s.revision != revision {
                    continue;
                }
                s.settled = revision;
                break;
            };
            let _config_guard = self.config_lane.lock().await;
            let fresh = self.transport.durable_config().await;
            let save = match fresh {
                Ok(v) => self.state.lock().unwrap().queue.read_result(token, Ok(v)),
                Err(_) => self.state.lock().unwrap().queue.read_result(token, Err(())),
            };
            if let Some(Effect::Save { token, patch }) = save {
                let saved_aliases = patch.get("commandAliases").cloned();
                let ok = success(&self.transport.send(SettingsRequest::Save(patch)).await);
                if ok {
                    if let Some(aliases) = saved_aliases {
                        self.state
                            .lock()
                            .unwrap()
                            .aliases
                            .retain(|id, (_, value)| aliases[id] != *value);
                    }
                }
                self.state.lock().unwrap().queue.finish(token, ok);
                if ok {
                    self.state.lock().unwrap().canonical = None;
                    if let Ok(v) = self.transport.durable_config().await {
                        self.state.lock().unwrap().canonical = Some(v);
                    }
                }
            }
        }
        if self.failures() > 0 {
            Err(CLOSE_ERROR.into())
        } else {
            Ok(())
        }
    }
}
pub fn success(r: &Result<Value, String>) -> bool {
    matches!(r,Ok(v) if v["ok"] != false)
}
