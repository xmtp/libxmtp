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
pub mod identity_cache;
pub mod identity_update;
pub mod key_package_history;
pub mod key_store_entry;
pub mod processed_sync_messages;
pub mod refresh_state;
pub mod schema;
mod schema_gen;
pub mod store;
pub mod user_preferences;

pub mod database;

pub use self::db_connection::DbConnection;
pub use diesel::sqlite::{Sqlite, SqliteConnection};

use super::{StorageError, xmtp_openmls_provider::XmtpOpenMlsProviderPrivate};
use crate::Store;

pub use database::*;

use db_connection::DbConnectionPrivate;
use diesel::{connection::LoadConnection, migration::MigrationConnection, prelude::*, sql_query};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

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

pub trait Database {
    type Db: XmtpDb;

    fn init_db() -> Result<(), StorageError>;
    fn db(&self) -> &Self::Db;
    fn take(self) -> Self::Db
    where
        Self: Sized;
}

#[allow(async_fn_in_trait)]
pub trait XmtpDb {
    type Error;

    type Connection: diesel::Connection<Backend = Sqlite>
        + diesel::connection::SimpleConnection
        + LoadConnection
        + MigrationConnection
        + MigrationHarness<<Self::Connection as diesel::Connection>::Backend>
        + Send;
    type TransactionManager: diesel::connection::TransactionManager<Self::Connection>;

    /// Validate a connection is as expected
    fn validate(&self, _opts: &StorageOption) -> Result<(), Self::Error> {
        Ok(())
    }

    /// Returns the Connection implementation for this Database
    fn conn(&self) -> Result<DbConnectionPrivate<Self::Connection>, Self::Error>;

    /// Reconnect to the database
    fn reconnect(&self) -> Result<(), Self::Error>;

    /// Release connection to the database, closing it
    fn release_connection(&self) -> Result<(), Self::Error>;
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
        impl $crate::Fetch<$model> for $crate::encrypted_store::db_connection::DbConnection {
            type Key = ();
            fn fetch(&self, _key: &Self::Key) -> Result<Option<$model>, $crate::StorageError> {
                use $crate::encrypted_store::schema::$table::dsl::*;
                Ok(self.raw_query_read(|conn| $table.first(conn).optional())?)
            }
        }
    };

    ($model:ty, $table:ident, $key:ty) => {
        impl $crate::Fetch<$model> for $crate::encrypted_store::db_connection::DbConnection {
            type Key = $key;
            fn fetch(&self, key: &Self::Key) -> Result<Option<$model>, $crate::StorageError> {
                use $crate::encrypted_store::schema::$table::dsl::*;
                Ok(self.raw_query_read(|conn| $table.find(key.clone()).first(conn).optional())?)
            }
        }
    };
}

#[macro_export]
macro_rules! impl_fetch_list {
    ($model:ty, $table:ident) => {
        impl $crate::FetchList<$model> for $crate::encrypted_store::db_connection::DbConnection {
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
        impl $crate::Store<$crate::encrypted_store::db_connection::DbConnection> for $model {
            type Output = ();

            fn store(
                &self,
                into: &$crate::encrypted_store::db_connection::DbConnection,
            ) -> Result<Self::Output, $crate::StorageError> {
                into.raw_query_write(|conn| {
                    diesel::insert_into($table::table)
                        .values(self)
                        .execute(conn)
                        .map_err(Into::into)
                        .map(|_| ())
                })
            }
        }
    };
}

#[macro_export]
macro_rules! impl_store_or_ignore {
    // Original variant without return type parameter (defaults to returning ())
    ($model:ty, $table:ident) => {
        impl $crate::StoreOrIgnore<$crate::encrypted_store::db_connection::DbConnection>
            for $model
        {
            type Output = ();

            fn store_or_ignore(
                &self,
                into: &$crate::encrypted_store::db_connection::DbConnection,
            ) -> Result<Self::Output, $crate::StorageError> {
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
    type Output = ();
    fn store(&self, into: &DbConnection) -> Result<Self::Output, StorageError> {
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
        E: From<StorageError> + std::error::Error;
}

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use diesel::sql_types::{BigInt, Blob, Integer, Text};
    use group::ConversationType;
    use schema::groups;

    use super::*;
    use crate::{
        Fetch, Store,
        group::{GroupMembershipState, StoredGroup},
        identity::StoredIdentity,
    };
    use xmtp_common::{rand_vec, time::now_ns, tmp_path};

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

    #[xmtp_common::test]
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

    #[cfg(not(target_arch = "wasm32"))]
    #[tokio::test]
    async fn mismatched_encryption_key() {
        use crate::native::NativeStorageError;
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
            matches!(
                res.err(),
                Some(StorageError::Native(
                    NativeStorageError::SqlCipherKeyIncorrect
                ))
            ),
            "Expected SqlCipherKeyIncorrect error"
        );
        EncryptedMessageStore::remove_db_files(db_path)
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
