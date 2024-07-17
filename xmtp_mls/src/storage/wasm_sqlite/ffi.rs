use wasm_bindgen::{prelude::*, JsValue};

/// Simple Connection
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(catch)]
    pub fn batch_execute(query: &str) -> Result<(), JsValue>;

    #[wasm_bindgen(catch)]
    pub fn establish(database_url: &str) -> Result<(), JsValue>;
}

/// Direct Shim for was-sqlite
#[wasm_bindgen]
extern "C" {
    pub fn sqlite3_result_text(context: i32, value: String);
    pub fn sqlite3_result_int(context: i32, value: i32);
    pub fn sqlite3_result_int64(context: i32, value: i64);
    pub fn sqlite3_result_double(context: i32, value: f64);
    pub fn sqlite3_result_blob(context: i32, value: Vec<u8>);
    pub fn sqlite3_result_null(context: i32);
}
