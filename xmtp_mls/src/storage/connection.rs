//! An SqliteConnection wrapped in a Arc/Mutex to make it Sync

use std::sync::{Arc, Mutex};

use diesel::{
    associations::HasTable,
    connection::{
        AnsiTransactionManager, ConnectionSealed, DefaultLoadingMode, LoadConnection,
        SimpleConnection, TransactionManager,
    },
    expression::{is_aggregate, MixedAggregates, ValidGrouping},
    helper_types::{Find, Update},
    prelude::{Connection, Identifiable, SqliteConnection},
    query_builder::{AsChangeset, IntoUpdateTarget, QueryFragment, QueryId},
    query_dsl::{
        methods::{ExecuteDsl, FindDsl, LoadQuery},
        UpdateAndFetchResults,
    },
    r2d2::R2D2Connection,
    sqlite::Sqlite,
    ConnectionResult, QueryResult, Table,
};

struct SyncSqliteConnection {
    inner: Arc<Mutex<SqliteConnection>>,
}

/// This is safe because all operations happen through Arc<Mutex<T>>
unsafe impl Sync for SyncSqliteConnection {}

impl Connection for SyncSqliteConnection {
    type Backend = Sqlite;
    type TransactionManager = AnsiTransactionManager;

    fn establish(database_url: &str) -> ConnectionResult<Self> {
        todo!()
    }

    fn execute_returning_count<T>(&mut self, source: &T) -> QueryResult<usize>
    where
        T: QueryFragment<Self::Backend> + QueryId,
    {
        todo!()
    }

    fn transaction_state(
        &mut self,
    ) -> &mut <Self::TransactionManager as TransactionManager<Self>>::TransactionStateData {
        todo!()
    }
}

impl ConnectionSealed for SyncSqliteConnection {}

impl SimpleConnection for SyncSqliteConnection {
    fn batch_execute(&mut self, query: &str) -> QueryResult<()> {
        todo!()
    }
}

impl From<SqliteConnection> for SyncSqliteConnection {
    fn from(connection: SqliteConnection) -> Self {
        Self {
            inner: Arc::new(Mutex::new(connection)),
        }
    }
}

impl R2D2Connection for SyncSqliteConnection {
    fn ping(&mut self) -> QueryResult<()> {
        let mut conn = self.inner.lock().unwrap();
        (*conn).ping()
    }

    fn is_broken(&mut self) -> bool {
        let mut conn = self.inner.lock().unwrap();
        (*conn).is_broken()
    }
}

impl<'b, Changes, Output> UpdateAndFetchResults<Changes, Output> for SyncSqliteConnection
where
    Changes: Copy + Identifiable,
    Changes: AsChangeset<Target = <Changes as HasTable>::Table> + IntoUpdateTarget,
    Changes::Table: FindDsl<Changes::Id>,
    Update<Changes, Changes>: ExecuteDsl<SqliteConnection>,
    Find<Changes::Table, Changes::Id>: LoadQuery<'b, SqliteConnection, Output>,
    <Changes::Table as Table>::AllColumns: ValidGrouping<()>,
    <<Changes::Table as Table>::AllColumns as ValidGrouping<()>>::IsAggregate:
        MixedAggregates<is_aggregate::No, Output = is_aggregate::No>,
{
    fn update_and_fetch(&mut self, changeset: Changes) -> QueryResult<Output> {
        let mut conn = self.inner.lock().unwrap();
        (*conn).update_and_fetch(changeset)
    }
}

/*
impl LoadConnection<DefaultLoadingMode> for SyncSqliteConnection {
    type Cursor<'conn, 'query>
    where
        Self: 'conn;

    type Row<'conn, 'query>
    where
        Self: 'conn;
}
*/
