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
#[cfg(feature = "sync")]
pub mod database;
#[cfg(feature = "sync")]
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
#[cfg(feature = "sync")]
pub mod pragmas;
pub mod processed_device_sync_messages;
pub mod readd_status;
pub mod refresh_state;
pub mod remote_commit_log;
#[cfg(feature = "sync")]
pub mod schema;
#[cfg(feature = "sync")]
mod schema_gen;
pub mod sql_int_enum;
#[cfg(feature = "sync")]
pub mod store;
pub mod tasks;
pub mod user_preferences;

#[cfg(test)]
mod migration_test;

#[cfg(feature = "sync")]
pub use self::db_connection::DbConnection;
#[cfg(feature = "sync")]
use diesel::{migration::Migration, result::DatabaseErrorKind};
#[cfg(feature = "sync")]
pub use diesel::{
    migration::MigrationSource,
    sqlite::{Sqlite, SqliteConnection},
};
use openmls::storage::OpenMlsProvider;
use prost::DecodeError;
use xmtp_common::{ErrorCode, RetryableError};
use xmtp_common::{MaybeSend, MaybeSync};
use xmtp_proto::ConversionError;
use zeroize::ZeroizeOnDrop;

#[cfg(feature = "sync")]
use super::StorageError;
use crate::SqlKeyStoreError;
use crate::XmtpMlsStorageProvider;
#[cfg(feature = "sync")]
use crate::Store;

#[cfg(feature = "sync")]
pub use database::*;
#[cfg(feature = "sync")]
pub use store::*;

#[cfg(feature = "sync")]
use diesel::{prelude::*, sql_query};
#[cfg(feature = "sync")]
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use std::ops::Deref;
#[cfg(feature = "sync")]
use std::sync::Arc;
#[cfg(feature = "sync")]
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
#[cfg(feature = "sync")]
#[derive(Debug)]
#[cfg_attr(feature = "sync", derive(QueryableByName))]
struct SqliteVersion {
    #[cfg_attr(feature = "sync", diesel(sql_type = diesel::sql_types::Text))]
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
    /// Database error.
    ///
    /// Diesel database query error. May be retryable.
    #[cfg(feature = "sync")]
    #[error(transparent)]
    Database(#[from] diesel::result::Error),
    #[cfg(feature = "sync")]
    #[error(transparent)]
    #[error_code(inherit)]
    Platform(#[from] PlatformStorageError),
    /// Decode error.
    ///
    /// Protobuf decode failed within DB layer. Not retryable.
    #[error(transparent)]
    DecodeError(#[from] DecodeError),
    /// Disconnect in transaction.
    ///
    /// Cannot disconnect while transaction is active. Retryable.
    #[error("disconnect not possible in transaction")]
    DisconnectInTransaction,
    /// Reconnect in transaction.
    ///
    /// Cannot reconnect while transaction is active. Retryable.
    #[error("reconnect not possible in transaction")]
    ReconnectInTransaction,
    /// Invalid query.
    ///
    /// Invalid query parameters or configuration. Not retryable.
    #[error("invalid query: {0}")]
    InvalidQuery(String),
    /// Postgres error from the async (sqlx) track.
    ///
    /// Retryability is classified by SQLSTATE -- see [`is_retryable_sqlx`].
    #[cfg(all(feature = "async", not(feature = "sync")))]
    #[error(transparent)]
    Sqlx(#[from] sqlx::Error),
    /// A transaction could not be committed because a handle to it outlived the
    /// transaction closure. The transaction is rolled back. Not retryable --
    /// this is a programming error, not contention.
    #[cfg(all(feature = "async", not(feature = "sync")))]
    #[error("transaction handle escaped its closure; transaction rolled back")]
    TransactionHandleEscaped,
    /// Invalid version.
    ///
    /// DB migration version mismatch -- running a newer DB on older LibXMTP. Not retryable.
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
            #[cfg(feature = "sync")]
            Self::Database(d) => d.is_retryable(),
            #[cfg(feature = "sync")]
            Self::Platform(n) => n.is_retryable(),
            Self::DecodeError(_) => false,
            Self::DisconnectInTransaction => true,
            Self::ReconnectInTransaction => true,
            Self::InvalidQuery(_) => false,
            Self::InvalidVersion { .. } => false,
            #[cfg(all(feature = "async", not(feature = "sync")))]
            Self::Sqlx(e) => is_retryable_sqlx(e),
            #[cfg(all(feature = "async", not(feature = "sync")))]
            Self::TransactionHandleEscaped => false,
        }
    }
}

/// Postgres counterpart to SQLite's `SQLITE_BUSY` retry classification.
///
/// On SQLite, contention shows up as a busy/locked code and the caller retries.
/// Postgres surfaces the same situations as SQLSTATE classes, and — because the
/// connection is now a network socket — adds a class SQLite has no equivalent
/// for: the round-trip itself failing. Both are retryable; a rejected statement
/// is not.
#[cfg(all(feature = "async", not(feature = "sync")))]
pub fn is_retryable_sqlx(e: &sqlx::Error) -> bool {
    use sqlx::error::DatabaseError;
    match e {
        // Contention. 40001 serialization_failure and 40P01 deadlock_detected are
        // what SQLITE_BUSY becomes under MVCC; 55P03 lock_not_available is the
        // `NOWAIT`/`SKIP LOCKED` analogue.
        sqlx::Error::Database(db) => matches!(
            DatabaseError::code(db.as_ref()).as_deref(),
            Some("40001" | "40P01" | "55P03")
        ),
        // The round trip failed rather than the statement. No SQLite analogue --
        // these only exist once the database is across a network.
        sqlx::Error::Io(_) | sqlx::Error::PoolTimedOut | sqlx::Error::PoolClosed => true,
        // Server closed the connection mid-flight.
        sqlx::Error::Protocol(_) | sqlx::Error::WorkerCrashed => true,
        // Statement was understood and rejected, or the result didn't match --
        // retrying re-runs an identical failure.
        _ => false,
    }
}

impl ConnectionError {
    /// True when the pool can't currently hand out a connection. Mirrors
    /// [`StorageError::db_needs_connection`].
    #[cfg(all(not(target_arch = "wasm32"), feature = "sync"))]
    pub fn db_needs_connection(&self) -> bool {
        use PlatformStorageError::{Pool, PoolNeedsConnection};
        matches!(self, Self::Platform(PoolNeedsConnection | Pool(_)))
    }

    #[cfg(any(target_arch = "wasm32", not(feature = "sync")))]
    pub fn db_needs_connection(&self) -> bool {
        false
    }
}

#[cfg(feature = "sync")]
pub trait ConnectionExt: MaybeSend + MaybeSync {
    /// Run a scoped query against the underlying SQLite connection.
    fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized;

    fn disconnect(&self) -> Result<(), ConnectionError>;
    fn reconnect(&self) -> Result<(), ConnectionError>;
}

#[cfg(feature = "sync")]
impl<C> ConnectionExt for &C
where
    C: ConnectionExt + xmtp_common::MaybeSync,
{
    fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query(self, fun)
    }

    fn disconnect(&self) -> Result<(), ConnectionError> {
        <C as ConnectionExt>::disconnect(self)
    }

    fn reconnect(&self) -> Result<(), ConnectionError> {
        <C as ConnectionExt>::reconnect(self)
    }
}

#[cfg(feature = "sync")]
impl<C> ConnectionExt for &mut C
where
    C: ConnectionExt,
{
    fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query(self, fun)
    }

