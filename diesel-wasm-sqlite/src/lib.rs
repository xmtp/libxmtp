//! Module for an SQLite backend accesible from the web.
pub mod backend;
pub mod connection;
pub mod ffi;
pub mod query_builder;
pub mod sqlite_types;
pub mod utils;

#[cfg(not(target_arch = "wasm32"))]
compile_error!("This crate only suports the `wasm32-unknown-unknown` target");

use self::ffi::SQLite;
use tokio::sync::OnceCell;
use wasm_bindgen::JsValue;

pub use backend::{SqliteType, WasmSqlite};

/// The SQLite Library
/// this global constant references the loaded SQLite WASM.
static SQLITE: OnceCell<SQLite> = OnceCell::const_new();

pub type SQLiteWasm = &'static JsValue;

pub(crate) async fn get_sqlite() -> &'static SQLite {
    SQLITE
        .get_or_init(|| async {
            let module = SQLite::wasm_module().await;
            SQLite::new(module)
        })
        .await
}

pub(crate) fn get_sqlite_unchecked() -> &'static SQLite {
    SQLITE.get().expect("SQLite is not initialized")
}

#[derive(Debug)]
pub struct WasmSqliteError(JsValue);

impl From<WasmSqliteError> for diesel::result::Error {
    fn from(value: WasmSqliteError) -> diesel::result::Error {
        log::error!("NOT IMPLEMENTED, {:?}", value);
        diesel::result::Error::NotFound
    }
}

impl From<WasmSqliteError> for diesel::result::ConnectionError {
    fn from(value: WasmSqliteError) -> diesel::result::ConnectionError {
        log::error!("NOT IMPLEMENTED, {:?}", value);
        web_sys::console::log_1(&value.0);
        diesel::result::ConnectionError::BadConnection("Not implemented".to_string())
    }
}

impl From<JsValue> for WasmSqliteError {
    fn from(err: JsValue) -> WasmSqliteError {
        WasmSqliteError(err)
    }
}
