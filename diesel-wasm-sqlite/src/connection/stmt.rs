#![allow(unsafe_code)] //TODO: can probably remove for wa-sqlite
use super::bind_collector::{InternalSqliteBindValue, SqliteBindCollector};
use super::raw::RawConnection;
use super::sqlite_value::OwnedSqliteValue;
use crate::{
    sqlite_types::{result_codes, PrepareOptions, SqlitePrepareFlags},
    SqliteType, WasmSqlite,
};
use diesel::{
    connection::{
        statement_cache::{MaybeCached, PrepareForCache},
        Instrumentation,
    },
    query_builder::{QueryFragment, QueryId},
    result::{Error::DatabaseError, *},
};
use std::cell::OnceCell;

use wasm_bindgen::JsValue;

pub(super) struct Statement {
    inner_statement: JsValue,
}

impl Statement {
    pub(super) async fn prepare(
        raw_connection: &RawConnection,
        sql: &str,
        is_cached: PrepareForCache,
    ) -> QueryResult<Self> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let flags = if matches!(is_cached, PrepareForCache::Yes) {
            Some(SqlitePrepareFlags::SQLITE_PREPARE_PERSISTENT.bits())
        } else {
            None
        };

        let options = PrepareOptions {
            flags,
            unscoped: None,
        };

        let stmt = sqlite3
            .prepare(
                &raw_connection.internal_connection,
                sql,
                Some(serde_wasm_bindgen::to_value(&options).unwrap()),
            )
            .await
            .unwrap();

        Ok(Statement {
            inner_statement: stmt,
        })
    }

    // The caller of this function has to ensure that:
    // * Any buffer provided as `SqliteBindValue::BorrowedBinary`, `SqliteBindValue::Binary`
    // `SqliteBindValue::String` or `SqliteBindValue::BorrowedString` is valid
    // till either a new value is bound to the same parameter or the underlying
    // prepared statement is dropped.
    fn bind(
        &self,
        _tpe: SqliteType,
        value: InternalSqliteBindValue<'_>,
        bind_index: i32,
    ) -> QueryResult<i32> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let value =
            serde_wasm_bindgen::to_value(&value).expect("Bind value failed to convert to JsValue");
        let result = sqlite3
            .bind(&self.inner_statement, bind_index, value.into())
            .unwrap();

        // TODO:insipx Pretty sure we can have a simpler implementation here
        // making use of `wa-sqlite` `bind` which abstracts over the individual bind functions in
        // sqlite3. However, not sure  how this will work further up the stack.
        // This might not work because of differences in how serde_json recognizes js types
        // and how wa-sqlite recogizes js types. In that case, need to resort to matching on
        // individual types as below.
        /*
        let result = match (tpe, value) {
            (_, InternalSqliteBindValue::Null) => {
                ffi::sqlite3_bind_null(self.inner_statement.as_ptr(), bind_index)
            }
            (SqliteType::Binary, InternalSqliteBindValue::BorrowedBinary(bytes)) => {
                ffi::sqlite3_bind_blob(
                    self.inner_statement.as_ptr(),
                    bind_index,
                    bytes.as_ptr() as *const libc::c_void,
                    bytes.len() as libc::c_int,
                    ffi::SQLITE_STATIC(),
                )
            }
            (SqliteType::Binary, InternalSqliteBindValue::Binary(mut bytes)) => {
                let len = bytes.len();
                // We need a separate pointer here to pass it to sqlite
                // as the returned pointer is a pointer to a dyn sized **slice**
                // and not the pointer to the first element of the slice
                let ptr = bytes.as_mut_ptr();
                ret_ptr = NonNull::new(Box::into_raw(bytes));
                ffi::sqlite3_bind_blob(
                    self.inner_statement.as_ptr(),
                    bind_index,
                    ptr as *const libc::c_void,
                    len as libc::c_int,
                    ffi::SQLITE_STATIC(),
                )
            }
            (SqliteType::Text, InternalSqliteBindValue::BorrowedString(bytes)) => {
                ffi::sqlite3_bind_text(
                    self.inner_statement.as_ptr(),
                    bind_index,
                    bytes.as_ptr() as *const libc::c_char,
                    bytes.len() as libc::c_int,
                    ffi::SQLITE_STATIC(),
                )
            }
            (SqliteType::Text, InternalSqliteBindValue::String(bytes)) => {
                let mut bytes = Box::<[u8]>::from(bytes);
                let len = bytes.len();
                // We need a separate pointer here to pass it to sqlite
                // as the returned pointer is a pointer to a dyn sized **slice**
                // and not the pointer to the first element of the slice
                let ptr = bytes.as_mut_ptr();
                ret_ptr = NonNull::new(Box::into_raw(bytes));
                ffi::sqlite3_bind_text(
                    self.inner_statement.as_ptr(),
                    bind_index,
                    ptr as *const libc::c_char,
                    len as libc::c_int,
                    ffi::SQLITE_STATIC(),
                )
            }
            (SqliteType::Float, InternalSqliteBindValue::F64(value))
            | (SqliteType::Double, InternalSqliteBindValue::F64(value)) => {
                ffi::sqlite3_bind_double(
                    self.inner_statement.as_ptr(),
                    bind_index,
                    value as libc::c_double,
                )
            }
            (SqliteType::SmallInt, InternalSqliteBindValue::I32(value))
            | (SqliteType::Integer, InternalSqliteBindValue::I32(value)) => {
                ffi::sqlite3_bind_int(self.inner_statement.as_ptr(), bind_index, value)
            }
            (SqliteType::Long, InternalSqliteBindValue::I64(value)) => {
                ffi::sqlite3_bind_int64(self.inner_statement.as_ptr(), bind_index, value)
            }
            (t, b) => {
                return Err(Error::SerializationError(
                    format!("Type mismatch: Expected {t:?}, got {b}").into(),
                ))
            }
        }
        */
        Ok(result)
    }

    async fn reset(&self) {
        let sqlite3 = crate::get_sqlite_unchecked();
        let _ = sqlite3.reset(&self.inner_statement).await.unwrap();
    }

    fn clear_bindings(&self) {
        let sqlite3 = crate::get_sqlite_unchecked();
        let _ = sqlite3.clear_bindings(&self.inner_statement).unwrap();
    }

    /* not sure if there is equivalent method or just cloning the stmt is enough
    fn raw_connection(&self) -> *mut ffi::sqlite3 {
        unsafe { ffi::sqlite3_db_handle(self.inner_statement.as_ptr()) }
    }
    */
}

