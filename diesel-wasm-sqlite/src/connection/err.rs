use crate::ffi;
use diesel::result::Error::DatabaseError;
use diesel::result::*;
use wasm_bindgen::JsValue;

pub(super) fn error_message(code: i32) -> String {
    let sqlite3 = crate::get_sqlite_unchecked();
    sqlite3.errstr(code)
}

pub(super) fn ensure_sqlite_ok(code: i32, raw_connection: &JsValue) -> QueryResult<()> {
    if code == *ffi::SQLITE_OK {
        Ok(())
    } else {
        Err(last_error(raw_connection))
    }
}

pub(super) fn last_error(raw_connection: &JsValue) -> diesel::result::Error {
    let error_message = last_error_message(raw_connection);
    let error_information = Box::new(error_message);
    let error_kind = match last_error_code(raw_connection) {
        e if *ffi::SQLITE_CONSTRAINT_UNIQUE | *ffi::SQLITE_CONSTRAINT_PRIMARYKEY == e => {
            DatabaseErrorKind::UniqueViolation
        }
        e if *ffi::SQLITE_CONSTRAINT_FOREIGNKEY == e => DatabaseErrorKind::ForeignKeyViolation,
        e if *ffi::SQLITE_CONSTRAINT_NOTNULL == e => DatabaseErrorKind::NotNullViolation,
        e if *ffi::SQLITE_CONSTRAINT_CHECK == e => DatabaseErrorKind::CheckViolation,
        _ => DatabaseErrorKind::Unknown,
    };
    DatabaseError(error_kind, error_information)
}

fn last_error_message(conn: &JsValue) -> String {
    let sqlite3 = crate::get_sqlite_unchecked();
    sqlite3.errmsg(conn)
}

fn last_error_code(conn: &JsValue) -> i32 {
    let sqlite3 = crate::get_sqlite_unchecked();
    sqlite3.extended_errcode(conn)
}
