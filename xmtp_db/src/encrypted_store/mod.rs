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
pub mod conversation_list;
pub mod database;
pub mod db_connection;
pub mod events;
pub mod group;
pub mod group_intent;
pub mod group_message;
pub mod icebox;
pub mod identity;
pub mod identity_cache;
pub mod identity_update;
pub mod key_package_history;
pub mod key_store_entry;
pub mod processed_device_sync_messages;
pub mod refresh_state;
pub mod schema;
mod schema_gen;
pub mod store;
pub mod user_preferences;

pub mod local_commit_log;
pub mod remote_commit_log;

pub use self::db_connection::DbConnection;
pub use diesel::sqlite::{Sqlite, SqliteConnection};
use openmls_traits::OpenMlsProvider;
use xmtp_common::{RetryableError, retryable};

use super::{StorageError, xmtp_openmls_provider::XmtpOpenMlsProvider};
use crate::Store;
use crate::sql_key_store::SqlKeyStore;

pub use database::*;
pub use store::*;

use diesel::connection::SimpleConnection;
use diesel::{connection::LoadConnection, migration::MigrationConnection, prelude::*, sql_query};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations/");

pub type EncryptionKey = [u8; 32];

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

pub struct TransactionGuard<'a> {
    pub(crate) in_transaction: Arc<AtomicBool>,
    pub(crate) _mutex_guard: parking_lot::MutexGuard<'a, ()>,
}

impl Drop for TransactionGuard<'_> {
    fn drop(&mut self) {
        self.in_transaction.store(false, Ordering::SeqCst);
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ConnectionError {
    #[error(transparent)]
    Database(#[from] diesel::result::Error),
    #[error(transparent)]
    Platform(#[from] PlatformStorageError),
}

impl RetryableError for ConnectionError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Database(d) => retryable!(d),
            Self::Platform(n) => retryable!(n),
        }
    }
}

// #[cfg_attr(any(test, feature = "test-utils"), mockall::automock(type Connection = diesel::SqliteConnection;))]
pub trait ConnectionExt {
    type Connection: diesel::Connection<Backend = Sqlite>
        + diesel::connection::SimpleConnection
        + LoadConnection
        + MigrationConnection
        + MigrationHarness<<Self::Connection as diesel::Connection>::Backend>
        + Send;

    fn start_transaction(&self) -> Result<TransactionGuard<'_>, crate::ConnectionError>;

    /// Run a scoped read-only query
    /// Implementors are expected to store an instance of 'TransactionGuard'
    /// in order to track transaction context
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized;

    /// Run a scoped write-only query
    /// Implementors are expected to store an instance of 'TransactionGuard'
    /// in order to track transaction context
    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized;

    fn is_in_transaction(&self) -> bool;
}

impl<C> ConnectionExt for &C
where
    C: ConnectionExt,
{
    type Connection = <C as ConnectionExt>::Connection;

    fn start_transaction(&self) -> Result<TransactionGuard<'_>, crate::ConnectionError> {
        <C as ConnectionExt>::start_transaction(self)
    }

    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query_read(self, fun)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query_write(self, fun)
    }

    fn is_in_transaction(&self) -> bool {
        <C as ConnectionExt>::is_in_transaction(self)
    }
}

impl<C> ConnectionExt for Arc<C>
where
    C: ConnectionExt,
{
    type Connection = <C as ConnectionExt>::Connection;

    fn start_transaction(&self) -> Result<TransactionGuard<'_>, crate::ConnectionError> {
        <C as ConnectionExt>::start_transaction(self)
    }

    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query_read(self, fun)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query_write(self, fun)
    }

    fn is_in_transaction(&self) -> bool {
        <C as ConnectionExt>::is_in_transaction(self)
    }
}

pub type BoxedDatabase = Box<dyn XmtpDb<Connection = diesel::SqliteConnection>>;

#[cfg_attr(any(feature = "test-utils", test), mockall::automock(type Connection = crate::mock::MockConnection;))]
pub trait XmtpDb: Send + Sync {
    /// The Connection type for this database
    type Connection: ConnectionExt + Send + Sync;

