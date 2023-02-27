use std::sync::Mutex;
use std::collections::HashMap;

use wasm_bindgen::prelude::*;
use xmtp_keystore::Keystore;
use js_sys::{Array, Uint8Array};

#[macro_use]
extern crate lazy_static;

lazy_static! {
    static ref KEYSTORE_MAP: Mutex<HashMap<String, Keystore>> = Mutex::new(HashMap::new());
}

// Returns a handle to a keystore instance
#[wasm_bindgen]
pub fn new_keystore() -> String {
    let mut keystore = KEYSTORE_MAP.lock().unwrap();
    let handle = (keystore.len() as u64).to_string();
    keystore.insert(handle.clone(), Keystore::new());
    return handle;
}

#[wasm_bindgen]
pub fn set_private_key_bundle(handle: &str, key_bytes: &[u8]) -> Result<bool, JsValue> {
    KEYSTORE_MAP
        .lock()
        .unwrap()
        .get_mut(handle)
        .unwrap()
        .set_private_key_bundle(key_bytes)
        .map_err(|e| e.to_string());
    Ok(true)
}

#[wasm_bindgen]
pub fn save_invitation(handle: &str, invite_bytes: &[u8]) -> Result<bool, JsValue> {
    KEYSTORE_MAP
        .lock()
        .unwrap()
        .get_mut(handle)
        .unwrap()
        .save_invitation(invite_bytes)
        .map_err(|e| e.to_string());
    Ok(true)
}

#[wasm_bindgen]
pub fn save_invites(handle: &str, save_invite_request: &[u8]) -> Result<Vec<u8>, JsValue> {
    KEYSTORE_MAP
        .lock()
        .unwrap()
        .get_mut(handle)
        .unwrap()
        .save_invites(save_invite_request)
        .map_err(|e| JsValue::from(&e.to_string()))
}

#[wasm_bindgen]
pub fn get_v2_conversations(handle: &str) -> Result<Array, JsValue> {
    let conversations = KEYSTORE_MAP
        .lock()
        .unwrap()
        .get_mut(handle)
        .unwrap()
        .get_v2_conversations()
        .map_err(|e| JsValue::from(e.to_string()));
    // Cast Vec<Vec<u8>> to Array<Vec<u8>>
    let array = Array::new();
    for conversation in conversations.unwrap() {
        array.push(&Uint8Array::from(conversation.as_slice()));
    }
    Ok(array)
}

#[wasm_bindgen]
pub fn get_topic_key(handle: &str, topic_id: &str) -> Option<Vec<u8>> {
    KEYSTORE_MAP
        .lock()
        .unwrap()
        .get(handle)?
        .get_topic_key(topic_id)
}

#[wasm_bindgen]
pub fn decrypt_v1(handle: &str, decrypt_request_bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
    KEYSTORE_MAP
        .lock()
        .unwrap()
        .get(handle)
        .unwrap()
        .decrypt_v1(decrypt_request_bytes)
        .map_err(|e| JsValue::from(e.to_string()))
}

#[wasm_bindgen]
pub fn decrypt_v2(handle: &str, decrypt_request_bytes: &[u8]) -> Result<Vec<u8>, JsValue> {
    KEYSTORE_MAP
        .lock()
        .unwrap()
        .get(handle)
        .unwrap()
        .decrypt_v2(decrypt_request_bytes)
        .map_err(|e| JsValue::from(e.to_string()))
}
