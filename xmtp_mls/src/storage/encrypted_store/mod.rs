//! A durable object store powered by Sqlite and Diesel.
//!
//! Provides mechanism to store objects between sessions. The behavior of the store can be tailored
//! by choosing an appropriate `StoreOption`.
//!
//! ## Migrations
//!
//! Table definitions are located `<PackageRoot>/migrations/`. On initialization the store will see
//! if there are any outstanding database migrations and perform them as needed. When updating the
//! table definitions `schema.rs` must also be updated. To generate the correct schemas you can run
//! `diesel print-schema` or use `cargo run update-schema` which will update the files for you.

pub mod association_state;
pub mod consent_record;
pub mod db_connection;
pub mod group;
pub mod group_intent;
pub mod group_message;
pub mod identity;
pub mod identity_update;
pub mod key_package_history;
pub mod key_store_entry;
pub mod refresh_state;
pub mod schema;
mod sqlcipher_connection;

use std::sync::Arc;

use diesel::{
    connection::{AnsiTransactionManager, SimpleConnection, TransactionManager},
    prelude::*,
    r2d2::{ConnectionManager, Pool, PoolTransactionManager, PooledConnection},
    result::Error,
    sql_query,
};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use parking_lot::RwLock;

use self::db_connection::DbConnection;

pub use self::sqlcipher_connection::{EncryptedConnection, EncryptionKey};

use super::StorageError;
use crate::{xmtp_openmls_provider::XmtpOpenMlsProvider, Store};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations/");
pub type RawDbConnection = PooledConnection<ConnectionManager<SqliteConnection>>;

// For PRAGMA query log statements
#[derive(QueryableByName, Debug)]
struct SqliteVersion {
    #[diesel(sql_type = diesel::sql_types::Text)]
    version: String,
}

#[derive(Default, Clone, Debug)]
pub enum StorageOption {
    #[default]
    Ephemeral,
    Persistent(String),
}

impl StorageOption {
    // create a completely new standalone connection
    fn conn(&self) -> Result<SqliteConnection, diesel::ConnectionError> {
        use StorageOption::*;
        match self {
            Persistent(path) => SqliteConnection::establish(path),
            Ephemeral => SqliteConnection::establish(":memory:"),
        }
    }
}

/// An Unencrypted Connection
/// Creates a Sqlite3 Database/Connection in WAL mode.
/// Sets `busy_timeout` on each connection.
/// _*NOTE:*_Unencrypted Connections are not validated and mostly meant for testing.
/// It is not recommended to use an unencrypted connection in production.
#[derive(Clone, Debug)]
pub struct UnencryptedConnection;

