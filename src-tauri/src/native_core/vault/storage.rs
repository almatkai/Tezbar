use crate::native_core::common::{read_json, write_json_atomic};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use zeroize::Zeroize;

pub const ENVELOPE_VERSION: u32 = 2;

/// On-disk file: the payload JSON encrypted with the random vault key, plus
/// a plaintext index of names so global search finds items while locked.
/// Values and notes only exist inside the ciphertext.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VaultEnvelope {
    pub version: u32,
    pub nonce: String,
    pub ciphertext: String,
    pub updated_at: u64,
    #[serde(default)]
    pub index: Vec<VaultIndexEntry>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VaultIndexEntry {
    pub id: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub updated_at: u64,
}

impl From<&VaultItem> for VaultIndexEntry {
    fn from(item: &VaultItem) -> Self {
        Self {
            id: item.id.clone(),
            name: item.name.clone(),
            kind: item.kind.clone(),
            updated_at: item.updated_at,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct VaultPayload {
    #[serde(default)]
    pub items: Vec<VaultItem>,
}

impl VaultPayload {
    /// Overwrite secret strings before the payload is dropped.
    pub fn wipe(&mut self) {
        for item in &mut self.items {
            item.value.zeroize();
            item.note.zeroize();
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VaultItem {
    #[serde(default)]
    pub id: String,
    pub name: String,
    pub value: String,
    /// "api-key" | "token" | "ssh-key" | "server" | "password" | "note"
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default)]
    pub created_at: u64,
    #[serde(default)]
    pub updated_at: u64,
}

/// What the list shows: never the value or the note.
#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct VaultItemHeader {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub created_at: u64,
    pub updated_at: u64,
}

impl From<&VaultItem> for VaultItemHeader {
    fn from(item: &VaultItem) -> Self {
        Self {
            id: item.id.clone(),
            name: item.name.clone(),
            kind: item.kind.clone(),
            created_at: item.created_at,
            updated_at: item.updated_at,
        }
    }
}

pub fn vault_path(root: &Path) -> PathBuf {
    root.join("vault.enc")
}

pub fn legacy_backup_path(root: &Path) -> PathBuf {
    root.join("vault.enc.v1.bak")
}

pub fn exists(root: &Path) -> bool {
    vault_path(root).exists()
}

/// Files written by the password-based dev build cannot be opened with the
/// device-auth key. Keep them aside instead of failing every unlock.
pub fn set_aside_legacy(root: &Path) -> Result<bool, String> {
    let path = vault_path(root);
    if !path.exists() {
        return Ok(false);
    }
    let version = read_json(&path)
        .ok()
        .and_then(|value| value["version"].as_u64());
    if version == Some(u64::from(ENVELOPE_VERSION)) {
        return Ok(false);
    }
    std::fs::rename(&path, legacy_backup_path(root))
        .map_err(|e| format!("Could not set the old vault file aside: {e}"))?;
    Ok(true)
}

pub fn read_envelope(root: &Path) -> Result<VaultEnvelope, String> {
    let value = read_json(&vault_path(root))?;
    let envelope: VaultEnvelope = serde_json::from_value(value)
        .map_err(|e| format!("Failed to parse vault file: {e}"))?;
    if envelope.version != ENVELOPE_VERSION {
        return Err(format!("Unsupported vault file version {}", envelope.version));
    }
    Ok(envelope)
}

/// Names for global search. Empty when there is no readable v2 vault.
pub fn read_index(root: &Path) -> Vec<VaultIndexEntry> {
    if !exists(root) {
        return vec![];
    }
    read_envelope(root).map(|envelope| envelope.index).unwrap_or_default()
}

pub fn write_envelope(root: &Path, envelope: &VaultEnvelope) -> Result<(), String> {
    let value = serde_json::to_value(envelope)
        .map_err(|e| format!("Failed to serialize vault file: {e}"))?;
    write_json_atomic(&vault_path(root), &value)
}