    fn disconnect(&self) -> Result<(), ConnectionError> {
        <C as ConnectionExt>::disconnect(self)
    }

    fn reconnect(&self) -> Result<(), ConnectionError> {
        <C as ConnectionExt>::reconnect(self)
    }
}

#[cfg(feature = "sync")]
impl<C> ConnectionExt for Arc<C>
where
    C: ConnectionExt,
{
    fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        <C as ConnectionExt>::raw_query(self, fun)
    }

    fn disconnect(&self) -> Result<(), ConnectionError> {
        <C as ConnectionExt>::disconnect(self)
    }

    fn reconnect(&self) -> Result<(), ConnectionError> {
        <C as ConnectionExt>::reconnect(self)
    }
}

#[cfg(feature = "sync")]
pub type BoxedDatabase = Box<
    dyn XmtpDb<
            Connection = diesel::SqliteConnection,
            DbQuery = DbConnection<diesel::SqliteConnection>,
        >,
>;

// Track-agnostic core (opts/conn/db/reconnect/disconnect). The SQLite-specific
// bootstrap (`init`/`validate`, diesel migrations) is gated to the sync track;
// async stores run their own (e.g. sqlx) migrations outside this trait. The
// automock names sync-only mock types, so mock generation stays sync-only.
#[cfg_attr(all(feature = "sync", any(feature = "test-utils", test)), mockall::automock(type Connection = crate::mock::MockConnection; type DbQuery = crate::mock::MockDbQuery;))]
pub trait XmtpDb: MaybeSend + MaybeSync {
    /// The Connection type for this database
    #[cfg(feature = "sync")]
    type Connection: ConnectionExt + MaybeSend + MaybeSync;
    #[cfg(not(feature = "sync"))]
    type Connection: MaybeSend + MaybeSync;

    type DbQuery: crate::DbQuery + MaybeSend + MaybeSync;