/* TODO: Useful for converting JS Error messages to Rust
fn last_error(raw_connection: *mut ffi::sqlite3) -> Error {
    let error_message = last_error_message(raw_connection);
    let error_information = Box::new(error_message);
    let error_kind = match last_error_code(raw_connection) {
        ffi::SQLITE_CONSTRAINT_UNIQUE | ffi::SQLITE_CONSTRAINT_PRIMARYKEY => {
            DatabaseErrorKind::UniqueViolation
        }
        ffi::SQLITE_CONSTRAINT_FOREIGNKEY => DatabaseErrorKind::ForeignKeyViolation,
        ffi::SQLITE_CONSTRAINT_NOTNULL => DatabaseErrorKind::NotNullViolation,
        ffi::SQLITE_CONSTRAINT_CHECK => DatabaseErrorKind::CheckViolation,
        _ => DatabaseErrorKind::Unknown,
    };
    DatabaseError(error_kind, error_information)
}

fn last_error_message(conn: *mut ffi::sqlite3) -> String {
    let c_str = unsafe { CStr::from_ptr(ffi::sqlite3_errmsg(conn)) };
    c_str.to_string_lossy().into_owned()
}


fn last_error_code(conn: *mut ffi::sqlite3) -> libc::c_int {
    unsafe { ffi::sqlite3_extended_errcode(conn) }
}
*/

impl Drop for Statement {
    fn drop(&mut self) {
        let sqlite3 = crate::get_sqlite_unchecked();
        // TODO:insipx potential problems here.
        // wa-sqlite does not throw an error if finalize fails:  -- it might just crash
        // doc: https://rhashimoto.github.io/wa-sqlite/docs/interfaces/SQLiteAPI.html#finalize.finalize-1
        // in that case we might not know if this errored or not
        // maybe depends how wasm panic/errors work
        // Worth unit testing the Drop implementation.
        let _ = sqlite3
            .finalize(&self.inner_statement)
            .expect("Error finalized SQLite prepared statement");
    }
}

