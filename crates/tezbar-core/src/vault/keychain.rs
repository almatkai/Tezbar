//! Storage for the random vault key. Reads happen only after device-owner
//! authentication succeeded (see `auth`).
pub type VaultKey = [u8; 32];

pub trait KeyStore: Send + Sync {
    /// Explicit recovery only; denied/transient access must remain an error.
    fn resettable(&self) -> Result<bool, String> {
        self.load().map(|key| key.is_none())
    }
    fn reset(&self, key: &VaultKey) -> Result<(), String> {
        self.save(key)
    }
    fn load(&self) -> Result<Option<VaultKey>, String>;
    fn save(&self, key: &VaultKey) -> Result<(), String>;
    fn delete(&self) -> Result<(), String>;
}

pub struct SystemKeychain;

#[cfg(target_os = "macos")]
const SERVICE_NAME: &str = "com.tezbar.app.vault";
#[cfg(target_os = "macos")]
const ACCOUNT_NAME: &str = "vault-key";
/// errSecItemNotFound
#[cfg(target_os = "macos")]
const ITEM_NOT_FOUND: i32 = -25300;

#[cfg(target_os = "macos")]
impl KeyStore for SystemKeychain {
    fn load(&self) -> Result<Option<VaultKey>, String> {
        use security_framework::passwords::get_generic_password;
        let mut data = match get_generic_password(SERVICE_NAME, ACCOUNT_NAME) {
            Ok(data) => data,
            Err(e) if e.code() == ITEM_NOT_FOUND => return Ok(None),
            Err(e) => return Err(format!("Could not read the vault key from Keychain: {e}")),
        };
        let result = if data.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&data);
            Ok(Some(key))
        } else {
            Err("The vault key in Keychain is malformed".to_string())
        };
        zeroize::Zeroize::zeroize(&mut data);
        result
    }

    fn save(&self, key: &VaultKey) -> Result<(), String> {
        use security_framework::passwords::set_generic_password;
        set_generic_password(SERVICE_NAME, ACCOUNT_NAME, key)
            .map_err(|e| format!("Could not save the vault key in Keychain: {e}"))
    }

    fn delete(&self) -> Result<(), String> {
        use security_framework::passwords::delete_generic_password;
        match delete_generic_password(SERVICE_NAME, ACCOUNT_NAME) {
            Ok(()) => Ok(()),
            Err(e) if e.code() == ITEM_NOT_FOUND => Ok(()),
            Err(e) => Err(format!("Could not delete the vault key from Keychain: {e}")),
        }
    }
}

/// The platform key store for a vault rooted at `root`.
pub fn system_store(root: &std::path::Path) -> std::sync::Arc<dyn KeyStore> {
    #[cfg(windows)]
    return std::sync::Arc::new(ProtectedKeyFile::new(root.join(PROTECTED_KEY_FILE)));
    #[cfg(not(windows))]
    {
        let _ = root;
        std::sync::Arc::new(SystemKeychain)
    }
}

/// Human name of the store, for recovery messages.
pub const STORE_NAME: &str = if cfg!(windows) {
    "Windows data protection"
} else {
    "Keychain"
};

#[cfg(windows)]
pub const PROTECTED_KEY_FILE: &str = "vault-key.dpapi";

/// Windows: the random vault key lives beside the vault, sealed with DPAPI for
/// the signed-in Windows account (the per-user secret store that also backs
/// Credential Manager). Each profile keeps its own key file, so previews and
/// production never share or overwrite one another's key, and no existing
/// Credential Manager record is read or changed.
#[cfg(windows)]
pub struct ProtectedKeyFile {
    path: std::path::PathBuf,
}

#[cfg(windows)]
impl ProtectedKeyFile {
    pub fn new(path: std::path::PathBuf) -> Self {
        Self { path }
    }
}

