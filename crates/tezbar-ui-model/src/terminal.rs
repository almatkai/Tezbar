//! Session palette and durable replay policy. Never creates a PTY.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SaveFor {
    Day,
    Week,
    Month,
    Forever,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeepAliveFor {
    #[serde(rename = "3h")]
    ThreeHours,
    #[serde(rename = "8h")]
    EightHours,
    #[serde(rename = "day")]
    Day,
    #[serde(rename = "until-stop")]
    UntilStop,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Defaults {
    #[serde(default = "default_save_for")]
    pub save_for: SaveFor,
    #[serde(default = "default_keep_alive_for")]
    pub keep_alive_for: KeepAliveFor,
}
impl Default for Defaults {
    fn default() -> Self {
        Self {
            save_for: SaveFor::Week,
            keep_alive_for: KeepAliveFor::ThreeHours,
        }
    }
}
impl Defaults {
    pub fn from_legacy(value: &serde_json::Value) -> Self {
        Self {
            save_for: serde_json::from_value(value["saveFor"].clone()).unwrap_or(SaveFor::Week),
            keep_alive_for: serde_json::from_value(value["keepAliveFor"].clone())
                .unwrap_or(KeepAliveFor::ThreeHours),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub session_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub shell: String,
    pub initial_command: Option<String>,
    pub last_command: Option<String>,
    #[serde(default = "default_save_for")]
    pub save_for: SaveFor,
    #[serde(default = "default_keep_alive_for")]
    pub keep_alive_for: KeepAliveFor,
    #[serde(
        rename = "status",
        serialize_with = "serialize_status",
        deserialize_with = "deserialize_status"
    )]
    pub running: bool,
    #[serde(flatten)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Attach(String),
    Detach(String),
    Restore {
        session: Session,
        restore_command: Option<String>,
    },
    Create {
        cwd: Option<String>,
        command: Option<String>,
        defaults: Defaults,
    },
    Delete(String),
    Kill(String),
    PersistPins(Vec<String>),
    PersistDefaults(Defaults),
}
#[derive(Default)]
pub struct Sessions {
    pub rows: Vec<Session>,
    pub selected: Option<usize>,
    pub pins: BTreeSet<String>,
    pub defaults: Defaults,
    attached: Option<String>,
    generation: u64,
}
impl Sessions {
    pub fn replace(&mut self, rows: Vec<Session>) {
        let id = self
            .selected
            .and_then(|i| self.rows.get(i))
            .map(|r| r.session_id.clone());
        self.rows = rows;
        self.selected = id.and_then(|id| self.rows.iter().position(|s| s.session_id == id));
    }
    pub fn down(&mut self) {
        self.selected = if self.rows.is_empty() {
            None
        } else {
            Some(
                self.selected
                    .filter(|i| *i < self.rows.len())
                    .map_or(0, |i| (i + 1).min(self.rows.len() - 1)),
            )
        };
    }
    pub fn activate(&mut self, index: usize) -> Option<Effect> {
        let s = self.rows.get(index)?.clone();
        self.selected = Some(index);
        Some(if s.running {
            Effect::Attach(s.session_id)
        } else {
            let restore_command = s.last_command.clone().or_else(|| s.initial_command.clone());
            Effect::Restore {
                session: s,
                restore_command,
            }
        })
    }
    pub fn attach(&mut self, id: String) -> (u64, Option<Effect>) {
        self.generation += 1;
        let old = self.attached.replace(id);
        (self.generation, old.map(Effect::Detach))
    }
    pub fn accepts_data(&self, token: u64, id: &str) -> bool {
        self.generation == token && self.attached.as_deref() == Some(id)
    }
    pub fn detach(&mut self) -> Option<Effect> {
        self.generation += 1;
        self.attached.take().map(Effect::Detach)
    }
    pub fn toggle_pin(&mut self, id: String) -> Effect {
        if !self.pins.remove(&id) {
            self.pins.insert(id);
        }
        Effect::PersistPins(self.pins.iter().cloned().collect())
    }
    pub fn set_defaults(&mut self, defaults: Defaults) -> Effect {
        self.defaults = defaults.clone();
        Effect::PersistDefaults(defaults)
    }
}
pub fn shortcut_index(
    code: &str,
    key: &str,
    command: bool,
    control: bool,
    alt: bool,
    shift: bool,
) -> Option<usize> {
    if !(command || control) || alt || shift {
        return None;
    }
    let digit = code
        .strip_prefix("Digit")
        .or_else(|| code.strip_prefix("Numpad"))
        .filter(|s| s.len() == 1)
        .unwrap_or(key);
    match digit.as_bytes() {
        [d @ b'1'..=b'9'] => Some((d - b'1') as usize),
        _ => None,
    }
}
pub fn normalize_command(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}
pub fn directory_label(cwd: &str) -> String {
    let normalized = cwd.replace('\\', "/");
    let p: Vec<_> = normalized.split('/').filter(|p| !p.is_empty()).collect();
    match p.as_slice() {
        [] => "/".into(),
        ["Users", _] => "~".into(),
        ["Users", _, leaf] => format!("~/{leaf}"),
        [drive, "Users", _, rest @ ..] if drive.ends_with(':') && !rest.is_empty() => {
            format!("~/{}", rest.join("/"))
        }
        _ => p[p.len().saturating_sub(2)..].join("/"),
    }
}
pub fn session_name(cwd: &str, command: &str) -> String {
    let d = directory_label(cwd);
    let c = normalize_command(command);
    if c.is_empty() {
        d
    } else {
        format!("{d} · {c}")
    }
}
pub fn command_from_line(line: &str) -> Option<String> {
    let re = regex::Regex::new(r"(?:^|\s)[%$#❯➜]\s+(.*)$").expect("static pattern");
    let c = normalize_command(re.captures(line)?.get(1)?.as_str());
    (!c.is_empty()).then_some(c)
}
pub fn is_absolute_path(s: &str) -> bool {
    let s = s.trim();
    s.starts_with('/')
        || s.starts_with("\\\\")
        || (s.len() > 2
            && s.as_bytes()[0].is_ascii_alphabetic()
            && s.as_bytes()[1] == b':'
            && matches!(s.as_bytes()[2], b'/' | b'\\'))
}
pub fn compact_path(s: &str) -> String {
    let s = s.replace('\\', "/");
    regex::Regex::new(r"^(?:[A-Za-z]:)?/Users/[^/]+(?P<tail>/|$)")
        .expect("static pattern")
        .replace(&s, "...$tail")
        .into_owned()
}
/// Linear overlap search, including Unicode; caller bounds buffered PTY data.
pub fn output_after_snapshot<'a>(snapshot: &str, buffered: &'a str) -> &'a str {
    // KMP prefix table avoids quadratic scans of repeated terminal output.
    let b = buffered.as_bytes();
    if b.is_empty() {
        return buffered;
    }
    let mut pi = vec![0; b.len()];
    for i in 1..b.len() {
        let mut j = pi[i - 1];
        while j > 0 && b[i] != b[j] {
            j = pi[j - 1];
        }
        if b[i] == b[j] {
            j += 1;
        }
        pi[i] = j;
    }
    let start = snapshot.len().saturating_sub(b.len());
    let mut matched = 0;
    for c in &snapshot.as_bytes()[start..] {
        if matched == b.len() {
            matched = pi[matched - 1];
        }
        while matched > 0 && *c != b[matched] {
            matched = pi[matched - 1];
        }
        if *c == b[matched] {
            matched += 1;
        }
    }
    while !buffered.is_char_boundary(matched) {
        matched = pi[matched - 1];
    }
    &buffered[matched..]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_is_explicit_and_down_clamps() {
        let mut s = Sessions::default();
        assert_eq!(s.selected, None);
        s.down();
        assert_eq!(s.selected, None);
    }
    #[test]
    fn shortcut_command_and_control_numbers() {
        assert_eq!(
            shortcut_index("Digit1", "1", true, false, false, false),
            Some(0)
        );
        assert_eq!(
            shortcut_index("Numpad9", "9", false, true, false, false),
            Some(8)
        );
    }
    #[test]
    fn option_and_bare_numbers_are_not_shortcuts() {
        assert_eq!(
            shortcut_index("Digit1", "¡", false, false, true, false),
            None
        );
        assert_eq!(
            shortcut_index("Digit1", "1", false, false, false, false),
            None
        );
    }
    #[test]
    fn labels_and_autocomplete() {
        assert_eq!(
            session_name("/Users/test/Downloads", "cd  Downloads"),
            "~/Downloads · cd Downloads"
        );
        assert_eq!(
            directory_label("/Users/test/Desktop/code/aml/aml-space"),
            "aml/aml-space"
        );
        assert_eq!(directory_label("/Users/test"), "~");
        assert_eq!(directory_label("/"), "/");
        assert_eq!(
            command_from_line("test@mac ~ % cd Downloads"),
            Some("cd Downloads".into())
        );
    }
    #[test]
    fn replay_deduplicates_snapshot() {
        for (a, b, c) in [
            ("old history\nnew prompt\n", "new prompt\n", ""),
            (
                "old history\nnew prompt",
                "new prompt\nnext line",
                "\nnext line",
            ),
            ("old history\n", "brand new output\n", "brand new output\n"),
            ("é😃", "😃x", "x"),
        ] {
            assert_eq!(output_after_snapshot(a, b), c);
        }
    }
    #[test]
    fn default_validation_and_data_fencing() {
        let mut s = Sessions::default();
        let (token, _) = s.attach("a".into());
        assert!(s.accepts_data(token, "a"));
        s.detach();
        assert!(!s.accepts_data(token, "a"));
        assert_eq!(
            Defaults::from_legacy(&serde_json::json!({"saveFor":"wrong","keepAliveFor":"day"}))
                .save_for,
            SaveFor::Week
        );
    }
    #[test]
    fn paths_are_cross_platform() {
        assert!(is_absolute_path("C:\\work"));
        assert!(is_absolute_path("\\\\host\\share"));
        assert!(!is_absolute_path("C:work"));
        assert_eq!(compact_path("/Users/test/a"), ".../a");
    }
}
fn serialize_status<S: serde::Serializer>(running: &bool, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(if *running { "running" } else { "exited" })
}
fn deserialize_status<'de, D: serde::Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let value = String::deserialize(d)?;
    match value.as_str() {
        "running" => Ok(true),
        "exited" => Ok(false),
        _ => Err(serde::de::Error::custom("Invalid terminal session status")),
    }
}
impl Session {
    pub fn restore_request(&self, cols: u16, rows: u16) -> serde_json::Value {
        serde_json::json!({"restoreSessionId":self.session_id,"restoreCommand":self.last_command.as_ref().or(self.initial_command.as_ref()),"cwd":self.cwd,"name":self.name,"saveFor":self.save_for,"keepAliveFor":self.keep_alive_for,"cols":cols,"rows":rows})
    }
}
/// Readiness gates for native transports: both subscriptions must precede Create/Attach.
#[derive(Default)]
pub struct ListenerGate {
    data: bool,
    exit: bool,
    pending: Option<Effect>,
}
impl ListenerGate {
    pub fn prepare(&mut self, effect: Effect) -> Option<Effect> {
        self.pending = Some(effect);
        self.drain()
    }
    pub fn data_ready(&mut self) -> Option<Effect> {
        self.data = true;
        self.drain()
    }
    pub fn exit_ready(&mut self) -> Option<Effect> {
        self.exit = true;
        self.drain()
    }
    fn drain(&mut self) -> Option<Effect> {
        if self.data && self.exit {
            self.pending.take()
        } else {
            None
        }
    }
    pub fn reset(&mut self) {
        self.data = false;
        self.exit = false;
        self.pending = None;
    }
}
#[cfg(test)]
mod wire_tests {
    use super::*;
    #[test]
    fn restore_uses_same_identity_and_never_executes_legacy_command() {
        let s:Session=serde_json::from_value(serde_json::json!({"sessionId":"native-terminal-123-4","name":"Downloads · git clone repo","cwd":"/Users/test/Downloads","shell":"/bin/zsh","lastCommand":"git clone repo","saveFor":"forever","keepAliveFor":"until-stop","status":"exited","createdAt":1})).unwrap();
        let r = s.restore_request(80, 24);
        assert_eq!(r["restoreSessionId"], "native-terminal-123-4");
        assert_eq!(r["restoreCommand"], "git clone repo");
        assert!(r.get("initialCommand").is_none());
        assert_eq!(serde_json::to_value(s).unwrap()["createdAt"], 1);
    }
    #[test]
    fn transport_waits_for_both_listeners_once() {
        let mut gate = ListenerGate::default();
        assert!(gate.prepare(Effect::Attach("id".into())).is_none());
        assert!(gate.data_ready().is_none());
        assert_eq!(gate.exit_ready(), Some(Effect::Attach("id".into())));
        assert!(gate.exit_ready().is_none());
    }
    #[test]
    fn selection_down_disappearance_and_restore() {
        let make = |id| {
            serde_json::from_value::<Session>(serde_json::json!({"sessionId":id,"name":"name","cwd":"/tmp","shell":"/bin/zsh","saveFor":"week","keepAliveFor":"3h","status":"running"})).unwrap()
        };
        let mut sessions = Sessions::default();
        sessions.replace(vec![make("first"), make("second")]);
        assert_eq!(sessions.selected, None);
        sessions.down();
        assert_eq!(sessions.selected, Some(0));
        sessions.down();
        assert_eq!(sessions.selected, Some(1));
        sessions.down();
        assert_eq!(sessions.selected, Some(1));
        sessions.replace(vec![make("first")]);
        assert_eq!(sessions.selected, None);
        sessions.down();
        assert_eq!(sessions.selected, Some(0));
    }
}

fn default_save_for() -> SaveFor {
    SaveFor::Week
}
fn default_keep_alive_for() -> KeepAliveFor {
    KeepAliveFor::ThreeHours
}

#[cfg(test)]
mod legacy_repair_tests {
    use super::*;
    #[test]
    fn legacy_rows_default_optional_fields_without_losing_unknown_metadata() {
        let rows:Vec<Session>=serde_json::from_value(serde_json::json!([{"sessionId":"old","name":"old","cwd":"/fixture","status":"exited","createdAt":7,"future":{"x":1}},{"sessionId":"minimal","status":"running"}])).unwrap();
        assert_eq!(rows[0].save_for, SaveFor::Week);
        assert_eq!(rows[0].keep_alive_for, KeepAliveFor::ThreeHours);
        assert_eq!(rows[0].shell, "");
        assert_eq!(rows[1].name, "");
        assert_eq!(rows[1].cwd, "");
        let wire = serde_json::to_value(&rows).unwrap();
        assert_eq!(wire[0]["future"], serde_json::json!({"x":1}));
        assert_eq!(wire[0]["createdAt"], 7);
    }
}
