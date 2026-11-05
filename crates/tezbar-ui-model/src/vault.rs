//! Secret UI policy. Values are deliberately absent from Debug and copy effects.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    ApiKey,
    Token,
    SshKey,
    Server,
    Password,
    Note,
}
impl Kind {
    pub fn default_icon(self) -> &'static str {
        match self {
            Self::ApiKey => "api-key",
            Self::Token => "token",
            Self::SshKey => "ssh-key",
            Self::Server => "server",
            Self::Password => "password",
            Self::Note => "note",
        }
    }
}
pub struct Detection {
    pub kind: Kind,
    pub suggested_name: String,
}
impl std::fmt::Debug for Detection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Detection")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}
pub fn detect(raw: &str) -> Detection {
    let v = raw.trim();
    let found = |kind, name: &str| Detection {
        kind,
        suggested_name: name.into(),
    };
    if v.is_empty() {
        return found(Kind::Password, "");
    }
    let re = |p: &str| regex::Regex::new(p).expect("static pattern");
    if re(r"-----BEGIN [A-Z ]*PRIVATE KEY-----").is_match(v) {
        return found(Kind::SshKey, "SSH private key");
    }
    let first = v.lines().next().unwrap_or("").trim();
    if let Some(c) = re(r"^ssh\s+(?:-\S+\s+(?:\S+\s+)?)*?([\w.-]+@[\w.-]+)").captures(first) {
        return found(Kind::Server, &c[1]);
    }
    if !v.chars().any(char::is_whitespace) {
        for (p, k, n) in [
            (r"^sk-ant-", Kind::ApiKey, "Anthropic API key"),
            (
                r"^sk-(proj-)?[A-Za-z0-9_-]{8,}",
                Kind::ApiKey,
                "OpenAI API key",
            ),
            (
                r"^(ghp_|gho_|ghu_|ghs_|github_pat_)",
                Kind::Token,
                "GitHub token",
            ),
            (r"^glpat-", Kind::Token, "GitLab token"),
            (r"^AKIA[0-9A-Z]{16}$", Kind::ApiKey, "AWS access key"),
            (r"^xox[abpr]-", Kind::Token, "Slack token"),
            (r"^AIza[0-9A-Za-z_-]{30,}$", Kind::ApiKey, "Google API key"),
            (r"^eyJ[\w-]+\.[\w-]+\.[\w-]+$", Kind::Token, "Token"),
        ] {
            if re(p).is_match(v) {
                return found(k, n);
            }
        }
        if let Some(c) = re(r"^([\w.-]+@[\w.-]+?)(?::\d+)?$").captures(v) {
            return found(Kind::Server, &c[1]);
        }
        return found(Kind::Password, "Password");
    }
    let name = if first.chars().count() > 40 {
        format!("{}…", first.chars().take(39).collect::<String>())
    } else {
        first.into()
    };
    found(Kind::Note, &name)
}
pub fn valid_icon(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 32
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}
pub const ICONS: &[&str] = &[
    "api-key",
    "token",
    "ssh-key",
    "server",
    "password",
    "note",
    "database",
    "cloud",
    "globe",
    "mail",
    "credit-card",
    "bank",
    "wallet",
    "wifi",
    "router",
    "phone",
    "laptop",
    "desktop",
    "shield",
    "fingerprint",
    "user",
    "users",
    "building",
    "home",
    "git-branch",
    "code",
    "box",
    "rocket",
    "bolt",
    "link",
    "bookmark",
    "star",
    "heart",
    "flag",
    "bell",
    "calendar",
    "folder",
    "document",
    "camera",
    "gamepad",
    "cart",
    "gift",
    "plane",
];
pub fn resolve_icon(kind: Kind, id: Option<&str>) -> &str {
    id.filter(|id| ICONS.contains(id))
        .unwrap_or(kind.default_icon())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Status { token: u64 },
    Unlock { token: u64 },
    Copy { token: u64, id: String },
    Save { token: u64 },
    Lock,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopyState {
    Idle,
    Checking,
    Unlocking,
    Copying,
    Copied,
    Cancelled,
    Error,
}
#[derive(Default)]
pub struct Vault {
    pub unlocked: bool,
    pub draft: Option<Draft>,
    pub message: Option<&'static str>,
    pending: Option<(u64, String, CopyState)>,
    next: u64,
    save: Option<(u64, SaveInput)>,
}
/// The adapter consumes this draft explicitly on Save; it is never serialized to diagnostics.
pub struct Draft {
    pub kind: Kind,
    pub name: String,
    pub value: String,
    pub id: Option<String>,
    pub note: String,
    pub name_touched: bool,
    pub clipboard_entry_id: Option<String>,
    pub icon: Option<String>,
}
impl std::fmt::Debug for Draft {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Draft([redacted])")
    }
}
impl Vault {
    pub fn copy(&mut self, id: String) -> Effect {
        self.next += 1;
        self.message = None;
        self.pending = Some((self.next, id, CopyState::Checking));
        Effect::Status { token: self.next }
    }
    pub fn status(&mut self, token: u64, unlocked: bool) -> Option<Effect> {
        let (t, id, state) = self.pending.as_mut()?;
        if *t != token || *state != CopyState::Checking {
            return None;
        }
        self.unlocked = unlocked;
        *state = if unlocked {
            CopyState::Copying
        } else {
            CopyState::Unlocking
        };
        Some(if unlocked {
            Effect::Copy {
                token,
                id: id.clone(),
            }
        } else {
            Effect::Unlock { token }
        })
    }
    pub fn unlock_result(&mut self, token: u64, ok: bool) -> Option<Effect> {
        let (t, id, state) = self.pending.as_mut()?;
        if *t != token || *state != CopyState::Unlocking {
            return None;
        }
        self.unlocked = ok;
        *state = if ok {
            CopyState::Copying
        } else {
            CopyState::Cancelled
        };
        ok.then(|| Effect::Copy {
            token,
            id: id.clone(),
        })
    }
    pub fn copied(&mut self, token: u64, ok: bool) {
        if let Some((t, _, s)) = &mut self.pending {
            if *t == token && *s == CopyState::Copying {
                *s = if ok {
                    CopyState::Copied
                } else {
                    CopyState::Error
                };
                self.message = Some(if ok {
                    "Copied · clears in 30 s"
                } else {
                    "Could not copy the secret"
                });
            }
        }
    }
    pub fn failed(&mut self, token: u64) {
        if let Some((t, _, state)) = &mut self.pending {
            if *t == token {
                *state = CopyState::Error;
                self.message = Some("Could not copy the secret");
            }
        }
    }
    pub fn copy_state(&self) -> CopyState {
        self.pending
            .as_ref()
            .map(|p| p.2.clone())
            .unwrap_or(CopyState::Idle)
    }
    pub fn paste_draft(&mut self, value: String) {
        let d = detect(&value);
        self.draft = Some(Draft {
            kind: d.kind,
            name: d.suggested_name,
            value,
            id: None,
            note: String::new(),
            name_touched: false,
            clipboard_entry_id: None,
            icon: None,
        });
    }
    pub fn lock(&mut self) -> Effect {
        self.next += 1;
        self.pending = None;
        self.draft = None;
        self.unlocked = false;
        self.message = None;
        self.save = None;
        Effect::Lock
    }
    pub fn save(&mut self) -> Result<Effect, &'static str> {
        let d = self.draft.as_ref().ok_or("No draft")?;
        if d.name.trim().is_empty() || d.value.is_empty() {
            return Err("Name and secret are required");
        }
        if d.icon.as_deref().is_some_and(|id| !valid_icon(id)) {
            return Err("Invalid icon");
        }
        if !self.unlocked {
            return Err("Unlock the vault first");
        }
        let payload = SaveInput {
            id: d.id.clone(),
            name: d.name.trim().into(),
            value: d.value.clone(),
            kind: d.kind,
            icon: d.icon.clone(),
            note: d.note.clone(),
            clipboard_entry_id: d.clipboard_entry_id.clone(),
        };
        self.next += 1;
        self.save = Some((self.next, payload));
        Ok(Effect::Save { token: self.next })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefixes() {
        for (v, k, n) in [
            ("sk-ant-fixture", Kind::ApiKey, "Anthropic API key"),
            ("sk-proj-abcdefgh", Kind::ApiKey, "OpenAI API key"),
            ("sk-abcdefgh", Kind::ApiKey, "OpenAI API key"),
            ("ghp_fixture", Kind::Token, "GitHub token"),
            ("github_pat_fixture", Kind::Token, "GitHub token"),
            ("glpat-fixture", Kind::Token, "GitLab token"),
            ("AKIAABCDEFGHIJKLMNOP", Kind::ApiKey, "AWS access key"),
            ("xoxb-fixture", Kind::Token, "Slack token"),
            (
                "AIzaabcdefghijklmnopqrstuvwxyz123456789",
                Kind::ApiKey,
                "Google API key",
            ),
            ("eyJabc.def.ghi", Kind::Token, "Token"),
        ] {
            let d = detect(v);
            assert_eq!(d.kind, k);
            assert_eq!(d.suggested_name, n);
        }
    }
    #[test]
    fn multiline_and_servers() {
        assert_eq!(
            detect("-----BEGIN OPENSSH PRIVATE KEY-----\nfixture").kind,
            Kind::SshKey
        );
        for (v, n) in [
            ("ssh -p 2222 deploy@203.0.113.10", "deploy@203.0.113.10"),
            ("ssh root@prod.example.com", "root@prod.example.com"),
            ("ubuntu@10.0.0.4:2222", "ubuntu@10.0.0.4"),
        ] {
            assert_eq!(detect(v).suggested_name, n);
        }
    }
    #[test]
    fn notes_and_passwords() {
        assert_eq!(
            detect("Wi-Fi office\npassword: fixture").suggested_name,
            "Wi-Fi office"
        );
        assert_eq!(
            detect("a very long first line that keeps going well past forty characters")
                .suggested_name
                .chars()
                .count(),
            40
        );
        assert_eq!(detect("hunter2!Xq").kind, Kind::Password);
        assert_eq!(detect("  ").suggested_name, "");
    }
    #[test]
    fn icon_ids_and_fallback() {
        let set: std::collections::BTreeSet<_> = ICONS.iter().collect();
        assert_eq!(set.len(), ICONS.len());
        assert!(ICONS.iter().all(|s| valid_icon(s)));
        for k in [
            Kind::ApiKey,
            Kind::Token,
            Kind::SshKey,
            Kind::Server,
            Kind::Password,
            Kind::Note,
        ] {
            assert_eq!(resolve_icon(k, None), k.default_icon());
        }
        assert_eq!(resolve_icon(Kind::Server, Some("cloud")), "cloud");
        assert_eq!(resolve_icon(Kind::Server, Some("future-icon")), "server");
    }
    #[test]
    fn copy_open_locked_cancelled_and_error() {
        for unlocked in [true, false] {
            let mut v = Vault::default();
            let Effect::Status { token } = v.copy("fixture-id".into()) else {
                panic!()
            };
            let e = v.status(token, unlocked);
            if !unlocked {
                assert_eq!(e, Some(Effect::Unlock { token }));
                assert!(matches!(
                    v.unlock_result(token, true),
                    Some(Effect::Copy { .. })
                ));
            } else {
                assert!(matches!(e, Some(Effect::Copy { .. })));
            }
            v.copied(token, true);
            assert_eq!(v.message, Some("Copied · clears in 30 s"));
        }
        let mut v = Vault::default();
        v.copy("id".into());
        v.status(1, false);
        assert_eq!(v.unlock_result(1, false), None);
        assert_eq!(v.copy_state(), CopyState::Cancelled);
        v.copy("gone".into());
        v.failed(2);
        assert_eq!(v.copy_state(), CopyState::Error);
    }
    #[test]
    fn lock_discards_sensitive_draft_and_pending_copy() {
        let mut v = Vault::default();
        v.paste_draft("fixture-private".into());
        assert!(!format!("{:?}", v.draft).contains("fixture-private"));
        v.copy("id".into());
        v.lock();
        assert_eq!(v.status(1, true), None);
        assert!(v.draft.is_none());
    }
}
/// Explicitly serializable only for the core request. Debug always redacts the payload.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveInput {
    pub id: Option<String>,
    pub name: String,
    pub value: String,
    pub kind: Kind,
    pub icon: Option<String>,
    pub note: String,
    #[serde(skip)]
    pub clipboard_entry_id: Option<String>,
}
impl std::fmt::Debug for SaveInput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SaveInput([redacted])")
    }
}
impl Vault {
    pub fn edit_value(&mut self, value: String) {
        if let Some(d) = &mut self.draft {
            let found = detect(&value);
            d.value = value;
            d.kind = found.kind;
            if !d.name_touched {
                d.name = found.suggested_name;
            }
        }
    }
    pub fn edit_name(&mut self, name: String) {
        if let Some(d) = &mut self.draft {
            d.name = name;
            d.name_touched = true;
        }
    }
    pub fn prepared_save(&self, token: u64) -> Option<&SaveInput> {
        self.save
            .as_ref()
            .filter(|(t, _)| *t == token)
            .map(|(_, p)| p)
    }
    /// Caller removes a clipboard-history entry only after a durable Save success.
    pub fn saved(&mut self, token: u64, success: bool) -> Option<String> {
        if !self.save.as_ref().is_some_and(|(t, _)| *t == token) {
            return None;
        }
        let (_, saved) = self.save.take().unwrap();
        if !success {
            self.message = Some("Could not save. Your draft has been kept.");
            return None;
        }
        if self.draft.as_ref().is_some_and(|d| {
            d.value == saved.value && d.name.trim() == saved.name && d.note == saved.note
        }) {
            self.draft = None;
        }
        self.message = Some("Saved");
        saved.clipboard_entry_id
    }
}
#[cfg(test)]
mod draft_tests {
    use super::*;
    #[test]
    fn name_touch_policy_and_frozen_save_payload() {
        let mut v = Vault::default();
        v.unlocked = true;
        v.paste_draft("fixture password".into());
        v.edit_name("Custom".into());
        v.edit_value("different".into());
        assert_eq!(v.draft.as_ref().unwrap().name, "Custom");
        let Effect::Save { token } = v.save().unwrap() else {
            panic!()
        };
        v.edit_value("new draft".into());
        assert_eq!(v.prepared_save(token).unwrap().value, "different");
        v.saved(token, true);
        assert_eq!(v.draft.as_ref().unwrap().value, "new draft");
    }
    #[test]
    fn failure_keeps_draft_and_lock_invalidates_pending_payload() {
        let mut v = Vault::default();
        v.unlocked = true;
        v.paste_draft("fixture".into());
        let Effect::Save { token } = v.save().unwrap() else {
            panic!()
        };
        v.saved(token, false);
        assert!(v.draft.is_some());
        let Effect::Save { token } = v.save().unwrap() else {
            panic!()
        };
        v.lock();
        assert!(v.prepared_save(token).is_none());
    }
}
