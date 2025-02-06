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
mod conversation_list;
pub mod db_connection;
pub mod group;
pub mod group_intent;
pub mod group_message;
pub mod identity;
pub mod identity_update;
pub mod key_package_history;
pub mod key_store_entry;
#[cfg(not(target_arch = "wasm32"))]
pub(super) mod native;
pub mod refresh_state;
pub mod schema;
mod schema_gen;
#[cfg(not(target_arch = "wasm32"))]
mod sqlcipher_connection;
pub mod user_preferences;
pub mod wallet_addresses;
#[cfg(target_arch = "wasm32")]
pub(super) mod wasm;

pub use self::db_connection::DbConnection;
#[cfg(not(target_arch = "wasm32"))]
pub use diesel::sqlite::{Sqlite, SqliteConnection};
#[cfg(not(target_arch = "wasm32"))]
pub use native::RawDbConnection;
#[cfg(not(target_arch = "wasm32"))]
pub use sqlcipher_connection::EncryptedConnection;

#[cfg(target_arch = "wasm32")]
pub use self::wasm::SqliteConnection;
#[cfg(target_arch = "wasm32")]
pub use sqlite_web::{connection::WasmSqliteConnection as RawDbConnection, WasmSqlite as Sqlite};

use super::{xmtp_openmls_provider::XmtpOpenMlsProviderPrivate, StorageError};
use crate::Store;
use db_connection::DbConnectionPrivate;
use diesel::{
    connection::{LoadConnection, TransactionManager},
    migration::MigrationConnection,
    prelude::*,
    result::Error,
    sql_query,
};
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

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

#[allow(async_fn_in_trait)]
pub trait XmtpDb {
    type Connection: diesel::Connection<Backend = Sqlite>
        + diesel::connection::SimpleConnection
        + LoadConnection
        + MigrationConnection
        + MigrationHarness<<Self::Connection as diesel::Connection>::Backend>
        + Send;
    type TransactionManager: diesel::connection::TransactionManager<Self::Connection>;

    /// Validate a connection is as expected
    fn validate(&self, _opts: &StorageOption) -> Result<(), StorageError> {
        Ok(())
    }

    /// Returns the Connection implementation for this Database
    fn conn(&self) -> Result<DbConnectionPrivate<Self::Connection>, StorageError>;

    /// Reconnect to the database
    fn reconnect(&self) -> Result<(), StorageError>;

    /// Release connection to the database, closing it
    fn release_connection(&self) -> Result<(), StorageError>;
}

#[cfg(not(target_arch = "wasm32"))]
pub type EncryptedMessageStore = self::private::EncryptedMessageStore<native::NativeDb>;

#[cfg(not(target_arch = "wasm32"))]
impl EncryptedMessageStore {
    /// Created a new store
    #[tracing::instrument(level = "trace", skip_all)]
    pub async fn new(opts: StorageOption, enc_key: EncryptionKey) -> Result<Self, StorageError> {
        Self::new_database(opts, Some(enc_key))
    }

    /// Create a new, unencrypted database
    pub async fn new_unencrypted(opts: StorageOption) -> Result<Self, StorageError> {
        Self::new_database(opts, None)
    }

    /// This function is private so that an unencrypted database cannot be created by accident
    #[tracing::instrument(level = "trace", skip_all)]
    fn new_database(
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
        _enc_key: Option<EncryptionKey>,
    ) -> Result<Self, StorageError> {
        let db = wasm::WasmDb::new(&opts).await?;
        let mut this = Self { db, opts };
        this.init_db()?;
        Ok(this)
    }
}

