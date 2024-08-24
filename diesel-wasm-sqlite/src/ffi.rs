mod wasm;

use js_sys::WebAssembly::Memory;
use serde::{Deserialize, Serialize};
use std::cell::LazyCell;
use tokio::sync::OnceCell;
use wasm_bindgen::{prelude::*, JsValue};

pub use wasm::*;
// WASM is ran in the browser thread, either main or worker`. Tokio is only a single-threaded runtime.
// We need SQLite available globally, so this should be ok until we get threads with WASI or
// something.
unsafe impl Send for SQLite {}
unsafe impl Sync for SQLite {}

/// The SQLite Library
/// this global constant references the loaded SQLite WASM.
pub(super) static SQLITE: OnceCell<SQLite> = OnceCell::const_new();

pub(super) const WASM: &[u8] =
    include_bytes!("../node_modules/@sqlite.org/sqlite-wasm/sqlite-wasm/jswasm/sqlite3.wasm");

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

pub(super) const WASM_MEMORY: LazyCell<Memory> = LazyCell::new(|| {
    let mem = serde_wasm_bindgen::to_value(&MemoryOpts {
        initial: 16_777_216 / 65_536,
        maximum: 2_147_483_648 / 65_536,
    })
    .expect("Serialization must be infallible for const struct");
    Memory::new(&js_sys::Object::from(mem)).expect("Wasm Memory could not be instantiated")
});

pub async fn init_sqlite() {
    SQLITE
        .get_or_init(|| async {
            let opts = serde_wasm_bindgen::to_value(&Opts {
                wasm_binary: WASM,
                wasm_memory: WASM_MEMORY.clone(),
                proxy_uri: wasm_bindgen::link_to!(module = "/src/sqlite3-opfs-async-proxy.js"),
            })
            .expect("serialization must be infallible for const struct");
            let opts = js_sys::Object::from(opts);
            let module = SQLite::init_module(WASM, &opts).await;
            SQLite::new(module)
        })
        .await;
}

pub(super) fn get_sqlite_unchecked() -> &'static SQLite {
    SQLITE.get().expect("SQLite is not initialized")
}

#[wasm_bindgen(typescript_custom_section)]
const SQLITE_COMPATIBLE_TYPE: &'static str =
    r#"type SQLiteCompatibleType = number|string|Uint8Array|Array<number>|bigint|null"#;

#[wasm_bindgen]
extern "C" {
    #[derive(Debug, Clone)]
    #[wasm_bindgen(typescript_type = "SQLiteCompatibleType")]
    pub type SQLiteCompatibleType;
}

/// Direct Sqlite3 bindings
#[wasm_bindgen(module = "/src/wa-sqlite-diesel-bundle.js")]
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

    #[wasm_bindgen(constructor)]
    pub fn new(module: JsValue) -> SQLite;

    #[wasm_bindgen(static_method_of = SQLite)]
    pub async fn init_module(wasm: &[u8], opts: &js_sys::Object) -> JsValue;

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

    #[wasm_bindgen(method, catch)]
    pub fn bind(
        this: &SQLite,
        stmt: &JsValue,
        idx: i32,
        value: SQLiteCompatibleType,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method)]
    pub fn bind_parameter_count(this: &SQLite, stmt: &JsValue) -> i32;

    #[wasm_bindgen(method)]
    pub fn bind_parameter_name(this: &SQLite, stmt: &JsValue, idx: i32) -> String;

    #[wasm_bindgen(method, catch)]
    pub fn bind_text(this: &SQLite, stmt: &JsValue, idx: i32, value: &str) -> Result<i32, JsValue>;

    #[wasm_bindgen(method, catch)]
    pub fn reset(this: &SQLite, stmt: &JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method)]
    pub fn value(this: &SQLite, pValue: &JsValue) -> SQLiteCompatibleType;

    #[wasm_bindgen(method)]
    pub fn value_dup(this: &SQLite, pValue: &JsValue) -> SQLiteCompatibleType;

    #[wasm_bindgen(method)]
    pub fn value_blob(this: &SQLite, pValue: &JsValue) -> Vec<u8>;

    #[wasm_bindgen(method)]
    pub fn value_bytes(this: &SQLite, pValue: &JsValue) -> i32;

    #[wasm_bindgen(method)]
    pub fn value_double(this: &SQLite, pValue: &JsValue) -> f64;

    #[wasm_bindgen(method)]
    pub fn value_int(this: &SQLite, pValue: &JsValue) -> i32;

    #[wasm_bindgen(method)]
    pub fn value_int64(this: &SQLite, pValue: &JsValue) -> i64;

    // TODO: If wasm-bindgen allows returning references, could return &str
    #[wasm_bindgen(method)]
    pub fn value_text(this: &SQLite, pValue: &JsValue) -> String;

    #[wasm_bindgen(method)]
    pub fn value_type(this: &SQLite, pValue: &JsValue) -> u32;

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

    #[wasm_bindgen(method, catch)]
    pub fn step(this: &SQLite, stmt: &JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch)]
    pub fn clear_bindings(this: &SQLite, stmt: &JsValue) -> Result<i32, JsValue>;

    #[wasm_bindgen(method, catch)]
    pub fn close(this: &SQLite, database: &JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(method)]
    pub fn column(this: &SQLite, stmt: &JsValue, idx: i32) -> SQLiteCompatibleType;

    #[wasm_bindgen(method, catch)]
    pub fn prepare_v3(
        this: &SQLite,
        database: &JsValue,
        sql: &str,
        n_byte: i32,
        prep_flags: u32,
        stmt: &JsValue,
        pzTail: &JsValue,
    ) -> Result<JsValue, JsValue>;

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
    pub fn value_free(this: &SQLite, value: &JsValue);

}
