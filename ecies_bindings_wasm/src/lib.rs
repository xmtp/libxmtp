mod utils;

use wasm_bindgen::prelude::*;
use xmtp_ecies::signed_payload::{decrypt_message, encrypt_message};

#[wasm_bindgen]
pub fn ecies_encrypt_k256_sha3_256(
    public_key: Vec<u8>,
    private_key: Vec<u8>,
    message: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let ciphertext = encrypt_message(
        public_key.as_slice(),
        private_key.as_slice(),
        message.as_slice(),
    )?;

    Ok(ciphertext)
}

#[wasm_bindgen]
pub fn ecies_decrypt_k256_sha3_256(
    public_key: Vec<u8>,
    private_key: Vec<u8>,
    message: Vec<u8>,
) -> Result<Vec<u8>, String> {
    let decrypted = decrypt_message(
        public_key.as_slice(),
        private_key.as_slice(),
        message.as_slice(),
    )?;

    Ok(decrypted)
}
