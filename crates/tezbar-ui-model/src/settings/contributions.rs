//! AI extension administration coordinates the existing core; never starts an MCP runtime itself.
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
mod secret_slots;
pub use secret_slots::{SecretDescriptor, SecretSlot};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    List,
    ParseImport,
    Add,
    Update,
    Remove,
    Test,
    SetEnabled,
    AcceptReview,
    SetTool,
    SetSecret,
    ResetSecrets,
    ClearAllowance,
    OAuthStart,
    OAuthStatus,
    OAuthCancel,
    Logs,
    ReadResource,
    Instructions,
    ComposerItems,
    PromptResolve,
    ReviewProject,
    DismissProject,
    TrustProject,
}
impl Action {
    pub fn channel(self) -> &'static str {
        match self {
            Self::List => "ai-extensions:list",
            Self::ParseImport => "ai-extensions:parse-import",
            Self::Add => "ai-extensions:add",
            Self::Update => "ai-extensions:update",
            Self::Remove => "ai-extensions:remove",
            Self::Test => "ai-extensions:test",
            Self::SetEnabled => "ai-extensions:set-enabled",
            Self::AcceptReview => "ai-extensions:accept-review",
            Self::SetTool => "ai-extensions:set-tool",
            Self::SetSecret => "ai-extensions:set-secret",
            Self::ResetSecrets => "ai-extensions:reset-secrets",
            Self::ClearAllowance => "ai-extensions:clear-allowance",
            Self::OAuthStart => "ai-extensions:oauth-start",
            Self::OAuthStatus => "ai-extensions:oauth-status",
            Self::OAuthCancel => "ai-extensions:oauth-cancel",
            Self::Logs => "ai-extensions:logs",
            Self::ReadResource => "ai-extensions:read-resource",
            Self::Instructions => "ai-extensions:set-instructions",
            Self::ComposerItems => "ai-extensions:composer-items",
            Self::PromptResolve => "ai-extensions:prompt-resolve",
            Self::ReviewProject => "ai-extensions:review-project",
            Self::DismissProject => "ai-extensions:dismiss-project-review",
            Self::TrustProject => "ai-extensions:trust-project",
        }
    }
}
pub struct Effect {
    pub token: u64,
    pub action: Action,
    pub args: Vec<Value>,
}
struct Pending {
    scope: String,
    action: Action,
    revision: u64,
}
#[derive(Default)]
pub struct Contributions {
    raw: Value,
    raw_revision: u64,
    pub display: Value,
    pub instructions: String,
    instructions_dirty: bool,
    instructions_revision: u64,
    pub result: Option<(Action, Value)>,
    pub preview: Option<Value>,
    preview_id: Option<String>,
    pub message: Option<&'static str>,
    next: u64,
    pending: BTreeMap<u64, Pending>,
    latest: BTreeMap<String, u64>,
    reviewed: BTreeSet<String>,
}
impl Contributions {
    fn request(&mut self, scope: String, action: Action, args: Vec<Value>) -> Effect {
        self.next += 1;
        self.latest.insert(scope.clone(), self.next);
        self.pending.retain(|_, p| p.scope != scope);
        self.pending.insert(
            self.next,
            Pending {
                scope,
                action,
                revision: self.instructions_revision,
            },
        );
        Effect {
            token: self.next,
            action,
            args,
        }
    }
    pub fn load(&mut self) -> Effect {
        self.request("list".into(), Action::List, vec![])
    }
    pub fn reply(&mut self, token: u64, result: Result<Value, ()>) -> Option<Effect> {
        let p = self.pending.remove(&token)?;
        if self.latest.get(&p.scope) != Some(&token) {
            return None;
        }
        let value = match result {
            Ok(v) if v["ok"] != false => v,
            _ => {
                self.message = Some("Could not complete the action. Try again.");
                return None;
            }
        };
        self.message = None;
        match p.action {
            Action::List => {
                self.raw_revision += 1;
                if !self.instructions_dirty {
                    self.instructions = value["globalInstructions"].as_str().unwrap_or("").into();
                }
                self.display = redacted(&value);
                self.raw = value;
                None
            }
            Action::ParseImport => {
                self.preview_id = value["previewId"].as_str().map(str::to_owned);
                self.preview = Some(redacted(&value));
                None
            }
            Action::SetSecret => {
                self.message = Some("Key saved.");
                None
            }
            Action::Instructions => {
                if p.revision == self.instructions_revision {
                    self.instructions_dirty = false;
                }
                Some(self.load())
            }
            Action::ComposerItems
            | Action::PromptResolve
            | Action::ReadResource
            | Action::Logs
            | Action::OAuthStatus => {
                self.result = Some((p.action, redacted(&value)));
                None
            }
            _ => Some(self.load()),
        }
    }
    pub fn import_preview(&mut self, input: String) -> Effect {
        self.preview = None;
        self.preview_id = None;
        self.request("preview".into(), Action::ParseImport, vec![json!(input)])
    }
    pub fn add_preview(&mut self) -> Result<Effect, &'static str> {
        let id = self
            .preview_id
            .take()
            .ok_or("Preview the configuration first")?;
        self.preview = None;
        Ok(self.request(
            "mutation".into(),
            Action::Add,
            vec![json!({"previewId":id})],
        ))
    }
    pub fn can_enable(&self, id: &str) -> bool {
        self.raw["extensions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["id"] == id)
            .is_some_and(|e| {
                e["connections"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .all(|c| c["tested"] == true && c["needsReview"] != true)
            })
    }
    pub fn enable(&mut self, id: &str, enabled: bool) -> Result<Effect, &'static str> {
        if enabled && !self.can_enable(id) {
            return Err("Test and review this extension before enabling it");
        }
        Ok(self.request(
            format!("extension:{id}"),
            Action::SetEnabled,
            vec![json!(id), json!(enabled)],
        ))
    }
    pub fn test(&mut self, id: &str) -> Effect {
        self.request(format!("extension:{id}"), Action::Test, vec![json!(id)])
    }
    pub fn review(&mut self, id: &str) -> Result<Value, &'static str> {
        let e = self.display["extensions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["id"] == id)
            .cloned()
            .ok_or("Extension not found")?;
        self.reviewed.insert(id.into());
        Ok(e)
    }
    pub fn accept_review(&mut self, id: &str, pins: Value) -> Result<Effect, &'static str> {
        if !self.reviewed.remove(id) {
            return Err("Review the changed descriptors first");
        }
        Ok(self.request(
            format!("extension:{id}"),
            Action::AcceptReview,
            vec![json!(id), pins],
        ))
    }
    pub fn set_secret(
        &mut self,
        id: &str,
        key: &str,
        value: String,
        provider: Option<&str>,
    ) -> Effect {
        self.request(
            format!("secret:{id}:{key}"),
            Action::SetSecret,
            vec![
                json!(id),
                json!(key),
                json!(value),
                provider
                    .map(|p| json!({"providerId":p}))
                    .unwrap_or(Value::Null),
            ],
        )
    }
    pub fn edit_instructions(&mut self, text: String) {
        self.instructions = text;
        self.instructions_dirty = true;
        self.instructions_revision += 1;
    }
    pub fn save_instructions(&mut self, use_agents_md: bool) -> Effect {
        self.request(
            "instructions".into(),
            Action::Instructions,
            vec![json!(self.instructions), json!(use_agents_md)],
        )
    }
    /// Uses the original opaque manifest so untouched secret references never become empty strings.
    pub fn update(&mut self, id: &str, field: &str, value: Value) -> Result<Effect, &'static str> {
        if !matches!(
            field,
            "name"
                | "connections"
                | "providers"
                | "prompts"
                | "commands"
                | "agents"
                | "instructions"
        ) {
            return Err("Unsupported manifest field");
        }
        let manifest = self.raw["extensions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["id"] == id)
            .cloned()
            .ok_or("Extension not found")?;
        let value = rehydrate(&value, &manifest[field])?;
        Ok(self.request(
            format!("extension:{id}"),
            Action::Update,
            vec![json!(id), json!({field: value})],
        ))
    }
    /// All operations are explicit, including reset, OAuth, project trust and resource reads.
    pub fn operation(
        &mut self,
        id: &str,
        action: Action,
        args: Vec<Value>,
    ) -> Result<Effect, &'static str> {
        if matches!(
            action,
            Action::List
                | Action::ParseImport
                | Action::Add
                | Action::Update
                | Action::SetEnabled
                | Action::AcceptReview
                | Action::SetSecret
                | Action::Instructions
        ) {
            return Err("Use the owned intent method");
        }
        Ok(self.request(format!("{}:{id}", action.channel()), action, args))
    }
    pub fn set_tool(
        &mut self,
        id: &str,
        name: &str,
        enabled: bool,
        connection: Option<&str>,
    ) -> Effect {
        self.request(
            format!("tool:{id}:{name}"),
            Action::SetTool,
            vec![json!(id), json!(name), json!(enabled), json!(connection)],
        )
    }
    pub fn dispose(&mut self) {
        self.pending.clear();
        self.latest.clear();
        self.preview = None;
        self.preview_id = None;
        self.raw = Value::Null;
        self.display = Value::Null;
        self.instructions.clear();
    }
}
fn redacted(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            if map.get("secret").is_some() {
                json!({"saved":true,"label":map.get("label").and_then(Value::as_str).unwrap_or("Saved")})
            } else {
                Value::Object(
                    map.iter()
                        .filter(|(k, _)| {
                            !matches!(
                                k.as_str(),
                                "apiKey" | "token" | "accessToken" | "refreshToken" | "password"
                            )
                        })
                        .map(|(k, v)| (k.clone(), redacted(v)))
                        .collect(),
                )
            }
        }
        Value::Array(a) => Value::Array(a.iter().map(redacted).collect()),
        _ => value.clone(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn state() -> Value {
        json!({"extensions":[{"id":"fixture","name":"Fixture","connections":[{"tested":false,"env":{"TOKEN":{"secret":"fixture-token","label":"Fixture token"}}}]}],"globalInstructions":"saved"})
    }
    #[test]
    fn enable_requires_test_and_review_and_hides_secret_refs() {
        let mut c = Contributions::default();
        let e = c.load();
        c.reply(e.token, Ok(state()));
        assert!(!c.display.to_string().contains("fixture-token"));
        assert!(c.enable("fixture", true).is_err());
        let mut s = state();
        s["extensions"][0]["connections"][0]["tested"] = json!(true);
        let e = c.load();
        c.reply(e.token, Ok(s));
        assert!(c.enable("fixture", true).is_ok());
    }
    #[test]
    fn import_preview_does_not_test_or_enable() {
        let mut c = Contributions::default();
        let e = c.import_preview("fixture input".into());
        assert_eq!(e.action, Action::ParseImport);
        c.reply(
            e.token,
            Ok(json!({"previewId":"preview","manifest":{},"secretCount":1})),
        );
        let e = c.add_preview().unwrap();
        assert_eq!(e.action, Action::Add);
        assert_eq!(e.args, vec![json!({"previewId":"preview"})]);
    }
    #[test]
    fn instructions_survive_reload_and_secrets_are_write_only() {
        let mut c = Contributions::default();
        c.edit_instructions("draft\nline".into());
        let e = c.load();
        c.reply(e.token, Ok(state()));
        assert_eq!(c.instructions, "draft\nline");
        let e = c.set_secret(
            "fixture",
            "MODEL_AUTH",
            "fixture-provider-key".into(),
            Some("custom:fixture"),
        );
        assert_eq!(e.args[3], json!({"providerId":"custom:fixture"}));
        c.reply(e.token, Ok(json!({"ok":true})));
        assert_eq!(c.message, Some("Key saved."));
        assert!(!c.display.to_string().contains("fixture-provider-key"));
    }
    #[test]
    fn stale_replies_cannot_replace_state_and_reset_is_explicit() {
        let mut c = Contributions::default();
        let a = c.load();
        let b = c.load();
        c.reply(b.token, Ok(state()));
        assert!(c.reply(a.token, Ok(json!({"extensions":[]}))).is_none());
        assert_eq!(c.display["extensions"].as_array().unwrap().len(), 1);
        let e = c.operation("", Action::ResetSecrets, vec![]).unwrap();
        assert_eq!(e.action, Action::ResetSecrets);
        assert!(c.reply(e.token, Ok(json!({"ok":true}))).is_some());
    }
}
#[cfg(test)]
mod revision_tests {
    use super::*;
    #[test]
    fn newer_instruction_draft_survives_older_save_acknowledgement() {
        let mut c = Contributions::default();
        c.edit_instructions("first".into());
        let save = c.save_instructions(true);
        assert_eq!(save.args, vec![json!("first"), json!(true)]);
        c.edit_instructions("new draft".into());
        let load = c.reply(save.token, Ok(json!({"ok":true}))).unwrap();
        c.reply(
            load.token,
            Ok(json!({"globalInstructions":"first","extensions":[]})),
        );
        assert_eq!(c.instructions, "new draft");
    }
}

