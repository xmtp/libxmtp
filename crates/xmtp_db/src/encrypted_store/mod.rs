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
pub mod d14n_migration_cutover;
pub mod database;
pub mod db_connection;
pub mod group;
pub mod group_intent;
pub mod group_message;
pub mod icebox;
pub mod identity;
pub mod identity_cache;
pub mod identity_update;
pub mod key_package_history;
pub mod key_store_entry;
pub mod local_commit_log;
pub mod message_deletion;
pub mod migrations;
pub mod pending_remove;
pub mod pragmas;
pub mod processed_device_sync_messages;
pub mod readd_status;
pub mod refresh_state;
pub mod remote_commit_log;
pub mod schema;
mod schema_gen;
pub mod store;
pub mod tasks;
pub mod user_preferences;

#[cfg(test)]
mod migration_test;

pub use self::db_connection::DbConnection;
use diesel::{migration::Migration, result::DatabaseErrorKind};
pub use diesel::{
    migration::MigrationSource,
    sqlite::{Sqlite, SqliteConnection},
};
use openmls::storage::OpenMlsProvider;
use prost::DecodeError;
use xmtp_common::{ErrorCode, MaybeSend, MaybeSync, RetryableError};
use xmtp_proto::ConversionError;
use zeroize::ZeroizeOnDrop;

use super::StorageError;
use crate::sql_key_store::SqlKeyStoreError;
use crate::{Store, XmtpMlsStorageProvider};

pub use database::*;
pub use store::*;

use diesel::{prelude::*, sql_query};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use std::{ops::Deref, sync::Arc};
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations/");

#[derive(ZeroizeOnDrop, Clone)]
pub struct EncryptionKey([u8; 32]);
impl std::fmt::Debug for EncryptionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("EncryptionKey").field(&"xxxx").finish()
    }
}

impl Deref for EncryptionKey {
    type Target = [u8; 32];
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> AsRef<T> for EncryptionKey
where
    T: ?Sized,
    <EncryptionKey as Deref>::Target: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.deref().as_ref()
    }
}

impl TryFrom<Vec<u8>> for EncryptionKey {
    type Error = ConversionError;
    fn try_from(v: Vec<u8>) -> Result<EncryptionKey, Self::Error> {
        Ok(EncryptionKey(v.as_slice().try_into()?))
    }
}

impl From<[u8; 32]> for EncryptionKey {
    fn from(v: [u8; 32]) -> Self {
        EncryptionKey(v)
    }
}

impl TryFrom<&[u8]> for EncryptionKey {
    type Error = ConversionError;
    fn try_from(v: &[u8]) -> Result<EncryptionKey, Self::Error> {
        let bytes: [u8; 32] = v.try_into()?;
        Ok(EncryptionKey(bytes))
    }
}

// For PRAGMA query log statements
#[derive(QueryableByName, Debug)]
struct SqliteVersion {
    #[diesel(sql_type = diesel::sql_types::Text)]
    version: String,
}

#[derive(Default, Clone, Debug, zeroize::ZeroizeOnDrop)]
pub enum StorageOption {
    #[default]
    Ephemeral,
    Persistent(String),
}

impl std::fmt::Display for StorageOption {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StorageOption::Ephemeral => write!(f, "Ephemeral"),
            StorageOption::Persistent(path) => write!(f, "Persistent({})", path),
        }
    }
}

#[derive(thiserror::Error, Debug, ErrorCode)]
pub enum ConnectionError {
    #[error(transparent)]
    Database(#[from] diesel::result::Error),
    #[error(transparent)]
    #[error_code(inherit)]
    Platform(#[from] PlatformStorageError),
    #[error(transparent)]
    DecodeError(#[from] DecodeError),
    #[error("disconnect not possible in transaction")]
    DisconnectInTransaction,
    #[error("reconnect not possible in transaction")]
    ReconnectInTransaction,
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    #[error(
        "Applied migrations does not match available migrations.\n\
    This is likely due to running a database that is newer than this version of libxmtp.\n\
    Expected: {expected}, found: {found}"
    )]
    InvalidVersion { expected: String, found: String },
}

impl RetryableError for ConnectionError {
    fn is_retryable(&self) -> bool {
        match self {
            Self::Database(d) => d.is_retryable(),
            Self::Platform(n) => n.is_retryable(),
            Self::DecodeError(_) => false,
            Self::DisconnectInTransaction => true,
            Self::ReconnectInTransaction => true,
            Self::InvalidQuery(_) => false,
            Self::InvalidVersion { .. } => false,
        }
    }
}

pub trait ConnectionExt: MaybeSend + MaybeSync {
    /// in order to track transaction context
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized;

    /// Run a scoped write-only query
    /// in order to track transaction context
    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized;

    fn disconnect(&self) -> Result<(), ConnectionError>;
    fn reconnect(&self) -> Result<(), ConnectionError>;
}

impl<C> ConnectionExt for &C
where
    C: ConnectionExt,
{
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query_read(self, fun)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query_write(self, fun)
    }

