#![allow(unsafe_code)] //TODO: can probably remove for wa-sqlite
use super::raw::RawConnection;
use super::sqlite_value::OwnedSqliteValue;
use crate::ffi;
use crate::{
    connection::{bind_collector::InternalSqliteBindValue, err::*, SqliteBindCollector},
    SqliteType, WasmSqlite,
};
use diesel::{
    connection::{
        statement_cache::{MaybeCached, PrepareForCache},
        Instrumentation,
    },
    query_builder::{QueryFragment, QueryId},
    result::{Error, QueryResult},
};
use std::ffi::CString;
use std::{cell::OnceCell, ptr::NonNull};

use wasm_bindgen::JsValue;

// this is OK b/c web runs in one thread
unsafe impl Send for Statement {}
#[derive(Debug)]
pub(super) struct Statement {
    // each iteration compiles a new statement for use
    inner_statement: JsValue,
}

impl Statement {
    // NOTE: During diesel prepared statements,
    // statements are cached. WASM might not like statements being cached
    // since the statement pointer might be invalidated if a memory resize
    // takes place.
    pub fn prepare(
        raw_connection: &RawConnection,
        sql: &str,
        is_cached: PrepareForCache,
    ) -> QueryResult<Self> {
        let sqlite3 = crate::get_sqlite_unchecked();

        let flags = if matches!(is_cached, PrepareForCache::Yes { counter: _ }) {
            Some(*ffi::SQLITE_PREPARE_PERSISTENT)
        } else {
            None
        };

        let wasm = sqlite3.inner().wasm();
        let stack = wasm.pstack().pointer();

        // convert the query to a cstring
        let sql = CString::new(sql)?;
        let sql_bytes_with_nul = sql.as_bytes_with_nul();
        // we want to move the query over to sqlite-wasm
        // allocate space for the query in sqlite-wasm space
        let sql_ptr = wasm.alloc(sql_bytes_with_nul.len() as u32);
        // copy cstring query to sqlite-wasm
        ffi::raw_copy_to_sqlite(sql_bytes_with_nul, sql_ptr);

        // allocate one 64bit pointer value
        let pp_stmt = wasm.pstack().alloc(8);
        let prepare_result = sqlite3.prepare_v3(
            &raw_connection.internal_connection,
            sql_ptr,
            sql_bytes_with_nul.len() as i32,
            flags.unwrap_or(0),
            &pp_stmt,
            &JsValue::NULL,
        );

        let p_stmt = wasm.peek_ptr(&pp_stmt);

        ensure_sqlite_ok(prepare_result, &raw_connection.internal_connection)?;

        wasm.pstack().restore(&stack);
        // sqlite3_prepare_v3 returns a null pointer for empty statements. This includes
        // empty or only whitespace strings or any other non-op query string like a comment
        if p_stmt.is_null() {
            return Err(diesel::result::Error::QueryBuilderError(Box::new(
                diesel::result::EmptyQuery,
            )));
        }

        Ok(Self {
            inner_statement: p_stmt,
        })
    }

    fn copy_value_to_sqlite(bytes: &[u8]) -> *mut u8 {
        let sqlite3 = crate::get_sqlite_unchecked();
        let wasm = sqlite3.inner().wasm();
        let wasm_inner = wasm.alloc_inner();

        let len = bytes.len();
        let ptr = if len == 0 {
            wasm.alloc_ptr(1, true)
        } else {
            wasm_inner.alloc_impl(len as u32)
        };
        ffi::raw_copy_to_sqlite(bytes, ptr);
        // TODO: Maybe check for null here and return [`std::ptr::NonNull`]?
        // null is valid for bind function, it will just bind_null instead
        // but seems like something that should be an error.
        ptr
    }

