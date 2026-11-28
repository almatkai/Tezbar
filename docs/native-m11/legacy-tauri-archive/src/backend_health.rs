use serde::Serialize;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackendStatus {
    pub phase: String,
    pub detail: String,
    pub revision: u64,
    pub generation: u64,
}
impl Default for BackendStatus {
    fn default() -> Self {
        Self {
            phase: "starting".into(),
            detail: "Starting Rust core…".into(),
            revision: 0,
            generation: 0,
        }
    }
}
