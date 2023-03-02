use hkdf::Hkdf;
use rand::Rng;
use sha2::Sha256;

use generic_array::GenericArray;

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    AeadInPlace,
    Aes256Gcm,
    Nonce, // Or `Aes128Gcm`
};

pub fn hkdf(secret: &[u8], salt: &[u8]) -> Result<[u8; 32], String> {
    let hk = Hkdf::<Sha256>::new(Some(&salt), &secret);
    let mut okm = [0u8; 42];
    let res = hk.expand(&[], &mut okm);
    if res.is_err() {
        return Err(res.err().unwrap().to_string());
    }
    okm[0..32]
        .try_into()
        .map_err(|_| "hkdf failed to fit in 32 bytes".to_string())
}

// encryption::decrypt_v1_inner(payload, peer_keys, header_bytes, is_sender);
pub fn decrypt_v1(
    ciphertext_bytes: &[u8],
    salt_bytes: &[u8],
    nonce_bytes: &[u8],
    secret_bytes: &[u8],
    additional_data: Option<&[u8]>,
) -> Result<Vec<u8>, String> {
    // Form a Payload struct from ciphertext_bytes and additional_data if it's present
    let mut payload = Payload::from(ciphertext_bytes);
    if additional_data.is_some() {
        payload.aad = additional_data.unwrap();
    }
    return decrypt_raw(payload, salt_bytes, nonce_bytes, secret_bytes);
}

// Decrypt but using associated data
pub fn decrypt_raw(
    payload: Payload,
    salt_bytes: &[u8],
    nonce_bytes: &[u8],
    secret_bytes: &[u8],
) -> Result<Vec<u8>, String> {
    let derived_key = hkdf(secret_bytes, salt_bytes)?;
    let key = Aes256Gcm::new(GenericArray::from_slice(&derived_key));
    let nonce = Nonce::from_slice(nonce_bytes);
    let res = key.decrypt(nonce, payload);
    if res.is_err() {
        return Err(res.err().unwrap().to_string());
    }
    Ok(res.unwrap())
}
