use wasm_bindgen::{prelude::*, JsValue};

/// Simple Connection
#[wasm_bindgen(module = "/src/package.js")]
extern "C" {
    #[wasm_bindgen(catch)]
    pub fn batch_execute(database: i32, query: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    pub fn establish(database_url: &str) -> Result<i32, JsValue>;
}

/// Direct Shim for wa-sqlite
#[wasm_bindgen(module = "/src/package.js")]
extern "C" {
    #[wasm_bindgen]
    pub fn sqlite3_result_text(context: i32, value: String);

    #[wasm_bindgen]
    pub fn sqlite3_result_int(context: i32, value: i32);

    #[wasm_bindgen]
    pub fn sqlite3_result_int64(context: i32, value: i64);

    #[wasm_bindgen]
    pub fn sqlite3_result_double(context: i32, value: f64);

    #[wasm_bindgen]
    pub fn sqlite3_result_blob(context: i32, value: Vec<u8>);

    #[wasm_bindgen]
    pub fn sqlite3_result_null(context: i32);
}