impl diesel::r2d2::CustomizeConnection<SqliteConnection, diesel::r2d2::Error>
    for UnencryptedConnection
{
    fn on_acquire(&self, conn: &mut SqliteConnection) -> Result<(), diesel::r2d2::Error> {
        conn.batch_execute("PRAGMA busy_timeout = 5000;")
            .map_err(diesel::r2d2::Error::QueryError)?;
        Ok(())
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
/// Manages a Sqlite db for persisting messages and other objects.
pub struct EncryptedMessageStore {
    connect_opt: StorageOption,
    pool: Arc<RwLock<Option<Pool<ConnectionManager<SqliteConnection>>>>>,
    enc_opts: Option<EncryptedConnection>,
}

impl EncryptedMessageStore {
    pub fn new(opts: StorageOption, enc_key: EncryptionKey) -> Result<Self, StorageError> {
        Self::new_database(opts, Some(enc_key))
    }

    pub fn new_unencrypted(opts: StorageOption) -> Result<Self, StorageError> {
        Self::new_database(opts, None)
    }

    /// This function is private so that an unencrypted database cannot be created by accident
    fn new_database(
        opts: StorageOption,
        enc_key: Option<EncryptionKey>,
    ) -> Result<Self, StorageError> {
        tracing::info!("Setting up DB connection pool");
        let mut builder = Pool::builder();

        let enc_opts = if let Some(key) = enc_key {
            let enc_opts = EncryptedConnection::new(key, &opts)?;
            builder = builder.connection_customizer(Box::new(enc_opts.clone()));
            Some(enc_opts)
        } else if matches!(opts, StorageOption::Persistent(_)) {
            builder = builder.connection_customizer(Box::new(UnencryptedConnection));
            None
        } else {
            None
        };

        let pool = match opts {
            StorageOption::Ephemeral => builder
                .max_size(1)
                .build(ConnectionManager::<SqliteConnection>::new(":memory:"))?,
            StorageOption::Persistent(ref path) => builder
                .max_size(25)
                .build(ConnectionManager::<SqliteConnection>::new(path))?,
        };

        let mut this = Self {
            connect_opt: opts,
            pool: Arc::new(Some(pool).into()),
            enc_opts,
        };

        this.init_db()?;
        Ok(this)
    }

    fn init_db(&mut self) -> Result<(), StorageError> {
        if let Some(ref encrypted_conn) = self.enc_opts {
            encrypted_conn.validate(&self.connect_opt)?;
        }

        let conn = &mut self.raw_conn()?;
        conn.batch_execute("PRAGMA journal_mode = WAL;")?;
        tracing::info!("Running DB migrations");
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| StorageError::DbInit(format!("Failed to run migrations: {}", e)))?;

        let sqlite_version =
            sql_query("SELECT sqlite_version() AS version").load::<SqliteVersion>(conn)?;
        tracing::info!("sqlite_version={}", sqlite_version[0].version);

        tracing::info!("Migrations successful");
        Ok(())
    }

    pub(crate) fn raw_conn(
        &self,
    ) -> Result<PooledConnection<ConnectionManager<SqliteConnection>>, StorageError> {
        let pool_guard = self.pool.read();

        let pool = pool_guard
            .as_ref()
            .ok_or(StorageError::PoolNeedsConnection)?;

        tracing::debug!(
            "Pulling connection from pool, idle_connections={}, total_connections={}",
            pool.state().idle_connections,
            pool.state().connections
        );

        Ok(pool.get()?)
    }

    pub fn conn(&self) -> Result<DbConnection, StorageError> {
        let conn = self.raw_conn()?;
        Ok(DbConnection::new(conn))
    }

    /// Start a new database transaction with the OpenMLS Provider from XMTP
    /// # Arguments
    /// `fun`: Scoped closure providing a MLSProvider to carry out the transaction
    ///
    /// # Examples
    ///
    /// ```ignore
    /// store.transaction(|provider| {
    ///     // do some operations requiring provider
    ///     // access the connection with .conn()
    ///     provider.conn().db_operation()?;
    /// })
    /// ```
    pub fn transaction<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&XmtpOpenMlsProvider) -> Result<T, E>,
        E: From<diesel::result::Error> + From<StorageError>,
    {
        tracing::debug!("Transaction beginning");
        let mut connection = self.raw_conn()?;
        AnsiTransactionManager::begin_transaction(&mut *connection)?;

        let db_connection = DbConnection::new(connection);
        let provider = XmtpOpenMlsProvider::new(db_connection);
        let conn = provider.conn_ref();

        match fun(&provider) {
            Ok(value) => {
                conn.raw_query(|conn| {
                    PoolTransactionManager::<AnsiTransactionManager>::commit_transaction(&mut *conn)
                })?;
                tracing::debug!("Transaction being committed");
                Ok(value)
            }
            Err(err) => {
                tracing::debug!("Transaction being rolled back");
                match conn.raw_query(|conn| {
                    PoolTransactionManager::<AnsiTransactionManager>::rollback_transaction(
                        &mut *conn,
                    )
                }) {
                    Ok(()) => Err(err),
                    Err(Error::BrokenTransactionManager) => Err(err),
                    Err(rollback) => Err(rollback.into()),
                }
            }
        }
    }

    /// Start a new database transaction with the OpenMLS Provider from XMTP
    /// # Arguments
    /// `fun`: Scoped closure providing an [`XmtpOpenMLSProvider`] to carry out the transaction in
    /// async context.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// store.transaction_async(|provider| async move {
    ///     // do some operations requiring provider
    ///     // access the connection with .conn()
    ///     provider.conn().db_operation()?;
    /// }).await
    /// ```
    pub async fn transaction_async<T, F, E, Fut>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(XmtpOpenMlsProvider) -> Fut,
        Fut: futures::Future<Output = Result<T, E>>,
        E: From<diesel::result::Error> + From<StorageError>,
    {
        tracing::debug!("Transaction async beginning");
        let mut connection = self.raw_conn()?;
        AnsiTransactionManager::begin_transaction(&mut *connection)?;
        let connection = Arc::new(parking_lot::Mutex::new(connection));
        let local_connection = Arc::clone(&connection);
        let db_connection = DbConnection::from_arc_mutex(connection);
        let provider = XmtpOpenMlsProvider::new(db_connection);

        // the other connection is dropped in the closure
        // ensuring we have only one strong reference
        let result = fun(provider).await;
        if Arc::strong_count(&local_connection) > 1 {
            tracing::warn!(
                "More than 1 strong connection references still exist during transaction"
            );
        }

        if Arc::weak_count(&local_connection) > 1 {
            tracing::warn!("More than 1 weak connection references still exist during transaction");
        }

        // after the closure finishes, `local_provider` should have the only reference ('strong')
        // to `XmtpOpenMlsProvider` inner `DbConnection`..
        let local_connection = DbConnection::from_arc_mutex(local_connection);
        match result {
            Ok(value) => {
                local_connection.raw_query(|conn| {
                    PoolTransactionManager::<AnsiTransactionManager>::commit_transaction(&mut *conn)
                })?;
                tracing::debug!("Transaction async being committed");
                Ok(value)
            }
            Err(err) => {
                tracing::debug!("Transaction async being rolled back");
                match local_connection.raw_query(|conn| {
                    PoolTransactionManager::<AnsiTransactionManager>::rollback_transaction(
                        &mut *conn,
                    )
                }) {
                    Ok(()) => Err(err),
                    Err(Error::BrokenTransactionManager) => Err(err),
                    Err(rollback) => Err(rollback.into()),
                }
            }
        }
    }

    pub fn release_connection(&self) -> Result<(), StorageError> {
        let mut pool_guard = self.pool.write();
        pool_guard.take();
        Ok(())
    }

    pub fn reconnect(&self) -> Result<(), StorageError> {
        let mut builder = Pool::builder();

        if let Some(ref opts) = self.enc_opts {
            builder = builder.connection_customizer(Box::new(opts.clone()));
        }

        let pool = match self.connect_opt {
            StorageOption::Ephemeral => builder
                .max_size(1)
                .build(ConnectionManager::<SqliteConnection>::new(":memory:"))?,
            StorageOption::Persistent(ref path) => builder
                .max_size(25)
                .build(ConnectionManager::<SqliteConnection>::new(path))?,
        };

        let mut pool_write = self.pool.write();
        *pool_write = Some(pool);

        Ok(())
    }
}

