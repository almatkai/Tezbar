//! Provider-independent durable history. Opaque payloads are available only to
//! their exact origin. They must never be included in renderer events or logs.
use super::super::s;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Protocol {
    OpenaiChat,
    OpenaiResponses,
    Anthropic,
    Gemini,
    Ollama,
}
impl Protocol {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "openai-chat" => Ok(Self::OpenaiChat),
            "openai-responses" => Ok(Self::OpenaiResponses),
            "anthropic" => Ok(Self::Anthropic),
            "gemini" => Ok(Self::Gemini),
            "ollama" => Ok(Self::Ollama),
            _ => Err(format!("Native configuration: unknown protocol {s}")),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub protocol: Protocol,
    pub provider: String,
    pub model: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Opaque {
    pub origin: Origin,
    pub value: Value,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Block {
    Text {
        text: String,
    },
    Image {
        mime: String,
        data: String,
    },
    Thinking {
        text: String,
    },
    ToolCall {
        id: String,
        wire_id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        call: String,
        wire_id: String,
        name: String,
        blocks: Vec<Block>,
        is_error: bool,
    },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub blocks: Vec<Block>,
    pub opaque: Option<Opaque>,
}
impl Message {
    pub fn text(role: &str, text: &str) -> Self {
        Self {
            role: role.into(),
            blocks: vec![Block::Text { text: text.into() }],
            opaque: None,
        }
    }
    pub fn text_content(&self) -> String {
        self.blocks
            .iter()
            .filter_map(|b| {
                if let Block::Text { text } = b {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub fn raw_for(&self, origin: &Origin) -> Option<&Value> {
        self.opaque
            .as_ref()
            .filter(|o| &o.origin == origin)
            .map(|o| &o.value)
    }
    pub fn calls(&self) -> Vec<Value> {
        self.blocks.iter().filter_map(|b| match b {
        Block::ToolCall{id,wire_id,name,arguments} => Some(json!({"id":wire_id,"callId":id,"type":"function","function":{"name":name,"arguments":arguments}})), _=>None
    }).collect()
    }
    pub fn legacy(v: &Value) -> Self {
        let mut m = Self::text(s(&v["role"]), s(&v["content"]));
        if let Some(c) = v["content"].as_array() {
            m.blocks.clear();
            for b in c {
                if b["type"] == "text" {
                    m.blocks.push(Block::Text {
                        text: s(&b["text"]).into(),
                    });
                }
                if let Some(url) = b["image_url"]["url"].as_str() {
                    if let Some((mime, data)) = url
                        .strip_prefix("data:")
                        .and_then(|u| u.split_once(";base64,"))
                    {
                        m.blocks.push(Block::Image {
                            mime: mime.into(),
                            data: data.into(),
                        });
                    }
                }
            }
        }
        m
    }
}
pub fn wire_id(id: &str) -> String {
    if !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return id.into();
    }
    use sha2::{Digest, Sha256};
    format!("tz_{:x}", Sha256::digest(id.as_bytes()))[..63].into()
}
pub fn bound_context(messages: &mut [Message]) -> Result<bool, String> {
    let size = |ms: &[Message]| -> usize {
        ms.iter()
            .map(|m| {
                m.blocks
                    .iter()
                    .map(|b| match b {
                        Block::Image { .. } => 0,
                        _ => serde_json::to_vec(b)
                            .map(|b| b.len())
                            .unwrap_or(usize::MAX / 1024),
                    })
                    .sum::<usize>()
                    + m.opaque
                        .as_ref()
                        .map(|o| o.value.to_string().len())
                        .unwrap_or(0)
            })
            .sum()
    };
    let mut trimmed = false;
    if size(messages) <= super::limits::CONTEXT_BYTES {
        return Ok(false);
    }
    for i in 0..messages.len() {
        for b in &mut messages[i].blocks {
            if let Block::ToolResult { blocks, .. } = b {
                *blocks = vec![Block::Text {
                    text: "[Earlier output removed to fit the context limit]".into(),
                }];
                trimmed = true;
            }
        }
        if size(messages) <= super::limits::CONTEXT_BYTES {
            return Ok(trimmed);
        }
    }
    Err("Native limit: text context exceeds 4 MiB".into())
}