/// Shared Code between WebAssembly and Native using the `XmtpDb` trait
pub mod private {
    use crate::storage::xmtp_openmls_provider::XmtpOpenMlsProviderPrivate;

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
        #[tracing::instrument(level = "trace", skip_all)]
        pub(super) fn init_db(&mut self) -> Result<(), StorageError> {
            self.db.validate(&self.opts)?;
            self.db.conn()?.raw_query_write(|conn| {
                conn.batch_execute("PRAGMA journal_mode = WAL;")?;
                tracing::info!("Running DB migrations");
                conn.run_pending_migrations(MIGRATIONS)?;

                let sqlite_version =
                    sql_query("SELECT sqlite_version() AS version").load::<SqliteVersion>(conn)?;
                tracing::info!("sqlite_version={}", sqlite_version[0].version);

                tracing::info!("Migrations successful");
                Ok::<_, StorageError>(())
            })?;

            Ok::<_, StorageError>(())
        }

        pub fn mls_provider(
            &self,
        ) -> Result<XmtpOpenMlsProviderPrivate<Db, Db::Connection>, StorageError> {
            let conn = self.conn()?;
            Ok(XmtpOpenMlsProviderPrivate::new(conn))
        }

        /// Pulls a new connection from the store
        pub fn conn(
            &self,
        ) -> Result<DbConnectionPrivate<<Db as XmtpDb>::Connection>, StorageError> {
            self.db.conn()
        }

        /// Release connection to the database, closing it
        pub fn release_connection(&self) -> Result<(), StorageError> {
            self.db.release_connection()
        }

        /// Reconnect to the database
        pub fn reconnect(&self) -> Result<(), StorageError> {
            self.db.reconnect()
        }
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
                Ok(self.raw_query_read(|conn| $table.first(conn).optional())?)
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
                Ok(self.raw_query_read(|conn| $table.find(key.clone()).first(conn).optional())?)
            }
        }
    };
}

#[macro_export]
macro_rules! impl_fetch_list {
    ($model:ty, $table:ident) => {
        impl $crate::FetchList<$model>
            for $crate::storage::encrypted_store::db_connection::DbConnection
        {
            fn fetch_list(&self) -> Result<Vec<$model>, $crate::StorageError> {
                use $crate::storage::encrypted_store::schema::$table::dsl::*;
                Ok(self.raw_query_read(|conn| $table.load::<$model>(conn))?)
            }
        }
    };
}

