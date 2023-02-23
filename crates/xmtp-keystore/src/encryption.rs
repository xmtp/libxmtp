use hkdf::Hkdf;
use sha2::Sha256;

use generic_array::GenericArray;

use aes_gcm::{
    aead::{Aead, KeyInit},
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
    let derived_key = hkdf(secret_bytes, salt_bytes)?;
    let key = Aes256Gcm::new(GenericArray::from_slice(&derived_key));
    let nonce = Nonce::from_slice(nonce_bytes);
    let _additional_data = additional_data.unwrap_or(&[]);
    let res = key.decrypt(nonce, ciphertext_bytes);
    if res.is_err() {
        return Err(res.err().unwrap().to_string());
    }
    Ok(res.unwrap())
}

// Decrypt but using associated data
pub fn decrypt_v1_with_associated_data(
    ciphertext_bytes: &[u8],
    salt_bytes: &[u8],
    nonce_bytes: &[u8],
    secret_bytes: &[u8],
    additional_data: &[u8],
) -> Result<Vec<u8>, String> {
    let derived_key = hkdf(secret_bytes, salt_bytes)?;
    let key = Aes256Gcm::new(GenericArray::from_slice(&derived_key));
    let nonce = Nonce::from_slice(nonce_bytes);
    // Utilize decrypt_in_place_detached to allow associated data
    let mutable_bytes = &mut ciphertext_bytes.to_vec();
    let res = key.decrypt_in_place(nonce, additional_data, mutable_bytes);
    if res.is_err() {
        return Err(res.err().unwrap().to_string());
    }
    Ok(mutable_bytes.to_vec())
}
