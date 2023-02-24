use once_cell::sync::Lazy;
use std::sync::Mutex;

use wasm_bindgen::prelude::*;
use xmtp_keystore::Keystore;

// Keep the keystore class in memory
static KEYSTORE: Lazy<Mutex<Keystore>> = Lazy::new(|| Mutex::new(Keystore::new()));

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

#[wasm_bindgen]
pub fn get_topic_key(topic_id: &str) -> Option<Vec<u8>> {
    KEYSTORE.lock().unwrap().get_topic_key(topic_id)
}

#[wasm_bindgen]
pub fn decrypt_v1(decrypt_request_bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
    KEYSTORE
        .lock()
        .unwrap()
        .decrypt_v1(decrypt_request_bytes)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}

#[wasm_bindgen]
pub fn decrypt_v2(decrypt_request_bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
    KEYSTORE
        .lock()
        .unwrap()
        .decrypt_v2(decrypt_request_bytes)
        .map_err(|e| JsValue::from_str(&e.to_string()))
}
