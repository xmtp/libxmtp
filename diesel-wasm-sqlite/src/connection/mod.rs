mod bind_collector;
mod err;
mod owned_row;
mod raw;
mod row;
mod serialized_database;
mod sqlite_value;
mod statement_iterator;
mod stmt;

pub(crate) use self::bind_collector::SqliteBindCollector;
pub use self::bind_collector::SqliteBindValue;
pub use self::sqlite_value::SqliteValue;
// pub use self::serialized_database::SerializedDatabase;

use self::raw::RawConnection;
pub use self::statement_iterator::*;
use self::stmt::{Statement, StatementUse};
use err::*;
// use diesel::connection::DynInstrumentation;
use diesel::{
    connection::WithMetadataLookup,
    connection::{statement_cache::StatementCache, DefaultLoadingMode, LoadConnection},
    expression::QueryMetadata,
    query_builder::Query,
    result::*,
    sql_types::TypeMetadata,
    RunQueryDsl,
};

use diesel::{
    connection::{
        AnsiTransactionManager, Connection, ConnectionSealed, Instrumentation, SimpleConnection,
        TransactionManager,
    },
    query_builder::{QueryFragment, QueryId},
    QueryResult,
};
use serialized_database::SerializedDatabase;

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
    metadata_lookup: (),
    // this exists for the sole purpose of implementing `WithMetadataLookup` trait
    // and avoiding static mut which will be deprecated in 2024 edition
    instrumentation: Box<dyn Instrumentation>,
}

impl ConnectionSealed for WasmSqliteConnection {}

impl SimpleConnection for WasmSqliteConnection {
    fn batch_execute(&mut self, query: &str) -> diesel::prelude::QueryResult<()> {
        get_sqlite_unchecked()
            .exec(&self.raw_connection.internal_connection, query)
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
        self.instrumentation = Box::new(instrumentation);
    }

    fn instrumentation(&mut self) -> &mut dyn Instrumentation {
        self.instrumentation.as_mut()
    }

    fn transaction_state(&mut self) -> &mut AnsiTransactionManager
    where
        Self: Sized,
    {
        &mut self.transaction_manager
    }

    fn set_prepared_statement_cache_size(&mut self, size: diesel::connection::CacheSize) {
        self.statement_cache.set_cache_size(size)
    }
}

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

impl WithMetadataLookup for WasmSqliteConnection {
    fn metadata_lookup(&mut self) -> &mut <WasmSqlite as TypeMetadata>::MetadataLookup {
        &mut self.metadata_lookup
    }
}

impl diesel::migration::MigrationConnection for WasmSqliteConnection {
    fn setup(&mut self) -> QueryResult<usize> {
        use diesel::RunQueryDsl;
        diesel::sql_query(diesel::migration::CREATE_MIGRATIONS_TABLE).execute(self)
    }
}

#[derive(diesel::QueryId)]
pub(crate) struct CheckConnectionQuery;

impl<DB> QueryFragment<DB> for CheckConnectionQuery
where
    DB: diesel::backend::Backend,
{
    fn walk_ast<'b>(
        &'b self,
        mut pass: diesel::query_builder::AstPass<'_, 'b, DB>,
    ) -> QueryResult<()> {
        pass.push_sql("SELECT 1");
        Ok(())
    }
}

impl Query for CheckConnectionQuery {
    type SqlType = diesel::sql_types::Integer;
}

impl<C> RunQueryDsl<C> for CheckConnectionQuery {}

#[cfg(feature = "r2d2")]
impl diesel::r2d2::R2D2Connection for crate::connection::WasmSqliteConnection {
    fn ping(&mut self) -> QueryResult<()> {
        CheckConnectionQuery.execute(self).map(|_| ())
    }

    fn is_broken(&mut self) -> bool {
        AnsiTransactionManager::is_broken_transaction_manager(self)
    }
}

impl diesel::connection::MultiConnectionHelper for WasmSqliteConnection {
    fn to_any<'a>(
        lookup: &mut <Self::Backend as diesel::sql_types::TypeMetadata>::MetadataLookup,
    ) -> &mut (dyn std::any::Any + 'a) {
        lookup
    }

    fn from_any(
        lookup: &mut dyn std::any::Any,
    ) -> Option<&mut <Self::Backend as diesel::sql_types::TypeMetadata>::MetadataLookup> {
        lookup.downcast_mut()
    }
}

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
            instrumentation.as_mut(),
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

        StatementUse::bind(statement, source, instrumentation.as_mut())
    }

    fn establish_inner(database_url: &str) -> Result<WasmSqliteConnection, ConnectionError> {
        let sqlite3 = crate::get_sqlite_unchecked();
        let raw_connection = RawConnection::establish(database_url).unwrap();
        tracing::debug!(
            "Established database at {}",
            sqlite3.filename(&raw_connection.internal_connection, "main".into())
        );
        sqlite3
            .register_diesel_sql_functions(&raw_connection.internal_connection)
            .map_err(WasmSqliteError::from)?;
        Ok(Self {
            statement_cache: StatementCache::new(),
            raw_connection,
            transaction_manager: AnsiTransactionManager::default(),
            instrumentation: Box::new(Nothing) as Box<dyn Instrumentation>,
            metadata_lookup: (),
        })
    }

    pub fn serialize(&self) -> SerializedDatabase {
        self.raw_connection.serialize()
    }

    pub fn deserialize(&self, data: &[u8]) -> i32 {
        self.raw_connection.deserialize(data)
    }
}

pub struct Nothing;

impl Instrumentation for Nothing {
    fn on_connection_event(&mut self, event: diesel::connection::InstrumentationEvent<'_>) {
        tracing::trace!("{:?}", event);
    }
}
