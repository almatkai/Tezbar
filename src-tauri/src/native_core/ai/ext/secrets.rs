//! AI connection secrets use a separate key from the authenticated personal vault.
//! There is no plaintext fallback when an OS key store is unavailable.
use super::super::*;
use crate::native_core::vault::{
    crypto,
    keychain::{KeyStore, VaultKey},
};
use std::path::PathBuf;
pub struct Secrets {
    lock: Mutex<()>,
    path: PathBuf,
    key: Arc<dyn KeyStore>,
}
impl Secrets {
    #[cfg(test)]
    pub(super) fn forget_fixture_key(&self) {
        self.key.delete().unwrap();
    }
    #[cfg(test)]
    fn with_key(config: &std::path::Path, key: Arc<dyn KeyStore>) -> Self {
        Self {
            lock: Mutex::new(()),
            path: config.join("ai-extension-secrets.bin"),
            key,
        }
    }
    pub fn new(config: &std::path::Path) -> Self {
        #[cfg(test)]
        let key: Arc<dyn KeyStore> = Arc::new(MemoryKey(Mutex::new(None)));
        #[cfg(not(test))]
        let key: Arc<dyn KeyStore> = Arc::new(OsKey {
            path: config.join("ai-extension-key.bin"),
        });
        Self {
            lock: Mutex::new(()),
            path: config.join("ai-extension-secrets.bin"),
            key,
        }
    }
    fn key(&self) -> Result<VaultKey, String> {
        if let Some(key) = self.key.load()? {
            return Ok(key);
        }
        if self.path.exists() {
            return Err("Connection secret key is missing. Restore the key to recover saved credentials, or use Reset saved connection secrets in AI settings and enter them again.".into());
        }
        let key = crypto::generate_random_bytes();
        self.key.save(&key)?;
        Ok(key)
    }
    pub fn reset_available(&self) -> Result<bool, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Connection secret store lock failed")?;
        Ok(self.path.exists() && self.key.resettable()?)
    }
    pub fn reset_missing(&self) -> Result<(), String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Connection secret store lock failed")?;
        if !self.path.exists() || !self.key.resettable()? {
            return Err("Reset is only available when the saved connection key is missing".into());
        }
        let archived = self.path.with_extension(format!(
            "bin.unreadable-{}-{}",
            now_ms(),
            uuid::Uuid::new_v4()
        ));
        std::fs::rename(&self.path, &archived).map_err(err)?;
        let revoked = self.path.with_extension("revoked.json");
        let archived_revoked = archived.with_extension("revoked.json");
        let had_revoked = revoked.exists();
        if had_revoked {
            if let Err(error) = std::fs::rename(&revoked, &archived_revoked) {
                std::fs::rename(&archived, &self.path).map_err(err)?;
                return Err(err(error));
            }
        }
        let key = crypto::generate_random_bytes();
        if let Err(error) = self.key.reset(&key) {
            std::fs::rename(&archived, &self.path)
                .map_err(|e| format!("{error}; ciphertext rollback failed: {e}"))?;
            if had_revoked {
                std::fs::rename(archived_revoked, revoked).map_err(err)?;
            }
            return Err(error);
        }
        Ok(())
    }
    fn load(&self) -> CoreResult {
        if !self.path.exists() {
            return Ok(json!({}));
        }
        let envelope = common::read_json(&self.path)?;
        let mut key = self.key()?;
        let decrypted = crypto::decrypt(&key, s(&envelope["nonce"]), s(&envelope["ciphertext"]));
        zeroize::Zeroize::zeroize(&mut key);
        let mut bytes = decrypted?;
        let result = serde_json::from_slice(&bytes).map_err(err);
        zeroize::Zeroize::zeroize(&mut bytes);
        result
    }
    pub fn values(&self, connection: &str) -> Result<HashMap<String, String>, String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Connection secret store lock failed")?;
        let revoked = self.path.with_extension("revoked.json");
        if revoked.exists()
            && common::read_json(&revoked)?
                .as_array()
                .into_iter()
                .flatten()
                .any(|id| id == connection)
        {
            return Ok(HashMap::new());
        }
        let value = self.load()?;
        Ok(value[connection]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
            .collect())
    }
    pub fn put(&self, connection: &str, name: &str, value: Option<&str>) -> Result<(), String> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| "Connection secret store lock failed")?;
        if value.is_none() && !self.path.exists() {
            return Ok(());
        }
        let mut all = match self.load() {
            Ok(all) => all,
            Err(_) if value.is_none() => {
                // Ciphertext may belong to other connections. Record revocation
                // without destroying it or silently replacing its encryption key.
                let path = self.path.with_extension("revoked.json");
                let mut revoked = if path.exists() {
                    common::read_json(&path)?
                } else {
                    json!([])
                };
                if !revoked
                    .as_array()
                    .ok_or("Invalid secret revocations")?
                    .iter()
                    .any(|v| v == connection)
                {
                    revoked.as_array_mut().unwrap().push(json!(connection));
                }
                common::write_json_atomic(&path, &revoked)?;
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        let revoked_path = self.path.with_extension("revoked.json");
        if revoked_path.exists() {
            for id in common::read_json(&revoked_path)?
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
            {
                all.as_object_mut()
                    .ok_or("Invalid secret store")?
                    .remove(id);
            }
        }
        if !all[connection].is_object() {
            all[connection] = json!({})
        }
        if let Some(value) = value {
            all[connection][name] = json!(value)
        } else {
            all.as_object_mut().unwrap().remove(connection);
        }
        let mut key = self.key()?;
        let mut bytes = serde_json::to_vec(&all).map_err(err)?;
        let encrypted = crypto::encrypt(&key, &bytes);
        zeroize::Zeroize::zeroize(&mut key);
        zeroize::Zeroize::zeroize(&mut bytes);
        let (nonce, ciphertext) = encrypted?;
        common::write_json_atomic(
            &self.path,
            &json!({"version":1,"nonce":nonce,"ciphertext":ciphertext}),
        )?;
        if revoked_path.exists() {
            common::write_json_atomic(&revoked_path, &json!([]))?;
        }
        Ok(())
    }
}
#[cfg(any(not(test), windows))]
struct OsKey {
    path: PathBuf,
}
#[cfg(all(not(test), target_os = "macos"))]
impl KeyStore for OsKey {
    fn load(&self) -> Result<Option<VaultKey>, String> {
        let _ = &self.path;
        match security_framework::passwords::get_generic_password(
            "com.tezbar.app.ai-extensions",
            "connection-key",
        ) {
            Ok(mut bytes) => {
                let result = bytes
                    .as_slice()
                    .try_into()
                    .map(Some)
                    .map_err(|_| "Connection key is malformed".into());
                zeroize::Zeroize::zeroize(&mut bytes);
                result
            }
            Err(e) if e.code() == -25300 => Ok(None),
            Err(_) => Err("Couldn't read connection secrets from Keychain".into()),
        }
    }
    fn save(&self, key: &VaultKey) -> Result<(), String> {
        security_framework::passwords::set_generic_password(
            "com.tezbar.app.ai-extensions",
            "connection-key",
            key,
        )
        .map_err(|_| "Couldn't save connection secrets in Keychain".into())
    }
    fn delete(&self) -> Result<(), String> {
        security_framework::passwords::delete_generic_password(
            "com.tezbar.app.ai-extensions",
            "connection-key",
        )
        .map_err(|_| "Couldn't remove connection secret key".into())
    }
}
#[cfg(windows)]
impl KeyStore for OsKey {
    fn resettable(&self) -> Result<bool, String> {
        if !self.path.exists() {
            return Ok(true);
        }
        let bytes = std::fs::read(&self.path).map_err(err)?;
        match dpapi(&bytes, false) {
            Ok(mut bytes) => {
                zeroize::Zeroize::zeroize(&mut bytes);
                Ok(false)
            }
            Err(error) if error.permanent() => Ok(true),
            Err(error) => Err(error.to_string()),
        }
    }
    fn reset(&self, key: &VaultKey) -> Result<(), String> {
        if !self.resettable()? {
            return Err("The saved key is still available".into());
        }
        let archive = self.path.with_extension(format!(
            "bin.unreadable-{}-{}",
            now_ms(),
            uuid::Uuid::new_v4()
        ));
        let exists = self.path.exists();
        if exists {
            std::fs::rename(&self.path, &archive).map_err(err)?;
        }
        if let Err(error) = self.save(key) {
            if exists {
                std::fs::rename(&archive, &self.path)
                    .map_err(|e| format!("{error}; key rollback failed: {e}"))?;
            }
            return Err(error);
        }
        Ok(())
    }
    fn load(&self) -> Result<Option<VaultKey>, String> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&self.path).map_err(err)?;
        let mut plain = dpapi(&bytes, false).map_err(|e| e.to_string())?;
        let key = plain
            .as_slice()
            .try_into()
            .map_err(|_| "Connection key is malformed");
        zeroize::Zeroize::zeroize(&mut plain);
        key.map(Some).map_err(str::to_string)
    }
    fn save(&self, key: &VaultKey) -> Result<(), String> {
        let bytes = dpapi(key, true).map_err(|e| e.to_string())?;
        std::fs::create_dir_all(self.path.parent().ok_or("Invalid connection key path")?)
            .map_err(err)?;
        let mut f = tempfile::NamedTempFile::new_in(self.path.parent().unwrap()).map_err(err)?;
        use std::io::Write;
        f.write_all(&bytes).map_err(err)?;
        f.persist(&self.path).map_err(err)?;
        Ok(())
    }
    fn delete(&self) -> Result<(), String> {
        if self.path.exists() {
            std::fs::remove_file(&self.path).map_err(err)?
        }
        Ok(())
    }
}
#[cfg(windows)]
#[derive(Debug)]
struct DpapiError(u32);
#[cfg(windows)]
impl DpapiError {
    fn permanent(&self) -> bool {
        matches!(self.0, 13 | 0x8009000b)
    }
}
#[cfg(windows)]
impl std::fmt::Display for DpapiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Windows couldn't decrypt connection secrets (0x{:08x}). {}",
            self.0,
            if self.permanent() {
                "Use Reset saved connection secrets to archive them and enter credentials again."
            } else {
                "Restore key-store access and retry."
            }
        )
    }
}
#[cfg(windows)]
fn dpapi(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, DpapiError> {
    #[repr(C)]
    struct Blob {
        len: u32,
        data: *mut u8,
    }
    #[link(name = "crypt32")]
    extern "system" {
        fn CryptProtectData(
            input: *const Blob,
            description: *const u16,
            entropy: *const Blob,
            reserved: *const std::ffi::c_void,
            prompt: *const std::ffi::c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
        fn CryptUnprotectData(
            input: *const Blob,
            description: *mut *mut u16,
            entropy: *const Blob,
            reserved: *const std::ffi::c_void,
            prompt: *const std::ffi::c_void,
            flags: u32,
            output: *mut Blob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLastError() -> u32;
        fn LocalFree(memory: *mut std::ffi::c_void) -> *mut std::ffi::c_void;
    }
    let input = Blob {
        len: bytes.len() as u32,
        data: bytes.as_ptr() as *mut _,
    };
    let mut output = Blob {
        len: 0,
        data: std::ptr::null_mut(),
    };
    unsafe {
        let ok = if encrypt {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                1,
                &mut output,
            )
        };
        if ok == 0 {
            return Err(DpapiError(GetLastError()));
        }
        let bytes = std::slice::from_raw_parts(output.data, output.len as usize).to_vec();
        LocalFree(output.data as _);
        Ok(bytes)
    }
}
#[cfg(all(not(test), not(any(windows, target_os = "macos"))))]
impl KeyStore for OsKey {
    fn load(&self) -> Result<Option<VaultKey>, String> {
        Err("No supported OS secret store".into())
    }
    fn save(&self, _: &VaultKey) -> Result<(), String> {
        Err("No supported OS secret store".into())
    }
    fn delete(&self) -> Result<(), String> {
        Ok(())
    }
}
#[cfg(test)]
struct MemoryKey(Mutex<Option<VaultKey>>);
#[cfg(test)]
impl KeyStore for MemoryKey {
    fn load(&self) -> Result<Option<VaultKey>, String> {
        Ok(*self.0.lock().unwrap())
    }
    fn save(&self, key: &VaultKey) -> Result<(), String> {
        *self.0.lock().unwrap() = Some(*key);
        Ok(())
    }
    fn delete(&self) -> Result<(), String> {
        *self.0.lock().unwrap() = None;
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Unavailable;
    impl KeyStore for Unavailable {
        fn load(&self) -> Result<Option<VaultKey>, String> {
            Err("Fixture key store unavailable".into())
        }
        fn save(&self, _: &VaultKey) -> Result<(), String> {
            Err("Fixture key store unavailable".into())
        }
        fn delete(&self) -> Result<(), String> {
            Ok(())
        }
    }
    #[test]
    fn reopened_store_uses_the_same_keystore_and_unavailable_keystore_never_writes_plaintext() {
        let dir = tempfile::tempdir().unwrap();
        let key: Arc<dyn KeyStore> = Arc::new(MemoryKey(Mutex::new(None)));
        let store = Secrets::with_key(dir.path(), key.clone());
        store
            .put("connection", "token", Some("fixture-persisted-private"))
            .unwrap();
        drop(store);
        let reopened = Secrets::with_key(dir.path(), key);
        assert_eq!(
            reopened.values("connection").unwrap()["token"],
            "fixture-persisted-private"
        );
        assert!(
            !std::fs::read_to_string(dir.path().join("ai-extension-secrets.bin"))
                .unwrap()
                .contains("fixture-persisted-private")
        );
        let other = tempfile::tempdir().unwrap();
        let store = Secrets::with_key(other.path(), Arc::new(Unavailable));
        assert!(store
            .put("connection", "token", Some("must-not-be-written"))
            .is_err());
        assert!(!other.path().join("ai-extension-secrets.bin").exists());
        std::fs::write(
            other.path().join("ai-extension-secrets.bin"),
            "fixture unreadable ciphertext",
        )
        .unwrap();
        assert!(store.reset_available().is_err());
        assert!(store.reset_missing().is_err());
        assert_eq!(
            std::fs::read_to_string(other.path().join("ai-extension-secrets.bin")).unwrap(),
            "fixture unreadable ciphertext"
        );
    }
}

#[cfg(all(test, windows))]
#[test]
fn windows_dpapi_key_roundtrip_uses_only_an_isolated_temp_profile() {
    let directory = tempfile::tempdir().unwrap();
    let store = OsKey {
        path: directory.path().join("connection-key.bin"),
    };
    assert!(store.load().unwrap().is_none());
    let key = [42u8; 32];
    store.save(&key).unwrap();
    let encrypted = std::fs::read(&store.path).unwrap();
    assert!(!encrypted.windows(key.len()).any(|window| window == key));
    assert_eq!(store.load().unwrap(), Some(key));
    store.delete().unwrap();
    assert!(!store.path.exists());
}

#[cfg(all(test, windows))]
#[test]
fn windows_permanent_dpapi_failure_archives_both_files_and_denied_replacement_rolls_back() {
    use std::os::windows::fs::OpenOptionsExt;
    let dir = tempfile::tempdir().unwrap();
    let key = Arc::new(OsKey {
        path: dir.path().join("ai-extension-key.bin"),
    });
    key.save(&[17; 32]).unwrap();
    let mut protected = std::fs::read(&key.path).unwrap();
    protected.truncate(20);
    std::fs::write(&key.path, &protected).unwrap();
    let error = dpapi(&protected, false).unwrap_err();
    assert!(error.permanent(), "actual DPAPI code: {error}");
    assert!(!DpapiError(5).permanent());
    assert!(!DpapiError(0x80090020).permanent());
    let store = Secrets::with_key(dir.path(), key.clone());
    let ciphertext = b"fixture unreadable ciphertext";
    std::fs::write(&store.path, ciphertext).unwrap();
    assert!(store.reset_available().unwrap());
    assert!(store.put("c", "token", Some("must-not-write")).is_err());
    // A real Windows sharing violation prevents renaming/overwriting the key.
    let denied = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&key.path)
        .unwrap();
    assert!(store.reset_missing().is_err());
    assert_eq!(std::fs::read(&key.path).unwrap(), protected);
    assert_eq!(std::fs::read(&store.path).unwrap(), ciphertext);
    drop(denied);
    store.reset_missing().unwrap();
    assert!(std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .any(|e| e
            .file_name()
            .to_string_lossy()
            .starts_with("ai-extension-key.bin.unreadable-")
            && std::fs::read(e.path()).unwrap() == protected));
    store
        .put("c", "token", Some("fixture-new-private"))
        .unwrap();
    assert_eq!(store.values("c").unwrap()["token"], "fixture-new-private");
}
