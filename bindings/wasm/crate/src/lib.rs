use wasm_bindgen::prelude::*;

use xmtp_keystore::Keystore;

#[wasm_bindgen]
pub fn set_private_key_bundle(key_bytes: &[u8]) -> Result<bool, JsValue> {
    let mut keystore = Keystore::new();
    keystore.set_private_key_bundle(key_bytes).map_err(|e| JsValue::from_str(&e.to_string()));
    Ok(true)
}
