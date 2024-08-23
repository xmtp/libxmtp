//! WASM bindings for memory management
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    pub type Capi;

    pub static SQLITE_DONE: i32;
}
