//! WASM bindings for memory management
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    pub type Wasm;

    #[wasm_bindgen(extends = Wasm)]
    type PStack;

    /// allocate some memory on the WASM stack
    #[wasm_bindgen(method)]
    pub fn alloc(this: &PStack, bytes: u32) -> JsValue;

    /// Resolves the current pstack position pointer.
    /// should only be used in argument for `restore`
    #[wasm_bindgen(method, getter)]
    pub fn pointer(this: &PStack) -> JsValue;

    /// resolves to total number of bytes available in pstack, including any
    /// space currently allocated. compile-time constant
    #[wasm_bindgen(method, getter)]
    pub fn quota(this: &PStack) -> u32;

    // Property resolves to the amount of space remaining in the pstack
    #[wasm_bindgen(method, getter)]
    pub fn remaining(this: &PStack) -> u32;

    /// sets current pstack
    pub fn restore(this: &PStack);

}
