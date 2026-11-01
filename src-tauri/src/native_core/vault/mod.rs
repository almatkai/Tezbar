pub mod auth;
pub mod crypto;
pub mod keychain;
pub mod storage;

use crate::native_core::common::{now_ms, CoreResult};
use auth::{AuthOutcome, DeviceAuthenticator, SystemAuthenticator};
use crypto::{decrypt, encrypt, generate_random_bytes, KEY_LEN};
use keychain::{KeyStore, SystemKeychain, VaultKey};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};
use storage::{
    exists, read_envelope, set_aside_legacy, write_envelope, VaultEnvelope, VaultItem,
    VaultItemHeader, VaultPayload, ENVELOPE_VERSION,
};
use zeroize::Zeroize;

const UNLOCK_REASON: &str = "unlock your Vault";
const RESET_REASON: &str = "erase your Vault";
const UNSUPPORTED: &str = "Vault is available on macOS for now";
const LOCKED: &str = "Vault is locked";
/// Opening the vault again within this idle window needs no new prompt.
const AUTO_LOCK_AFTER: Duration = Duration::from_secs(30 * 60);
/// How long a copied secret may stay on the pasteboard.
const CLIPBOARD_CLEAR_AFTER: Duration = Duration::from_secs(30);
const KINDS: [&str; 6] = ["api-key", "token", "ssh-key", "server", "password", "note"];

#[derive(Clone)]
pub struct VaultService {
    root: PathBuf,
    supported: bool,
    auth: Arc<dyn DeviceAuthenticator>,
    keys: Arc<dyn KeyStore>,
    inner: Arc<Mutex<InnerState>>,
    app: Option<tauri::AppHandle>,
}

struct InnerState {
    key: Option<VaultKey>,
    payload: Option<VaultPayload>,
    last_active: Instant,
}

impl VaultService {
    pub fn new(root: PathBuf, app: Option<tauri::AppHandle>) -> Self {
        Self::with_parts(
            root,
            cfg!(target_os = "macos"),
            Arc::new(SystemAuthenticator),
            Arc::new(SystemKeychain),
            app,
        )
    }

