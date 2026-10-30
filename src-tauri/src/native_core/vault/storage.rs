use crate::native_core::common::{read_json, write_json_atomic};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VaultEnvelope {
    pub version: u32,
    pub salt: String,
    pub nonce: String,
    pub ciphertext: String,
    pub updated_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VaultSettings {
    #[serde(default = "default_auto_lock_minutes")]
    pub auto_lock_minutes: u32,
    #[serde(default)]
    pub biometrics_enabled: bool,
}

fn default_auto_lock_minutes() -> u32 {
    15
}

impl Default for VaultSettings {
    fn default() -> Self {
        Self {
            auto_lock_minutes: default_auto_lock_minutes(),
            biometrics_enabled: false,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct VaultPayload {
    #[serde(default)]
    pub items: Vec<VaultItem>,
    #[serde(default)]
    pub settings: VaultSettings,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VaultItem {
    pub id: String,
    pub category: String, // "server" | "api-key" | "login" | "note"
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub secret: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_secret: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VaultItemHeader {
    pub id: String,
    pub category: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_type: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

impl From<&VaultItem> for VaultItemHeader {
    fn from(item: &VaultItem) -> Self {
        Self {
            id: item.id.clone(),
            category: item.category.clone(),
            title: item.title.clone(),
            service: item.service.clone(),
            account: item.account.clone(),
            port: item.port,
            auth_type: item.auth_type.clone(),
            tags: item.tags.clone(),
            created_at: item.created_at,
            updated_at: item.updated_at,
        }
    }
}

pub fn vault_path(root: &Path) -> std::path::PathBuf {
    root.join("vault.enc")
}

pub fn exists(root: &Path) -> bool {
    vault_path(root).exists()
}

pub fn read_envelope(root: &Path) -> Result<VaultEnvelope, String> {
    let path = vault_path(root);
    if !path.exists() {
        return Err("Vault envelope file does not exist".to_string());
    }
    let value = read_json(&path)?;
    serde_json::from_value(value).map_err(|e| format!("Failed to parse vault envelope: {e}"))
}

pub fn write_envelope(root: &Path, envelope: &VaultEnvelope) -> Result<(), String> {
    let path = vault_path(root);
    let value = serde_json::to_value(envelope)
        .map_err(|e| format!("Failed to serialize vault envelope: {e}"))?;
    write_json_atomic(&path, &value)
}
