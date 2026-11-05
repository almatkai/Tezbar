use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::engine::general_purpose::STANDARD;
use base64::Engine;

pub const NONCE_LEN: usize = 12;
pub const KEY_LEN: usize = 32;

pub fn generate_random_bytes<const N: usize>() -> [u8; N] {
    use aes_gcm::aead::rand_core::RngCore;
    let mut bytes = [0u8; N];
    OsRng.fill_bytes(&mut bytes);
    bytes
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
        .map_err(|_| "Decryption failed: wrong key or corrupted vault file".to_string())
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
    fn encrypt_decrypt_roundtrip_uses_a_fresh_nonce() {
        let key = generate_random_bytes::<KEY_LEN>();
        let secret = b"server root password";
        let (nonce_a, cipher_a) = encrypt(&key, secret).unwrap();
        let (nonce_b, cipher_b) = encrypt(&key, secret).unwrap();
        assert_ne!(nonce_a, nonce_b);
        assert_ne!(cipher_a, cipher_b);
        assert_eq!(decrypt(&key, &nonce_a, &cipher_a).unwrap(), secret);
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let key = generate_random_bytes::<KEY_LEN>();
        let other = generate_random_bytes::<KEY_LEN>();
        let (nonce, cipher) = encrypt(&key, b"api key").unwrap();
        assert!(decrypt(&other, &nonce, &cipher).is_err());
    }
}
