use libsecp256k1::{PublicKey, SecretKey};
use xmtp_cryptography::hash::sha256_bytes;
use xmtp_proto::xmtp::message_contents::{
    ciphertext::{Aes256gcmHkdfsha256, Union},
    Ciphertext,
};
use xmtp_v2::encryption::{decrypt, encrypt, hkdf};

use crate::CodecError;

pub const ENCODED_CONTENT_ENCRYPTION_KEY_SALT: &[u8] = b"XMTP_ENCODED_CONTENT_ENCRYPTION";

pub struct EncryptAttachmentResult {
    pub secret: Vec<u8>,
    pub content_digest: String,
    pub nonce: Vec<u8>,
    pub payload: Vec<u8>,
    pub salt: Vec<u8>,
    pub content_length_kb: Option<u32>,
    pub filename: Option<String>,
}

pub fn encrypt_bytes_for_remote_attachment(
    bytes: Vec<u8>,
    filename: Option<String>,
) -> Result<EncryptAttachmentResult, String> {
    let private_key = SecretKey::random(&mut rand::thread_rng());
    let public_key = PublicKey::from_secret_key(&private_key);
    let ciphertext: Ciphertext =
        encrypt_to_ciphertext(&private_key.serialize(), &bytes, &public_key.serialize())?;

    // Get the payload from the ciphertext
    let payload = match &ciphertext.union {
        Some(Union::Aes256GcmHkdfSha256(aes)) => &aes.payload,
        _ => return Err("Invalid ciphertext format".to_string()),
    };

    let nonce = match &ciphertext.union {
        Some(Union::Aes256GcmHkdfSha256(aes)) => &aes.gcm_nonce,
        _ => return Err("Invalid ciphertext format".to_string()),
    };

    let salt = match &ciphertext.union {
        Some(Union::Aes256GcmHkdfSha256(aes)) => &aes.hkdf_salt,
        _ => return Err("Invalid ciphertext format".to_string()),
    };

    // Calculate digest and convert to hex string
    let digest = sha256_bytes(payload);
    let content_digest = hex::encode(digest);

    Ok(EncryptAttachmentResult {
        content_digest,
        secret: private_key.serialize().to_vec(),
        salt: salt.clone(),
        nonce: nonce.clone(),
        payload: payload.clone(),
        content_length_kb: Some(bytes_to_kb(payload.len())),
        filename,
    })
}

pub fn decrypt_remote_attachment_to_bytes(
    encrypted_attachment: EncryptAttachmentResult,
) -> Result<Vec<u8>, CodecError> {
    // Reconstruct keys from the stored secret
    let secret_key_bytes: [u8; 32] = encrypted_attachment
        .secret
        .try_into()
        .map_err(|_| CodecError::Decode("Secret key must be exactly 32 bytes".to_string()))?;

    let secret_key = SecretKey::parse(&secret_key_bytes)
        .map_err(|e| CodecError::Decode(format!("Failed to parse secret key: {}", e)))?;
    let public_key = PublicKey::from_secret_key(&secret_key);

    let ciphertext = Ciphertext {
        union: Some(Union::Aes256GcmHkdfSha256(Aes256gcmHkdfsha256 {
            hkdf_salt: encrypted_attachment.salt,
            gcm_nonce: encrypted_attachment.nonce,
            payload: encrypted_attachment.payload,
        })),
    };

    let decrypted =
        decrypt_ciphertext(&secret_key.serialize(), ciphertext, &public_key.serialize())
            .map_err(|e| CodecError::Decode(format!("Failed to decrypt ciphertext: {}", e)))?;

    Ok(decrypted)
}

fn derive_encryption_key(private_key: &[u8]) -> Result<[u8; 32], String> {
    let derived_key = hkdf(private_key, ENCODED_CONTENT_ENCRYPTION_KEY_SALT)?;

    Ok(derived_key)
}

pub fn encrypt_to_ciphertext(
    private_key: &[u8],
    message: &[u8],
    additional_data: &[u8],
) -> Result<Ciphertext, String> {
    let secret_key = derive_encryption_key(private_key)?;
    let raw_ciphertext = encrypt(message, &secret_key, Some(additional_data))?;

    Ok(Ciphertext {
        union: Some(Union::Aes256GcmHkdfSha256(Aes256gcmHkdfsha256 {
            hkdf_salt: raw_ciphertext.hkdf_salt,
            gcm_nonce: raw_ciphertext.gcm_nonce,
            payload: raw_ciphertext.payload,
        })),
    })
}

pub fn decrypt_ciphertext(
    private_key: &[u8],
    ciphertext: Ciphertext,
    additional_data: &[u8],
) -> Result<Vec<u8>, String> {
    let encryption_key = derive_encryption_key(private_key)?;
    let unwrapped = unwrap_ciphertext(ciphertext)?;

    let decrypted = decrypt(
        unwrapped.payload.as_slice(),
        unwrapped.hkdf_salt.as_slice(),
        unwrapped.gcm_nonce.as_slice(),
        &encryption_key,
        Some(additional_data),
    )?;
    Ok(decrypted)
}

fn unwrap_ciphertext(ciphertext: Ciphertext) -> Result<Aes256gcmHkdfsha256, String> {
    match ciphertext.union {
        Some(Union::Aes256GcmHkdfSha256(data)) => Ok(data),
        _ => Err("unrecognized format".to_string()),
    }
}

fn bytes_to_kb(bytes: usize) -> u32 {
    (bytes / 1000) as u32
}
