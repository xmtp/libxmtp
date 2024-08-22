#![allow(unsafe_code)] //TODO: can probably remove for wa-sqlite
use super::bind_collector::{OwnedSqliteBindValue, SqliteBindCollectorData};
use super::raw::RawConnection;
use super::sqlite_value::OwnedSqliteValue;
use crate::ffi::SQLiteCompatibleType;
use crate::{
    sqlite_types::{self, PrepareOptions, SqlitePrepareFlags},
    SqliteType, WasmSqliteError,
};
use diesel::{
    connection::{
        statement_cache::{MaybeCached, PrepareForCache},
        Instrumentation,
    },
    result::{Error, QueryResult},
};
use js_sys::AsyncIterator;
use std::cell::OnceCell;
use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;
use wasm_bindgen::JsValue;
use wasm_bindgen_futures::JsFuture;

// TODO: Drop impl make sure to free JS async iterat9or
pub struct StatementFactory {
    statement_iterator: js_sys::AsyncIterator,
}

impl StatementFactory {
    pub async fn new(
        raw_connection: &RawConnection,
        sql: &str,
        is_cached: PrepareForCache,
    ) -> QueryResult<Self> {
        tracing::debug!("new statement factory, is_cached = {:?}", is_cached);
        let sqlite3 = crate::get_sqlite_unchecked();
        let flags = if matches!(is_cached, PrepareForCache::Yes) {
            Some(SqlitePrepareFlags::SQLITE_PREPARE_PERSISTENT.bits())
        } else {
            None
        };

        let options = PrepareOptions {
            flags,
            unscoped: Some(true),
        };

        let stmt = sqlite3
            // TODO: rename to something more fitting
            .prepare(
                &raw_connection.internal_connection,
                sql,
                serde_wasm_bindgen::to_value(&options).unwrap(),
            )
            .await
            .map_err(WasmSqliteError::from)?;

        let statement_iterator = js_sys::AsyncIterator::from(stmt);
        Ok(Self { statement_iterator })
    }

    /// compile a new statement based on given SQL in [`StatementFactory`]
    pub async fn prepare(&self) -> Statement {
        let inner_statement = JsFuture::from(self.statement_iterator.next().expect("No Next"))
            .await
            .expect("statement failed to compile");
        let inner_statement: JsValue = js_sys::Reflect::get(&inner_statement, &"value".into())
            .expect("Async Iterator API should be stable");
        tracing::debug!("Statement: {:?}", inner_statement);

        Statement { inner_statement }
    }
}

// this is OK b/c web runs in one thread
unsafe impl Send for Statement {}
#[derive(Debug)]
pub(super) struct Statement {
    // each iteration compiles a new statement for use
    inner_statement: JsValue,
}

impl Statement {
    // The caller of this function has to ensure that:
    // * Any buffer provided as `SqliteBindValue::BorrowedBinary`, `SqliteBindValue::Binary`
    // `SqliteBindValue::String` or `SqliteBindValue::BorrowedString` is valid
    // till either a new value is bound to the same parameter or the underlying
    // prepared statement is dropped.
    fn bind(
        &self,
        _tpe: SqliteType,
        value: OwnedSqliteBindValue,
        bind_index: i32,
    ) -> QueryResult<i32> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let value =
            serde_wasm_bindgen::to_value(&value).expect("Bind value failed to convert to JsValue");
        tracing::info!("Statement: {:?}", self.inner_statement);

        let result = sqlite3
            .bind(&self.inner_statement, bind_index, value.into())
            .expect("could not bind");

        // TODO:insipx Pretty sure we can have a simpler implementation here vs diesel
        // making use of `wa-sqlite` `bind` which abstracts over the individual bind functions in
        // sqlite3. However, not sure  how this will work further up the stack.
        // This might not work because of differences in how serde_json recognizes js types
        // and how wa-sqlite recogizes js types. In that case, need to resort to matching on
        // individual types with bind_$type fns .

        Ok(result)
    }

    async fn reset(&self) -> QueryResult<()> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let _ = sqlite3
            .reset(&self.inner_statement)
            .await
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
struct BoundStatement<'stmt> {
    statement: MaybeCached<'stmt, Statement>,
    // we need to store the query here to ensure no one does
    // drop it till the end of the statement
    // We use a boxed queryfragment here just to erase the
    // generic type, we use NonNull to communicate
    // that this is a shared buffer
    // query: Option<Box<dyn QueryFragment<WasmSqlite>>>,
    #[allow(unused)]
    instrumentation: Arc<Mutex<dyn Instrumentation>>,
    has_error: bool,
    drop_signal: Option<oneshot::Sender<JsValue>>,
}

