//! Native port of Raycast's perplexity/ask-perplexity (MIT).
//! Source, attribution, and current host draft/fallback/navigation limits:
//! docs/NATIVE_EXTENSIONS.md and licenses/raycast-extensions-MIT.txt.
use crate::native_core::common::{CoreContext, CoreResult};
use serde_json::{json, Value};

pub(super) const EXTENSION: &str = "raycast.perplexity";
pub(super) const COMMAND: &str = "ask-perplexity";
pub(super) const SUBMIT: &str = "perplexity:submit";

pub(super) struct Session {
    pub use_app: bool,
}

impl Session {
    pub fn view(&self, id: &str) -> Value {
        // No value or onChange action: FormRuntime retains local edits across validation.
        let field = json!({"id":"query","title":"Ask Anything"});
        json!({"ok":true,"mode":"view","sessionId":id,"extensionId":EXTENSION,
            "commandName":COMMAND,"title":"Ask Perplexity",
            "root":{"type":"Form","children":[{"type":"Form.TextArea","props":field}]},
            "actions":[{"id":SUBMIT,"title":"Submit","kind":"submit-form","style":"default"}]})
    }

    pub async fn open(&self, ctx: &CoreContext, query: &str) -> CoreResult {
        let base = if self.use_app {
            "perplexity-app://search"
        } else {
            "https://www.perplexity.ai/search"
        };
        let url = format!("{base}?q={}", encode_query(query));
        ctx.request("shell:open", vec![json!(url)]).await?;
        Ok(json!({"ok":true,"mode":"no-view","message":"","effects":[{"kind":"open","value":url}]}))
    }
}

// URLSearchParams uses application/x-www-form-urlencoded on UTF-8 bytes, with
// spaces as '+' and only ASCII alphanumerics plus '*', '-', '.', '_' unescaped.
fn encode_query(query: &str) -> String {
    const HEX: &[u8] = b"0123456789ABCDEF";
    let mut encoded = String::new();
    for byte in query.bytes() {
        match byte {
            b' ' => encoded.push('+'),
            b'*' | b'-' | b'.' | b'_' | b'0'..=b'9' | b'A'..=b'Z' | b'a'..=b'z' => {
                encoded.push(byte as char)
            }
            _ => {
                encoded.push('%');
                encoded.push(HEX[(byte >> 4) as usize] as char);
                encoded.push(HEX[(byte & 15) as usize] as char);
            }
        }
    }
    encoded
}
