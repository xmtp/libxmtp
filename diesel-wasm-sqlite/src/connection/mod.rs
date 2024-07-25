mod bind_collector;
// mod functions;
// mod owned_row;
mod raw;
// mod row;
// mod serialized_database;
// mod sqlite_value;
// mod statement_iterator;
// mod stmt;

pub(crate) use self::bind_collector::SqliteBindCollector;
pub use self::bind_collector::SqliteBindValue;
// pub use self::serialized_database::SerializedDatabase;
// pub use self::sqlite_value::SqliteValue;

/*
use self::raw::RawConnection;
use self::statement_iterator::*;
use self::stmt::{Statement, StatementUse};
use super::SqliteAggregateFunction;
use crate::connection::instrumentation::StrQueryHelper;
use crate::connection::statement_cache::StatementCache;
use crate::connection::*;
use crate::query_builder::*;
use crate::sqlite::WasmSqlite;
use diesel::deserialize::{FromSqlRow, StaticallySizedRow};
use diesel::expression::QueryMetadata;
use diesel::result::*;
use diesel::serialize::ToSql;
use diesel::sql_types::{HasSqlType, TypeMetadata};
*/
use diesel::{
    backend::Backend,
    connection::Instrumentation,
    query_builder::{AsQuery, QueryFragment, QueryId},
    result::QueryResult,
    row::Field,
};
use diesel_async::{AnsiTransactionManager, AsyncConnection, SimpleAsyncConnection};
use futures::stream::Stream;
use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{get_sqlite, get_sqlite_unchecked, WasmSqlite, WasmSqliteError};
use std::future::Ready;

unsafe impl Send for WasmSqliteConnection {}
#[derive(Debug)]
pub struct WasmSqliteConnection {
    raw: raw::RawConnection,
}

#[async_trait::async_trait(?Send)]
impl SimpleAsyncConnection for WasmSqliteConnection {
    async fn batch_execute(&mut self, query: &str) -> diesel::prelude::QueryResult<()> {
        get_sqlite_unchecked()
            .batch_execute(&self.raw.internal_connection, query)
            .map_err(WasmSqliteError::from)
            .map_err(Into::into)
    }
}

/// TODO: The placeholder stuff all needs to be re-done

pub struct OwnedSqliteFieldPlaceholder<'field> {
    field: PhantomData<&'field ()>,
}

impl<'f> Field<'f, WasmSqlite> for OwnedSqliteFieldPlaceholder<'f> {
    fn field_name(&self) -> Option<&str> {
        Some("placeholder")
    }
    fn value(&self) -> Option<<WasmSqlite as Backend>::RawValue<'_>> {
        todo!()
    }
}

pub struct InnerPartialRowPlaceholder;

pub struct RowReady(Option<()>);

impl std::future::Future for RowReady {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Poll::Ready(self.0.take().expect("`Ready` polled after completion"))
    }
}

impl diesel::row::RowSealed for RowReady {}

impl<'a, 'b> diesel::row::RowIndex<&'a str> for RowReady {
    fn idx(&self, idx: &'a str) -> Option<usize> {
        todo!()
    }
}

impl diesel::row::RowIndex<usize> for RowReady {
    fn idx(&self, idx: usize) -> Option<usize> {
        todo!()
    }
}

impl<'a> diesel::row::Row<'a, WasmSqlite> for RowReady {
    type Field<'f> = OwnedSqliteFieldPlaceholder<'f>
    where
        'a: 'f,
        Self: 'f;

    type InnerPartialRow = Self;

    fn field_count(&self) -> usize {
        todo!()
    }

    fn get<'b, I>(&'b self, idx: I) -> Option<Self::Field<'b>>
    where
        'a: 'b,
        Self: diesel::row::RowIndex<I>,
    {
        todo!()
    }

    fn partial_row(
        &self,
        range: std::ops::Range<usize>,
    ) -> diesel::row::PartialRow<'_, Self::InnerPartialRow> {
        todo!()
    }
}

pub struct RowReadyStreamPlaceholder;

impl Stream for RowReadyStreamPlaceholder {
    type Item = QueryResult<RowReady>;
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        todo!();
    }
}

impl diesel::connection::ConnectionSealed for WasmSqliteConnection {}

#[async_trait::async_trait(?Send)]
impl AsyncConnection for WasmSqliteConnection {
    type Backend = WasmSqlite;
    type TransactionManager = AnsiTransactionManager;
    // placeholders
    type ExecuteFuture<'conn, 'query> = Ready<QueryResult<usize>>;
    type LoadFuture<'conn, 'query> = Ready<QueryResult<Self::Stream<'conn, 'query>>>;
    type Stream<'conn, 'query> = RowReadyStreamPlaceholder;
    type Row<'conn, 'query> = RowReady;

    async fn establish(database_url: &str) -> diesel::prelude::ConnectionResult<Self> {
        Ok(WasmSqliteConnection {
            raw: raw::RawConnection::establish(database_url).await.unwrap(),
        })
    }