#[cfg(windows)]
mod dpapi {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };
    /// Binds the blob to Tezbar's vault purpose in addition to the user account.
    const ENTROPY: &[u8] = b"com.tezbar.app.vault/vault-key";

    fn blob(bytes: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: bytes.len() as u32,
            pbData: bytes.as_ptr() as *mut u8,
        }
    }

    unsafe fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        let bytes = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        std::ptr::write_bytes(out.pbData, 0, out.cbData as usize);
        LocalFree(out.pbData as _);
        bytes
    }

    pub fn protect(plain: &[u8]) -> Result<Vec<u8>, String> {
        let input = blob(plain);
        let entropy = blob(ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptProtectData(
                &input,
                std::ptr::null(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err(format!(
                "Could not protect the vault key: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(unsafe { take(out) })
    }

    pub fn unprotect(sealed: &[u8]) -> Result<Vec<u8>, String> {
        let input = blob(sealed);
        let entropy = blob(ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };
        let ok = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                &entropy,
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut out,
            )
        };
        if ok == 0 {
            return Err(format!(
                "Could not read the protected vault key: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(unsafe { take(out) })
    }
}

#[cfg(windows)]
impl KeyStore for ProtectedKeyFile {
    fn load(&self) -> Result<Option<VaultKey>, String> {
        let sealed = match std::fs::read(&self.path) {
            Ok(sealed) => sealed,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(format!("Could not read the vault key: {e}")),
        };
        let mut plain = dpapi::unprotect(&sealed)?;
        let result = if plain.len() == 32 {
            let mut key = [0u8; 32];
            key.copy_from_slice(&plain);
            Ok(Some(key))
        } else {
            Err("The protected vault key is malformed".to_string())
        };
        zeroize::Zeroize::zeroize(&mut plain);
        result
    }

    fn save(&self, key: &VaultKey) -> Result<(), String> {
        let sealed = dpapi::protect(key)?;
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("Could not save the vault key: {e}"))?;
        }
        let staged = self.path.with_extension("dpapi.tmp");
        std::fs::write(&staged, &sealed).map_err(|e| format!("Could not save the vault key: {e}"))?;
        std::fs::rename(&staged, &self.path).map_err(|e| {
            let _ = std::fs::remove_file(&staged);
            format!("Could not save the vault key: {e}")
        })
    }

    fn delete(&self) -> Result<(), String> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("Could not delete the vault key: {e}")),
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
impl KeyStore for SystemKeychain {
    fn load(&self) -> Result<Option<VaultKey>, String> {
        Err("Vault is available on macOS for now".to_string())
    }

    fn save(&self, _key: &VaultKey) -> Result<(), String> {
        Err("Vault is available on macOS for now".to_string())
    }

    fn delete(&self) -> Result<(), String> {
        Ok(())
    }
}

#[cfg(windows)]
#[cfg(test)]
mod windows_tests {
    use super::*;

    #[test]
    fn protected_key_file_round_trips_and_is_sealed_per_profile() {
        let dir = tempfile::tempdir().unwrap();
        let store = system_store(dir.path());
        assert_eq!(store.load().unwrap(), None);
        let key: VaultKey = core::array::from_fn(|i| i as u8 ^ 0x5a);
        store.save(&key).unwrap();
        let sealed = std::fs::read(dir.path().join(PROTECTED_KEY_FILE)).unwrap();
        assert!(!sealed.windows(key.len()).any(|w| w == key), "key must not be stored in clear");
        assert_eq!(store.load().unwrap(), Some(key));
        // A tampered blob is an error, never silently treated as "no key".
        let mut tampered = sealed.clone();
        let last = tampered.len() - 1;
        tampered[last] ^= 0xff;
        std::fs::write(dir.path().join(PROTECTED_KEY_FILE), &tampered).unwrap();
        assert!(store.load().is_err());
        store.delete().unwrap();
        assert_eq!(store.load().unwrap(), None);
        store.delete().unwrap();
    }
}