    fn init(&self, opts: &StorageOption) -> Result<(), ConnectionError> {
        self.validate(opts)?;
        self.conn().raw_query_write(|conn| {
            conn.batch_execute("PRAGMA journal_mode = WAL;")?;
            conn.run_pending_migrations(MIGRATIONS)
                .map_err(diesel::result::Error::QueryBuilderError)?;

            let sqlite_version =
                sql_query("SELECT sqlite_version() AS version").load::<SqliteVersion>(conn)?;
            tracing::info!("sqlite_version={}", sqlite_version[0].version);

            tracing::info!("Migrations successful");
            Ok(())
        })?;
        Ok(())
    }

    /// The Options this databae was created with
    fn opts(&self) -> &StorageOption;

    /// Validate a connection is as expected
    fn validate(&self, _opts: &StorageOption) -> Result<(), ConnectionError> {
        Ok(())
    }

    /// Returns the Connection implementation for this Database
    fn conn(&self) -> Self::Connection;

    /// Returns a higher-level wrapeped DbConnection from which high-level queries may be
    /// accessed.
    fn db(&self) -> DbConnection<Self::Connection>;

    /// Reconnect to the database
    fn reconnect(&self) -> Result<(), ConnectionError>;

    /// Release connection to the database, closing it
    fn disconnect(&self) -> Result<(), ConnectionError>;
}

#[macro_export]
macro_rules! impl_fetch {
    ($model:ty, $table:ident) => {
        impl<C> $crate::Fetch<$model> for C
        where
            C: $crate::ConnectionExt,
        {
            type Key = ();
            fn fetch(&self, _key: &Self::Key) -> Result<Option<$model>, $crate::StorageError> {
                use $crate::encrypted_store::schema::$table::dsl::*;
                self.raw_query_read(|conn| $table.first(conn).optional())
                    .map_err(Into::into)
            }
        }
    };

    ($model:ty, $table:ident, $key:ty) => {
        impl<C> $crate::Fetch<$model> for C
        where
            C: $crate::ConnectionExt,
        {
            type Key = $key;
            fn fetch(&self, key: &Self::Key) -> Result<Option<$model>, $crate::StorageError> {
                use $crate::encrypted_store::schema::$table::dsl::*;
                self.raw_query_read::<_, _>(|conn| $table.find(key.clone()).first(conn).optional())
                    .map_err(Into::into)
            }
        }
    };
}

#[macro_export]
macro_rules! impl_fetch_list {
    ($model:ty, $table:ident) => {
        impl<C> $crate::FetchList<$model> for C
        where
            C: $crate::ConnectionExt,
        {
            fn fetch_list(&self) -> Result<Vec<$model>, $crate::StorageError> {
                use $crate::encrypted_store::schema::$table::dsl::*;
                self.raw_query_read(|conn| $table.load::<$model>(conn))
                    .map_err(Into::into)
            }
        }
    };
}

// Inserts the model into the database by primary key, erroring if the model already exists
#[macro_export]
macro_rules! impl_store {
    ($model:ty, $table:ident) => {
        impl<C> $crate::Store<C> for $model
        where
            C: $crate::ConnectionExt,
        {
            type Output = ();
            fn store(&self, into: &C) -> Result<(), $crate::StorageError> {
                into.raw_query_write::<_, _>(|conn| {
                    diesel::insert_into($table::table)
                        .values(self)
                        .execute(conn)
                        .map_err(Into::into)
                        .map(|_| ())
                })
                .map_err(Into::into)
            }
        }
    };
}

#[macro_export]
macro_rules! impl_store_or_ignore {
    // Original variant without return type parameter (defaults to returning ())
    ($model:ty, $table:ident) => {
        impl<C> $crate::StoreOrIgnore<C> for $model
        where
            C: $crate::ConnectionExt,
        {
            type Output = ();

            fn store_or_ignore(&self, into: &C) -> Result<(), $crate::StorageError> {
                into.raw_query_write(|conn| {
                    diesel::insert_or_ignore_into($table::table)
                        .values(self)
                        .execute(conn)
                        .map_err(Into::into)
                        .map(|_| ())
                })
                .map_err(Into::into)
            }
        }
    };
}

impl<T> Store<DbConnection> for Vec<T>
where
    T: Store<DbConnection>,
{
    type Output = ();
    fn store(&self, into: &DbConnection) -> Result<Self::Output, StorageError> {
        for item in self {
            item.store(into)?;
        }
        Ok(())
    }
}

