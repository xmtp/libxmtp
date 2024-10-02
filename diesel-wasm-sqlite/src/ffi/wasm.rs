//! WASM bindings for memory management
use std::ptr::NonNull;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[derive(Debug)]
    #[wasm_bindgen(extends = super::Inner)]
    pub type Wasm;

    #[wasm_bindgen(method, js_name = "peekPtr")]
    pub fn peek_ptr(this: &Wasm, stmt: &JsValue) -> JsValue;
    /// The "pstack" (pseudo-stack) API is a special-purpose allocator
    /// intended solely for use with allocating small amounts of memory such
    /// as that needed for output pointers.
    /// It is more efficient than the scoped allocation API,
    /// and covers many of the use cases for that API, but it
    /// has a tiny static memory limit (with an unspecified total size no less than 4kb).
    #[wasm_bindgen(method, getter)]
    pub fn pstack(this: &Wasm) -> PStack;

    #[wasm_bindgen(method)]
    pub fn alloc(this: &Wasm, bytes: u32) -> *mut u8;

    #[wasm_bindgen(method, getter, js_name = "alloc")]
    pub fn alloc_inner(this: &Wasm) -> Alloc;

    /// Uses alloc() to allocate enough memory for the byte-length of the given JS string,
    /// plus 1 (for a NUL terminator), copies the given JS string to that memory using jstrcpy(),
    /// NUL-terminates it, and returns the pointer to that C-string.
    /// Ownership of the pointer is transfered to the caller, who must eventually pass the pointer to dealloc() to free it.
    //TODO: Avoid using this since it allocates in JS and other webassembly. Instead use technique
    // used in Statement::prepare
    #[wasm_bindgen(method, js_name = "allocCString")]
    pub fn alloc_cstring(this: &Wasm, string: String) -> *mut u8;

    /// Allocates one or more pointers as a single chunk of memory and zeroes them out.
    /// The first argument is the number of pointers to allocate.
    /// The second specifies whether they should use a "safe" pointer size (8 bytes)
    /// or whether they may use the default pointer size (typically 4 but also possibly 8).
    /// How the result is returned depends on its first argument: if passed 1, it returns the allocated memory address.
    /// If passed more than one then an array of pointer addresses is returned
    #[wasm_bindgen(method, js_name = "allocPtr")]
    pub fn alloc_ptr(this: &Wasm, how_many: u32, safe_ptr_size: bool) -> *mut u8;

    #[wasm_bindgen(method)]
    pub fn dealloc(this: &Wasm, ptr: NonNull<u8>);

    /// View into the wasm memory reprsented as unsigned 8-bit integers
    #[wasm_bindgen(method)]
    pub fn heap8u(this: &Wasm) -> js_sys::Uint8Array;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = Wasm)]
    pub type PStack;

    /// allocate some memory on the PStack
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
    #[wasm_bindgen(method)]
    pub fn restore(this: &PStack, ptr: &JsValue);

}

#[wasm_bindgen]
extern "C" {
    pub type Alloc;

    /// Non-throwing version of `Wasm::Alloc`
    /// returns NULL pointer if cannot allocate
    #[wasm_bindgen(method, js_name = "impl")]
    pub fn alloc_impl(this: &Alloc, bytes: u32) -> *mut u8;
}
