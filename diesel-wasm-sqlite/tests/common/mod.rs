//! Common utilities/imports amongst WebAssembly tests
use prelude::*;

use wasm_bindgen_futures::wasm_bindgen::prelude::*;

// like ctor but for wasm
#[wasm_bindgen(start)]
pub async fn main_js() {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();
    diesel_wasm_sqlite::init_sqlite().await;
}

pub async fn connection() -> WasmSqliteConnection {
    diesel_wasm_sqlite::init_sqlite().await;
    WasmSqliteConnection::establish(":memory:").unwrap()
}

// re-exports used in tests
pub mod prelude {
    pub(crate) use diesel::{
        connection::{Connection, LoadConnection},
        debug_query, insert_into,
        prelude::*,
    };
    pub(crate) use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
    pub(crate) use diesel_wasm_sqlite::{
        connection::WasmSqliteConnection, DebugQueryWrapper, WasmSqlite,
    };
    pub(crate) use serde::Deserialize;
    pub(crate) use wasm_bindgen_test::*;
    pub(crate) use web_sys::console;
}