/// Redacted snapshots may round-trip only with a unique stable id in arrays. Never
/// accept a view-supplied opaque reference or guess which row held a placeholder.
fn rehydrate(value: &Value, raw: &Value) -> Result<Value, &'static str> {
    match value {
        Value::Object(m) if m.contains_key("saved") => {
            if raw.get("secret").is_some() && *value == redacted(raw) {
                Ok(raw.clone())
            } else {
                Err("Saved secret placeholder changed or has no unique original")
            }
        }
        Value::Object(m) => {
            if m.contains_key("secret") {
                return Err("Use the scoped secret editor");
            }
            let mut result = serde_json::Map::new();
            for (key, v) in m {
                result.insert(key.clone(), rehydrate(v, &raw[key])?);
            }
            // Fields hidden entirely by redaction remain opaque and untouched.
            if let Some(old) = raw.as_object() {
                for (k, v) in old {
                    if matches!(
                        k.as_str(),
                        "apiKey" | "token" | "accessToken" | "refreshToken" | "password"
                    ) {
                        if m.contains_key(k) {
                            return Err("Use the scoped secret editor");
                        }
                        result.insert(k.clone(), v.clone());
                    }
                }
            }
            Ok(Value::Object(result))
        }
        Value::Array(a) => {
            let mut ids = BTreeSet::new();
            a.iter()
                .map(|v| {
                    let old = if let Some(id) = v["id"].as_str() {
                        if !ids.insert(id) {
                            return Err("Ambiguous manifest row identity");
                        }
                        let matches: Vec<_> = raw
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|r| r["id"] == id)
                            .collect();
                        if matches.len() > 1 {
                            return Err("Ambiguous manifest row identity");
                        }
                        matches.first().copied().unwrap_or(&Value::Null)
                    } else {
                        &Value::Null
                    };
                    rehydrate(v, old)
                })
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array)
        }
        _ => Ok(value.clone()),
    }
}
impl Contributions {
    /// Safe field edit also supports legacy rows without stable ids. The raw row
    /// is selected inside this controller; secrets and untouched fields are retained.
    pub fn edit_row(
        &mut self,
        id: &str,
        collection: &str,
        index: usize,
        field: &str,
        value: Value,
    ) -> Result<Effect, &'static str> {
        let allowed = match collection {
            "connections" => matches!(field, "url" | "command" | "args"),
            "providers" => matches!(field, "name" | "baseURL" | "models"),
            _ => false,
        };
        if !allowed {
            return Err("Unsupported row field");
        }
        let mut manifest = self.raw["extensions"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|e| e["id"] == id)
            .cloned()
            .ok_or("Extension not found")?;
        let row = manifest[collection]
            .as_array_mut()
            .and_then(|a| a.get_mut(index))
            .ok_or("Row not found")?;
        row[field] = rehydrate(&value, &row[field])?;
        let changed = manifest[collection].clone();
        Ok(self.request(
            format!("extension:{id}"),
            Action::Update,
            vec![json!(id), json!({collection: changed})],
        ))
    }
}

