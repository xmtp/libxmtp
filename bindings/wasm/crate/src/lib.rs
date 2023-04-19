use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn add_two_numbers(left: usize, right: usize) -> usize {
    libxmtp_core::add(left, right)
}