#[allow(dead_code)]
fn warn_length<T>(list: &[T], str_id: &str, max_length: usize) {
    if list.len() > max_length {
        tracing::warn!(
            "EncryptedStore expected at most {} {} however found {}. Using the Oldest.",
            max_length,
            str_id,
            list.len()
        )
    }
}

#[macro_export]
macro_rules! impl_fetch {
    ($model:ty, $table:ident) => {
        impl $crate::Fetch<$model>
            for $crate::storage::encrypted_store::db_connection::DbConnection
        {
            type Key = ();
            fn fetch(&self, _key: &Self::Key) -> Result<Option<$model>, $crate::StorageError> {
                use $crate::storage::encrypted_store::schema::$table::dsl::*;
                Ok(self.raw_query(|conn| $table.first(conn).optional())?)
            }
        }
    };

    ($model:ty, $table:ident, $key:ty) => {
        impl $crate::Fetch<$model>
            for $crate::storage::encrypted_store::db_connection::DbConnection
        {
            type Key = $key;
            fn fetch(&self, key: &Self::Key) -> Result<Option<$model>, $crate::StorageError> {
                use $crate::storage::encrypted_store::schema::$table::dsl::*;
                Ok(self.raw_query(|conn| $table.find(key.clone()).first(conn).optional())?)
            }
        }
    };
}

// Inserts the model into the database by primary key, erroring if the model already exists
#[macro_export]
macro_rules! impl_store {
    ($model:ty, $table:ident) => {
        impl $crate::Store<$crate::storage::encrypted_store::db_connection::DbConnection>
            for $model
        {
            fn store(
                &self,
                into: &$crate::storage::encrypted_store::db_connection::DbConnection,
            ) -> Result<(), $crate::StorageError> {
                into.raw_query(|conn| {
                    diesel::insert_into($table::table)
                        .values(self)
                        .execute(conn)
                })?;
                Ok(())
            }
        }
    };
}

