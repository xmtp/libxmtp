mod bind_collector;
// mod functions;
mod owned_row;
mod raw;
mod row;
// mod serialized_database;
mod sqlite_value;
mod statement_stream;
mod stmt;

pub(crate) use self::bind_collector::SqliteBindCollector;
pub use self::bind_collector::SqliteBindValue;
pub use self::sqlite_value::SqliteValue;
 // pub use self::serialized_database::SerializedDatabase; 
               
use self::raw::RawConnection;
// use self::statement_iterator::*;
use self::stmt::{Statement, StatementUse};
use crate::query_builder::*;
use diesel::query_builder::MoveableBindCollector;
use diesel::{connection::{statement_cache::StatementCacheKey}, query_builder::QueryBuilder as _, result::*};
use futures::future::LocalBoxFuture;
use futures::stream::LocalBoxStream;
use futures::FutureExt;
use owned_row::OwnedSqliteRow;
use statement_stream::StatementStream;
use std::future::Future;
use std::sync::{Arc, Mutex};


use diesel::{connection::{ConnectionSealed, Instrumentation}, query_builder::{AsQuery, QueryFragment, QueryId}, QueryResult};
pub use diesel_async::{AnsiTransactionManager, AsyncConnection, SimpleAsyncConnection, TransactionManager, stmt_cache::StmtCache};

use crate::{get_sqlite_unchecked, WasmSqlite, WasmSqliteError};

// This relies on the invariant that RawConnection or Statement are never
// leaked. If a reference to one of those was held on a different thread, this
// would not be thread safe.
// Web is in one thread. Web workers can establish & hold a WasmSqliteConnection
// separately.
#[allow(unsafe_code)]
unsafe impl Send for WasmSqliteConnection {}

pub struct WasmSqliteConnection {
    // statement_cache needs to be before raw_connection
    // otherwise we will get errors about open statements before closing the
    // connection itself
    statement_cache: StmtCache<WasmSqlite, Statement>,
    pub raw_connection: RawConnection,
    transaction_manager: AnsiTransactionManager,
    // this exists for the sole purpose of implementing `WithMetadataLookup` trait
    // and avoiding static mut which will be deprecated in 2024 edition
    instrumentation: Arc<Mutex<Option<Box<dyn Instrumentation>>>>,
}

impl ConnectionSealed for WasmSqliteConnection {}

 #[async_trait::async_trait(?Send)]
impl SimpleAsyncConnection for WasmSqliteConnection {
    async fn batch_execute(&mut self, query: &str) -> diesel::prelude::QueryResult<()> {
        get_sqlite_unchecked()
            .batch_execute(&self.raw_connection.internal_connection, query)
            .map_err(WasmSqliteError::from)
            .map_err(Into::into)
    }
}

#[async_trait::async_trait(?Send)]
impl AsyncConnection for WasmSqliteConnection {
    type Backend = WasmSqlite;
    type TransactionManager = AnsiTransactionManager;
    type ExecuteFuture<'conn, 'query> = LocalBoxFuture<'conn, QueryResult<usize>>;
    type LoadFuture<'conn, 'query> = LocalBoxFuture<'conn, QueryResult<Self::Stream<'conn, 'query>>>;
    type Stream<'conn, 'query> = LocalBoxStream<'conn, QueryResult<Self::Row<'conn, 'query>>>;
    type Row<'conn, 'query> = OwnedSqliteRow;

    async fn establish(database_url: &str) -> diesel::prelude::ConnectionResult<Self> {
        WasmSqliteConnection::establish_inner(database_url).await
    }

    fn load<'conn, 'query, T>(&'conn mut self, source: T) -> Self::LoadFuture<'conn, 'query>
    where
        T: AsQuery,
        T::Query: QueryFragment<Self::Backend> + QueryId,
    {
        let query = source.as_query();
        self.with_prepared_statement(query, |_, statement| async move {
            Ok(StatementStream::new(statement).stream())
        })
    }

    fn execute_returning_count<'conn, 'query, T>(
        &'conn mut self,
        query: T,
    ) -> Self::ExecuteFuture<'conn, 'query>
    where
        T: QueryFragment<Self::Backend> + QueryId + 'query,
    {
        self.with_prepared_statement(query, |conn, statement| async move {
            statement.run().await.map(|_| {
                conn.rows_affected_by_last_query()
            })
        })
    }
    
    fn transaction_state(
        &mut self,
    ) -> &mut <Self::TransactionManager as diesel_async::TransactionManager<Self>>::TransactionStateData{
        &mut self.transaction_manager
    }

    fn instrumentation(&mut self) -> &mut dyn Instrumentation {
        todo!()
    }

    fn set_instrumentation(&mut self, _instrumentation: impl Instrumentation) {
        todo!()
    }
}



#[cfg(feature = "r2d2")]
impl crate::r2d2::R2D2Connection for crate::sqlite::SqliteConnection {
    fn ping(&mut self) -> QueryResult<()> {
        use crate::RunQueryDsl;

        crate::r2d2::CheckConnectionQuery.execute(self).map(|_| ())
    }