    fn with_parts(
        root: PathBuf,
        supported: bool,
        auth: Arc<dyn DeviceAuthenticator>,
        keys: Arc<dyn KeyStore>,
        app: Option<tauri::AppHandle>,
    ) -> Self {
        Self {
            root,
            supported,
            auth,
            keys,
            inner: Arc::new(Mutex::new(InnerState {
                key: None,
                payload: None,
                last_active: Instant::now(),
            })),
            app,
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, InnerState> {
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        Self::check_auto_lock(&mut state);
        state
    }

    fn check_auto_lock(state: &mut InnerState) {
        if state.key.is_none() {
            return;
        }
        if state.last_active.elapsed() > AUTO_LOCK_AFTER {
            Self::lock_internal(state);
        }
    }

    fn lock_internal(state: &mut InnerState) {
        if let Some(mut key) = state.key.take() {
            key.zeroize();
        }
        if let Some(mut payload) = state.payload.take() {
            payload.wipe();
        }
    }

    pub fn lock(&self) {
        let mut state = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        Self::lock_internal(&mut state);
    }

    pub fn is_unlocked(&self) -> bool {
        self.state().key.is_some()
    }

    pub async fn handle(&self, channel: &str, args: &[Value]) -> Option<CoreResult> {
        if !channel.starts_with("vault:") {
            return None;
        }
        if !self.supported {
            return Some(match channel {
                "vault:status" => Ok(json!({ "supported": false, "unlocked": false, "itemCount": 0 })),
                _ => Err(UNSUPPORTED.to_string()),
            });
        }
        let result = match channel {
            "vault:unlock" => self.unlock().await,
            "vault:reset" => self.reset().await,
            _ => {
                let this = self.clone();
                let channel = channel.to_string();
                let args = args.to_vec();
                tokio::task::spawn_blocking(move || this.dispatch(&channel, &args))
                    .await
                    .unwrap_or_else(|e| Err(e.to_string()))
            }
        };
        Some(result)
    }

    fn dispatch(&self, channel: &str, args: &[Value]) -> CoreResult {
        match channel {
            "vault:status" => self.status(),
            "vault:lock" => {
                self.lock();
                Ok(json!({ "ok": true }))
            }
            "vault:list" => self.list(),
            "vault:get" => self.get(id_arg(args)?),
            "vault:save" => self.save(args),
            "vault:delete" => self.delete(id_arg(args)?),
            "vault:copy" => self.copy(id_arg(args)?),
            _ => Err(format!("Unknown vault channel: {channel}")),
        }
    }

    fn status(&self) -> CoreResult {
        let state = self.state();
        Ok(json!({
            "supported": true,
            "unlocked": state.key.is_some(),
            "itemCount": state.payload.as_ref().map(|p| p.items.len()).unwrap_or(0),
        }))
    }

    fn set_blur_suppressed(&self, value: bool) {
        if let Some(app) = &self.app {
            if let Some(state) = app.try_state::<crate::WindowBehaviorState>() {
                if let Ok(mut suppress) = state.suppress_blur_hide.lock() {
                    *suppress = value;
                }
            }
        }
    }

    fn refocus_main_window(&self) {
        if let Some(app) = &self.app {
            if let Some(main_window) = app.get_webview_window("main") {
                let _ = crate::show_main_window_native(&main_window);
                let _ = main_window.set_focus();
                let _ = main_window.emit("window-shown", json!({ "resetUi": false }));
            }
        }
    }

    async fn unlock(&self) -> CoreResult {
        if self.is_unlocked() {
            return Ok(json!({ "ok": true }));
        }
        self.set_blur_suppressed(true);
        let auth_outcome = self.auth.authenticate(UNLOCK_REASON).await;
        self.set_blur_suppressed(false);
        match auth_outcome {
            AuthOutcome::Granted => {}
            AuthOutcome::Cancelled => {
                self.refocus_main_window();
                return Ok(json!({ "ok": false, "cancelled": true }));
            }
            AuthOutcome::Failed(message) => {
                self.refocus_main_window();
                return Err(message);
            }
        }
        let this = self.clone();
        let open_res = tokio::task::spawn_blocking(move || this.open())
            .await
            .unwrap_or_else(|e| Err(e.to_string()));
        self.refocus_main_window();
        open_res
    }

    /// Runs only after a successful device-owner authentication.
    fn open(&self) -> CoreResult {
        set_aside_legacy(&self.root)?;
        let has_file = exists(&self.root);
        let key = match self.keys.load()? {
            Some(key) => key,
            None if has_file => {
                return Err(
                    "The vault key is missing from Keychain, so this vault cannot be opened."
                        .to_string(),
                )
            }
            None => {
                let key = generate_random_bytes::<KEY_LEN>();
                self.keys.save(&key)?;
                key
            }
        };
        let payload = if has_file {
            let envelope = read_envelope(&self.root)?;
            let mut plaintext = decrypt(&key, &envelope.nonce, &envelope.ciphertext)?;
            let parsed = serde_json::from_slice::<VaultPayload>(&plaintext)
                .map_err(|e| format!("Failed to read vault contents: {e}"));
            plaintext.zeroize();
            let payload = parsed?;
            // Files written before the search index existed get one now.
            if envelope.index.len() != payload.items.len() {
                persist(&self.root, &key, &payload)?;
            }
            payload
        } else {
            let payload = VaultPayload::default();
            persist(&self.root, &key, &payload)?;
            payload
        };
        let mut state = self.state();
        state.key = Some(key);
        state.payload = Some(payload);
        state.last_active = Instant::now();
        Ok(json!({ "ok": true }))
    }

    async fn reset(&self) -> CoreResult {
        self.set_blur_suppressed(true);
        let auth_outcome = self.auth.authenticate(RESET_REASON).await;
        self.set_blur_suppressed(false);
        match auth_outcome {
            AuthOutcome::Granted => {}
            AuthOutcome::Cancelled => {
                self.refocus_main_window();
                return Ok(json!({ "ok": false, "cancelled": true }));
            }
            AuthOutcome::Failed(message) => {
                self.refocus_main_window();
                return Err(message);
            }
        }
        self.lock();
        let path = storage::vault_path(&self.root);
        if path.exists() {
            std::fs::remove_file(path).map_err(|e| format!("Could not delete the vault: {e}"))?;
        }
        self.keys.delete()?;
        self.refocus_main_window();
        Ok(json!({ "ok": true }))
    }

    fn list(&self) -> CoreResult {
        let mut state = self.state();
        let mut headers: Vec<VaultItemHeader> = state
            .payload
            .as_ref()
            .ok_or(LOCKED)?
            .items
            .iter()
            .map(VaultItemHeader::from)
            .collect();
        state.last_active = Instant::now();
        headers.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(json!(headers))
    }

    fn find(&self, id: &str) -> Result<VaultItem, String> {
        let mut state = self.state();
        let item = state
            .payload
            .as_ref()
            .ok_or(LOCKED)?
            .items
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| "This secret no longer exists".to_string())?;
        state.last_active = Instant::now();
        Ok(item)
    }

    fn get(&self, id: &str) -> CoreResult {
        Ok(json!(self.find(id)?))
    }

    fn save(&self, args: &[Value]) -> CoreResult {
        let input = args.first().ok_or("Missing item")?;
        let input = input.get("item").unwrap_or(input);
        let value = input["value"].as_str().unwrap_or_default().to_string();
        if value.trim().is_empty() {
            return Err("Paste a value to save".to_string());
        }
        let name = input["name"].as_str().unwrap_or_default().trim().to_string();
        let kind = input["kind"]
            .as_str()
            .filter(|kind| KINDS.contains(kind))
            .unwrap_or("password")
            .to_string();
        let icon = input["icon"]
            .as_str()
            .filter(|id| {
                (1..=32).contains(&id.len())
                    && id.bytes().all(|byte| {
                        byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'
                    })
            })
            .map(str::to_string);
        let note = input["note"]
            .as_str()
            .map(str::trim)
            .filter(|note| !note.is_empty())
            .map(str::to_string);
        let id = input["id"].as_str().unwrap_or_default().trim().to_string();

        let mut state = self.state();
        let key = state.key.ok_or(LOCKED)?;
        let payload = state.payload.as_mut().ok_or(LOCKED)?;
        let now = now_ms();
        let item = if let Some(existing) = payload.items.iter_mut().find(|it| !id.is_empty() && it.id == id) {
            existing.name = if name.is_empty() { existing.name.clone() } else { name };
            existing.value.zeroize();
            existing.value = value;
            existing.kind = kind;
            existing.icon = icon;
            existing.note.zeroize();
            existing.note = note;
            existing.updated_at = now;
            existing.clone()
        } else {
            let item = VaultItem {
                id: uuid::Uuid::new_v4().to_string(),
                name: if name.is_empty() { "Untitled".to_string() } else { name },
                value,
                kind,
                icon,
                note,
                created_at: now,
                updated_at: now,
            };
            payload.items.insert(0, item.clone());
            item
        };
        persist(&self.root, &key, payload)?;
        state.last_active = Instant::now();
        Ok(json!(VaultItemHeader::from(&item)))
    }

    fn delete(&self, id: &str) -> CoreResult {
        let mut state = self.state();
        let key = state.key.ok_or(LOCKED)?;
        let payload = state.payload.as_mut().ok_or(LOCKED)?;
        let before = payload.items.len();
        payload.items.retain(|item| item.id != id);
        if payload.items.len() != before {
            persist(&self.root, &key, payload)?;
        }
        state.last_active = Instant::now();
        Ok(json!({ "ok": true }))
    }

    /// Copies a value marked concealed + transient, so clipboard managers
    /// (Tezbar's own history included) skip it, then clears it after 30 s
    /// unless something else was copied meanwhile.
    fn copy(&self, id: &str) -> CoreResult {
        let mut item = self.find(id)?;
        let written = crate::native_core::launcher::clipboard_native::write_concealed_text(&item.value);
        item.value.zeroize();
        item.note.zeroize();
        let sequence = written?;
        std::thread::spawn(move || {
            std::thread::sleep(CLIPBOARD_CLEAR_AFTER);
            crate::native_core::launcher::clipboard_native::clear_if_sequence(sequence);
        });
        Ok(json!({ "ok": true, "clearAfterSeconds": CLIPBOARD_CLEAR_AFTER.as_secs() }))
    }
}

fn id_arg(args: &[Value]) -> Result<&str, String> {
    args.first()
        .and_then(|value| value.as_str().or_else(|| value["id"].as_str()))
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| "Item id is required".to_string())
}

