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
use diesel::{connection::{statement_cache::StatementCacheKey}, query_builder::QueryBuilder as _, result::*};
use futures::future::LocalBoxFuture;
use futures::stream::LocalBoxStream;
use futures::FutureExt;
use statement_stream::PrivateStatementStream;
use std::sync::{Arc, Mutex};


use diesel::{connection::{ConnectionSealed, Instrumentation}, query_builder::{AsQuery, QueryFragment, QueryId}, QueryResult};
pub use diesel_async::{AnsiTransactionManager, AsyncConnection, SimpleAsyncConnection, TransactionManager, stmt_cache::StmtCache};
use row::SqliteRow;

use crate::{get_sqlite_unchecked, WasmSqlite, WasmSqliteError};

pub struct WasmSqliteConnection {
    // statement_cache needs to be before raw_connection
    // otherwise we will get errors about open statements before closing the
    // connection itself
    statement_cache: StmtCache<WasmSqlite, Statement>,
    pub raw_connection: RawConnection,
    transaction_state: AnsiTransactionManager,
    // this exists for the sole purpose of implementing `WithMetadataLookup` trait
    // and avoiding static mut which will be deprecated in 2024 edition
    metadata_lookup: (),
    instrumentation: Arc<Mutex<Option<Box<dyn Instrumentation>>>>,
}


// This relies on the invariant that RawConnection or Statement are never
// leaked. If a reference to one of those was held on a different thread, this
// would not be thread safe.
// Web is in one thread. Web workers can be used to hold a WasmSqliteConnection
// separately.

#[allow(unsafe_code)]
unsafe impl Send for WasmSqliteConnection {}


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
    type ExecuteFuture<'conn, 'query> = LocalBoxFuture<'query, QueryResult<usize>>;
    type LoadFuture<'conn, 'query> = LocalBoxFuture<'query, QueryResult<Self::Stream<'conn, 'query>>>;
    type Stream<'conn, 'query> = LocalBoxStream<'query, QueryResult<SqliteRow<'conn, 'query>>>;
    type Row<'conn, 'query> = SqliteRow<'conn, 'query>;

    async fn establish(database_url: &str) -> diesel::prelude::ConnectionResult<Self> {
        WasmSqliteConnection::establish_inner(database_url).await
    }

    fn load<'conn, 'query, T>(&'conn mut self, source: T) -> Self::LoadFuture<'conn, 'query>
    where
        T: AsQuery + 'query,
        T::Query: QueryFragment<Self::Backend> + QueryId + 'query,
    {
        async {
            let statement = self.prepared_query(source.as_query()).await?;
            Ok(PrivateStatementStream::new(statement).stream())
        }.boxed_local()
    }

    fn execute_returning_count<'conn, 'query, T>(
        &'conn mut self,
        _source: T,
    ) -> Self::ExecuteFuture<'conn, 'query>
    where
        T: QueryFragment<Self::Backend> + QueryId + 'query,
    {
        todo!()
    }
    
    fn transaction_state(
        &mut self,
    ) -> &mut <Self::TransactionManager as diesel_async::TransactionManager<Self>>::TransactionStateData{
        todo!()
    }

    fn instrumentation(&mut self) -> &mut dyn Instrumentation {
        todo!()
    }

    fn set_instrumentation(&mut self, _instrumentation: impl Instrumentation) {
        todo!()
    }
}

/*
impl LoadConnection<DefaultLoadingMode> for WasmSqliteConnection {
    type Cursor<'conn, 'query> = StatementIterator<'conn, 'query>;
    type Row<'conn, 'query> = self::row::SqliteRow<'conn, 'query>;

    fn load<'conn, 'query, T>(
        &'conn mut self,
        source: T,
    ) -> QueryResult<Self::Cursor<'conn, 'query>>
    where
        T: Query + QueryFragment<Self::Backend> + QueryId + 'query,
        Self::Backend: QueryMetadata<T::SqlType>,
    {
        let statement = self.prepared_query(source)?;

        Ok(StatementIterator::new(statement))
    }
}
*/
/*
impl WithMetadataLookup for WasmSqliteConnection {
    fn metadata_lookup(&mut self) -> &mut <WasmSqlite as TypeMetadata>::MetadataLookup {
        &mut self.metadata_lookup
    }
}
 */

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
    
    async fn prepared_query<'conn, 'query, T>(
        &'conn mut self,
        source: T,
    ) -> QueryResult<StatementUse<'conn, 'query>>
    where
        T: QueryFragment<WasmSqlite> + QueryId + 'query,
    {
        let raw_connection = &self.raw_connection;
        let cache = &mut self.statement_cache;
        let maybe_type_id = T::query_id();
        let cache_key = StatementCacheKey::for_source(maybe_type_id, &source, &[], &WasmSqlite)?;
       

        let is_safe_to_cache_prepared = source.is_safe_to_cache_prepared(&WasmSqlite)?;
        let mut qb = SqliteQueryBuilder::new();
        let sql = source.to_sql(&mut qb, &WasmSqlite).map(|()| qb.finish())?;
        
        let statement = cache.cached_prepared_statement(
            cache_key,
            sql,
            is_safe_to_cache_prepared,
            &[],
            raw_connection.clone(),
            &self.instrumentation,
        ).await?.0; // Cloned RawConnection is dropped here
        

        Ok(StatementUse::bind(statement, source, self.instrumentation.as_ref())?)
        
    }
    
    async fn establish_inner(database_url: &str) -> Result<WasmSqliteConnection, ConnectionError> {
        // use diesel::result::ConnectionError::CouldntSetupConfiguration;
        let raw_connection = RawConnection::establish(database_url).await.unwrap();
        let sqlite3 = crate::get_sqlite().await;
        
        sqlite3.register_diesel_sql_functions(&raw_connection.internal_connection).map_err(WasmSqliteError::from)?;

        Ok(Self {
            statement_cache: StmtCache::new(),
            raw_connection,
            transaction_state: AnsiTransactionManager::default(),
            metadata_lookup: (),
            instrumentation: Arc::new(Mutex::new(None)),
        })
    }
}
