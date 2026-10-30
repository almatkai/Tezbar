const SERVICE_NAME: &str = "com.tezbar.app.vault";
const ACCOUNT_NAME: &str = "master-key";

#[cfg(target_os = "macos")]
pub fn is_biometrics_supported() -> bool {
    // macOS Keychain supports hardware-backed items and Touch ID / user presence
    true
}

#[cfg(not(target_os = "macos"))]
pub fn is_biometrics_supported() -> bool {
    false
}

#[cfg(target_os = "macos")]
pub fn save_master_key(key: &[u8; 32]) -> Result<(), String> {
    use security_framework::passwords::set_generic_password;
    // Overwrite or create
    let _ = delete_master_key();
    set_generic_password(SERVICE_NAME, ACCOUNT_NAME, key)
        .map_err(|e| format!("Failed to save key in macOS Keychain: {e}"))
}

#[cfg(not(target_os = "macos"))]
pub fn save_master_key(_key: &[u8; 32]) -> Result<(), String> {
    Err("Keychain integration is only supported on macOS".to_string())
}

#[cfg(target_os = "macos")]
pub fn read_master_key() -> Result<[u8; 32], String> {
    use security_framework::passwords::get_generic_password;
    let data = get_generic_password(SERVICE_NAME, ACCOUNT_NAME)
        .map_err(|e| format!("Failed to read key from macOS Keychain: {e}"))?;

    if data.len() != 32 {
        return Err(format!(
            "Key length mismatch from Keychain: expected 32, got {}",
            data.len()
        ));
    }

    let mut key = [0u8; 32];
    key.copy_from_slice(&data);
    Ok(key)
}

#[cfg(not(target_os = "macos"))]
pub fn read_master_key() -> Result<[u8; 32], String> {
    Err("Keychain integration is only supported on macOS".to_string())
}

#[cfg(target_os = "macos")]
pub fn delete_master_key() -> Result<(), String> {
    use security_framework::passwords::delete_generic_password;
    match delete_generic_password(SERVICE_NAME, ACCOUNT_NAME) {
        Ok(_) => Ok(()),
        Err(e) => {
            // If item not found, that's not a fatal error
            let msg = e.to_string();
            if msg.contains("not found") || msg.contains("-25300") {
                Ok(())
            } else {
                Err(format!("Failed to delete key from macOS Keychain: {e}"))
            }
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn delete_master_key() -> Result<(), String> {
    Ok(())
}
