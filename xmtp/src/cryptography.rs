use aes::cipher::{KeyIvInit, StreamCipher};
use aes::Aes256;
use ethers_core::k256::elliptic_curve::subtle::ConstantTimeEq;
use hkdf::hmac::{Hmac, Mac};
use sha2::Sha256;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum EncryptionError {
    #[error("bad data")]
    BadKeyOrIv,
    #[error("unknown error")]
    Unknown,
}

#[derive(Debug, Error)]
pub enum DecryptionError {
    #[error("bad ciphertext {0}")]
    BadCiphertext(String),
    #[error("bad data")]
    BadKeyOrIv,
    #[error("unknown error")]
    Unknown,
}

pub(crate) fn aes_256_ctr_encrypt(ptext: &[u8], key: &[u8]) -> Result<Vec<u8>, EncryptionError> {
    let key: [u8; 32] = key.try_into().map_err(|_| EncryptionError::BadKeyOrIv)?;

    let zero_nonce = [0u8; 16];
    let mut cipher = ctr::Ctr32BE::<Aes256>::new(key[..].into(), zero_nonce[..].into());

    let mut ctext = ptext.to_vec();
    cipher.apply_keystream(&mut ctext);
    Ok(ctext)
}

fn aes_256_ctr_decrypt(ctext: &[u8], key: &[u8]) -> Result<Vec<u8>, DecryptionError> {
    aes_256_ctr_encrypt(ctext, key).map_err(|e| match e {
        EncryptionError::BadKeyOrIv => DecryptionError::BadKeyOrIv,
        EncryptionError::Unknown => DecryptionError::Unknown,
    })
}

pub(crate) fn hmac_sha256(key: &[u8], input: &[u8]) -> [u8; 32] {
    let mut hmac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC-SHA256 failed to create");
    hmac.update(input);
    hmac.finalize().into_bytes().into()
}

pub(crate) fn aes256_ctr_hmac_sha256_encrypt(
    msg: &[u8],
    cipher_key: &[u8],
    mac_key: &[u8],
) -> Result<Vec<u8>, EncryptionError> {
    let mut ctext = aes_256_ctr_encrypt(msg, cipher_key)?;
    let mac = hmac_sha256(mac_key, &ctext);
    ctext.extend_from_slice(&mac[..10]);
    Ok(ctext)
}

pub(crate) fn aes256_ctr_hmac_sha256_decrypt(
    ciphertext: &[u8],
    cipher_key: &[u8],
    mac_key: &[u8],
) -> Result<Vec<u8>, DecryptionError> {
    if ciphertext.len() < 10 {
        return Err(DecryptionError::BadCiphertext(
            "truncated ciphertext".to_string(),
        ));
    }
    let ptext_len = ciphertext.len() - 10;
    let our_mac = hmac_sha256(mac_key, &ciphertext[..ptext_len]);
    let same: bool = our_mac[..10].ct_eq(&ciphertext[ptext_len..]).into();
    if !same {
        return Err(DecryptionError::BadCiphertext(
            "MAC verification failed".to_string(),
        ));
    }
    aes_256_ctr_decrypt(&ciphertext[..ptext_len], cipher_key)
}
