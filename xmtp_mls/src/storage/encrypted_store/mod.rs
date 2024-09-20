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
pub mod key_store_entry;
#[cfg(not(target_arch = "wasm32"))]
mod native;
pub mod refresh_state;
pub mod schema;
#[cfg(not(target_arch = "wasm32"))]
mod sqlcipher_connection;
#[cfg(target_arch = "wasm32")]
mod wasm;

pub use self::db_connection::DbConnection;
#[cfg(not(target_arch = "wasm32"))]
pub use self::native::SqliteConnection;
#[cfg(target_arch = "wasm32")]
pub use self::wasm::SqliteConnection;
use super::StorageError;
use crate::{xmtp_openmls_provider::XmtpOpenMlsProviderPrivate, Store};
use db_connection::DbConnectionPrivate;
#[cfg(not(target_arch = "wasm32"))]
pub use diesel::sqlite::Sqlite;
use diesel::{
    connection::{LoadConnection, TransactionManager},
    migration::MigrationConnection,
    prelude::*,
    result::{DatabaseErrorKind, Error},
    sql_query,
};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
#[cfg(target_arch = "wasm32")]
pub use diesel_wasm_sqlite::WasmSqlite as Sqlite;
#[cfg(not(target_arch = "wasm32"))]
pub use sqlcipher_connection::EncryptedConnection;
use std::sync::Arc;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("./migrations/");

#[cfg(target_arch = "wasm32")]
pub use diesel_wasm_sqlite::connection::WasmSqliteConnection as RawDbConnection;
#[cfg(not(target_arch = "wasm32"))]
pub use native::RawDbConnection;

pub type EncryptionKey = [u8; 32];

// For PRAGMA query log statements
#[derive(QueryableByName, Debug)]
struct SqliteVersion {
    #[diesel(sql_type = diesel::sql_types::Text)]
    version: String,
}

pub fn ignore_unique_violation<T>(
    result: Result<T, diesel::result::Error>,
) -> Result<(), StorageError> {
    match result {
        Ok(_) => Ok(()),
        Err(Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => Ok(()),
        Err(error) => Err(StorageError::from(error)),
    }
}

#[derive(Default, Clone, Debug)]
pub enum StorageOption {
    #[default]
    Ephemeral,
    Persistent(String),
}

/// Global Marker trait for WebAssembly
#[cfg(target_arch = "wasm32")]
pub trait Wasm {}
#[cfg(target_arch = "wasm32")]
impl<T> Wasm for T {}

#[allow(async_fn_in_trait)]
pub trait XmtpDb {
    type Connection: diesel::Connection<Backend = Sqlite>
        + diesel::connection::SimpleConnection
        + LoadConnection
        + MigrationConnection
        + MigrationHarness<<Self::Connection as diesel::Connection>::Backend>
        + Send;
    type TransactionManager: diesel::connection::TransactionManager<Self::Connection>;

    fn validate(&self, _opts: &StorageOption) -> Result<(), StorageError> {
        Ok(())
    }

    /// Returns the Connection implementation for this Database
    fn conn(&self) -> Result<DbConnectionPrivate<Self::Connection>, StorageError>;

    fn reconnect(&self) -> Result<(), StorageError>;

    fn release_connection(&self) -> Result<(), StorageError>;
}

#[cfg(not(target_arch = "wasm32"))]
pub type EncryptedMessageStore = self::private::EncryptedMessageStore<native::NativeDb>;

#[cfg(not(target_arch = "wasm32"))]
impl EncryptedMessageStore {
    pub async fn new(opts: StorageOption, enc_key: EncryptionKey) -> Result<Self, StorageError> {
        Self::new_database(opts, Some(enc_key))
    }

    pub async fn new_unencrypted(opts: StorageOption) -> Result<Self, StorageError> {
        Self::new_database(opts, None)
    }

    /// This function is private so that an unencrypted database cannot be created by accident
    fn new_database(
        opts: StorageOption,
        enc_key: Option<EncryptionKey>,
    ) -> Result<Self, StorageError> {
        log::info!("Setting up DB connection pool");
        let db = native::NativeDb::new(&opts, enc_key)?;
        let mut this = Self { db, opts };
        this.init_db()?;
        Ok(this)
    }
}

#[cfg(target_arch = "wasm32")]
pub type EncryptedMessageStore = self::private::EncryptedMessageStore<wasm::WasmDb>;

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
        enc_key: Option<EncryptionKey>,
    ) -> Result<Self, StorageError> {
        log::info!("Setting up DB connection pool");
        let db = wasm::WasmDb::new(&opts, enc_key).await?;
        let mut this = Self { db, opts };
        this.init_db()?;
        Ok(this)
    }
}