pub trait MlsProviderExt: OpenMlsProvider {
    type Connection: ConnectionExt;

    /// Start a new database transaction with the OpenMLS Provider from XMTP
    /// with the provided connection
    /// # Arguments
    /// `fun`: Scoped closure providing a MLSProvider to carry out the transaction
    ///
    /// # Examples
    ///
    /// ```ignore
    /// provider.transaction(|provider| {
    ///     // do some operations requiring provider
    ///     // access the connection with .conn()
    ///     provider.conn().db_operation()?;
    /// })
    /// ```
    fn transaction<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&XmtpOpenMlsProvider<Self::Connection>) -> Result<T, E>,
        E: std::error::Error + From<crate::ConnectionError>;

    /// Get the underlying DbConnection this provider is using
    fn db(&self) -> &DbConnection<Self::Connection>;

    fn key_store(&self) -> &SqlKeyStore<Self::Connection>;
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use diesel::sql_types::Blob;

    use super::*;
    use crate::{Fetch, Store, XmtpTestDb, identity::StoredIdentity};
    use xmtp_common::{rand_vec, tmp_path};

    #[xmtp_common::test]
    async fn ephemeral_store() {
        let store = crate::TestDb::create_ephemeral_store().await;
        let conn = store.conn();

        let inbox_id = "inbox_id";
        StoredIdentity::new(inbox_id.to_string(), rand_vec::<24>(), rand_vec::<24>())
            .store(&conn)
            .unwrap();

        let fetched_identity: StoredIdentity = conn.fetch(&()).unwrap().unwrap();
        assert_eq!(fetched_identity.inbox_id, inbox_id);
    }

    #[xmtp_common::test]
    async fn persistent_store() {
        let db_path = tmp_path();
        {
            let store = crate::TestDb::create_persistent_store(Some(db_path.clone())).await;
            let conn = &store.conn();

            let inbox_id = "inbox_id";
            StoredIdentity::new(inbox_id.to_string(), rand_vec::<24>(), rand_vec::<24>())
                .store(conn)
                .unwrap();

            let fetched_identity: StoredIdentity = conn.fetch(&()).unwrap().unwrap();
            assert_eq!(fetched_identity.inbox_id, inbox_id);
        }
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }

    #[xmtp_common::test]
    async fn test_migration_25() {
        let db_path = tmp_path();
        let opts = StorageOption::Persistent(db_path.clone());

        #[cfg(not(target_arch = "wasm32"))]
        let db =
            native::NativeDb::new(&opts, EncryptedMessageStore::<()>::generate_enc_key()).unwrap();
        #[cfg(target_arch = "wasm32")]
        let db = wasm::WasmDb::new(&opts).await.unwrap();

        let store = EncryptedMessageStore { db };
        store.db.validate(&opts).unwrap();

        store
            .conn()
            .raw_query_write(|conn| {
                for _ in 0..25 {
                    conn.run_next_migration(MIGRATIONS).unwrap();
                }

                sql_query(
                    r#"
                INSERT INTO user_preferences (
                    hmac_key
                ) VALUES ($1)"#,
                )
                .bind::<Blob, _>(vec![1, 2, 3, 4, 5])
                .execute(conn)?;

                Ok(())
            })
            .unwrap();

        store
            .conn()
            .raw_query_write(|conn| {
                conn.run_pending_migrations(MIGRATIONS).unwrap();
                Ok(())
            })
            .unwrap();
    }

    #[xmtp_common::test]
    async fn encrypted_db_with_multiple_connections() {
        let db_path = tmp_path();
        {
            let store = crate::TestDb::create_persistent_store(Some(db_path.clone())).await;
            let conn1 = &store.conn();
            let inbox_id = "inbox_id";
            StoredIdentity::new(inbox_id.to_string(), rand_vec::<24>(), rand_vec::<24>())
                .store(conn1)
                .unwrap();

            let conn2 = &store.conn();
            tracing::info!("Getting conn 2");
            let fetched_identity: StoredIdentity = conn2.fetch(&()).unwrap().unwrap();
            assert_eq!(fetched_identity.inbox_id, inbox_id);
        }
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }
}
