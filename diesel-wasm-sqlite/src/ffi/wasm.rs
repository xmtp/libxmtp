//! WASM bindings for memory management
use std::cell::LazyCell;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[derive(Debug)]
    #[wasm_bindgen(extends = super::Inner)]
    pub type Wasm;

    #[wasm_bindgen(method, js_name = "peekPtr")]
    pub fn peek_ptr(this: &Wasm, stmt: &JsValue) -> JsValue;

    #[wasm_bindgen(method, getter)]
    pub fn pstack(this: &Wasm) -> PStack;
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(extends = Wasm)]
    pub type PStack;

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
    #[wasm_bindgen(method)]
    pub fn restore(this: &PStack, ptr: &JsValue);

}
/// Normally, statics or methods cannot be pattern-matched.
/// Pattern matching also does not automatically dereference.
/// constants are imported into wasm-bindgen as statics and/or const
/// methods. We use a combination of `LazyCell`
/// and exported wasm getters to achieve some kind of
/// pattern-matching syntax
/// ```
///     match variable {
///         v if *ffi::SQLITE_DONE == v {
///             /* SQLITE_DONE */
///         },
///         v if *ffi:SQLITE_ROW == v {
///             /* SQLITE_ROW */
///         }
///     }
/// ```
///
/// This is also a micro-optimization,
/// These constants will be initialized exactly once, rather
/// than on every access thus reducing the context-switching between wasm-js barrier.
macro_rules! generate_sqlite_constant {
    ($fn_name:ident) => {
        pub const $fn_name: LazyCell<u32> = LazyCell::new(|| {
            let capi: CApi = crate::get_sqlite_unchecked().inner().capi();
            CApi::$fn_name(&capi)
        });
    };
}
generate_sqlite_constant!(SQLITE_DONE);
generate_sqlite_constant!(SQLITE_ROW);

generate_sqlite_constant!(SQLITE_INTEGER);
generate_sqlite_constant!(SQLITE_FLOAT);
generate_sqlite_constant!(SQLITE_TEXT);
generate_sqlite_constant!(SQLITE_BLOB);
generate_sqlite_constant!(SQLITE_NULL);

generate_sqlite_constant!(SQLITE_PREPARE_PERSISTENT);

// C-Style API Constants
#[wasm_bindgen]
extern "C" {
    /// C-Api Style bindings
    #[wasm_bindgen(extends = super::Inner)]
    pub type CApi;

    /// SQLite statement returns
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_DONE(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_ROW(this: &CApi) -> u32;

    // Fundamental datatypes.
    // https://www.sqlite.org/c3ref/c_blob.html
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_INTEGER(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_FLOAT(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_TEXT(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_BLOB(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_NULL(this: &CApi) -> u32;

    /// SQLite Open Flags
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_READONLY(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_READWRITE(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_CREATE(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_DELETEONCLOSE(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_EXCLUSIVE(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_AUTOPROXY(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_URI(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_MEMORY(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_MAIN_DB(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_TEMP_DB(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_TRANSIENT_DB(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_MAIN_JOURNAL(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_TEMP_JOURNAL(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_SUBJOURNAL(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_SUPER_JOURNAL(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_NOMUTEX(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_FULLMUTEX(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_SHAREDCACHE(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_PRIVATECACHE(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_WAL(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_NOFOLLOW(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_EXRESCODE(this: &CApi) -> u32;

    // SQLite Text Encodings https://www.sqlite.org/capi3ref.html#SQLITE_ANY
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_UTF8(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_UTF16LE(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_UTF16BE(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_UTF16(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_ANY(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_UTF16_ALIGNED(this: &CApi) -> u32;

    /// SQLite Function Flags https://www.sqlite.org/capi3ref.html#sqlitedeterministic
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_DETERMINISTIC(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_DIRECTONLY(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_SUBTYPE(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_INNOCUOUS(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_RESULT_SUBTYPE(this: &CApi) -> u32;

    // SQLite Prepare Flags https://www.sqlite.org/c3ref/c_prepare_normalize.html#sqlitepreparepersistent
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_PREPARE_PERSISTENT(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub fn SQLITE_PREPARE_NORMALIZE(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub fn SQLITE_PREPARE_NO_VTAB(this: &CApi) -> u32;
}
