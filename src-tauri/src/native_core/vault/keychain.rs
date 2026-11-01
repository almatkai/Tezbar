//! Storage for the random vault key. Reads happen only after device-owner
//! authentication succeeded (see `auth`).
pub type VaultKey = [u8; 32];

pub trait KeyStore: Send + Sync {
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

#[cfg(not(target_os = "macos"))]
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
