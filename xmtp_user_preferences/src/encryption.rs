use xmtp_proto::xmtp::message_contents::{
    ciphertext::{Aes256gcmHkdfsha256, Union},
    Ciphertext,
};
use xmtp_v2::encryption::{decrypt, encrypt, hkdf};

const PRIVATE_PREFERENCES_ENCRYPTION_KEY_SALT: &[u8] = b"XMTP_PRIVATE_PREFERENCES_ENCRYPTION";

fn derive_encryption_key(private_key: &[u8]) -> Result<[u8; 32], String> {
    let derived_key = hkdf(private_key, PRIVATE_PREFERENCES_ENCRYPTION_KEY_SALT)?;

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
