#![allow(clippy::unwrap_used)]

use std::path::Path;

use crate::{DbConnection, EncryptedMessageStore, StorageOption};
mod impls;
mod mls_memory_storage;

pub use mls_memory_storage::*;

pub type TestDb = EncryptedMessageStore<crate::DefaultDatabase>;

#[allow(async_fn_in_trait)]
pub trait XmtpTestDb {
    /// Create a validated, ephemeral database, running the migrations
    async fn create_ephemeral_store() -> EncryptedMessageStore<crate::DefaultDatabase>;

    async fn create_ephemeral_store_from_snapshot(
        snapshot: &[u8],
        path: Option<impl AsRef<Path>>,
    ) -> EncryptedMessageStore<crate::DefaultDatabase>;

    /// Create a validated, persistent database running the migrations
    async fn create_persistent_store(
        path: Option<String>,
    ) -> EncryptedMessageStore<crate::DefaultDatabase>;
    /// Create an empty database
    /// does no validation and does not run migrations.
    async fn create_database(path: Option<String>) -> crate::DefaultDatabase;
}

impl<Db> EncryptedMessageStore<Db> {
    pub fn generate_enc_key() -> [u8; 32] {
        let key = xmtp_common::rand_array::<32>();
        tracing::debug!("generated key is [{}]", hex::encode(key));
        key
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn remove_db_files<P: AsRef<str>>(path: P) {
        use crate::database::native::EncryptedConnection;

        let path = path.as_ref();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_file(EncryptedConnection::salt_file(path).unwrap()).unwrap();
    }

    /// just a no-op on wasm32
    #[cfg(target_arch = "wasm32")]
    pub fn remove_db_files<P: AsRef<str>>(_path: P) {}
}

impl Clone for crate::MockXmtpDb {
    fn clone(&self) -> Self {
        panic!("clone is not allowed")
    }
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub use wasm::*;
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
mod wasm {
    use super::*;
    use crate::{PersistentOrMem, WasmDbConnection};
    use std::sync::Arc;

    impl XmtpTestDb for super::TestDb {
        async fn create_ephemeral_store() -> EncryptedMessageStore<crate::DefaultDatabase> {
            let db = crate::database::WasmDb::new(&StorageOption::Ephemeral)
                .await
                .unwrap();
            EncryptedMessageStore::new(db).unwrap()
        }

        async fn create_ephemeral_store_from_snapshot(
            snapshot: &[u8],
        ) -> EncryptedMessageStore<crate::DefaultDatabase> {
            let db = crate::database::WasmDb::new(&StorageOption::Ephemeral)
                .await
                .unwrap();
            let store = EncryptedMessageStore::new_uninit(db).unwrap();
            store
                .db()
                .raw_query_write(|conn| conn.deserialize_database_from_buffer(snapshot))
                .unwrap();

            store
        }

        async fn create_persistent_store(
            path: Option<String>,
        ) -> EncryptedMessageStore<crate::DefaultDatabase> {
            let tmp = path.unwrap_or(xmtp_common::tmp_path());
            let db = crate::database::WasmDb::new(&StorageOption::Persistent(tmp))
                .await
                .unwrap();
            EncryptedMessageStore::new(db).unwrap()
        }

        async fn create_database(path: Option<String>) -> crate::DefaultDatabase {
            let tmp = path.unwrap_or(xmtp_common::tmp_path());
            crate::database::WasmDb::new(&StorageOption::Persistent(tmp))
                .await
                .unwrap()
        }
    }

    /// Test harness that loads an Ephemeral store.
    pub async fn with_connection<F, R>(fun: F) -> R
    where
        F: FnOnce(
            &crate::DbConnection<Arc<PersistentOrMem<WasmDbConnection, WasmDbConnection>>>,
        ) -> R,
    {
        let db = crate::database::WasmDb::new(&StorageOption::Ephemeral)
            .await
            .unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();
        let conn = store.conn();
        fun(&DbConnection::new(conn))
    }

    /// Test harness that loads an Ephemeral store.
    pub async fn with_connection_async<F, T, R>(fun: F) -> R
    where
        F: FnOnce(
            crate::DbConnection<Arc<PersistentOrMem<WasmDbConnection, WasmDbConnection>>>,
        ) -> T,
        T: Future<Output = R>,
    {
        let db = crate::database::WasmDb::new(&StorageOption::Ephemeral)
            .await
            .unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();
        let conn = store.conn();
        fun(DbConnection::new(conn)).await
    }

    impl EncryptedMessageStore<crate::database::WasmDb> {
        pub async fn new_test() -> Self {
            let db = crate::database::WasmDb::new(&StorageOption::Ephemeral)
                .await
                .unwrap();
            EncryptedMessageStore::new(db).expect("constructing message store failed.")
        }

