use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn add(a: i32, b: i32) -> Result<i32, JsValue> {
    Ok(a + b)
}