#[doc(hidden)]
pub mod private {
    use super::*;
    use diesel::connection::SimpleConnection;
    use diesel_migrations::MigrationHarness;

    #[derive(Clone, Debug)]
    /// Manages a Sqlite db for persisting messages and other objects.
    pub struct EncryptedMessageStore<Db> {
        pub(super) opts: StorageOption,
        pub(super) db: Db,
    }

    impl<Db> EncryptedMessageStore<Db>
    where
        Db: XmtpDb,
    {
        pub(super) fn init_db(&mut self) -> Result<(), StorageError> {
            self.db.validate(&self.opts)?;
            self.db.conn()?.raw_query(|conn| {
                conn.batch_execute("PRAGMA journal_mode = WAL;")?;
                log::info!("Running DB migrations");
                conn.run_pending_migrations(MIGRATIONS)?;

                let sqlite_version =
                    sql_query("SELECT sqlite_version() AS version").load::<SqliteVersion>(conn)?;
                log::info!("sqlite_version={}", sqlite_version[0].version);

                log::info!("Migrations successful");
                Ok::<_, StorageError>(())
            })?;
            Ok::<_, StorageError>(())
        }

        pub fn conn(
            &self,
        ) -> Result<DbConnectionPrivate<<Db as XmtpDb>::Connection>, StorageError> {
            self.db.conn()
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
            F: FnOnce(&XmtpOpenMlsProviderPrivate<<Db as XmtpDb>::Connection>) -> Result<T, E>,
            E: From<diesel::result::Error> + From<StorageError>,
        {
            log::debug!("Transaction beginning");
            let connection = self.db.conn()?;
            {
                let mut connection = connection.inner_mut_ref();
                <Db as XmtpDb>::TransactionManager::begin_transaction(&mut *connection)?;
            }

            let provider = XmtpOpenMlsProviderPrivate::new(connection);
            let conn = provider.conn_ref();

            match fun(&provider) {
                Ok(value) => {
                    conn.raw_query(|conn| {
                        <Db as XmtpDb>::TransactionManager::commit_transaction(&mut *conn)
                    })?;
                    log::debug!("Transaction being committed");
                    Ok(value)
                }
                Err(err) => {
                    log::debug!("Transaction being rolled back");
                    match conn.raw_query(|conn| {
                        <Db as XmtpDb>::TransactionManager::rollback_transaction(&mut *conn)
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
            F: FnOnce(XmtpOpenMlsProviderPrivate<<Db as XmtpDb>::Connection>) -> Fut,
            Fut: futures::Future<Output = Result<T, E>>,
            E: From<diesel::result::Error> + From<StorageError>,
        {
            log::debug!("Transaction async beginning");
            let db_connection = self.db.conn()?;
            {
                let mut connection = db_connection.inner_mut_ref();
                <Db as XmtpDb>::TransactionManager::begin_transaction(&mut *connection)?;
            }
            let local_connection = db_connection.inner_ref();
            let provider = XmtpOpenMlsProviderPrivate::new(db_connection);

            // the other connection is dropped in the closure
            // ensuring we have only one strong reference
            let result = fun(provider).await;
            if Arc::strong_count(&local_connection) > 1 {
                log::warn!(
                    "More than 1 strong connection references still exist during transaction"
                );
            }

            if Arc::weak_count(&local_connection) > 1 {
                log::warn!("More than 1 weak connection references still exist during transaction");
            }

            // after the closure finishes, `local_provider` should have the only reference ('strong')
            // to `XmtpOpenMlsProvider` inner `DbConnection`..
            let local_connection = DbConnectionPrivate::from_arc_mutex(local_connection);
            match result {
                Ok(value) => {
                    local_connection.raw_query(|conn| {
                        <Db as XmtpDb>::TransactionManager::commit_transaction(&mut *conn)
                    })?;
                    log::debug!("Transaction async being committed");
                    Ok(value)
                }
                Err(err) => {
                    log::debug!("Transaction async being rolled back");
                    match local_connection.raw_query(|conn| {
                        <Db as XmtpDb>::TransactionManager::rollback_transaction(&mut *conn)
                    }) {
                        Ok(()) => Err(err),
                        Err(Error::BrokenTransactionManager) => Err(err),
                        Err(rollback) => Err(rollback.into()),
                    }
                }
            }
        }

        pub fn release_connection(&self) -> Result<(), StorageError> {
            self.db.release_connection()
        }

        pub fn reconnect(&self) -> Result<(), StorageError> {
            self.db.reconnect()
        }
    }
}

#[allow(dead_code)]
fn warn_length<T>(list: &[T], str_id: &str, max_length: usize) {
    if list.len() > max_length {
        log::warn!(
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
                let result = into.raw_query(|conn| {
                    diesel::insert_into($table::table)
                        .values(self)
                        .execute(conn)
                });
                $crate::storage::ignore_unique_violation(result)
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
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;
    use crate::{
        storage::{
            group::{GroupMembershipState, StoredGroup},
            identity::StoredIdentity,
        },
        utils::test::{rand_vec, tmp_path},
        Fetch, Store, StreamHandle as _,
    };
    use std::sync::Barrier;

    /// Test harness that loads an Ephemeral store.
    pub async fn with_connection<F, R>(fun: F) -> R
    where
        F: FnOnce(&DbConnection) -> R,
    {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();
        let conn = &store.conn().expect("acquiring a Connection failed");
        fun(conn)
    }

    impl EncryptedMessageStore {
        pub async fn new_test() -> Self {
            let tmp_path = tmp_path();
            EncryptedMessageStore::new(
                StorageOption::Persistent(tmp_path),
                EncryptedMessageStore::generate_enc_key(),
            )
            .await
            .expect("constructing message store failed.")
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn ephemeral_store() {
        let store = EncryptedMessageStore::new(
            StorageOption::Ephemeral,
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();
        let conn = &store.conn().unwrap();

        let inbox_id = "inbox_id";
        StoredIdentity::new(inbox_id.to_string(), rand_vec(), rand_vec())
            .store(conn)
            .unwrap();

        let fetched_identity: StoredIdentity = conn.fetch(&()).unwrap().unwrap();
        assert_eq!(fetched_identity.inbox_id, inbox_id);
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
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
            StoredIdentity::new(inbox_id.to_string(), rand_vec(), rand_vec())
                .store(conn)
                .unwrap();

            let fetched_identity: StoredIdentity = conn.fetch(&()).unwrap().unwrap();
            assert_eq!(fetched_identity.inbox_id, inbox_id);
        }
        EncryptedMessageStore::remove_db_files(db_path)
    }

    // #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    #[cfg(not(target_arch = "wasm32"))]
    async fn releases_db_lock() {
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
            StoredIdentity::new(inbox_id.to_string(), rand_vec(), rand_vec())
                .store(conn)
                .unwrap();

            let fetched_identity: StoredIdentity = conn.fetch(&()).unwrap().unwrap();

            assert_eq!(fetched_identity.inbox_id, inbox_id);

            store.release_connection().unwrap();
            assert!(store.db.pool.read().is_none());
            store.reconnect().unwrap();
            let fetched_identity2: StoredIdentity = conn.fetch(&()).unwrap().unwrap();

            assert_eq!(fetched_identity2.inbox_id, inbox_id);
        }

        EncryptedMessageStore::remove_db_files(db_path)
    }

    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    #[cfg(not(target_arch = "wasm32"))]
    async fn mismatched_encryption_key() {
        let mut enc_key = [1u8; 32];

        let db_path = tmp_path();
        {
            // Setup a persistent store
            let store =
                EncryptedMessageStore::new(StorageOption::Persistent(db_path.clone()), enc_key)
                    .await
                    .unwrap();

            StoredIdentity::new("dummy_address".to_string(), rand_vec(), rand_vec())
                .store(&store.conn().unwrap())
                .unwrap();
        } // Drop it

        enc_key[3] = 145; // Alter the enc_key
        let res =
            EncryptedMessageStore::new(StorageOption::Persistent(db_path.clone()), enc_key).await;

        // Ensure it fails
        assert!(
            matches!(res.err(), Some(StorageError::SqlCipherKeyIncorrect)),
            "Expected SqlCipherKeyIncorrect error"
        );
        EncryptedMessageStore::remove_db_files(db_path)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
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
            StoredIdentity::new(inbox_id.to_string(), rand_vec(), rand_vec())
                .store(conn1)
                .unwrap();

            let conn2 = &store.conn().unwrap();
            log::info!("Getting conn 2");
            let fetched_identity: StoredIdentity = conn2.fetch(&()).unwrap().unwrap();
            assert_eq!(fetched_identity.inbox_id, inbox_id);
        }
        EncryptedMessageStore::remove_db_files(db_path)
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn it_returns_ok_when_given_ok_result() {
        let result: Result<(), diesel::result::Error> = Ok(());
        assert!(
            super::ignore_unique_violation(result).is_ok(),
            "Expected Ok(()) when given Ok result"
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn it_returns_ok_on_unique_violation_error() {
        let result: Result<(), diesel::result::Error> = Err(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::UniqueViolation,
            Box::new("violation".to_string()),
        ));
        assert!(
            super::ignore_unique_violation(result).is_ok(),
            "Expected Ok(()) when given UniqueViolation error"
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn it_returns_err_on_non_unique_violation_database_errors() {
        let result: Result<(), diesel::result::Error> = Err(diesel::result::Error::DatabaseError(
            diesel::result::DatabaseErrorKind::NotNullViolation,
            Box::new("other kind".to_string()),
        ));
        assert!(
            super::ignore_unique_violation(result).is_err(),
            "Expected Err when given non-UniqueViolation database error"
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn it_returns_err_on_non_database_errors() {
        let result: Result<(), diesel::result::Error> = Err(diesel::result::Error::NotFound);
        assert!(
            super::ignore_unique_violation(result).is_err(),
            "Expected Err when given a non-database error"
        );
    }

    // get two connections
    // start a transaction
    // try to write with second connection
    // write should fail & rollback
    // first thread succeeds
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_transaction_rollback() {
        let db_path = tmp_path();
        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(db_path.clone()),
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
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

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_async_transaction() {
        let db_path = tmp_path();

        let store = EncryptedMessageStore::new(
            StorageOption::Persistent(db_path.clone()),
            EncryptedMessageStore::generate_enc_key(),
        )
        .await
        .unwrap();

        let store_pointer = store.clone();

        let handle = crate::spawn(None, async move {
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
                    );
                    group.store(conn1).unwrap();

                    anyhow::bail!("force a rollback")
                })
                .await?;
            Ok::<_, anyhow::Error>(())
        });

        let result = handle.join().await.unwrap();
        assert!(result.is_err());

        let conn = store.conn().unwrap();
        // this group should not exist because of the rollback
        let groups = conn.find_group(b"should not exist".to_vec()).unwrap();
        assert_eq!(groups, None);
    }
}