        pub async fn new_test_with_path(path: &str) -> Self {
            let db = crate::database::WasmDb::new(&StorageOption::Persistent(path.into()))
                .await
                .unwrap();
            EncryptedMessageStore::new(db).expect("constructing message store failed.")
        }
    }
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub use native::*;
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
mod native {
    use super::*;
    use crate::{
        ConnectionExt, EphemeralDbConnection, MIGRATIONS, NativeDbConnection, PersistentOrMem,
    };
    use diesel::{Connection, SqliteConnection, connection::SimpleConnection};
    use diesel_migrations::MigrationHarness;
    use std::sync::Arc;

    impl XmtpTestDb for super::TestDb {
        async fn create_ephemeral_store() -> crate::DefaultStore {
            let opts = StorageOption::Ephemeral;
            let db = crate::database::NativeDb::new_unencrypted(&opts).unwrap();
            EncryptedMessageStore::new(db).unwrap()
        }
        async fn create_ephemeral_store_from_snapshot(
            mut snapshot: &[u8],
            path: Option<impl AsRef<Path>>,
        ) -> crate::DefaultStore {
            let path = path.as_ref();
            let mut buffer;

            let mut i = 0;
            let store = loop {
                let opts = StorageOption::Ephemeral;
                let db = crate::database::NativeDb::new_unencrypted(&opts).unwrap();
                let store = EncryptedMessageStore::new_uninit(db).unwrap();
                let result = store.db().raw_query_write(|conn| {
                    conn.deserialize_database_from_buffer(snapshot)?;
                    conn.batch_execute("PRAGMA journal_mode = DELETE")?;
                    Ok(())
                });

                if result.is_ok() {
                    break store;
                } else if i >= 1 {
                    result.unwrap();
                }

                if let Some(path) = path {
                    let path = path.as_ref();
                    // WAL is not compatible with ephemeral databases. Attempt to update and try one more time.
                    {
                        let mut conn =
                            SqliteConnection::establish(&path.to_string_lossy().to_string())
                                .unwrap();
                        conn.batch_execute("PRAGMA journal_mode = DELETE").unwrap();
                    }

                    buffer = tokio::fs::read(path).await.unwrap();
                    snapshot = &buffer;
                };

                i += 1;
            };

            store
                .conn()
                .raw_query_write(|c| {
                    c.run_pending_migrations(MIGRATIONS).unwrap();
                    Ok(())
                })
                .unwrap();

            store
        }
        async fn create_persistent_store(path: Option<String>) -> crate::DefaultStore {
            let path = path.unwrap_or(xmtp_common::tmp_path());
            let opts = StorageOption::Persistent(path.to_string());
            let db = crate::database::NativeDb::new(&opts, [0u8; 32]).unwrap();
            EncryptedMessageStore::new(db).expect("constructing message store failed.")
        }

        async fn create_database(path: Option<String>) -> crate::DefaultDatabase {
            let path = path.unwrap_or(xmtp_common::tmp_path());
            let opts = StorageOption::Persistent(path.to_string());
            crate::database::NativeDb::new(&opts, xmtp_common::rand_array::<32>()).unwrap()
        }
    }

    /// Test harness that loads an Ephemeral store.
    pub async fn with_connection<F, R>(fun: F) -> R
    where
        F: FnOnce(
            &crate::DbConnection<Arc<PersistentOrMem<NativeDbConnection, EphemeralDbConnection>>>,
        ) -> R,
    {
        let opts = StorageOption::Ephemeral;
        let db = crate::database::NativeDb::new_unencrypted(&opts).unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();
        let conn = store.conn();
        fun(&DbConnection::new(conn))
    }

    /// Test harness that loads an Ephemeral store.
    pub async fn with_connection_async<F, T, R>(fun: F) -> R
    where
        F: FnOnce(
            crate::DbConnection<Arc<PersistentOrMem<NativeDbConnection, EphemeralDbConnection>>>,
        ) -> T,
        T: Future<Output = R>,
    {
        let opts = StorageOption::Ephemeral;
        let db = crate::database::NativeDb::new_unencrypted(&opts).unwrap();
        let store = EncryptedMessageStore::new(db).unwrap();
        let conn = store.conn();
        fun(DbConnection::new(conn)).await
    }

    impl EncryptedMessageStore<crate::database::NativeDb> {
        pub async fn new_test() -> Self {
            let tmp_path = xmtp_common::tmp_path();
            let opts = StorageOption::Persistent(tmp_path);
            let db =
                crate::database::NativeDb::new(&opts, xmtp_common::rand_array::<32>()).unwrap();
            EncryptedMessageStore::new(db).expect("constructing message store failed.")
        }

        pub async fn new_test_with_path(path: &str) -> Self {
            let opts = StorageOption::Persistent(path.to_string());
            let db =
                crate::database::NativeDb::new(&opts, xmtp_common::rand_array::<32>()).unwrap();
            EncryptedMessageStore::new(db).expect("constructing message store failed.")
        }
    }
}
