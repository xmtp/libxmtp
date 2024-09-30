mod constants;
mod wasm;

use js_sys::{Object, Uint8Array, WebAssembly::Memory};
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;
use wasm_bindgen::{prelude::*, JsValue};

pub use constants::*;
pub use wasm::*;
// WASM is ran in the browser thread, either main or worker`. Tokio is only a single-threaded runtime.
// We need SQLite available globally, so this should be ok until we get threads with WASI or
// something.
unsafe impl Send for SQLite {}
unsafe impl Sync for SQLite {}

/// The SQLite Library
/// this global constant references the loaded SQLite WASM.
pub(super) static SQLITE: OnceCell<SQLite> = OnceCell::const_new();

// it should be possible to:
// - shared WASM memory between us and SQLite, thereby reducing allocation overhead
// - Instantiate the WebAssembly.Module + WebAssembly.Instance from Rust (this could enable sharing
// of memory)
// - SQLite OpfsVfs just needs to be instantiated from WASM
//     - OpfsVfs instantiation would be a one-time cost
// - this would make things overall more efficient since we wouldn't
// have to go through JS/browser at all.

/// the raw WASM bytes
pub(super) const WASM: &[u8] = include_bytes!("js/sqlite3.wasm");

/// Options for instantiating memory constraints
#[derive(Serialize, Deserialize)]
struct MemoryOpts {
    initial: u32,
    maximum: u32,
}
/// Opts for the WASM Module
#[derive(Serialize, Deserialize)]
struct Opts {
    /// The Sqlite3 WASM blob, compiled from C
    #[serde(rename = "wasmBinary")]
    wasm_binary: &'static [u8],
    /// the shared WebAssembly Memory buffer
    /// this allows us to manipulate the WASM memory from rust
    #[serde(with = "serde_wasm_bindgen::preserve", rename = "wasmMemory")]
    wasm_memory: Memory,
    /// The URI for the OPFS async proxy.
    #[serde(rename = "proxyUri")]
    proxy_uri: String,
}

/// Copy the contents of this wasms typed array into SQLite's memory.
///
/// This function will efficiently copy the memory from a typed
/// array into this wasm module's own linear memory, initializing
/// the memory destination provided.
///
/// # Safety
///
/// This function requires `dst` to point to a buffer
/// large enough to fit this array's contents.
pub fn raw_copy_to_sqlite<B: Into<Uint8Array>>(bytes: B, dst: *mut u8) {
    let wasm = get_sqlite_unchecked().inner().wasm();
    let bytes: Uint8Array = bytes.into();
    let wasm_sqlite_mem = wasm.heap8u();
    let offset = dst as usize / std::mem::size_of::<u8>();
    wasm_sqlite_mem.set(&bytes, offset as u32);
}

/// Copy the contents of this SQLite bytes this Wasms memory.
///
/// This function will efficiently copy the memory from a typed
/// array into this wasm module's own linear memory, initializing
/// the memory destination provided.
///
/// # Safety
///
/// This function requires `buf` to point to a buffer
/// large enough to fit this array's contents.
pub unsafe fn raw_copy_from_sqlite(src: *mut u8, len: u32, buf: &mut [u8]) {
    let wasm = crate::get_sqlite_unchecked().inner().wasm();
    let mem = wasm.heap8u();
    // this is safe because we view the slice and immediately copy it into
    // our memory.
    let view = Uint8Array::new_with_byte_offset_and_length(&mem.buffer(), src as u32, len);
    view.raw_copy_to_ptr(buf.as_mut_ptr())
}

pub async fn init_sqlite() {
    SQLITE
        .get_or_init(|| async {
            let mem = serde_wasm_bindgen::to_value(&MemoryOpts {
                initial: 16_777_216 / 65_536,
                maximum: 2_147_483_648 / 65_536,
            })
            .expect("Serialization must be infallible for const struct");
            let mem = Memory::new(&js_sys::Object::from(mem))
                .expect("Wasm Memory could not be instantiated");
            let opts = serde_wasm_bindgen::to_value(&Opts {
                wasm_binary: WASM,
                wasm_memory: mem,
                proxy_uri: wasm_bindgen::link_to!(module = "/src/js/sqlite3-opfs-async-proxy.js"),
            })
            .expect("serialization must be infallible for const struct");
            let opts = Object::from(opts);
            let object = SQLite::init_module(&opts).await;
            let sqlite3 = SQLite::new(object);
            let version: crate::ffi::Version = serde_wasm_bindgen::from_value(sqlite3.version())
                .expect("Version unexpected format");
            tracing::info!(
                "SQLite initialized. version={}, download_version={}",
                version.lib_version,
                version.download_version
            );

            sqlite3
        })
        .await;
}

