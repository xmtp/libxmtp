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
use diesel::result::*;
use diesel::serialize::ToSql;
use diesel::sql_types::HasSqlType;
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
#[derive(Clone, Debug)]
pub(super) struct RawConnection {
    pub(super) internal_connection: JsValue,
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

        Ok(RawConnection {
            internal_connection: sqlite3
                .open_v2(&database_url, Some(flags.bits() as i32))
                .await
                .map_err(WasmSqliteError::from)
                .map_err(ConnectionError::from)?,
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

    pub(super) fn register_sql_function<F>(
        &self,
        fn_name: &str,
        num_args: i32,
        deterministic: bool,
        f: F,
    ) -> QueryResult<()>
    where
        F: FnMut(JsValue, JsValue) + 'static,
    {
        let sqlite3 = crate::get_sqlite_unchecked();
        let flags = Self::get_flags(deterministic);

        let cb = Closure::new(f);
        sqlite3
            .create_function(
                &self.internal_connection,
                fn_name,
                num_args,
                flags,
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
}

/* TODO: AsyncDrop
impl Drop for RawConnection {
    fn drop(&mut self) {
        use std::thread::panicking;

        let sqlite3 = crate::get_sqlite_unchecked();

        let close_result = sqlite3.close(self.internal_connection).unwrap();

        if close_result != ffi::SQLITE_OK {
            let error_message = super::error_message(close_result);
            if panicking() {
                write!(stderr(), "Error closing SQLite connection: {error_message}")
                    .expect("Error writing to `stderr`");
            } else {
                panic!("Error closing SQLite connection: {}", error_message);
            }
        }
    }
}
*/

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