    fn is_broken(&mut self) -> bool {
        AnsiTransactionManager::is_broken_transaction_manager(self)
    }
}
                                                /*
impl MultiConnectionHelper for SqliteConnection {
    fn to_any<'a>(
        lookup: &mut <Self::Backend as crate::sql_types::TypeMetadata>::MetadataLookup,
    ) -> &mut (dyn std::any::Any + 'a) {
        lookup
    }

    fn from_any(
        lookup: &mut dyn std::any::Any,
    ) -> Option<&mut <Self::Backend as crate::sql_types::TypeMetadata>::MetadataLookup> {
        lookup.downcast_mut()
    }
}
*/                      

impl WasmSqliteConnection {
    /// Run a transaction with `BEGIN IMMEDIATE`
    ///
    /// This method will return an error if a transaction is already open.
    ///
    /// # Example
    ///
    /// ```rust
    /// # include!("../../doctest_setup.rs");
    /// #
    /// # fn main() {
    /// #     run_test().unwrap();
    /// # }
    /// #
    /// # fn run_test() -> QueryResult<()> {
    /// #     let mut conn = SqliteConnection::establish(":memory:").unwrap();
    /// conn.immediate_transaction(|conn| {
    ///     // Do stuff in a transaction
    ///     Ok(())
    /// })
    /// # }
    /// ```
    pub async fn immediate_transaction<T, E, F>(&mut self, f: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self) -> Result<T, E>,
        E: From<Error>,
    {
        self.transaction_sql(f, "BEGIN IMMEDIATE").await
    }

    /// Run a transaction with `BEGIN EXCLUSIVE`
    ///
    /// This method will return an error if a transaction is already open.
    ///
    /// # Example
    ///
    /// ```rust
    /// # include!("../../doctest_setup.rs");
    /// #
    /// # fn main() {
    /// #     run_test().unwrap();
    /// # }
    /// #
    /// # fn run_test() -> QueryResult<()> {
    /// #     let mut conn = SqliteConnection::establish(":memory:").unwrap();
    /// conn.exclusive_transaction(|conn| {
    ///     // Do stuff in a transaction
    ///     Ok(())
    /// })
    /// # }
    /// ```
    pub async fn exclusive_transaction<T, E, F>(&mut self, f: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self) -> Result<T, E>,
        E: From<Error>,
    {
        self.transaction_sql(f, "BEGIN EXCLUSIVE").await
    }

    async fn transaction_sql<T, E, F>(&mut self, f: F, sql: &str) -> Result<T, E>
    where
        F: FnOnce(&mut Self) -> Result<T, E>,
        E: From<Error>,
    {
        AnsiTransactionManager::begin_transaction_sql(&mut *self, sql).await?;
        match f(&mut *self) {
            Ok(value) => {
                AnsiTransactionManager::commit_transaction(&mut *self).await?;
                Ok(value)
            }
            Err(e) => {
                AnsiTransactionManager::rollback_transaction(&mut *self).await?;
                Err(e)
            }
        }
    }
    
    fn with_prepared_statement<'conn, Q, F, R>(
        &'conn mut self,
        query: Q,
        callback: impl (FnOnce(&'conn mut RawConnection, StatementUse<'conn>) -> F) + 'conn
    ) -> LocalBoxFuture<'_, QueryResult<R>>
    where
        Q: QueryFragment<WasmSqlite> + QueryId,
        F: Future<Output = QueryResult<R>>,
    {
        let WasmSqliteConnection {
            ref mut raw_connection,
            ref mut statement_cache,
            ref mut instrumentation,
            ..
        } = self;
        
        let maybe_type_id = Q::query_id();
        let instrumentation = instrumentation.clone();
        
        let cache_key = StatementCacheKey::for_source(maybe_type_id, &query, &[], &WasmSqlite);
        let is_safe_to_cache_prepared = query.is_safe_to_cache_prepared(&WasmSqlite);
      
        // C put this in box to avoid virtual fn call for SQLite C
        // not sure if that still applies here
        let query = Box::new(query);
        let mut bind_collector = SqliteBindCollector::new();
        let bind_collector = query.collect_binds(&mut bind_collector, &mut (), &WasmSqlite).map(|()| bind_collector.moveable());
        
        let mut qb = SqliteQueryBuilder::new();
        let sql = query.to_sql(&mut qb, &WasmSqlite).map(|()| qb.finish()); 
        
        async move {
            let (statement, conn) = statement_cache.cached_prepared_statement(
                cache_key?,
                sql?,
                is_safe_to_cache_prepared?,
                &[],
                raw_connection,
                &instrumentation,
            ).await?; // Cloned RawConnection is dropped here
            let statement = StatementUse::bind(statement, bind_collector?, instrumentation)?;
            callback(conn, statement).await
        }.boxed_local()
    }
    
    async fn establish_inner(database_url: &str) -> Result<WasmSqliteConnection, ConnectionError> {
        // use diesel::result::ConnectionError::CouldntSetupConfiguration;
        let raw_connection = RawConnection::establish(database_url).await.unwrap();
        let sqlite3 = crate::get_sqlite().await;
        
        sqlite3.register_diesel_sql_functions(&raw_connection.internal_connection).map_err(WasmSqliteError::from)?;

        Ok(Self {
            statement_cache: StmtCache::new(),
            raw_connection,
            transaction_manager: AnsiTransactionManager::default(),
            instrumentation: Arc::new(Mutex::new(None)),
        })
    }
}
