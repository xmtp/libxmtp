#![allow(unsafe_code)] //TODO: can probably remove for wa-sqlite
use super::bind_collector::{OwnedSqliteBindValue, SqliteBindCollectorData};
use super::raw::RawConnection;
use super::sqlite_value::OwnedSqliteValue;
use crate::ffi::SQLiteCompatibleType;
use crate::{
    sqlite_types::{self, PrepareOptions, SqlitePrepareFlags},
    SqliteType, WasmSqliteError, WasmSqlite, connection::{SqliteBindCollector, bind_collector::InternalSqliteBindValue},

};
use diesel::{
    query_builder::{QueryId, QueryFragment},
    connection::{
        statement_cache::{MaybeCached, PrepareForCache},
        Instrumentation,
    },
    result::{Error, QueryResult},
};
use js_sys::AsyncIterator;
use std::cell::OnceCell;

use tokio::sync::oneshot;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

// this is OK b/c web runs in one thread
unsafe impl Send for Statement {}
#[derive(Debug)]
pub(super) struct Statement {
    // each iteration compiles a new statement for use
    inner_statement: JsValue,
}

impl Statement {
    pub fn prepare(
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

        // placeholder until we allocate with `wasm`
        let stmt = JsValue::NULL;

        let stmt = sqlite3
            .prepare_v3(
                &raw_connection.internal_connection,
                sql,
                -1,
                flags.unwrap_or(0),
                stmt,
                JsValue::NULL,
            )
            .map_err(WasmSqliteError::from)?;
        Ok(Self {
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
    ) -> QueryResult<Option<JsValue>> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let value =
            serde_wasm_bindgen::to_value(&value).expect("Bind value failed to convert to JsValue");
        tracing::info!("Statement: {:?}", self.inner_statement);

        let result = sqlite3
            .bind(&self.inner_statement, bind_index, value.into())
            .expect("could not bind");

        Ok(Some(result))
    }

    fn reset(&self) -> QueryResult<()> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let _ = sqlite3
            .reset(&self.inner_statement)
            .map_err(WasmSqliteError::from)?;
        Ok(())
    }

    fn clear_bindings(&self) -> QueryResult<()> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let _ = sqlite3
            .clear_bindings(&self.inner_statement)
            .map_err(WasmSqliteError::from)?;
        Ok(())
    }
}

impl Drop for Statement {
    fn drop(&mut self) {
        let sqlite3 = crate::get_sqlite_unchecked();
        // TODO:insipx potential problems here.
        // wa-sqlite does not throw an error if finalize fails:  -- it might just crash
        // doc: https://rhashimoto.github.io/wa-sqlite/docs/interfaces/SQLiteAPI.html#finalize.finalize-1
        // in that case we might not know if this errored or not
        // maybe depends how wasm panic/errors work
        // Worth unit testing the Drop implementation.
        tracing::info!("Statement dropped & finalized!");
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
    binds_to_free: Vec<(i32, Option<JsValue>)>,
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

        Ok(ret)
    }

    // This is a separated function so that
    // not the whole constructor is generic over the query type T.
    // This hopefully prevents binary bloat.
    fn bind_buffers(&mut self, binds: Vec<(InternalSqliteBindValue<'_>, SqliteType)>) -> QueryResult<()> {
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
            let is_borrowed_bind = matches!(bind,
                InternalSqliteBindValue::BorrowedString(_)
                    |   InternalSqliteBindValue::BorrowedBinary(_)
            );
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
            } else if is_borrowed_bind {
                // Store the id's of borrowed binds to unbind them on drop
                self.binds_to_free.push((bind_idx, None));
            }

        }
        Ok(())
    }

    fn finish_query_with_error(mut self, _e: &Error) {
        self.has_error = true;
    }
}

// we have to free the wawsm memory here not C memory so this will change significantly
impl<'stmt, 'query> Drop for BoundStatement<'stmt, 'query> {
    fn drop(&mut self) {
        self.statement.reset();
        self.statement.clear_bindings();
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
            /*
            if let Some(buffer) = buffer {
                unsafe {
                    // Constructing the `Box` here is safe as we
                    // got the pointer from a box + it is guaranteed to be not null.
                    std::mem::drop(Box::from_raw(buffer.as_ptr()));
                }
            }
            */
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
        T: QueryFragment<WasmSqlite> + QueryId + 'query
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
        let res = match serde_wasm_bindgen::from_value::<i32>(
            sqlite3
                .step(&self.statement.statement.inner_statement)
                .unwrap(),
        )
        .unwrap()
        {
            sqlite_types::SQLITE_DONE => Ok(false),
            sqlite_types::SQLITE_ROW => Ok(true),
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

    pub(super) fn copy_value(&self, idx: i32) -> Option<OwnedSqliteValue> {
        OwnedSqliteValue::copy_from_ptr(&self.column_value(idx)?.into())
    }

    pub(super) fn column_value(&self, idx: i32) -> Option<SQLiteCompatibleType> {
        let sqlite3 = crate::get_sqlite_unchecked();
        Some(sqlite3.column(&self.statement.statement.inner_statement, idx))
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
