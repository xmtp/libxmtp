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
pub mod db_connection;
pub mod group;
pub mod group_intent;
pub mod group_message;
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

pub mod database;

pub use self::db_connection::DbConnection;
pub use diesel::sqlite::{Sqlite, SqliteConnection};
use xmtp_common::{RetryableError, retryable};

use super::{StorageError, xmtp_openmls_provider::XmtpOpenMlsProvider};
use crate::Store;

pub use database::*;

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
    in_transaction: Arc<AtomicBool>,
    _mutex_guard: parking_lot::MutexGuard<'a, ()>,
}

impl Drop for TransactionGuard<'_> {
    fn drop(&mut self) {
        self.in_transaction.store(false, Ordering::SeqCst);
    }
}

pub trait Database {
    type Db: XmtpDb;

    fn init_db(&mut self) -> Result<(), StorageError>;
    fn db(&self) -> &Self::Db;
    fn take(self) -> Self::Db
    where
        Self: Sized;
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

pub trait ConnectionExt {
    type Error;
    type Connection: diesel::Connection<Backend = Sqlite>
        + diesel::connection::SimpleConnection
        + LoadConnection
        + MigrationConnection
        + MigrationHarness<<Self::Connection as diesel::Connection>::Backend>
        + Send;

    fn start_transaction(&self) -> Result<TransactionGuard<'_>, Self::Error>;

    /// Run a scoped read-only query
    /// Implementors are expected to store an instance of 'TransactionGuard'
    /// in order to track transaction context
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, Self::Error>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized;

    /// Run a scoped write-only query
    /// Implementors are expected to store an instance of 'TransactionGuard'
    /// in order to track transaction context
    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, Self::Error>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized;
}

impl<C> ConnectionExt for &C
where
    C: ConnectionExt,
{
    type Connection = <C as ConnectionExt>::Connection;
    type Error = <C as ConnectionExt>::Error;

    fn start_transaction(&self) -> Result<TransactionGuard<'_>, Self::Error> {
        <C as ConnectionExt>::start_transaction(self)
    }

    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, Self::Error>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query_read(self, fun)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, Self::Error>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query_write(self, fun)
    }
}

impl<C> ConnectionExt for Arc<C>
where
    C: ConnectionExt,
{
    type Error = <C as ConnectionExt>::Error;
    type Connection = <C as ConnectionExt>::Connection;

    fn start_transaction(&self) -> Result<TransactionGuard<'_>, Self::Error> {
        <C as ConnectionExt>::start_transaction(self)
    }

    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, Self::Error>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query_read(self, fun)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, Self::Error>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query_write(self, fun)
    }
}
pub trait XmtpDb {
    type Error;
    /// The Connection type for this database
    type Connection: ConnectionExt + Send;

    /// Validate a connection is as expected
    fn validate(&self, _opts: &StorageOption) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Returns the Connection implementation for this Database
    fn conn(&self) -> Self::Connection;

    /// Reconnect to the database
    fn reconnect(&self) -> Result<(), Self::Error>;

    /// Release connection to the database, closing it
    fn disconnect(&self) -> Result<(), Self::Error>;
}

#[cfg(not(target_arch = "wasm32"))]
pub type EncryptedMessageStore = self::store::EncryptedMessageStore<native::NativeDb>;

#[cfg(not(target_arch = "wasm32"))]
impl EncryptedMessageStore {
    /// Created a new store
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn new(opts: StorageOption, enc_key: EncryptionKey) -> Result<Self, StorageError> {
        Self::new_database(opts, Some(enc_key)).await
    }

    /// Create a new, unencrypted database
    pub async fn new_unencrypted(opts: StorageOption) -> Result<Self, StorageError> {
        Self::new_database(opts, None).await
    }

    /// This function is private so that an unencrypted database cannot be created by accident
    #[tracing::instrument(level = "debug", skip_all)]
    async fn new_database(
        opts: StorageOption,
        enc_key: Option<EncryptionKey>,
    ) -> Result<Self, StorageError> {
        tracing::info!("Setting up DB connection pool");
        let db = native::NativeDb::new(&opts, enc_key)?;
        let mut store = Self { db, opts };
        store.init_db()?;
        Ok(store)
    }
}

#[cfg(target_arch = "wasm32")]
pub type EncryptedMessageStore = self::store::EncryptedMessageStore<wasm::WasmDb>;

#[cfg(target_arch = "wasm32")]
impl EncryptedMessageStore {
    pub async fn new(opts: StorageOption, enc_key: EncryptionKey) -> Result<Self, StorageError> {
        Self::new_database(opts, Some(enc_key)).await
    }

    pub async fn new_unencrypted(opts: StorageOption) -> Result<Self, StorageError> {
        Self::new_database(opts, None).await
    }

