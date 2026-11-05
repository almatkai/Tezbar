//! Window-owned durable Settings intents. Queue order never changes intent revisions.
//! Config values can contain credentials; no config, intent, or write effect implements Debug.
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
pub type OwnerId = u64;
pub type Revision = u64;
pub const CLOSE_ERROR: &str =
    "Couldn't save Settings. Retry the edit, or retry saved changes from a previous tab.";
/// Reads the existing core catalog at compile time; no second provider catalog.
pub fn provider_catalog() -> &'static Value {
    static CATALOG: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../tezbar-core/src/ai/provider_defaults.json"
        ))
        .expect("core catalog")
    })
}
pub fn recommended(provider: &str) -> String {
    provider_catalog()["RECOMMENDED_AI_MODEL"][provider]
        .as_str()
        .unwrap_or("")
        .into()
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Key {
    Field(String, String),
    Models(String),
    Definition(String),
    Selection(String),
    Active,
    Global(String),
    Remove(String),
}
impl Key {
    fn provider(&self) -> Option<&str> {
        match self {
            Self::Field(p, _)
            | Self::Models(p)
            | Self::Definition(p)
            | Self::Selection(p)
            | Self::Remove(p) => Some(p),
            _ => None,
        }
    }
}
#[derive(Clone)]
pub enum Edit {
    Field {
        provider: String,
        field: String,
        value: Value,
    },
    Models {
        provider: String,
        models: Vec<Value>,
    },
    Definition {
        provider: String,
        value: Value,
    },
    Select {
        provider: String,
        model: String,
    },
    Activate {
        provider: String,
        model: String,
    },
    RemoveProvider(String),
    RemoveModels {
        provider: String,
        ids: BTreeSet<String>,
    },
    Global {
        field: String,
        value: Value,
    },
}
impl Edit {
    fn key(&self) -> Key {
        match self {
            Self::Field {
                provider, field, ..
            } => Key::Field(provider.clone(), field.clone()),
            Self::Models { provider, .. } | Self::RemoveModels { provider, .. } => {
                Key::Models(provider.clone())
            }
            Self::Definition { provider, .. } => Key::Definition(provider.clone()),
            Self::Select { provider, .. } => Key::Selection(provider.clone()),
            Self::Activate { .. } => Key::Active,
            Self::RemoveProvider(p) => Key::Remove(p.clone()),
            Self::Global { field, .. } => Key::Global(field.clone()),
        }
    }
}
#[derive(Clone)]
struct Intent {
    id: u64,
    owner: OwnerId,
    revision: Revision,
    edits: Vec<Edit>,
    optional: bool,
    sign_in: bool,
}
pub enum Effect {
    Read { token: u64 },
    Save { token: u64, patch: Value },
    StartDeviceSignIn { token: u64 },
    AcknowledgeClose { request: u64 },
    Close { request: u64 },
    KeepOpen { request: u64, message: &'static str },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Reading,
    Saving,
}
struct Flight {
    token: u64,
    intent: Intent,
    phase: Phase,
    keys: Vec<Key>,
}
#[derive(Default)]
pub struct WriteQueue {
    revision: Revision,
    next: u64,
    owners: BTreeMap<OwnerId, bool>,
    pending: VecDeque<Intent>,
    flight: Option<Flight>,
    failures: BTreeMap<Key, Intent>,
    saved: BTreeMap<Key, Revision>,
    removed: BTreeMap<String, Revision>,
    close: Option<(u64, u64, bool)>,
    pub message: Option<&'static str>,
}
impl WriteQueue {
    pub fn register(&mut self, owner: OwnerId) {
        self.owners.insert(owner, false);
    }
    pub fn retire(&mut self, owner: OwnerId) {
        if let Some(retired) = self.owners.get_mut(&owner) {
            *retired = true;
        }
        self.collect_owners();
    }
    pub fn enqueue(&mut self, owner: OwnerId, edits: Vec<Edit>) -> Revision {
        self.insert(owner, edits, false, false)
    }
    pub fn discover(&mut self, owner: OwnerId, edits: Vec<Edit>) -> Revision {
        self.insert(owner, edits, true, false)
    }
    pub fn sign_in(
        &mut self,
        owner: OwnerId,
        copilot_edits: Vec<Edit>,
    ) -> Result<Revision, &'static str> {
        if copilot_edits
            .iter()
            .any(|e| e.key().provider() != Some("copilot"))
        {
            return Err("Sign-in may only save Copilot fields");
        }
        Ok(self.insert(owner, copilot_edits, false, true))
    }
    fn insert(
        &mut self,
        owner: OwnerId,
        edits: Vec<Edit>,
        optional: bool,
        sign_in: bool,
    ) -> Revision {
        self.revision += 1;
        self.next += 1;
        self.owners.entry(owner).or_insert(false);
        self.pending.push_back(Intent {
            id: self.next,
            owner,
            revision: self.revision,
            edits,
            optional,
            sign_in,
        });
        self.revision
    }
    pub fn next_effect(&mut self) -> Option<Effect> {
        if self.flight.is_some() {
            return None;
        }
        let intent = self.pending.pop_front()?;
        self.next += 1;
        let token = self.next;
        self.flight = Some(Flight {
            token,
            intent,
            phase: Phase::Reading,
            keys: vec![],
        });
        Some(Effect::Read { token })
    }
    pub fn phase(&self) -> Option<(u64, Phase)> {
        self.flight.as_ref().map(|f| (f.token, f.phase))
    }
    fn stale(&self, key: &Key, revision: Revision) -> bool {
        self.saved.get(key).copied().unwrap_or(0) > revision
            || key
                .provider()
                .is_some_and(|p| self.removed.get(p).copied().unwrap_or(0) > revision)
    }
    pub fn read_result(&mut self, token: u64, current: Result<Value, ()>) -> Option<Effect> {
        if !self
            .flight
            .as_ref()
            .is_some_and(|f| f.token == token && f.phase == Phase::Reading)
        {
            return None;
        }
        let current = match current {
            Ok(c) => c,
            Err(()) => {
                self.finish(token, false);
                return None;
            }
        };
        let intent = self.flight.as_ref()?.intent.clone();
        let (patch, keys) = self.merge(&intent, &current);
        let f = self.flight.as_mut()?;
        f.phase = Phase::Saving;
        f.keys = keys;
        Some(Effect::Save { token, patch })
    }
    fn merge(&self, intent: &Intent, current: &Value) -> (Value, Vec<Key>) {
        let mut next = current.clone();
        if !next.is_object() {
            next = json!({});
        }
        let mut keys = Vec::new();
        let mut touched = BTreeSet::new();
        // Explicit selection is resolved before conditional removal fallback (T-R1).
        let edits = intent
            .edits
            .iter()
            .filter(|e| !matches!(e, Edit::RemoveModels { .. }))
            .chain(
                intent
                    .edits
                    .iter()
                    .filter(|e| matches!(e, Edit::RemoveModels { .. })),
            );
        for e in edits {
            let key = e.key();
            if self.stale(&key, intent.revision) {
                continue;
            }
            let provider = key.provider();
            if provider.is_some_and(|p| {
                current["customProviders"].as_array().is_some_and(|a| {
                    a.iter()
                        .any(|v| v["id"] == p && v.get("_extensionId").is_some())
                })
            }) {
                continue;
            }
            match e {
                Edit::Field {
                    provider,
                    field,
                    value,
                } => {
                    ensure_map(&mut next, "providerConfigs");
                    if !next["providerConfigs"][provider].is_object() {
                        next["providerConfigs"][provider] = json!({});
                    }
                    next["providerConfigs"][provider][field] = value.clone();
                    touched.insert("providerConfigs");
                }
                Edit::Models { provider, models } => {
                    ensure_map(&mut next, "providerModels");
                    next["providerModels"][provider] = json!(models);
                    touched.insert("providerModels");
                }
                Edit::Definition { provider, value } => {
                    let mut a = next["customProviders"]
                        .as_array()
                        .cloned()
                        .unwrap_or_default();
                    if let Some(v) = a.iter_mut().find(|v| v["id"] == *provider) {
                        *v = value.clone();
                    } else {
                        a.push(value.clone());
                    }
                    next["customProviders"] = json!(a);
                    touched.insert("customProviders");
                }
                Edit::Select { provider, model } => {
                    set_selection(&mut next, provider, model, &mut touched);
                }
                Edit::Activate { provider, model } => {
                    next["provider"] = json!(provider);
                    next["model"] = json!(model);
                    ensure_map(&mut next, "taskProviderOverrides");
                    ensure_map(&mut next, "taskModelOverrides");
                    next["taskProviderOverrides"]["chat"] = json!(provider);
                    next["taskModelOverrides"]["chat"] = json!(model);
                    touched.extend([
                        "provider",
                        "model",
                        "taskProviderOverrides",
                        "taskModelOverrides",
                    ]);
                }
                Edit::RemoveProvider(p) => {
                    for name in [
                        "providerConfigs",
                        "providerModels",
                        "providerSelectedModels",
                    ] {
                        ensure_map(&mut next, name);
                        next[name].as_object_mut().unwrap().remove(p);
                        touched.insert(name);
                    }
                    let a = next["customProviders"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|v| v["id"] != *p)
                        .cloned()
                        .collect::<Vec<_>>();
                    next["customProviders"] = json!(a);
                    touched.insert("customProviders");
                    let model = next["providerSelectedModels"]["ollama"]
                        .as_str()
                        .map(str::to_owned)
                        .unwrap_or_else(|| recommended("ollama"));
                    if next["provider"] == *p {
                        next["provider"] = json!("ollama");
                        next["model"] = json!(model);
                        touched.extend(["provider", "model"]);
                    }
                    if next["taskProviderOverrides"]["chat"] == *p {
                        ensure_map(&mut next, "taskModelOverrides");
                        next["taskProviderOverrides"]["chat"] = json!("ollama");
                        next["taskModelOverrides"]["chat"] = json!(model);
                        touched.extend(["taskProviderOverrides", "taskModelOverrides"]);
                    }
                }
                Edit::RemoveModels { provider, ids } => {
                    ensure_map(&mut next, "providerModels");
                    let models = next["providerModels"][provider]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter(|m| !ids.contains(m["id"].as_str().unwrap_or("")))
                        .cloned()
                        .collect::<Vec<_>>();
                    next["providerModels"][provider] = json!(models);
                    touched.insert("providerModels");
                    let selected =
                        next["providerSelectedModels"][provider]
                            .as_str()
                            .or_else(|| {
                                if next["provider"] == *provider {
                                    next["model"].as_str()
                                } else {
                                    None
                                }
                            });
                    if selected.is_some_and(|m| ids.contains(m))
                        && !self.stale(&Key::Selection(provider.clone()), intent.revision)
                    {
                        let fallback = models
                            .first()
                            .and_then(|m| m["id"].as_str())
                            .map(str::to_owned)
                            .unwrap_or_else(|| recommended(provider));
                        set_selection(&mut next, provider, &fallback, &mut touched);
                        keys.push(Key::Selection(provider.clone()));
                    }
                }
                Edit::Global { field, value } => {
                    if matches!(
                        field.as_str(),
                        "providerConfigs"
                            | "providerModels"
                            | "customProviders"
                            | "providerSelectedModels"
                            | "provider"
                            | "model"
                            | "taskProviderOverrides"
                            | "taskModelOverrides"
                    ) {
                        continue;
                    }
                    next[field] = value.clone();
                }
            }
            keys.push(key);
        }
        let mut patch = Map::new();
        for name in touched {
            patch.insert(name.into(), next[name].clone());
        }
        for k in &keys {
            if let Key::Global(name) = k {
                patch.insert(name.clone(), next[name].clone());
            }
        }
        (Value::Object(patch), keys)
    }
    /// `ok:false` and transport failures both call this with false. Error payloads are not retained.
    pub fn finish(&mut self, token: u64, success: bool) -> Option<Effect> {
        if !self.flight.as_ref().is_some_and(|f| f.token == token) {
            return None;
        }
        let f = self.flight.take()?;
        if success {
            for k in &f.keys {
                self.saved
                    .entry(k.clone())
                    .and_modify(|r| *r = (*r).max(f.intent.revision))
                    .or_insert(f.intent.revision);
                if let Key::Remove(p) = k {
                    self.removed
                        .entry(p.clone())
                        .and_modify(|r| *r = (*r).max(f.intent.revision))
                        .or_insert(f.intent.revision);
                }
            }
            let saved = &self.saved;
            let removed = &self.removed;
            self.failures.retain(|k, i| {
                saved.get(k).copied().unwrap_or(0) < i.revision
                    && !k
                        .provider()
                        .is_some_and(|p| removed.get(p).copied().unwrap_or(0) >= i.revision)
            });
            if self.failures.is_empty() {
                self.message = None;
            }
            self.collect_owners();
            if f.intent.sign_in {
                return Some(Effect::StartDeviceSignIn { token });
            }
        } else if !f.intent.optional {
            for e in &f.intent.edits {
                let k = e.key();
                if self.stale(&k, f.intent.revision) {
                    continue;
                }
                if self
                    .failures
                    .get(&k)
                    .is_none_or(|i| i.revision <= f.intent.revision)
                {
                    self.failures.insert(k, f.intent.clone());
                }
            }
            self.message = Some(CLOSE_ERROR);
        }
        None
    }
    pub fn retry(&mut self, retired_only: bool) {
        let mut seen = BTreeSet::new();
        for i in self.failures.values() {
            if (!retired_only || self.owners.get(&i.owner) == Some(&true))
                && seen.insert(i.id)
                && !self.pending.iter().any(|p| p.id == i.id)
                && !self.flight.as_ref().is_some_and(|f| f.intent.id == i.id)
            {
                self.pending.push_back(i.clone());
            }
        }
    }
    pub fn failure_count(&self) -> usize {
        self.failures
            .values()
            .map(|i| i.id)
            .collect::<BTreeSet<_>>()
            .len()
    }
    fn collect_owners(&mut self) {
        let busy: BTreeSet<_> = self
            .pending
            .iter()
            .chain(self.failures.values())
            .map(|i| i.owner)
            .chain(self.flight.iter().map(|f| f.intent.owner))
            .collect();
        self.owners
            .retain(|id, retired| !*retired || busy.contains(id));
    }
    pub fn request_close(&mut self, request: u64, now: u64, acknowledge: bool) -> Option<Effect> {
        self.close = Some((request, now, acknowledge));
        acknowledge.then_some(Effect::AcknowledgeClose { request })
    }
    pub fn close_status(&mut self, now: u64) -> Option<Effect> {
        let (request, started, ack) = self.close?;
        if !ack {
            if now.saturating_sub(started) < 3000 {
                return None;
            }
        } else {
            if self.flight.is_some() || !self.pending.is_empty() {
                return None;
            }
            if !self.failures.is_empty() {
                return Some(Effect::KeepOpen {
                    request,
                    message: CLOSE_ERROR,
                });
            }
        }
        self.close = None;
        Some(Effect::Close { request })
    }
}
fn ensure_map(v: &mut Value, key: &str) {
    if !v[key].is_object() {
        v[key] = json!({});
    }
}
fn set_selection(v: &mut Value, p: &str, m: &str, touched: &mut BTreeSet<&'static str>) {
    ensure_map(v, "providerSelectedModels");
    v["providerSelectedModels"][p] = json!(m);
    touched.insert("providerSelectedModels");
    if v["provider"] == p {
        v["model"] = json!(m);
        touched.insert("model");
    }
    let chat = v["taskProviderOverrides"]["chat"]
        .as_str()
        .or_else(|| v["provider"].as_str());
    if chat == Some(p) {
        ensure_map(v, "taskModelOverrides");
        v["taskModelOverrides"]["chat"] = json!(m);
        touched.insert("taskModelOverrides");
    }
}
/// Hydration never writes. Dirty fields stay with their owner across external notifications.
#[derive(Default)]
pub struct Draft {
    pub owner: OwnerId,
    fields: BTreeMap<Key, Value>,
    dirty: BTreeSet<Key>,
    load: u64,
}
impl Draft {
    pub fn read(&mut self) -> u64 {
        self.load += 1;
        self.load
    }
    pub fn hydrate(&mut self, token: u64, fields: BTreeMap<Key, Value>) {
        if token == self.load {
            for (k, v) in fields {
                if !self.dirty.contains(&k) {
                    self.fields.insert(k, v);
                }
            }
        }
    }
    pub fn edit(&mut self, key: Key, value: Value) {
        self.load += 1;
        self.dirty.insert(key.clone());
        self.fields.insert(key, value);
    }
    pub fn value(&self, key: &Key) -> Option<&Value> {
        self.fields.get(key)
    }
    pub fn is_dirty(&self) -> bool {
        !self.dirty.is_empty()
    }
}
#[cfg(test)]
mod tests;

/// One selected provider's editable records, isolated from legacy fields and extension contributions.
#[derive(Default)]
pub struct ProviderDraft {
    pub provider: String,
    config: Map<String, Value>,
    pub models: Vec<Value>,
    pub selected_model: String,
    read_only: bool,
    generation: u64,
}
impl ProviderDraft {
    pub fn load(&mut self, provider: String, current: &Value) {
        self.generation += 1;
        self.read_only = current["customProviders"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|p| p["id"] == provider && p.get("_extensionId").is_some());
        let records = current["providerConfigs"].as_object();
        let legacy = !self.read_only && records.is_none_or(|r| !r.values().any(Value::is_object));
        self.config = records
            .and_then(|r| r.get(&provider))
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_else(|| {
                if legacy && current["provider"] == provider {
                    current.as_object().cloned().unwrap_or_default()
                } else {
                    Map::new()
                }
            });
        self.models = current["providerModels"][&provider]
            .as_array()
            .cloned()
            .unwrap_or_else(|| {
                provider_catalog()["DEFAULT_PROVIDER_MODELS"][&provider]
                    .as_array()
                    .cloned()
                    .unwrap_or_default()
            });
        self.selected_model = current["providerSelectedModels"][&provider]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| recommended(&provider));
        self.provider = provider;
    }
    pub fn field(&self, key: &str) -> Option<&Value> {
        self.config.get(key)
    }
    pub fn edit(&mut self, key: String, value: Value) -> Result<Edit, &'static str> {
        if self.read_only {
            return Err("Provider is managed by its extension");
        }
        self.generation += 1;
        self.config.insert(key.clone(), value.clone());
        Ok(Edit::Field {
            provider: self.provider.clone(),
            field: key,
            value,
        })
    }
    pub fn discovery_token(&self) -> u64 {
        self.generation
    }
    /// Discovery owns models only, preserving protocol, token limits, selection and manual entries.
    pub fn discovered(&mut self, token: u64, ids: Vec<String>) -> Option<Edit> {
        if token != self.generation || self.read_only {
            return None;
        }
        let mut merged = Vec::new();
        let mut seen = BTreeSet::new();
        for id in ids {
            if !seen.insert(id.clone()) {
                continue;
            }
            merged.push(
                self.models
                    .iter()
                    .find(|m| m["id"] == id)
                    .cloned()
                    .unwrap_or_else(|| json!({"id":id})),
            );
        }
        for m in &self.models {
            if let Some(id) = m["id"].as_str() {
                if seen.insert(id.into()) {
                    merged.push(m.clone());
                }
            }
        }
        self.models = merged;
        Some(Edit::Models {
            provider: self.provider.clone(),
            models: self.models.clone(),
        })
    }
    pub fn remove_models(&mut self, ids: BTreeSet<String>) -> Edit {
        self.generation += 1;
        self.models
            .retain(|m| !ids.contains(m["id"].as_str().unwrap_or("")));
        Edit::RemoveModels {
            provider: self.provider.clone(),
            ids,
        }
    }
    pub fn select_model(&mut self, id: String) -> Edit {
        self.generation += 1;
        self.selected_model = id.clone();
        Edit::Select {
            provider: self.provider.clone(),
            model: id,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DevicePhase {
    Idle,
    Saving,
    Starting,
    Waiting {
        code: String,
        verification_uri: String,
    },
    Polling,
    Complete,
    Failed,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeviceEffect {
    Start {
        token: u64,
        client_id: Option<String>,
    },
    Poll {
        token: u64,
    },
    Cancel,
}
pub struct DeviceSignIn {
    pub phase: DevicePhase,
    token: u64,
    poll_at: u64,
    interval: u64,
    polls: u16,
}
impl Default for DeviceSignIn {
    fn default() -> Self {
        Self {
            phase: DevicePhase::Idle,
            token: 0,
            poll_at: 0,
            interval: 1000,
            polls: 0,
        }
    }
}
impl DeviceSignIn {
    pub fn saving(&mut self) -> u64 {
        self.token += 1;
        self.phase = DevicePhase::Saving;
        self.polls = 0;
        self.token
    }
    pub fn saved(
        &mut self,
        token: u64,
        ok: bool,
        client_id: Option<String>,
    ) -> Option<DeviceEffect> {
        if self.token != token || self.phase != DevicePhase::Saving {
            return None;
        }
        self.phase = if ok {
            DevicePhase::Starting
        } else {
            DevicePhase::Failed
        };
        ok.then_some(DeviceEffect::Start { token, client_id })
    }
    pub fn started(
        &mut self,
        token: u64,
        code: String,
        verification_uri: String,
        interval_seconds: u64,
        now: u64,
    ) -> Result<(), &'static str> {
        if self.token != token || self.phase != DevicePhase::Starting {
            return Err("Obsolete device sign-in");
        }
        let url = url::Url::parse(&verification_uri).map_err(|_| "Invalid verification URI")?;
        if url.scheme() != "https" {
            return Err("Verification URI must be HTTPS");
        }
        self.interval = interval_seconds.saturating_mul(1000).max(1000);
        self.poll_at = now + self.interval;
        self.phase = DevicePhase::Waiting {
            code,
            verification_uri,
        };
        Ok(())
    }
    pub fn tick(&mut self, now: u64) -> Option<DeviceEffect> {
        if !matches!(self.phase, DevicePhase::Waiting { .. }) || now < self.poll_at {
            return None;
        }
        if self.polls >= 120 {
            self.phase = DevicePhase::Failed;
            return None;
        }
        self.polls += 1;
        self.phase = DevicePhase::Polling;
        Some(DeviceEffect::Poll { token: self.token })
    }
    pub fn polled(&mut self, token: u64, status: &str, code: String, uri: String, now: u64) {
        if self.token != token || self.phase != DevicePhase::Polling {
            return;
        }
        self.phase = match status {
            "success" => DevicePhase::Complete,
            "error" => DevicePhase::Failed,
            _ => {
                self.poll_at = now + self.interval + if status == "slow_down" { 5000 } else { 0 };
                DevicePhase::Waiting {
                    code,
                    verification_uri: uri,
                }
            }
        };
    }
    pub fn cancel(&mut self) -> DeviceEffect {
        self.token += 1;
        self.phase = DevicePhase::Cancelled;
        DeviceEffect::Cancel
    }
}
pub mod contributions;
pub mod providers;
