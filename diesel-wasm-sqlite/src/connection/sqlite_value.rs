#![allow(unsafe_code)] // ffi calls

use std::cell::Ref;

use crate::ffi::{self, SQLiteCompatibleType};
use crate::{backend::SqliteType, sqlite_types};
use wasm_bindgen::JsValue;

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
    value: SQLiteCompatibleType,
}

#[derive(Debug, Clone)]
pub(super) struct OwnedSqliteValue {
    pub(super) value: SQLiteCompatibleType,
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
            PrivateSqliteRow::Direct(stmt) => stmt.column_value(col_idx)?,
            PrivateSqliteRow::Duplicated { values, .. } => values
                .get(col_idx as usize)
                .and_then(|v| v.as_ref())?
                .value
                .clone(),
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
            .value
            .clone();
        let ret = Self { _row: None, value };
        if ret.value_type().is_none() {
            None
        } else {
            Some(ret)
        }
    }

    pub(crate) fn parse_string<R>(&self, f: impl FnOnce(String) -> R) -> R {
        let sqlite3 = crate::get_sqlite_unchecked();
        let s = sqlite3.value_text(&self.value);
        f(s)
    }

    // TODO: Wasm bindgen can't work with references yet
    // not sure if this will effect perf
    pub(crate) fn read_text(&self) -> String {
        self.parse_string(|s| s)
    }

    pub(crate) fn read_blob(&self) -> Vec<u8> {
        let sqlite3 = crate::get_sqlite_unchecked();
        sqlite3.value_blob(&self.value)
    }

    pub(crate) fn read_integer(&self) -> i32 {
        let sqlite3 = crate::get_sqlite_unchecked();
        sqlite3.value_int(&self.value)
    }

    pub(crate) fn read_long(&self) -> i64 {
        let sqlite3 = crate::get_sqlite_unchecked();
        sqlite3.value_int64(&self.value)
    }

    pub(crate) fn read_double(&self) -> f64 {
        let sqlite3 = crate::get_sqlite_unchecked();
        sqlite3.value_double(&self.value)
    }

    /// Get the type of the value as returned by sqlite
    pub fn value_type(&self) -> Option<SqliteType> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let tpe = sqlite3.value_type(&self.value);
        match tpe {
            sqlite_types::SQLITE_TEXT => Some(SqliteType::Text),
            sqlite_types::SQLITE_INTEGER => Some(SqliteType::Long),
            sqlite_types::SQLITE_FLOAT => Some(SqliteType::Double),
            sqlite_types::SQLITE_BLOB => Some(SqliteType::Binary),
            sqlite_types::SQLITE_NULL => None,
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
    pub(super) fn copy_from_ptr(ptr: &JsValue) -> Option<OwnedSqliteValue> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let tpe = sqlite3.value_type(&ptr);
        if sqlite_types::SQLITE_NULL == tpe {
            return None;
        }

        let value = sqlite3.value_dup(ptr);

        Some(Self {
            value: value.into(),
        })
    }

    /*
    pub(super) fn copy_from_ptr(ptr: NonNull<ffi::sqlite3_value>) -> Option<OwnedSqliteValue> {
        let tpe = unsafe { ffi::sqlite3_value_type(ptr.as_ptr()) };
        if ffi::SQLITE_NULL == tpe {
            return None;
        }
        let value = unsafe { ffi::sqlite3_value_dup(ptr.as_ptr()) };
        Some(Self {
            value: NonNull::new(value)?,
        })
    }
    */
}
