//! Module for an SQLite backend accesible from the web.
pub mod backend;
pub mod connection;
pub mod ffi;
pub mod query_builder;
pub mod sqlite_fixes;
pub mod sqlite_types;

#[global_allocator]
static ALLOCATOR: talc::TalckWasm = unsafe { talc::TalckWasm::new_global() };

#[cfg(any(feature = "unsafe-debug-query", test))]
pub use query_builder::insert_with_default_sqlite::unsafe_debug_query::DebugQueryWrapper;

#[cfg(not(target_arch = "wasm32"))]
compile_error!("This crate only suports the `wasm32-unknown-unknown` target");

use wasm_bindgen::JsValue;

pub use backend::{SqliteType, WasmSqlite};
pub(crate) use ffi::get_sqlite_unchecked;
pub use ffi::init_sqlite;
pub use sqlite_fixes::dsl;

#[derive(thiserror::Error, Debug)]
pub enum WasmSqliteError {
    #[error("JS Bridge Error {0:?}")]
    Js(JsValue),
    #[error(transparent)]
    Diesel(#[from] diesel::result::Error),
    #[error(transparent)]
    Bindgen(#[from] serde_wasm_bindgen::Error),
}

impl From<WasmSqliteError> for diesel::result::Error {
    fn from(value: WasmSqliteError) -> diesel::result::Error {
        tracing::error!("NOT IMPLEMENTED, {:?}", value);
        diesel::result::Error::NotFound
    }
}

impl From<WasmSqliteError> for diesel::result::ConnectionError {
    fn from(value: WasmSqliteError) -> diesel::result::ConnectionError {
        tracing::error!("{:?}", value);
        diesel::result::ConnectionError::BadConnection("Not implemented".to_string())
    }
}

impl From<JsValue> for WasmSqliteError {
    fn from(err: JsValue) -> WasmSqliteError {
        WasmSqliteError::Js(err)
    }
}
