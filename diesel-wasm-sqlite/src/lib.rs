//! Module for an SQLite backend accesible from the web.
pub mod backend;
pub mod connection;
pub mod ffi;
pub mod query_builder;
pub mod sqlite_types;

use diesel::{
    connection::{AnsiTransactionManager, Instrumentation, SimpleConnection, TransactionManager},
    query_builder::{QueryFragment, QueryId},
    result::QueryResult,
    Connection,
};
use wasm_bindgen::JsValue;

pub use backend::{SqliteType, WasmSqlite};
pub struct WasmSqliteConnection {}
#[derive(Debug)]
pub struct WasmSqliteError(JsValue);

impl SimpleConnection for WasmSqliteConnection {
    fn batch_execute(&mut self, query: &str) -> diesel::prelude::QueryResult<()> {
        ffi::batch_execute(query)
            .map_err(WasmSqliteError::from)
            .map_err(Into::into)
    }
}

impl diesel::connection::ConnectionSealed for WasmSqliteConnection {}

impl Connection for WasmSqliteConnection {
    type Backend = WasmSqlite;
    type TransactionManager = AnsiTransactionManager;

    fn establish(database_url: &str) -> diesel::prelude::ConnectionResult<Self> {
        ffi::establish(database_url)
            .map_err(WasmSqliteError::from)
            .map_err(Into::<diesel::result::ConnectionError>::into)?;
        Ok(WasmSqliteConnection {})
    }

    fn execute_returning_count<T>(&mut self, source: &T) -> QueryResult<usize>
    where
        T: QueryFragment<Self::Backend> + QueryId,
    {
        todo!()
    }

    fn transaction_state(
        &mut self,
    ) -> &mut <Self::TransactionManager as TransactionManager<Self>>::TransactionStateData {
        todo!()
    }

    fn instrumentation(&mut self) -> &mut dyn Instrumentation {
        todo!()
    }

    fn set_instrumentation(&mut self, instrumentation: impl diesel::connection::Instrumentation) {
        todo!()
    }
}

impl From<WasmSqliteError> for diesel::result::Error {
    fn from(value: WasmSqliteError) -> diesel::result::Error {
        log::error!("NOT IMPLEMENTED, {:?}", value);
        diesel::result::Error::NotFound
    }
}

impl From<WasmSqliteError> for diesel::result::ConnectionError {
    fn from(value: WasmSqliteError) -> diesel::result::ConnectionError {
        log::error!("NOT IMPLEMENTED, {:?}", value);
        diesel::result::ConnectionError::BadConnection("Not implemented".to_string())
    }
}

impl From<JsValue> for WasmSqliteError {
    fn from(err: JsValue) -> WasmSqliteError {
        WasmSqliteError(err)
    }
}
