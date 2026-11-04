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
            return Err("Connection secret key is missing. Save the secrets again.".into());
        }
        let key = crypto::generate_random_bytes();
        self.key.save(&key)?;
        Ok(key)
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
        let mut all = self.load()?;
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
        )
    }
}
#[cfg(not(test))]
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
#[cfg(all(not(test), windows))]
impl KeyStore for OsKey {
    fn load(&self) -> Result<Option<VaultKey>, String> {
        if !self.path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(&self.path).map_err(err)?;
        let mut plain = dpapi(&bytes, false)?;
        let key = plain
            .as_slice()
            .try_into()
            .map_err(|_| "Connection key is malformed");
        zeroize::Zeroize::zeroize(&mut plain);
        key.map(Some).map_err(str::to_string)
    }
    fn save(&self, key: &VaultKey) -> Result<(), String> {
        let bytes = dpapi(key, true)?;
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
#[cfg(all(not(test), windows))]
fn dpapi(bytes: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
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
            return Err("Windows couldn't access connection secrets".into());
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
    }
}
