use once_cell::sync::Lazy;
use std::sync::Mutex;

use wasm_bindgen::prelude::*;
use xmtp_keystore::Keystore;

// Keep the keystore class in memory
static KEYSTORE: Lazy<Mutex<Keystore>> = Lazy::new(|| Mutex::new(Keystore::new()));

#[wasm_bindgen]
pub fn add_two_numbers(left: usize, right: usize) -> usize {
    libxmtp_core::add(left, right)
}

#[wasm_bindgen]
pub fn set_private_key_bundle(key_bytes: &[u8]) -> Result<bool, JsValue> {
    KEYSTORE
        .lock()
        .unwrap()
        .set_private_key_bundle(key_bytes)
        .map_err(|e| JsValue::from_str(&e.to_string()));
    Ok(true)
}

#[wasm_bindgen]
pub fn save_invitation(invite_bytes: &[u8]) -> Result<bool, JsValue> {
    KEYSTORE
        .lock()
        .unwrap()
        .save_invitation(invite_bytes)
        .map_err(|e| JsValue::from_str(&e.to_string()));
    Ok(true)
}
