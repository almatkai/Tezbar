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
/// Capability projection is disposable: canonical images survive model changes.
pub fn image_view(messages: &[Message], vision: bool) -> Vec<Message> {
    fn project(blocks: &mut [Block]) {
        for b in blocks { match b {
            Block::Image{..} => *b=Block::Text{text:"[Earlier image omitted: the selected model cannot read images]".into()},
            Block::ToolResult{blocks,..} => project(blocks), _=>{}
        } }
    }
    let mut view=messages.to_vec();
    if !vision { for m in &mut view { project(&mut m.blocks); } }
    view
}
fn text_size(b: &Block) -> usize {
    match b { Block::Image{..}=>0, Block::ToolResult{blocks,..}=>128+blocks.iter().map(text_size).sum::<usize>(), _=>serde_json::to_vec(b).map(|b|b.len()).unwrap_or(usize::MAX/1024) }
}
pub fn bound_context(messages: &mut [Message]) -> Result<bool, String> {
    let size = |ms: &[Message]| -> usize {
        ms.iter()
            .map(|m| {
                m.blocks
                    .iter()
                    .map(text_size)
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

/// Crash recovery closes dangling calls with unknown outcomes. It never executes
/// them. This runs after loading the selected history window, before encoding.
pub fn close_unanswered(messages: &mut Vec<Message>) -> bool {
    use std::collections::HashSet;
    let answered: HashSet<String> = messages
        .iter()
        .flat_map(|m| &m.blocks)
        .filter_map(|b| {
            if let Block::ToolResult { call, .. } = b {
                Some(call.clone())
            } else {
                None
            }
        })
        .collect();
    let mut repaired = Vec::with_capacity(messages.len());
    let mut changed = false;
    for message in std::mem::take(messages) {
        let calls = message.calls();
        repaired.push(message);
        for call in calls {
            if !answered.contains(s(&call["callId"])) {
                changed = true;
                repaired.push(Message {
                    role: "tool".into(),
                    opaque: None,
                    blocks: vec![Block::ToolResult {
                        call: s(&call["callId"]).into(),
                        wire_id: s(&call["id"]).into(),
                        name: s(&call["function"]["name"]).into(),
                        blocks: vec![Block::Text {
                            text: "Interrupted. The outcome of this call is unknown.".into(),
                        }],
                        is_error: true,
                    }],
                });
            }
        }
    }
    *messages = repaired;
    changed
}
pub fn missing_images(messages: &[Message]) -> bool {
    fn missing(blocks: &[Block]) -> bool {
        blocks.iter().any(|b| match b {
            Block::Text { text } => text.contains("[Earlier image is no longer available]"),
            Block::ToolResult { blocks, .. } => missing(blocks),
            _ => false,
        })
    }
    messages.iter().any(|m| missing(&m.blocks))
}

/// Bound decoded image history independently from text. Eviction is explicit.
pub fn bound_images(messages: &mut [Message]) {
    fn size(blocks: &[Block]) -> usize {
        blocks
            .iter()
            .map(|b| match b {
                Block::Image { data, .. } => data.len().saturating_mul(3) / 4,
                Block::ToolResult { blocks, .. } => size(blocks),
                _ => 0,
            })
            .sum()
    }
    fn evict(blocks: &mut [Block], bytes: &mut usize) {
        for block in blocks {
            if *bytes <= super::limits::CONVERSATION_IMAGES {
                return;
            }
            match block {
                Block::Image { data, .. } => {
                    *bytes = bytes.saturating_sub(data.len().saturating_mul(3) / 4);
                    *block = Block::Text {
                        text: "[Earlier image is no longer available]".into(),
                    };
                }
                Block::ToolResult { blocks, .. } => evict(blocks, bytes),
                _ => {}
            }
        }
    }
    let mut bytes = messages.iter().map(|m| size(&m.blocks)).sum();
    for message in messages {
        evict(&mut message.blocks, &mut bytes)
    }
}

pub fn contains_images(blocks: &[Block]) -> bool {
    blocks.iter().any(|b| match b {
        Block::Image { .. } => true,
        Block::ToolResult { blocks, .. } => contains_images(blocks),
        _ => false,
    })
}

#[cfg(test)]
mod repair_tests {
    use super::*;
    #[test]
    fn nested_images_are_excluded_from_text_context_and_projected_recursively() {
        let image=Block::Image{mime:"image/png".into(),data:"A".repeat(5*1024*1024)};
        let mut messages=vec![Message{role:"tool".into(),opaque:None,blocks:vec![Block::ToolResult{call:"c".into(),wire_id:"c".into(),name:"screen".into(),blocks:vec![image],is_error:false}]}];
        assert!(!bound_context(&mut messages).unwrap());
        assert!(!serde_json::to_string(&image_view(&messages,false)).unwrap().contains(&"A".repeat(100)));
        assert!(serde_json::to_string(&messages).unwrap().contains(&"A".repeat(100)));
    }
}
