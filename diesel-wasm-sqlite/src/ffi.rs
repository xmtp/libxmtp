use wasm_bindgen::{prelude::*, JsValue};

/*
/// Simple Connection
#[wasm_bindgen(module = "/src/package.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    pub fn batch_execute(database: &JsValue, query: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    pub async fn establish(database_url: &str) -> Result<JsValue, JsValue>;
}
*/

// WASM is ran in the browser `main thread`. Tokio is only a single-threaded runtime.
// We need SQLite available globally, so this should be ok until we get threads with WASI or
// something. At which point we can (hopefully) use multi-threaded async runtime to block the
// thread and get SQLite.
unsafe impl Send for SQLite {}
unsafe impl Sync for SQLite {}

/// Direct Shim for wa-sqlite
#[wasm_bindgen(module = "/src/package.js")]
extern "C" {
    pub type SQLite;

    #[wasm_bindgen(constructor)]
    pub fn new(module: JsValue) -> SQLite;

    #[wasm_bindgen(static_method_of = SQLite)]
    pub async fn wasm_module() -> JsValue;

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
    pub async fn open_v2(
        this: &SQLite,
        database_url: &str,
        iflags: Option<i32>,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch)]
    pub async fn exec(this: &SQLite, database: &JsValue, query: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(method)]
    pub fn changes(this: &SQLite, database: &JsValue) -> usize;

    #[wasm_bindgen(method, catch)]
    pub fn batch_execute(this: &SQLite, database: &JsValue, query: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(method, catch)]
    pub fn create_function(
        this: &SQLite,
        database: &JsValue,
        functionName: &str,
        n_arg: i32,
        textRep: i32,
        x_func: Option<&Closure<dyn FnMut(JsValue, JsValue)>>,
        x_step: Option<&Closure<dyn FnMut(JsValue, JsValue)>>,
        x_final: Option<&Closure<dyn FnMut(JsValue)>>,
    ) -> Result<(), JsValue>;
}

impl std::fmt::Debug for SQLite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(f, "SQLite WASM bridge")
    }
}
