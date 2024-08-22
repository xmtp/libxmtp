//! Module for an SQLite backend accesible from the web.
pub mod backend;
pub mod connection;
pub mod ffi;
pub mod query_builder;
pub mod sqlite_fixes;
pub mod sqlite_types;
pub mod utils;
// pub mod migrations;
//
use serde::{Deserialize, Serialize};

#[cfg(any(feature = "unsafe-debug-query", test))]
pub use query_builder::insert_with_default_sqlite::unsafe_debug_query::DebugQueryWrapper;

#[cfg(not(target_arch = "wasm32"))]
compile_error!("This crate only suports the `wasm32-unknown-unknown` target");

use self::ffi::SQLite;
use std::cell::LazyCell;
use wasm_bindgen::JsValue;

pub use backend::{SqliteType, WasmSqlite};
pub(crate) use ffi::{get_sqlite, get_sqlite_unchecked};

/// the local tokio current-thread runtime
/// dont need locking, because this is current-thread only
const RUNTIME: LazyCell<tokio::runtime::Runtime> = LazyCell::new(|| {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("Runtime should never fail to build")
});

#[derive(thiserror::Error, Debug)]
pub enum WasmSqliteError {
    #[error("JS Bridge Error {0:?}")]
    Js(JsValue),
    #[error(transparent)]
    OneshotRecv(#[from] tokio::sync::oneshot::error::RecvError),
    #[error(transparent)]
    Diesel(#[from] diesel::result::Error),
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
