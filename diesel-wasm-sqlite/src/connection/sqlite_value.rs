#![allow(unsafe_code)] // ffi calls

use std::cell::Ref;

use crate::backend::SqliteType;
use crate::ffi;

use super::owned_row::OwnedSqliteRow;
use super::row::PrivateSqliteRow;

/// Raw sqlite value as received from the database
///
/// Use existing `FromSql` implementations to convert this into
/// rust values
#[allow(missing_debug_implementations, missing_copy_implementations)]
pub struct SqliteValue<'row, 'stmt, 'query> {
    // This field exists to ensure that nobody
    // can modify the underlying row while we are
    // holding a reference to some row value here
    _row: Option<Ref<'row, PrivateSqliteRow<'stmt, 'query>>>,
    // we extract the raw value pointer as part of the constructor
    // to safe the match statements for each method
    // According to benchmarks this leads to a ~20-30% speedup
    //
    //
    // This is sound as long as nobody calls `stmt.step()`
    // while holding this value. We ensure this by including
    // a reference to the row above.
    /// We are referencing SQLites WASM memory,
    /// so we just have to trust its not null rather than using `NonNull`
    /// (i dont think that would work unless its our mem)
    value: *mut u8,
}

#[derive(Debug)]
pub(super) struct OwnedSqliteValue {
    // maybe make JsValue?
    pub(super) value: *mut u8,
}

impl Drop for OwnedSqliteValue {
    fn drop(&mut self) {
        let sqlite3 = crate::get_sqlite_unchecked();
        sqlite3.value_free(self.value)
    }
}

// Unsafe Send impl safe since sqlite3_value is built with sqlite3_value_dup
// see https://www.sqlite.org/c3ref/value.html
unsafe impl Send for OwnedSqliteValue {}

impl<'row, 'stmt, 'query> SqliteValue<'row, 'stmt, 'query> {
    pub(super) fn new(
        row: Ref<'row, PrivateSqliteRow<'stmt, 'query>>,
        col_idx: i32,
    ) -> Option<SqliteValue<'row, 'stmt, 'query>> {
        let value = match &*row {
            PrivateSqliteRow::Direct(stmt) => stmt.column_value(col_idx),
            PrivateSqliteRow::Duplicated { values, .. } => {
                values.get(col_idx as usize).and_then(|v| v.as_ref())?.value
            }
        };

        let ret = Self {
            _row: Some(row),
            value,
        };
        if ret.value_type().is_none() {
            None
        } else {
            Some(ret)
        }
    }

    pub(super) fn from_owned_row(
        row: &'row OwnedSqliteRow,
        col_idx: i32,
    ) -> Option<SqliteValue<'row, 'stmt, 'query>> {
        let value = row
            .values
            .get(col_idx as usize)
            .and_then(|v| v.as_ref())?
            .value;
        let ret = Self { _row: None, value };
        if ret.value_type().is_none() {
            None
        } else {
            Some(ret)
        }
    }

    pub(crate) fn read_text(&self) -> String {
        self.parse_string(|s| s)
    }

    // TODO: If we share memory with SQLITE, we can return a &'value str here rathre than an
    // allocated String
    pub(crate) fn parse_string<R>(&self, f: impl FnOnce(String) -> R) -> R {
        let sqlite3 = crate::get_sqlite_unchecked();
        // TODO:
        // for some reason sqlite3_value_text returns the String and not a
        // pointer. There's probably a way to make it return a pointer
        let s = sqlite3.value_text(self.value);
        // let s = unsafe {
        // let ptr = sqlite3.value_text(self.value);
        // let len = sqlite3.value_bytes(self.value);
        // let mut bytes = Vec::with_capacity(len as usize);
        // ffi::raw_copy_from_sqlite(ptr, len, bytes.as_mut_slice());
        // unsafe { bytes.set_len(len) }; // not sure we need this
        // String::from_utf8_unchecked(bytes)
        // };
        f(s)
    }

    pub(crate) fn read_blob(&self) -> Vec<u8> {
        let sqlite3 = crate::get_sqlite_unchecked();
        unsafe {
            let ptr = sqlite3.value_blob(self.value);
            let len = sqlite3.value_bytes(self.value);
            let mut bytes = Vec::with_capacity(len as usize);
            bytes.set_len(len as usize);
            ffi::raw_copy_from_sqlite(ptr, len, bytes.as_mut_slice());
            bytes
        }
    }

    pub(crate) fn read_integer(&self) -> i32 {
        let sqlite3 = crate::get_sqlite_unchecked();
        sqlite3.value_int(self.value)
    }

    pub(crate) fn read_long(&self) -> i64 {
        let sqlite3 = crate::get_sqlite_unchecked();
        sqlite3.value_int64(self.value)
    }

    pub(crate) fn read_double(&self) -> f64 {
        let sqlite3 = crate::get_sqlite_unchecked();
        sqlite3.value_double(self.value)
    }

    /// Get the type of the value as returned by sqlite
    pub fn value_type(&self) -> Option<SqliteType> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let tpe = sqlite3.value_type(self.value);

        match tpe {
            _ if *ffi::SQLITE_TEXT == tpe => Some(SqliteType::Text),
            _ if *ffi::SQLITE_INTEGER == tpe => Some(SqliteType::Long),
            _ if *ffi::SQLITE_FLOAT == tpe => Some(SqliteType::Double),
            _ if *ffi::SQLITE_BLOB == tpe => Some(SqliteType::Binary),
            _ if *ffi::SQLITE_NULL == tpe => None,
            _ => unreachable!(
                "Sqlite's documentation state that this case ({}) is not reachable. \
                 If you ever see this error message please open an issue at \
                 https://github.com/diesel-rs/diesel.",
                tpe
            ),
        }
    }
}

impl OwnedSqliteValue {
    pub(super) fn copy_from_ptr(ptr: *mut u8) -> Option<OwnedSqliteValue> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let tpe = sqlite3.value_type(ptr);
        if *ffi::SQLITE_NULL == tpe {
            return None;
        }

        let value = sqlite3.value_dup(ptr);

        Some(Self { value })
    }

    pub(super) fn duplicate(&self) -> OwnedSqliteValue {
        let sqlite3 = crate::get_sqlite_unchecked();
        let value = sqlite3.value_dup(self.value);
        OwnedSqliteValue { value }
    }
}
