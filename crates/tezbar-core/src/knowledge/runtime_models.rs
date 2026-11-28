//! Pinned embedding artifacts. Quantizations never share installation/index identity.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmbeddingVariantId {
    #[default]
    #[serde(rename = "q8_0")]
    Q8_0,
    #[serde(rename = "q4_k_m")]
    Q4KM,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelDescriptor {
    pub id: EmbeddingVariantId,
    pub directory: &'static str,
    pub repository: &'static str,
    pub revision: &'static str,
    pub file: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub model_key: &'static str,
}
impl ModelDescriptor {
    pub fn url(&self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}",
            self.repository, self.revision, self.file
        )
    }
}
pub const Q8: ModelDescriptor = ModelDescriptor {
    id: EmbeddingVariantId::Q8_0,
    directory: "embeddinggemma-2",
    repository: "ggml-org/embeddinggemma-2-GGUF",
    revision: "bfcd298762cc34d0357ece5ebdd31791a3a374d8",
    file: "embeddinggemma-2-Q8_0.gguf",
    bytes: 309855456,
    sha256: "2188ac1deca4b77dffefd603c2776a9d76d9d74ec01841392982ebb840b09135",
    model_key: "google/embeddinggemma-2:bfcd298762cc34d0357ece5ebdd31791a3a374d8:q8:256:search-v1",
};
pub const Q4: ModelDescriptor = ModelDescriptor {
    id: EmbeddingVariantId::Q4KM,
    directory: "embeddinggemma-2-q4_k_m",
    repository: "ngquocvinh/EmbeddingGemma-2-GGUF",
    revision: "8e83fab7c71a90d8f97ae7cacf6a488f654cb5b6",
    file: "EmbeddingGemma-2-Q4_K_M.gguf",
    bytes: 181695104,
    sha256: "ea25550b050fa8826707c5e44659a7c18259606dcc6ba449f771e3c75cf4e663",
    model_key: "ngquocvinh/EmbeddingGemma-2-GGUF:8e83fab7c71a90d8f97ae7cacf6a488f654cb5b6:ea25550b050fa8826707c5e44659a7c18259606dcc6ba449f771e3c75cf4e663:q4_k_m:256:search-v1",
};
impl EmbeddingVariantId {
    pub const ALL: [Self; 2] = [Self::Q8_0, Self::Q4KM];
    pub fn descriptor(self) -> &'static ModelDescriptor {
        match self {
            Self::Q8_0 => &Q8,
            Self::Q4KM => &Q4,
        }
    }
}
