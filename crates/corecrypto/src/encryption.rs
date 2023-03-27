use hkdf::Hkdf;
use rand::Rng;
use sha2::Sha256;

use generic_array::GenericArray;

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm,
    Nonce,
};

// Lightweight ciphertext holder
pub struct Ciphertext {
    pub payload: Vec<u8>,
    pub hkdf_salt: Vec<u8>,
    pub gcm_nonce: Vec<u8>,
}

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

pub fn decrypt(
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
fn decrypt_raw(
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

pub fn encrypt(
    plaintext_bytes: &[u8],
    secret_bytes: &[u8],
    additional_data: Option<&[u8]>,
) -> Result<Ciphertext, String> {
    // Form a Payload struct from plaintext_bytes and additional_data if it's present
    let mut payload = Payload::from(plaintext_bytes);
    if additional_data.is_some() {
        payload.aad = additional_data.unwrap();
    }
    return encrypt_raw(payload, secret_bytes);
}

fn encrypt_raw(payload: Payload, secret_bytes: &[u8]) -> Result<Ciphertext, String> {
    let salt_bytes = rand::thread_rng().gen::<[u8; 32]>();
    let nonce_bytes = rand::thread_rng().gen::<[u8; 12]>();
    let derived_key = hkdf(secret_bytes, &salt_bytes)?;
    let key = Aes256Gcm::new(GenericArray::from_slice(&derived_key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let res = key.encrypt(nonce, payload);
    if res.is_err() {
        return Err(res.err().unwrap().to_string());
    }
    let ciphertext_bytes = res.unwrap();
    Ok(Ciphertext {
        payload: ciphertext_bytes,
        hkdf_salt: salt_bytes.to_vec(),
        gcm_nonce: nonce_bytes.to_vec(),
    })
}
