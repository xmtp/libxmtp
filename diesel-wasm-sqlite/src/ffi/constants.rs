//! WASM Constant bindings
use std::sync::LazyLock;
use wasm_bindgen::prelude::*;

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
    ($fn_name:ident, $ty: ident) => {
        pub static $fn_name: LazyLock<$ty> = LazyLock::new(|| {
            let capi: CApi = crate::get_sqlite_unchecked().inner().capi();
            CApi::$fn_name(&capi)
        });
    };
}

generate_sqlite_constant!(SQLITE_OK, i32);

generate_sqlite_constant!(SQLITE_DONE, i32);
generate_sqlite_constant!(SQLITE_ROW, i32);

generate_sqlite_constant!(SQLITE_INTEGER, i32);
generate_sqlite_constant!(SQLITE_FLOAT, i32);
generate_sqlite_constant!(SQLITE_TEXT, i32);
generate_sqlite_constant!(SQLITE_BLOB, i32);
generate_sqlite_constant!(SQLITE_NULL, i32);

generate_sqlite_constant!(SQLITE_PREPARE_PERSISTENT, u32);

generate_sqlite_constant!(SQLITE_CONSTRAINT_PRIMARYKEY, i32);
generate_sqlite_constant!(SQLITE_CONSTRAINT_UNIQUE, i32);
generate_sqlite_constant!(SQLITE_CONSTRAINT_FOREIGNKEY, i32);
generate_sqlite_constant!(SQLITE_CONSTRAINT_NOTNULL, i32);
generate_sqlite_constant!(SQLITE_CONSTRAINT_CHECK, i32);

generate_sqlite_constant!(SQLITE_STATIC, i32);

// C-Style API Constants
#[wasm_bindgen]
extern "C" {
    /// C-Api Style bindings
    #[wasm_bindgen(extends = super::Inner)]
    pub type CApi;

    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OK(this: &CApi) -> i32;

    /// SQLite statement returns
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_DONE(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_ROW(this: &CApi) -> i32;

    // Fundamental datatypes.
    // https://www.sqlite.org/c3ref/c_blob.html
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_INTEGER(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_FLOAT(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_TEXT(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_BLOB(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_NULL(this: &CApi) -> i32;

    /// SQLite Open Flags
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_READONLY(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_READWRITE(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_CREATE(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_DELETEONCLOSE(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_EXCLUSIVE(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_AUTOPROXY(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_URI(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_MEMORY(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_MAIN_DB(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_TEMP_DB(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_TRANSIENT_DB(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_MAIN_JOURNAL(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_TEMP_JOURNAL(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_SUBJOURNAL(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_SUPER_JOURNAL(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_NOMUTEX(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_FULLMUTEX(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_SHAREDCACHE(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_PRIVATECACHE(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_WAL(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_NOFOLLOW(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_OPEN_EXRESCODE(this: &CApi) -> i32;

    // SQLite Text Encodings https://www.sqlite.org/capi3ref.html#SQLITE_ANY
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_UTF8(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_UTF16LE(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_UTF16BE(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_UTF16(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_ANY(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_UTF16_ALIGNED(this: &CApi) -> i32;

    /// SQLite Function Flags https://www.sqlite.org/capi3ref.html#sqlitedeterministic
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_DETERMINISTIC(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_DIRECTONLY(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_SUBTYPE(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_INNOCUOUS(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_RESULT_SUBTYPE(this: &CApi) -> i32;

    // SQLite Prepare Flags https://www.sqlite.org/c3ref/c_prepare_normalize.html#sqlitepreparepersistent
    #[wasm_bindgen(method, getter)]
    pub const fn SQLITE_PREPARE_PERSISTENT(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub fn SQLITE_PREPARE_NORMALIZE(this: &CApi) -> u32;
    #[wasm_bindgen(method, getter)]
    pub fn SQLITE_PREPARE_NO_VTAB(this: &CApi) -> u32;

    /// Constraint

    #[wasm_bindgen(method, getter)]
    pub fn SQLITE_CONSTRAINT_UNIQUE(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub fn SQLITE_CONSTRAINT_PRIMARYKEY(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub fn SQLITE_CONSTRAINT_FOREIGNKEY(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub fn SQLITE_CONSTRAINT_NOTNULL(this: &CApi) -> i32;
    #[wasm_bindgen(method, getter)]
    pub fn SQLITE_CONSTRAINT_CHECK(this: &CApi) -> i32;

    /// Binds
    #[wasm_bindgen(method, getter)]
    pub fn SQLITE_STATIC(this: &CApi) -> i32;
}
