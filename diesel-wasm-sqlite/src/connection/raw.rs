#![allow(dead_code)]
// functions are needed, but missing functionality means they aren't used yet.

use crate::{ffi, WasmSqlite, WasmSqliteError};
use diesel::{result::*, sql_types::HasSqlType};
use wasm_bindgen::{closure::Closure, JsValue};

use super::serialized_database::SerializedDatabase;

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
        sqlite3
            .exec(&self.internal_connection, query)
            .map_err(WasmSqliteError::from)?;
        Ok(())
    }

    pub(super) fn rows_affected_by_last_query(&self) -> usize {
        let sqlite3 = crate::get_sqlite_unchecked();
        sqlite3.changes(&self.internal_connection)
    }

    pub(super) fn register_sql_function<F, RetSqlType>(
        &self,
        fn_name: &str,
        num_args: usize,
        deterministic: bool,
        f: F,
    ) -> QueryResult<()>
    where
        F: FnMut(JsValue, Vec<JsValue>) -> JsValue + 'static,
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

    /// Serializes the database from sqlite to be stored by the user/client.
    pub(super) fn serialize(&self) -> SerializedDatabase {
        let sqlite3 = crate::get_sqlite_unchecked();
        let wasm = sqlite3.inner().wasm();

        let p_size = wasm.pstack().alloc(std::mem::size_of::<i64>() as u32);
        let data_ptr = sqlite3.sqlite3_serialize(&self.internal_connection, "main", &p_size, 0);
        if data_ptr.is_null() {
            panic!("Serialization failed");
        }

        let len = p_size.as_f64().unwrap() as u32;
        unsafe { SerializedDatabase::new(data_ptr, len) }
    }

    /// Deserializes the database from the data slice given to be loaded
    /// by sqlite in the wasm space.
    pub(super) fn deserialize(&self, data: &[u8]) -> i32 {
        let sqlite3 = crate::get_sqlite_unchecked();
        let wasm = sqlite3.inner().wasm();

        // allocate the space in wasm, and copy the buffer to the wasm
        // memory space.
        let p_data = wasm.alloc(data.len() as u32);
        ffi::raw_copy_to_sqlite(data, p_data);

        let result = sqlite3.sqlite3_deserialize(
            &self.internal_connection,
            "main",
            p_data,
            data.len() as i64,
            data.len() as i64,
            0,
        );

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
