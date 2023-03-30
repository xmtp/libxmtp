use wasm_bindgen::prelude::*;
use xmtpv3;


#[wasm_bindgen]
pub fn e2e_selftest() -> Result<bool, JsValue> {
    xmtpv3::e2e_selftest().map(|x| x == "Self test successful").map_err(|e| JsValue::from_str(&e.to_string()))
}