// Inserts the model into the database by primary key, silently skipping on unique constraints
#[macro_export]
macro_rules! impl_store_or_ignore {
    ($model:ty, $table:ident) => {
        impl $crate::StoreOrIgnore<$crate::storage::encrypted_store::db_connection::DbConnection>
            for $model
        {
            fn store_or_ignore(
                &self,
                into: &$crate::storage::encrypted_store::db_connection::DbConnection,
            ) -> Result<(), $crate::StorageError> {
                into.raw_query(|conn| {
                    diesel::insert_or_ignore_into($table::table)
                        .values(self)
                        .execute(conn)
                        .map(|_| ())
                })
                .map_err($crate::StorageError::from)
            }
        }
    };
}

impl<T> Store<DbConnection> for Vec<T>
where
    T: Store<DbConnection>,
{
    fn store(&self, into: &DbConnection) -> Result<(), StorageError> {
        for item in self {
            item.store(into)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;

    use crate::{
        storage::group::{GroupMembershipState, StoredGroup},
        storage::identity::StoredIdentity,
        utils::test::{rand_vec, tmp_path},
        Fetch, Store,
    };
    use std::sync::Arc;

    /// Test harness that loads an Ephemeral store.
    pub fn with_connection<F, R>(fun: F) -> R
    where
        F: FnOnce(&DbConnection) -> R,
    {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();
        let conn = &store.conn().expect("acquiring a Connection failed");
        fun(conn)
    }

    impl EncryptedMessageStore {
        pub fn new_test() -> Self {
            let tmp_path = tmp_path();
            EncryptedMessageStore::new(
                StorageOption::Persistent(tmp_path),
                EncryptedMessageStore::generate_enc_key(),
            )
            .expect("constructing message store failed.")
        }
    }

    #[test]
    fn ephemeral_store() {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();
        let conn = &store.conn().unwrap();

        let inbox_id = "inbox_id";
        StoredIdentity::new(inbox_id.to_string(), rand_vec(), rand_vec())
            .store(conn)
            .unwrap();

        let fetched_identity: StoredIdentity = conn.fetch(&()).unwrap().unwrap();
        assert_eq!(fetched_identity.inbox_id, inbox_id);
    }

    #[test]
    fn persistent_store() {
        let db_path = tmp_path();
        {
            let store = EncryptedMessageStore::new(
                StorageOption::Persistent(db_path.clone()),
                EncryptedMessageStore::generate_enc_key(),
            )
            .unwrap();
            let conn = &store.conn().unwrap();

            let inbox_id = "inbox_id";
            StoredIdentity::new(inbox_id.to_string(), rand_vec(), rand_vec())
                .store(conn)
                .unwrap();

            let fetched_identity: StoredIdentity = conn.fetch(&()).unwrap().unwrap();
            assert_eq!(fetched_identity.inbox_id, inbox_id);
        }
        EncryptedMessageStore::remove_db_files(db_path)
    }

    #[test]
    fn releases_db_lock() {
        let db_path = tmp_path();
        {
            let store = EncryptedMessageStore::new(
                StorageOption::Persistent(db_path.clone()),
                EncryptedMessageStore::generate_enc_key(),
            )
            .unwrap();
            let conn = &store.conn().unwrap();

            let inbox_id = "inbox_id";
            StoredIdentity::new(inbox_id.to_string(), rand_vec(), rand_vec())
                .store(conn)
                .unwrap();

            let fetched_identity: StoredIdentity = conn.fetch(&()).unwrap().unwrap();

            assert_eq!(fetched_identity.inbox_id, inbox_id);

            store.release_connection().unwrap();
            assert!(store.pool.read().is_none());
            store.reconnect().unwrap();
            let fetched_identity2: StoredIdentity = conn.fetch(&()).unwrap().unwrap();

            assert_eq!(fetched_identity2.inbox_id, inbox_id);
        }

        EncryptedMessageStore::remove_db_files(db_path)
    }

    #[test]
    fn mismatched_encryption_key() {
        let mut enc_key = [1u8; 32];

        let db_path = tmp_path();
        {
            // Setup a persistent store
            let store =
                EncryptedMessageStore::new(StorageOption::Persistent(db_path.clone()), enc_key)
                    .unwrap();

            StoredIdentity::new("dummy_address".to_string(), rand_vec(), rand_vec())
                .store(&store.conn().unwrap())
                .unwrap();
        } // Drop it

        enc_key[3] = 145; // Alter the enc_key
        let res = EncryptedMessageStore::new(StorageOption::Persistent(db_path.clone()), enc_key);

        // Ensure it fails
        assert!(
            matches!(res.err(), Some(StorageError::SqlCipherKeyIncorrect)),
            "Expected SqlCipherKeyIncorrect error"
        );
        EncryptedMessageStore::remove_db_files(db_path)
    }

    #[tokio::test]
    async fn encrypted_db_with_multiple_connections() {
        let db_path = tmp_path();
        {
            let store = EncryptedMessageStore::new(
                StorageOption::Persistent(db_path.clone()),
                EncryptedMessageStore::generate_enc_key(),
            )
            .unwrap();

            let conn1 = &store.conn().unwrap();
            let inbox_id = "inbox_id";
            StoredIdentity::new(inbox_id.to_string(), rand_vec(), rand_vec())
                .store(conn1)
                .unwrap();

            let conn2 = &store.conn().unwrap();
            tracing::info!("Getting conn 2");
            let fetched_identity: StoredIdentity = conn2.fetch(&()).unwrap().unwrap();
            assert_eq!(fetched_identity.inbox_id, inbox_id);
        }
        EncryptedMessageStore::remove_db_files(db_path)
    }

    // get two connections
    // start a transaction
    // try to write with second connection
    // write should fail & rollback
    // first thread succeeds
    #[test]
    fn test_transaction_rollback() {
        let db_path = tmp_path();
        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(db_path.clone()),
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();

        let barrier = Arc::new(Barrier::new(2));

        let store_pointer = store.clone();
        let barrier_pointer = barrier.clone();
        let handle = std::thread::spawn(move || {
            store_pointer.transaction(|provider| {
                let conn1 = provider.conn_ref();
                StoredIdentity::new("correct".to_string(), rand_vec(), rand_vec())
                    .store(conn1)
                    .unwrap();
                // wait for second transaction to start
                barrier_pointer.wait();
                // wait for second transaction to finish
                barrier_pointer.wait();
                Ok::<_, StorageError>(())
            })
        });

        let store_pointer = store.clone();
        let handle2 = std::thread::spawn(move || {
            barrier.wait();
            let result = store_pointer.transaction(|provider| -> Result<(), anyhow::Error> {
                let connection = provider.conn_ref();
                let group = StoredGroup::new(
                    b"should not exist".to_vec(),
                    0,
                    GroupMembershipState::Allowed,
                    "goodbye".to_string(),
                    None,
                );
                group.store(connection)?;
                Ok(())
            });
            barrier.wait();
            result
        });

        let result = handle.join().unwrap();
        assert!(result.is_ok());

        let result = handle2.join().unwrap();

        // handle 2 errored because the first transaction has precedence
        assert_eq!(
            result.unwrap_err().to_string(),
            "Diesel result error: database is locked"
        );
        let groups = store
            .conn()
            .unwrap()
            .find_group(b"should not exist".to_vec())
            .unwrap();
        assert_eq!(groups, None);
    }

    #[tokio::test]
    async fn test_async_transaction() {
        let db_path = tmp_path();

        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(db_path.clone()),
            EncryptedMessageStore::generate_enc_key(),
        )
        .unwrap();

        let store_pointer = store.clone();

        let handle = tokio::spawn(async move {
            store_pointer
                .transaction_async(|provider| async move {
                    let conn1 = provider.conn_ref();
                    StoredIdentity::new("crab".to_string(), rand_vec(), rand_vec())
                        .store(conn1)
                        .unwrap();

                    let group = StoredGroup::new(
                        b"should not exist".to_vec(),
                        0,
                        GroupMembershipState::Allowed,
                        "goodbye".to_string(),
                        None,
                    );
                    group.store(conn1).unwrap();

                    anyhow::bail!("force a rollback")
                })
                .await?;
            Ok::<_, anyhow::Error>(())
        });

        let result = handle.await.unwrap();
        assert!(result.is_err());

        let conn = store.conn().unwrap();
        // this group should not exist because of the rollback
        let groups = conn.find_group(b"should not exist".to_vec()).unwrap();
        assert_eq!(groups, None);
    }
}