pub(super) fn get_sqlite_unchecked() -> &'static SQLite {
    SQLITE.get().expect("SQLite is not initialized")
}

#[wasm_bindgen]
#[derive(Serialize, Deserialize, Debug)]
struct Version {
    #[serde(rename = "libVersion")]
    lib_version: String,
    #[serde(rename = "libVersionNumber")]
    lib_version_number: u32,
    #[serde(rename = "sourceId")]
    source_id: String,
    #[serde(rename = "downloadVersion")]
    download_version: u32,
}

/// Direct Sqlite3 bindings
#[wasm_bindgen(module = "/src/js/wa-sqlite-diesel-bundle.js")]
extern "C" {
    #[derive(Debug)]
    pub type SQLite;

    #[derive(Debug)]
    #[wasm_bindgen(extends = SQLite)]
    pub type Inner;

    #[wasm_bindgen(method, getter, js_name = "sqlite3")]
    pub fn inner(this: &SQLite) -> Inner;

    #[wasm_bindgen(method, getter)]
    pub fn wasm(this: &Inner) -> Wasm;

    #[wasm_bindgen(method, getter)]
    pub fn capi(this: &Inner) -> CApi;

    #[wasm_bindgen(static_method_of = SQLite)]
    pub async fn init_module(module: &Object) -> JsValue;

    #[wasm_bindgen(constructor)]
    pub fn new(module: JsValue) -> SQLite;

    #[wasm_bindgen(method)]
    pub fn version(this: &SQLite) -> JsValue;

    #[wasm_bindgen(method)]
    pub fn filename(this: &SQLite, db: &JsValue, name: String) -> String;

    #[wasm_bindgen(method)]
    pub fn errstr(this: &SQLite, code: i32) -> String;

    #[wasm_bindgen(method)]
    pub fn errmsg(this: &SQLite, conn: &JsValue) -> String;

    #[wasm_bindgen(method)]
    pub fn extended_errcode(this: &SQLite, conn: &JsValue) -> i32;

    #[wasm_bindgen(method)]
    pub fn result_text(this: &SQLite, context: i32, value: String);

    #[wasm_bindgen(method)]
    pub fn result_int(this: &SQLite, context: i32, value: i32);

    #[wasm_bindgen(method)]
    pub fn result_int64(this: &SQLite, context: i32, value: i64);

    #[wasm_bindgen(method)]
    pub fn result_double(this: &SQLite, context: i32, value: f64);

    #[wasm_bindgen(method)]
    pub fn result_blob(this: &SQLite, context: i32, value: Vec<u8>);

    #[wasm_bindgen(method)]
    pub fn result_null(this: &SQLite, context: i32);

    #[wasm_bindgen(method)]
    pub fn bind_parameter_count(this: &SQLite, stmt: &JsValue) -> i32;

    #[wasm_bindgen(method)]
    pub fn bind_parameter_name(this: &SQLite, stmt: &JsValue, idx: i32) -> String;

    #[wasm_bindgen(method)]
    pub fn bind_null(this: &SQLite, stmt: &JsValue, idx: i32) -> i32;

    #[wasm_bindgen(method)]
    pub fn bind_text(
        this: &SQLite,
        stmt: &JsValue,
        idx: i32,
        ptr: *mut u8,
        len: i32,
        flags: i32,
    ) -> i32;

    #[wasm_bindgen(method)]
    pub fn bind_blob(
        this: &SQLite,
        stmt: &JsValue,
        idx: i32,
        ptr: *mut u8,
        len: i32,
        flags: i32,
    ) -> i32;

    #[wasm_bindgen(method)]
    pub fn bind_double(this: &SQLite, stmt: &JsValue, idx: i32, value: f64) -> i32;

    #[wasm_bindgen(method)]
    pub fn bind_int(this: &SQLite, stmt: &JsValue, idx: i32, value: i32) -> i32;

    #[wasm_bindgen(method)]
    pub fn bind_int64(this: &SQLite, stmt: &JsValue, idx: i32, value: i64) -> i32;

    #[wasm_bindgen(method)]
    pub fn reset(this: &SQLite, stmt: &JsValue) -> i32;

    #[wasm_bindgen(method)]
    pub fn value_dup(this: &SQLite, pValue: *mut u8) -> *mut u8;

    #[wasm_bindgen(method)]
    pub fn value_blob(this: &SQLite, pValue: *mut u8) -> *mut u8;

    #[wasm_bindgen(method)]
    pub fn value_bytes(this: &SQLite, pValue: *mut u8) -> u32;

    #[wasm_bindgen(method)]
    pub fn value_double(this: &SQLite, pValue: *mut u8) -> f64;

    #[wasm_bindgen(method)]
    pub fn value_free(this: &SQLite, pValue: *mut u8);

    #[wasm_bindgen(method)]
    pub fn sqlite3_free(this: &SQLite, pValue: *mut u8);

    #[wasm_bindgen(method)]
    pub fn value_int(this: &SQLite, pValue: *mut u8) -> i32;

    #[wasm_bindgen(method)]
    pub fn value_int64(this: &SQLite, pValue: *mut u8) -> i64;

    #[wasm_bindgen(method)]
    pub fn value_text(this: &SQLite, pValue: *mut u8) -> String;

    #[wasm_bindgen(method)]
    pub fn value_type(this: &SQLite, pValue: *mut u8) -> i32;

    #[wasm_bindgen(method, catch)]
    pub fn open(this: &SQLite, database_url: &str, iflags: Option<i32>)
        -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch)]
    pub fn exec(this: &SQLite, database: &JsValue, query: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(method, catch)]
    pub fn finalize(this: &SQLite, stmt: &JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(method)]
    pub fn changes(this: &SQLite, database: &JsValue) -> usize;

    #[wasm_bindgen(method, catch)]
    pub fn get_stmt_from_iterator(this: &SQLite, iterator: &JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method)]
    pub fn step(this: &SQLite, stmt: &JsValue) -> i32;

    #[wasm_bindgen(method)]
    pub fn clear_bindings(this: &SQLite, stmt: &JsValue) -> i32;

    #[wasm_bindgen(method)]
    pub fn close(this: &SQLite, database: &JsValue) -> i32;

    #[wasm_bindgen(method)]
    pub fn db_handle(this: &SQLite, stmt: &JsValue) -> JsValue;

    #[wasm_bindgen(method)]
    pub fn column_value(this: &SQLite, stmt: &JsValue, idx: i32) -> *mut u8;

    #[wasm_bindgen(method)]
    pub fn prepare_v3(
        this: &SQLite,
        database: &JsValue,
        sql: *mut u8,
        n_byte: i32,
        prep_flags: u32,
        stmt: &JsValue,
        pzTail: &JsValue,
    ) -> i32;

    #[wasm_bindgen(method)]
    pub fn column_name(this: &SQLite, stmt: &JsValue, idx: i32) -> String;

    #[wasm_bindgen(method)]
    pub fn column_count(this: &SQLite, stmt: &JsValue) -> i32;

    #[wasm_bindgen(method, catch)]
    pub fn create_function(
        this: &SQLite,
        database: &JsValue,
        functionName: &str,
        n_arg: i32,
        textRep: i32,
        pApp: i32, //ignored
        x_func: Option<&Closure<dyn FnMut(JsValue, Vec<JsValue>) -> JsValue>>,
        x_step: Option<&Closure<dyn FnMut(JsValue, Vec<JsValue>) -> JsValue>>,
        x_final: Option<&Closure<dyn FnMut(JsValue)>>,
    ) -> Result<(), JsValue>;

    #[wasm_bindgen(method, catch)]
    pub fn register_diesel_sql_functions(this: &SQLite, database: &JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(method)]
    pub fn sqlite3_serialize(
        this: &SQLite,
        database: &JsValue,
        z_schema: &str,
        p_size: &JsValue,
        m_flags: u32,
    ) -> *mut u8;

    #[wasm_bindgen(method)]
    pub fn sqlite3_deserialize(
        this: &SQLite,
        database: &JsValue,
        z_schema: &str,
        p_data: *mut u8,
        sz_database: i64,
        sz_buffer: i64,
        m_flags: u32,
    ) -> i32;
}
