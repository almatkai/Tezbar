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
