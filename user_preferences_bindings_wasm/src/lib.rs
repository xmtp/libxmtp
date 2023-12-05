mod utils;

use wasm_bindgen::prelude::*;
use xmtp_user_preferences::{
    decrypt_message, encrypt_message, topic::generate_private_preferences_topic_identifier,
};

#[wasm_bindgen]
pub fn user_preferences_encrypt(
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
pub fn user_preferences_decrypt(
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

#[wasm_bindgen]
pub fn generate_private_preferences_topic(private_key: Vec<u8>) -> Result<String, String> {
    let topic = generate_private_preferences_topic_identifier(private_key.as_slice())?;

    Ok(topic)
}