impl<'stmt> BoundStatement<'stmt> {
    fn bind(
        statement: MaybeCached<'stmt, Statement>,
        bind_collector: SqliteBindCollectorData,
        instrumentation: Arc<Mutex<dyn Instrumentation>>,
    ) -> QueryResult<BoundStatement<'stmt>> {
        match &statement {
            MaybeCached::CannotCache(s) => {
                tracing::debug!("BoundStatement::bind, NOT CACHED statement={:?}", s)
            }
            MaybeCached::Cached(s) => {
                tracing::debug!(
                    "BoundStatement::bind, MaybeCached::Cached statement={:?}",
                    s
                )
            }
            &_ => todo!(),
        }
        let SqliteBindCollectorData { binds } = bind_collector;

        let (tx, rx) = tokio::sync::oneshot::channel::<JsValue>();
        wasm_bindgen_futures::spawn_local(async move {
            let result = (|| async move {
                let inner_statement = rx.await?;
                let this = Statement { inner_statement };
                this.reset().await?;
                this.clear_bindings()?;
                tracing::debug!("Bound statement dropped succesfully!");
                // we forget here because we need a clone that's sent to this task
                // we don't want to `finalizing` this Statement yet (which is what
                // dropping it would do);
                std::mem::forget(this);
                Ok::<_, WasmSqliteError>(())
            })();
            if let Err(e) = result.await {
                tracing::error!("BoundStatement never dropped! {}", e);
            }
        });

        let mut ret = BoundStatement {
            statement,
            instrumentation,
            has_error: false,
            drop_signal: Some(tx),
        };

        ret.bind_buffers(binds)?;

        Ok(ret)
    }

    // This is a separated function so that
    // not the whole constructor is generic over the query type T.
    // This hopefully prevents binary bloat.
    fn bind_buffers(&mut self, binds: Vec<(OwnedSqliteBindValue, SqliteType)>) -> QueryResult<()> {
        for (bind_idx, (bind, tpe)) in (1..).zip(binds) {
            // It's safe to call bind here as:
            // * The type and value matches
            // * We ensure that corresponding buffers lives long enough below
            // * The statement is not used yet by `step` or anything else
            let _ = self.statement.bind(tpe, bind, bind_idx)?;

            // we don't track binds to free like sqlite3 C bindings
            // The assumption is that wa-sqlite, being WASM run in web browser that
            // lies in the middle of rust -> sqlite, takes care of this for us.
            // if we run into memory issues, especially memory leaks
            // this should be the first place to pay attention to.
            //
            // The bindings shuold be collected/freed with JS once `clear_bindings` is
            // run on `Drop` for `BoundStatement`
        }
        Ok(())
    }

    fn finish_query_with_error(mut self, _e: &Error) {
        self.has_error = true;
    }

    // FIXME: [`AsyncDrop`](https://github.com/rust-lang/rust/issues/126482) is a missing feature in rust.
    // Until then we need to manually reset the statement object.
    pub async fn reset(&mut self) -> QueryResult<()> {
        self.statement.reset().await?;
        self.statement.clear_bindings()?;
        Ok(())
    }
}

// Eventually replace with `AsyncDrop`: https://github.com/rust-lang/rust/issues/126482
impl<'stmt> Drop for BoundStatement<'stmt> {
    fn drop(&mut self) {
        let sender = self.drop_signal.take().expect("Drop may only be ran once");
        let _ = sender.send(self.statement.inner_statement.clone());
    }
}

#[allow(missing_debug_implementations)]
pub struct StatementUse<'stmt> {
    statement: BoundStatement<'stmt>,
    column_names: OnceCell<Vec<String>>,
}

impl<'stmt> StatementUse<'stmt> {
    pub(super) fn bind(
        statement: MaybeCached<'stmt, Statement>,
        bind_collector: SqliteBindCollectorData,
        instrumentation: Arc<Mutex<dyn Instrumentation>>,
    ) -> QueryResult<StatementUse<'stmt>>
where {
        Ok(Self {
            statement: BoundStatement::bind(statement, bind_collector, instrumentation)?,
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
        let res = match serde_wasm_bindgen::from_value::<i32>(
            sqlite3
                .step(&self.statement.statement.inner_statement)
                .await
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
