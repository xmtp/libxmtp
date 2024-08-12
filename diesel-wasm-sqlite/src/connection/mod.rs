mod bind_collector;
// mod functions;
mod owned_row;
mod raw;
mod row;
// mod serialized_database;
mod sqlite_value;
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
use diesel::connection::{DefaultLoadingMode, LoadConnection};
use diesel::deserialize::{FromSqlRow, StaticallySizedRow};
use diesel::expression::QueryMetadata;
use diesel::result::*;
use diesel::serialize::ToSql;
use diesel::sql_types::HasSqlType;
use futures::{FutureExt, TryFutureExt};

use diesel::{connection::{ConnectionSealed, Instrumentation, WithMetadataLookup}, query_builder::{AsQuery, QueryFragment, QueryId}, sql_types::TypeMetadata, QueryResult};
pub use diesel_async::{AnsiTransactionManager, AsyncConnection, SimpleAsyncConnection, TransactionManager, stmt_cache::StmtCache};
use futures::{future::BoxFuture, stream::BoxStream};
use row::SqliteRow;

use crate::{get_sqlite_unchecked, WasmSqlite, WasmSqliteError};

pub struct WasmSqliteConnection {
    // statement_cache needs to be before raw_connection
    // otherwise we will get errors about open statements before closing the
    // connection itself
    statement_cache: StmtCache<WasmSqlite, Statement>,
    raw_connection: RawConnection,
    transaction_state: AnsiTransactionManager,
    // this exists for the sole purpose of implementing `WithMetadataLookup` trait
    // and avoiding static mut which will be deprecated in 2024 edition
    metadata_lookup: (),
    instrumentation: Option<Box<dyn Instrumentation>>,
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
    type ExecuteFuture<'conn, 'query> = BoxFuture<'query, QueryResult<usize>>;
    type LoadFuture<'conn, 'query> = BoxFuture<'query, QueryResult<Self::Stream<'conn, 'query>>>;
    type Stream<'conn, 'query> = BoxStream<'static, QueryResult<SqliteRow<'conn, 'query>>>;
    type Row<'conn, 'query> = SqliteRow<'conn, 'query>;

    async fn establish(database_url: &str) -> diesel::prelude::ConnectionResult<Self> {
        Ok(WasmSqliteConnection {
            statement_cache: StmtCache::new(),
            raw_connection: raw::RawConnection::establish(database_url).await.unwrap(),
            transaction_state: Default::default(),
            metadata_lookup: (),
            instrumentation: None
        })
    }

