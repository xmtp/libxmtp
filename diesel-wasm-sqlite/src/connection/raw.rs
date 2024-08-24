#![allow(dead_code)]
// functions are needed, but missing functionality means they aren't used yet.

use crate::{WasmSqlite, WasmSqliteError};
use diesel::{result::*, serialize::ToSql, sql_types::HasSqlType};
use wasm_bindgen::{closure::Closure, JsValue};

#[allow(missing_copy_implementations)]
pub(super) struct RawConnection {
    pub(super) internal_connection: JsValue,
}

impl RawConnection {
    pub(super) fn establish(database_url: &str) -> ConnectionResult<Self> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let database_url = if database_url.starts_with("sqlite://") {
            database_url.replacen("sqlite://", "file:", 1)
        } else {
            database_url.to_string()
        };

        let capi = sqlite3.inner().capi();
        let flags =
            capi.SQLITE_OPEN_READWRITE() | capi.SQLITE_OPEN_CREATE() | capi.SQLITE_OPEN_URI();

        // TODO: flags are ignored for now
        Ok(RawConnection {
            internal_connection: sqlite3
                .open(&database_url, Some(flags as i32))
                .map_err(WasmSqliteError::from)
                .map_err(ConnectionError::from)?,
        })
    }

    pub(super) fn exec(&self, query: &str) -> QueryResult<()> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let result = sqlite3.exec(&self.internal_connection, query).unwrap();
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
        let capi = crate::get_sqlite_unchecked().inner().capi();
        let mut flags = capi.SQLITE_UTF8();
        if deterministic {
            flags |= capi.SQLITE_DETERMINISTIC();
        }
        flags as i32
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
        let sqlite3 = crate::get_sqlite_unchecked();
        match sqlite3.close(&self.internal_connection) {
            Ok(_) => tracing::info!("RawConnection succesfully dropped & connection closed"),
            Err(e) => {
                tracing::error!("Dropping `RawConnection` enocountered {e:?}");
            }
        }
    }
}
