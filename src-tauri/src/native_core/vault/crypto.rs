use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;
use pbkdf2::pbkdf2_hmac;
use sha2::Sha256;

pub const SALT_LEN: usize = 32;
pub const NONCE_LEN: usize = 12;
pub const KEY_LEN: usize = 32;
pub const PBKDF2_ROUNDS: u32 = 100_000;

pub fn generate_random_bytes<const N: usize>() -> [u8; N] {
    use aes_gcm::aead::rand_core::RngCore;
    let mut bytes = [0u8; N];
    OsRng.fill_bytes(&mut bytes);
    bytes
}

pub fn derive_key(password: &str, salt: &[u8; SALT_LEN]) -> [u8; KEY_LEN] {
    let mut key = [0u8; KEY_LEN];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, PBKDF2_ROUNDS, &mut key);
    key
}

pub fn encrypt(key: &[u8; KEY_LEN], plaintext: &[u8]) -> Result<(String, String), String> {
    let cipher_key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(cipher_key);

    let nonce_bytes = generate_random_bytes::<NONCE_LEN>();
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("Encryption failed: {e}"))?;

    let nonce_hex = hex::encode(nonce_bytes);
    let ciphertext_b64 = STANDARD.encode(ciphertext);

    Ok((nonce_hex, ciphertext_b64))
}

pub fn decrypt(
    key: &[u8; KEY_LEN],
    nonce_hex: &str,
    ciphertext_b64: &str,
) -> Result<Vec<u8>, String> {
    let nonce_bytes = hex::decode(nonce_hex).map_err(|e| format!("Invalid nonce hex: {e}"))?;
    if nonce_bytes.len() != NONCE_LEN {
        return Err(format!(
            "Nonce length mismatch: expected {NONCE_LEN}, got {}",
            nonce_bytes.len()
        ));
    }

    let ciphertext = STANDARD
        .decode(ciphertext_b64)
        .map_err(|e| format!("Invalid ciphertext base64: {e}"))?;

    let cipher_key = Key::<Aes256Gcm>::from_slice(key);
    let cipher = Aes256Gcm::new(cipher_key);
    let nonce = Nonce::from_slice(&nonce_bytes);

    cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| "Decryption failed: incorrect password or corrupted data".to_string())
}

pub mod hex {
    pub fn encode<T: AsRef<[u8]>>(data: T) -> String {
        let bytes = data.as_ref();
        let mut s = String::with_capacity(bytes.len() * 2);
        for &b in bytes {
            use std::fmt::Write;
            let _ = write!(s, "{:02x}", b);
        }
        s
    }

    pub fn decode(hex: &str) -> Result<Vec<u8>, String> {
        let s = hex.trim();
        if s.len() % 2 != 0 {
            return Err("Hex string has odd length".to_string());
        }
        (0..s.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&s[i..i + 2], 16)
                    .map_err(|e| format!("Invalid hex byte: {e}"))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_key_consistent() {
        let salt = generate_random_bytes::<SALT_LEN>();
        let k1 = derive_key("my-secret-pass", &salt);
        let k2 = derive_key("my-secret-pass", &salt);
        assert_eq!(k1, k2);

        let k3 = derive_key("wrong-pass", &salt);
        assert_ne!(k1, k3);
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let salt = generate_random_bytes::<SALT_LEN>();
        let key = derive_key("master-vault-password", &salt);
        let secret_data = b"Hello, Tezbar Local Vault! Highly confidential server credentials.";

        let (nonce_hex, ciphertext_b64) = encrypt(&key, secret_data).expect("encryption succeeds");
        assert!(!nonce_hex.is_empty());
        assert!(!ciphertext_b64.is_empty());

        let decrypted = decrypt(&key, &nonce_hex, &ciphertext_b64).expect("decryption succeeds");
        assert_eq!(decrypted, secret_data);
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let salt = generate_random_bytes::<SALT_LEN>();
        let key1 = derive_key("correct-password", &salt);
        let key2 = derive_key("wrong-password", &salt);
        let secret_data = b"Confidential notes.";

        let (nonce_hex, ciphertext_b64) = encrypt(&key1, secret_data).expect("encryption succeeds");
        let result = decrypt(&key2, &nonce_hex, &ciphertext_b64);
        assert!(result.is_err());
    }
}
