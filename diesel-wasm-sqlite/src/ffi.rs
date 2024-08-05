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
    pub type SQLiteCompatibleType;
    // pub type SqlitePrepareOptions;

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
    pub fn bind(
        this: &SQLite,
        stmt: &JsValue,
        idx: i32,
        value: SQLiteCompatibleType,
    ) -> Result<i32, JsValue>;
    /*
        #[wasm_bindgen(method, catch)]
        pub fn bind_blob(
            this: &SQLite,
            stmt: &JsValue,
            idx: i32,
            value: Vec<u8>,
        ) -> Result<i32, JsValue>;

        // JsValue here is an interesting type that needs to be ported in order to make use of this
        // but not currently using it.

        #[wasm_bindgen(method, catch)]
        pub fn bind_collection(
            this: &SQLite,
            stmt: &JsValue,
            bindings: JsValue,
        ) -> Result<i32, JsValue>;

        #[wasm_bindgen(method, catch)]
        pub fn bind_double(this: &SQLite, stmt: &JsValue, idx: i32, value: f64)
            -> Result<i32, JsValue>;

        #[wasm_bindgen(method, catch)]
        pub fn bind_int(this: &SQLite, stmt: &JsValue, idx: i32, value: i32) -> Result<i32, JsValue>;

        #[wasm_bindgen(method, catch)]
        pub fn bind_int64(this: &SQLite, stmt: &JsValue, idx: i32, value: i64) -> Result<i32, JsValue>;

        #[wasm_bindgen(method, catch)]
        pub fn bind_null(this: &SQLite, stmt: &JsValue, idx: i32) -> Result<i32, JsValue>;
    */
    #[wasm_bindgen(method)]
    pub fn bind_parameter_count(this: &SQLite, stmt: &JsValue) -> i32;

    #[wasm_bindgen(method)]
    pub fn bind_parameter_name(this: &SQLite, stmt: &JsValue, idx: i32) -> String;

    #[wasm_bindgen(method, catch)]
    pub fn bind_text(this: &SQLite, stmt: &JsValue, idx: i32, value: &str) -> Result<i32, JsValue>;

    #[wasm_bindgen(method, catch)]
    pub async fn reset(this: &SQLite, stmt: &JsValue) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method)]
    pub fn value(this: &SQLite, pValue: &JsValue);

    #[wasm_bindgen(method, catch)]
    pub async fn open_v2(
        this: &SQLite,
        database_url: &str,
        iflags: Option<i32>,
    ) -> Result<JsValue, JsValue>;

    #[wasm_bindgen(method, catch)]
    pub async fn exec(this: &SQLite, database: &JsValue, query: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(method, catch)]
    pub fn finalize(this: &SQLite, stmt: &JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(method)]
    pub fn changes(this: &SQLite, database: &JsValue) -> usize;

    #[wasm_bindgen(method, catch)]
    pub async fn prepare(
        db: &SQLite,
        database: &JsValue,
        sql: &str,
        options: Option<JsValue>,
    ) -> Result<JsValue, JsValue>;

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