    // The caller of this function has to ensure that:
    // * Any buffer provided as `SqliteBindValue::BorrowedBinary`, `SqliteBindValue::Binary`
    // `SqliteBindValue::String` or `SqliteBindValue::BorrowedString` is valid
    // till either a new value is bound to the same parameter or the underlying
    // prepared statement is dropped.
    fn bind(
        &self,
        tpe: SqliteType,
        value: InternalSqliteBindValue<'_>,
        bind_index: i32,
    ) -> QueryResult<Option<NonNull<u8>>> {
        let sqlite3 = crate::get_sqlite_unchecked();

        let mut ret_ptr = None;
        let wasm = sqlite3.inner().wasm();

        let result = match (tpe, value) {
            (_, InternalSqliteBindValue::Null) => {
                sqlite3.bind_null(&self.inner_statement, bind_index)
            }
            (SqliteType::Binary, InternalSqliteBindValue::BorrowedBinary(bytes)) => {
                // copy bytes from our WASM memory to SQLites WASM memory
                tracing::trace!("Binding binary Borrowed! len={}", bytes.len());
                let ptr = Self::copy_value_to_sqlite(bytes);
                ret_ptr = NonNull::new(ptr);
                sqlite3.bind_blob(
                    &self.inner_statement,
                    bind_index,
                    ptr,
                    bytes.len() as i32,
                    *ffi::SQLITE_STATIC,
                )
            }
            (SqliteType::Binary, InternalSqliteBindValue::Binary(bytes)) => {
                tracing::trace!("Binding binary Owned! len={}", bytes.len());
                let ptr = Self::copy_value_to_sqlite(bytes.as_slice());
                ret_ptr = NonNull::new(ptr);
                sqlite3.bind_blob(
                    &self.inner_statement,
                    bind_index,
                    ptr,
                    bytes.len() as i32,
                    *ffi::SQLITE_STATIC,
                )
            }
            (SqliteType::Text, InternalSqliteBindValue::BorrowedString(bytes)) => {
                tracing::trace!("Binding Borrowed String! len={}", bytes.len());
                let ptr = wasm.alloc_cstring(bytes.to_string());
                ret_ptr = NonNull::new(ptr);
                sqlite3.bind_text(
                    &self.inner_statement,
                    bind_index,
                    ptr,
                    bytes.len() as i32,
                    *ffi::SQLITE_STATIC,
                )
            }
            (SqliteType::Text, InternalSqliteBindValue::String(bytes)) => {
                tracing::trace!("Binding Owned String!");
                let len = bytes.len();
                let ptr = wasm.alloc_cstring(bytes);
                ret_ptr = NonNull::new(ptr);
                sqlite3.bind_text(
                    &self.inner_statement,
                    bind_index,
                    ptr,
                    len as i32,
                    *ffi::SQLITE_STATIC,
                )
            }
            (SqliteType::Float, InternalSqliteBindValue::F64(value))
            | (SqliteType::Double, InternalSqliteBindValue::F64(value)) => {
                sqlite3.bind_double(&self.inner_statement, bind_index, value)
            }
            (SqliteType::SmallInt, InternalSqliteBindValue::I32(value))
            | (SqliteType::Integer, InternalSqliteBindValue::I32(value)) => {
                sqlite3.bind_int(&self.inner_statement, bind_index, value)
            }
            (SqliteType::Long, InternalSqliteBindValue::I64(value)) => {
                sqlite3.bind_int64(&self.inner_statement, bind_index, value)
            }
            (t, b) => {
                return Err(Error::SerializationError(
                    format!("Type mismatch: Expected {t:?}, got {b}").into(),
                ))
            }
        };
        match ensure_sqlite_ok(result, &self.raw_connection()) {
            Ok(()) => Ok(ret_ptr),
            Err(e) => {
                if let Some(ptr) = ret_ptr {
                    wasm.dealloc(ptr);
                }
                Err(e)
            }
        }
    }

    fn reset(&self) {
        let sqlite3 = crate::get_sqlite_unchecked();
        let _ = sqlite3.reset(&self.inner_statement);
    }

