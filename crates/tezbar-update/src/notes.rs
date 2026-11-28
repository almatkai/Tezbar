//! Demand-driven release notes; no automatic request or duplicated update policy.
use serde::{Deserialize, Serialize};
#[cfg(feature = "native")]
pub mod http;
const RELEASE_TAG_PREFIX: &str = "https://api.github.com/repos/almatkai/Tezbar/releases/tags/v";
const RELEASE_ACCEPT: &str = "application/vnd.github+json";
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseNotes {
    pub version: String,
    pub body: String,
    pub published_at: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    Fetch {
        generation: u64,
        url: String,
        accept: &'static str,
    },
    Cancel {
        generation: u64,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum State {
    Idle,
    Loading { version: String },
    Ready(ReleaseNotes),
    Error(String),
}
#[derive(Debug)]
pub struct Notes {
    pub state: State,
    generation: u64,
    requested: Option<String>,
}
impl Default for Notes {
    fn default() -> Self {
        Self {
            state: State::Idle,
            generation: 0,
            requested: None,
        }
    }
}
fn valid_version(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-+".contains(&b))
}
impl Notes {
    pub fn open(&mut self, version: &str, current: &str, bundled: Option<&str>) -> Vec<Effect> {
        let mut effects = self.close();
        self.generation += 1;
        if !valid_version(version) {
            self.state = State::Error("Invalid release version.".into());
            return effects;
        }
        if version == current {
            self.state = if let Some(body) = bundled.filter(|s| !s.trim().is_empty()) {
                State::Ready(ReleaseNotes {
                    version: version.into(),
                    body: body.into(),
                    published_at: None,
                })
            } else {
                State::Error("This build is missing its Tezbar release notes.".into())
            };
            return effects;
        }
        self.requested = Some(version.into());
        self.state = State::Loading {
            version: version.into(),
        };
        effects.push(Effect::Fetch {
            generation: self.generation,
            url: format!("{RELEASE_TAG_PREFIX}{version}"),
            accept: RELEASE_ACCEPT,
        });
        effects
    }
    pub fn complete(&mut self, generation: u64, response: Result<&str, String>) -> bool {
        if generation != self.generation {
            return false;
        }
        let Some(version) = self.requested.take() else {
            return false;
        };
        self.state = match response.and_then(|s| parse_release(&version, s)) {
            Ok(n) => State::Ready(n),
            Err(e) => State::Error(e),
        };
        true
    }
    pub fn close(&mut self) -> Vec<Effect> {
        let effects = if self.requested.take().is_some() {
            vec![Effect::Cancel {
                generation: self.generation,
            }]
        } else {
            vec![]
        };
        self.generation += 1;
        self.state = State::Idle;
        effects
    }
}
pub fn parse_release(version: &str, s: &str) -> Result<ReleaseNotes, String> {
    if s.len() > 1024 * 1024 {
        return Err("Release notes exceed 1 MiB.".into());
    }
    let v: serde_json::Value =
        serde_json::from_str(s).map_err(|_| "Release notes are unavailable right now.")?;
    if v["tag_name"].as_str() != Some(&format!("v{version}")) {
        return Err("Release version does not match.".into());
    }
    Ok(ReleaseNotes {
        version: version.into(),
        body: v["body"].as_str().unwrap_or("").into(),
        published_at: v["published_at"].as_str().map(String::from),
    })
}
/// First install records the version without offering; otherwise only explicit read/dismiss writes it.
pub fn offer_post_update(last: &mut Option<String>, first_install: bool, version: &str) -> bool {
    if first_install {
        *last = Some(version.into());
        false
    } else {
        last.as_deref() != Some(version)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn installed_notes_without_network() {
        let mut n = Notes::default();
        assert!(n.open("1", "1", Some("## Changes")).is_empty());
        assert!(matches!(n.state, State::Ready(_)));
    }
    #[test]
    fn missing_installed_notes_never_substitute_remote() {
        let mut n = Notes::default();
        assert!(n.open("1", "1", Some("")).is_empty());
        assert!(matches!(n.state, State::Error(_)));
    }
    #[test]
    fn exact_tag_preserves_markdown() {
        let mut n = Notes::default();
        let e = n.open("0.3.0", "0.4.0", None);
        let Effect::Fetch {
            generation, url, ..
        } = &e[0]
        else {
            panic!()
        };
        assert!(url.ends_with("/v0.3.0"));
        let body = "# What’s new\n\n".to_owned() + &"- A change\n".repeat(20);
        let wire = serde_json::json!({"tag_name":"v0.3.0","body":body});
        assert!(n.complete(*generation, Ok(&wire.to_string())));
        assert_eq!(
            n.state,
            State::Ready(ReleaseNotes {
                version: "0.3.0".into(),
                body,
                published_at: None
            })
        );
    }
    #[test]
    fn rejects_wrong_version() {
        assert!(parse_release("1", r#"{"tag_name":"v2"}"#)
            .unwrap_err()
            .contains("does not match"));
    }
    #[test]
    fn missing_release_is_error() {
        let mut n = Notes::default();
        let e = n.open("1", "2", None);
        let Effect::Fetch { generation, .. } = e[0] else {
            panic!()
        };
        n.complete(generation, Err("unavailable".into()));
        assert_eq!(n.state, State::Error("unavailable".into()));
    }
    #[test]
    fn fresh_install_records_version() {
        let mut last = None;
        assert!(!offer_post_update(&mut last, true, "1"));
        assert!(!offer_post_update(&mut last, false, "1"));
        assert!(offer_post_update(&mut last, false, "2"));
    }
    #[test]
    fn upgrade_unread_until_dismissed() {
        let mut last = Some("1".into());
        assert!(offer_post_update(&mut last, false, "2"));
        assert_eq!(last, Some("1".into()));
        last = Some("2".into());
        assert!(!offer_post_update(&mut last, false, "2"));
    }
    #[test]
    fn existing_before_tracking() {
        assert!(offer_post_update(&mut None, false, "1"));
    }
    #[test]
    fn close_cancels_and_stale_reply_cannot_reopen() {
        let mut n = Notes::default();
        let Effect::Fetch { generation, .. } = n.open("1", "2", None)[0] else {
            panic!()
        };
        assert_eq!(n.close(), vec![Effect::Cancel { generation }]);
        assert!(!n.complete(generation, Ok("{}")));
        assert_eq!(n.state, State::Idle);
    }
}
