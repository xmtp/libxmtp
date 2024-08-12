#![allow(unsafe_code)] // ffi calls

// use std::io::{stderr, Write};

// use super::functions::{build_sql_function_args, process_sql_function_result};
// use super::serialized_database::SerializedDatabase;
// use super::stmt::ensure_sqlite_ok;
// use super::{Sqlite, SqliteAggregateFunction};
// use crate::deserialize::FromSqlRow;
// use crate::result::Error::DatabaseError;
use crate::{
    sqlite_types::{SqliteFlags, SqliteOpenFlags},
    WasmSqlite, WasmSqliteError,
};
use diesel::{result::*, serialize::ToSql, sql_types::HasSqlType};
use futures::future::BoxFuture;
use tokio::sync::oneshot;
use wasm_bindgen::{closure::Closure, JsValue};

/*
/// For use in FFI function, which cannot unwind.
/// Print the message, ask to open an issue at Github and [`abort`](std::process::abort).
macro_rules! assert_fail {
    ($fmt:expr $(,$args:tt)*) => {
        eprint!(concat!(
            $fmt,
            "If you see this message, please open an issue at https://github.com/diesel-rs/diesel/issues/new.\n",
            "Source location: {}:{}\n",
        ), $($args,)* file!(), line!());
        std::process::abort()
    };
}
*/

#[allow(missing_copy_implementations)]
#[derive(Debug)]
pub(super) struct RawConnection {
    pub(super) internal_connection: JsValue,
    drop_signal: Option<oneshot::Sender<JsValue>>,
}

impl RawConnection {
    pub(super) async fn establish(database_url: &str) -> ConnectionResult<Self> {
        let sqlite3 = crate::get_sqlite().await;
        let database_url = if database_url.starts_with("sqlite://") {
            database_url.replacen("sqlite://", "file:", 1)
        } else {
            database_url.to_string()
        };
        let flags = SqliteOpenFlags::SQLITE_OPEN_READWRITE
            | SqliteOpenFlags::SQLITE_OPEN_CREATE
            | SqliteOpenFlags::SQLITE_OPEN_URI;

        let (tx, rx) = oneshot::channel::<JsValue>();
        wasm_bindgen_futures::spawn_local(async move {
            let conn = rx.await;

            let sqlite3 = crate::get_sqlite_unchecked();

            match sqlite3.close(&conn.unwrap()).await {
                Ok(_) => log::debug!("db closed"),
                Err(e) => {
                    log::error!("error during db close");
                    web_sys::console::log_1(&e);
                }
            }
        });

        Ok(RawConnection {
            internal_connection: sqlite3
                .open_v2(&database_url, Some(flags.bits() as i32))
                .await
                .map_err(WasmSqliteError::from)
                .map_err(ConnectionError::from)?,
            drop_signal: Some(tx),
        })
    }

    pub(super) async fn exec(&self, query: &str) -> QueryResult<()> {
        let sqlite3 = crate::get_sqlite().await;
        let result = sqlite3
            .exec(&self.internal_connection, query)
            .await
            .unwrap();

        Ok(result)
    }

    pub(super) fn rows_affected_by_last_query(&self) -> usize {
        let sqlite3 = crate::get_sqlite_unchecked();
        sqlite3.changes(&self.internal_connection)
    }

    pub(super) fn register_sql_function<F, Ret, RetSqlType>(
        &self,
        fn_name: &str,
        num_args: usize,
        deterministic: bool,
        f: F,
    ) -> QueryResult<()>
    where
        F: FnMut(JsValue, Vec<JsValue>) -> JsValue + 'static,
        Ret: ToSql<RetSqlType, WasmSqlite>,
        WasmSqlite: HasSqlType<RetSqlType>,
    {
        let sqlite3 = crate::get_sqlite_unchecked();
        let flags = Self::get_flags(deterministic);

        let cb = Closure::new(f);
        sqlite3
            .create_function(
                &self.internal_connection,
                fn_name,
                num_args
                    .try_into()
                    .expect("usize to i32 panicked in register_sql_function"),
                flags,
                0,
                Some(&cb),
                None,
                None,
            )
            .unwrap();
        Ok(())
    }

    fn get_flags(deterministic: bool) -> i32 {
        let mut flags = SqliteFlags::SQLITE_UTF8;
        if deterministic {
            flags |= SqliteFlags::SQLITE_DETERMINISTIC;
        }
        flags.bits() as i32
    }

    /* possible to implement this, but would need to fill in the missing wa-sqlite functions
    pub(super) fn serialize(&mut self) -> SerializedDatabase {
        unsafe {
            let mut size: ffi::sqlite3_int64 = 0;
            let data_ptr = ffi::sqlite3_serialize(
                self.internal_connection.as_ptr(),
                std::ptr::null(),
                &mut size as *mut _,
                0,
            );
            SerializedDatabase::new(data_ptr, size as usize)
        }
    }

    pub(super) fn deserialize(&mut self, data: &[u8]) -> QueryResult<()> {
        // the cast for `ffi::SQLITE_DESERIALIZE_READONLY` is required for old libsqlite3-sys versions
        #[allow(clippy::unnecessary_cast)]
        unsafe {
            let result = ffi::sqlite3_deserialize(
                self.internal_connection.as_ptr(),
                std::ptr::null(),
                data.as_ptr() as *mut u8,
                data.len() as i64,
                data.len() as i64,
                ffi::SQLITE_DESERIALIZE_READONLY as u32,
            );

            ensure_sqlite_ok(result, self.internal_connection.as_ptr())
        }
    }
    */
}

impl Drop for RawConnection {
    fn drop(&mut self) {
        let sender = self
            .drop_signal
            .take()
            .expect("Drop is only unwrapped once");
        sender.send(self.internal_connection.clone());
    }
}

/*
enum SqliteCallbackError {
    Abort(&'static str),
    DieselError(crate::result::Error),
    Panic(String),
}

impl SqliteCallbackError {
    fn emit(&self, ctx: *mut ffi::sqlite3_context) {
        let s;
        let msg = match self {
            SqliteCallbackError::Abort(msg) => *msg,
            SqliteCallbackError::DieselError(e) => {
                s = e.to_string();
                &s
            }
            SqliteCallbackError::Panic(msg) => msg,
        };
        unsafe {
            context_error_str(ctx, msg);
        }
    }
}

impl From<crate::result::Error> for SqliteCallbackError {
    fn from(e: crate::result::Error) -> Self {
        Self::DieselError(e)
    }
}
*/
/*
#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::{AsyncConnection, WasmSqliteConnection};
    use diesel::connection::Connection;
    use wasm_bindgen_test::*;
    use web_sys::console;
    wasm_bindgen_test_configure!(run_in_dedicated_worker);

    #[wasm_bindgen_test]
    async fn test_fn_registration() {
        let mut result = WasmSqliteConnection::establish("test").await;
        let mut conn = result.unwrap();
        console::log_1(&"CONNECTED".into());
        conn.raw
            .register_sql_function("test", 0, true, |ctx, values| {
                console::log_1(&"Inside Fn".into());
            });
    }
}
*/