    fn raw_connection(&self) -> JsValue {
        let sqlite3 = crate::get_sqlite_unchecked();
        sqlite3.db_handle(&self.inner_statement)
    }
}

impl Drop for Statement {
    fn drop(&mut self) {
        let sqlite3 = crate::get_sqlite_unchecked();
        tracing::trace!("Statement dropped & finalized!");
        sqlite3
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
    binds_to_free: Vec<(i32, Option<NonNull<u8>>)>,
    #[allow(unused)]
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
            binds_to_free: Vec::new(),
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
        self.binds_to_free.reserve(
            binds
                .iter()
                .filter(|&(b, _)| {
                    matches!(
                        b,
                        InternalSqliteBindValue::BorrowedBinary(_)
                            | InternalSqliteBindValue::BorrowedString(_)
                            | InternalSqliteBindValue::String(_)
                            | InternalSqliteBindValue::Binary(_)
                    )
                })
                .count(),
        );
        for (bind_idx, (bind, tpe)) in (1..).zip(binds) {
            // It's safe to call bind here as:
            // * The type and value matches
            // * We ensure that corresponding buffers lives long enough below
            // * The statement is not used yet by `step` or anything else
            let res = self.statement.bind(tpe, bind, bind_idx)?;

            // it's important to push these only after
            // the call to bind succeeded, otherwise we might attempt to
            // call bind to an non-existing bind position in
            // the destructor

            if let Some(ptr) = res {
                // Store the id + pointer for a owned bind
                // as we must unbind and free them on drop
                self.binds_to_free.push((bind_idx, Some(ptr)));
            }
        }
        Ok(())
    }

    fn finish_query_with_error(mut self, e: &Error) {
        if let Some(q) = &self.query {
            tracing::warn!(
                "Query finished with error query={:?}, err={:?}",
                &diesel::debug_query(&q),
                e
            );
        }
        self.has_error = true;
    }
}

// we have to free the wawsm memory here not C memory so this will change significantly
impl<'stmt, 'query> Drop for BoundStatement<'stmt, 'query> {
    fn drop(&mut self) {
        self.statement.reset();
        // self.statement.clear_bindings().unwrap();
        let wasm = ffi::get_sqlite_unchecked().inner().wasm();
        for (idx, buffer) in std::mem::take(&mut self.binds_to_free) {
            // It's always safe to bind null values, as there is no buffer that needs to outlife something
            self.statement
                .bind(SqliteType::Text, InternalSqliteBindValue::Null, idx)
                .expect(
                    "Binding a null value should never fail. \
                         If you ever see this error message please open \
                         an issue at diesels issue tracker containing \
                         code how to trigger this message.",
                );

            if let Some(buffer) = buffer {
                wasm.dealloc(buffer);
            }
        }
        if let Some(query) = self.query.take() {
            std::mem::drop(query);
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

    pub(super) fn run(mut self) -> QueryResult<()> {
        // This is safe as we pass `first_step = true`
        // and we consume the statement so nobody could
        // access the columns later on anyway.
        let r = self.step(true).map(|_| ());
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
    pub(super) fn step(&mut self, first_step: bool) -> QueryResult<bool> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let res = match sqlite3.step(&self.statement.statement.inner_statement) {
            v if *ffi::SQLITE_DONE == v => Ok(false),
            v if *ffi::SQLITE_ROW == v => Ok(true),
            _ => Err(last_error(&self.statement.statement.raw_connection())),
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

    pub(super) fn copy_value(&self, idx: i32) -> Option<OwnedSqliteValue> {
        OwnedSqliteValue::copy_from_ptr(self.column_value(idx))
    }

    pub(super) fn column_value(&self, idx: i32) -> *mut u8 {
        let sqlite3 = crate::get_sqlite_unchecked();
        sqlite3.column_value(&self.statement.statement.inner_statement, idx)
    }
}

/*
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
*/
