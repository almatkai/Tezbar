pub mod crypto;
pub mod keychain;
pub mod storage;

use crate::native_core::common::{now_ms, CoreResult};
use crypto::{decrypt, derive_key, encrypt, generate_random_bytes, hex, SALT_LEN};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use storage::{
    exists, read_envelope, write_envelope, VaultEnvelope, VaultItem, VaultItemHeader,
    VaultPayload, VaultSettings,
};

#[derive(Clone)]
pub struct VaultService {
    root: PathBuf,
    inner: Arc<Mutex<InnerState>>,
}

struct InnerState {
    unlocked_key: Option<[u8; 32]>,
    cached_payload: Option<VaultPayload>,
    cached_salt: Option<[u8; SALT_LEN]>,
    last_active: Instant,
}

impl VaultService {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            inner: Arc::new(Mutex::new(InnerState {
                unlocked_key: None,
                cached_payload: None,
                cached_salt: None,
                last_active: Instant::now(),
            })),
        }
    }

    fn check_auto_lock(state: &mut InnerState) {
        if state.unlocked_key.is_none() {
            return;
        }

        let minutes = state
            .cached_payload
            .as_ref()
            .map(|p| p.settings.auto_lock_minutes)
            .unwrap_or(15);

        if state.last_active.elapsed() > Duration::from_secs(u64::from(minutes) * 60) {
            Self::lock_internal(state);
        }
    }

    fn lock_internal(state: &mut InnerState) {
        if let Some(mut key) = state.unlocked_key.take() {
            // Zeroize sensitive key buffer
            key.fill(0);
        }
        state.cached_payload = None;
        state.cached_salt = None;
    }

    pub fn lock(&self) {
        let mut state = self.inner.lock().unwrap();
        Self::lock_internal(&mut state);
    }

    #[allow(dead_code)]
    pub fn is_unlocked(&self) -> bool {
        let mut state = self.inner.lock().unwrap();
        Self::check_auto_lock(&mut state);
        state.unlocked_key.is_some()
    }

    pub async fn handle(&self, channel: &str, args: &[Value]) -> Option<CoreResult> {
        if !channel.starts_with("vault:") {
            return None;
        }

        let this = self.clone();
        let channel = channel.to_string();
        let args = args.to_vec();

        match tokio::task::spawn_blocking(move || this.dispatch(&channel, &args)).await {
            Ok(res) => Some(res),
            Err(e) => Some(Err(e.to_string())),
        }
    }

    fn dispatch(&self, channel: &str, args: &[Value]) -> CoreResult {
        match channel {
            "vault:status" => self.handle_status(),
            "vault:setup" => self.handle_setup(args),
            "vault:unlock" => self.handle_unlock(args),
            "vault:lock" => self.handle_lock(),
            "vault:list" => self.handle_list(),
            "vault:get" => self.handle_get(args),
            "vault:save" => self.handle_save(args),
            "vault:delete" => self.handle_delete(args),
            "vault:reset" => self.handle_reset(),
            "vault:change-password" => self.handle_change_password(args),
            _ => Err(format!("Unknown vault channel: {channel}")),
        }
    }

    fn handle_status(&self) -> CoreResult {
        let mut state = self.inner.lock().unwrap();
        Self::check_auto_lock(&mut state);

        let initialized = exists(&self.root);
        let unlocked = state.unlocked_key.is_some();
        let biometrics_available = keychain::is_biometrics_supported();
        let auto_lock_minutes = state
            .cached_payload
            .as_ref()
            .map(|p| p.settings.auto_lock_minutes)
            .unwrap_or(15);
        let item_count = state
            .cached_payload
            .as_ref()
            .map(|p| p.items.len())
            .unwrap_or(0);

        Ok(json!({
            "initialized": initialized,
            "unlocked": unlocked,
            "biometricsAvailable": biometrics_available,
            "autoLockMinutes": auto_lock_minutes,
            "itemCount": item_count,
        }))
    }

    fn handle_setup(&self, args: &[Value]) -> CoreResult {
        let first = args.first().ok_or("Missing setup arguments")?;
        let password = first["password"]
            .as_str()
            .ok_or("Password is required")?
            .trim();

        if password.len() < 4 {
            return Err("Master password must be at least 4 characters long".to_string());
        }

        let enable_biometrics = first["enableBiometrics"].as_bool().unwrap_or(false);

        let salt_bytes = generate_random_bytes::<SALT_LEN>();
        let key = derive_key(password, &salt_bytes);

        let payload = VaultPayload {
            items: Vec::new(),
            settings: VaultSettings {
                auto_lock_minutes: 15,
                biometrics_enabled: enable_biometrics,
            },
        };

        let serialized = serde_json::to_vec(&payload)
            .map_err(|e| format!("Failed to serialize initial payload: {e}"))?;

        let (nonce_hex, ciphertext_b64) = encrypt(&key, &serialized)?;

        let envelope = VaultEnvelope {
            version: 1,
            salt: hex::encode(salt_bytes),
            nonce: nonce_hex,
            ciphertext: ciphertext_b64,
            updated_at: now_ms(),
        };

        write_envelope(&self.root, &envelope)?;

        if enable_biometrics && keychain::is_biometrics_supported() {
            let _ = keychain::save_master_key(&key);
        }

        let mut state = self.inner.lock().unwrap();
        state.unlocked_key = Some(key);
        state.cached_salt = Some(salt_bytes);
        state.cached_payload = Some(payload);
        state.last_active = Instant::now();

        Ok(json!({ "ok": true }))
    }

    fn handle_unlock(&self, args: &[Value]) -> CoreResult {
        let first = args.first();
        let use_biometrics = first
            .and_then(|v| v["useBiometrics"].as_bool())
            .unwrap_or(false);

        let envelope = read_envelope(&self.root)?;
        let salt_bytes = hex::decode(&envelope.salt)?;
        if salt_bytes.len() != SALT_LEN {
            return Err("Invalid salt in vault envelope".to_string());
        }
        let mut salt = [0u8; SALT_LEN];
        salt.copy_from_slice(&salt_bytes);

        let key = if use_biometrics {
            keychain::read_master_key()?
        } else {
            let password = first
                .and_then(|v| v["password"].as_str())
                .ok_or("Password or biometrics required to unlock")?;
            derive_key(password, &salt)
        };

        let plaintext = decrypt(&key, &envelope.nonce, &envelope.ciphertext)?;
        let payload: VaultPayload = serde_json::from_slice(&plaintext)
            .map_err(|e| format!("Failed to parse vault payload: {e}"))?;

        if payload.settings.biometrics_enabled && !use_biometrics && keychain::is_biometrics_supported() {
            let _ = keychain::save_master_key(&key);
        }

        let mut state = self.inner.lock().unwrap();
        state.unlocked_key = Some(key);
        state.cached_salt = Some(salt);
        state.cached_payload = Some(payload);
        state.last_active = Instant::now();

        Ok(json!({ "ok": true }))
    }

    fn handle_lock(&self) -> CoreResult {
        self.lock();
        Ok(json!({ "ok": true }))
    }

    fn handle_list(&self) -> CoreResult {
        let mut state = self.inner.lock().unwrap();
        Self::check_auto_lock(&mut state);

        let mut headers: Vec<VaultItemHeader> = match state.cached_payload.as_ref() {
            Some(p) => p.items.iter().map(VaultItemHeader::from).collect(),
            None => return Err("Vault is locked".to_string()),
        };

        state.last_active = Instant::now();
        headers.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        Ok(json!(headers))
    }

    fn handle_get(&self, args: &[Value]) -> CoreResult {
        let id = args
            .first()
            .and_then(|v| {
                if let Some(s) = v.as_str() {
                    Some(s)
                } else {
                    v["id"].as_str()
                }
            })
            .ok_or("Item id is required")?;

        let mut state = self.inner.lock().unwrap();
        Self::check_auto_lock(&mut state);

        let item = match state.cached_payload.as_ref() {
            Some(p) => p
                .items
                .iter()
                .find(|it| it.id == id)
                .cloned()
                .ok_or_else(|| format!("Vault item '{id}' not found"))?,
            None => return Err("Vault is locked".to_string()),
        };

        state.last_active = Instant::now();
        Ok(json!(item))
    }

    fn handle_save(&self, args: &[Value]) -> CoreResult {
        let first = args.first().ok_or("Missing item payload")?;
        let item_val = first.get("item").unwrap_or(first);

        let mut item: VaultItem = serde_json::from_value(item_val.clone())
            .map_err(|e| format!("Invalid vault item: {e}"))?;

        let now = now_ms();
        if item.id.trim().is_empty() {
            item.id = uuid::Uuid::new_v4().to_string();
            item.created_at = now;
        }
        item.updated_at = now;

        let mut state = self.inner.lock().unwrap();
        Self::check_auto_lock(&mut state);

        let key = state.unlocked_key.ok_or("Vault is locked")?;
        let salt = state.cached_salt.ok_or("Vault salt missing")?;

        let payload = state
            .cached_payload
            .as_mut()
            .ok_or("Vault is locked")?;

        if let Some(existing) = payload.items.iter_mut().find(|it| it.id == item.id) {
            *existing = item.clone();
        } else {
            payload.items.insert(0, item.clone());
        }

        // Persist to disk
        Self::persist_payload(&self.root, &key, &salt, payload)?;
        state.last_active = Instant::now();

        Ok(json!(item))
    }

    fn handle_delete(&self, args: &[Value]) -> CoreResult {
        let id = args
            .first()
            .and_then(|v| {
                if let Some(s) = v.as_str() {
                    Some(s)
                } else {
                    v["id"].as_str()
                }
            })
            .ok_or("Item id is required")?;

        let mut state = self.inner.lock().unwrap();
        Self::check_auto_lock(&mut state);

        let key = state.unlocked_key.ok_or("Vault is locked")?;
        let salt = state.cached_salt.ok_or("Vault salt missing")?;

        let payload = state
            .cached_payload
            .as_mut()
            .ok_or("Vault is locked")?;

        let before = payload.items.len();
        payload.items.retain(|it| it.id != id);

        if before != payload.items.len() {
            Self::persist_payload(&self.root, &key, &salt, payload)?;
        }

        state.last_active = Instant::now();
        Ok(json!({ "ok": true }))
    }

    fn handle_reset(&self) -> CoreResult {
        self.lock();
        let path = storage::vault_path(&self.root);
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
        let _ = keychain::delete_master_key();
        Ok(json!({ "ok": true }))
    }

    fn handle_change_password(&self, args: &[Value]) -> CoreResult {
        let first = args.first().ok_or("Missing password arguments")?;
        let old_pass = first["oldPassword"].as_str().ok_or("Old password required")?;
        let new_pass = first["newPassword"].as_str().ok_or("New password required")?.trim();

        if new_pass.len() < 4 {
            return Err("New master password must be at least 4 characters long".to_string());
        }

        let envelope = read_envelope(&self.root)?;
        let salt_bytes = hex::decode(&envelope.salt)?;
        let mut old_salt = [0u8; SALT_LEN];
        old_salt.copy_from_slice(&salt_bytes);

        let old_key = derive_key(old_pass, &old_salt);
        let plaintext = decrypt(&old_key, &envelope.nonce, &envelope.ciphertext)?;
        let payload: VaultPayload =
            serde_json::from_slice(&plaintext).map_err(|e| e.to_string())?;

        let new_salt = generate_random_bytes::<SALT_LEN>();
        let new_key = derive_key(new_pass, &new_salt);

        Self::persist_payload(&self.root, &new_key, &new_salt, &payload)?;

        if payload.settings.biometrics_enabled && keychain::is_biometrics_supported() {
            let _ = keychain::save_master_key(&new_key);
        }

        let mut state = self.inner.lock().unwrap();
        state.unlocked_key = Some(new_key);
        state.cached_salt = Some(new_salt);
        state.cached_payload = Some(payload);
        state.last_active = Instant::now();

        Ok(json!({ "ok": true }))
    }

    fn persist_payload(
        root: &Path,
        key: &[u8; 32],
        salt: &[u8; 32],
        payload: &VaultPayload,
    ) -> Result<(), String> {
        let bytes = serde_json::to_vec(payload)
            .map_err(|e| format!("Failed to serialize payload: {e}"))?;
        let (nonce_hex, ciphertext_b64) = encrypt(key, &bytes)?;

        let envelope = VaultEnvelope {
            version: 1,
            salt: hex::encode(salt),
            nonce: nonce_hex,
            ciphertext: ciphertext_b64,
            updated_at: now_ms(),
        };

        write_envelope(root, &envelope)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_vault_lifecycle() {
        let temp_dir = tempfile::tempdir().unwrap();
        let service = VaultService::new(temp_dir.path().to_path_buf());

        // Status: not initialized
        let st = service.dispatch("vault:status", &[]).unwrap();
        assert_eq!(st["initialized"], false);
        assert_eq!(st["unlocked"], false);

        // Setup with password
        let setup_res = service
            .dispatch(
                "vault:setup",
                &[json!({ "password": "super-master-key", "enableBiometrics": false })],
            )
            .unwrap();
        assert_eq!(setup_res["ok"], true);

        // Status: initialized and unlocked
        let st2 = service.dispatch("vault:status", &[]).unwrap();
        assert_eq!(st2["initialized"], true);
        assert_eq!(st2["unlocked"], true);
        assert_eq!(st2["itemCount"], 0);

        // Save server credential
        let item = service
            .dispatch(
                "vault:save",
                &[json!({
                    "item": {
                        "id": "",
                        "category": "server",
                        "title": "Prod Server 1",
                        "service": "prod.example.com",
                        "account": "root",
                        "secret": "super_secret_ssh_key_or_pass",
                        "port": 22,
                        "tags": ["prod", "infra"]
                    }
                })],
            )
            .unwrap();

        assert_eq!(item["title"], "Prod Server 1");
        let id = item["id"].as_str().unwrap().to_string();

        // List headers: contains item, but no raw secret
        let list = service.dispatch("vault:list", &[]).unwrap();
        let arr = list.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["title"], "Prod Server 1");
        assert!(arr[0].get("secret").is_none());

        // Get single item: secret is present
        let retrieved = service.dispatch("vault:get", &[json!(id)]).unwrap();
        assert_eq!(retrieved["secret"], "super_secret_ssh_key_or_pass");

        // Lock vault
        service.dispatch("vault:lock", &[]).unwrap();
        assert_eq!(service.is_unlocked(), false);

        // List fails when locked
        assert!(service.dispatch("vault:list", &[]).is_err());

        // Unlock with wrong password fails
        assert!(service
            .dispatch("vault:unlock", &[json!({ "password": "wrong-password" })])
            .is_err());

        // Unlock with correct password succeeds
        let unlock_res = service
            .dispatch(
                "vault:unlock",
                &[json!({ "password": "super-master-key" })],
            )
            .unwrap();
        assert_eq!(unlock_res["ok"], true);
        assert_eq!(service.is_unlocked(), true);

        // Delete item
        let del = service.dispatch("vault:delete", &[json!(id)]).unwrap();
        assert_eq!(del["ok"], true);

        let list_empty = service.dispatch("vault:list", &[]).unwrap();
        assert_eq!(list_empty.as_array().unwrap().len(), 0);
    }
}
