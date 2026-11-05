//! Knowledge request and mutation ownership. Time is supplied in milliseconds by the caller.
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Variant {
    #[serde(rename = "q8_0")]
    Standard,
    #[serde(rename = "q4_k_m")]
    Compact,
}
impl Variant {
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard => "Standard (Q8_0)",
            Self::Compact => "Compact (Q4_K_M)",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Standard => "q8_0",
            Self::Compact => "q4_k_m",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FileState {
    NotDownloaded,
    Downloading,
    Verifying,
    Ready,
    Error,
}
impl Default for FileState {
    fn default() -> Self {
        Self::NotDownloaded
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FailureStage {
    Download,
    Integrity,
    Startup,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Failure {
    pub stage: FailureStage,
    pub message: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub id: Variant,
    #[serde(default)]
    pub weight_bytes: u64,
    pub state: FileState,
    #[serde(default)]
    pub download_progress: f64,
    #[serde(default)]
    pub download_bytes: u64,
    #[serde(default)]
    pub download_total_bytes: u64,
    pub failure: Option<Failure>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SemanticStatus {
    pub supported: bool,
    pub model_state: FileState,
    pub weight_bytes: Option<u64>,
    pub variants: Option<Vec<ModelFile>>,
    pub error: Option<String>,
    pub error_source: Option<String>,
    pub integrity_state: Option<String>,
    pub download_progress: f64,
    pub download_bytes: u64,
    pub download_total_bytes: u64,
    pub index_variant: Option<Variant>,
    pub indexed_chunks: u64,
    pub total_chunks: u64,
    pub indexing: bool,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}
impl SemanticStatus {
    pub fn files(&self) -> Vec<ModelFile> {
        [Variant::Standard, Variant::Compact]
            .into_iter()
            .map(|id| {
                if let Some(file) = self
                    .variants
                    .as_ref()
                    .and_then(|a| a.iter().find(|v| v.id == id))
                {
                    return file.clone();
                }
                let legacy = self.variants.is_none() && id == Variant::Standard;
                let failure = if legacy {
                    self.error.as_ref().and_then(|m| {
                        let stage = match self.error_source.as_deref() {
                            Some("integrity") => FailureStage::Integrity,
                            Some("runtime") if self.model_state == FileState::Ready => {
                                FailureStage::Startup
                            }
                            Some("download") => FailureStage::Download,
                            None if self.model_state == FileState::Error => FailureStage::Download,
                            _ => return None,
                        };
                        Some(Failure {
                            stage,
                            message: m.clone(),
                        })
                    })
                } else {
                    None
                };
                let state = if legacy {
                    if self.integrity_state.as_deref() == Some("verifying")
                        && self.model_state != FileState::Downloading
                    {
                        FileState::Verifying
                    } else if failure
                        .as_ref()
                        .is_some_and(|f| f.stage == FailureStage::Integrity)
                        && self.model_state != FileState::Downloading
                    {
                        FileState::Error
                    } else {
                        self.model_state
                    }
                } else {
                    FileState::NotDownloaded
                };
                ModelFile {
                    id,
                    weight_bytes: if legacy {
                        self.weight_bytes.unwrap_or(309_855_456)
                    } else if self.variants.is_none() {
                        181_695_104
                    } else {
                        0
                    },
                    state,
                    download_progress: if legacy { self.download_progress } else { 0.0 },
                    download_bytes: if legacy { self.download_bytes } else { 0 },
                    download_total_bytes: if legacy { self.download_total_bytes } else { 0 },
                    failure,
                }
            })
            .collect()
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub settings: Settings,
    pub roots: Vec<Value>,
    pub status: Value,
    #[serde(default)]
    pub semantic_search: SemanticStatus,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub depth: String,
    pub embedding_model: String,
    pub embedding_variant: Option<Variant>,
    pub indexing_speed: String,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, Value>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Operation {
    Switch(Option<Variant>),
    Download(Variant),
    Cancel(Variant),
    Remove(Variant),
    Speed(String),
    Depth(String),
    RootDepth { id: String, depth: String },
    RootEnabled { id: String, enabled: bool },
    AddRoot(String),
    AddMajor,
    RemoveRoot(String),
    Pause,
    Resume,
    Start,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Read { token: u64 },
    Mutate { token: u64, operation: Operation },
    Focus { target: String },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Pending {
        token: u64,
        trigger: String,
        operation: Operation,
        deadline: u64,
    },
    Reconciling,
    Unconfirmed,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Button {
    pub trigger: String,
    pub label: String,
    pub inert: bool,
    pub focusable: bool,
    pub destructive: bool,
    pub accessible_name: String,
}
pub struct KnowledgeSettings {
    pub snapshot: Option<Snapshot>,
    pub loading: bool,
    pub error: Option<String>,
    pub local_error: Option<String>,
    pub phase: Phase,
    active: bool,
    visible: bool,
    next: u64,
    read: Option<(u64, u64)>,
    failed: bool,
    poll_at: u64,
    read_seq: u64,
    barrier: Option<u64>,
    lock_trigger: Option<String>,
}
impl Default for KnowledgeSettings {
    fn default() -> Self {
        Self {
            snapshot: None,
            loading: false,
            error: None,
            local_error: None,
            phase: Phase::Idle,
            active: false,
            visible: true,
            next: 0,
            read: None,
            failed: false,
            poll_at: 0,
            read_seq: 0,
            barrier: None,
            lock_trigger: None,
        }
    }
}
impl KnowledgeSettings {
    pub fn activate(&mut self, active: bool, now: u64) -> Vec<Effect> {
        if self.active == active {
            return vec![];
        }
        self.active = active;
        if active {
            self.refresh(now, false)
        } else {
            self.next += 1;
            self.read = None;
            self.loading = false;
            // Mutation/reconciliation ownership survives tab deactivation.
            vec![]
        }
    }
    pub fn refresh(&mut self, now: u64, background: bool) -> Vec<Effect> {
        if !self.active {
            return vec![];
        }
        self.next += 1;
        self.read = Some((self.next, now + 10_000));
        self.loading = !background;
        self.error = None;
        self.failed = false;
        vec![Effect::Read { token: self.next }]
    }
    pub fn visible(&mut self, visible: bool, now: u64) -> Vec<Effect> {
        let changed = self.visible != visible;
        self.visible = visible;
        if changed && visible && self.active && !self.failed {
            self.refresh(now, self.snapshot.is_some())
        } else {
            vec![]
        }
    }
    pub fn accepts_read(&self, token: u64) -> bool {
        self.active && self.read.is_some_and(|r| r.0 == token)
    }
    pub fn read_result(&mut self, token: u64, result: Result<Snapshot, String>, now: u64) {
        if !self.accepts_read(token) {
            return;
        }
        self.read = None;
        self.loading = false;
        self.poll_at = now + 1500;
        match result {
            Ok(s) => {
                self.snapshot = Some(s);
                self.failed = false;
                self.error = None;
                self.read_seq += 1;
                if self.barrier.is_some_and(|seq| self.read_seq > seq)
                    && matches!(self.phase, Phase::Reconciling | Phase::Unconfirmed)
                {
                    self.unlock();
                }
            }
            Err(e) => {
                self.failed = true;
                self.error = Some(e);
                if self.barrier.is_some() && !matches!(self.phase, Phase::Pending { .. }) {
                    self.phase = Phase::Unconfirmed;
                }
            }
        }
    }
    fn unlock(&mut self) {
        self.phase = Phase::Idle;
        self.barrier = None;
        self.lock_trigger = None;
    }
    pub fn begin(&mut self, operation: Operation, trigger: String, now: u64) -> Vec<Effect> {
        if !self.active || self.snapshot.is_none() {
            return vec![];
        }
        let cancel = matches!(operation, Operation::Cancel(_));
        if self.phase != Phase::Idle && !cancel {
            return vec![];
        }
        if let Phase::Pending {
            operation: old,
            trigger: old_trigger,
            ..
        } = &self.phase
        {
            if *old_trigger == trigger {
                return vec![];
            }
            if !matches!(old, Operation::Cancel(_)) {
                self.barrier = Some(self.read_seq);
            }
        }
        self.next += 1;
        self.local_error = None;
        self.lock_trigger = Some(trigger.clone());
        self.phase = Phase::Pending {
            token: self.next,
            trigger,
            operation: operation.clone(),
            deadline: now + 15_000,
        };
        vec![Effect::Mutate {
            token: self.next,
            operation,
        }]
    }
    pub fn mutation_result(
        &mut self,
        token: u64,
        result: Result<Snapshot, String>,
        now: u64,
    ) -> Vec<Effect> {
        if !matches!(self.phase,Phase::Pending{token:t,..}if t==token) {
            return vec![];
        }
        let operation = match &self.phase {
            Phase::Pending { operation, .. } => operation.clone(),
            _ => return vec![],
        };
        match result {
            Ok(s) => {
                self.read = None;
                self.loading = false;
                self.failed = false;
                self.error = None;
                self.snapshot = Some(s);
            }
            Err(e) => {
                self.local_error = Some(match operation {
                    Operation::Download(v) => format!("{} download failed: {e}", v.name()),
                    Operation::Switch(v) => format!(
                        "Couldn't switch to {}: {e}",
                        v.map_or("Built-in lightweight search", Variant::name)
                    ),
                    _ => e,
                });
            }
        }
        if self.barrier.is_some() {
            self.barrier = Some(self.read_seq);
            self.phase = Phase::Reconciling;
            self.refresh(now, false)
        } else {
            self.unlock();
            vec![]
        }
    }
    pub fn tick(&mut self, now: u64) -> Vec<Effect> {
        if !self.active {
            return vec![];
        }
        if self.read.is_some_and(|(_, d)| now >= d) {
            self.read = None;
            self.loading = false;
            self.failed = true;
            self.error =
                Some("Loading Knowledge settings timed out. Try Refresh status or Retry.".into());
            if self.barrier.is_some() && !matches!(self.phase, Phase::Pending { .. }) {
                self.phase = Phase::Unconfirmed;
            }
        }
        if matches!(self.phase,Phase::Pending{deadline,..}if now>=deadline) {
            self.barrier = Some(self.read_seq);
            self.phase = Phase::Reconciling;
            return self.refresh(now, false);
        }
        if self.visible
            && self.snapshot.is_some()
            && !self.failed
            && self.read.is_none()
            && now >= self.poll_at
        {
            return self.refresh(now, true);
        }
        vec![]
    }
    /// Status events update counters only; they cannot reconcile an unknown mutation.
    pub fn status_event(&mut self, status: Value) {
        if self.active {
            if let Some(s) = &mut self.snapshot {
                s.status = status;
            }
        }
    }
    pub fn files(&self) -> Vec<ModelFile> {
        self.snapshot
            .as_ref()
            .map(|s| s.semantic_search.files())
            .unwrap_or_default()
    }
    pub fn selected(&self) -> String {
        let Some(s) = &self.snapshot else {
            return String::new();
        };
        if let Phase::Pending {
            operation: Operation::Switch(v),
            ..
        } = &self.phase
        {
            return v.map_or_else(
                || "built-in".into(),
                |v| format!("embeddinggemma-2:{}", v.id()),
            );
        }
        if s.settings.embedding_model == "built-in" {
            "built-in".into()
        } else {
            format!(
                "embeddinggemma-2:{}",
                s.settings
                    .embedding_variant
                    .unwrap_or(Variant::Standard)
                    .id()
            )
        }
    }
    pub fn enable_variant(&self) -> Option<Variant> {
        let s = self.snapshot.as_ref()?;
        if !s.semantic_search.supported {
            return None;
        }
        self.files()
            .iter()
            .find(|v| v.state == FileState::Ready && v.failure.is_none())
            .map(|v| v.id)
    }
    pub fn buttons(&self, id: Variant) -> Vec<Button> {
        let Some(s) = &self.snapshot else {
            return vec![];
        };
        if !s.semantic_search.supported {
            return vec![];
        }
        let file = self.files().into_iter().find(|f| f.id == id).unwrap();
        let startup = file
            .failure
            .as_ref()
            .is_some_and(|f| f.stage == FailureStage::Startup);
        let mut actions: Vec<(&str, &str, bool)> = match file.state {
            FileState::Verifying => vec![],
            FileState::Downloading => vec![("cancel", "Cancel", false)],
            FileState::Ready if !startup => vec![("remove", "Remove", true)],
            _ if startup => vec![
                ("start", "Retry start", false),
                ("download", "Repair", false),
            ],
            FileState::Error => vec![(
                "download",
                if file
                    .failure
                    .as_ref()
                    .is_some_and(|f| f.stage == FailureStage::Integrity)
                {
                    "Repair"
                } else {
                    "Retry download"
                },
                false,
            )],
            _ => vec![("download", "Download", false)],
        };
        if file.state == FileState::Error
            && (file.download_bytes > 0
                || file
                    .failure
                    .as_ref()
                    .is_some_and(|f| f.stage == FailureStage::Integrity))
        {
            actions.push(("remove", "Remove", true));
        }
        actions
            .into_iter()
            .map(|(kind, label, destructive)| {
                let trigger = format!("{kind}:{}", id.id());
                let own = self.lock_trigger.as_deref() == Some(&trigger);
                let pending = matches!(&self.phase,Phase::Pending{trigger:t,..}if *t==trigger);
                let inert = self.phase != Phase::Idle && (kind != "cancel" || pending);
                let accessible_name = if pending {
                    format!(
                        "{} {}…",
                        match kind {
                            "start" | "download" => "Starting",
                            "remove" => "Removing",
                            "cancel" => "Cancelling",
                            _ => label,
                        },
                        id.name()
                    )
                } else if kind == "cancel" {
                    format!("Cancel {} download", id.name())
                } else {
                    format!("{label} {}", id.name())
                };
                let label = if pending {
                    match kind {
                        "start" => "Starting…",
                        "download" => "Starting…",
                        "remove" => "Removing…",
                        "cancel" => "Cancelling…",
                        _ => label,
                    }
                } else {
                    label
                };
                Button {
                    trigger,
                    label: label.into(),
                    inert,
                    focusable: own || !inert,
                    destructive,
                    accessible_name,
                }
            })
            .collect()
    }
    /// Stable row focus survives Download→Cancel, but never moves onto Remove.
    pub fn recover_focus(&self, id: Variant, old_trigger: &str) -> Effect {
        let buttons = self.buttons(id);
        let target = buttons
            .iter()
            .find(|b| b.trigger == old_trigger && !b.destructive)
            .or_else(|| buttons.iter().find(|b| b.focusable && !b.destructive))
            .map(|b| b.trigger.clone())
            .unwrap_or_else(|| format!("row:{}", id.id()));
        Effect::Focus { target }
    }
    pub fn alerts(&self) -> Vec<String> {
        let mut out = vec![];
        if let Some(e) = &self.local_error {
            out.push(e.clone());
        }
        let Some(s) = &self.snapshot else {
            return out;
        };
        let files = self.files();
        for file in &files {
            if let Some(f) = &file.failure {
                let name = file.id.name();
                let text = match f.stage {
                    FailureStage::Integrity => {
                        format!("{name} didn't pass the file check. Repair downloads it again.")
                    }
                    FailureStage::Startup => {
                        format!("{name} is installed but couldn't start: {}", f.message)
                    }
                    FailureStage::Download => format!("{name} download failed: {}", f.message),
                };
                if !out.contains(&text) {
                    out.push(text);
                }
            }
        }
        if let Some(e) = &s.semantic_search.error {
            if s.semantic_search.error_source.as_deref() != Some("integrity")
                && !files
                    .iter()
                    .any(|f| f.failure.as_ref().is_some_and(|f| f.message == *e))
                && !out.contains(e)
            {
                out.push(e.clone());
            }
        }
        out
    }
    /// Transition text is separate from rapidly changing counts for live-region adapters.
    pub fn semantic_summary(&self) -> Option<(String, Option<(u64, u64)>)> {
        let s = self.snapshot.as_ref()?;
        if s.settings.embedding_model == "built-in" {
            return None;
        }
        let v = s.settings.embedding_variant.unwrap_or(Variant::Standard);
        let file = self.files().into_iter().find(|f| f.id == v)?;
        let name = v.name();
        let semantic = &s.semantic_search;
        if file.state == FileState::Verifying {
            return Some((format!("Checking {name} installation…"), None));
        }
        if file
            .failure
            .as_ref()
            .is_some_and(|f| f.stage == FailureStage::Startup)
        {
            return Some((
                format!("Semantic search is off: {name} couldn't start."),
                None,
            ));
        }
        if file.state != FileState::Ready || file.failure.is_some() {
            return Some((
                format!(
                    "Semantic search is off: {name} {}",
                    if file.state == FileState::Error || file.failure.is_some() {
                        "needs repair."
                    } else {
                        "isn't installed."
                    }
                ),
                None,
            ));
        }
        if semantic.error.as_ref().is_some_and(|error| {
            !self.files().iter().any(|f| {
                f.id != v
                    && f.failure
                        .as_ref()
                        .is_some_and(|failure| &failure.message == error)
            })
        }) {
            return Some((
                format!("Semantic search is off: {name} needs repair."),
                None,
            ));
        }
        let count_variant = semantic
            .index_variant
            .or_else(|| semantic.variants.is_none().then_some(Variant::Standard));
        if count_variant != Some(v) {
            return Some((format!("Preparing semantic index for {name}…"), None));
        }
        let line = if s.status["state"] == "paused" {
            "Semantic indexing paused"
        } else if semantic.indexing {
            "Building semantic index"
        } else if semantic.indexed_chunks < semantic.total_chunks {
            "Preparing semantic index"
        } else {
            "Semantic index"
        };
        Some((
            format!("{line} · {name}"),
            Some((semantic.indexed_chunks, semantic.total_chunks)),
        ))
    }
}
pub fn download_percent(progress: f64) -> u8 {
    if !progress.is_finite() {
        0
    } else {
        (progress * 100.0).floor().clamp(0.0, 99.0) as u8
    }
}
#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq)]
pub struct CoreRequest {
    pub channel: &'static str,
    pub args: Vec<Value>,
}
impl Operation {
    pub fn request(&self) -> CoreRequest {
        use serde_json::json;
        let (channel, args) = match self {
            Self::Switch(v) => (
                "knowledge:embedding:set",
                v.map_or(vec![json!("built-in")], |v| {
                    vec![json!("embeddinggemma-2"), json!(v.id())]
                }),
            ),
            Self::Download(v) => ("knowledge:embedding:download", vec![json!(v.id())]),
            Self::Cancel(v) => ("knowledge:embedding:cancel-download", vec![json!(v.id())]),
            Self::Remove(v) => ("knowledge:embedding:remove", vec![json!(v.id())]),
            Self::Speed(speed) => ("knowledge:speed:set", vec![json!(speed)]),
            Self::Depth(depth) => ("knowledge:depth:set", vec![json!(depth)]),
            Self::RootDepth { id, depth } => (
                "knowledge:root:set-depth",
                vec![json!({"rootId":id,"depth":depth})],
            ),
            Self::RootEnabled { id, enabled } => (
                "knowledge:root:set-enabled",
                vec![json!({"rootId":id,"enabled":enabled})],
            ),
            Self::AddRoot(path) => ("knowledge:root:add", vec![json!(path)]),
            Self::AddMajor => ("knowledge:roots:add-major", vec![]),
            Self::RemoveRoot(id) => ("knowledge:root:remove", vec![json!(id)]),
            Self::Pause => ("knowledge:index:pause", vec![]),
            Self::Resume => ("knowledge:index:resume", vec![]),
            Self::Start => ("knowledge:index:start", vec![]),
        };
        CoreRequest { channel, args }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub variant: Variant,
    pub fraction: Option<f64>,
    pub percent: Option<u8>,
    pub bytes: Option<(u64, u64)>,
}
impl KnowledgeSettings {
    pub fn progress(&self) -> Vec<Progress> {
        self.files()
            .into_iter()
            .filter_map(|v| match v.state {
                FileState::Verifying => Some(Progress {
                    variant: v.id,
                    fraction: None,
                    percent: None,
                    bytes: None,
                }),
                FileState::Downloading => Some(Progress {
                    variant: v.id,
                    fraction: Some(v.download_progress.clamp(0.0, 0.99)),
                    percent: Some(download_percent(v.download_progress)),
                    bytes: (v.download_total_bytes > 0)
                        .then_some((v.download_bytes, v.download_total_bytes)),
                }),
                _ => None,
            })
            .collect()
    }
    pub fn control(&self, trigger: &str) -> Button {
        let own = self.lock_trigger.as_deref() == Some(trigger);
        let inert = self.phase != Phase::Idle;
        Button {
            trigger: trigger.into(),
            label: trigger.into(),
            inert: inert || self.snapshot.is_none(),
            focusable: self.snapshot.is_some() && (!inert || own),
            destructive: false,
            accessible_name: trigger.into(),
        }
    }
    pub fn recover_owned_focus(
        &self,
        id: Option<Variant>,
        old_trigger: &str,
        still_owned: bool,
    ) -> Option<Effect> {
        if !still_owned {
            return None;
        }
        Some(if let Some(id) = id {
            self.recover_focus(id, old_trigger)
        } else {
            Effect::Focus {
                target: "refresh".into(),
            }
        })
    }
    pub fn file_label(&self, file: &ModelFile) -> String {
        if self
            .snapshot
            .as_ref()
            .is_some_and(|s| !s.semantic_search.supported)
        {
            return "Not available on this platform".into();
        }
        match file.state {
            FileState::NotDownloaded => "Not downloaded".into(),
            FileState::Downloading => format!(
                "Downloading · {}%",
                download_percent(file.download_progress)
            ),
            FileState::Verifying => "Checking file…".into(),
            FileState::Error => if file
                .failure
                .as_ref()
                .is_some_and(|f| f.stage == FailureStage::Integrity)
            {
                "File check failed"
            } else {
                "Download failed"
            }
            .into(),
            FileState::Ready => if file
                .failure
                .as_ref()
                .is_some_and(|f| f.stage == FailureStage::Startup)
            {
                "Installed · couldn't start"
            } else if self.snapshot.as_ref().is_some_and(|s| {
                s.settings.embedding_model == "embeddinggemma-2"
                    && s.settings.embedding_variant.unwrap_or(Variant::Standard) == file.id
            }) {
                "Installed · in use"
            } else {
                "Installed"
            }
            .into(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EmbeddingAction {
    Download,
    Cancel,
    Remove,
}
/// Legacy IPC callers omit the variant; explicit callers keep variant in arg0.
pub fn embedding_request(action: EmbeddingAction, variant: Option<Variant>) -> CoreRequest {
    CoreRequest {
        channel: match action {
            EmbeddingAction::Download => "knowledge:embedding:download",
            EmbeddingAction::Cancel => "knowledge:embedding:cancel-download",
            EmbeddingAction::Remove => "knowledge:embedding:remove",
        },
        args: variant
            .map(|v| vec![serde_json::json!(v.id())])
            .unwrap_or_default(),
    }
}