// A warning for future editors:
// Changing this code to something "simpler" may
// introduce undefined behaviour. Make sure you read
// the following discussions for details about
// the current version:
//
// * https://github.com/weiznich/diesel/pull/7
// * https://users.rust-lang.org/t/code-review-for-unsafe-code-in-diesel/66798/
// * https://github.com/rust-lang/unsafe-code-guidelines/issues/194
struct BoundStatement<'stmt, 'query> {
    statement: MaybeCached<'stmt, Statement>,
    // we need to store the query here to ensure no one does
    // drop it till the end of the statement
    // We use a boxed queryfragment here just to erase the
    // generic type, we use NonNull to communicate
    // that this is a shared buffer
    query: Option<Box<dyn QueryFragment<WasmSqlite> + 'query>>,
    instrumentation: &'stmt mut dyn Instrumentation,
    has_error: bool,
}

impl<'stmt, 'query> BoundStatement<'stmt, 'query> {
    fn bind<T>(
        statement: MaybeCached<'stmt, Statement>,
        query: T,
        instrumentation: &'stmt mut dyn Instrumentation,
    ) -> QueryResult<BoundStatement<'stmt, 'query>>
    where
        T: QueryFragment<WasmSqlite> + QueryId + 'query,
    {
        // Don't use a trait object here to prevent using a virtual function call
        // For sqlite this can introduce a measurable overhead
        // Query is boxed here to make sure it won't move in memory anymore, so any bind
        // it could output would stay valid.
        let query = Box::new(query);

        let mut bind_collector = SqliteBindCollector::new();
        query.collect_binds(&mut bind_collector, &mut (), &WasmSqlite)?;
        let SqliteBindCollector { binds } = bind_collector;

        let mut ret = BoundStatement {
            statement,
            query: None,
            instrumentation,
            has_error: false,
        };

        ret.bind_buffers(binds)?;

        let query = query as Box<dyn QueryFragment<WasmSqlite> + 'query>;
        ret.query = Some(query);

        Ok(ret)
    }

