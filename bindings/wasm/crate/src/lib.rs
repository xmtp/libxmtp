use wasm_bindgen::prelude::*;

use xmtp_keystore::Keystore;

#[wasm_bindgen]
pub fn add(a: i32, b: i32) -> Result<i32, JsValue> {
    Ok(a + b)
}

#[wasm_bindgen]
pub fn generate_mnemonic() -> Result<String, JsValue> {
    let keystore = Keystore::new();
    let key = keystore.generate_mnemonic();
    Ok(key)
}