#[macro_export]
macro_rules! impl_fetch_list_with_key {
    ($model:ty, $table:ident, $key:ty, $column:ident) => {
        impl $crate::FetchListWithKey<$model>
            for $crate::storage::encrypted_store::db_connection::DbConnection
        {
            type Key = $key;
            fn fetch_list_with_key(
                &self,
                keys: &[Self::Key],
            ) -> Result<Vec<$model>, $crate::StorageError> {
                use $crate::storage::encrypted_store::schema::$table::dsl::{$column, *};
                Ok(self.raw_query_read(|conn| {
                    $table.filter($column.eq_any(keys)).load::<$model>(conn)
                })?)
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
                into.raw_query_write(|conn| {
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
                into.raw_query_write(|conn| {
                    diesel::insert_or_ignore_into($table::table)
                        .values(self)
                        .execute(conn)
                        .map_err(Into::into)
                        .map(|_| ())
                })
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

pub trait ProviderTransactions<Db>
where
    Db: XmtpDb,
{
    fn transaction<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&XmtpOpenMlsProviderPrivate<Db, <Db as XmtpDb>::Connection>) -> Result<T, E>,
        E: From<diesel::result::Error> + From<StorageError> + std::error::Error;
}

impl<Db> ProviderTransactions<Db> for XmtpOpenMlsProviderPrivate<Db, <Db as XmtpDb>::Connection>
where
    Db: XmtpDb,
{
    /// Start a new database transaction with the OpenMLS Provider from XMTP
    /// with the provided connection
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
    fn transaction<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&XmtpOpenMlsProviderPrivate<Db, <Db as XmtpDb>::Connection>) -> Result<T, E>,
        E: From<diesel::result::Error> + From<StorageError> + std::error::Error,
    {
        tracing::debug!("Transaction beginning");

        let conn = self.conn_ref();
        let _guard = conn.start_transaction::<Db>()?;

        match fun(self) {
            Ok(value) => {
                conn.raw_query_write(|conn| {
                    <Db as XmtpDb>::TransactionManager::commit_transaction(&mut *conn)
                })?;
                tracing::debug!("Transaction being committed");
                Ok(value)
            }
            Err(err) => {
                tracing::debug!("Transaction being rolled back");
                match conn.raw_query_write(|conn| {
                    <Db as XmtpDb>::TransactionManager::rollback_transaction(&mut *conn)
                }) {
                    Ok(()) => Err(err),
                    Err(Error::BrokenTransactionManager) => Err(err),
                    Err(rollback) => Err(rollback.into()),
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use diesel::sql_types::{BigInt, Blob, Integer, Text};
    use group::ConversationType;
    use schema::groups;
    use wasm_bindgen_test::wasm_bindgen_test;

    use super::*;
    use crate::{
        storage::{
            group::{GroupMembershipState, StoredGroup},
            identity::StoredIdentity,
        },
        Fetch, Store,
    };
    use xmtp_common::{rand_vec, time::now_ns, tmp_path};

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

    #[wasm_bindgen_test(unsupported = tokio::test)]
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

    #[wasm_bindgen_test(unsupported = tokio::test)]
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
    #[cfg(not(target_arch = "wasm32"))]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
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
            StoredIdentity::new(inbox_id.to_string(), rand_vec::<24>(), rand_vec::<24>())
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

    #[wasm_bindgen_test::wasm_bindgen_test(unsupported = tokio::test)]
    async fn test_dm_id_migration() {
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
            .unwrap()
            .raw_query_write(|conn| {
                for _ in 0..15 {
                    conn.run_next_migration(MIGRATIONS)?;
                }

                sql_query(
                    r#"
                INSERT INTO groups (
                    id,
                    created_at_ns,
                    membership_state,
                    installations_last_checked,
                    added_by_inbox_id,
                    rotated_at_ns,
                    conversation_type,
                    dm_inbox_id
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
                )
                .bind::<Blob, _>(vec![1, 2, 3, 4, 5])
                .bind::<BigInt, _>(now_ns())
                .bind::<Integer, _>(GroupMembershipState::Allowed as i32)
                .bind::<BigInt, _>(now_ns())
                .bind::<Text, _>("121212")
                .bind::<BigInt, _>(now_ns())
                .bind::<Integer, _>(ConversationType::Dm as i32)
                .bind::<Text, _>("98765")
                .execute(conn)?;

                Ok::<_, StorageError>(())
            })
            .unwrap();

        let conn = store.db.conn().unwrap();

        let inbox_id = "inbox_id";
        StoredIdentity::new(inbox_id.to_string(), rand_vec::<24>(), rand_vec::<24>())
            .store(&conn)
            .unwrap();

        let fetched_identity: StoredIdentity = conn.fetch(&()).unwrap().unwrap();
        assert_eq!(fetched_identity.inbox_id, inbox_id);

        store
            .db
            .conn()
            .unwrap()
            .raw_query_write(|conn| {
                conn.run_pending_migrations(MIGRATIONS)?;
                Ok::<_, StorageError>(())
            })
            .unwrap();

        let groups = conn
            .raw_query_read(|conn| groups::table.load::<StoredGroup>(conn))
            .unwrap();
        assert_eq!(groups.len(), 1);
        assert_eq!(&**groups[0].dm_id.as_ref().unwrap(), "dm:98765:inbox_id");
    }

    #[tokio::test]
    async fn mismatched_encryption_key() {
        let mut enc_key = [1u8; 32];

        let db_path = tmp_path();
        {
            // Setup a persistent store
            let store =
                EncryptedMessageStore::new(StorageOption::Persistent(db_path.clone()), enc_key)
                    .await
                    .unwrap();

            StoredIdentity::new(
                "dummy_address".to_string(),
                rand_vec::<24>(),
                rand_vec::<24>(),
            )
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

    #[wasm_bindgen_test(unsupported = tokio::test)]
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