    // This is a separated function so that
    // not the whole constructor is generic over the query type T.
    // This hopefully prevents binary bloat.
    fn bind_buffers(
        &mut self,
        binds: Vec<(InternalSqliteBindValue<'_>, SqliteType)>,
    ) -> QueryResult<()> {
        for (bind_idx, (bind, tpe)) in (1..).zip(binds) {
            // It's safe to call bind here as:
            // * The type and value matches
            // * We ensure that corresponding buffers lives long enough below
            // * The statement is not used yet by `step` or anything else
            let _ = self.statement.bind(tpe, bind, bind_idx)?;

            // we don't track binds to free like sqlite3 C bindings
            // The assumption is that wa-sqlite, being WASM run in web browser that
            // lies in the middle of rust -> sqlite, takes care of this for us.
            // if we run into memory issues, especailly memory leaks
            // this should be the first place to pay attention to.
            //
            // The bindings shuold be collected/freed with JS once `clear_bindings` is
            // run on `Drop` for `BoundStatement`
        }
        Ok(())
    }

    fn finish_query_with_error(mut self, _e: &Error) {
        self.has_error = true;
        /*
        if let Some(q) = self.query {
            // it's safe to get a reference from this ptr as it's guaranteed to not be null
            let q = unsafe { q.as_ref() };
            self.instrumentation.on_connection_event(
                diesel::connection::InstrumentationEvent::FinishQuery {
                    query: &crate::debug_query(&q),
                    error: Some(e),
                },
            );
        }
        */
    }
}

// TODO: AsyncDrop
impl<'stmt, 'query> Drop for BoundStatement<'stmt, 'query> {
    fn drop(&mut self) {
        // First reset the statement, otherwise the bind calls
        // below will fail
        self.statement.reset();
        self.statement.clear_bindings();

        if let Some(query) = &mut self.query {
            std::mem::drop(query);
            self.query = None;
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct StatementUse<'stmt, 'query> {
    statement: BoundStatement<'stmt, 'query>,
    column_names: OnceCell<Vec<String>>,
}

impl<'stmt, 'query> StatementUse<'stmt, 'query> {
    pub(super) fn bind<T>(
        statement: MaybeCached<'stmt, Statement>,
        query: T,
        instrumentation: &'stmt mut dyn Instrumentation,
    ) -> QueryResult<StatementUse<'stmt, 'query>>
    where
        T: QueryFragment<WasmSqlite> + QueryId + 'query,
    {
        Ok(Self {
            statement: BoundStatement::bind(statement, query, instrumentation)?,
            column_names: OnceCell::new(),
        })
    }

    pub(super) async fn run(mut self) -> QueryResult<()> {
        // This is safe as we pass `first_step = true`
        // and we consume the statement so nobody could
        // access the columns later on anyway.
        let r = self.step(true).await.map(|_| ());

        if let Err(ref e) = r {
            self.statement.finish_query_with_error(e);
        }
        r
    }

    // This function is marked as unsafe incorrectly passing `false` to `first_step`
    // for a first call to this function could cause access to freed memory via
    // the cached column names.
    //
    // It's always safe to call this function with `first_step = true` as this removes
    // the cached column names
    pub(super) async fn step(&mut self, first_step: bool) -> QueryResult<bool> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let res = match sqlite3
            .step(&self.statement.statement.inner_statement)
            .await
            .unwrap()
        {
            result_codes::SQLITE_DONE => Ok(false),
            result_codes::SQLITE_ROW => Ok(true),
            _ => panic!("SQLite Step returned Unhandled Result Code. Turn into err message"),
        };

        if first_step {
            self.column_names = OnceCell::new();
        }
        res
    }

    // The returned string pointer is valid until either the prepared statement is
    // destroyed by sqlite3_finalize() or until the statement is automatically
    // reprepared by the first call to sqlite3_step() for a particular run or
    // until the next call to sqlite3_column_name() or sqlite3_column_name16()
    // on the same column.
    //
    // https://sqlite.org/c3ref/column_name.html
    //
    // Note: This function is marked as unsafe, as calling it can invalidate
    // other existing column name pointers on the same column. To prevent that,
    // it should maximally be called once per column at all.
    fn column_name(&self, idx: i32) -> String {
        let sqlite3 = crate::get_sqlite_unchecked();

        sqlite3.column_name(&self.statement.statement.inner_statement, idx)
    }

    pub(super) fn column_count(&self) -> i32 {
        let sqlite3 = crate::get_sqlite_unchecked();
        sqlite3.column_count(&self.statement.statement.inner_statement)
    }

    pub(super) fn index_for_column_name(&mut self, field_name: &str) -> Option<usize> {
        (0..self.column_count())
            .find(|idx| self.field_name(*idx) == Some(field_name))
            .map(|v| v as usize)
    }

    pub(super) fn field_name(&self, idx: i32) -> Option<&str> {
        let column_names = self.column_names.get_or_init(|| {
            let count = self.column_count();
            (0..count).map(|idx| self.column_name(idx)).collect()
        });

        column_names.get(idx as usize).map(AsRef::as_ref)
    }
    /*
    pub(super) fn copy_value(&self, idx: i32) -> Option<OwnedSqliteValue> {
        OwnedSqliteValue::copy_from_ptr(self.column_value(idx)?)
    }

    pub(super) fn column_value(&self, idx: i32) -> Option<NonNull<ffi::sqlite3_value>> {
        let ptr = unsafe {
            ffi::sqlite3_column_value(self.statement.statement.inner_statement.as_ptr(), idx)
        };
        NonNull::new(ptr)
    }
    */
}

#[cfg(test)]
mod tests {
    use crate::prelude::*;
    use crate::sql_types::Text;

    // this is a regression test for
    // https://github.com/diesel-rs/diesel/issues/3558
    #[test]
    fn check_out_of_bounds_bind_does_not_panic_on_drop() {
        let mut conn = SqliteConnection::establish(":memory:").unwrap();

        let e = crate::sql_query("SELECT '?'")
            .bind::<Text, _>("foo")
            .execute(&mut conn);

        assert!(e.is_err());
        let e = e.unwrap_err();
        if let crate::result::Error::DatabaseError(crate::result::DatabaseErrorKind::Unknown, m) = e
        {
            assert_eq!(m.message(), "column index out of range");
        } else {
            panic!("Wrong error returned");
        }
    }
}