    fn load<'conn, 'query, T>(&'conn mut self, source: T) -> Self::LoadFuture<'conn, 'query>
    where
        T: AsQuery + 'query,
        T::Query: QueryFragment<Self::Backend> + QueryId + 'query,
    {
        todo!()
    }

    fn execute_returning_count<'conn, 'query, T>(
        &'conn mut self,
        source: T,
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

    fn set_instrumentation(&mut self, instrumentation: impl Instrumentation) {
        todo!()
    }
}

/*
pub struct SqliteConnection {
    // statement_cache needs to be before raw_connection
    // otherwise we will get errors about open statements before closing the
    // connection itself
    statement_cache: StatementCache<Sqlite, Statement>,
    raw_connection: RawConnection,
    transaction_state: AnsiTransactionManager,
    // this exists for the sole purpose of implementing `WithMetadataLookup` trait
    // and avoiding static mut which will be deprecated in 2024 edition
    metadata_lookup: (),
    instrumentation: Option<Box<dyn Instrumentation>>,
}
*/

// This relies on the invariant that RawConnection or Statement are never
// leaked. If a reference to one of those was held on a different thread, this
// would not be thread safe.
/*
#[allow(unsafe_code)]
unsafe impl Send for SqliteConnection {}

impl SimpleConnection for SqliteConnection {
    fn batch_execute(&mut self, query: &str) -> QueryResult<()> {
        self.instrumentation
            .on_connection_event(InstrumentationEvent::StartQuery {
                query: &StrQueryHelper::new(query),
            });
        let resp = self.raw_connection.exec(query);
        self.instrumentation
            .on_connection_event(InstrumentationEvent::FinishQuery {
                query: &StrQueryHelper::new(query),
                error: resp.as_ref().err(),
            });
        resp
    }
}
*/
/*
impl ConnectionSealed for SqliteConnection {}

impl Connection for SqliteConnection {
    type Backend = Sqlite;
    type TransactionManager = AnsiTransactionManager;

    /// Establish a connection to the database specified by `database_url`.
    ///
    /// See [SqliteConnection] for supported `database_url`.
    ///
    /// If the database does not exist, this method will try to
    /// create a new database and then establish a connection to it.
    fn establish(database_url: &str) -> ConnectionResult<Self> {
        let mut instrumentation = crate::connection::instrumentation::get_default_instrumentation();
        instrumentation.on_connection_event(InstrumentationEvent::StartEstablishConnection {
            url: database_url,
        });

        let establish_result = Self::establish_inner(database_url);
        instrumentation.on_connection_event(InstrumentationEvent::FinishEstablishConnection {
            url: database_url,
            error: establish_result.as_ref().err(),
        });
        let mut conn = establish_result?;
        conn.instrumentation = instrumentation;
        Ok(conn)
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

    fn transaction_state(&mut self) -> &mut AnsiTransactionManager
    where
        Self: Sized,
    {
        &mut self.transaction_state
    }

    fn instrumentation(&mut self) -> &mut dyn Instrumentation {
        &mut self.instrumentation
    }

    fn set_instrumentation(&mut self, instrumentation: impl Instrumentation) {
        self.instrumentation = Some(Box::new(instrumentation));
    }
}

impl LoadConnection<DefaultLoadingMode> for SqliteConnection {
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

impl WithMetadataLookup for SqliteConnection {
    fn metadata_lookup(&mut self) -> &mut <Sqlite as TypeMetadata>::MetadataLookup {
        &mut self.metadata_lookup
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

impl SqliteConnection {
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
        T: QueryFragment<Sqlite> + QueryId + 'query,
    {
        self.instrumentation
            .on_connection_event(InstrumentationEvent::StartQuery {
                query: &crate::debug_query(&source),
            });
        let raw_connection = &self.raw_connection;
        let cache = &mut self.statement_cache;
        let statement = match cache.cached_statement(
            &source,
            &Sqlite,
            &[],
            |sql, is_cached| Statement::prepare(raw_connection, sql, is_cached),
            &mut self.instrumentation,
        ) {
            Ok(statement) => statement,
            Err(e) => {
                self.instrumentation
                    .on_connection_event(InstrumentationEvent::FinishQuery {
                        query: &crate::debug_query(&source),
                        error: Some(&e),
                    });

                return Err(e);
            }
        };

        StatementUse::bind(statement, source, &mut self.instrumentation)
    }

    #[doc(hidden)]
    pub fn register_sql_function<ArgsSqlType, RetSqlType, Args, Ret, F>(
        &mut self,
        fn_name: &str,
        deterministic: bool,
        mut f: F,
    ) -> QueryResult<()>
    where
        F: FnMut(Args) -> Ret + std::panic::UnwindSafe + Send + 'static,
        Args: FromSqlRow<ArgsSqlType, Sqlite> + StaticallySizedRow<ArgsSqlType, Sqlite>,
        Ret: ToSql<RetSqlType, Sqlite>,
        Sqlite: HasSqlType<RetSqlType>,
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
        Ret: ToSql<RetSqlType, Sqlite>,
        Sqlite: HasSqlType<RetSqlType>,
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

    fn register_diesel_sql_functions(&self) -> QueryResult<()> {
        use crate::sql_types::{Integer, Text};

        functions::register::<Text, Integer, _, _, _>(
            &self.raw_connection,
            "diesel_manage_updated_at",
            false,
            |conn, table_name: String| {
                conn.exec(&format!(
                    include_str!("diesel_manage_updated_at.sql"),
                    table_name = table_name
                ))
                .expect("Failed to create trigger");
                0 // have to return *something*
            },
        )
    }

    fn establish_inner(database_url: &str) -> Result<SqliteConnection, ConnectionError> {
        use crate::result::ConnectionError::CouldntSetupConfiguration;
        let raw_connection = RawConnection::establish(database_url)?;
        let conn = Self {
            statement_cache: StatementCache::new(),
            raw_connection,
            transaction_state: AnsiTransactionManager::default(),
            metadata_lookup: (),
            instrumentation: None,
        };
        conn.register_diesel_sql_functions()
            .map_err(CouldntSetupConfiguration)?;
        Ok(conn)
    }
}
*/

/*
fn error_message(err_code: i32) -> &'static str {
    let sqlite3 = crate::get_sqlite_unchecked();
    sqlite3.code_to_str(err_code)
}
*/