    /// This function is private so that an unencrypted database cannot be created by accident
    async fn new_database(
        opts: StorageOption,
        _enc_key: Option<EncryptionKey>,
    ) -> Result<Self, StorageError> {
        let db = wasm::WasmDb::new(&opts).await?;
        let mut this = Self { db, opts };
        this.init_db()?;
        Ok(this)
    }
}

/// Shared Code between WebAssembly and Native using the `XmtpDb` trait
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
        impl<C> $crate::Fetch<$model> for C
        where
            C: $crate::ConnectionExt,
            $crate::StorageError: From<<C as $crate::ConnectionExt>::Error>,
        {
            type Key = ();
            fn fetch(&self, _key: &Self::Key) -> Result<Option<$model>, $crate::StorageError> {
                use $crate::encrypted_store::schema::$table::dsl::*;
                self.raw_query_read::<_, _>(|conn| $table.first(conn).optional())
                    .map_err(Into::into)
            }
        }
    };

    ($model:ty, $table:ident, $key:ty) => {
        impl<C> $crate::Fetch<$model> for C
        where
            C: $crate::ConnectionExt,
            $crate::StorageError: From<<C as $crate::ConnectionExt>::Error>,
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
            StorageError: From<<C as $crate::ConnectionExt>::Error>,
        {
            fn fetch_list(&self) -> Result<Vec<$model>, $crate::StorageError> {
                use $crate::encrypted_store::schema::$table::dsl::*;
                Ok(self.raw_query_read(|conn| $table.load::<$model>(conn))?)
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
            $crate::StorageError: From<<C as $crate::ConnectionExt>::Error>,
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
            $crate::StorageError: From<<C as $crate::ConnectionExt>::Error>,
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

pub trait ProviderTransactions<C>
where
    C: ConnectionExt,
{
    fn transaction<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&XmtpOpenMlsProvider<C>) -> Result<T, E>,
        E: From<<C as ConnectionExt>::Error> + std::error::Error,
        E: From<crate::ConnectionError>;
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use diesel::sql_types::Blob;

    use super::*;
    use crate::{Fetch, Store, identity::StoredIdentity};
    use xmtp_common::{rand_vec, tmp_path};

    #[xmtp_common::test]
    async fn ephemeral_store() {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();
        let conn = &store.conn().unwrap();

        let inbox_id = "inbox_id";
        StoredIdentity::new(inbox_id.to_string(), rand_vec::<24>(), rand_vec::<24>())
            .store(conn)
            .unwrap();

        let fetched_identity: StoredIdentity = conn.fetch(&()).unwrap().unwrap();
        assert_eq!(fetched_identity.inbox_id, inbox_id);
    }

    #[xmtp_common::test]
    async fn persistent_store() {
        let db_path = tmp_path();
        {
            let store = EncryptedMessageStore::new(
                StorageOption::Persistent(db_path.clone()),
                EncryptedMessageStore::generate_enc_key(),
            )
            .await
            .unwrap();
            let conn = &store.conn().unwrap();

            let inbox_id = "inbox_id";
            StoredIdentity::new(inbox_id.to_string(), rand_vec::<24>(), rand_vec::<24>())
                .store(conn)
                .unwrap();

            let fetched_identity: StoredIdentity = conn.fetch(&()).unwrap().unwrap();
            assert_eq!(fetched_identity.inbox_id, inbox_id);
        }
        EncryptedMessageStore::remove_db_files(db_path)
    }

    #[xmtp_common::test]
    async fn test_migration_25() {
        let db_path = tmp_path();
        let opts = StorageOption::Persistent(db_path.clone());

        #[cfg(not(target_arch = "wasm32"))]
        let db =
            native::NativeDb::new(&opts, Some(EncryptedMessageStore::generate_enc_key())).unwrap();
        #[cfg(target_arch = "wasm32")]
        let db = wasm::WasmDb::new(&opts).await.unwrap();

        let store = EncryptedMessageStore { db, opts };
        store.db.validate(&store.opts).unwrap();

        store
            .db
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
            .db
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
            let store = EncryptedMessageStore::new(
                StorageOption::Persistent(db_path.clone()),
                EncryptedMessageStore::generate_enc_key(),
            )
            .await
            .unwrap();

            let conn1 = &store.conn().unwrap();
            let inbox_id = "inbox_id";
            StoredIdentity::new(inbox_id.to_string(), rand_vec::<24>(), rand_vec::<24>())
                .store(conn1)
                .unwrap();

            let conn2 = &store.conn().unwrap();
            tracing::info!("Getting conn 2");
            let fetched_identity: StoredIdentity = conn2.fetch(&()).unwrap().unwrap();
            assert_eq!(fetched_identity.inbox_id, inbox_id);
        }
        EncryptedMessageStore::remove_db_files(db_path)
    }
}