    // SQLite bootstrap via diesel migrations. Sync-track only; async (sqlx) stores
    // run their own migrations before constructing the client.
    #[cfg(feature = "sync")]
    fn init(&self) -> Result<(), ConnectionError> {
        self.conn().raw_query(|conn| {
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
    #[cfg(feature = "sync")]
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
            async fn fetch(&self, _key: &Self::Key) -> Result<Option<$model>, $crate::StorageError> {
                use $crate::encrypted_store::schema::$table::dsl::*;
                self.raw_query(|conn| $table.first(conn).optional())
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
            async fn fetch(&self, key: &Self::Key) -> Result<Option<$model>, $crate::StorageError> {
                use $crate::encrypted_store::schema::$table::dsl::*;
                self.raw_query::<_, _>(|conn| $table.find(key.clone()).first(conn).optional())
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
                self.raw_query(|conn| $table.load::<$model>(conn))
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
            async fn store(&self, into: &C) -> Result<(), $crate::StorageError> {
                into.raw_query::<_, _>(|conn| {
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

            async fn store_or_ignore(&self, into: &C) -> Result<(), $crate::StorageError> {
                into.raw_query(|conn| {
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

#[cfg(feature = "sync")]
impl<T, C> Store<DbConnection<C>> for Vec<T>
where
    T: Store<DbConnection<C>> + MaybeSync,
    C: MaybeSync,
{
    type Output = ();
    async fn store(&self, into: &DbConnection<C>) -> Result<Self::Output, StorageError> {
        for item in self {
            item.store(into).await?;
        }
        Ok(())
    }
}

pub trait MlsProviderExt: OpenMlsProvider<StorageError = SqlKeyStoreError> {
    type XmtpStorage: XmtpMlsStorageProvider;

    fn key_store(&self) -> &Self::XmtpStorage;
}

#[cfg(feature = "sync")]
trait EmbeddedMigrationsExt {
    fn final_migration(&self) -> String;
}
#[cfg(feature = "sync")]
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

#[cfg(feature = "sync")]
trait MigrationHarnessExt {
    fn final_migration(&mut self) -> Result<String, diesel::result::Error>;
}

#[cfg(feature = "sync")]
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
    use super::*;
    use crate::{Store, XmtpTestDb, identity::StoredIdentity};
    use xmtp_common::{rand_vec, tmp_path};

    #[xmtp_common::test]
    async fn ephemeral_store() {
        let store = crate::TestDb::create_ephemeral_store().await;
        let conn = store.conn();

        let inbox_id = "inbox_id";
        StoredIdentity::new(inbox_id.to_string(), rand_vec::<24>(), rand_vec::<24>())
            .store(&conn)
            .await.unwrap();

        let fetched_identity: StoredIdentity = crate::Fetch::<StoredIdentity>::fetch(&conn, &()).await.unwrap().unwrap();
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
                .await.unwrap();

            let fetched_identity: StoredIdentity = crate::Fetch::<StoredIdentity>::fetch(conn, &()).await.unwrap().unwrap();
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
                .await.unwrap();

            let conn2 = &store.conn();
            tracing::info!("Getting conn 2");
            let fetched_identity: StoredIdentity = crate::Fetch::<StoredIdentity>::fetch(conn2, &()).await.unwrap().unwrap();
            assert_eq!(fetched_identity.inbox_id, inbox_id);
        }
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }

    /// A query failing because the pool can't hand out a connection must report
    /// `db_needs_connection()`. Uses a persistent store since only it has a pool to drop.
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test]
    async fn pool_failure_needs_connection() {
        let db_path = tmp_path();
        {
            let store = crate::TestDb::create_persistent_store(Some(db_path.clone())).await;
            let conn = store.conn();

            // Healthy pool: a query succeeds.
            let ok: Result<Option<StoredIdentity>, _> = crate::Fetch::<StoredIdentity>::fetch(&conn, &()).await;
            assert!(ok.is_ok());

            // Drop the pool, then run a real query against it.
            conn.disconnect().unwrap();
            let res: Result<Option<StoredIdentity>, _> = crate::Fetch::<StoredIdentity>::fetch(&conn, &()).await;
            let err = res.expect_err("query against a disconnected pool should fail");

            assert!(
                err.db_needs_connection(),
                "expected db_needs_connection() for a pool failure, got: {err:?}"
            );
        }
        EncryptedMessageStore::<()>::remove_db_files(db_path)
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod db_needs_connection_tests {
    use crate::{ConnectionError, PlatformStorageError, StorageError};

    /// Pool errors must classify as "needs reconnect" (direct and `Connection`-wrapped);
    /// non-pool errors must not. `Pool(_)` is covered e2e in `pool_failure_needs_connection`.
    #[test]
    fn pool_errors_need_connection() {
        assert!(
            StorageError::Platform(PlatformStorageError::PoolNeedsConnection).db_needs_connection()
        );
        assert!(
            StorageError::Connection(ConnectionError::Platform(
                PlatformStorageError::PoolNeedsConnection
            ))
            .db_needs_connection()
        );

        // A non-pool error must NOT be classified as needing a reconnect.
        assert!(!StorageError::DbSerialize.db_needs_connection());
    }
}