fn persist(root: &Path, key: &VaultKey, payload: &VaultPayload) -> Result<(), String> {
    let mut bytes =
        serde_json::to_vec(payload).map_err(|e| format!("Failed to serialize vault: {e}"))?;
    let encrypted = encrypt(key, &bytes);
    bytes.zeroize();
    let (nonce, ciphertext) = encrypted?;
    write_envelope(
        root,
        &VaultEnvelope {
            version: ENVELOPE_VERSION,
            nonce,
            ciphertext,
            updated_at: now_ms(),
            index: payload.items.iter().map(Into::into).collect(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use auth::AuthFuture;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FakeAuth(AuthOutcome, Arc<AtomicUsize>);
    impl DeviceAuthenticator for FakeAuth {
        fn authenticate(&self, _reason: &str) -> AuthFuture {
            self.1.fetch_add(1, Ordering::SeqCst);
            let outcome = self.0.clone();
            Box::pin(async move { outcome })
        }
    }

    #[derive(Default)]
    struct FakeKeys {
        key: Mutex<Option<VaultKey>>,
        loads: AtomicUsize,
    }
    impl KeyStore for FakeKeys {
        fn load(&self) -> Result<Option<VaultKey>, String> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(*self.key.lock().unwrap())
        }
        fn save(&self, key: &VaultKey) -> Result<(), String> {
            *self.key.lock().unwrap() = Some(*key);
            Ok(())
        }
        fn delete(&self) -> Result<(), String> {
            *self.key.lock().unwrap() = None;
            Ok(())
        }
    }

    fn service(root: &Path, outcome: AuthOutcome, keys: Arc<FakeKeys>) -> (VaultService, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let service = VaultService::with_parts(
            root.to_path_buf(),
            true,
            Arc::new(FakeAuth(outcome, calls.clone())),
            keys,
            None,
        );
        (service, calls)
    }

    async fn call(service: &VaultService, channel: &str, args: &[Value]) -> CoreResult {
        service.handle(channel, args).await.unwrap()
    }

    #[test]
    fn old_json_without_icons_deserializes() {
        let legacy = json!({ "id": "old", "name": "Old", "value": "secret", "kind": "password", "createdAt": 1, "updatedAt": 2 });
        let item: VaultItem = serde_json::from_value(legacy.clone()).unwrap();
        let header: VaultItemHeader = serde_json::from_value(legacy.clone()).unwrap();
        let index: storage::VaultIndexEntry = serde_json::from_value(legacy).unwrap();
        assert_eq!(item.icon, None);
        assert_eq!(header.icon, None);
        assert_eq!(index.icon, None);
        for value in [json!(item), json!(header), json!(index)] {
            assert!(value.get("icon").is_none());
        }
    }

    #[tokio::test]
    async fn custom_icon_persists_in_item_header_and_plaintext_index() {
        let dir = tempfile::tempdir().unwrap();
        let (vault, _) = service(
            dir.path(),
            AuthOutcome::Granted,
            Arc::new(FakeKeys::default()),
        );
        call(&vault, "vault:unlock", &[]).await.unwrap();
        let saved = call(
            &vault,
            "vault:save",
            &[json!({ "name": "Cloud", "value": "secret", "kind": "api-key", "icon": "cloud-2" })],
        )
        .await
        .unwrap();
        let id = saved["id"].clone();
        assert_eq!(saved["icon"], "cloud-2");
        assert_eq!(
            storage::read_index(dir.path())[0].icon.as_deref(),
            Some("cloud-2")
        );
        call(&vault, "vault:lock", &[]).await.unwrap();
        call(&vault, "vault:unlock", &[]).await.unwrap();
        assert_eq!(
            call(&vault, "vault:get", &[id]).await.unwrap()["icon"],
            "cloud-2"
        );
        assert_eq!(
            call(&vault, "vault:list", &[]).await.unwrap()[0]["icon"],
            "cloud-2"
        );
    }

    #[tokio::test]
    async fn invalid_icons_are_dropped_and_edits_can_reset_to_default() {
        let dir = tempfile::tempdir().unwrap();
        let (vault, _) = service(
            dir.path(),
            AuthOutcome::Granted,
            Arc::new(FakeKeys::default()),
        );
        call(&vault, "vault:unlock", &[]).await.unwrap();
        let saved = call(
            &vault,
            "vault:save",
            &[json!({ "value": "secret", "icon": "a".repeat(32) })],
        )
        .await
        .unwrap();
        let id = saved["id"].clone();
        assert_eq!(saved["icon"], "a".repeat(32));
        for invalid in [
            json!(""),
            json!("Cloud"),
            json!("cloud_2"),
            json!(" cloud"),
            json!("a".repeat(33)),
            json!("é"),
            json!(7),
            Value::Null,
        ] {
            let edited = call(
                &vault,
                "vault:save",
                &[json!({ "id": id, "value": "secret", "icon": invalid })],
            )
            .await
            .unwrap();
            assert!(edited.get("icon").is_none());
            assert_eq!(storage::read_index(dir.path())[0].icon, None);
            assert!(call(&vault, "vault:get", &[id.clone()])
                .await
                .unwrap()
                .get("icon")
                .is_none());
        }
        call(
            &vault,
            "vault:save",
            &[json!({ "id": id, "value": "secret", "icon": "star" })],
        )
        .await
        .unwrap();
        let reset = call(
            &vault,
            "vault:save",
            &[json!({ "id": id, "value": "secret" })],
        )
        .await
        .unwrap();
        assert!(reset.get("icon").is_none());
        assert!(call(&vault, "vault:list", &[]).await.unwrap()[0]
            .get("icon")
            .is_none());
        assert_eq!(storage::read_index(dir.path())[0].icon, None);
    }

    #[tokio::test]
    async fn denied_or_cancelled_auth_keeps_the_vault_locked_without_reading_the_key() {
        let dir = tempfile::tempdir().unwrap();
        for outcome in [AuthOutcome::Cancelled, AuthOutcome::Failed("no".into())] {
            let keys = Arc::new(FakeKeys::default());
            let (vault, calls) = service(dir.path(), outcome.clone(), keys.clone());
            let result = call(&vault, "vault:unlock", &[]).await;
            match outcome {
                AuthOutcome::Cancelled => assert_eq!(result.unwrap()["cancelled"], true),
                _ => assert!(result.is_err()),
            }
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert_eq!(keys.loads.load(Ordering::SeqCst), 0);
            assert!(!vault.is_unlocked());
            assert!(call(&vault, "vault:list", &[]).await.is_err());
        }
    }

    #[tokio::test]
    async fn first_unlock_creates_a_key_and_an_empty_encrypted_vault() {
        let dir = tempfile::tempdir().unwrap();
        let keys = Arc::new(FakeKeys::default());
        let (vault, _) = service(dir.path(), AuthOutcome::Granted, keys.clone());
        assert_eq!(call(&vault, "vault:unlock", &[]).await.unwrap()["ok"], true);
        assert!(keys.key.lock().unwrap().is_some());
        assert!(exists(dir.path()));
        let status = call(&vault, "vault:status", &[]).await.unwrap();
        assert_eq!(status["unlocked"], true);
        assert_eq!(status["itemCount"], 0);
        let raw = std::fs::read_to_string(storage::vault_path(dir.path())).unwrap();
        assert!(raw.contains("\"version\": 2"));
    }

    #[tokio::test]
    async fn items_roundtrip_across_lock_and_list_never_exposes_values() {
        let dir = tempfile::tempdir().unwrap();
        let keys = Arc::new(FakeKeys::default());
        let (vault, _) = service(dir.path(), AuthOutcome::Granted, keys.clone());
        call(&vault, "vault:unlock", &[]).await.unwrap();
        let saved = call(
            &vault,
            "vault:save",
            &[json!({ "item": { "name": "OpenAI API key", "value": "value-under-test-123", "kind": "api-key", "note": " prod " } })],
        )
        .await
        .unwrap();
        let id = saved["id"].as_str().unwrap().to_string();
        assert!(saved.get("value").is_none());

        let raw = std::fs::read_to_string(storage::vault_path(dir.path())).unwrap();
        assert!(!raw.contains("value-under-test-123"));
        assert!(!raw.contains("prod"));
        let index = storage::read_index(dir.path());
        assert_eq!(index.len(), 1);
        assert_eq!(index[0].name, "OpenAI API key");
        assert_eq!(index[0].id, id);

        call(&vault, "vault:lock", &[]).await.unwrap();
        assert!(call(&vault, "vault:get", &[json!(id)]).await.is_err());
        call(&vault, "vault:unlock", &[]).await.unwrap();

        let list = call(&vault, "vault:list", &[]).await.unwrap();
        assert_eq!(list[0]["name"], "OpenAI API key");
        assert!(list[0].get("value").is_none());
        assert!(list[0].get("note").is_none());
        let item = call(&vault, "vault:get", &[json!(id)]).await.unwrap();
        assert_eq!(item["value"], "value-under-test-123");
        assert_eq!(item["note"], "prod");

        let created = item["createdAt"].as_u64().unwrap();
        let edited = call(
            &vault,
            "vault:save",
            &[json!({ "item": { "id": id, "name": "", "value": "replaced-value", "kind": "bogus" } })],
        )
        .await
        .unwrap();
        assert_eq!(edited["name"], "OpenAI API key");
        assert_eq!(edited["kind"], "password");
        assert_eq!(edited["createdAt"].as_u64().unwrap(), created);

        assert!(call(&vault, "vault:save", &[json!({ "item": { "name": "x", "value": "  " } })]).await.is_err());

        call(&vault, "vault:delete", &[json!(id)]).await.unwrap();
        assert_eq!(call(&vault, "vault:list", &[]).await.unwrap().as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn a_password_based_v1_file_is_set_aside() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            storage::vault_path(dir.path()),
            r#"{"version":1,"salt":"00","nonce":"00","ciphertext":"","updatedAt":0}"#,
        )
        .unwrap();
        let keys = Arc::new(FakeKeys::default());
        let (vault, _) = service(dir.path(), AuthOutcome::Granted, keys);
        assert_eq!(call(&vault, "vault:unlock", &[]).await.unwrap()["ok"], true);
        assert!(storage::legacy_backup_path(dir.path()).exists());
        assert_eq!(call(&vault, "vault:status", &[]).await.unwrap()["itemCount"], 0);
    }

    #[tokio::test]
    async fn reset_requires_authentication() {
        let dir = tempfile::tempdir().unwrap();
        let keys = Arc::new(FakeKeys::default());
        let (vault, _) = service(dir.path(), AuthOutcome::Granted, keys.clone());
        call(&vault, "vault:unlock", &[]).await.unwrap();

        let (denied, _) = service(dir.path(), AuthOutcome::Cancelled, keys.clone());
        assert_eq!(call(&denied, "vault:reset", &[]).await.unwrap()["cancelled"], true);
        assert!(exists(dir.path()));
        assert!(keys.key.lock().unwrap().is_some());

        assert_eq!(call(&vault, "vault:reset", &[]).await.unwrap()["ok"], true);
        assert!(!exists(dir.path()));
        assert!(keys.key.lock().unwrap().is_none());
        assert!(!vault.is_unlocked());
    }

    #[tokio::test]
    async fn unsupported_platforms_report_it_instead_of_faking_support() {
        let dir = tempfile::tempdir().unwrap();
        let vault = VaultService::with_parts(
            dir.path().to_path_buf(),
            false,
            Arc::new(FakeAuth(AuthOutcome::Granted, Arc::new(AtomicUsize::new(0)))),
            Arc::new(FakeKeys::default()),
            None,
        );
        assert_eq!(call(&vault, "vault:status", &[]).await.unwrap()["supported"], false);
        assert!(call(&vault, "vault:unlock", &[]).await.is_err());
    }
}