#[cfg(test)]
mod repair_tests {
    use super::*;
    fn fixture() -> Contributions {
        let mut c = Contributions::default();
        let e = c.load();
        c.reply(e.token,Ok(json!({"extensions":[{"id":"fixture","connections":[{"id":"a","url":"old","env":{"KEY":{"secret":"ref-a","label":"Key"}},"headers":{"Authorization":{"secret":"ref-h"}}},{"id":"b","command":"old","env":{"KEY":{"secret":"ref-b"}}}],"providers":[{"id":"p","baseURL":"old","apiKey":{"secret":"ref-p"}}],"unknown":{"future":true}}]})));
        c
    }
    #[test]
    fn edits_and_reorders_rehydrate_opaque_refs_by_stable_identity() {
        let mut c = fixture();
        let mut rows = c.display["extensions"][0]["connections"]
            .as_array()
            .unwrap()
            .clone();
        rows.reverse();
        rows[0]["command"] = json!("new-command");
        rows[1]["url"] = json!("https://example.test");
        let e = c.update("fixture", "connections", json!(rows)).unwrap();
        assert_eq!(e.args[1]["connections"][0]["env"]["KEY"]["secret"], "ref-b");
        assert_eq!(e.args[1]["connections"][1]["env"]["KEY"]["secret"], "ref-a");
        assert_eq!(
            e.args[1]["connections"][1]["headers"]["Authorization"]["secret"],
            "ref-h"
        );
        assert!(e.args[1].get("providers").is_none());
        assert!(e.args[1].get("unknown").is_none());
        assert_eq!(
            c.display["extensions"][0]["unknown"],
            json!({"future":true})
        );
        let mut providers = c.display["extensions"][0]["providers"].clone();
        providers[0]["baseURL"] = json!("new");
        let e = c.update("fixture", "providers", providers).unwrap();
        assert_eq!(e.args[1]["providers"][0]["apiKey"]["secret"], "ref-p");
        let e = c
            .edit_row("fixture", "connections", 0, "url", json!("new"))
            .unwrap();
        assert_eq!(e.args[1]["connections"][0]["env"]["KEY"]["secret"], "ref-a");
        assert!(!c.display.to_string().contains("ref-"));
    }
    #[test]
    fn ambiguous_or_tampered_secret_placeholders_are_rejected() {
        let mut c = fixture();
        let rows = c.display["extensions"][0]["connections"].clone();
        let mut tampered = rows.clone();
        tampered[0]["env"]["KEY"]["label"] = json!("tampered");
        assert!(c.update("fixture", "connections", tampered).is_err());
        let mut ambiguous = rows.clone();
        ambiguous[1]["id"] = json!("a");
        assert!(c.update("fixture", "connections", ambiguous).is_err());
        let mut missing = rows.clone();
        missing[0].as_object_mut().unwrap().remove("id");
        assert!(c.update("fixture", "connections", missing).is_err());
        assert!(c
            .edit_row("fixture", "connections", 0, "env", json!({}))
            .is_err());
    }
}
