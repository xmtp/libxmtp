use aes_gcm::{Aes256Gcm, KeyInit, aead::Aead};
use hkdf::Hkdf;
use sha2::Sha256;

use crate::CodecError;

/// Size of the HKDF salt in bytes (256-bit)
pub const HKDF_SALT_SIZE: usize = 32;

/// Size of the AES-GCM nonce in bytes (96-bit)
pub const AES_GCM_NONCE_SIZE: usize = 12;

/// Size of the AES-GCM authentication tag in bytes (128-bit)
pub const AES_GCM_TAG_SIZE: usize = 16;

/// Size of the encryption secret in bytes (256-bit)
pub const SECRET_SIZE: usize = 32;

/// Encrypted payload containing the ciphertext and encryption parameters.
#[derive(Debug, Clone)]
pub struct EncryptedPayload {
    /// The encrypted content (ciphertext + 16-byte auth tag)
    pub payload: Vec<u8>,
    /// The 32-byte salt used for HKDF key derivation
    pub salt: Vec<u8>,
    /// The 12-byte nonce used for AES-GCM encryption
    pub nonce: Vec<u8>,
}

/// Encrypts plaintext using AES-256-GCM with HKDF-SHA256 key derivation.
pub fn encrypt(plaintext: &[u8], secret: &[u8]) -> Result<EncryptedPayload, CodecError> {
    // Generate random salt and nonce
    let salt: [u8; HKDF_SALT_SIZE] = xmtp_common::rand_array();
    let nonce: [u8; AES_GCM_NONCE_SIZE] = xmtp_common::rand_array();

    // Derive AES-256 key using HKDF-SHA256
    let key = derive_key(secret, &salt)?;

    // Create cipher
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| CodecError::Encode(format!("failed to create cipher: {e}")))?;

    let ciphertext = cipher
        .encrypt((&nonce).into(), plaintext)
        .map_err(|e| CodecError::Encode(format!("encryption failed: {e}")))?;

    Ok(EncryptedPayload {
        payload: ciphertext,
        salt: salt.to_vec(),
        nonce: nonce.to_vec(),
    })
}

/// Decrypts ciphertext that was encrypted with [`encrypt`].
pub fn decrypt(encrypted: &EncryptedPayload, secret: &[u8]) -> Result<Vec<u8>, CodecError> {
    // Validate salt and nonce lengths
    if encrypted.salt.len() != HKDF_SALT_SIZE {
        return Err(CodecError::Decode(format!(
            "invalid salt length: expected {}, got {}",
            HKDF_SALT_SIZE,
            encrypted.salt.len()
        )));
    }
    if encrypted.nonce.len() != AES_GCM_NONCE_SIZE {
        return Err(CodecError::Decode(format!(
            "invalid nonce length: expected {}, got {}",
            AES_GCM_NONCE_SIZE,
            encrypted.nonce.len()
        )));
    }

    // Derive AES-256 key using HKDF-SHA256
    let key = derive_key(secret, &encrypted.salt)?;

    // Create cipher
    let cipher = Aes256Gcm::new_from_slice(&key)
        .map_err(|e| CodecError::Decode(format!("failed to create cipher: {e}")))?;

    let nonce: &[u8; AES_GCM_NONCE_SIZE] = encrypted
        .nonce
        .as_slice()
        .try_into()
        .expect("nonce length already validated");

    cipher
        .decrypt(nonce.into(), encrypted.payload.as_slice())
        .map_err(|e| CodecError::Decode(format!("decryption failed: {e}")))
}

/// Computes the SHA-256 hash of the given bytes.
pub fn sha256(bytes: &[u8]) -> Vec<u8> {
    use sha2::Digest;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hasher.finalize().to_vec()
}

/// Derives an AES-256 key from a secret and salt using HKDF-SHA256.
fn derive_key(secret: &[u8], salt: &[u8]) -> Result<[u8; 32], CodecError> {
    let hkdf = Hkdf::<Sha256>::new(Some(salt), secret);

    let mut key = [0u8; 32];
    // Empty info, matching the TypeScript implementation
    hkdf.expand(&[], &mut key)
        .map_err(|e| CodecError::Encode(format!("HKDF key derivation failed: {e}")))?;

    Ok(key)
}

#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encrypt_decrypt_roundtrip() {
        let secret: [u8; SECRET_SIZE] = xmtp_common::rand_array();
        let plaintext = b"Hello, XMTP remote attachments!";

        let encrypted = encrypt(plaintext, &secret).unwrap();
        let decrypted = decrypt(&encrypted, &secret).unwrap();

        assert_eq!(plaintext.as_slice(), decrypted.as_slice());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_decrypt_wrong_secret_fails() {
        let secret: [u8; SECRET_SIZE] = xmtp_common::rand_array();
        let wrong_secret: [u8; SECRET_SIZE] = xmtp_common::rand_array();
        let plaintext = b"Secret message";

        let encrypted = encrypt(plaintext, &secret).unwrap();
        let result = decrypt(&encrypted, &wrong_secret);

        assert!(result.is_err());
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encrypt_produces_different_output_each_time() {
        let secret: [u8; SECRET_SIZE] = xmtp_common::rand_array();
        let plaintext = b"Same message";

        let encrypted1 = encrypt(plaintext, &secret).unwrap();
        let encrypted2 = encrypt(plaintext, &secret).unwrap();

        // Due to random salt and nonce, encrypted output should differ
        assert_ne!(encrypted1.payload, encrypted2.payload);
        assert_ne!(encrypted1.salt, encrypted2.salt);
        assert_ne!(encrypted1.nonce, encrypted2.nonce);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_encrypted_payload_sizes() {
        let secret: [u8; SECRET_SIZE] = xmtp_common::rand_array();
        let plaintext = b"Test message";

        let encrypted = encrypt(plaintext, &secret).unwrap();

        assert_eq!(encrypted.salt.len(), HKDF_SALT_SIZE);
        assert_eq!(encrypted.nonce.len(), AES_GCM_NONCE_SIZE);
        // Ciphertext is plaintext + authentication tag
        assert_eq!(encrypted.payload.len(), plaintext.len() + AES_GCM_TAG_SIZE);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_invalid_salt_length() {
        let secret: [u8; SECRET_SIZE] = xmtp_common::rand_array();
        let encrypted = EncryptedPayload {
            payload: vec![0u8; 32],
            salt: vec![0u8; 16], // Wrong size
            nonce: vec![0u8; AES_GCM_NONCE_SIZE],
        };

        let result = decrypt(&encrypted, &secret);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid salt length")
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn test_invalid_nonce_length() {
        let secret: [u8; SECRET_SIZE] = xmtp_common::rand_array();
        let encrypted = EncryptedPayload {
            payload: vec![0u8; 32],
            salt: vec![0u8; HKDF_SALT_SIZE],
            nonce: vec![0u8; 8], // Wrong size
        };

        let result = decrypt(&encrypted, &secret);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("invalid nonce length")
        );
    }
}
