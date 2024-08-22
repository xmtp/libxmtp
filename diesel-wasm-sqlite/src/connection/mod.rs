mod bind_collector;
// mod functions;
mod owned_row;
mod raw;
mod row;
// mod serialized_database;
mod sqlite_value;
// mod statement_stream;
// mod statement_iterator;
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
use diesel::{connection::{statement_cache::{StatementCacheKey, StatementCache}}, query_builder::QueryBuilder as _, result::*};
// use diesel::connection::instrumentation::DynInstrumentation
use futures::future::LocalBoxFuture;
use futures::stream::LocalBoxStream;
use futures::FutureExt;
use owned_row::OwnedSqliteRow;
use std::future::Future;
use std::sync::{Arc, Mutex};


use diesel::{connection::{ConnectionSealed, Instrumentation, Connection, SimpleConnection, AnsiTransactionManager, TransactionManager}, query_builder::{AsQuery, QueryFragment, QueryId}, QueryResult};

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
    statement_cache: StatementCache<WasmSqlite, Statement>,
    raw_connection: RawConnection,
    transaction_manager: AnsiTransactionManager,
    // this exists for the sole purpose of implementing `WithMetadataLookup` trait
    // and avoiding static mut which will be deprecated in 2024 edition
    instrumentation: Option<Box<dyn Instrumentation>>,
}

impl ConnectionSealed for WasmSqliteConnection {}



impl SimpleConnection for WasmSqliteConnection {
    fn batch_execute(&mut self, query: &str) -> diesel::prelude::QueryResult<()> {
        get_sqlite_unchecked()
            .batch_execute(&self.raw_connection.internal_connection, query)
            .map_err(WasmSqliteError::from)
            .map_err(Into::into)
    }
}

impl Connection for WasmSqliteConnection {
    type Backend = WasmSqlite;
    type TransactionManager = AnsiTransactionManager;
    
    fn establish(database_url: &str) -> ConnectionResult<Self> {
        WasmSqliteConnection::establish_inner(database_url)
    }
    
    fn execute_returning_count<T>(&mut self, source: &T) -> QueryResult<usize>
    where
        T: QueryFragment<Self::Backend> + QueryId,
    {
        let statement_use = self.prepared_query(source)?;
        statement_use
            .run()
            .map(|_| self.raw_connection.rows_affected_by_last_query())
    }
    
    fn set_instrumentation(&mut self, instrumentation: impl Instrumentation) {
        self.instrumentation = Some(Box::new(instrumentation));
    }

    fn instrumentation(&mut self) -> &mut dyn Instrumentation{
        let instrumentation = self.instrumentation.as_mut().unwrap();
        &mut *instrumentation
    }
    
    fn transaction_state(&mut self) -> &mut AnsiTransactionManager
    where
        Self: Sized,
    {
        &mut self.transaction_manager
    }
}
/*
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
    pub fn immediate_transaction<T, E, F>(&mut self, f: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self) -> Result<T, E>,
        E: From<Error>,
    {
        self.transaction_sql(f, "BEGIN IMMEDIATE")
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
    pub fn exclusive_transaction<T, E, F>(&mut self, f: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self) -> Result<T, E>,
        E: From<Error>,
    {
        self.transaction_sql(f, "BEGIN EXCLUSIVE")
    }

    fn transaction_sql<T, E, F>(&mut self, f: F, sql: &str) -> Result<T, E>
    where
        F: FnOnce(&mut Self) -> Result<T, E>,
        E: From<Error>,
    {
        AnsiTransactionManager::begin_transaction_sql(&mut *self, sql)?;
        match f(&mut *self) {
            Ok(value) => {
                AnsiTransactionManager::commit_transaction(&mut *self)?;
                Ok(value)
            }
            Err(e) => {
                AnsiTransactionManager::rollback_transaction(&mut *self)?;
                Err(e)
            }
        }
    }
    

    fn prepared_query<'conn, 'query, T>(
        &'conn mut self,
        source: T,
    ) -> QueryResult<StatementUse<'conn, 'query>>
    where
        T: QueryFragment<WasmSqlite> + QueryId + 'query,
    {
        /*
        self.instrumentation
            .on_connection_event(InstrumentationEvent::StartQuery {
                query: &crate::debug_query(&source),
            });
        */
        let WasmSqliteConnection {
            ref mut raw_connection,
            ref mut statement_cache,
            ref mut instrumentation,
            ..
        } = self;
        
        let statement = match statement_cache.cached_statement(
            &source,
            &WasmSqlite,
            &[],
            |sql, is_cached| Statement::prepare(raw_connection, sql, is_cached),
            instrumentation.as_mut().unwrap(),
        ) {
            Ok(statement) => statement,
            Err(e) => {
                /*
                self.instrumentation
                    .on_connection_event(InstrumentationEvent::FinishQuery {
                        query: &crate::debug_query(&source),
                        error: Some(&e),
                    });
                */
                return Err(e);
            }
        };

        StatementUse::bind(statement, source, instrumentation.as_mut().unwrap())
    } 
   
    fn establish_inner(database_url: &str) -> Result<WasmSqliteConnection, ConnectionError> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let raw_connection = RawConnection::establish(database_url).unwrap();
        sqlite3.register_diesel_sql_functions(&raw_connection.internal_connection).map_err(WasmSqliteError::from)?;

        Ok(Self {
            statement_cache: StatementCache::new(),
            raw_connection,
            transaction_manager: AnsiTransactionManager::default(),
            instrumentation: None ,
        })
    }
}