    fn load<'conn, 'query, T>(&'conn mut self, _source: T) -> Self::LoadFuture<'conn, 'query>
    where
        T: AsQuery + 'query,
        T::Query: QueryFragment<Self::Backend> + QueryId + 'query,
    {
        todo!()
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
/*
    fn prepared_query<'conn, 'query, T>(
        &'conn mut self,
        source: T,
    ) -> QueryResult<StatementUse<'conn, 'query>>
    where
        T: QueryFragment<WasmSqlite> + QueryId + 'query,
    {
        let raw_connection = &self.raw_connection;
        let cache = &mut self.statement_cache;
        let statement = match cache.cached_prepared_statement(
            &source,
            &WasmSqlite,
            &[],
            |sql, is_cached| Statement::prepare(raw_connection, sql, is_cached),
            &mut self.instrumentation,
        ) {
            Ok(statement) => statement,
            Err(e) => {
                return Err(e);
            }
        };

        StatementUse::bind(statement, source, &mut self.instrumentation)
    }
    */
          /*
    #[doc(hidden)]
    pub fn register_sql_function<ArgsSqlType, RetSqlType, Args, Ret, F>(
        &mut self,
        fn_name: &str,
        deterministic: bool,
        mut f: F,
    ) -> QueryResult<()>
    where
        F: FnMut(Args) -> Ret + std::panic::UnwindSafe + Send + 'static,
        Args: FromSqlRow<ArgsSqlType, WasmSqlite> + StaticallySizedRow<ArgsSqlType, WasmSqlite>,
        Ret: ToSql<RetSqlType, WasmSqlite>,
        WasmSqlite: HasSqlType<RetSqlType>,
    {
        functions::register(
            &self.raw_connection,
            fn_name,
            deterministic,
            move |_, args| f(args),
        )
    }
    
    #[doc(hidden)]
    pub fn register_noarg_sql_function<RetSqlType, Ret, F>(
        &self,
        fn_name: &str,
        deterministic: bool,
        f: F,
    ) -> QueryResult<()>
    where
        F: FnMut() -> Ret + std::panic::UnwindSafe + Send + 'static,
        Ret: ToSql<RetSqlType, WasmSqlite>,
        WasmSqlite: HasSqlType<RetSqlType>,
    {
        functions::register_noargs(&self.raw_connection, fn_name, deterministic, f)
    }

    #[doc(hidden)]
    pub fn register_aggregate_function<ArgsSqlType, RetSqlType, Args, Ret, A>(
        &mut self,
        fn_name: &str,
    ) -> QueryResult<()>
    where
        A: SqliteAggregateFunction<Args, Output = Ret> + 'static + Send + std::panic::UnwindSafe,
        Args: FromSqlRow<ArgsSqlType, Sqlite> + StaticallySizedRow<ArgsSqlType, Sqlite>,
        Ret: ToSql<RetSqlType, Sqlite>,
        Sqlite: HasSqlType<RetSqlType>,
    {
        functions::register_aggregate::<_, _, _, _, A>(&self.raw_connection, fn_name)
    }
    
    /// Register a collation function.
    ///
    /// `collation` must always return the same answer given the same inputs.
    /// If `collation` panics and unwinds the stack, the process is aborted, since it is used
    /// across a C FFI boundary, which cannot be unwound across and there is no way to
    /// signal failures via the SQLite interface in this case..
    ///
    /// If the name is already registered it will be overwritten.
    ///
    /// This method will return an error if registering the function fails, either due to an
    /// out-of-memory situation or because a collation with that name already exists and is
    /// currently being used in parallel by a query.
    ///
    /// The collation needs to be specified when creating a table:
    /// `CREATE TABLE my_table ( str TEXT COLLATE MY_COLLATION )`,
    /// where `MY_COLLATION` corresponds to name passed as `collation_name`.
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
    /// // sqlite NOCASE only works for ASCII characters,
    /// // this collation allows handling UTF-8 (barring locale differences)
    /// conn.register_collation("RUSTNOCASE", |rhs, lhs| {
    ///     rhs.to_lowercase().cmp(&lhs.to_lowercase())
    /// })
    /// # }
    /// ```
    pub fn register_collation<F>(&mut self, collation_name: &str, collation: F) -> QueryResult<()>
    where
        F: Fn(&str, &str) -> std::cmp::Ordering + Send + 'static + std::panic::UnwindSafe,
    {
        self.raw_connection
            .register_collation_function(collation_name, collation)
    }
    /// Serialize the current SQLite database into a byte buffer.
    ///
    /// The serialized data is identical to the data that would be written to disk if the database
    /// was saved in a file.
    ///
    /// # Returns
    ///
    /// This function returns a byte slice representing the serialized database.
    pub fn serialize_database_to_buffer(&mut self) -> SerializedDatabase {
        self.raw_connection.serialize()
    }
    
    /// Deserialize an SQLite database from a byte buffer.
    ///
    /// This function takes a byte slice and attempts to deserialize it into a SQLite database.
    /// If successful, the database is loaded into the connection. If the deserialization fails,
    /// an error is returned.
    ///
    /// The database is opened in READONLY mode.
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use diesel::sqlite::SerializedDatabase;
    /// # use diesel::sqlite::SqliteConnection;
    /// # use diesel::result::QueryResult;
    /// # use diesel::sql_query;
    /// # use diesel::Connection;
    /// # use diesel::RunQueryDsl;
    /// # fn main() {
    /// let connection = &mut SqliteConnection::establish(":memory:").unwrap();
    ///
    /// sql_query("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, email TEXT)")
    ///     .execute(connection).unwrap();
    /// sql_query("INSERT INTO users (name, email) VALUES ('John Doe', 'john.doe@example.com'), ('Jane Doe', 'jane.doe@example.com')")
    ///     .execute(connection).unwrap();
    ///
    /// // Serialize the database to a byte vector
    /// let serialized_db: SerializedDatabase = connection.serialize_database_to_buffer();
    ///
    /// // Create a new in-memory SQLite database
    /// let connection = &mut SqliteConnection::establish(":memory:").unwrap();
    ///
    /// // Deserialize the byte vector into the new database
    /// connection.deserialize_readonly_database_from_buffer(serialized_db.as_slice()).unwrap();
    /// #
    /// # }
    /// ```
    pub fn deserialize_readonly_database_from_buffer(&mut self, data: &[u8]) -> QueryResult<()> {
        self.raw_connection.deserialize(data)
    }
    */
   /* 
    async fn register_diesel_sql_functions(&self) -> QueryResult<()> {
        use diesel::sql_types::{Integer, Text};

        functions::register::<Text, Integer, _, _, _>(
            &self.raw_connection,
            "diesel_manage_updated_at",
            false,
            |conn, table_name: String| async {
                conn.exec(&format!(
                    include_str!("diesel_manage_updated_at.sql"),
                    table_name = table_name
                ))
                .await
                .expect("Failed to create trigger");
                0 // have to return *something*
            }.boxed(),
        )
    }
    */

    async fn establish_inner(database_url: &str) -> Result<WasmSqliteConnection, ConnectionError> {
        use diesel::result::ConnectionError::CouldntSetupConfiguration;
        let raw_connection = RawConnection::establish(database_url).await.unwrap();
        let conn = Self {
            statement_cache: StmtCache::new(),
            raw_connection,
            transaction_state: AnsiTransactionManager::default(),
            metadata_lookup: (),
            instrumentation: None,
        };
        /*
        conn.register_diesel_sql_functions()
            .map_err(CouldntSetupConfiguration).await?;
    */
        Ok(conn)
    }
}

/*
fn error_message(err_code: i32) -> &'static str {
    let sqlite3 = diesel::get_sqlite_unchecked();
    sqlite3.code_to_str(err_code)
}
*/
