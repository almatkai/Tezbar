//! Shared resource limits for native execution; image bytes are budgeted separately.
pub const TURNS: usize = 100;
pub const CALLS: usize = 256;
pub const CALLS_PER_TURN: usize = 32;
pub const STREAM_BYTES: usize = 8 * 1024 * 1024;
pub const ARGUMENT_BYTES: usize = 1024 * 1024;
pub const RESULT_BYTES: usize = 64 * 1024;
pub const CONTEXT_BYTES: usize = 4 * 1024 * 1024;
pub const FILE_BYTES: u64 = 2 * 1024 * 1024;
pub const IMAGE_BYTES: usize = 8 * 1024 * 1024;
pub const CONVERSATION_IMAGES: usize = 128 * 1024 * 1024;

/// Primary docs checked 2026-11-04: Claude vision uses a 10 MB *base64*
/// per-image limit; Bedrock isn't an adapter here. Other adapters retain the
/// app's 8 MiB decoded input cap. Whole-request limits are checked after encode.
pub fn validate_images(
    protocol: &super::transcript::Protocol,
    messages: &[super::transcript::Message],
) -> Result<(), String> {
    validate_images_budget(protocol, messages, request_bytes(protocol))
}
pub fn validate_images_budget(
    protocol: &super::transcript::Protocol,
    messages: &[super::transcript::Message],
    budget: usize,
) -> Result<(), String> {
    use super::transcript::{Block, Protocol};
    fn check(protocol: &Protocol, blocks: &[Block]) -> Result<(), String> {
        for b in blocks {
            match b {
                Block::Image { data, .. } => {
                    let (max, label) = if *protocol == Protocol::Anthropic {
                        (10_000_000, "Anthropic's 10 MB base64 per-image limit")
                    } else {
                        (
                            IMAGE_BYTES.div_ceil(3) * 4,
                            "Tezbar's 8 MiB per-image limit",
                        )
                    };
                    if data.len() > max {
                        return Err(format!("Native limit: image exceeds {label}. Resize or remove it, or select another provider."));
                    }
                }
                Block::ToolResult { blocks, .. } => check(protocol, blocks)?,
                _ => {}
            }
        }
        Ok(())
    }
    for m in messages {
        check(protocol, &m.blocks)?;
    }
    fn bytes(blocks: &[Block]) -> usize {
        blocks
            .iter()
            .map(|b| match b {
                Block::Image { data, .. } => data.len(),
                Block::ToolResult { blocks, .. } => bytes(blocks),
                _ => 0,
            })
            .sum()
    }
    if messages.iter().map(|m| bytes(&m.blocks)).sum::<usize>()
        > budget.saturating_sub(CONTEXT_BYTES + 1024 * 1024)
    {
        return Err(format!(
            "Native limit: {:?} image request exceeds {} bytes. Remove or resize attachments.",
            protocol, budget
        ));
    }

    Ok(())
}
pub fn request_bytes(protocol: &super::transcript::Protocol) -> usize {
    use super::transcript::Protocol;
    match protocol {
        Protocol::Anthropic => 32_000_000,
        Protocol::Gemini => 100_000_000,
        Protocol::OpenaiChat | Protocol::OpenaiResponses => 512_000_000,
        Protocol::Ollama => 128 * 1024 * 1024,
    }
}

/// Oversized historical/tool images must not poison every subsequent request.
/// Keep canonical bytes and omit only from this provider's disposable wire view.
pub fn project_images(
    protocol: &super::transcript::Protocol,
    messages: &mut [super::transcript::Message],
) -> bool {
    project_images_budget(protocol, messages, request_bytes(protocol))
}
pub fn project_images_budget(
    protocol: &super::transcript::Protocol,
    messages: &mut [super::transcript::Message],
    budget: usize,
) -> bool {
    use super::transcript::{Block, Protocol};
    let mut remaining = budget.saturating_sub(CONTEXT_BYTES + 1024 * 1024);
    fn project(
        protocol: &Protocol,
        blocks: &mut [Block],
        remaining: &mut usize,
        changed: &mut bool,
    ) {
        for b in blocks.iter_mut().rev() {
            match b {
                Block::Image { data, .. } => {
                    let max = if *protocol == Protocol::Anthropic {
                        10_000_000
                    } else {
                        IMAGE_BYTES.div_ceil(3) * 4
                    };
                    if data.len() > max || data.len() > *remaining {
                        *changed = true;
                        *b=Block::Text{text:format!("[Earlier image omitted: exceeds {:?} image/request limit; original image is retained]",protocol)};
                    } else {
                        *remaining -= data.len();
                    }
                }
                Block::ToolResult { blocks, .. } => project(protocol, blocks, remaining, changed),
                _ => {}
            }
        }
    }
    let mut changed = false;
    for m in messages.iter_mut().rev() {
        project(protocol, &mut m.blocks, &mut remaining, &mut changed);
    }
    changed
}
