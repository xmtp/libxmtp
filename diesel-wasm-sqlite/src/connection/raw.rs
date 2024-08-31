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

    pub(super) fn serialize(&self, schema: &str, flags: u32) -> SerializedDatabase {
        let sqlite3 = crate::get_sqlite_unchecked();
        let mut p_size: i64 = 0;

        let data_ptr = unsafe { 
            sqlite3.sqlite3_serialize(
                &self.internal_connection,
                schema,
                Some(&mut p_size),
                flags
        )};

        if data_ptr.is_null() {
            panic!("Serialization failed");
        }

        unsafe { SerializedDatabase::new(data_ptr, p_size as usize) }
    }

    pub(super) fn deserialize(&self, schema: &str, serialized_db: SerializedDatabase, total_size: usize, flags: u32) -> i32 {
        let sqlite3 = crate::get_sqlite_unchecked();

        if serialized_db.len > total_size {
            panic!("Serialized database size exceeds the buffer size");
        }

        let result = unsafe {
            sqlite.sqlite3_deserialize(
                &self.internal_connection,
                schema,
                serialized_db.data,
                serialized_db.len as i64,
                total_size as i64,
                flags,
            )
        };
        
        if result != 0 {
            panic!("Deserialization failed");
        }

        result
    }
}

impl Drop for RawConnection {
    fn drop(&mut self) {
        let sqlite3 = crate::get_sqlite_unchecked();
        

        let result = sqlite3.close(&self.internal_connection); 
        if result != *crate::ffi::SQLITE_OK {
            let error_message = super::error_message(result);
            panic!("Error closing SQLite connection: {}", error_message);
        }
    }
}