    fn disconnect(&self) -> Result<(), ConnectionError> {
        <C as ConnectionExt>::disconnect(self)
    }

    fn reconnect(&self) -> Result<(), ConnectionError> {
        <C as ConnectionExt>::reconnect(self)
    }
}

impl<C> ConnectionExt for &mut C
where
    C: ConnectionExt,
{
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query_read(self, fun)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query_write(self, fun)
    }

    fn disconnect(&self) -> Result<(), ConnectionError> {
        <C as ConnectionExt>::disconnect(self)
    }

    fn reconnect(&self) -> Result<(), ConnectionError> {
        <C as ConnectionExt>::reconnect(self)
    }
}

impl<C> ConnectionExt for Arc<C>
where
    C: ConnectionExt,
{
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query_read(self, fun)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query_write(self, fun)
    }

    fn disconnect(&self) -> Result<(), ConnectionError> {
        <C as ConnectionExt>::disconnect(self)
    }

    fn reconnect(&self) -> Result<(), ConnectionError> {
        <C as ConnectionExt>::reconnect(self)
    }
}

pub type BoxedDatabase = Box<
    dyn XmtpDb<
            Connection = diesel::SqliteConnection,
            DbQuery = DbConnection<diesel::SqliteConnection>,
        >,
>;

#[cfg_attr(any(feature = "test-utils", test), mockall::automock(type Connection = crate::mock::MockConnection; type DbQuery = crate::mock::MockDbQuery;))]
pub trait XmtpDb: MaybeSend + MaybeSync {
    /// The Connection type for this database
    type Connection: ConnectionExt + MaybeSend + MaybeSync;

    type DbQuery: crate::DbQuery + MaybeSend + MaybeSync;

    fn init(&self) -> Result<(), ConnectionError> {
        self.conn().raw_query_write(|conn| {
            self.validate(conn).map_err(|e| {
                diesel::result::Error::DatabaseError(
                    DatabaseErrorKind::Unknown,
                    Box::new(e.to_string()),
                )
            })?;
            conn.run_pending_migrations(MIGRATIONS)
                .map_err(diesel::result::Error::QueryBuilderError)?;

            // Ensure the database version is what we expect
            let db_version = conn.final_migration()?;
            let last_migration = MIGRATIONS.final_migration();
            if db_version != last_migration {
                return Ok(Err(ConnectionError::InvalidVersion {
                    expected: last_migration,
                    found: db_version,
                }));
            }

            let sqlite_version =
                sql_query("SELECT sqlite_version() AS version").load::<SqliteVersion>(conn)?;
            tracing::info!("sqlite_version={}", sqlite_version[0].version);

            tracing::info!("Migrations successful");
            Ok(Ok(()))
        })??;

        Ok(())
    }

    /// The Options this database was created with
    fn opts(&self) -> &StorageOption;

    /// Validate a connection is as expected
    fn validate(&self, _conn: &mut SqliteConnection) -> Result<(), ConnectionError> {
        Ok(())
    }

    /// Returns the Connection implementation for this Database
    fn conn(&self) -> Self::Connection;

    /// Returns a higher-level wrapeped DbConnection from which high-level queries may be
    /// accessed.
    fn db(&self) -> Self::DbQuery;

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

impl<T, C> Store<DbConnection<C>> for Vec<T>
where
    T: Store<DbConnection<C>>,
{
    type Output = ();
    fn store(&self, into: &DbConnection<C>) -> Result<Self::Output, StorageError> {
        for item in self {
            item.store(into)?;
        }
        Ok(())
    }
}

pub trait MlsProviderExt: OpenMlsProvider<StorageError = SqlKeyStoreError> {
    type XmtpStorage: XmtpMlsStorageProvider;

    fn key_store(&self) -> &Self::XmtpStorage;
}

trait EmbeddedMigrationsExt {
    fn final_migration(&self) -> String;
}
impl EmbeddedMigrationsExt for EmbeddedMigrations {
    fn final_migration(&self) -> String {
        let migrations: Vec<Box<dyn Migration<Sqlite>>> = self
            .migrations()
            .expect("Migrations are directly embedded, so this cannot error");
        migrations
            .first()
            .expect("There is at least one migration")
            .name()
            .to_string()
            .chars()
            .filter(|c| c.is_numeric())
            .collect()
    }
}

trait MigrationHarnessExt {
    fn final_migration(&mut self) -> Result<String, diesel::result::Error>;
}

impl MigrationHarnessExt for SqliteConnection {
    fn final_migration(&mut self) -> Result<String, diesel::result::Error> {
        let migration: String = self
            .applied_migrations()
            .map_err(diesel::result::Error::QueryBuilderError)?
            .pop()
            .expect("This function should be run after migrations are applied")
            .to_string();

        Ok(migration)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

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
